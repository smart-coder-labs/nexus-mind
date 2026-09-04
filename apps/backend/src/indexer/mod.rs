pub mod chunker;
pub mod doc_walker;
pub mod tree_sitter_chunker;
pub mod walker;

use anyhow::Result;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::{
    db::queries as db_queries,
    embed::{self, EmbedService},
    indexer::{
        chunker::Chunker,
        tree_sitter_chunker::{FileGraph, TreeSitterChunker},
        walker::walk_files,
    },
    models::types::{CodeProject, IndexProjectResponse},
};

/// Target combined on-disk size (bytes) of one Pass-1 batch (~8 MB). Bounds the
/// peak memory held while a batch's file contents + ASTs are live in parallel.
const BATCH_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Hard cap on files per Pass-1 batch, so a run of tiny files still yields
/// bounded-count batches (rayon parses the whole batch at once).
const BATCH_MAX_FILES: usize = 64;

/// Plan contiguous `[start, end)` batch ranges over `sizes`, cutting a batch as
/// soon as adding the next file would push its combined size over `max_bytes` or
/// its count to `max_files`. A single file whose size alone exceeds `max_bytes`
/// forms its own batch (files are never split). Order is preserved and every
/// index in `0..sizes.len()` is covered exactly once.
fn plan_batches(sizes: &[u64], max_bytes: u64, max_files: usize) -> Vec<(usize, usize)> {
    let max_files = max_files.max(1);
    let mut batches = Vec::new();
    let mut start = 0usize;
    let mut acc: u64 = 0;
    let mut i = 0usize;
    while i < sizes.len() {
        let batch_len = i - start;
        // Cut the current (non-empty) batch before adding a file that would
        // overflow the byte cap or reach the count cap.
        if batch_len > 0
            && (batch_len >= max_files || acc.saturating_add(sizes[i]) > max_bytes)
        {
            batches.push((start, i));
            start = i;
            acc = 0;
            continue;
        }
        acc = acc.saturating_add(sizes[i]);
        i += 1;
    }
    if start < sizes.len() {
        batches.push((start, sizes.len()));
    }
    batches
}

