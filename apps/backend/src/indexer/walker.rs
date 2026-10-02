use anyhow::Result;

use crate::indexer::chunker::language_for_ext;

/// Maximum file size to index (1 MB).
const MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Directory names that are skipped wholesale (dependencies, build output, VCS).
/// This prunes huge trees (e.g. `node_modules`) even when the repo has no
/// `.gitignore`, so large repos stay indexable. Matched on any path component.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    "dist",
    "build",
    "out",
    "target",
    "vendor",
    "bin",
    "obj",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".angular",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".cache",
    ".gradle",
    ".idea",
    ".vscode",
    "Pods",
    "DerivedData",
    ".terraform",
];

/// Well-known lock / dependency-manifest files that pollute code search: they are
/// huge, machine-generated, and carry no semantic code signal, yet several of them
/// (`pnpm-lock.yaml`, `package-lock.json`) have extensions on the allowlist and so
/// would otherwise be chunked and rank at the top for real code queries. Matched by
/// exact file name.
const NOISE_FILES: &[&str] = &[
    "pnpm-lock.yaml",
    "package-lock.json",
    "yarn.lock",
    "Cargo.lock",
    "poetry.lock",
    "composer.lock",
    "Gemfile.lock",
    "go.sum",
    "bun.lockb",
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
    "rs", // TypeScript / JavaScript
    "ts", "tsx", "js", "jsx", "mjs", "cjs", // Python
    "py", // Go
    "go", // JVM
    "java", "kt", "kts", // C / C++
    "c", "h", "cc", "cpp", "cxx", "hpp", // C#
    "cs", // Ruby / PHP
    "rb", "php", // Swift
    "swift", // Shell
    "sh", "bash", "zsh",
    // Web source (markup/styles/components — real source, not config)
    "html", "htm", "css", "scss", "sass", "vue", "svelte", // SQL
    "sql",
];

/// Config files admitted into the code index for LEXICAL search only (no
/// embeddings): CI workflows, deploy manifests, Dockerfiles, build manifests and
/// GraphQL schemas are where infrastructure and API tasks land, and BM25 matches
/// their specific terms without the semantic noise that kept them out of the
/// code corpus. Lockfiles stay excluded via [`NOISE_FILES`]; env files are never
/// matched (they may hold secrets).
const CONFIG_EXTENSIONS: &[&str] = &["yml", "yaml", "toml", "json", "graphql", "gql"];

/// Config files are capped tighter than code: a big one is generated data.
const CONFIG_MAX_FILE_SIZE: u64 = 256 * 1024;

/// Hidden entries admitted when config is walked; every other dot-entry stays out.
const ALLOWED_HIDDEN: &[&str] = &[".github", ".gitlab-ci.yml"];

/// The language label of a config file admitted for lexical search, if it is one.
fn config_language(file_name: &str, ext: Option<&str>) -> Option<&'static str> {
    let lower = file_name.to_ascii_lowercase();
    // Manifests named for secrets (a k8s Secret, credentials.json) may hold values.
    if lower.contains("secret") || lower.contains("credential") {
        return None;
    }
    if file_name == "Dockerfile"
        || file_name.starts_with("Dockerfile.")
        || file_name.ends_with(".dockerfile")
    {
        return Some("dockerfile");
    }
    if file_name == "Makefile" {
        return Some("makefile");
    }
    match ext? {
        "graphql" | "gql" => Some("graphql"),
        ext if CONFIG_EXTENSIONS.contains(&ext) => language_for_ext(ext),
        _ => None,
    }
}

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
    /// Indexed for lexical search only: chunks get no embedding (config files).
    pub lexical_only: bool,
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
    walk(root_path, false)
}

/// The files of the code index: source code (embedded and lexical) plus config
/// files for lexical search only (see [`CONFIG_EXTENSIONS`]), including CI
/// workflows under `.github/`.
pub fn walk_index_files(root_path: &str) -> Result<Vec<FileMeta>> {
    walk(root_path, true)
}

