//! Documentation search and indexing state.
//!
//! A corpus separate from code search on purpose — see `indexer::doc_walker`.
//! These endpoints never return a code chunk, and `/v1/code/search` never
//! returns a documentation chunk; that separation is asserted by
//! `indexer::tests::code_search_results_unchanged_after_doc_indexing`.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::helpers::require_permission,
    db::doc_queries,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "db lock poisoned".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

#[derive(Deserialize)]
pub struct DocSearchParams {
    pub q: String,
    #[serde(default)]
    pub limit: Option<i64>,
    /// `semantic` when an embedding service is available, otherwise `keyword`.
    /// Requesting semantic without one degrades rather than failing.
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DocSearchHit {
    pub chunk_id: String,
    pub document_id: String,
    pub path: String,
    pub heading_path: String,
    pub anchor: String,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Serialize)]
pub struct DocSearchResponse {
    /// The mode actually used, which may differ from the one requested.
    pub mode: String,
    pub hits: Vec<DocSearchHit>,
}

pub async fn search(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<DocSearchParams>,
) -> Result<Json<DocSearchResponse>, (StatusCode, Json<ApiError>)> {
    let query = params.q.trim();
    if query.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "q must not be empty".to_string(),
                code: "validation_error".to_string(),
            }),
        ));
    }
    let limit = params.limit.unwrap_or(20).clamp(1, 100);

    // Embed outside the lock: it is CPU-bound, and holding the connection mutex
    // across it would stall every other request for the duration.
    let wants_semantic = params.mode.as_deref() != Some("keyword");
    let query_vector = match (wants_semantic, store.embed_service()) {
        (true, Some(svc)) => svc.embed_one(query).ok(),
        _ => None,
    };

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "memory:read")?;

    let (mode, hits) = match query_vector {
        Some(vector) => (
            "semantic",
            doc_queries::search_docs_semantic(&conn, &auth.org_id, &vector, limit)
                .map_err(db_err)?,
        ),
        None => (
            "keyword",
            doc_queries::search_docs_keyword(&conn, &auth.org_id, query, limit).map_err(db_err)?,
        ),
    };

    Ok(Json(DocSearchResponse {
        mode: mode.to_string(),
        hits: hits
            .into_iter()
            .map(|h| DocSearchHit {
                chunk_id: h.chunk_id,
                document_id: h.document_id,
                path: h.path,
                heading_path: h.heading_path,
                anchor: h.anchor,
                content: h.content,
                score: h.score,
            })
            .collect(),
    }))
}

#[derive(Debug, Serialize)]
pub struct IndexStatusResponse {
    pub doc_chunks_total: i64,
    pub doc_chunks_pending_embedding: i64,
    pub migrated_artifacts_pending_index: i64,
}

/// What is searchable by similarity and what is not yet.
///
/// A non-zero pending count is a normal state, not an error: artifacts are
/// persisted first and vectorized afterwards, so the backlog is visible by
/// design rather than hidden behind a claim that everything is indexed.
pub async fn index_status(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<IndexStatusResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "memory:read")?;

    let (total, pending) = doc_queries::index_status(&conn, &auth.org_id).map_err(db_err)?;
    let migrated_pending =
        crate::db::migration_queries::count_pending_index(&conn, &auth.org_id).map_err(db_err)?;

    Ok(Json(IndexStatusResponse {
        doc_chunks_total: total,
        doc_chunks_pending_embedding: pending,
        migrated_artifacts_pending_index: migrated_pending,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::connect, migrations},
        models::types::{Role, UserRole},
    };

    fn auth() -> AuthContext {
        AuthContext {
            org_id: "org1".to_string(),
            user_id: "u1".to_string(),
            role: UserRole::Standard(Role::Admin),
        }
    }

    fn store_with_docs() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'U2S', 'u2s')",
            [],
        )
        .unwrap();
        let (doc_id, _) = doc_queries::upsert_document(
            &conn,
            "org1",
            None,
            None,
            "docs/ENGINEERING_PROCESS.md",
            "sha1",
        )
        .unwrap();
        doc_queries::replace_chunks(
            &conn,
            &doc_id,
            "docs/ENGINEERING_PROCESS.md",
            "sha1",
            "# Process\n\n## Principles\n\nBYOM: never depend on an LLM provider.\n",
        )
        .unwrap();
        SqliteStore::new(conn)
    }

    #[tokio::test]
    async fn docs_search_returns_no_code_chunks() {
        let store = store_with_docs();
        {
            // A code chunk that would match the same query, to prove the two
            // corpora do not bleed into each other at the API boundary either.
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO code_projects (id, org_id, name, root_path) VALUES (1, 'org1', 'p', '/tmp')",
                [],
            )
            .ok();
            conn.execute(
                "INSERT INTO code_chunks (project_id, file_path, file_hash, language, symbol, start_line, end_line, content)
                 VALUES (1, 'src/byom.rs', 'h', 'rust', 'byom', 1, 2, 'fn byom() {}')",
                [],
            )
            .ok();
        }

        let Json(resp) = search(
            State(store.clone()),
            Extension(auth()),
            Query(DocSearchParams {
                q: "BYOM".to_string(),
                limit: None,
                mode: None,
            }),
        )
        .await
        .unwrap();

        assert_eq!(
            resp.mode, "keyword",
            "no embed service configured → keyword"
        );
        assert!(!resp.hits.is_empty());
        assert!(
            resp.hits.iter().all(|h| h.path.ends_with(".md")),
            "documentation search must never return a code chunk"
        );
    }

    #[tokio::test]
    async fn index_status_reports_pending_count() {
        let store = store_with_docs();
        let Json(status) = index_status(State(store.clone()), Extension(auth()))
            .await
            .unwrap();
        assert!(status.doc_chunks_total > 0);
        assert_eq!(
            status.doc_chunks_pending_embedding, status.doc_chunks_total,
            "nothing is vectorized without an embedding service, and the API says so"
        );
        assert_eq!(status.migrated_artifacts_pending_index, 0);
    }

    #[tokio::test]
    async fn empty_query_is_rejected() {
        let store = store_with_docs();
        let err = search(
            State(store),
            Extension(auth()),
            Query(DocSearchParams {
                q: "   ".to_string(),
                limit: None,
                mode: None,
            }),
        )
        .await
        .expect_err("an empty query must not scan the corpus");
        assert_eq!(err.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
