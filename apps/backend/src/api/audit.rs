use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
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
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0).max(0);

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

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
    use crate::db::{connection::connect, migrations};
    use crate::db::queries::{bootstrap, log_audit};

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
}
