use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    db::queries as db_queries,
    models::types::{ApiError, AuthContext, Memory, StoreMemoryRequest},
    store::{sqlite::SqliteStore, MemoryFilters, MemoryStore, SearchMode},
    api::helpers::require_permission,
};

const EXPORT_HARD_CAP: i64 = 10_000;

#[derive(Deserialize)]
pub struct ExportParams {
    #[serde(default = "default_csv")]
    pub format: ExportFormat,
}

#[derive(Deserialize, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
}

fn default_csv() -> ExportFormat {
    ExportFormat::Csv
}

fn truncate_content(s: &str) -> String {
    const MAX: usize = 500;
    for (count, (idx, _)) in s.char_indices().enumerate() {
        if count == MAX {
            return format!("{}…", &s[..idx]);
        }
    }
    s.to_string()
}

fn memory_rows_to_csv(memories: &[Memory]) -> anyhow::Result<Vec<u8>> {
    let mut wtr = csv::WriterBuilder::new()
        .quote_style(csv::QuoteStyle::Necessary)
        .from_writer(Vec::new());

    wtr.write_record(["id", "title", "type", "scope", "project", "tool", "content", "created_at"])?;

    for m in memories {
        let title = m.title.as_deref().unwrap_or("");
        let memory_type = m.memory_type.as_deref().unwrap_or("");
        let content = truncate_content(&m.content);
        wtr.write_record([
            &m.id,
            title,
            memory_type,
            &m.scope,
            &m.project,
            &m.tool,
            &content,
            &m.created_at,
        ])?;
    }

    Ok(wtr.into_inner()?)
}

pub async fn export(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ExportParams>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        require_permission(&conn, &auth, None, "memory:read")?;
    }

    let filters = MemoryFilters {
        user_id: None,
        tool: None,
        project: None,
        memory_type: None,
        scope: None,
        limit: EXPORT_HARD_CAP,
        offset: 0,
    };
    let memories = store.list(&auth.org_id, &filters).map_err(store_err)?;

    let truncated = memories.len() as i64 == EXPORT_HARD_CAP;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let (content_type, filename, body) = match params.format {
        ExportFormat::Csv => {
            let body = memory_rows_to_csv(&memories).map_err(store_err)?;
            (
                "text/csv; charset=utf-8",
                format!("memories-{today}.csv"),
                body,
            )
        }
        ExportFormat::Json => {
            let body = serde_json::to_vec_pretty(&memories)
                .map_err(|e| store_err(anyhow::anyhow!(e)))?;
            (
                "application/json; charset=utf-8",
                format!("memories-{today}.json"),
                body,
            )
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap(),
    );
    if truncated {
        headers.insert("x-export-truncated", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, headers, body).into_response())
}

#[derive(Deserialize)]
pub struct SearchInput {
    pub query: String,
    pub limit: Option<i64>,
    /// Search mode: "keyword" (default), "semantic", or "hybrid".
    pub mode: Option<String>,
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
    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        let project = input.project.as_deref().unwrap_or("default");
        require_permission(&conn, &auth, Some(project), "memory:write")?;
    }

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
    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        require_permission(&conn, &auth, None, "memory:search")?;
    }

    let limit = input.limit.unwrap_or(20);
    let mode = input.mode.as_deref().unwrap_or("keyword").parse::<SearchMode>().unwrap_or(SearchMode::Keyword);
    let memories = store
        .search(&auth.org_id, &auth.user_id, &input.query, limit, mode)
        .map_err(store_err)?;
    Ok(Json(memories))
}

pub async fn list(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Memory>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        require_permission(&conn, &auth, params.project.as_deref(), "memory:read")?;
    }

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

