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

use std::collections::HashSet;

use tree_sitter::{Language, Node, Parser};

use crate::indexer::chunker::{is_comment_line, Chunker, LineWindowChunker, MarkdownChunker, RawChunk};

// ── Code graph types ──────────────────────────────────────────────────────────

/// The kind of code entity a `RawSymbol` represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolType {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Type,
    Module,
    File,
    Folder,
    Project,
    External,
}

impl SymbolType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolType::Function  => "Function",
            SymbolType::Method    => "Method",
            SymbolType::Class     => "Class",
            SymbolType::Struct    => "Struct",
            SymbolType::Enum      => "Enum",
            SymbolType::Interface => "Interface",
            SymbolType::Type      => "Type",
            SymbolType::Module    => "Module",
            SymbolType::File      => "File",
            SymbolType::Folder    => "Folder",
            SymbolType::Project   => "Project",
            SymbolType::External  => "External",
        }
    }
}

/// The directed relationship between two graph nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeType {
    Defines,
    DefinesMethod,
    Imports,
    ContainsFolder,
    ContainsFile,
    ContainsProject,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::Defines        => "defines",
            EdgeType::DefinesMethod  => "defines_method",
            EdgeType::Imports        => "imports",
            EdgeType::ContainsFolder => "contains_folder",
            EdgeType::ContainsFile   => "contains_file",
            EdgeType::ContainsProject => "contains_project",
        }
    }
}

/// Controls the upsert strategy used when persisting a symbol.
///
/// * `Shared`   — stable virtual nodes (File, Folder, Project, External): `INSERT OR IGNORE` then
///   `SELECT id`. The node survives per-file re-indexes and is only removed via CASCADE when the
///   project is deleted.
/// * `FileOwned` — code symbols tied to a specific file (Function, Method, Class, …): plain
///   `INSERT` + `last_insert_rowid()`. Deleted and re-inserted atomically on every file re-index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Persist {
    Shared,
    FileOwned,
}

/// A code entity extracted from one source file.
#[derive(Debug, Clone)]
pub struct RawSymbol {
    pub symbol_type:    SymbolType,
    pub name:           String,
    /// Stable identity key: `{rel_path}::{name}#{start_line}` for FileOwned, or
    /// `file::`, `folder::`, `project::`, `external::` prefixes for Shared nodes.
    pub qualified_name: String,
    pub file_path:      Option<String>,
    pub file_hash:      Option<String>,
    pub start_line:     Option<i64>,
    pub end_line:       Option<i64>,
    pub language:       String,
    pub persist:        Persist,
}

/// A directed edge between two code entities, identified by their `qualified_name`.
#[derive(Debug, Clone)]
pub struct RawEdge {
    pub from_qname: String,
    pub to_qname:   String,
    pub edge_type:  EdgeType,
    pub file_path:  Option<String>,
    pub persist:    Persist,
}

/// All graph nodes and edges extracted from a single source file.
#[derive(Debug, Clone)]
pub struct FileGraph {
    pub file_rel_path: String,
    pub symbols:       Vec<RawSymbol>,
    pub edges:         Vec<RawEdge>,
}

/// AST-aware chunker with a line-window fallback.
pub struct TreeSitterChunker {
    /// Fallback used for unsupported languages, parse failures, files without
    /// recognized definitions, and sub-splitting oversized symbols.
    fallback: LineWindowChunker,
    /// Heading-section chunker for Markdown, which has no tree-sitter grammar here.
    markdown: MarkdownChunker,
    /// Symbols spanning more than this many lines are sub-split via `fallback`.
    max_chunk_lines: usize,
}

