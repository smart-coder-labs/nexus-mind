use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

use crate::{
    db::queries,
    models::types::{ApiError, AuthContext, Memory},
};

#[derive(Deserialize)]
pub struct StoreInput {
    pub project: Option<String>,
    pub tool: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
}

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
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

pub async fn store(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<StoreInput>,
) -> Result<(StatusCode, Json<Memory>), (StatusCode, Json<ApiError>)> {
    let conn = db.lock().map_err(|_| lock_err())?;

    let project = input.project.as_deref().unwrap_or("default");
    let tags = input.tags.as_deref().unwrap_or(&[]);

    let memory = queries::store_memory(
        &conn,
        &auth.org_id,
        &auth.user_id,
        project,
        &input.tool,
        &input.content,
        tags,
    )
    .map_err(db_err)?;

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "store",
        "memory",
        Some(&memory.id),
        serde_json::json!({ "tool": memory.tool, "project": memory.project }),
    );

    Ok((StatusCode::CREATED, Json(memory)))
}

pub async fn search(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<SearchInput>,
) -> Result<Json<Vec<Memory>>, (StatusCode, Json<ApiError>)> {
    let conn = db.lock().map_err(|_| lock_err())?;

    let limit = input.limit.unwrap_or(20);
    let memories = queries::search_memories(&conn, &auth.org_id, &input.query, limit)
        .map_err(db_err)?;

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "search",
        "memory",
        None,
        serde_json::json!({ "query": input.query, "results": memories.len() }),
    );

    Ok(Json(memories))
}

pub async fn list(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Memory>>, (StatusCode, Json<ApiError>)> {
    let conn = db.lock().map_err(|_| lock_err())?;

    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let memories = queries::list_memories(
        &conn,
        &auth.org_id,
        params.user_id.as_deref(),
        params.tool.as_deref(),
        params.project.as_deref(),
        limit,
        offset,
    )
    .map_err(db_err)?;

    Ok(Json(memories))
}

pub async fn delete(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let conn = db.lock().map_err(|_| lock_err())?;

    let deleted = queries::delete_memory(&conn, &auth.org_id, &id).map_err(db_err)?;

    if deleted {
        let _ = queries::log_audit(
            &conn,
            &auth.org_id,
            &auth.user_id,
            "delete",
            "memory",
            Some(&id),
            serde_json::json!({}),
        );
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
        db::{connection::connect, migrations, queries as q},
    };

    fn make_db() -> Arc<Mutex<Connection>> {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn app(db: Arc<Mutex<Connection>>) -> Router {
        Router::new()
            .route("/v1/memory/store", post(store))
            .route("/v1/memory/search", post(search))
            .route("/v1/memory/:id", delete(crate::api::memory::delete))
            .route("/v1/memory", get(list))
            .layer(middleware::from_fn_with_state(db.clone(), auth_mw::auth))
            .with_state(db)
    }

    fn setup_with_key() -> (Arc<Mutex<Connection>>, String) {
        let db = make_db();
        let raw_key = {
            let conn = db.lock().unwrap();
            let (_, _, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        (db, raw_key)
    }

    #[tokio::test]
    async fn store_memory_returns_201() {
        let (db, key) = setup_with_key();
        let body = serde_json::json!({ "tool": "claude", "content": "use snake_case" });

        let resp = app(db)
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
        let db = make_db();
        let body = serde_json::json!({ "tool": "claude", "content": "test" });

        let resp = app(db)
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
        let (db, key) = setup_with_key();

        let resp = app(db)
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
