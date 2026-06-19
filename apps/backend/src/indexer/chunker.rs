/// Represents a raw chunk extracted from a file before DB persistence.
#[derive(Debug, Clone)]
pub struct RawChunk {
    pub file_path: String,
    pub file_hash: String,
    pub language: Option<String>,
    pub symbol: Option<String>,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
}

/// Trait for chunking strategies. Allows future tree-sitter backends to implement
/// the same interface without schema changes.
pub trait Chunker: Send + Sync {
    /// Chunk the given file content into raw chunks.
    fn chunk(&self, file_path: &str, file_hash: &str, language: Option<&str>, content: &str) -> Vec<RawChunk>;
}

/// Heuristic line-window chunker with overlap.
///
/// Splits content into overlapping windows of `window` lines each,
/// with `overlap` lines of shared context between consecutive chunks.
/// Also attempts to extract a leading symbol name from each window using
/// a simple regex covering the most common patterns (fn, def, class, struct, impl).
pub struct LineWindowChunker {
    pub window: usize,
    pub overlap: usize,
}

impl Default for LineWindowChunker {
    fn default() -> Self {
        LineWindowChunker {
            window: 60,
            overlap: 15,
        }
    }
}

impl LineWindowChunker {
    /// Extract the first symbol name found in a block of lines (best-effort).
    fn extract_symbol(lines: &[&str]) -> Option<String> {
        // Covers: fn name, pub fn name, async fn name, pub async fn name,
        //         def name, class Name, struct Name, impl Name, impl<T> Name
        let pattern = regex::Regex::new(
            r"(?m)(?:pub\s+)?(?:async\s+)?(?:fn|def|class|struct|impl(?:<[^>]*>)?)\s+([A-Za-z_][A-Za-z0-9_]*)"
        ).expect("static regex must compile");

        for line in lines {
            if let Some(cap) = pattern.captures(line) {
                if let Some(m) = cap.get(1) {
                    return Some(m.as_str().to_string());
                }
            }
        }
        None
    }
}

impl Chunker for LineWindowChunker {
    fn chunk(&self, file_path: &str, file_hash: &str, language: Option<&str>, content: &str) -> Vec<RawChunk> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return vec![];
        }

        let mut chunks = Vec::new();
        let step = if self.window > self.overlap {
            self.window - self.overlap
        } else {
            1
        };

        let mut start = 0usize;
        while start < lines.len() {
            let end = (start + self.window).min(lines.len());
            let chunk_lines = &lines[start..end];
            let content_str = chunk_lines.join("\n");
            let symbol = Self::extract_symbol(chunk_lines);

            chunks.push(RawChunk {
                file_path: file_path.to_string(),
                file_hash: file_hash.to_string(),
                language: language.map(|s| s.to_string()),
                symbol,
                start_line: (start + 1) as i64, // 1-indexed
                end_line: end as i64,
                content: content_str,
            });

            if end == lines.len() {
                break;
            }
            start += step;
        }

        chunks
    }
}

// ── Utility: detect language from file extension ──────────────────────────────

