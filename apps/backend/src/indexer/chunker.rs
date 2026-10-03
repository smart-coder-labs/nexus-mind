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

/// Section-aware chunker for Markdown documents.
///
/// Markdown has no tree-sitter grammar wired into the indexer, so rather than
/// blind fixed-size line windows it is split into semantic sections: each ATX
/// heading (`#` … `######`) starts a new chunk whose `symbol` is the heading
/// text and which runs until the next heading of any level. Content before the
/// first heading becomes a leading "preamble" chunk (symbol `None`). Sections
/// longer than `max_section_lines` are sub-split via [`LineWindowChunker`] so a
/// huge section never overflows the embedding model. Lines inside fenced code
/// blocks (```` ``` ```` or `~~~`) are never mistaken for headings.
pub struct MarkdownChunker {
    /// Sections longer than this are sub-split via the line-window fallback.
    pub max_section_lines: usize,
    fallback: LineWindowChunker,
}

impl Default for MarkdownChunker {
    fn default() -> Self {
        MarkdownChunker {
            max_section_lines: 200,
            fallback: LineWindowChunker::default(),
        }
    }
}

impl MarkdownChunker {
    /// Parse an ATX heading line into its title text, or `None` if the line is
    /// not a heading. Requires 1–6 leading `#` followed by whitespace (or EOL).
    fn heading_title(line: &str) -> Option<String> {
        let trimmed = line.trim_start();
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes == 0 || hashes > 6 {
            return None;
        }
        let rest = &trimmed[hashes..];
        // A valid ATX heading needs a space after the hashes (or nothing at all).
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return None;
        }
        Some(rest.trim().trim_end_matches('#').trim().to_string())
    }

    /// The fence marker character (`` ` `` or `~`) if the line is a code-fence
    /// delimiter, else `None`. A fence can only be closed by its own marker type
    /// (CommonMark), so the caller tracks which marker opened the current fence.
    fn fence_marker(line: &str) -> Option<char> {
        let t = line.trim_start();
        if t.starts_with("```") {
            Some('`')
        } else if t.starts_with("~~~") {
            Some('~')
        } else {
            None
        }
    }

    /// Emit one section (its lines start at file line `start_line`, 1-indexed) as
    /// one or more `RawChunk`s. Skips sections that are entirely blank.
    fn emit_section(
        &self,
        file_path: &str,
        file_hash: &str,
        symbol: Option<String>,
        start_line: usize,
        lines: &[&str],
        out: &mut Vec<RawChunk>,
    ) {
        if lines.iter().all(|l| l.trim().is_empty()) {
            return;
        }
        let span = lines.len();
        let content = lines.join("\n");

        if span > self.max_section_lines {
            // Oversized section — sub-split and re-base the (section-relative,
            // 1-indexed) line numbers onto the file, forcing the heading symbol.
            for mut sub in self
                .fallback
                .chunk(file_path, file_hash, Some("markdown"), &content)
            {
                sub.start_line += start_line as i64 - 1;
                sub.end_line += start_line as i64 - 1;
                sub.symbol = symbol.clone();
                sub.language = Some("markdown".to_string());
                out.push(sub);
            }
            return;
        }

        out.push(RawChunk {
            file_path: file_path.to_string(),
            file_hash: file_hash.to_string(),
            language: Some("markdown".to_string()),
            symbol,
            start_line: start_line as i64,
            end_line: (start_line + span - 1) as i64,
            content,
        });
    }
}

impl Chunker for MarkdownChunker {
    fn chunk(&self, file_path: &str, file_hash: &str, _language: Option<&str>, content: &str) -> Vec<RawChunk> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return vec![];
        }

        let mut out = Vec::new();
        // The marker char of the currently-open fence, or `None` outside a fence.
        let mut fence: Option<char> = None;
        let mut sec_start = 1usize;
        let mut sec_symbol: Option<String> = None;
        let mut sec_lines: Vec<&str> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            if let Some(marker) = Self::fence_marker(line) {
                // Open on the first marker; close only on a matching marker type so a
                // stray `~~~`/``` inside the other kind of fence can't desync state.
                match fence {
                    None => fence = Some(marker),
                    Some(open) if open == marker => fence = None,
                    Some(_) => {}
                }
            }
            let heading = if fence.is_some() { None } else { Self::heading_title(line) };

            if let Some(title) = heading {
                if !sec_lines.is_empty() {
                    self.emit_section(file_path, file_hash, sec_symbol.clone(), sec_start, &sec_lines, &mut out);
                }
                sec_start = i + 1; // 1-indexed
                sec_symbol = Some(title);
                sec_lines = vec![line];
            } else {
                if sec_lines.is_empty() {
                    sec_start = i + 1;
                }
                sec_lines.push(line);
            }
        }
        if !sec_lines.is_empty() {
            self.emit_section(file_path, file_hash, sec_symbol.clone(), sec_start, &sec_lines, &mut out);
        }

        out
    }
}

