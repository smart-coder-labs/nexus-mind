use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::require_permission,
    db::queries,
    models::types::{ApiError, AuditEntry, AuthContext},
    store::sqlite::SqliteStore,
};

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
}
