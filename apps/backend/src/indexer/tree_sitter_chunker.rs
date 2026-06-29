//! AST-aware chunker backed by tree-sitter.
//!
//! Replaces the heuristic [`LineWindowChunker`] for the supported "core 6"
//! languages (Rust, TypeScript, TSX, JavaScript, Python, Go). Instead of
//! arbitrary line windows, each chunk is a real syntactic unit — a function,
//! method, class, struct, etc. — with its true symbol name and line span.
//!
//! Design guarantees (no recall regressions):
//!   * Unsupported language or parse error  → delegate the whole file to the
//!     inner [`LineWindowChunker`].
//!   * File parses but yields no recognized definitions (only comments,
//!     imports, top-level statements) → fall back to the line-window chunker
//!     for the whole file.
//!   * A single oversized symbol (larger than `max_chunk_lines`) is sub-split
//!     with the line-window chunker so it never overflows the embedding model.
//!
//! `tree_sitter::Parser` is not `Sync`, so this struct holds **no** parser —
//! it constructs a fresh `Parser` inside each `chunk()` call (cheap). Only the
//! `Send + Sync` configuration is stored, satisfying the `Chunker` trait.

use tree_sitter::{Language, Node, Parser};

use crate::indexer::chunker::{Chunker, LineWindowChunker, RawChunk};

/// AST-aware chunker with a line-window fallback.
pub struct TreeSitterChunker {
    /// Fallback used for unsupported languages, parse failures, files without
    /// recognized definitions, and sub-splitting oversized symbols.
    fallback: LineWindowChunker,
    /// Symbols spanning more than this many lines are sub-split via `fallback`.
    max_chunk_lines: usize,
}

impl Default for TreeSitterChunker {
    fn default() -> Self {
        TreeSitterChunker {
            fallback: LineWindowChunker::default(),
            max_chunk_lines: 200,
        }
    }
}

impl TreeSitterChunker {
    /// Resolve a tree-sitter grammar from the detected language string and the
    /// file path (the path disambiguates `.tsx` from `.ts`, since both map to
    /// the `"typescript"` language string via `language_for_ext`).
    fn grammar_for(language: Option<&str>, file_path: &str) -> Option<Language> {
        let lang = language?;
        let grammar: Language = match lang {
            "rust" => tree_sitter_rust::LANGUAGE.into(),
            "python" => tree_sitter_python::LANGUAGE.into(),
            "go" => tree_sitter_go::LANGUAGE.into(),
            "javascript" => tree_sitter_javascript::LANGUAGE.into(),
            "typescript" => {
                if file_path.ends_with(".tsx") {
                    tree_sitter_typescript::LANGUAGE_TSX.into()
                } else {
                    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
                }
            }
            _ => return None,
        };
        Some(grammar)
    }

    /// Build a single `RawChunk` for `node`, sub-splitting if it is oversized.
    fn emit_node(
        &self,
        node: Node,
        file_path: &str,
        file_hash: &str,
        language: Option<&str>,
        content: &str,
        out: &mut Vec<RawChunk>,
    ) {
        let start_row = node.start_position().row; // 0-indexed
        let end_row = node.end_position().row; // 0-indexed
        let span_lines = end_row.saturating_sub(start_row) + 1;
        let symbol = node_symbol(node, content);
        let text = &content[node.byte_range()];

        if span_lines > self.max_chunk_lines {
            // Oversized symbol — sub-split with the line-window fallback and
            // re-base its (1-indexed, node-relative) line numbers onto the file,
            // forcing the parent symbol onto every sub-chunk.
            for mut sub in self
                .fallback
                .chunk(file_path, file_hash, language, text)
                .into_iter()
            {
                sub.start_line += start_row as i64;
                sub.end_line += start_row as i64;
                sub.symbol = symbol.clone();
                out.push(sub);
            }
            return;
        }

        out.push(RawChunk {
            file_path: file_path.to_string(),
            file_hash: file_hash.to_string(),
            language: language.map(|s| s.to_string()),
            symbol,
            start_line: (start_row as i64) + 1,
            end_line: (end_row as i64) + 1,
            content: text.to_string(),
        });
    }

    /// Collect the definition nodes to emit from the tree root.
    /// Returns an empty vec when the file has no recognized definitions.
    fn collect_definitions<'a>(&self, root: Node<'a>) -> Vec<Node<'a>> {
        let mut defs = Vec::new();
        let mut i = 0;
        while i < root.named_child_count() {
            if let Some(child) = root.named_child(i as u32) {
                let node = unwrap_export(child);
                let kind = node.kind();
                if is_container(kind) {
                    let methods = collect_methods(node);
                    if methods.is_empty() {
                        defs.push(node); // container with no methods → emit whole
                    } else {
                        defs.extend(methods);
                    }
                } else if is_definition(kind) {
                    defs.push(node);
                }
                // else: imports, top-level statements, etc. — skipped.
            }
            i += 1;
        }
        defs
    }
}

