//! Documentation corpus: documents, their chunks, and the vectors over them.
//!
//! A corpus separate from `code_chunks` on purpose — see `indexer::doc_walker`
//! for why. The chunking itself is done by [`crate::indexer::chunker::MarkdownChunker`],
//! which has existed and been tested since the code-search work and until now
//! had no caller.
//!
//! # Indexing is not the same act as migrating
//!
//! A document lands here whether or not the candidates derived from it are ever
//! approved. Being able to *find* a document and accepting a claim as team
//! knowledge are different things, and conflating them would mean a reviewer's
//! "no" also erases the ability to look the source up again.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::embed;
use crate::indexer::chunker::{Chunker, MarkdownChunker};

#[derive(Debug, Clone, PartialEq)]
pub struct DocChunkHit {
    pub chunk_id: String,
    pub document_id: String,
    pub path: String,
    pub heading_path: String,
    pub anchor: String,
    pub content: String,
    pub score: f32,
}

/// Inserts or refreshes a document and returns `(document_id, changed)`.
///
/// `changed` is false when the content hash is unchanged, which lets a caller
/// skip re-chunking a file that has not moved — the common case on a rescan.
pub fn upsert_document(
    conn: &Connection,
    org_id: &str,
    client_id: Option<&str>,
    project_id: Option<&str>,
    path: &str,
    content_sha: &str,
) -> Result<(String, bool)> {
    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT id, content_sha FROM doc_documents
              WHERE org_id = ?1 AND project_id IS ?2 AND path = ?3",
            rusqlite::params![org_id, project_id, path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    if let Some((id, existing_sha)) = existing {
        if existing_sha == content_sha {
            return Ok((id, false));
        }
        conn.execute(
            "UPDATE doc_documents SET content_sha = ?2, scanned_at = datetime('now') WHERE id = ?1",
            rusqlite::params![id, content_sha],
        )?;
        return Ok((id, true));
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO doc_documents (id, org_id, client_id, project_id, path, content_sha)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, org_id, client_id, project_id, path, content_sha],
    )?;
    Ok((id, true))
}

/// Replaces a document's chunks wholesale.
///
/// Deleting first rather than diffing is deliberate: a heading rename shifts
/// every anchor after it, so an incremental update would leave orphans that
/// still answer searches with text no longer in the file. `ON DELETE CASCADE`
/// takes the stale embeddings with them.
pub fn replace_chunks(
    conn: &Connection,
    document_id: &str,
    path: &str,
    content_sha: &str,
    content: &str,
) -> Result<Vec<String>> {
    conn.execute("DELETE FROM doc_chunks WHERE document_id = ?1", [document_id])?;

    let chunker = MarkdownChunker::default();
    let raw = chunker.chunk(path, content_sha, Some("markdown"), content);

    let mut ids = Vec::with_capacity(raw.len());
    for (ordinal, chunk) in raw.iter().enumerate() {
        let id = Uuid::new_v4().to_string();
        // `symbol` is the section heading the chunker extracted; an unheaded
        // preamble legitimately has none.
        let heading_path = chunk.symbol.clone().unwrap_or_default();
        let anchor = slugify(&heading_path, chunk.start_line);
        conn.execute(
            "INSERT INTO doc_chunks (id, document_id, heading_path, anchor, ordinal, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, document_id, heading_path, anchor, ordinal as i64, chunk.content],
        )?;
        ids.push(id);
    }
    Ok(ids)
}

/// A stable, human-recognizable anchor. The start line is part of it so two
/// sections sharing a heading — which happens in long documents — do not
/// collide on `UNIQUE(document_id, anchor, ordinal)`.
fn slugify(heading: &str, start_line: i64) -> String {
    let base: String = heading
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = base.trim_matches('-').replace("--", "-");
    if trimmed.is_empty() {
        format!("l{start_line}")
    } else {
        format!("{trimmed}-l{start_line}")
    }
}

pub fn store_chunk_embedding(conn: &Connection, chunk_id: &str, vector: &[f32]) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO doc_chunk_embeddings (chunk_id, embedding) VALUES (?1, ?2)",
        rusqlite::params![chunk_id, embed::serialize(vector)],
    )?;
    Ok(())
}

pub fn get_chunk_content(conn: &Connection, chunk_id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT content FROM doc_chunks WHERE id = ?1", [chunk_id], |r| {
            r.get(0)
        })
        .optional()?)
}