/// Orchestrates walking, chunking, embedding, and persisting a code project.
///
/// For each file:
///   1. Compute SHA-256 hash.
///   2. Check against stored hash — skip if unchanged.
///   3. If changed/new: delete old chunks, insert new chunks + embeddings.
///
/// Returns the indexing summary on success.
pub fn index_project(
    org_id: &str,
    project_name: &str,
    root_path: &str,
    db: &Arc<Mutex<Connection>>,
    embed_svc: Option<&Arc<EmbedService>>,
    graph_only: bool,
) -> Result<IndexProjectResponse> {
    let chunker = TreeSitterChunker::default();

    // Walk the directory
    let files = walk_files(root_path)?;

    // Get or create the code_project row and fetch stored file hashes
    let code_project_id = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::upsert_code_project(&conn, org_id, project_name, root_path)?
    };

    // Mark as indexing
    {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        let _ = db_queries::set_code_project_indexing(&conn, code_project_id);
    }

    let stored_hashes: HashMap<String, String> = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::list_indexed_files_with_hashes(&conn, code_project_id)?
    };

    // Fetch exclude_patterns for this project
    let exclude_patterns: Vec<String> = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::get_code_project(org_id, project_name, &conn)
            .unwrap_or(None)
            .map(|p| p.exclude_patterns)
            .unwrap_or_default()
    };

    // Build the set of all relative paths for structural graph nodes
    let known_files: HashSet<String> = files
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(root_path)
                .unwrap_or(&f.path)
                .trim_start_matches('/')
                .to_string()
        })
        .filter(|p| !p.is_empty())
        .collect();

    let all_rel_paths: Vec<String> = known_files.iter().cloned().collect();

    // Persist the structural nodes (Project, Folder, File) once per index run
    {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::persist_structure(&conn, code_project_id, project_name, &all_rel_paths)?;
    }

    let mut files_indexed = 0i64;
    // Changed files needing (re-)embedding in pass 2: (index into `files`, rel_path).
    let mut changed: Vec<(usize, String)> = Vec::new();

    // Files already fully indexed (have both graph symbols and stored source) are
    // skipped without re-parsing when unchanged. Loaded once (cheap) for the run.
    let (complete_graph, complete_source) = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        (
            db_queries::list_files_with_symbols(&conn, code_project_id).unwrap_or_default(),
            db_queries::list_files_with_source(&conn, code_project_id).unwrap_or_default(),
        )
    };

    // ── PASS 1: graph extraction for ALL files (fast — AST only, no embeddings) ──
    // Tree-sitter parsing is CPU-bound and embarrassingly parallel, so each batch of
    // files is parsed across cores (rayon); persistence stays serial (single SQLite
    // writer). Batching bounds peak memory to one batch of file contents at a time.
    struct Parsed {
        idx: usize,
        rel_path: String,
        content: String,
        hash: String,
        unchanged: bool,
        fg: Option<FileGraph>,
        has_chunks: bool,
        skip: bool,
    }

    // Batch by BYTES, not file count: a batch accumulates files until their combined
    // on-disk size reaches BATCH_MAX_BYTES OR the file count reaches BATCH_MAX_FILES,
    // whichever comes first. This bounds peak memory (full `content` Strings + ASTs of
    // one batch, parsed across all cores) regardless of individual file sizes — a run
    // of large files yields small batches, a run of tiny files yields count-capped ones.
    let sizes: Vec<u64> = files.iter().map(|f| f.size).collect();
    for (start, end) in plan_batches(&sizes, BATCH_MAX_BYTES, BATCH_MAX_FILES) {
        // Parallel: read + parse each file in the batch (no DB access here).
        let parsed: Vec<Parsed> = (start..end)
            .into_par_iter()
            .filter_map(|idx| {
                let file_meta = &files[idx];
                let rel_path = file_meta
                    .path
                    .strip_prefix(root_path)
                    .unwrap_or(&file_meta.path)
                    .trim_start_matches('/')
                    .to_string();
                if exclude_patterns.iter().any(|pat| rel_path.contains(pat.as_str())) {
                    return None;
                }
                let (content, hash) = walker::read_file(&file_meta.path)?;
                let unchanged = stored_hashes.get(&rel_path).map(|h| h == &hash).unwrap_or(false);
                // Already complete and unchanged → don't re-parse, just count later.
                if unchanged
                    && complete_graph.contains(&rel_path)
                    && complete_source.contains(&rel_path)
                {
                    return Some(Parsed {
                        idx, rel_path, content: String::new(), hash,
                        unchanged, fg: None, has_chunks: false, skip: true,
                    });
                }
                let (raw_chunks, fg) = chunker.chunk_with_graph(
                    &rel_path, &hash, file_meta.language.as_deref(), &content, &known_files,
                );
                Some(Parsed {
                    idx, rel_path, content, hash, unchanged, fg,
                    has_chunks: !raw_chunks.is_empty(), skip: false,
                })
            })
            .collect();

        // Serial: persist the batch (per-op lock keeps the health endpoint responsive).
        for p in parsed {
            if p.skip {
                // Unchanged + already complete: nothing to re-parse. Its chunks are
                // still in the table and counted project-wide after Pass 2.
                files_indexed += 1;
                continue;
            }
            if let Some(fg) = &p.fg {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                if let Err(e) = db_queries::persist_file_graph(&conn, code_project_id, fg) {
                    tracing::warn!("Failed to persist graph for {}: {e}", p.rel_path);
                }
            }
            {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                let _ = db_queries::upsert_code_file(
                    &conn, code_project_id, &p.rel_path, &p.content, &p.hash,
                );
            }
            files_indexed += 1;
            if !p.has_chunks {
                continue;
            }
            // Unchanged files keep their existing chunks; graph_only intentionally
            // skips (re-)embedding. Both are counted project-wide after Pass 2. Only
            // changed files in a full index are cleared and queued for re-embedding.
            if !p.unchanged && !graph_only {
                {
                    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                    db_queries::delete_chunks_for_file(&conn, code_project_id, &p.rel_path)?;
                }
                changed.push((p.idx, p.rel_path));
            }
        }
        // `parsed` (and every file `content` String it held) is dropped here at the end
        // of each iteration, before the next batch is read — so peak memory is bounded
        // to one batch's worth of source + ASTs at a time.
    }

    // ── PASS 2: embeddings for changed files (slow — powers semantic search) ──
    // Runs after the graph is complete, so the graph is available long before this
    // finishes. Re-reads each file's content on demand (one at a time — bounded
    // memory). Skipped entirely in `graph_only` mode for codebase-memory-style fast
    // indexing (structure/graph only, no semantic search).
    if !graph_only {
        for (idx, rel_path) in &changed {
            let file_meta = &files[*idx];
            let language = file_meta.language.as_deref();
            let (content, hash) = match walker::read_file(&file_meta.path) {
                Some(ch) => ch,
                None => continue,
            };
            let raw_chunks = chunker.chunk(rel_path, &hash, language, &content);
            if raw_chunks.is_empty() {
                continue;
            }

            let embeddings: Vec<Option<Vec<u8>>> = if let Some(svc) = embed_svc {
                // Embed a compact NL-friendly skeleton (symbol name + signature +
                // leading doc comment), NOT the raw body — this is what cosine ranks
                // against. `chunk.content` still stores the real body for get_context
                // / snippet retrieval; only the embedded-against text changes.
                let embed_texts: Vec<String> = raw_chunks
                    .iter()
                    .map(|c| chunker::build_embed_text(c.symbol.as_deref(), &c.content))
                    .collect();
                let texts: Vec<&str> = embed_texts.iter().map(|s| s.as_str()).collect();
                match svc.embed_documents(&texts) {
                    Ok(vecs) => vecs.into_iter().map(|v| Some(embed::serialize(&v))).collect(),
                    Err(e) => {
                        tracing::warn!("Failed to embed batch for {rel_path}: {e}");
                        raw_chunks.iter().map(|_| None).collect()
                    }
                }
            } else {
                raw_chunks.iter().map(|_| None).collect()
            };

            let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            for (chunk, embedding) in raw_chunks.iter().zip(embeddings.iter()) {
                db_queries::insert_code_chunk(
                    &conn,
                    code_project_id,
                    rel_path,
                    &chunk.file_hash,
                    chunk.language.as_deref(),
                    chunk.symbol.as_deref(),
                    chunk.start_line,
                    chunk.end_line,
                    &chunk.content,
                    embedding.as_deref(),
                )?;
            }
        }
    }

    // Authoritative chunk count: Pass 2 inserts freshly-embedded chunks without
    // touching `total_chunks` (only Pass-1 unchanged/graph_only files increment it),
    // so a fresh index would report 0. Read the real row count from the table.
    let total_chunks = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::count_chunks_for_project(&conn, code_project_id)?
    };

    // Update project stats and mark success
    let last_indexed = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::update_code_project_stats(
            &conn,
            code_project_id,
            files_indexed,
            total_chunks,
            &last_indexed,
        )?;
        let _ = db_queries::set_code_project_success(&conn, code_project_id, files_indexed, &last_indexed);
    }

    Ok(IndexProjectResponse {
        project: project_name.to_string(),
        status: "indexed".to_string(),
        file_count: files_indexed,
        chunk_count: total_chunks,
        last_indexed,
    })
}