pub async fn get_by_id(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Memory>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    let memory = db_queries::get_memory_by_id_for_org(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    match memory {
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Some(m) => {
            // Permission check: requires memory:read for the memory's project.
            require_permission(&conn, &auth, Some(&m.project.clone()), "memory:read")?;
            Ok(Json(m))
        }
    }
}

pub async fn delete(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;
        // Fetch owner and project to enforce project overrides and ownership
        let details = db_queries::get_memory_owner_and_project(&conn, &auth.org_id, &id).map_err(store_err)?;

        match details {
            None => return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "Memory not found".to_string(),
                    code: "not_found".to_string(),
                }),
            )),
            Some((ref owner_id, ref project_name)) => {
                require_permission(&conn, &auth, Some(project_name), "memory:delete")?;

                if *owner_id != auth.user_id && !auth.role.is_admin() {
                    let is_project_admin = match db_queries::get_project_member_role(&conn, &auth.org_id, project_name, &auth.user_id) {
                        Ok(Some(role_str)) => role_str == "admin",
                        _ => false,
                    };
                    if !is_project_admin {
                        return Err((
                            StatusCode::FORBIDDEN,
                            Json(ApiError {
                                error: "Insufficient permissions".to_string(),
                                code: "forbidden".to_string(),
                            }),
                        ));
                    }
                }
            }
        }
    }

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
            .route("/v1/memory/:id", get(super::get_by_id).delete(super::delete))
            .route("/v1/memory", get(list))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
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

    /// Bootstraps the org and returns (store, admin_key, org_id).
    fn setup_org() -> (SqliteStore, String, String) {
        let store = make_store();
        let (admin_key, org_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            (key, org.id)
        };
        (store, admin_key, org_id)
    }

    /// Creates an extra user with the given role, returns their raw API key.
    fn create_test_user(store: &SqliteStore, org_id: &str, role: &str) -> String {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, ?3, 'Test', ?4, 'active', datetime('now'))",
            rusqlite::params![user_id, org_id, format!("{role}@test.com"), role],
        ).unwrap();
        let key_id = Uuid::new_v4().to_string();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org_id, key_hash],
        ).unwrap();
        raw_key
    }

    /// Stores a memory via admin key and returns its id.
    async fn seed_memory(store: &SqliteStore, admin_key: &str) -> String {
        let body = serde_json::json!({ "tool": "claude", "content": "seed content" });
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        mem["id"].as_str().unwrap().to_string()
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

    // ── T4: store role gate ───────────────────────────────────────────────────

    #[tokio::test]
    async fn store_viewer_returns_403() {
        let (store, _, org_id) = setup_org();
        let viewer_key = create_test_user(&store, &org_id, "viewer");
        let body = serde_json::json!({ "tool": "claude", "content": "test" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {viewer_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn store_member_returns_201() {
        let (store, _, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");
        let body = serde_json::json!({ "tool": "claude", "content": "member content" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // ── T5: delete role + ownership gate ─────────────────────────────────────

    #[tokio::test]
    async fn delete_viewer_returns_403() {
        let (store, admin_key, org_id) = setup_org();
        let viewer_key = create_test_user(&store, &org_id, "viewer");
        let mem_id = seed_memory(&store, &admin_key).await;

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {viewer_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_member_own_memory_returns_204() {
        let (store, _, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        // Member stores their own memory
        let body = serde_json::json!({ "tool": "claude", "content": "member memory" });
        let store_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(store_resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(store_resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mem_id = mem["id"].as_str().unwrap().to_string();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_member_other_memory_returns_403() {
        let (store, admin_key, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        // Admin stores a memory; member tries to delete it
        let mem_id = seed_memory(&store, &admin_key).await;

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_admin_other_memory_returns_204() {
        let (store, admin_key, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        // Member stores a memory; admin deletes it
        let body = serde_json::json!({ "tool": "claude", "content": "member content" });
        let store_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(store_resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mem_id = mem["id"].as_str().unwrap().to_string();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_nonexistent_memory_returns_404() {
        let (store, _, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/does-not-exist")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── T-05 tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_by_id_returns_200_for_own_memory() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(mem["id"].as_str().unwrap(), mem_id);
        // project_id field must be present (may be null but the key must exist)
        assert!(mem.get("project_id").is_some(), "response must include project_id field");
    }

    #[tokio::test]
    async fn get_by_id_returns_404_for_unknown_id() {
        let (store, admin_key, _) = setup_org();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/memory/ghost-id-does-not-exist")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── T-01 memory export tests (RED phase) ─────────────────────────────────

    fn app_with_export(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/memory/store", post(super::store))
            .route("/v1/memory/export", get(super::export))
            .route("/v1/memory/:id", get(super::get_by_id).delete(super::delete))
            .route("/v1/memory", get(list))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn memory_export_csv_returns_200() {
        let (store, admin_key) = setup_with_key();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/csv"), "expected text/csv, got: {ct}");
    }

    #[tokio::test]
    async fn memory_export_csv_contains_header_row() {
        let (store, admin_key) = setup_with_key();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        let first_line = body.lines().next().unwrap_or("");
        assert_eq!(
            first_line,
            "id,title,type,scope,project,tool,content,created_at",
            "first line must be the CSV header row"
        );
    }

    #[tokio::test]
    async fn memory_export_json_returns_200() {
        let (store, admin_key) = setup_with_key();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=json")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("application/json"), "expected application/json, got: {ct}");
    }

    #[tokio::test]
    async fn memory_export_content_truncated() {
        let (store, admin_key) = setup_with_key();

        // Store a memory with content exceeding 500 chars
        let long_content = "a".repeat(600);
        let body = serde_json::json!({ "tool": "claude", "content": long_content });
        app_with_export(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&bytes).unwrap();
        // The data row content cell must be truncated (500 chars + "…" = 501+ char cell, not 600)
        // The CSV has: id,title,type,scope,project,tool,content,created_at
        // content cell must end with "…" (the ellipsis character)
        assert!(body_str.contains('…'), "truncated content must end with … character");
        // Full 600-char content must NOT appear verbatim
        assert!(!body_str.contains(&"a".repeat(501)), "content must be truncated at 500 chars");
    }

    #[tokio::test]
    async fn get_by_id_returns_404_for_other_org_memory() {
        // Org A stores a memory; org B tries to fetch it — must get 404, not 403.
        let store_a = make_store();
        let key_a = {
            let db = store_a.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "OrgA", "orga", "admin@a.com", "AdminA").unwrap();
            key
        };

        let store_b = make_store();
        let key_b = {
            let db = store_b.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "OrgB", "orgb", "admin@b.com", "AdminB").unwrap();
            key
        };

        // Org A stores a memory in their own store.
        let mem_id = seed_memory(&store_a, &key_a).await;

        // Org B tries to fetch org A's memory id via org B's store — must be 404.
        let resp = app(store_b)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {key_b}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