impl Chunker for TreeSitterChunker {
    fn chunk(
        &self,
        file_path: &str,
        file_hash: &str,
        language: Option<&str>,
        content: &str,
    ) -> Vec<RawChunk> {
        if content.is_empty() {
            return vec![];
        }

        // Unsupported language → fallback.
        let grammar = match Self::grammar_for(language, file_path) {
            Some(g) => g,
            None => return self.fallback.chunk(file_path, file_hash, language, content),
        };

        let mut parser = Parser::new();
        if parser.set_language(&grammar).is_err() {
            return self.fallback.chunk(file_path, file_hash, language, content);
        }

        // Parse failure → fallback.
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return self.fallback.chunk(file_path, file_hash, language, content),
        };

        let defs = self.collect_definitions(tree.root_node());
        if defs.is_empty() {
            // No recognized definitions (only comments/imports) → fallback.
            return self.fallback.chunk(file_path, file_hash, language, content);
        }

        let mut out = Vec::with_capacity(defs.len());
        for node in defs {
            self.emit_node(node, file_path, file_hash, language, content, &mut out);
        }
        out
    }
}

// ── Grammar-aware node classification ─────────────────────────────────────────

/// Top-level node kinds emitted as their own chunk.
fn is_definition(kind: &str) -> bool {
    matches!(
        kind,
        // Rust
        "function_item"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "mod_item"
            | "macro_definition"
            | "type_item"
            // Python
            | "function_definition"
            | "decorated_definition"
            // Go
            | "function_declaration"
            | "method_declaration"
            | "type_declaration"
            // JS / TS
            | "generator_function_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration"
    )
}

/// Container kinds whose methods we extract individually instead of emitting
/// the whole container as one chunk.
fn is_container(kind: &str) -> bool {
    matches!(
        kind,
        "impl_item"                  // Rust
            | "class_definition"     // Python
            | "class_declaration"    // JS / TS
            | "abstract_class_declaration"
    )
}

/// Method/function kinds found inside a container body.
fn is_method(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"            // Rust impl
            | "function_definition" // Python class
            | "decorated_definition"
            | "method_definition"   // JS / TS class
    )
}

/// Unwrap an `export`/`export default` statement to its inner declaration.
fn unwrap_export(node: Node) -> Node {
    if node.kind() == "export_statement" {
        let mut i = 0;
        while i < node.named_child_count() {
            if let Some(child) = node.named_child(i as u32) {
                return child;
            }
            i += 1;
        }
    }
    node
}

/// Find the body node of a container (the block holding its members).
fn container_body(node: Node) -> Option<Node> {
    let mut i = 0;
    while i < node.named_child_count() {
        if let Some(child) = node.named_child(i as u32) {
            if matches!(child.kind(), "declaration_list" | "class_body" | "block") {
                return Some(child);
            }
        }
        i += 1;
    }
    None
}

/// Collect method nodes from a container's body.
fn collect_methods(container: Node) -> Vec<Node> {
    let mut methods = Vec::new();
    if let Some(body) = container_body(container) {
        let mut i = 0;
        while i < body.named_child_count() {
            if let Some(child) = body.named_child(i as u32) {
                if is_method(child.kind()) {
                    methods.push(child);
                }
            }
            i += 1;
        }
    }
    methods
}

