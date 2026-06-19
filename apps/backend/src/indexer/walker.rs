use anyhow::Result;

use crate::indexer::chunker::language_for_ext;

/// Maximum file size to index (1 MB).
const MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Metadata for an eligible source file discovered by the walker.
#[derive(Debug, Clone)]
pub struct FileMeta {
    /// Absolute path to the file.
    pub path: String,
    /// File extension (without dot), if available.
    pub ext: Option<String>,
    /// Detected language, if recognized.
    pub language: Option<String>,
    /// Raw file contents (UTF-8).
    pub content: String,
    /// SHA-256 hex digest of `content`.
    pub hash: String,
}

/// Walk `root_path` using the `ignore` crate (respects `.gitignore` and standard VCS ignore rules),
/// filter by extension allowlist, skip binaries, and skip files over 1 MB.
///
/// Returns metadata for every eligible file, in an unspecified order.
pub fn walk_files(root_path: &str) -> Result<Vec<FileMeta>> {
    let mut results = Vec::new();

    for entry in ignore::WalkBuilder::new(root_path)
        .hidden(true)       // skip hidden files (dot-files)
        .git_ignore(true)   // respect .gitignore in traversed directories
        .git_global(true)   // respect global gitignore
        .git_exclude(true)  // respect .git/info/exclude
        .require_git(false) // respect .gitignore even outside a git repo
        .build()
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!("Walker error (skipped): {e}");
                continue;
            }
        };

        // Only process regular files
        let file_type = match entry.file_type() {
            Some(ft) if ft.is_file() => ft,
            _ => continue,
        };
        let _ = file_type; // suppress unused warning — just used for the is_file check

        let path = entry.path().to_path_buf();

        // Check file size first (cheap)
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("Could not stat {:?}: {e}", path);
                continue;
            }
        };

        if metadata.len() > MAX_FILE_SIZE {
            tracing::debug!("Skipping oversized file {:?} ({} bytes)", path, metadata.len());
            continue;
        }

        // Check extension is in the allowlist
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());

        let language = ext
            .as_deref()
            .and_then(language_for_ext)
            .map(|s| s.to_string());

        if language.is_none() {
            // Extension not in allowlist — skip
            continue;
        }

        // Read content, skipping non-UTF-8 (binary) files
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                tracing::debug!("Skipping binary/non-UTF-8 file {:?}", path);
                continue;
            }
        };

        // Compute SHA-256 hash of content
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let hash = hex::encode(hasher.finalize());

        results.push(FileMeta {
            path: path.to_string_lossy().into_owned(),
            ext,
            language,
            content,
            hash,
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
        // Normal file
        fs::write(dir.path().join("small.rs"), "fn foo() {}").unwrap();
        // Oversized file (1 MB + 1 byte)
        let big_content = "x".repeat(1024 * 1024 + 1);
        fs::write(dir.path().join("big.rs"), big_content).unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1, "oversized file must be skipped, only small.rs");
        assert!(files[0].path.ends_with("small.rs"));
    }

    #[test]
    fn walk_skips_binary_file() {
        let dir = make_temp_project();
        // Create a file with a .rs extension but containing null bytes (binary)
        let binary: Vec<u8> = vec![0u8, 1, 2, 3, 255, 0, 0, 0];
        let bin_path = dir.path().join("binary.rs");
        fs::write(&bin_path, &binary).unwrap();
        // Also write a valid UTF-8 file
        fs::write(dir.path().join("valid.rs"), "fn ok() {}").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        // binary.rs must be skipped, valid.rs must be included
        assert_eq!(files.len(), 1, "binary file must be skipped");
        assert!(files[0].path.ends_with("valid.rs"), "only valid.rs must be indexed");
    }

    #[test]
    fn walk_respects_gitignore() {
        let dir = make_temp_project();
        // Write .gitignore excluding target/
        fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
        // Create ignored directory
        fs::create_dir(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("target").join("ignored.rs"), "fn build() {}").unwrap();
        // Create non-ignored file
        fs::write(dir.path().join("src.rs"), "fn real() {}").unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1, "target/ must be excluded by .gitignore");
        assert!(files[0].path.ends_with("src.rs"));
    }

    #[test]
    fn walk_computes_sha256_hash() {
        let dir = make_temp_project();
        let content = "fn foo() { println!(\"hello\"); }";
        fs::write(dir.path().join("foo.rs"), content).unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1);
        // Hash must be a 64-char hex string (SHA-256)
        assert_eq!(files[0].hash.len(), 64, "SHA-256 must be 64 hex chars");
        // Hash must be deterministic for the same content
        let files2 = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files[0].hash, files2[0].hash, "same content must produce same hash");
    }

    #[test]
    fn walk_content_matches_file() {
        let dir = make_temp_project();
        let content = "pub fn greet(name: &str) -> String { format!(\"hello {name}\") }";
        fs::write(dir.path().join("greet.rs"), content).unwrap();

        let files = walk_files(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].content, content);
    }
}