/// Chunks with no vector yet, oldest document first.
pub fn list_pending_chunk_embeddings(
    conn: &Connection,
    org_id: &str,
    limit: i64,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM doc_chunks c
           JOIN doc_documents d ON d.id = c.document_id
          WHERE d.org_id = ?1
            AND NOT EXISTS (SELECT 1 FROM doc_chunk_embeddings e WHERE e.chunk_id = c.id)
          ORDER BY d.scanned_at, c.ordinal
          LIMIT ?2",
    )?;
    let ids = stmt
        .query_map(rusqlite::params![org_id, limit], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}

/// How much of the corpus is searchable by similarity, and how much is not yet.
pub fn index_status(conn: &Connection, org_id: &str) -> Result<(i64, i64)> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM doc_chunks c JOIN doc_documents d ON d.id = c.document_id
          WHERE d.org_id = ?1",
        [org_id],
        |r| r.get(0),
    )?;
    let embedded: i64 = conn.query_row(
        "SELECT COUNT(*) FROM doc_chunks c
           JOIN doc_documents d ON d.id = c.document_id
           JOIN doc_chunk_embeddings e ON e.chunk_id = c.id
          WHERE d.org_id = ?1",
        [org_id],
        |r| r.get(0),
    )?;
    Ok((total, total - embedded))
}