/// Extract the declared name of a definition node (best-effort).
fn node_symbol(node: Node, src: &str) -> Option<String> {
    // For decorated defs (Python), the name lives on the inner definition.
    let target = if node.kind() == "decorated_definition" {
        node.child_by_field_name("definition").unwrap_or(node)
    } else {
        node
    };

    // Most grammars expose the name under the "name" field.
    if let Some(name) = target.child_by_field_name("name") {
        if let Ok(text) = name.utf8_text(src.as_bytes()) {
            return Some(text.to_string());
        }
    }
    // Rust `impl_item` uses the "type" field for the implemented type.
    if let Some(ty) = target.child_by_field_name("type") {
        if let Ok(text) = ty.utf8_text(src.as_bytes()) {
            return Some(text.to_string());
        }
    }
    // Fallback: first identifier-like named child.
    let mut i = 0;
    while i < target.named_child_count() {
        if let Some(child) = target.named_child(i as u32) {
            if child.kind().contains("identifier") {
                if let Ok(text) = child.utf8_text(src.as_bytes()) {
                    return Some(text.to_string());
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols(chunks: &[RawChunk]) -> Vec<String> {
        chunks.iter().filter_map(|c| c.symbol.clone()).collect()
    }

    #[test]
    fn rust_two_top_level_functions() {
        let chunker = TreeSitterChunker::default();
        let src = "fn alpha() {}\n\nfn beta(x: i32) -> i32 { x }\n";
        let chunks = chunker.chunk("src/lib.rs", "h", Some("rust"), src);
        assert_eq!(chunks.len(), 2, "two top-level fns → two chunks");
        let syms = symbols(&chunks);
        assert!(syms.contains(&"alpha".to_string()));
        assert!(syms.contains(&"beta".to_string()));
        // First chunk spans exactly line 1.
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 1);
    }

    #[test]
    fn rust_impl_methods_extracted_individually() {
        let chunker = TreeSitterChunker::default();
        let src = "struct S;\nimpl S {\n    fn one(&self) {}\n    fn two(&self) {}\n}\n";
        let chunks = chunker.chunk("src/s.rs", "h", Some("rust"), src);
        let syms = symbols(&chunks);
        // struct S + two methods (impl container is not double-counted).
        assert!(syms.contains(&"one".to_string()), "method one extracted");
        assert!(syms.contains(&"two".to_string()), "method two extracted");
        assert!(syms.contains(&"S".to_string()), "struct S emitted");
        // The impl block itself must not appear as its own chunk on top of methods.
        assert_eq!(chunks.len(), 3, "struct + 2 methods, not the impl block");
    }

    #[test]
    fn python_class_methods_extracted() {
        let chunker = TreeSitterChunker::default();
        let src = "class Service:\n    def process(self, req):\n        return req\n\n    def close(self):\n        pass\n";
        let chunks = chunker.chunk("service.py", "h", Some("python"), src);
        let syms = symbols(&chunks);
        assert!(syms.contains(&"process".to_string()), "method process");
        assert!(syms.contains(&"close".to_string()), "method close");
    }

    #[test]
    fn typescript_export_function() {
        let chunker = TreeSitterChunker::default();
        let src = "export function handler(req: Request): Response {\n  return new Response();\n}\n";
        let chunks = chunker.chunk("api/handler.ts", "h", Some("typescript"), src);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].symbol.as_deref(), Some("handler"));
    }

    #[test]
    fn tsx_grammar_selected_by_extension() {
        let chunker = TreeSitterChunker::default();
        // JSX syntax is only valid under the TSX grammar.
        let src = "export function App() {\n  return <div className=\"x\">hi</div>;\n}\n";
        let chunks = chunker.chunk("ui/App.tsx", "h", Some("typescript"), src);
        assert_eq!(chunks.len(), 1, "tsx must parse with the TSX grammar");
        assert_eq!(chunks[0].symbol.as_deref(), Some("App"));
    }

    #[test]
    fn go_function_and_method() {
        let chunker = TreeSitterChunker::default();
        let src = "package main\n\nfunc Add(a, b int) int { return a + b }\n\nfunc (s *Server) Start() {}\n";
        let chunks = chunker.chunk("main.go", "h", Some("go"), src);
        let syms = symbols(&chunks);
        assert!(syms.contains(&"Add".to_string()), "top-level func");
        assert!(syms.contains(&"Start".to_string()), "method declaration");
    }

    #[test]
    fn unsupported_language_falls_back() {
        let chunker = TreeSitterChunker::default();
        let src = "puts 'hello'\ndef ruby_method\nend\n";
        // "ruby" has no grammar here → must delegate to the line-window fallback.
        let ts_chunks = chunker.chunk("a.rb", "h", Some("ruby"), src);
        let fallback = LineWindowChunker::default();
        let fb_chunks = fallback.chunk("a.rb", "h", Some("ruby"), src);
        assert_eq!(
            ts_chunks.len(),
            fb_chunks.len(),
            "unsupported language must match line-window output"
        );
        assert!(!ts_chunks.is_empty(), "fallback must still produce chunks");
    }

    #[test]
    fn file_without_definitions_falls_back() {
        let chunker = TreeSitterChunker::default();
        // Only comments and an import — no definitions for the AST to pick up.
        let src = "// header comment\n// another line\nuse std::fmt;\n";
        let chunks = chunker.chunk("src/x.rs", "h", Some("rust"), src);
        assert_eq!(chunks.len(), 1, "no defs → whole-file fallback (one window)");
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 3);
    }

    #[test]
    fn oversized_symbol_is_subsplit() {
        let chunker = TreeSitterChunker {
            fallback: LineWindowChunker { window: 60, overlap: 15 },
            max_chunk_lines: 50,
        };
        // A single function body of ~200 lines.
        let body: String = (0..200)
            .map(|i| format!("    let v{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");
        let src = format!("fn huge() {{\n{body}\n}}\n");
        let chunks = chunker.chunk("src/huge.rs", "h", Some("rust"), &src);
        assert!(
            chunks.len() > 1,
            "oversized function must be split into multiple chunks, got {}",
            chunks.len()
        );
        // Every sub-chunk keeps the parent symbol.
        assert!(
            chunks.iter().all(|c| c.symbol.as_deref() == Some("huge")),
            "all sub-chunks carry the parent symbol"
        );
        // Line numbers are file-absolute (start at line 1, not node-relative).
        assert_eq!(chunks[0].start_line, 1);
    }

    #[test]
    fn empty_file_produces_no_chunks() {
        let chunker = TreeSitterChunker::default();
        assert!(chunker.chunk("src/empty.rs", "h", Some("rust"), "").is_empty());
    }
}
