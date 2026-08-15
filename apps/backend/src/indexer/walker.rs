use anyhow::Result;

use crate::indexer::chunker::language_for_ext;

/// Maximum file size to index (1 MB).
const MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Directory names that are skipped wholesale (dependencies, build output, VCS).
/// This prunes huge trees (e.g. `node_modules`) even when the repo has no
/// `.gitignore`, so large repos stay indexable. Matched on any path component.
const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", ".hg", ".svn", "dist", "build", "out", "target",
    "vendor", "bin", "obj", ".next", ".nuxt", ".svelte-kit", ".angular",
    "coverage", "__pycache__", ".venv", "venv", ".tox", ".cache", ".gradle",
    ".idea", ".vscode", "Pods", "DerivedData", ".terraform",
];

/// Well-known lock / dependency-manifest files that pollute code search: they are
/// huge, machine-generated, and carry no semantic code signal, yet several of them
/// (`pnpm-lock.yaml`, `package-lock.json`) have extensions on the allowlist and so
/// would otherwise be chunked and rank at the top for real code queries. Matched by
/// exact file name.
const NOISE_FILES: &[&str] = &[
    "pnpm-lock.yaml", "package-lock.json", "yarn.lock", "Cargo.lock",
    "poetry.lock", "composer.lock", "Gemfile.lock", "go.sum", "bun.lockb",
];

/// True when `file_name` is a machine-generated noise file that must never be
/// indexed: a well-known lockfile, or a minified bundle (`*.min.js` / `*.min.css`).
fn is_noise_file(file_name: &str) -> bool {
    NOISE_FILES.contains(&file_name)
        || file_name.ends_with(".min.js")
        || file_name.ends_with(".min.css")
}

/// Real source-code file extensions admitted into the CODE corpus.
///
/// Documentation (`.md`), data, and config (`.json`, `.yaml`, `.toml`, …) files
/// are deliberately EXCLUDED: they dominate code-search results with non-code
/// prose (`README.md`, `AGENTS.md`) or machine-generated noise while carrying no
/// code signal. `language_for_ext` still recognizes several of those extensions
/// for other callers (e.g. `MarkdownChunker`), so this code-only gate lives in the
/// walker rather than in language detection — the walker simply won't feed a `.md`
/// or config file to the code index.
const CODE_EXTENSIONS: &[&str] = &[
    // Rust
    "rs",
    // TypeScript / JavaScript
    "ts", "tsx", "js", "jsx", "mjs", "cjs",
    // Python
    "py",
    // Go
    "go",
    // JVM
    "java", "kt", "kts",
    // C / C++
    "c", "h", "cc", "cpp", "cxx", "hpp",
    // C#
    "cs",
    // Ruby / PHP
    "rb", "php",
    // Swift
    "swift",
    // Shell
    "sh", "bash", "zsh",
    // Web source (markup/styles/components — real source, not config)
    "html", "htm", "css", "scss", "sass", "vue", "svelte",
    // SQL
    "sql",
];

/// True when `ext` is a real source-code extension admitted into the code corpus.
/// Excludes docs (`md`), data, and config (`json`, `yaml`, `toml`, `txt`, …).
fn is_code_extension(ext: &str) -> bool {
    CODE_EXTENSIONS.contains(&ext)
}

/// Lightweight metadata for an eligible source file. Deliberately holds NO file
/// content: large repos must not be loaded into memory all at once — content is
/// read on demand (see [`read_file`]) one file at a time during indexing.
#[derive(Debug, Clone)]
pub struct FileMeta {
    /// Absolute path to the file.
    pub path: String,
    /// File extension (without dot), if available.
    pub ext: Option<String>,
    /// Detected language, if recognized.
    pub language: Option<String>,
    /// On-disk size in bytes (captured during the walk's cheap `metadata` stat).
    /// Used by the indexer to bound Pass-1 batches by bytes so peak memory stays
    /// bounded regardless of individual file sizes.
    pub size: u64,
}

/// Reads a file's UTF-8 content and its SHA-256 hex hash on demand.
/// Returns `None` for binary / non-UTF-8 files. Keeps peak memory bounded to a
/// single file at a time during streaming indexing.
pub fn read_file(path: &str) -> Option<(String, String)> {
    let content = std::fs::read_to_string(path).ok()?;
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hex::encode(hasher.finalize());
    Some((content, hash))
}

