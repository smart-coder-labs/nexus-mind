pub mod chunker;
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

    let mut total_chunks = 0i64;
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

    const BATCH: usize = 256;
    let mut start = 0usize;
    while start < files.len() {
        let end = (start + BATCH).min(files.len());

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
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                total_chunks += db_queries::count_chunks_for_file(&conn, code_project_id, &p.rel_path)?;
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
            if p.unchanged || graph_only {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                total_chunks += db_queries::count_chunks_for_file(&conn, code_project_id, &p.rel_path)?;
            } else {
                {
                    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                    db_queries::delete_chunks_for_file(&conn, code_project_id, &p.rel_path)?;
                }
                changed.push((p.idx, p.rel_path));
            }
        }

        start = end;
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
                let texts: Vec<&str> = raw_chunks.iter().map(|c| c.content.as_str()).collect();
                match svc.embed_batch(&texts) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    /// End-to-end: a Markdown file in the project tree is indexed into the
    /// searchable `code_chunks` content path, split by heading section with the
    /// heading as its symbol. Runs without an embedding service — chunks are
    /// still persisted (with NULL embeddings), which is what makes them findable.
    #[test]
    fn indexes_markdown_file_into_searchable_chunks() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(
            dir.path().join("docs").join("guide.md"),
            "# Getting Started\n\nInstall the CLI.\n\n## Authentication\n\nUse an API key.\n",
        )
        .unwrap();

        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        let db = Arc::new(Mutex::new(conn));

        let summary = index_project(
            "org1",
            "myproj",
            dir.path().to_str().unwrap(),
            &db,
            None,
            false,
        )
        .expect("index must succeed");
        assert!(summary.file_count >= 1, "markdown file must be counted as indexed");

        // The markdown content must land in code_chunks with heading symbols.
        let conn = db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT file_path, symbol, language FROM code_chunks")
            .unwrap();
        let rows: Vec<(String, Option<String>, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(
            rows.iter().any(|(path, _, _)| path.ends_with("guide.md")),
            "guide.md must be indexed into code_chunks, got: {rows:?}"
        );
        assert!(
            rows.iter().any(|(_, sym, _)| sym.as_deref() == Some("Getting Started")),
            "an H1 heading must become a chunk symbol, got: {rows:?}"
        );
        assert!(
            rows.iter().any(|(_, sym, _)| sym.as_deref() == Some("Authentication")),
            "an H2 heading must become a chunk symbol, got: {rows:?}"
        );
        assert!(
            rows.iter()
                .filter(|(path, _, _)| path.ends_with("guide.md"))
                .all(|(_, _, lang)| lang.as_deref() == Some("markdown")),
            "markdown chunks must be tagged with the markdown language, got: {rows:?}"
        );
    }
}
