use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use crate::api::helpers::AppJson;

use crate::{
    db::queries,
    models::types::{ApiError, AuthContext, CreateSessionRequest, Memory, PatchSessionRequest, Session},
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

fn not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Session not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

/// Whether the caller may see a session in `project`. Admins always may; non-admins only
/// when the project is org-shared (no registered project row) or they are a member of it.
fn session_project_visible(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    project: &str,
) -> Result<bool, (StatusCode, Json<ApiError>)> {
    let viewer = if auth.role.is_admin() { None } else { Some(auth.user_id.as_str()) };
    queries::user_can_view_project_name(conn, &auth.org_id, project, viewer).map_err(db_err)
}

#[derive(serde::Serialize)]
pub struct CreateSessionResponse {
    pub id: String,
    pub name: Option<String>,
}

pub async fn create_session_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let session = queries::create_session(&conn, &auth.org_id, &input).map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(CreateSessionResponse { id: session.id, name: session.name })))
}

pub async fn get_session_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(session_id): Path<String>,
) -> Result<Json<Session>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let result = queries::get_session(&conn, &auth.org_id, &session_id).map_err(db_err)?;

    match result {
        // Non-admins may only read a session whose project they can see; otherwise 404
        // (indistinguishable from a non-existent session — no existence leak).
        Some(session) if session_project_visible(&conn, &auth, &session.project)? => Ok(Json(session)),
        _ => Err(not_found()),
    }
}

pub async fn patch_session_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(session_id): Path<String>,
    AppJson(input): AppJson<PatchSessionRequest>,
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

pub async fn list_sessions_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<crate::models::types::SessionWithCount>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let viewer = if auth.role.is_admin() { None } else { Some(auth.user_id.as_str()) };
    let sessions = queries::list_sessions_visible(&conn, &auth.org_id, viewer).map_err(db_err)?;

    Ok(Json(sessions))
}

