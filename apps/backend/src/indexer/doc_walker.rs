//! Documentation walker — the sibling of [`super::walker`], with the allowlist
//! inverted.
//!
//! `walker.rs` is code-only by an explicit decision recorded in its own header:
//! Markdown files "dominate code-search results with non-code prose while
//! carrying no code signal". That decision stands and this module does not
//! touch it. What it does is take up the invitation the same comment leaves —
//! *"`language_for_ext` still recognizes several of those extensions for other
//! callers (e.g. `MarkdownChunker`)"* — and feed that chunker from a corpus of
//! its own.
//!
//! Two corpora, two walkers, one shared ignore configuration. A single walker
//! with an `is_doc` flag would mean every existing code query needs a filter,
//! and the one that gets forgotten reintroduces exactly the ranking bug the
//! code-search precision work paid to fix.

use anyhow::Result;

use super::walker::FileMeta;

/// Documentation extensions admitted into the DOC corpus. Deliberately narrow:
/// this is prose a human wrote, not config and not data.
const DOC_EXTENSIONS: &[&str] = &["md", "mdx"];

/// Directories never worth indexing as documentation, pruned before descent.
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
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".cache",
];

/// Files that are documentation by extension but not knowledge by nature.
/// `CHANGELOG.md` is machine-shaped and enormous; licences are boilerplate every
/// repo shares. Both would dominate a documentation search the way `README.md`
/// once dominated code search.
const DEFAULT_EXCLUDED_NAMES: &[&str] = &["CHANGELOG.md", "CHANGELOG.mdx"];
const DEFAULT_EXCLUDED_PREFIXES: &[&str] = &["LICENSE", "LICENCE", "COPYING"];

/// 1 MB, matching the code walker. A Markdown file above this is generated.
const MAX_FILE_SIZE: u64 = 1_048_576;

fn is_doc_extension(ext: &str) -> bool {
    DOC_EXTENSIONS.contains(&ext)
}

fn is_excluded_name(name: &str) -> bool {
    DEFAULT_EXCLUDED_NAMES.contains(&name)
        || DEFAULT_EXCLUDED_PREFIXES
            .iter()
            .any(|p| name.to_uppercase().starts_with(p))
}

#[derive(Debug, Clone, Default)]
pub struct DocWalkOptions {
    /// Extra path substrings to exclude, on top of the defaults. The connectors
    /// use this for `docs/marketing/**` and similar.
    pub extra_excludes: Vec<String>,
    /// When non-empty, only paths containing one of these substrings are kept.
    pub includes: Vec<String>,
    /// Drop the built-in name exclusions. Off by default; a caller that really
    /// wants the CHANGELOG has to say so.
    pub include_default_excluded: bool,
}

/// Walk `root_path` for documentation, respecting `.gitignore` and the standard
/// VCS ignores exactly as the code walker does. Returns metadata only — content
/// is read later, one file at a time, so a large repo's discovery stays cheap.
pub fn walk_docs(root_path: &str, opts: &DocWalkOptions) -> Result<Vec<FileMeta>> {
    let mut results = Vec::new();

    let walker = ignore::WalkBuilder::new(root_path)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(|entry| {
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
                tracing::debug!("Doc walker error (skipped): {e}");
                continue;
            }
        };
        match entry.file_type() {
            Some(ft) if ft.is_file() => {}
            _ => continue,
        }
        let path = entry.path();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());
        if !ext.as_deref().map(is_doc_extension).unwrap_or(false) {
            continue;
        }

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !opts.include_default_excluded && is_excluded_name(name) {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        if opts.extra_excludes.iter().any(|x| path_str.contains(x)) {
            continue;
        }
        if !opts.includes.is_empty() && !opts.includes.iter().any(|i| path_str.contains(i)) {
            continue;
        }

        let size = match std::fs::metadata(path) {
            Ok(m) if m.len() > MAX_FILE_SIZE => continue,
            Ok(m) => m.len(),
            Err(e) => {
                tracing::debug!("Could not stat {:?}: {e}", path);
                continue;
            }
        };

        results.push(FileMeta {
            path: path_str,
            ext,
            language: Some("markdown".to_string()),
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

    fn names(files: &[FileMeta]) -> Vec<String> {
        let mut v: Vec<String> = files
            .iter()
            .map(|f| {
                std::path::Path::new(&f.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn doc_walker_admits_markdown_and_respects_gitignore() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored.md\n").unwrap();
        fs::write(dir.path().join("README.md"), "# Hi").unwrap();
        fs::write(dir.path().join("guide.mdx"), "# Guide").unwrap();
        fs::write(dir.path().join("ignored.md"), "# No").unwrap();

        let found = walk_docs(dir.path().to_str().unwrap(), &DocWalkOptions::default()).unwrap();
        assert_eq!(names(&found), vec!["README.md", "guide.mdx"]);
    }

    /// The inverse of the code walker's allowlist. If this ever admits a `.rs`
    /// file, the two corpora have started to merge.
    #[test]
    fn doc_walker_excludes_code_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("app.ts"), "export {}").unwrap();
        fs::write(dir.path().join("config.json"), "{}").unwrap();
        fs::write(dir.path().join("notes.md"), "# Notes").unwrap();

        let found = walk_docs(dir.path().to_str().unwrap(), &DocWalkOptions::default()).unwrap();
        assert_eq!(names(&found), vec!["notes.md"]);
    }

    #[test]
    fn doc_walker_applies_default_excludes() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CHANGELOG.md"), "# 1.0").unwrap();
        fs::write(dir.path().join("LICENSE.md"), "MIT").unwrap();
        fs::write(dir.path().join("ARCHITECTURE.md"), "# Arch").unwrap();

        let found = walk_docs(dir.path().to_str().unwrap(), &DocWalkOptions::default()).unwrap();
        assert_eq!(names(&found), vec!["ARCHITECTURE.md"]);

        let with_all = walk_docs(
            dir.path().to_str().unwrap(),
            &DocWalkOptions {
                include_default_excluded: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(with_all.len(), 3, "the caller can opt back in explicitly");
    }

    #[test]
    fn doc_walker_honours_includes_and_extra_excludes() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("docs/marketing")).unwrap();
        fs::create_dir_all(dir.path().join("docs/adr")).unwrap();
        fs::write(dir.path().join("docs/adr/ADR-001.md"), "# ADR").unwrap();
        fs::write(dir.path().join("docs/marketing/pitch.md"), "# Pitch").unwrap();
        fs::write(dir.path().join("elsewhere.md"), "# Other").unwrap();

        let found = walk_docs(
            dir.path().to_str().unwrap(),
            &DocWalkOptions {
                includes: vec!["docs/".to_string()],
                extra_excludes: vec!["docs/marketing".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(names(&found), vec!["ADR-001.md"]);
    }

    #[test]
    fn doc_walker_prunes_heavy_directories() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        fs::write(dir.path().join("node_modules/pkg/README.md"), "# dep").unwrap();
        fs::write(dir.path().join("README.md"), "# ours").unwrap();

        let found = walk_docs(dir.path().to_str().unwrap(), &DocWalkOptions::default()).unwrap();
        assert_eq!(found.len(), 1, "a dependency's README is not our documentation");
    }
}
