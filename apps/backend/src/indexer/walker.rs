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

        // Size cap (cheap stat — no read)
        match std::fs::metadata(path) {
            Ok(m) if m.len() > MAX_FILE_SIZE => {
                tracing::debug!("Skipping oversized file {:?}", path);
                continue;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::debug!("Could not stat {:?}: {e}", path);
                continue;
            }
        }

        // Extension allowlist
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());
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