pub fn language_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "py" => Some("python"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        "cs" => Some("csharp"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        "swift" => Some("swift"),
        "kt" | "kts" => Some("kotlin"),
        "sh" | "bash" | "zsh" => Some("shell"),
        "md" | "mdx" => Some("markdown"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "json" => Some("json"),
        "sql" => Some("sql"),
        "html" | "htm" => Some("html"),
        "css" | "scss" | "sass" => Some("css"),
        "vue" => Some("vue"),
        "svelte" => Some("svelte"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunker_single_window_small_file() {
        let chunker = LineWindowChunker { window: 60, overlap: 15 };
        let content = (1..=10).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunker.chunk("src/lib.rs", "abc123", Some("rust"), &content);
        // File smaller than window — must produce exactly 1 chunk
        assert_eq!(chunks.len(), 1, "small file must produce 1 chunk");
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 10);
    }

    #[test]
    fn chunker_respects_window_and_overlap() {
        // 100 lines, window=60, overlap=15 → step=45 → starts at 0,45 → 2 chunks
        let chunker = LineWindowChunker { window: 60, overlap: 15 };
        let content = (1..=100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunker.chunk("src/main.rs", "deadbeef", Some("rust"), &content);
        assert!(chunks.len() >= 2, "100-line file with window=60,overlap=15 must produce >= 2 chunks");
        // Check overlap: second chunk starts before first ends
        if chunks.len() >= 2 {
            assert!(chunks[1].start_line <= chunks[0].end_line,
                    "chunks must overlap: chunk[1].start={} <= chunk[0].end={}",
                    chunks[1].start_line, chunks[0].end_line);
        }
    }

    #[test]
    fn chunker_last_chunk_ends_at_eof() {
        let chunker = LineWindowChunker { window: 20, overlap: 5 };
        let content = (1..=55).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunker.chunk("src/foo.rs", "cafebabe", Some("rust"), &content);
        let last = chunks.last().unwrap();
        assert_eq!(last.end_line, 55, "last chunk end_line must equal line count");
    }

    #[test]
    fn chunker_empty_file_produces_no_chunks() {
        let chunker = LineWindowChunker::default();
        let chunks = chunker.chunk("src/empty.rs", "e3b0c44", Some("rust"), "");
        assert!(chunks.is_empty(), "empty file must produce no chunks");
    }

    #[test]
    fn chunker_symbol_extraction_rust_fn() {
        let chunker = LineWindowChunker::default();
        let content = "use std::fmt;\n\npub fn authenticate_user(token: &str) -> bool {\n    true\n}\n";
        let chunks = chunker.chunk("src/auth.rs", "aabbcc", Some("rust"), content);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].symbol.as_deref(), Some("authenticate_user"),
                   "must extract fn name from Rust code");
    }

    #[test]
    fn chunker_symbol_extraction_python_def() {
        let chunker = LineWindowChunker::default();
        let content = "class MyService:\n    def process_request(self, req):\n        pass\n";
        let chunks = chunker.chunk("service.py", "112233", Some("python"), content);
        assert_eq!(chunks.len(), 1);
        // Should capture class OR def — either is fine; just ensure it's not None
        assert!(chunks[0].symbol.is_some(), "must extract symbol from Python code");
    }

    #[test]
    fn chunker_symbol_extraction_no_symbol() {
        let chunker = LineWindowChunker::default();
        let content = "// just a comment\n// another comment\nlet x = 1;\n";
        let chunks = chunker.chunk("src/misc.rs", "998877", Some("rust"), content);
        assert_eq!(chunks.len(), 1);
        // No function/struct/etc. — symbol may be None
        // (Let/const captures are not guaranteed by the design)
        let _ = chunks[0].symbol.as_ref(); // just ensure it doesn't panic
    }

    #[test]
    fn chunker_metadata_preserved() {
        let chunker = LineWindowChunker::default();
        let content = "fn foo() {}\n";
        let chunks = chunker.chunk("src/lib.rs", "ff00ff", Some("rust"), content);
        assert_eq!(chunks[0].file_path, "src/lib.rs");
        assert_eq!(chunks[0].file_hash, "ff00ff");
        assert_eq!(chunks[0].language.as_deref(), Some("rust"));
    }

    #[test]
    fn language_for_ext_known_extensions() {
        assert_eq!(language_for_ext("rs"), Some("rust"));
        assert_eq!(language_for_ext("ts"), Some("typescript"));
        assert_eq!(language_for_ext("py"), Some("python"));
        assert_eq!(language_for_ext("go"), Some("go"));
        assert_eq!(language_for_ext("js"), Some("javascript"));
    }

    #[test]
    fn language_for_ext_unknown_returns_none() {
        assert_eq!(language_for_ext("xyz"), None);
        assert_eq!(language_for_ext("lock"), None);
        assert_eq!(language_for_ext("png"), None);
    }
}