/// Keyword search over the documentation corpus. Always available, including
/// when no embedding service is configured.
pub fn search_docs_keyword(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<DocChunkHit>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, d.path, c.heading_path, c.anchor, c.content
           FROM doc_chunks c
           JOIN doc_documents d ON d.id = c.document_id
          WHERE d.org_id = ?1 AND (c.content LIKE ?2 OR c.heading_path LIKE ?2)
          ORDER BY d.path, c.ordinal
          LIMIT ?3",
    )?;
    let hits = stmt
        .query_map(rusqlite::params![org_id, pattern, limit], |r| {
            Ok(DocChunkHit {
                chunk_id: r.get(0)?,
                document_id: r.get(1)?,
                path: r.get(2)?,
                heading_path: r.get(3)?,
                anchor: r.get(4)?,
                content: r.get(5)?,
                score: 0.0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(hits)
}

/// Semantic search over the documentation corpus.
pub fn search_docs_semantic(
    conn: &Connection,
    org_id: &str,
    query_vector: &[f32],
    limit: i64,
) -> Result<Vec<DocChunkHit>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, d.path, c.heading_path, c.anchor, c.content, e.embedding
           FROM doc_chunks c
           JOIN doc_documents d ON d.id = c.document_id
           JOIN doc_chunk_embeddings e ON e.chunk_id = c.id
          WHERE d.org_id = ?1",
    )?;
    let mut scored: Vec<DocChunkHit> = stmt
        .query_map([org_id], |r| {
            let blob: Vec<u8> = r.get(6)?;
            let vector = embed::deserialize(&blob);
            Ok(DocChunkHit {
                chunk_id: r.get(0)?,
                document_id: r.get(1)?,
                path: r.get(2)?,
                heading_path: r.get(3)?,
                anchor: r.get(4)?,
                content: r.get(5)?,
                score: embed::cosine(query_vector, &vector),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit.max(0) as usize);
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'U2S', 'u2s')",
            [],
        )
        .unwrap();
        conn
    }

    const DOC: &str = "# Engineering Process\n\nIntro prose.\n\n## Principles\n\nBYOM: never depend on an LLM provider.\n\n## Stack\n\nRust and SQLite.\n";

    #[test]
    fn doc_chunks_preserve_heading_path() {
        let conn = setup();
        let (doc_id, changed) =
            upsert_document(&conn, "org1", None, None, "docs/ENGINEERING_PROCESS.md", "sha1").unwrap();
        assert!(changed);
        replace_chunks(&conn, &doc_id, "docs/ENGINEERING_PROCESS.md", "sha1", DOC).unwrap();

        let headings: Vec<String> = conn
            .prepare("SELECT heading_path FROM doc_chunks WHERE document_id = ?1 ORDER BY ordinal")
            .unwrap()
            .query_map([&doc_id], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(
            headings.iter().any(|h| h.contains("Principles")),
            "the section heading must survive chunking; got {headings:?}"
        );
        assert!(headings.iter().any(|h| h.contains("Stack")));
    }

    /// A heading rename shifts every anchor after it. Re-chunking must replace,
    /// never accumulate.
    #[test]
    fn reindexing_same_document_replaces_chunks_without_duplicates() {
        let conn = setup();
        let (doc_id, _) = upsert_document(&conn, "org1", None, None, "a.md", "sha1").unwrap();
        replace_chunks(&conn, &doc_id, "a.md", "sha1", DOC).unwrap();
        let first: i64 = conn
            .query_row("SELECT COUNT(*) FROM doc_chunks", [], |r| r.get(0))
            .unwrap();
        assert!(first > 1);

        replace_chunks(&conn, &doc_id, "a.md", "sha2", DOC).unwrap();
        let second: i64 = conn
            .query_row("SELECT COUNT(*) FROM doc_chunks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(first, second, "re-chunking replaces, it does not accumulate");
    }

    #[test]
    fn unchanged_document_reports_no_change() {
        let conn = setup();
        let (id1, changed1) = upsert_document(&conn, "org1", None, None, "a.md", "sha1").unwrap();
        let (id2, changed2) = upsert_document(&conn, "org1", None, None, "a.md", "sha1").unwrap();
        assert_eq!(id1, id2);
        assert!(changed1 && !changed2, "an unchanged file must not be re-chunked");

        let (_, changed3) = upsert_document(&conn, "org1", None, None, "a.md", "sha2").unwrap();
        assert!(changed3, "an edited file must be");
    }

    #[test]
    fn reconciliation_vectorizes_pending_and_updates_state() {
        let conn = setup();
        let (doc_id, _) = upsert_document(&conn, "org1", None, None, "a.md", "sha1").unwrap();
        let chunk_ids = replace_chunks(&conn, &doc_id, "a.md", "sha1", DOC).unwrap();

        let (total, pending) = index_status(&conn, "org1").unwrap();
        assert_eq!(total, chunk_ids.len() as i64);
        assert_eq!(pending, total, "nothing is vectorized yet");

        // Stand in for the embedding service — the reconciliation path is what
        // is under test, not the model.
        for id in &chunk_ids {
            store_chunk_embedding(&conn, id, &[0.1, 0.2, 0.3]).unwrap();
        }

        let (total_after, pending_after) = index_status(&conn, "org1").unwrap();
        assert_eq!(total_after, total);
        assert_eq!(pending_after, 0);
        assert!(list_pending_chunk_embeddings(&conn, "org1", 10).unwrap().is_empty());
    }

    #[test]
    fn keyword_search_finds_sections_and_scopes_by_org() {
        let conn = setup();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org2', 'Other', 'other')",
            [],
        )
        .unwrap();
        let (doc_id, _) = upsert_document(&conn, "org1", None, None, "a.md", "sha1").unwrap();
        replace_chunks(&conn, &doc_id, "a.md", "sha1", DOC).unwrap();

        let hits = search_docs_keyword(&conn, "org1", "BYOM", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("BYOM"));
        assert!(hits[0].heading_path.contains("Principles"));

        let other_org = search_docs_keyword(&conn, "org2", "BYOM", 10).unwrap();
        assert!(other_org.is_empty(), "the corpus is scoped by organization");
    }

    #[test]
    fn semantic_search_ranks_by_similarity() {
        let conn = setup();
        let (doc_id, _) = upsert_document(&conn, "org1", None, None, "a.md", "sha1").unwrap();
        let ids = replace_chunks(&conn, &doc_id, "a.md", "sha1", DOC).unwrap();
        store_chunk_embedding(&conn, &ids[0], &[1.0, 0.0, 0.0]).unwrap();
        store_chunk_embedding(&conn, &ids[1], &[0.0, 1.0, 0.0]).unwrap();

        let hits = search_docs_semantic(&conn, "org1", &[0.0, 1.0, 0.0], 5).unwrap();
        assert_eq!(hits[0].chunk_id, ids[1], "the closest vector ranks first");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn anchors_are_unique_within_a_document() {
        let conn = setup();
        let repeated = "## Notes\n\nfirst\n\n## Notes\n\nsecond\n";
        let (doc_id, _) = upsert_document(&conn, "org1", None, None, "a.md", "sha1").unwrap();
        replace_chunks(&conn, &doc_id, "a.md", "sha1", repeated)
            .expect("two sections with the same heading must not collide");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM doc_chunks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }
}