impl Default for TreeSitterChunker {
    fn default() -> Self {
        TreeSitterChunker {
            fallback: LineWindowChunker::default(),
            markdown: MarkdownChunker::default(),
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
        let node_start_row = node.start_position().row; // 0-indexed
        let end_row = node.end_position().row; // 0-indexed
        let symbol = node_symbol(node, content);

        // Extend the chunk's start upward over the contiguous doc-comment block
        // immediately preceding the symbol (`///`, `//`, `/** */`, `#`, `"""`, …),
        // tolerating blank lines between the comment and the symbol. Tree-sitter
        // definition nodes start AT the symbol, so without this the doc-comment —
        // the one part `build_embed_text` uses for NL ranking — would be dropped.
        let lines: Vec<&str> = content.lines().collect();
        let start_row = extend_start_over_doc_comment(&lines, node_start_row);
        let span_lines = end_row.saturating_sub(start_row) + 1;

        // Rebuild the chunk text from whole lines [start_row, end_row] so the
        // preceding doc-comment (and any blank line before the symbol) is included.
        let last = end_row.min(lines.len().saturating_sub(1));
        let text: String = lines[start_row..=last].join("\n");

        if span_lines > self.max_chunk_lines {
            // Oversized symbol — sub-split with the line-window fallback and
            // re-base its (1-indexed, chunk-relative) line numbers onto the file,
            // forcing the parent symbol onto every sub-chunk.
            for mut sub in self
                .fallback
                .chunk(file_path, file_hash, language, &text)
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
            content: text,
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

impl TreeSitterChunker {
    /// Single-parse entry point that produces both code chunks **and** the file-level graph.
    ///
    /// Uses exactly one `tree_sitter::Parser::parse` call — the resulting tree is walked twice:
    /// once for chunk extraction (same logic as [`Chunker::chunk`]) and once for graph data
    /// collection. Returns `None` for the graph when the language is unsupported or the parse
    /// fails.
    pub fn chunk_with_graph(
        &self,
        file_path: &str,
        file_hash: &str,
        language: Option<&str>,
        content: &str,
        known_files: &HashSet<String>,
    ) -> (Vec<RawChunk>, Option<FileGraph>) {
        if content.is_empty() {
            return (vec![], None);
        }

        // Markdown has no code graph — chunk it into heading sections (content only).
        if language == Some("markdown") {
            return (
                self.markdown.chunk(file_path, file_hash, language, content),
                None,
            );
        }

        let grammar = match Self::grammar_for(language, file_path) {
            Some(g) => g,
            None => {
                return (
                    self.fallback.chunk(file_path, file_hash, language, content),
                    None,
                );
            }
        };

        let mut parser = Parser::new();
        if parser.set_language(&grammar).is_err() {
            return (
                self.fallback.chunk(file_path, file_hash, language, content),
                None,
            );
        }

        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => {
                return (
                    self.fallback.chunk(file_path, file_hash, language, content),
                    None,
                );
            }
        };

        let lang_str = language.unwrap_or("unknown");
        let root = tree.root_node();

        // Chunk extraction — same logic as Chunker::chunk.
        let defs = self.collect_definitions(root);
        let mut chunks = if defs.is_empty() {
            self.fallback.chunk(file_path, file_hash, language, content)
        } else {
            let mut out = Vec::with_capacity(defs.len());
            for node in defs {
                self.emit_node(node, file_path, file_hash, language, content, &mut out);
            }
            out
        };
        // Safety net: a non-empty file must always yield ≥1 searchable chunk. If the
        // AST path produced nothing (grammar edge cases, all-error trees), fall back
        // to line windows rather than dropping the file from the index.
        if chunks.is_empty() {
            chunks = self.fallback.chunk(file_path, file_hash, language, content);
        }

        // Graph extraction — reuses the same already-parsed tree.
        let file_graph =
            collect_graph_data(root, file_path, file_hash, lang_str, content, known_files);

        (chunks, Some(file_graph))
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

        // Markdown → heading-section chunker (no tree-sitter grammar).
        if language == Some("markdown") {
            return self.markdown.chunk(file_path, file_hash, language, content);
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
        // Safety net: a non-empty file must always yield ≥1 searchable chunk.
        if out.is_empty() {
            return self.fallback.chunk(file_path, file_hash, language, content);
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

// ── Graph data collection ─────────────────────────────────────────────────────

/// Walk a parsed tree-sitter root and collect code graph symbols and edges.
///
/// Returns a [`FileGraph`] that may be empty (no symbols/edges) for files that
/// contain only unsupported constructs. Never returns an error — unsupported nodes
/// are simply skipped.
fn collect_graph_data<'a>(
    root: Node<'a>,
    file_path: &str,
    file_hash: &str,
    language: &str,
    content: &str,
    known_files: &HashSet<String>,
) -> FileGraph {
    let mut symbols: Vec<RawSymbol> = Vec::new();
    let mut edges: Vec<RawEdge> = Vec::new();
    // De-duplicate External stubs across this file.
    let mut seen_externals: HashSet<String> = HashSet::new();
    // De-duplicate edges within this file (from_qname, to_qname, edge_type).
    let mut seen_edges: HashSet<(String, String, &'static str)> = HashSet::new();

    let file_qname = format!("file::{}", file_path);
    let file_dir = std::path::Path::new(file_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    let emit_edge = |edges: &mut Vec<RawEdge>,
                         seen_edges: &mut HashSet<(String, String, &'static str)>,
                         from: String,
                         to: String,
                         et: EdgeType,
                         fp: Option<String>,
                         persist: Persist| {
        let key = (from.clone(), to.clone(), et.as_str());
        if seen_edges.insert(key) {
            edges.push(RawEdge {
                from_qname: from,
                to_qname: to,
                edge_type: et,
                file_path: fp,
                persist,
            });
        }
    };

    let mut i = 0;
    while i < root.named_child_count() {
        if let Some(child) = root.named_child(i as u32) {
            let node = unwrap_export(child);
            let kind = node.kind();

            // ── Import extraction ──────────────────────────────────────────────
            let is_import = matches!(
                (language, kind),
                ("rust", "use_declaration")
                    | ("typescript" | "javascript", "import_statement")
                    | ("python", "import_statement" | "import_from_statement")
                    | ("go", "import_declaration")
            );
            if is_import {
                let sources = extract_import_sources(node, language, content);
                for source in sources {
                    if let Some(resolved) =
                        resolve_import(&source, file_dir, language, known_files)
                    {
                        let to_qname = format!("file::{}", resolved);
                        emit_edge(
                            &mut edges,
                            &mut seen_edges,
                            file_qname.clone(),
                            to_qname,
                            EdgeType::Imports,
                            Some(file_path.to_string()),
                            Persist::FileOwned,
                        );
                    } else {
                        // External stub
                        let ext_name = external_name(&source);
                        if !ext_name.is_empty() && seen_externals.insert(ext_name.clone()) {
                            let ext_qname = format!("external::{}", ext_name);
                            symbols.push(RawSymbol {
                                symbol_type:    SymbolType::External,
                                name:           ext_name.clone(),
                                qualified_name: ext_qname.clone(),
                                file_path:      None,
                                file_hash:      None,
                                start_line:     None,
                                end_line:       None,
                                language:       "external".to_string(),
                                persist:        Persist::Shared,
                            });
                            emit_edge(
                                &mut edges,
                                &mut seen_edges,
                                file_qname.clone(),
                                ext_qname,
                                EdgeType::Imports,
                                Some(file_path.to_string()),
                                Persist::FileOwned,
                            );
                        } else if !ext_name.is_empty() {
                            // Already seen — just add the edge (dedup handles duplicates)
                            let ext_qname = format!("external::{}", ext_name);
                            emit_edge(
                                &mut edges,
                                &mut seen_edges,
                                file_qname.clone(),
                                ext_qname,
                                EdgeType::Imports,
                                Some(file_path.to_string()),
                                Persist::FileOwned,
                            );
                        }
                    }
                }
                i += 1;
                continue;
            }

            // ── Definition extraction ──────────────────────────────────────────
            match (language, kind) {
                ("rust", "impl_item") => {
                    // Container: emit as Class + emit each method with defines_method edge.
                    if let Some(container_name) = node_symbol(node, content) {
                        let cs = (node.start_position().row as i64) + 1;
                        let ce = (node.end_position().row as i64) + 1;
                        let container_qname =
                            format!("{}::{}#{}", file_path, container_name, cs);
                        symbols.push(RawSymbol {
                            symbol_type:    SymbolType::Class,
                            name:           container_name.clone(),
                            qualified_name: container_qname.clone(),
                            file_path:      Some(file_path.to_string()),
                            file_hash:      Some(file_hash.to_string()),
                            start_line:     Some(cs),
                            end_line:       Some(ce),
                            language:       language.to_string(),
                            persist:        Persist::FileOwned,
                        });
                        emit_edge(
                            &mut edges,
                            &mut seen_edges,
                            file_qname.clone(),
                            container_qname.clone(),
                            EdgeType::Defines,
                            Some(file_path.to_string()),
                            Persist::FileOwned,
                        );
                        for method in collect_methods(node) {
                            if let Some(mname) = node_symbol(method, content) {
                                let ms = (method.start_position().row as i64) + 1;
                                let me = (method.end_position().row as i64) + 1;
                                let method_qname =
                                    format!("{}::{}::{}#{}", file_path, container_name, mname, ms);
                                symbols.push(RawSymbol {
                                    symbol_type:    SymbolType::Method,
                                    name:           mname,
                                    qualified_name: method_qname.clone(),
                                    file_path:      Some(file_path.to_string()),
                                    file_hash:      Some(file_hash.to_string()),
                                    start_line:     Some(ms),
                                    end_line:       Some(me),
                                    language:       language.to_string(),
                                    persist:        Persist::FileOwned,
                                });
                                emit_edge(
                                    &mut edges,
                                    &mut seen_edges,
                                    container_qname.clone(),
                                    method_qname,
                                    EdgeType::DefinesMethod,
                                    Some(file_path.to_string()),
                                    Persist::FileOwned,
                                );
                            }
                        }
                    }
                }
                ("python", "class_definition") | ("typescript" | "javascript", "class_declaration" | "abstract_class_declaration") => {
                    // Container: emit as Class + emit each method with defines_method edge.
                    if let Some(container_name) = node_symbol(node, content) {
                        let cs = (node.start_position().row as i64) + 1;
                        let ce = (node.end_position().row as i64) + 1;
                        let container_qname =
                            format!("{}::{}#{}", file_path, container_name, cs);
                        symbols.push(RawSymbol {
                            symbol_type:    SymbolType::Class,
                            name:           container_name.clone(),
                            qualified_name: container_qname.clone(),
                            file_path:      Some(file_path.to_string()),
                            file_hash:      Some(file_hash.to_string()),
                            start_line:     Some(cs),
                            end_line:       Some(ce),
                            language:       language.to_string(),
                            persist:        Persist::FileOwned,
                        });
                        emit_edge(
                            &mut edges,
                            &mut seen_edges,
                            file_qname.clone(),
                            container_qname.clone(),
                            EdgeType::Defines,
                            Some(file_path.to_string()),
                            Persist::FileOwned,
                        );
                        for method in collect_methods(node) {
                            if let Some(mname) = node_symbol(method, content) {
                                let ms = (method.start_position().row as i64) + 1;
                                let me = (method.end_position().row as i64) + 1;
                                let method_qname =
                                    format!("{}::{}::{}#{}", file_path, container_name, mname, ms);
                                symbols.push(RawSymbol {
                                    symbol_type:    SymbolType::Method,
                                    name:           mname,
                                    qualified_name: method_qname.clone(),
                                    file_path:      Some(file_path.to_string()),
                                    file_hash:      Some(file_hash.to_string()),
                                    start_line:     Some(ms),
                                    end_line:       Some(me),
                                    language:       language.to_string(),
                                    persist:        Persist::FileOwned,
                                });
                                emit_edge(
                                    &mut edges,
                                    &mut seen_edges,
                                    container_qname.clone(),
                                    method_qname,
                                    EdgeType::DefinesMethod,
                                    Some(file_path.to_string()),
                                    Persist::FileOwned,
                                );
                            }
                        }
                    }
                }
                // Go: type_declaration disambiguated by inner type_spec child kind.
                ("go", "type_declaration") => {
                    let sym_type = go_type_decl_symbol_type(node);
                    if let Some(name) = go_type_decl_name(node, content) {
                        let start = (node.start_position().row as i64) + 1;
                        let end   = (node.end_position().row as i64) + 1;
                        let qname = format!("{}::{}#{}", file_path, name, start);
                        symbols.push(RawSymbol {
                            symbol_type:    sym_type,
                            name:           name.clone(),
                            qualified_name: qname.clone(),
                            file_path:      Some(file_path.to_string()),
                            file_hash:      Some(file_hash.to_string()),
                            start_line:     Some(start),
                            end_line:       Some(end),
                            language:       language.to_string(),
                            persist:        Persist::FileOwned,
                        });
                        emit_edge(
                            &mut edges,
                            &mut seen_edges,
                            file_qname.clone(),
                            qname,
                            EdgeType::Defines,
                            Some(file_path.to_string()),
                            Persist::FileOwned,
                        );
                    }
                }
                // All other recognized definition node kinds.
                _ if is_definition(kind) => {
                    let sym_type = lang_kind_to_symbol_type(language, kind);
                    if let Some(name) = node_symbol(node, content) {
                        let start = (node.start_position().row as i64) + 1;
                        let end   = (node.end_position().row as i64) + 1;
                        let qname = format!("{}::{}#{}", file_path, name, start);
                        symbols.push(RawSymbol {
                            symbol_type:    sym_type,
                            name:           name.clone(),
                            qualified_name: qname.clone(),
                            file_path:      Some(file_path.to_string()),
                            file_hash:      Some(file_hash.to_string()),
                            start_line:     Some(start),
                            end_line:       Some(end),
                            language:       language.to_string(),
                            persist:        Persist::FileOwned,
                        });
                        emit_edge(
                            &mut edges,
                            &mut seen_edges,
                            file_qname.clone(),
                            qname,
                            EdgeType::Defines,
                            Some(file_path.to_string()),
                            Persist::FileOwned,
                        );
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    FileGraph {
        file_rel_path: file_path.to_string(),
        symbols,
        edges,
    }
}

/// Map a (language, ast-node-kind) pair to a [`SymbolType`].
fn lang_kind_to_symbol_type(language: &str, kind: &str) -> SymbolType {
    match (language, kind) {
        ("rust", "function_item")                        => SymbolType::Function,
        ("rust", "struct_item")                          => SymbolType::Struct,
        ("rust", "enum_item")                            => SymbolType::Enum,
        ("rust", "trait_item")                           => SymbolType::Interface,
        ("rust", "type_item" | "type_alias")             => SymbolType::Type,
        ("rust", "mod_item")                             => SymbolType::Module,
        ("typescript" | "javascript",
         "function_declaration" | "arrow_function")      => SymbolType::Function,
        ("typescript" | "javascript", "method_definition") => SymbolType::Method,
        ("typescript" | "javascript", "interface_declaration") => SymbolType::Interface,
        ("typescript" | "javascript", "type_alias_declaration") => SymbolType::Type,
        ("typescript" | "javascript", "enum_declaration") => SymbolType::Enum,
        ("python", "function_definition" | "decorated_definition") => SymbolType::Function,
        ("go", "function_declaration")                   => SymbolType::Function,
        ("go", "method_declaration")                     => SymbolType::Method,
        _                                                => SymbolType::Function,
    }
}

// ── Go type_declaration helpers ───────────────────────────────────────────────

/// Disambiguate a Go `type_declaration` into Struct / Interface / Type by
/// inspecting the inner `type_spec` child's `type` field.
fn go_type_decl_symbol_type(node: Node) -> SymbolType {
    let mut i = 0;
    while i < node.named_child_count() {
        if let Some(spec) = node.named_child(i as u32) {
            if spec.kind() == "type_spec" {
                if let Some(ty) = spec.child_by_field_name("type") {
                    return match ty.kind() {
                        "struct_type"    => SymbolType::Struct,
                        "interface_type" => SymbolType::Interface,
                        _                => SymbolType::Type,
                    };
                }
            }
        }
        i += 1;
    }
    SymbolType::Type
}

/// Extract the declared name from a Go `type_declaration` via its `type_spec` child.
fn go_type_decl_name<'a>(node: Node<'a>, content: &str) -> Option<String> {
    let mut i = 0;
    while i < node.named_child_count() {
        if let Some(spec) = node.named_child(i as u32) {
            if spec.kind() == "type_spec" {
                if let Some(name_node) = spec.child_by_field_name("name") {
                    return name_node.utf8_text(content.as_bytes()).ok().map(|s| s.to_string());
                }
            }
        }
        i += 1;
    }
    None
}

// ── Import helpers ────────────────────────────────────────────────────────────

/// Extract zero or more import-source strings from an import node.
/// Returns the raw module/path strings (e.g. `"./utils"`, `"requests"`, `"fmt"`).
fn extract_import_sources(node: Node, language: &str, content: &str) -> Vec<String> {
    let mut sources = Vec::new();
    match language {
        "typescript" | "javascript" => {
            if node.kind() == "import_statement" {
                if let Some(src) = node.child_by_field_name("source") {
                    if let Ok(text) = src.utf8_text(content.as_bytes()) {
                        let clean = text.trim_matches('"').trim_matches('\'');
                        if !clean.is_empty() {
                            sources.push(clean.to_string());
                        }
                    }
                }
            }
        }
        "python" => {
            if node.kind() == "import_from_statement" {
                if let Some(module) = node.child_by_field_name("module_name") {
                    if let Ok(text) = module.utf8_text(content.as_bytes()) {
                        sources.push(text.to_string());
                    }
                }
            } else if node.kind() == "import_statement" {
                // import X, Y — iterate named children looking for module identifiers
                let mut j = 0;
                while j < node.named_child_count() {
                    if let Some(c) = node.named_child(j as u32) {
                        let k = c.kind();
                        if k == "dotted_name" || k == "aliased_import" {
                            // For aliased_import, first child is the actual module
                            let target = if k == "aliased_import" {
                                c.named_child(0).unwrap_or(c)
                            } else {
                                c
                            };
                            if let Ok(text) = target.utf8_text(content.as_bytes()) {
                                // Only the first dotted segment matters for external detection
                                let first = text.split('.').next().unwrap_or(text);
                                sources.push(first.to_string());
                            }
                        }
                    }
                    j += 1;
                }
            }
        }
        "go" => {
            // import_declaration may directly contain import_spec or wrap a list
            collect_go_import_specs(node, content, &mut sources);
        }
        "rust" => {
            if let Ok(text) = node.utf8_text(content.as_bytes()) {
                let path = text
                    .trim_start_matches("pub ")
                    .trim_start_matches("use ")
                    .trim_end_matches(';')
                    .split('{')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches("::")
                    .to_string();
                if !path.is_empty() {
                    sources.push(path);
                }
            }
        }
        _ => {}
    }
    sources
}

/// Recursively collect Go import path strings from an `import_declaration` node.
fn collect_go_import_specs(node: Node, content: &str, out: &mut Vec<String>) {
    let kind = node.kind();
    if kind == "import_spec" {
        if let Some(path_node) = node.child_by_field_name("path") {
            if let Ok(text) = path_node.utf8_text(content.as_bytes()) {
                let clean = text.trim_matches('"');
                if !clean.is_empty() {
                    out.push(clean.to_string());
                }
            }
        }
    }
    let mut i = 0;
    while i < node.named_child_count() {
        if let Some(child) = node.named_child(i as u32) {
            collect_go_import_specs(child, content, out);
        }
        i += 1;
    }
}

/// Normalize a raw import source to a canonical external name.
/// For bare package names like `"requests"` or `"fmt"` this is the name itself;
/// for dotted paths like `"github.com/gin-gonic/gin"` it is the last segment.
fn external_name(source: &str) -> String {
    // Remove leading dots (Python relative) or slashes
    let clean = source.trim_start_matches('.').trim_start_matches('/');
    // Use last path segment as the canonical name (rsplit is more efficient on a DEI)
    clean
        .rsplit('/')
        .next()
        .unwrap_or(clean)
        .split('.')
        .next()
        .unwrap_or(clean)
        .to_string()
}

/// Try to resolve an import source string to a known project file path.
///
/// Returns `Some(rel_path)` if found in `known_files`, `None` if external.
fn resolve_import(
    source: &str,
    file_dir: &str,
    language: &str,
    known_files: &HashSet<String>,
) -> Option<String> {
    // Relative imports (starts with ./ or ../)
    if source.starts_with("./") || source.starts_with("../") {
        let joined = if file_dir.is_empty() {
            source.to_string()
        } else {
            format!("{}/{}", file_dir, source)
        };
        let base = normalize_path(&joined);

        match language {
            "typescript" | "javascript" => {
                for ext in &["ts", "tsx", "js", "jsx"] {
                    let candidate = format!("{}.{}", base, ext);
                    if known_files.contains(&candidate) {
                        return Some(candidate);
                    }
                }
                for ext in &["ts", "js"] {
                    let candidate = format!("{}/index.{}", base, ext);
                    if known_files.contains(&candidate) {
                        return Some(candidate);
                    }
                }
            }
            "python" => {
                let candidate = format!("{}.py", base);
                if known_files.contains(&candidate) {
                    return Some(candidate);
                }
            }
            _ => {}
        }
        return None;
    }

    // Rust crate-relative imports
    if language == "rust" && source.starts_with("crate::") {
        let module = source.trim_start_matches("crate::");
        let rel = module.replace("::", "/");
        let candidate = format!("src/{}.rs", rel);
        if known_files.contains(&candidate) {
            return Some(candidate);
        }
        let mod_candidate = format!("src/{}/mod.rs", rel);
        if known_files.contains(&mod_candidate) {
            return Some(mod_candidate);
        }
    }

    // Python relative-dot imports (e.g. "." prefix after strip)
    if language == "python" && (source.starts_with('.') || source.is_empty()) {
        // We can't resolve without knowing the package structure; treat as internal-unknown
        return None;
    }

    None
}

/// Collapse a path like `"a/b/../c"` into `"a/c"` without hitting the filesystem.
fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => { parts.pop(); }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Given the file's lines and a symbol's 0-indexed start row, return the row the
/// chunk should start at so it includes the symbol's leading doc-comment block.
///
/// Walks upward from the symbol, first tolerating blank lines directly above it
/// (a doc-comment separated from its symbol by a blank line still belongs to it),
/// then swallowing the contiguous run of comment lines above. If no comment line
/// is found, the original symbol row is returned unchanged (blank lines alone are
/// never pulled in). Blank-line-tolerant, language-agnostic — `is_comment_line`
/// recognizes `///`, `//`, `/*`/`*`, `#`, `"""`/`'''`, `<!--`, `--`.
fn extend_start_over_doc_comment(lines: &[&str], symbol_start_row: usize) -> usize {
    // Skip blank lines directly above the symbol.
    let mut j = symbol_start_row;
    while j > 0 && lines[j - 1].trim().is_empty() {
        j -= 1;
    }
    // Swallow the contiguous comment block above.
    let mut start = j;
    while start > 0 && is_comment_line(lines[start - 1].trim()) {
        start -= 1;
    }
    // Only extend when a comment was actually found; otherwise keep the symbol row
    // so we never absorb bare blank lines that carry no doc.
    if start < j {
        start
    } else {
        symbol_start_row
    }
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
    fn unsupported_language_java_and_shell_yield_chunks() {
        let chunker = TreeSitterChunker::default();
        // Java has no tree-sitter grammar wired in here → must fall back to line
        // windows and still produce searchable chunks (not an empty vec).
        let java = "public class OrderService {\n  public Order create(Cart c) { return new Order(c); }\n}\n";
        let jc = chunker.chunk("OrderService.java", "h", Some("java"), java);
        assert!(!jc.is_empty(), "a .java file must yield ≥1 chunk via fallback");

        // Same guarantee for a shell script.
        let sh = "#!/usr/bin/env bash\nset -euo pipefail\ndeploy() {\n  kubectl apply -f k8s/\n}\n";
        let sc = chunker.chunk("deploy.sh", "h", Some("shell"), sh);
        assert!(!sc.is_empty(), "a .sh file must yield ≥1 chunk via fallback");
    }

    #[test]
    fn typescript_extracts_code_symbols_and_edges() {
        let chunker = TreeSitterChunker::default();
        let src = "import { Order } from './order';\nimport express from 'express';\n\nexport interface Product {\n  id: string;\n  price: number;\n}\n\nexport class OrderService {\n  private orders: Order[] = [];\n  createOrder(p: Product): Order { return {} as Order; }\n  async cancelOrder(id: string): Promise<void> {}\n}\n\nexport function calculateTotal(products: Product[]): number {\n  return products.reduce((s, p) => s + p.price, 0);\n}\n\nexport type OrderId = string;\nexport enum Status { Pending, Shipped }\n";
        let known: std::collections::HashSet<String> =
            ["src/order.ts".to_string()].into_iter().collect();
        let (_chunks, fg) =
            chunker.chunk_with_graph("src/service.ts", "h", Some("typescript"), src, &known);
        let fg = fg.expect("TS file should produce a FileGraph");
        let types: Vec<&str> = fg.symbols.iter().map(|s| s.symbol_type.as_str()).collect();
        assert!(types.contains(&"Interface"), "TS interface extracted");
        assert!(types.contains(&"Class"), "TS class extracted");
        assert!(types.contains(&"Method"), "TS method extracted");
        assert!(types.contains(&"Function"), "TS function extracted");
        assert!(types.contains(&"Type"), "TS type alias extracted");
        assert!(types.contains(&"Enum"), "TS enum extracted");
        // defines edges from the File node to each top-level symbol
        let defines = fg
            .edges
            .iter()
            .filter(|e| e.edge_type.as_str() == "defines")
            .count();
        assert!(defines >= 4, "expected >=4 defines edges, got {defines}");
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
            markdown: MarkdownChunker::default(),
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

    #[test]
    fn ts_symbol_chunk_includes_preceding_doc_comment() {
        use crate::indexer::chunker::build_embed_text;
        let chunker = TreeSitterChunker::default();
        // A JSDoc block sits directly above the function. The tree-sitter node starts
        // at `export function`, so the chunk must be extended upward to capture it.
        let src = "import { db } from './db';\n\n/** Lists all users */\nexport function listUsers(): User[] {\n  return db.query('SELECT * FROM users');\n}\n";
        let chunks = chunker.chunk("api/users.ts", "h", Some("typescript"), src);
        let chunk = chunks
            .iter()
            .find(|c| c.symbol.as_deref() == Some("listUsers"))
            .expect("listUsers chunk must exist");
        assert!(
            chunk.content.contains("Lists all users"),
            "chunk content must include the doc-comment: {:?}",
            chunk.content
        );
        // The whole point: build_embed_text must now surface the doc text.
        let embed = build_embed_text(chunk.symbol.as_deref(), &chunk.content);
        assert!(
            embed.contains("Lists all users"),
            "build_embed_text must capture the doc-comment: {embed}"
        );
    }

    #[test]
    fn rust_symbol_chunk_includes_doc_comment_with_blank_line() {
        use crate::indexer::chunker::build_embed_text;
        let chunker = TreeSitterChunker::default();
        // A `///` doc block separated from the fn by a blank line still belongs to it.
        let src = "/// Authenticates the caller.\n\npub fn authenticate(token: &str) -> bool {\n    !token.is_empty()\n}\n";
        let chunks = chunker.chunk("src/auth.rs", "h", Some("rust"), src);
        let chunk = chunks
            .iter()
            .find(|c| c.symbol.as_deref() == Some("authenticate"))
            .expect("authenticate chunk must exist");
        assert_eq!(chunk.start_line, 1, "chunk must start at the doc-comment line");
        let embed = build_embed_text(chunk.symbol.as_deref(), &chunk.content);
        assert!(
            embed.contains("Authenticates the caller"),
            "doc-comment above a blank line must be captured: {embed}"
        );
    }

    // ── chunk_with_graph tests ────────────────────────────────────────────────

    fn graph_symbols(fg: &FileGraph) -> Vec<(String, &str)> {
        fg.symbols
            .iter()
            .map(|s| (s.name.clone(), s.symbol_type.as_str()))
            .collect()
    }

    fn graph_edges(fg: &FileGraph) -> Vec<(&str, String, String)> {
        fg.edges
            .iter()
            .map(|e| (e.edge_type.as_str(), e.from_qname.clone(), e.to_qname.clone()))
            .collect()
    }

    #[test]
    fn rust_struct_and_impl_produce_symbols_and_edges() {
        let chunker = TreeSitterChunker::default();
        let src = "struct Foo;\nimpl Foo {\n    fn bar(&self) {}\n}\n";
        let known = HashSet::new();
        let (chunks, graph_opt) = chunker.chunk_with_graph("src/foo.rs", "h", Some("rust"), src, &known);
        assert!(!chunks.is_empty(), "chunks must not be empty");
        let fg = graph_opt.expect("graph must be Some for supported language");
        let syms = graph_symbols(&fg);
        let names: Vec<&str> = syms.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"Foo"), "Struct Foo must be in symbols");
        assert!(names.contains(&"bar"), "Method bar must be in symbols");
        // defines_method edge from Foo to bar
        let edges = graph_edges(&fg);
        let has_defines_method = edges.iter().any(|(et, from, to)| {
            *et == "defines_method" && from.contains("Foo") && to.contains("bar")
        });
        assert!(has_defines_method, "defines_method edge from Foo to bar must exist");
        // defines edge from File to Foo (container)
        let has_defines_foo = edges.iter().any(|(et, from, to)| {
            *et == "defines" && from.starts_with("file::") && to.contains("Foo")
        });
        assert!(has_defines_foo, "defines edge from File to Foo must exist");
    }

    #[test]
    fn go_type_declaration_struct_vs_interface_vs_type() {
        let chunker = TreeSitterChunker::default();
        let src = "package main\n\ntype Dog struct { Name string }\n\ntype Sayer interface { Say() string }\n\ntype MyInt int\n";
        let known = HashSet::new();
        let (_chunks, graph_opt) = chunker.chunk_with_graph("main.go", "h", Some("go"), src, &known);
        let fg = graph_opt.expect("graph must be Some for Go");
        let syms = graph_symbols(&fg);
        let dog = syms.iter().find(|(n, _)| n == "Dog").map(|(_, t)| *t);
        let sayer = syms.iter().find(|(n, _)| n == "Sayer").map(|(_, t)| *t);
        let myint = syms.iter().find(|(n, _)| n == "MyInt").map(|(_, t)| *t);
        assert_eq!(dog, Some("Struct"), "Dog must be Struct");
        assert_eq!(sayer, Some("Interface"), "Sayer must be Interface");
        assert_eq!(myint, Some("Type"), "MyInt must be Type");
    }

    #[test]
    fn ts_relative_import_resolves_to_file_node() {
        let chunker = TreeSitterChunker::default();
        let src = "import { foo } from './utils';\nexport function main() {}\n";
        let mut known = HashSet::new();
        known.insert("src/utils.ts".to_string());
        let (_chunks, graph_opt) =
            chunker.chunk_with_graph("src/index.ts", "h", Some("typescript"), src, &known);
        let fg = graph_opt.expect("graph must be Some for TypeScript");
        let edges = graph_edges(&fg);
        let has_import_to_utils = edges.iter().any(|(et, _from, to)| {
            *et == "imports" && to == "file::src/utils.ts"
        });
        assert!(has_import_to_utils, "imports edge to file::src/utils.ts must exist; edges: {:?}", edges);
        // No external stub for './utils' when it resolves
        let ext_stub = fg.symbols.iter().any(|s| s.symbol_type == SymbolType::External);
        assert!(!ext_stub, "no External stub must be created for a resolved import");
    }

    #[test]
    fn python_bare_import_creates_external_stub() {
        let chunker = TreeSitterChunker::default();
        let src = "import requests\n\ndef fetch(url):\n    return requests.get(url)\n";
        let known = HashSet::new();
        let (_chunks, graph_opt) =
            chunker.chunk_with_graph("src/client.py", "h", Some("python"), src, &known);
        let fg = graph_opt.expect("graph must be Some for Python");
        let has_ext = fg
            .symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::External && s.name == "requests");
        assert!(has_ext, "External stub 'requests' must be created");
        let edges = graph_edges(&fg);
        let has_import_to_ext = edges.iter().any(|(et, _from, to)| {
            *et == "imports" && to == "external::requests"
        });
        assert!(has_import_to_ext, "imports edge to external::requests must exist");
    }

    #[test]
    fn unsupported_language_returns_none_graph() {
        let chunker = TreeSitterChunker::default();
        let src = "key: value\nlist:\n  - item\n";
        let known = HashSet::new();
        let (_chunks, graph_opt) =
            chunker.chunk_with_graph("config.yaml", "h", None, src, &known);
        assert!(graph_opt.is_none(), "unsupported language must produce None graph");
    }

    #[test]
    fn chunk_with_graph_existing_chunk_tests_still_pass() {
        // chunk_with_graph must produce the same chunks as chunk() for a simple Rust file.
        let chunker = TreeSitterChunker::default();
        let src = "fn alpha() {}\n\nfn beta(x: i32) -> i32 { x }\n";
        let known = HashSet::new();
        let (cwg_chunks, _) =
            chunker.chunk_with_graph("src/lib.rs", "h", Some("rust"), src, &known);
        let plain_chunks = chunker.chunk("src/lib.rs", "h", Some("rust"), src);
        assert_eq!(
            cwg_chunks.len(),
            plain_chunks.len(),
            "chunk_with_graph must produce the same chunk count as chunk()"
        );
    }
}
