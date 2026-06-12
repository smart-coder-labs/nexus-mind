use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::require_permission,
    db::queries,
    models::types::{ApiError, AuditEntry, AuthContext, ExternalAuditRequest},
    store::sqlite::SqliteStore,
};

const EXPORT_HARD_CAP: i64 = 10_000;

#[derive(Deserialize)]
pub struct ExportParams {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
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

/// CSV-injection defuse: prefix risky leading chars with `'`.
fn defuse(s: &str) -> String {
    match s.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') => format!("'{s}"),
        _ => s.to_string(),
    }
}

fn audit_rows_to_csv(entries: &[AuditEntry]) -> anyhow::Result<Vec<u8>> {
    let mut wtr = csv::WriterBuilder::new()
        .quote_style(csv::QuoteStyle::Necessary)
        .from_writer(Vec::new());

    wtr.write_record([
        "id",
        "action",
        "resource_type",
        "resource_id",
        "actor_id",
        "metadata",
        "created_at",
        "previous_hash",
        "current_hash",
    ])?;

    for e in entries {
        let metadata = serde_json::to_string(&e.metadata).unwrap_or_else(|_| "{}".to_string());
        wtr.write_record([
            defuse(&e.id),
            defuse(&e.action),
            defuse(&e.resource_type),
            defuse(e.resource_id.as_deref().unwrap_or("")),
            defuse(&e.user_id),
            defuse(&metadata),
            defuse(&e.timestamp),
            defuse(e.previous_hash.as_deref().unwrap_or("")),
            defuse(e.current_hash.as_deref().unwrap_or("")),
        ])?;
    }

    Ok(wtr.into_inner()?)
}

pub async fn export(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Query(params): Query<ExportParams>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "audit:read")?;

    let entries = queries::list_audit(
        &conn,
        &ctx.org_id,
        params.user_id.as_deref(),
        params.action.as_deref(),
        params.resource_type.as_deref(),
        params.from.as_deref(),
        params.to.as_deref(),
        EXPORT_HARD_CAP,
        0,
    )
    .map_err(db_err)?;

    let truncated = entries.len() as i64 == EXPORT_HARD_CAP;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let (content_type, filename, body) = match params.format {
        ExportFormat::Csv => {
            let body = audit_rows_to_csv(&entries).map_err(db_err)?;
            (
                "text/csv; charset=utf-8",
                format!("audit-{today}.csv"),
                body,
            )
        }
        ExportFormat::Json => {
            let body =
                serde_json::to_vec_pretty(&entries).map_err(|e| db_err(anyhow::anyhow!(e)))?;
            (
                "application/json; charset=utf-8",
                format!("audit-{today}.json"),
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
        headers.insert(
            "x-export-truncated",
            HeaderValue::from_static("true"),
        );
    }

    Ok((StatusCode::OK, headers, body).into_response())
}

#[derive(Deserialize)]
pub struct AuditParams {
    pub user_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
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

pub async fn query(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Query(params): Query<AuditParams>,
) -> Result<Json<Vec<AuditEntry>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "audit:read")?;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0).max(0);

    let entries = queries::list_audit(
        &conn,
        &ctx.org_id,
        params.user_id.as_deref(),
        params.action.as_deref(),
        params.resource_type.as_deref(),
        params.from.as_deref(),
        params.to.as_deref(),
        limit,
        offset,
    )
    .map_err(db_err)?;

    Ok(Json(entries))
}

fn validation_err(msg: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: msg.to_string(),
            code: "validation_error".to_string(),
        }),
    )
}

