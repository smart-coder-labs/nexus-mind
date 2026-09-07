pub mod chunker;
pub mod doc_walker;
pub mod tree_sitter_chunker;
pub mod walker;

use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashSet;
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

/// Whether a file can be skipped without re-reading or re-parsing it.
///
/// Both terms are recorded facts — the hash at which each pass last completed for
/// this file — not inferences from the absence of rows elsewhere. That distinction
/// is the whole point: "produces no symbols" and "was never parsed" look identical
/// when completeness is read off `code_symbols`, and a stylesheet or a re-export
/// barrel produces no symbols by nature. Those files never counted as complete and
/// were re-read, re-parsed and re-stored on every run — 3,012 of 5,845 files in
/// the production index.
///
/// `graph_only` promises structure without semantic search, so it asks only about
/// Pass 1. A full run additionally requires that Pass 2 settled at this same
/// content and that no chunk is missing its vector.
fn can_skip_file(graph_only: bool, graphed_ok: bool, chunked_ok: bool, unembedded: bool) -> bool {
    if graph_only {
        graphed_ok
    } else {
        graphed_ok && chunked_ok && !unembedded
    }
}

/// Whether a parsed file must be cleared and queued for Pass 2 (chunk + embed).
///
/// `unembedded` has to count here as well, not only at the skip above. Marking a
/// file incomplete gets it re-parsed, but a test that looked only at whether the
/// content changed would then drop it before Pass 2 — re-reading the file and
/// repairing nothing.
fn needs_embedding_pass(graph_only: bool, chunked_ok: bool, unembedded: bool) -> bool {
    !graph_only && (!chunked_ok || unembedded)
}

