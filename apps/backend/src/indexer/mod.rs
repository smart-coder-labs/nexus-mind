pub mod chunker;
pub mod tree_sitter_chunker;
pub mod walker;

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::{
    db::queries as db_queries,
    embed::{self, EmbedService},
    indexer::{
        chunker::Chunker,
        tree_sitter_chunker::TreeSitterChunker,
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

    // ── PASS 1: graph extraction for ALL files (fast — AST only, no embeddings) ──
    // The structural + symbol graph is built from a single tree-sitter parse per
    // file and is queryable as soon as this pass finishes, independent of the slow
    // embedding pass below.
    for (idx, file_meta) in files.iter().enumerate() {
        let rel_path = file_meta.path
            .strip_prefix(root_path)
            .unwrap_or(&file_meta.path)
            .trim_start_matches('/')
            .to_string();

        if exclude_patterns.iter().any(|pat| rel_path.contains(pat.as_str())) {
            continue;
        }

        let language = file_meta.language.as_deref();
        let unchanged = stored_hashes
            .get(&rel_path)
            .map(|h| h == &file_meta.hash)
            .unwrap_or(false);

        if unchanged {
            // Backfill graph only when missing (projects indexed before the graph
            // feature have chunks but no code symbols). Avoids re-parsing in steady state.
            let needs_graph_backfill = {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                !db_queries::file_has_graph_symbols(&conn, code_project_id, &rel_path)
                    .unwrap_or(false)
            };
            if needs_graph_backfill {
                let (_chunks, file_graph) = chunker
                    .chunk_with_graph(&rel_path, &file_meta.hash, language, &file_meta.content, &known_files);
                if let Some(fg) = file_graph {
                    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                    if let Err(e) = db_queries::persist_file_graph(&conn, code_project_id, &fg) {
                        tracing::warn!("Failed to backfill graph for {rel_path}: {e}");
                    }
                }
            }
            let existing_count = {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                db_queries::count_chunks_for_file(&conn, code_project_id, &rel_path)?
            };
            total_chunks += existing_count;
            files_indexed += 1;
            continue;
        }

        // Changed file: extract + persist graph now; defer embedding to pass 2.
        let (raw_chunks, file_graph) =
            chunker.chunk_with_graph(&rel_path, &file_meta.hash, language, &file_meta.content, &known_files);
        if let Some(fg) = file_graph {
            let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            if let Err(e) = db_queries::persist_file_graph(&conn, code_project_id, &fg) {
                tracing::warn!("Failed to persist graph for {rel_path}: {e}");
            }
        }
        if raw_chunks.is_empty() {
            continue;
        }
        {
            let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            db_queries::delete_chunks_for_file(&conn, code_project_id, &rel_path)?;
        }
        total_chunks += raw_chunks.len() as i64;
        files_indexed += 1;
        changed.push((idx, rel_path));
    }

    // ── PASS 2: embeddings for changed files (slow — powers semantic search) ──
    // Runs after the graph is complete, so the graph is available long before this
    // finishes. Re-chunks from the already-in-memory file content (no extra reads).
    for (idx, rel_path) in &changed {
        let file_meta = &files[*idx];
        let language = file_meta.language.as_deref();
        let raw_chunks = chunker.chunk(rel_path, &file_meta.hash, language, &file_meta.content);
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