/// Returns the current indexing status of a code project.
pub fn get_project_status(
    org_id: &str,
    project_name: &str,
    db: &Arc<Mutex<Connection>>,
) -> Result<Option<CodeProject>> {
    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
    db_queries::get_code_project(org_id, project_name, &conn)
}

/// Result of one documentation indexing pass.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct IndexDocsResponse {
    pub documents_scanned: usize,
    pub documents_changed: usize,
    pub chunks_written: usize,
    pub chunks_embedded: usize,
    pub chunks_pending_embedding: usize,
}

/// Indexes the documentation under `root_path` into the DOC corpus.
///
/// Runs beside [`index_project`] and shares nothing with it but the ignore
/// configuration. It writes only `doc_documents` / `doc_chunks` /
/// `doc_chunk_embeddings`, and never touches `code_chunks` — that separation is
/// the whole reason the corpus exists (see `indexer::doc_walker`).
///
/// Embedding is best-effort: a chunk with no vector is still searchable by
/// keyword, and `chunks_pending_embedding` reports how many are waiting.
pub fn index_documents(
    org_id: &str,
    client_id: Option<&str>,
    project_id: Option<&str>,
    root_path: &str,
    opts: &doc_walker::DocWalkOptions,
    db: &Arc<Mutex<Connection>>,
    embed_svc: Option<&Arc<EmbedService>>,
) -> Result<IndexDocsResponse> {
    let files = doc_walker::walk_docs(root_path, opts)?;
    let mut out = IndexDocsResponse {
        documents_scanned: files.len(),
        ..Default::default()
    };

    for file in &files {
        let Some((content, content_sha)) = walker::read_file(&file.path) else {
            continue;
        };
        // Store the path relative to the scan root: an absolute path would carry
        // the operator's home directory into a shared corpus.
        let rel = file
            .path
            .strip_prefix(root_path)
            .unwrap_or(&file.path)
            .trim_start_matches('/')
            .to_string();

        let chunk_ids = {
            let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            let (doc_id, changed) = crate::db::doc_queries::upsert_document(
                &conn,
                org_id,
                client_id,
                project_id,
                &rel,
                &content_sha,
            )?;
            if !changed {
                continue;
            }
            out.documents_changed += 1;
            crate::db::doc_queries::replace_chunks(&conn, &doc_id, &rel, &content_sha, &content)?
        };
        out.chunks_written += chunk_ids.len();

        // Embedding happens outside the lock above, one chunk at a time, for the
        // same reason the migration commit vectorizes after committing: it is
        // CPU-bound and must not hold a write lock across a batch.
        if let Some(svc) = embed_svc {
            for chunk_id in &chunk_ids {
                let text = {
                    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                    crate::db::doc_queries::get_chunk_content(&conn, chunk_id)?
                };
                let Some(text) = text else { continue };
                match svc.embed_document(&text) {
                    Ok(vector) => {
                        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                        if crate::db::doc_queries::store_chunk_embedding(&conn, chunk_id, &vector)
                            .is_ok()
                        {
                            out.chunks_embedded += 1;
                        }
                    }
                    Err(e) => tracing::warn!("doc index: failed to embed chunk {chunk_id}: {e}"),
                }
            }
        }
    }

    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
    let (_total, pending) = crate::db::doc_queries::index_status(&conn, org_id)?;
    out.chunks_pending_embedding = pending as usize;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    /// The byte-bounded batcher must never emit a batch whose combined size
    /// exceeds the cap (the sole exception being a single file larger than the
    /// cap, which forms its own batch), must respect the count cap, and must
    /// cover every file exactly once in order.
    #[test]
    fn plan_batches_never_exceeds_byte_cap() {
        let max_bytes = 8 * 1024 * 1024u64;
        let max_files = 64usize;
        // Mixed sizes: many small files, a few large ones near the 1 MB file cap.
        let mut sizes = Vec::new();
        for i in 0..500u64 {
            sizes.push(if i % 37 == 0 { 900 * 1024 } else { 2 * 1024 });
        }

        let batches = plan_batches(&sizes, max_bytes, max_files);

        // Full, ordered, non-overlapping coverage.
        assert_eq!(batches.first().unwrap().0, 0);
        assert_eq!(batches.last().unwrap().1, sizes.len());
        for w in batches.windows(2) {
            assert_eq!(w[0].1, w[1].0, "batches must be contiguous");
        }

        for (start, end) in &batches {
            let count = end - start;
            assert!(count >= 1, "no empty batches");
            assert!(count <= max_files, "count cap must hold: {count} > {max_files}");
            let total: u64 = sizes[*start..*end].iter().sum();
            // A batch may exceed the byte cap only if it is a single file.
            assert!(
                total <= max_bytes || count == 1,
                "batch [{start},{end}) sums to {total} bytes, over cap {max_bytes}"
            );
        }
    }

    /// A file larger than the byte cap still forms its own batch (never split).
    #[test]
    fn plan_batches_isolates_oversized_file() {
        let sizes = vec![1_000u64, 20 * 1024 * 1024, 1_000];
        let batches = plan_batches(&sizes, 8 * 1024 * 1024, 64);
        assert_eq!(batches, vec![(0, 1), (1, 2), (2, 3)]);
    }

    fn setup_indexer_db() -> Arc<Mutex<Connection>> {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        Arc::new(Mutex::new(conn))
    }

    /// End-to-end: docs (`.md`) are excluded from the CODE corpus while real
    /// source files (`.ts`) are indexed. `README.md`/`AGENTS.md` previously ranked
    /// at the top of code search; the walker's code-only allowlist now keeps them out.
    #[test]
    fn excludes_docs_from_code_corpus_indexes_source() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("README.md"),
            "# Getting Started\n\nInstall the CLI.\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src").join("users.ts"),
            "export function listUsers(): string[] {\n  return [];\n}\n",
        )
        .unwrap();

        let db = setup_indexer_db();
        let summary = index_project("org1", "myproj", dir.path().to_str().unwrap(), &db, None, false)
            .expect("index must succeed");
        assert_eq!(summary.file_count, 1, "only the .ts source file must be indexed");

        let conn = db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT file_path FROM code_chunks")
            .unwrap();
        let paths: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("users.ts")),
            "users.ts must be indexed into code_chunks, got: {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.ends_with("README.md")),
            "README.md must NOT enter the code corpus, got: {paths:?}"
        );
    }

    /// A fresh index (all files new → embedded in Pass 2) must report
    /// `chunk_count` equal to the actual number of rows in `code_chunks`, not 0.
    /// Regression: Pass 2 inserts chunks without incrementing the in-loop counter.
    #[test]
    fn fresh_index_reports_actual_chunk_count() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src").join("lib.rs"),
            "pub fn alpha() {}\n\npub fn beta() {}\n\npub fn gamma() {}\n",
        )
        .unwrap();

        let db = setup_indexer_db();
        let summary = index_project("org1", "myproj", dir.path().to_str().unwrap(), &db, None, false)
            .expect("index must succeed");

        let actual_rows: i64 = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM code_chunks", [], |r| r.get(0)).unwrap()
        };
        assert!(actual_rows > 0, "a fresh index must have inserted chunks");
        assert_eq!(
            summary.chunk_count, actual_rows,
            "reported chunk_count must equal actual code_chunks rows"
        );
    }

    // ── T-17: the two corpora must not merge ─────────────────────────────────

    use std::fs;
    use tempfile::TempDir;

    fn repo_with_code_and_docs() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("docs")).unwrap();
        fs::write(
            dir.path().join("src/handler.rs"),
            "pub fn handle_payment(amount: i64) -> i64 {\n    amount * 2\n}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/util.rs"),
            "pub fn normalize(s: &str) -> String {\n    s.trim().to_string()\n}\n",
        )
        .unwrap();
        // Prose that mentions the same words as the code — the exact material
        // that used to out-rank real handlers in code search.
        fs::write(
            dir.path().join("README.md"),
            "# Payments\n\nThis service handles payment normalization and handler routing.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("docs/ARCHITECTURE.md"),
            "# Architecture\n\n## Handlers\n\nEvery handler normalizes its payment input.\n",
        )
        .unwrap();
        dir
    }

    fn seeded_db() -> Arc<Mutex<Connection>> {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'U2S', 'u2s')",
            [],
        )
        .unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn code_chunk_fingerprint(db: &Arc<Mutex<Connection>>) -> Vec<(String, String)> {
        let conn = db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT file_path, content FROM code_chunks ORDER BY file_path, start_line")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    /// The regression this whole design exists to avoid: documentation entering
    /// the code corpus and out-ranking real handlers. Indexing docs must leave
    /// the code corpus byte-identical.
    #[test]
    fn code_search_results_unchanged_after_doc_indexing() {
        let dir = repo_with_code_and_docs();
        let root = dir.path().to_str().unwrap();
        let db = seeded_db();

        index_project("org1", "proj", root, &db, None, false).unwrap();
        let before = code_chunk_fingerprint(&db);
        assert!(!before.is_empty(), "the code corpus must have been populated");
        assert!(
            before.iter().all(|(p, _)| p.ends_with(".rs")),
            "only code files belong in the code corpus; got {:?}",
            before.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );

        index_documents(
            "org1",
            None,
            None,
            root,
            &doc_walker::DocWalkOptions::default(),
            &db,
            None,
        )
        .unwrap();

        let after = code_chunk_fingerprint(&db);
        assert_eq!(
            before, after,
            "indexing documentation must not change the code corpus by a single byte"
        );
    }

    #[test]
    fn doc_indexing_populates_only_the_doc_corpus() {
        let dir = repo_with_code_and_docs();
        let root = dir.path().to_str().unwrap();
        let db = seeded_db();

        let resp = index_documents(
            "org1",
            None,
            None,
            root,
            &doc_walker::DocWalkOptions::default(),
            &db,
            None,
        )
        .unwrap();
        assert_eq!(resp.documents_scanned, 2, "README.md and docs/ARCHITECTURE.md");
        assert_eq!(resp.documents_changed, 2);
        assert!(resp.chunks_written >= 2);

        let conn = db.lock().unwrap();
        let code_chunks: i64 = conn
            .query_row("SELECT COUNT(*) FROM code_chunks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(code_chunks, 0, "the doc pass must not write to code_chunks");

        // And the doc corpus answers for prose the code corpus never held.
        let hits = crate::db::doc_queries::search_docs_keyword(&conn, "org1", "normaliz", 10).unwrap();
        assert!(!hits.is_empty(), "documentation search must find the prose");
        assert!(hits.iter().all(|h| h.path.ends_with(".md")));
    }

    /// Paths are stored relative to the scan root. An absolute path would carry
    /// the operator's home directory into a corpus the whole org can read.
    #[test]
    fn doc_paths_are_stored_relative_to_the_scan_root() {
        let dir = repo_with_code_and_docs();
        let root = dir.path().to_str().unwrap();
        let db = seeded_db();
        index_documents("org1", None, None, root, &doc_walker::DocWalkOptions::default(), &db, None)
            .unwrap();

        let conn = db.lock().unwrap();
        let paths: Vec<String> = conn
            .prepare("SELECT path FROM doc_documents ORDER BY path")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(paths, vec!["README.md", "docs/ARCHITECTURE.md"]);
        assert!(
            paths.iter().all(|p| !p.starts_with('/')),
            "no absolute paths may reach the shared corpus"
        );
    }

    #[test]
    fn rescanning_unchanged_documentation_is_a_no_op() {
        let dir = repo_with_code_and_docs();
        let root = dir.path().to_str().unwrap();
        let db = seeded_db();
        let opts = doc_walker::DocWalkOptions::default();

        let first = index_documents("org1", None, None, root, &opts, &db, None).unwrap();
        assert_eq!(first.documents_changed, 2);

        let second = index_documents("org1", None, None, root, &opts, &db, None).unwrap();
        assert_eq!(second.documents_scanned, 2);
        assert_eq!(second.documents_changed, 0, "unchanged files are not re-chunked");
        assert_eq!(second.chunks_written, 0);
    }
}
