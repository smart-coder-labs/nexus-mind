use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    models::types::{ApiError, AuthContext, Memory, StoreMemoryRequest},
    store::{sqlite::SqliteStore, MemoryFilters, MemoryStore},
};

#[derive(Deserialize)]
pub struct SearchInput {
    pub query: String,
    pub limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub user_id: Option<String>,
    pub tool: Option<String>,
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub type_filter: Option<String>,
    pub scope: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn store_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let msg = e.to_string();
    if msg.starts_with("invalid_session_id:") {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: msg.replacen("invalid_session_id:", "session_id '", 1) + "' not found for this org",
                code: "invalid_session_id".to_string(),
            }),
        );
    }
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: msg,
            code: "internal_error".to_string(),
        }),
    )
}

pub async fn store(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<StoreMemoryRequest>,
) -> Result<(StatusCode, Json<Memory>), (StatusCode, Json<ApiError>)> {
    let is_upsert = input.topic_key.is_some();
    let memory = store.store(&auth.org_id, &auth.user_id, &input).map_err(store_err)?;

    let status = if is_upsert && memory.revision_count > 1 {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((status, Json(memory)))
}

pub async fn search(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<SearchInput>,
) -> Result<Json<Vec<Memory>>, (StatusCode, Json<ApiError>)> {
    let limit = input.limit.unwrap_or(20);
    let memories = store
        .search(&auth.org_id, &auth.user_id, &input.query, limit)
        .map_err(store_err)?;
    Ok(Json(memories))
}

pub async fn list(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Memory>>, (StatusCode, Json<ApiError>)> {
    let filters = MemoryFilters {
        user_id: params.user_id.as_deref(),
        tool: params.tool.as_deref(),
        project: params.project.as_deref(),
        memory_type: params.type_filter.as_deref(),
        scope: params.scope.as_deref(),
        limit: params.limit.unwrap_or(50),
        offset: params.offset.unwrap_or(0),
    };
    let memories = store.list(&auth.org_id, &filters).map_err(store_err)?;
    Ok(Json(memories))
}

pub async fn delete(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let deleted = store
        .delete(&auth.org_id, &auth.user_id, &id)
        .map_err(store_err)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{delete, get, post},
        Router,
    };
    use tower::util::ServiceExt;

    use crate::{
        api::middleware as auth_mw,
        db::{connection::connect, migrations},
        db::queries as q,
        store::sqlite::SqliteStore,
    };

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/memory/store", post(super::store))
            .route("/v1/memory/search", post(search))
            .route("/v1/memory/:id", delete(super::delete))
            .route("/v1/memory", get(list))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .with_state(store)
    }

    fn setup_with_key() -> (SqliteStore, String) {
        let store = make_store();
        let raw_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        (store, raw_key)
    }

    #[tokio::test]
    async fn store_memory_returns_201() {
        let (store, key) = setup_with_key();
        let body = serde_json::json!({ "tool": "claude", "content": "use snake_case" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn store_memory_unauthenticated_returns_401() {
        let store = make_store();
        let body = serde_json::json!({ "tool": "claude", "content": "test" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn delete_memory_not_found_returns_404() {
        let (store, key) = setup_with_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/nonexistent-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
