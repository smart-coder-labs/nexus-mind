use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use crate::{
    db::queries,
    models::types::{ApiError, AuthContext, CreateSessionRequest, PatchSessionRequest, Session},
    store::sqlite::SqliteStore,
};

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

#[derive(serde::Serialize)]
pub struct CreateSessionResponse {
    pub id: String,
}

pub async fn create_session_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let session = queries::create_session(&conn, &auth.org_id, &input).map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(CreateSessionResponse { id: session.id })))
}

pub async fn patch_session_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(session_id): Path<String>,
    Json(input): Json<PatchSessionRequest>,
) -> Result<Json<Session>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let result = queries::patch_session(&conn, &auth.org_id, &session_id, &input)
        .map_err(db_err)?;

    match result {
        Some(session) => Ok(Json(session)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Session not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{patch, post},
        Router,
    };
    use tower::util::ServiceExt;

    use crate::{
        api::middleware as auth_mw,
        db::{connection::connect, migrations, queries as q},
        store::sqlite::SqliteStore,
    };

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/sessions", post(create_session_handler))
            .route("/v1/sessions/:id", patch(patch_session_handler))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_with_key() -> (SqliteStore, String) {
        let store = make_store();
        let raw_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        (store, raw_key)
    }

    #[tokio::test]
    async fn create_session_returns_201_with_id() {
        let (store, key) = setup_with_key();
        let body = serde_json::json!({ "project": "nexusmind" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/sessions")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["id"].is_string(), "response must include an id field");
        assert!(!json["id"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_session_missing_project_returns_422() {
        let (store, key) = setup_with_key();
        let body = serde_json::json!({ "directory": "/tmp" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/sessions")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn patch_session_returns_200_with_updated_fields() {
        let (store, key) = setup_with_key();

        // First create a session
        let create_body = serde_json::json!({ "project": "nexusmind" });
        let create_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/sessions")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX).await.unwrap();
        let create_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let session_id = create_json["id"].as_str().unwrap().to_string();

        // Now patch it
        let patch_body = serde_json::json!({
            "ended_at": "2026-01-01T01:00:00Z",
            "summary": "Session complete"
        });
        let patch_resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/sessions/{session_id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(patch_resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(patch_resp.into_body(), usize::MAX).await.unwrap();
        let patch_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(patch_json["ended_at"], "2026-01-01T01:00:00Z");
        assert_eq!(patch_json["summary"], "Session complete");
    }

    #[tokio::test]
    async fn patch_session_wrong_id_returns_404() {
        let (store, key) = setup_with_key();
        let patch_body = serde_json::json!({ "summary": "Done" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/v1/sessions/nonexistent-session-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_session_unauthenticated_returns_401() {
        let db = make_store();
        let body = serde_json::json!({ "project": "nexusmind" });

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/sessions")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