pub async fn list_session_memories_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<Memory>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let session = queries::get_session(&conn, &auth.org_id, &session_id).map_err(db_err)?;
    match session {
        // Non-members of the session's project get 404 (no existence leak), consistent
        // with get_session_handler.
        Some(ref s) if session_project_visible(&conn, &auth, &s.project)? => {}
        _ => return Err(not_found()),
    }

    let viewer = if auth.role.is_admin() { None } else { Some(auth.user_id.as_str()) };
    let memories = queries::list_memories_visible(
        &conn,
        &auth.org_id,
        None,
        None,
        None,
        None,
        None,
        Some(&session_id),
        1000,
        0,
        false,
        None,
        None,
        None,
        viewer,
    )
    .map_err(db_err)?;

    Ok(Json(memories))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    use crate::{
        api::middleware as auth_mw,
        db::{connection::connect, migrations, queries as q},
        models::types::StoreMemoryRequest,
        store::sqlite::SqliteStore,
    };

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/sessions", get(list_sessions_handler).post(create_session_handler))
            .route("/v1/sessions/:id", get(get_session_handler).patch(patch_session_handler))
            .route("/v1/sessions/:id/memories", get(list_session_memories_handler))
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

    fn create_member_with_id(store: &SqliteStore, org_id: &str, role: &str) -> (String, String) {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, ?3, 'Test', ?4, 'active', datetime('now'))",
            rusqlite::params![user_id, org_id, format!("{}-{role}@test.com", &user_id[..8]), role],
        ).unwrap();
        let key_id = Uuid::new_v4().to_string();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org_id, key_hash],
        ).unwrap();
        (raw_key, user_id)
    }

    async fn session_ids(store: &SqliteStore, key: &str) -> Vec<String> {
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .uri("/v1/sessions")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let arr: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        arr.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_string()).collect()
    }

    async fn get_session_status(store: &SqliteStore, key: &str, id: &str) -> StatusCode {
        app(store.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/sessions/{id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    /// Non-admins may only see sessions of projects they belong to, plus sessions whose
    /// project is org-shared (no registered project row). Never another project's sessions —
    /// via list, get-by-id (404, no existence leak), or session-memories.
    #[tokio::test]
    async fn member_only_sees_member_and_orgshared_sessions() {
        let (store, admin_key) = setup_with_key();
        let org_id: String = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)).unwrap()
        };

        // Registered projects (create_project does NOT auto-seed members) and one org-shared
        // session whose project has no projects row.
        let (secret_sid, shared_sid, orphan_sid, member_key) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let _secret = q::create_project(&conn, &org_id, "proj-secret", None, None).unwrap();
            let shared_id = q::create_project(&conn, &org_id, "proj-shared", None, None).unwrap().id;
            let mk = |p: &str| q::create_session(&conn, &org_id, &crate::models::types::CreateSessionRequest {
                project: p.to_string(), name: None, directory: None, summary: None,
            }).unwrap().id;
            let secret_sid = mk("proj-secret");
            let shared_sid = mk("proj-shared");
            let orphan_sid = mk("no-such-project"); // org-shared: no projects row
            drop(conn);
            let (member_key, member_id) = create_member_with_id(&store, &org_id, "member");
            // Member belongs to proj-shared only.
            let db2 = store.conn();
            let conn2 = db2.lock().unwrap();
            conn2.execute(
                "INSERT INTO project_members (id, project_id, user_id, role, created_at)
                 VALUES (?1, ?2, ?3, 'member', datetime('now'))",
                rusqlite::params![uuid::Uuid::new_v4().to_string(), shared_id, member_id],
            ).unwrap();
            (secret_sid, shared_sid, orphan_sid, member_key)
        };

        // LIST: member sees shared + orphan, not secret.
        let ids = session_ids(&store, &member_key).await;
        assert!(ids.contains(&shared_sid), "member must see their project's session");
        assert!(ids.contains(&orphan_sid), "member must see org-shared session");
        assert!(!ids.contains(&secret_sid), "member must NOT see a non-member project's session");

        // GET by id: 404 for secret, 200 for shared.
        assert_eq!(get_session_status(&store, &member_key, &secret_sid).await, StatusCode::NOT_FOUND);
        assert_eq!(get_session_status(&store, &member_key, &shared_sid).await, StatusCode::OK);

        // SESSION MEMORIES: 404 for a session the member cannot see.
        let mem_status = app(store.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/sessions/{secret_sid}/memories"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status();
        assert_eq!(mem_status, StatusCode::NOT_FOUND);

        // ADMIN sees all three sessions.
        let admin_ids = session_ids(&store, &admin_key).await;
        assert!(admin_ids.contains(&secret_sid) && admin_ids.contains(&shared_sid) && admin_ids.contains(&orphan_sid));
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
    async fn list_sessions_returns_empty_for_new_org() {
        let (store, key) = setup_with_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/sessions")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn list_sessions_returns_created_sessions_with_memory_count() {
        let (store, key) = setup_with_key();

        // Create a session
        let body = serde_json::json!({ "project": "nexusmind" });
        let create_resp = app(store.clone())
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
        assert_eq!(create_resp.status(), StatusCode::CREATED);

        // List sessions
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/sessions")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["project"], "nexusmind");
        assert_eq!(arr[0]["memory_count"], 0);
    }

    #[tokio::test]
    async fn get_session_by_id_returns_200() {
        let (store, key) = setup_with_key();

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

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/sessions/{session_id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["id"], session_id);
        assert_eq!(json["project"], "nexusmind");
    }

    #[tokio::test]
    async fn get_session_by_id_wrong_id_returns_404() {
        let (store, key) = setup_with_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/sessions/nonexistent-session-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
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

    #[tokio::test]
    async fn list_session_memories_returns_memories_for_session() {
        let (store, key) = setup_with_key();

        // Create a session
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
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX).await.unwrap();
        let create_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let session_id = create_json["id"].as_str().unwrap().to_string();

        // Insert a memory linked to that session
        {
            let (org_id, user_id) = {
                let db = store.conn();
                let conn = db.lock().unwrap();
                let org = q::list_orgs(&conn).unwrap().into_iter().next().unwrap();
                let users = q::list_users(&conn, &org.id).unwrap();
                (org.id, users[0].id.clone())
            };
            let req = StoreMemoryRequest {
                project: Some("nexusmind".to_string()),
                tool: "claude-code".to_string(),
                content: "session memory content".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: Some(session_id.clone()),
            };
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::get_or_create_project(&conn, &org_id, "nexusmind").unwrap();
            q::upsert_memory(&conn, &org_id, &user_id, &req).unwrap();
        }

        // GET /v1/sessions/:id/memories
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/sessions/{session_id}/memories"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["session_id"], session_id.as_str());
        assert_eq!(arr[0]["content"], "session memory content");
    }

    #[tokio::test]
    async fn list_session_memories_unknown_session_returns_404() {
        let (store, key) = setup_with_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/sessions/nonexistent-session/memories")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_session_memories_empty_for_session_with_no_memories() {
        let (store, key) = setup_with_key();

        // Create a session but attach no memories
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
        let session_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/sessions/{session_id}/memories"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.as_array().unwrap().is_empty());
    }
}