fn walk(root_path: &str, include_config: bool) -> Result<Vec<FileMeta>> {
    let mut results = Vec::new();

    let walker = ignore::WalkBuilder::new(root_path)
        // With config, hidden entries are filtered below so `.github` gets in.
        .hidden(!include_config)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(move |entry| {
            if include_config && entry.depth() > 0 {
                let name = entry.file_name().to_string_lossy();
                if name.starts_with('.') && !ALLOWED_HIDDEN.contains(&name.as_ref()) {
                    return false;
                }
            }
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
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if let Some(language) = include_config
                .then(|| config_language(file_name, ext.as_deref()))
                .flatten()
            {
                if size <= CONFIG_MAX_FILE_SIZE {
                    results.push(FileMeta {
                        path: path.to_string_lossy().into_owned(),
                        ext,
                        language: Some(language.to_string()),
                        size,
                        lexical_only: true,
                    });
                }
            }
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
            lexical_only: false,
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
        assert_eq!(
            files.len(),
            1,
            "oversized file must be skipped, only small.rs"
        );
        assert!(files[0].path.ends_with("small.rs"));
    }

    #[test]
    fn walk_prunes_heavy_directories() {
        let dir = make_temp_project();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(
            dir.path().join("node_modules").join("dep.js"),
            "module.exports = {}",
        )
        .unwrap();
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
        assert_eq!(
            files.len(),
            1,
            "only the real source file must remain: {files:?}"
        );
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
        assert_eq!(
            files.len(),
            1,
            "only the .ts source file must remain: {files:?}"
        );
        assert!(files[0].path.ends_with("foo.ts"));
        assert_eq!(files[0].language.as_deref(), Some("typescript"));
    }

    #[test]
    fn the_index_walk_adds_config_for_lexical_search_only() {
        let dir = make_temp_project();
        let root = dir.path();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::create_dir_all(root.join(".secret")).unwrap();
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::write(root.join(".github/workflows/ci.yml"), "on: push\n").unwrap();
        fs::write(root.join(".gitlab-ci.yml"), "stages: []\n").unwrap();
        fs::write(root.join(".secret/keys.yml"), "k: v\n").unwrap();
        fs::write(root.join(".env"), "TOKEN=x\n").unwrap();
        fs::write(root.join(".env.production.json"), "{}").unwrap();
        fs::write(root.join("Dockerfile"), "FROM rust\n").unwrap();
        fs::write(root.join("Dockerfile.worker"), "FROM rust\n").unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(root.join("schema/api.graphql"), "type Query { a: Int }\n").unwrap();
        fs::write(root.join("package-lock.json"), "{}").unwrap();
        fs::write(root.join("README.md"), "# docs stay in the doc corpus\n").unwrap();
        fs::write(root.join("huge.json"), vec![b' '; 300 * 1024]).unwrap();
        fs::write(root.join("app-secret.yaml"), "kind: Secret\n").unwrap();
        fs::write(root.join("credentials.json"), "{}").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();

        let mut found: Vec<(String, bool, String)> = walk_index_files(root.to_str().unwrap())
            .unwrap()
            .into_iter()
            .map(|f| {
                let rel = f
                    .path
                    .strip_prefix(root.to_str().unwrap())
                    .unwrap()
                    .trim_start_matches('/')
                    .to_string();
                (rel, f.lexical_only, f.language.unwrap_or_default())
            })
            .collect();
        found.sort();
        let expect = |rel: &str, lexical: bool, language: &str| {
            (rel.to_string(), lexical, language.to_string())
        };
        assert_eq!(
            found,
            vec![
                expect(".github/workflows/ci.yml", true, "yaml"),
                expect(".gitlab-ci.yml", true, "yaml"),
                expect("Cargo.toml", true, "toml"),
                expect("Dockerfile", true, "dockerfile"),
                expect("Dockerfile.worker", true, "dockerfile"),
                expect("main.rs", false, "rust"),
                expect("schema/api.graphql", true, "graphql"),
            ]
        );
        // The code-only walk (migration connector) is unchanged.
        let code = walk_files(root.to_str().unwrap()).unwrap();
        assert_eq!(code.len(), 1);
        assert!(!code[0].lexical_only);
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
        assert!(
            read_file(p.to_str().unwrap()).is_none(),
            "binary returns None"
        );
    }
}