// ── Skeleton builder: what actually gets embedded ────────────────────────────

/// True for a source-comment line in any of the indexed languages (best-effort).
/// Used to isolate a chunk's leading doc-comment block from its body, and (in the
/// tree-sitter chunker) to extend a symbol chunk upward over its doc-comment lines.
/// `line` is expected to be already trimmed of leading whitespace.
pub(crate) fn is_comment_line(line: &str) -> bool {
    line.starts_with("//")        // Rust / TS / JS / Go / C-family
        || line.starts_with('#')  // Python / shell / Ruby (also Rust attributes)
        || line.starts_with("/*") || line.starts_with('*') // block comments
        || line.starts_with("\"\"\"") || line.starts_with("'''") // Python docstrings
        || line.starts_with("<!--") // HTML / Markdown
        || line.starts_with("--")   // SQL
}

/// Build the compact, NL-friendly text that is embedded and cosine-ranked for a
/// code chunk — deliberately NOT the full body.
///
/// A raw symbol body is dominated by loop bodies, SQL, and nested statements that
/// drown the semantic signal, so a natural-language query ("where are users
/// listed") matches declaration-heavy files instead of the `listUsers` handler
/// that does the work. The skeleton keeps only the parts that carry intent:
///   * the symbol name (led, so its identifier tokens weigh heavily),
///   * the immediately-preceding doc-comment block, when the chunk starts with one,
///   * the declaration / signature line(s): the first up-to-3 non-blank, non-comment
///     lines, stopping at the body opener (`{`).
///
/// Deterministic and language-agnostic — it reads only the chunk text, so it works
/// for tree-sitter chunks and line-window fallbacks alike. Falls back to the head of
/// the content when no signature can be isolated, so a chunk never embeds as empty.
pub fn build_embed_text(symbol: Option<&str>, content: &str) -> String {
    let mut doc: Vec<&str> = Vec::new();
    let mut sig: Vec<&str> = Vec::new();
    let mut past_doc = false;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            // Blank lines before any signature line are skipped; a blank AFTER the
            // signature has started marks the end of the declaration region.
            if sig.is_empty() {
                continue;
            }
            break;
        }
        if !past_doc && is_comment_line(line) {
            doc.push(line);
            continue;
        }
        past_doc = true;
        sig.push(line);
        // Stop at the body opener or after 3 signature lines — the deep body is
        // intentionally excluded.
        if line.contains('{') || sig.len() >= 3 {
            break;
        }
    }

    let mut parts: Vec<&str> = Vec::new();
    if let Some(name) = symbol {
        if !name.is_empty() {
            parts.push(name);
        }
    }
    parts.extend(&doc);
    parts.extend(&sig);

    let text = parts.join("\n");
    if text.trim().is_empty() {
        // Nothing isolatable (no symbol, no comment, no code) → bounded head of the
        // content so the chunk still embeds against a non-empty string.
        content.lines().take(3).collect::<Vec<_>>().join("\n")
    } else {
        text
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
        "md" | "mdx" | "markdown" => Some("markdown"),
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

    #[test]
    fn language_for_ext_markdown_extensions() {
        assert_eq!(language_for_ext("md"), Some("markdown"));
        assert_eq!(language_for_ext("mdx"), Some("markdown"));
        // `.markdown` was previously dropped by the walker — now recognized.
        assert_eq!(language_for_ext("markdown"), Some("markdown"));
    }

    #[test]
    fn markdown_splits_by_heading_sections() {
        let chunker = MarkdownChunker::default();
        let content = "\
# Title

Intro paragraph.

## Installation

Run the installer.

## Usage

Call the API.
";
        let chunks = chunker.chunk("docs/guide.md", "hash1", Some("markdown"), content);
        // One chunk per heading section.
        let symbols: Vec<Option<&str>> = chunks.iter().map(|c| c.symbol.as_deref()).collect();
        assert!(symbols.contains(&Some("Title")), "must produce a chunk for the H1 heading");
        assert!(symbols.contains(&Some("Installation")), "must produce a chunk for Installation");
        assert!(symbols.contains(&Some("Usage")), "must produce a chunk for Usage");
        // All chunks are tagged as markdown so extension filtering works.
        assert!(chunks.iter().all(|c| c.language.as_deref() == Some("markdown")));
    }

    #[test]
    fn markdown_captures_preamble_before_first_heading() {
        let chunker = MarkdownChunker::default();
        let content = "Some intro text with no heading.\n\n# First\n\nBody.\n";
        let chunks = chunker.chunk("readme.md", "h", Some("markdown"), content);
        // Preamble becomes a leading chunk with no symbol.
        assert!(chunks.iter().any(|c| c.symbol.is_none() && c.content.contains("intro text")));
        assert!(chunks.iter().any(|c| c.symbol.as_deref() == Some("First")));
    }

    #[test]
    fn markdown_ignores_hash_inside_code_fence() {
        let chunker = MarkdownChunker::default();
        let content = "\
# Real Heading

```bash
# this is a shell comment, not a heading
echo hi
```

More text.
";
        let chunks = chunker.chunk("x.md", "h", Some("markdown"), content);
        // The shell comment must NOT create its own section.
        assert!(chunks.iter().all(|c| c.symbol.as_deref() != Some("this is a shell comment, not a heading")));
        assert_eq!(
            chunks.iter().filter(|c| c.symbol.is_some()).count(),
            1,
            "only the real heading should yield a titled section"
        );
    }

    #[test]
    fn markdown_mixed_fence_markers_do_not_desync() {
        // A backtick fence containing a stray `~~~` line must NOT close the fence,
        // so the `#` line inside it is not parsed as a heading and the real heading
        // after the fence is still detected.
        let chunker = MarkdownChunker::default();
        let content = "\
# Intro

```text
~~~
# not a heading (inside a backtick fence)
~~~
```

## After Fence

Body.
";
        let chunks = chunker.chunk("x.md", "h", Some("markdown"), content);
        let symbols: Vec<Option<&str>> = chunks.iter().map(|c| c.symbol.as_deref()).collect();
        assert!(symbols.contains(&Some("Intro")));
        assert!(symbols.contains(&Some("After Fence")), "real heading after mixed fences must be found: {symbols:?}");
        assert!(
            !symbols.iter().any(|s| s.map(|t| t.contains("not a heading")).unwrap_or(false)),
            "a `#` line inside the fence must not become a heading: {symbols:?}"
        );
    }

    #[test]
    fn embed_text_keeps_name_signature_doc_excludes_body() {
        // A chunk with a doc comment + signature + a deep body.
        let content = "\
/// Lists all users for the current tenant.
pub fn list_users(tenant: &str) -> Vec<User> {
    let rows = db.query(\"SELECT id, email FROM users WHERE tenant = ?\");
    let mut out = Vec::new();
    for r in rows { out.push(User::from(r)); }
    out
}";
        let text = build_embed_text(Some("list_users"), content);
        // Name is present (led).
        assert!(text.contains("list_users"), "must contain the symbol name: {text}");
        // Doc comment is present.
        assert!(text.contains("Lists all users"), "must contain the doc comment: {text}");
        // Signature is present.
        assert!(text.contains("pub fn list_users(tenant: &str)"), "must contain the signature: {text}");
        // The deep body must be excluded.
        assert!(!text.contains("SELECT id, email"), "must exclude the SQL body: {text}");
        assert!(!text.contains("out.push"), "must exclude the loop body: {text}");
    }

    #[test]
    fn embed_text_without_doc_still_has_name_and_signature() {
        let content = "function createOrder(p) {\n  return db.insert(p);\n}\n";
        let text = build_embed_text(Some("createOrder"), content);
        assert!(text.contains("createOrder"));
        assert!(text.contains("function createOrder(p)"));
        assert!(!text.contains("db.insert"), "body excluded: {text}");
    }

    #[test]
    fn embed_text_falls_back_to_head_when_nothing_isolatable() {
        // No symbol, no comment — a data blob (e.g. a JSON fallback chunk).
        let content = "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}\n";
        let text = build_embed_text(None, content);
        assert!(!text.trim().is_empty(), "must never embed an empty string");
    }

    #[test]
    fn markdown_empty_produces_no_chunks() {
        let chunker = MarkdownChunker::default();
        assert!(chunker.chunk("e.md", "h", Some("markdown"), "").is_empty());
    }

    #[test]
    fn markdown_oversized_section_is_subsplit() {
        let chunker = MarkdownChunker { max_section_lines: 10, fallback: LineWindowChunker { window: 5, overlap: 1 } };
        let body = (1..=40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let content = format!("# Big\n\n{body}\n");
        let chunks = chunker.chunk("big.md", "h", Some("markdown"), &content);
        assert!(chunks.len() > 1, "an oversized section must be sub-split into multiple chunks");
        // Every sub-chunk keeps the heading as its symbol.
        assert!(chunks.iter().all(|c| c.symbol.as_deref() == Some("Big")));
    }
}