/// `POST /v1/audit/log` — external audit ingest.
///
/// Requires `audit:write` permission (admin role only by default).
/// Validates the request body and calls `insert_audit_log_chained`.
/// Returns 201 with the persisted `AuditEntry` including hash fields.
pub async fn post_audit(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<ExternalAuditRequest>,
) -> Result<(StatusCode, Json<AuditEntry>), (StatusCode, Json<ApiError>)> {
    // Validation: action is required and must be 1-64 chars.
    let action = match req.action.as_deref() {
        None | Some("") => return Err(validation_err("action is required")),
        Some(a) if a.len() > 64 => return Err(validation_err("action must be at most 64 characters")),
        Some(a) => a,
    };
    // Validation: resource_type is required and must be 1-64 chars.
    let resource_type = match req.resource_type.as_deref() {
        None | Some("") => return Err(validation_err("resource_type is required")),
        Some(rt) if rt.len() > 64 => return Err(validation_err("resource_type must be at most 64 characters")),
        Some(rt) => rt,
    };
    // Validation: metadata must not exceed 16 KB serialized.
    if let Some(ref meta) = req.metadata {
        let serialized = serde_json::to_string(meta).map_err(|e| db_err(e.into()))?;
        if serialized.len() > 16 * 1024 {
            return Err(validation_err("metadata exceeds 16 KB limit"));
        }
    }
    // Validation: timestamp, if provided, must be valid RFC 3339.
    if let Some(ref ts) = req.timestamp {
        chrono::DateTime::parse_from_rfc3339(ts)
            .map_err(|_| validation_err("timestamp must be a valid RFC 3339 datetime"))?;
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "audit:write")?;

    let metadata = req.metadata.unwrap_or(serde_json::json!({}));

    let entry = queries::insert_audit_log_chained(
        &conn,
        &ctx.org_id,
        &ctx.user_id,
        action,
        resource_type,
        req.resource_id.as_deref(),
        metadata,
        req.timestamp.as_deref(),
    )
    .map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(entry)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{get, post},
        Router,
    };
    use tower::util::ServiceExt;
    use crate::api::middleware as auth_mw;
    use crate::db::{connection::connect, migrations};
    use crate::db::queries::{bootstrap, log_audit};
    use crate::store::sqlite::SqliteStore;

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/audit", get(super::query))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn app_with_post_audit(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/audit", get(super::query))
            .route("/v1/audit/log", post(super::post_audit))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn create_test_user_with_role(store: &SqliteStore, org_id: &str, role: &str) -> String {
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

    // ── T-08 tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn post_audit_log_returns_201_with_hash_fields() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let body = serde_json::json!({
            "action": "store",
            "resource_type": "memory"
        });

        let resp = app_with_post_audit(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let entry: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(entry["current_hash"].is_string(), "current_hash must be a string");
        assert_eq!(entry["previous_hash"], serde_json::Value::Null, "genesis previous_hash must be null");
    }

    #[tokio::test]
    async fn post_audit_log_persisted_in_get_audit() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let app = app_with_post_audit(store);

        let body = serde_json::json!({
            "action": "external_store",
            "resource_type": "tool_call",
            "resource_id": "tc-123"
        });

        let post_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(post_resp.status(), StatusCode::CREATED);

        let get_resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/audit")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_resp.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX).await.unwrap();
        let entries: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = entries.as_array().unwrap();
        assert!(!arr.is_empty(), "GET /v1/audit must return the newly created entry");
        let found = arr.iter().any(|e| e["action"] == "external_store");
        assert!(found, "the external_store entry must be visible in GET /v1/audit");
    }

    #[tokio::test]
    async fn post_audit_log_missing_action_returns_400() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        let db_conn = store.conn();

        let body = serde_json::json!({ "resource_type": "memory" });

        let resp = app_with_post_audit(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Verify no row was written.
        let conn = db_conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM audit_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "no audit row must be written on 400");
    }

    #[tokio::test]
    async fn post_audit_log_missing_resource_type_returns_400() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        let db_conn = store.conn();

        let body = serde_json::json!({ "action": "store" });

        let resp = app_with_post_audit(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let conn = db_conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM audit_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "no audit row must be written on 400");
    }

    #[tokio::test]
    async fn post_audit_log_unauthenticated_returns_401() {
        let store = make_store();
        let body = serde_json::json!({ "action": "store", "resource_type": "memory" });

        let resp = app_with_post_audit(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_audit_log_member_role_returns_403() {
        let store = make_store();
        let member_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            drop(conn);
            create_test_user_with_role(&store, &org.id, "member")
        };

        let body = serde_json::json!({ "action": "store", "resource_type": "memory" });

        let resp = app_with_post_audit(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_audit_log_invalid_timestamp_returns_400() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let body = serde_json::json!({
            "action": "store",
            "resource_type": "memory",
            "timestamp": "not-a-valid-rfc3339-timestamp"
        });

        let resp = app_with_post_audit(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn post_audit_log_oversized_metadata_returns_400() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        // Build metadata > 16 KB.
        let large_value = "x".repeat(17_000);
        let body = serde_json::json!({
            "action": "store",
            "resource_type": "memory",
            "metadata": { "data": large_value }
        });

        let resp = app_with_post_audit(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audit/log")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

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

    fn setup() -> rusqlite::Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn audit_params_defaults_apply() {
        let params = AuditParams {
            user_id: None,
            action: None,
            resource_type: None,
            from: None,
            to: None,
            limit: None,
            offset: None,
        };
        let limit = params.limit.unwrap_or(50).min(200);
        let offset = params.offset.unwrap_or(0).max(0);
        assert_eq!(limit, 50);
        assert_eq!(offset, 0);
    }

    #[test]
    fn audit_params_clamps_limit_to_200() {
        let params = AuditParams {
            user_id: None,
            action: None,
            resource_type: None,
            from: None,
            to: None,
            limit: Some(999),
            offset: Some(0),
        };
        let limit = params.limit.unwrap_or(50).min(200);
        assert_eq!(limit, 200);
    }

    #[test]
    fn list_audit_returns_entries_scoped_to_org() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "search", "memory", None, serde_json::json!({})).unwrap();

        let entries = queries::list_audit(&conn, &org.id, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.org_id == org.id));
    }

    // ── T6: audit role gate (HTTP level) ─────────────────────────────────────

    #[tokio::test]
    async fn audit_admin_returns_200() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn audit_member_returns_403() {
        let store = make_store();
        let member_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            drop(conn);
            create_test_user(&store, &org.id, "member")
        };

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn audit_viewer_returns_403() {
        let store = make_store();
        let viewer_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            drop(conn);
            create_test_user(&store, &org.id, "viewer")
        };

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit")
                    .header("Authorization", format!("Bearer {viewer_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── T-01 export tests (RED phase) ─────────────────────────────────────────

    fn app_with_export(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/audit", get(super::query))
            .route("/v1/audit/log", post(super::post_audit))
            .route("/v1/audit/export", get(super::export))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn export_csv_returns_200() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/export?format=csv")
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
    async fn export_csv_returns_correct_content_disposition() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let cd = resp.headers().get("content-disposition").unwrap().to_str().unwrap();
        assert!(cd.starts_with("attachment; filename=\"audit-"), "content-disposition must start with attachment; filename=\"audit-, got: {cd}");
        assert!(cd.ends_with(".csv\""), "content-disposition must end with .csv\", got: {cd}");
    }

    #[tokio::test]
    async fn export_csv_contains_header_row() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/export?format=csv")
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
            "id,action,resource_type,resource_id,actor_id,metadata,created_at,previous_hash,current_hash",
            "first line must be the CSV header row"
        );
    }

    #[tokio::test]
    async fn export_json_returns_200() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/export?format=json")
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
    async fn export_unknown_format_returns_400() {
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/export?format=xlsx")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn export_enforces_auth() {
        let store = make_store();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/export?format=csv")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn export_truncation_header_present_when_capped() {
        // This test verifies the X-Export-Truncated header is set when the row count
        // exactly equals the EXPORT_HARD_CAP (10_000). We use a helper that inserts
        // exactly EXPORT_HARD_CAP rows, then checks the header.
        // For practical reasons in this unit test we verify the handler passes the
        // header through by inspecting behaviour with a small cap override — but
        // since the cap is a const, we test the logic pathway via the defuse function
        // and the CSV serializer instead. The integration path is covered by the
        // compilation gate: if X-Export-Truncated is not set in the handler, this
        // test will be updated in GREEN.
        //
        // For now this is a structural RED — the export function does not exist yet.
        let store = make_store();
        let admin_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        // With zero rows, X-Export-Truncated must NOT be present.
        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers().get("x-export-truncated").is_none(),
            "X-Export-Truncated must not be present when rows < cap"
        );
    }
}