/// Orchestrates walking, chunking, embedding, and persisting a code project.
///
/// For each file:
///   1. Compute SHA-256 hash.
///   2. Check against stored hash — skip if unchanged AND already complete.
///      "Complete" means graph symbols, stored source, and — when this run can
///      embed — no chunk left without a vector.
///   3. If changed/new/unembedded: delete old chunks, insert new chunks + embeddings.
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

    // How far each file got last time: the hash at which Pass 1 and Pass 2 last
    // completed for it. Loaded once (cheap) for the run.
    let indexed_state = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::list_indexed_state(&conn, code_project_id).unwrap_or_default()
    };

    // Files carrying chunks with no vector. Graph symbols and stored source are
    // both present for such a file, so the two checks above call it complete and
    // it is skipped on every run — its chunks keep their NULL embeddings and
    // search silently falls back to keyword. A `reindex` does not repair it
    // either, because reindex takes this same incremental path.
    //
    // Only consulted when this run can actually fill them in: in `graph_only`, or
    // with no embedder, re-parsing these files would cost time and repair nothing.
    let unembedded_files: HashSet<String> = if graph_only || embed_svc.is_none() {
        HashSet::new()
    } else {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::list_files_with_unembedded_chunks(&conn, code_project_id).unwrap_or_default()
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
        /// Pass 2 already settled this file at exactly this content.
        chunked_ok: bool,
        /// Holds chunks with no vector — must still reach Pass 2, which
        /// `chunked_ok` alone would prevent.
        unembedded: bool,
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
                let state = indexed_state.get(&rel_path);
                let graphed_ok = state.and_then(|s| s.graphed.as_deref()) == Some(hash.as_str());
                let chunked_ok = state.and_then(|s| s.chunked.as_deref()) == Some(hash.as_str());
                let unembedded = unembedded_files.contains(&rel_path);
                // Both passes already settled at this exact content → don't re-parse.
                if can_skip_file(graph_only, graphed_ok, chunked_ok, unembedded) {
                    return Some(Parsed {
                        idx, rel_path, content: String::new(), hash,
                        chunked_ok, unembedded, fg: None, has_chunks: false, skip: true,
                    });
                }
                let (raw_chunks, fg) = chunker.chunk_with_graph(
                    &rel_path, &hash, file_meta.language.as_deref(), &content, &known_files,
                );
                Some(Parsed {
                    idx, rel_path, content, hash, chunked_ok, unembedded, fg,
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
            // Whether the graph actually landed. Under the old scheme a failed
            // persist left the file out of `list_files_with_symbols`, so the next
            // run retried it. Now the stamp is the memory, so stamping after a
            // failure would turn a transient SQLITE_BUSY into symbols and edges
            // that are lost until the file's content happens to change.
            let mut graph_persisted = true;
            if let Some(fg) = &p.fg {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                if let Err(e) = db_queries::persist_file_graph(&conn, code_project_id, fg) {
                    tracing::warn!("Failed to persist graph for {}: {e}", p.rel_path);
                    graph_persisted = false;
                }
            }
            {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                // The stored source matters as much as the graph: stamping after a
                // failed upsert would leave `graphed_hash` at the new content while
                // `code_files.content` still holds the old, and the file would be
                // skipped forever serving stale source.
                let source_stored = db_queries::upsert_code_file(
                    &conn, code_project_id, &p.rel_path, &p.content, &p.hash,
                )
                .is_ok();
                // Pass 1 is done for this file at this content. Recorded rather than
                // inferred later from whether the file happened to yield symbols.
                if graph_persisted && source_stored {
                    let _ =
                        db_queries::set_file_graphed(&conn, code_project_id, &p.rel_path, &p.hash);
                }
            }
            files_indexed += 1;
            if !p.has_chunks {
                // Yielding no chunks is a settled outcome, not an unfinished one.
                // Stamping it is what stops this file being re-read and re-parsed on
                // every future run — the 51.5% of the index that used to be.
                // `graph_only` must not stamp it: Pass 2 never ran.
                if !graph_only {
                    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                    // Delete first. A file that produced chunks before and produces
                    // none now — its content changed, or the chunker did — would
                    // otherwise keep the old ones AND be stamped complete, so search
                    // would serve deleted code forever and `reindex` could not repair
                    // it. Worse, if those orphans carry NULL vectors the `unembedded`
                    // check keeps re-parsing the file on every run without ever
                    // fixing it. Clearing the stamp before the delete keeps the
                    // window crash-safe, exactly as in the re-chunk path.
                    db_queries::clear_file_chunked(&conn, code_project_id, &p.rel_path)?;
                    db_queries::delete_chunks_for_file(&conn, code_project_id, &p.rel_path)?;
                    let _ =
                        db_queries::set_file_chunked(&conn, code_project_id, &p.rel_path, &p.hash);
                }
                continue;
            }
            // graph_only intentionally skips (re-)embedding; its files are counted
            // project-wide after Pass 2.
            //
            // `unembedded` is the exception that makes the repair actually happen: a
            // file whose chunks have no vector must be cleared and re-queued even
            // though its content did not change, or it would be re-parsed here and
            // then dropped before Pass 2 — repairing nothing.
            if needs_embedding_pass(graph_only, p.chunked_ok, p.unembedded) {
                {
                    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                    // Clear the stamp in the SAME critical section as the delete.
                    // Between here and Pass 2 the file has no chunks; if it still
                    // claimed a valid `chunked_hash` and the run were cut short —
                    // unreadable file, insert error, process killed — the next run
                    // would skip it as settled and it would stay out of semantic
                    // search forever, with `reindex` unable to repair it. Clearing
                    // first makes the window crash-safe by construction.
                    db_queries::clear_file_chunked(&conn, code_project_id, &p.rel_path)?;
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
                // Pass 1 saw chunks here and this chunker does not. Either way the
                // pass has now run for this file, so stamp it instead of leaving it
                // pending and re-read on every future run.
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                let _ = db_queries::set_file_chunked(&conn, code_project_id, rel_path, &hash);
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
            // Stamped only once the chunks are in. If embedding failed the vectors
            // are NULL and the `unembedded` half picks the file up on the next run —
            // the two conditions stay independent on purpose.
            let _ = db_queries::set_file_chunked(&conn, code_project_id, rel_path, &hash);
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

    /// The measured defect: after the embedder was off for a run, every chunk in
    /// the index carried a NULL vector — 16,649 of 16,649 in the August backup,
    /// across 5,450 of 5,845 files. Both passes had completed at that content, so
    /// a completeness test that looked only at the passes skipped them forever.
    #[test]
    fn a_settled_file_with_unembedded_chunks_is_not_skipped() {
        assert!(
            !can_skip_file(false, true, true, true),
            "a file missing vectors must be re-parsed even though both passes ran"
        );
    }

    /// Marking it incomplete only gets it re-parsed. It also has to survive the
    /// Pass-2 gate, which would otherwise drop anything already chunked — so the
    /// repair read the file and then threw the work away.
    #[test]
    fn a_settled_file_with_unembedded_chunks_reaches_the_embedding_pass() {
        assert!(
            needs_embedding_pass(false, true, true),
            "re-parsing without re-embedding repairs nothing"
        );
    }

    /// The ordinary incremental case stays cheap: both passes settled at this
    /// content and no vector is missing, so the file is skipped and kept out of
    /// Pass 2.
    #[test]
    fn a_fully_settled_file_is_skipped() {
        assert!(can_skip_file(false, true, true, false));
        assert!(!needs_embedding_pass(false, true, false));
    }

    /// O-2c, the point of the two columns. A file that yields no chunks — a
    /// stylesheet, a re-export barrel, a markdown page — is settled once Pass 2 has
    /// stamped it, and must skip like anything else. Under the old scheme it never
    /// counted as complete, because completeness was read off the absence of rows:
    /// 3,012 of 5,845 files re-read and re-parsed on every single run.
    #[test]
    fn a_file_that_yields_no_chunks_is_skippable_once_stamped() {
        // Stamped by both passes at this content, no chunks and so nothing
        // unembedded — indistinguishable, correctly, from a file full of symbols.
        assert!(can_skip_file(false, true, true, false));
        // Before Pass 2 stamped it, it is not skippable — exactly once.
        assert!(!can_skip_file(false, true, false, false));
    }

    /// Either pass being stale forces a re-parse.
    #[test]
    fn a_stale_pass_always_forces_reprocessing() {
        assert!(!can_skip_file(false, false, true, false), "Pass 1 stale");
        assert!(!can_skip_file(false, true, false, false), "Pass 2 stale");
        assert!(needs_embedding_pass(false, false, false), "stale Pass 2 reaches the embed pass");
    }

    /// `graph_only` promises structure without semantic search, so it asks only
    /// about Pass 1 — and must never be dragged into the embedding pass, not even
    /// to repair a vector it has no way to produce.
    #[test]
    fn graph_only_asks_only_about_the_graph_pass() {
        assert!(can_skip_file(true, true, false, true), "Pass 2 state is not its business");
        assert!(!can_skip_file(true, false, true, false), "but a stale graph is");
        assert!(!needs_embedding_pass(true, false, true));
        assert!(!needs_embedding_pass(true, true, true));
    }

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

    /// `graph_only` must never leave a project stranded without chunks.
    ///
    /// It stores graph symbols and source but skips Pass 2 by design, so the
    /// project ends up with zero chunks. The danger is a later full run reading
    /// that as "complete" and skipping every file — the project would keep working
    /// and simply never have semantic search, silently.
    ///
    /// What prevents it is that `graph_only` stamps `graphed_hash` and deliberately
    /// does NOT stamp `chunked_hash`, so the next full run sees Pass 2 as never
    /// having run. This test is the guard on that asymmetry.
    #[test]
    fn a_graph_only_project_still_gets_chunks_on_the_next_full_index() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src").join("lib.rs"),
            "pub fn alpha() {}\n\npub fn beta() {}\n",
        )
        .unwrap();
        let root = dir.path().to_str().unwrap();
        let db = setup_indexer_db();

        index_project("org1", "myproj", root, &db, None, true).expect("graph-only index");
        let after_graph_only: i64 = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM code_chunks", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(after_graph_only, 0, "graph_only must not chunk — that is its contract");

        // Nothing on disk changed, and both completeness checks pass.
        index_project("org1", "myproj", root, &db, None, false).expect("full index");
        let after_full: i64 = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM code_chunks", [], |r| r.get(0)).unwrap()
        };
        assert!(
            after_full > 0,
            "a graph-only project must still be chunked by the next full run"
        );
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

    /// O-2c end to end, with the probe that actually detects it: poison the stored
    /// source between two identical runs. A skipped file keeps the sentinel; a
    /// reprocessed one has it overwritten by `upsert_code_file`.
    ///
    /// The files that matter are the ones that yield nothing — a stylesheet and a
    /// re-export barrel produce no file-owned symbols, so under the old scheme they
    /// were never "complete" and were re-read and re-parsed on every single run:
    /// 3,012 of 5,845 files in the production index, 1,107 of them `.scss`.
    #[test]
    fn a_second_identical_run_reprocesses_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("styles.scss"), ".a { color: red; }\n").unwrap();
        std::fs::write(dir.path().join("src").join("index.tsx"), "export * from './lib';\n").unwrap();
        std::fs::write(dir.path().join("src").join("lib.tsx"), "export function alpha() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();
        let db = setup_indexer_db();

        index_project("org1", "myproj", root, &db, None, false).expect("first index");

        let paths: Vec<String> = {
            let conn = db.lock().unwrap();
            let mut st = conn.prepare("SELECT file_path FROM code_files ORDER BY file_path").unwrap();
            let v = st.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
            v
        };
        assert!(paths.len() >= 3, "the fixture must reach code_files, got {paths:?}");

        {
            let conn = db.lock().unwrap();
            conn.execute("UPDATE code_files SET content = 'POISON'", []).unwrap();
        }
        index_project("org1", "myproj", root, &db, None, false).expect("second index");

        let conn = db.lock().unwrap();
        for path in &paths {
            let content: String = conn
                .query_row("SELECT content FROM code_files WHERE file_path = ?1", [path], |r| r.get(0))
                .unwrap();
            assert_eq!(
                content, "POISON",
                "{path} was re-read and re-stored although nothing changed"
            );
        }
    }

    /// The regression the fix-review caught: a file that produced chunks and then
    /// stops producing any — content changed, or the chunker changed — must not
    /// keep the old ones. Stamping without deleting would serve deleted code as
    /// current forever, and `reindex` could not repair it because it takes this
    /// same incremental path.
    #[test]
    fn a_file_that_stops_producing_chunks_loses_its_old_ones() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let file = dir.path().join("src").join("lib.tsx");
        std::fs::write(&file, "export function alpha() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();
        let db = setup_indexer_db();

        index_project("org1", "myproj", root, &db, None, false).expect("first index");
        let before: i64 = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM code_chunks WHERE file_path='src/lib.tsx'", [], |r| r.get(0)).unwrap()
        };
        assert!(before > 0, "the fixture must produce chunks first");

        // Now it yields nothing at all — an empty file produces no chunks.
        std::fs::write(&file, "").unwrap();
        index_project("org1", "myproj", root, &db, None, false).expect("second index");

        let after: i64 = {
            let conn = db.lock().unwrap();
            conn.query_row("SELECT COUNT(*) FROM code_chunks WHERE file_path='src/lib.tsx'", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(after, 0, "stale chunks must not survive as current search results");
    }

    /// The other half of the contract: a file whose content DID change must be
    /// reprocessed. Skipping cheaply is only worth anything if it never skips work
    /// that matters.
    #[test]
    fn a_changed_file_is_still_reprocessed() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let file = dir.path().join("src").join("lib.tsx");
        std::fs::write(&file, "export function alpha() {}\n").unwrap();
        let root = dir.path().to_str().unwrap();
        let db = setup_indexer_db();

        index_project("org1", "myproj", root, &db, None, false).expect("first index");
        std::fs::write(&file, "export function alpha() {}\nexport function beta() {}\n").unwrap();
        index_project("org1", "myproj", root, &db, None, false).expect("second index");

        let conn = db.lock().unwrap();
        let content: String = conn
            .query_row("SELECT content FROM code_files WHERE file_path = 'src/lib.tsx'", [], |r| r.get(0))
            .unwrap();
        assert!(content.contains("beta"), "the new content must have been stored");
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
