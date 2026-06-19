pub mod chunker;
pub mod walker;

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::{
    db::queries as db_queries,
    embed::{self, EmbedService},
    indexer::{
        chunker::{LineWindowChunker, Chunker},
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
    let chunker = LineWindowChunker::default();

    // Walk the directory
    let files = walk_files(root_path)?;

    // Get or create the code_project row and fetch stored file hashes
    let code_project_id = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::upsert_code_project(&conn, org_id, project_name, root_path)?
    };

    let stored_hashes: HashMap<String, String> = {
        let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        db_queries::list_indexed_files_with_hashes(&conn, code_project_id)?
    };

    let mut total_chunks = 0i64;
    let mut files_indexed = 0i64;

    for file_meta in &files {
        // Determine relative path for storage (strip root prefix if possible)
        let rel_path = file_meta.path
            .strip_prefix(root_path)
            .unwrap_or(&file_meta.path)
            .trim_start_matches('/')
            .to_string();

        let stored_hash = stored_hashes.get(&rel_path);

        if stored_hash.map(|h| h == &file_meta.hash).unwrap_or(false) {
            // File unchanged — count its existing chunks
            let existing_count = {
                let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                db_queries::count_chunks_for_file(&conn, code_project_id, &rel_path)?
            };
            total_chunks += existing_count;
            files_indexed += 1;
            continue;
        }

        // Delete stale chunks for this file
        {
            let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            db_queries::delete_chunks_for_file(&conn, code_project_id, &rel_path)?;
        }

        // Chunk the file
        let language = file_meta.language.as_deref();
        let raw_chunks = chunker.chunk(&rel_path, &file_meta.hash, language, &file_meta.content);
        if raw_chunks.is_empty() {
            continue;
        }

        // Embed all chunks (best-effort — no embedding BLOB if service is unavailable)
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

        // Persist chunks
        {
            let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            for (chunk, embedding) in raw_chunks.iter().zip(embeddings.iter()) {
                db_queries::insert_code_chunk(
                    &conn,
                    code_project_id,
                    &rel_path,
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

        total_chunks += raw_chunks.len() as i64;
        files_indexed += 1;
    }

    // Update project stats
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
