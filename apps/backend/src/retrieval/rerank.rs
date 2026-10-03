//! Cross-encoder re-ranking of the head of a file ranking (factory F2).
//!
//! A cross-encoder reads the query and a candidate together, which a bi-encoder
//! cannot, at the cost of one model pass per candidate: only the first
//! [`RERANK_DEPTH`] files are re-scored, each as its path, best symbol and the
//! start of that chunk.

use super::FileHit;
use fastembed::{RerankInitOptions, RerankerModel, TextRerank};
use rusqlite::Connection;

/// Files re-scored per query; the rest keep their order after them.
pub const RERANK_DEPTH: usize = 20;
/// Chunk text shown to the model per candidate.
const SNIPPET_CHARS: usize = 1200;

pub struct Reranker {
    model: TextRerank,
}

impl Reranker {
    /// Loads a reranker by name: `bge-base` (BAAI/bge-reranker-base, MIT) or
    /// `jina-turbo` (jinaai/jina-reranker-v1-turbo-en, Apache-2.0).
    pub fn init(name: &str) -> anyhow::Result<Self> {
        let model = match name {
            "bge-base" => RerankerModel::BGERerankerBase,
            "jina-turbo" => RerankerModel::JINARerankerV1TurboEn,
            other => anyhow::bail!("unknown reranker {other}"),
        };
        let model = TextRerank::try_new(RerankInitOptions::new(model).with_max_length(512))?;
        Ok(Self { model })
    }

    /// `hits` with its first [`RERANK_DEPTH`] files re-ordered by the model.
    pub fn rerank_files(
        &self,
        conn: &Connection,
        code_project_id: i64,
        query: &str,
        hits: Vec<FileHit>,
    ) -> anyhow::Result<Vec<FileHit>> {
        let depth = hits.len().min(RERANK_DEPTH);
        if depth < 2 {
            return Ok(hits);
        }
        let mut hits = hits;
        let tail = hits.split_off(depth);
        let documents: Vec<String> = hits
            .iter()
            .map(|hit| candidate_text(conn, code_project_id, hit))
            .collect::<anyhow::Result<_>>()?;
        let scored = self.model.rerank(
            query,
            documents.iter().map(String::as_str).collect(),
            false,
            None,
        )?;
        let mut reranked: Vec<FileHit> = scored
            .into_iter()
            .map(|result| FileHit {
                score: result.score,
                ..hits[result.index].clone()
            })
            .collect();
        reranked.extend(tail);
        Ok(reranked)
    }
}

/// What the model reads for a file: its path, the symbol that matched, and the
/// beginning of that chunk.
fn candidate_text(
    conn: &Connection,
    code_project_id: i64,
    hit: &FileHit,
) -> anyhow::Result<String> {
    let content: Option<String> = conn
        .query_row(
            "SELECT content FROM code_chunks
             WHERE code_project_id = ?1 AND file_path = ?2 AND symbol IS ?3
             ORDER BY start_line LIMIT 1",
            rusqlite::params![code_project_id, hit.file_path, hit.top_symbol],
            |r| r.get(0),
        )
        .ok();
    let snippet: String = content
        .unwrap_or_default()
        .chars()
        .take(SNIPPET_CHARS)
        .collect();
    Ok(format!(
        "{}\n{}\n{snippet}",
        hit.file_path,
        hit.top_symbol.as_deref().unwrap_or_default()
    ))
}