/// Walk `root_path` (respecting `.gitignore` + standard VCS ignores, and pruning
/// known heavy directories), filtering by extension allowlist and the 1 MB size
/// cap. Returns lightweight metadata (paths only — NO content) so a huge repo's
/// discovery stays cheap; content is read later, one file at a time.
pub fn walk_files(root_path: &str) -> Result<Vec<FileMeta>> {
    let mut results = Vec::new();

    let walker = ignore::WalkBuilder::new(root_path)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(|entry| {
            // Prune heavy directories before descending into them.
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    if SKIP_DIRS.contains(&name) {
                        return false;
                    }
                }
            }
            true
        })
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!("Walker error (skipped): {e}");
                continue;
            }
        };

        // Only regular files
        match entry.file_type() {
            Some(ft) if ft.is_file() => {}
            _ => continue,
        }

        let path = entry.path();

        // Noise exclusion: skip machine-generated lockfiles and minified bundles
        // by exact file name before any other check — they pollute search results.
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if is_noise_file(name) {
                tracing::debug!("Skipping noise file {:?}", path);
                continue;
            }
        }

        // Size cap (cheap stat — no read). Capture the size so the indexer can
        // bound Pass-1 batches by bytes without re-stat'ing.
        let size = match std::fs::metadata(path) {
            Ok(m) if m.len() > MAX_FILE_SIZE => {
                tracing::debug!("Skipping oversized file {:?}", path);
                continue;
            }
            Ok(m) => m.len(),
            Err(e) => {
                tracing::debug!("Could not stat {:?}: {e}", path);
                continue;
            }
        };

        // Extension allowlist — CODE files only. Docs (`.md`) and config/data
        // (`.json`, `.yaml`, `.toml`, …) are excluded from the code corpus even
        // when `language_for_ext` recognizes them, so they never pollute code
        // search (READMEs/AGENTS.md/lockfiles ranking above real handlers).
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());
        if !ext.as_deref().map(is_code_extension).unwrap_or(false) {
            continue;
        }
        let language = ext
            .as_deref()
            .and_then(language_for_ext)
            .map(|s| s.to_string());
        if language.is_none() {
            continue;
        }

        results.push(FileMeta {
            path: path.to_string_lossy().into_owned(),
            ext,
            language,
            size,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_temp_project() -> TempDir {
        tempfile::tempdir().expect("tempdir must succeed")
    }

    #[test]
    fn walk_finds_rust_files() {
        let dir = make_temp_project();
        fs::write(dir.path().join("lib.rs"), "fn foo() {}").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 2, "should find 2 .rs files");
        assert!(files.iter().all(|f| f.language.as_deref() == Some("rust")));
    }

    #[test]
    fn walk_skips_unknown_extension() {
        let dir = make_temp_project();
        fs::write(dir.path().join("data.csv"), "a,b,c").unwrap();
        fs::write(dir.path().join("config.lock"), "locked").unwrap();
        fs::write(dir.path().join("notes.txt"), "notes").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 0, "unknown extensions must be skipped");
    }

    #[test]
    fn walk_skips_oversized_file() {
        let dir = make_temp_project();
        fs::write(dir.path().join("small.rs"), "fn foo() {}").unwrap();
        let big_content = "x".repeat(1024 * 1024 + 1);
        fs::write(dir.path().join("big.rs"), big_content).unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1, "oversized file must be skipped, only small.rs");
        assert!(files[0].path.ends_with("small.rs"));
    }

    #[test]
    fn walk_prunes_heavy_directories() {
        let dir = make_temp_project();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules").join("dep.js"), "module.exports = {}").unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("target").join("build.rs"), "fn b() {}").unwrap();
        fs::write(dir.path().join("app.js"), "function app() {}").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1, "node_modules and target must be pruned");
        assert!(files[0].path.ends_with("app.js"));
    }

    #[test]
    fn walk_excludes_noise_lockfiles_and_minified() {
        let dir = make_temp_project();
        // Lockfiles whose extensions are on the allowlist (would otherwise index).
        fs::write(dir.path().join("pnpm-lock.yaml"), "lockfileVersion: 9\n").unwrap();
        fs::write(dir.path().join("package-lock.json"), "{}").unwrap();
        // Minified bundles.
        fs::write(dir.path().join("app.min.js"), "var a=1;").unwrap();
        fs::write(dir.path().join("styles.min.css"), "a{color:red}").unwrap();
        // A real source file that must survive.
        fs::write(dir.path().join("app.js"), "function app() {}").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1, "only the real source file must remain: {files:?}");
        assert!(files[0].path.ends_with("app.js"));
    }

    #[test]
    fn walk_excludes_docs_and_config_from_code_corpus() {
        let dir = make_temp_project();
        // Docs + config that `language_for_ext` still recognizes, but which must
        // NOT enter the code corpus.
        fs::write(dir.path().join("README.md"), "# Title\n\nProse.\n").unwrap();
        fs::write(dir.path().join("AGENTS.md"), "# Agents\n").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("config.yaml"), "a: 1\n").unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        // A real source file that must survive.
        fs::write(dir.path().join("foo.ts"), "export function foo() {}").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1, "only the .ts source file must remain: {files:?}");
        assert!(files[0].path.ends_with("foo.ts"));
        assert_eq!(files[0].language.as_deref(), Some("typescript"));
    }

    #[test]
    fn walk_respects_gitignore() {
        let dir = make_temp_project();
        fs::write(dir.path().join(".gitignore"), "ignored.rs\n").unwrap();
        fs::write(dir.path().join("ignored.rs"), "fn x() {}").unwrap();
        fs::write(dir.path().join("src.rs"), "fn real() {}").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1, "gitignored file must be excluded");
        assert!(files[0].path.ends_with("src.rs"));
    }

    #[test]
    fn read_file_returns_content_and_stable_hash() {
        let dir = make_temp_project();
        let p = dir.path().join("foo.rs");
        fs::write(&p, "fn foo() {}").unwrap();
        let (content, hash) = read_file(p.to_str().unwrap()).expect("readable");
        assert_eq!(content, "fn foo() {}");
        assert_eq!(hash.len(), 64, "SHA-256 hex is 64 chars");
        let (_c2, hash2) = read_file(p.to_str().unwrap()).unwrap();
        assert_eq!(hash, hash2, "hash is deterministic");
    }

    #[test]
    fn read_file_skips_binary() {
        let dir = make_temp_project();
        let p = dir.path().join("bin.rs");
        fs::write(&p, [0u8, 159, 146, 150]).unwrap(); // invalid UTF-8
        assert!(read_file(p.to_str().unwrap()).is_none(), "binary returns None");
    }
}
