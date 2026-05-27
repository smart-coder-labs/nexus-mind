use axum::{extract::{Path, Query, State}, http::StatusCode, Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    db::queries,
    email::{send_password_setup, EmailConfig},
    models::types::{ApiError, AuditEntry, GlobalMetrics, Org, OrgWithStats, User},
    store::sqlite::SqliteStore,
};

// ── Error helpers ─────────────────────────────────────────────────────────────

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: "Database lock error".to_string(), code: "internal_error".to_string() }))
}

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string(), code: "internal_error".to_string() }))
}

fn unauthorized() -> (StatusCode, Json<ApiError>) {
    (StatusCode::UNAUTHORIZED, Json(ApiError { error: "Valid superuser key required".to_string(), code: "unauthorized".to_string() }))
}

// ── Superuser guard ───────────────────────────────────────────────────────────

fn require_superuser(
    superuser_key: &Option<String>,
    headers: &axum::http::HeaderMap,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let expected = superuser_key.as_deref().ok_or_else(unauthorized)?;
    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    if provided != expected {
        return Err(unauthorized());
    }
    Ok(())
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn get_metrics(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<GlobalMetrics>, (StatusCode, Json<ApiError>)> {
    require_superuser(&superuser_key, &headers)?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let metrics = queries::get_global_metrics(&conn).map_err(db_err)?;
    Ok(Json(metrics))
}

pub async fn list_orgs(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<OrgWithStats>>, (StatusCode, Json<ApiError>)> {
    require_superuser(&superuser_key, &headers)?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let orgs = queries::list_orgs_with_stats(&conn).map_err(db_err)?;
    Ok(Json(orgs))
}

#[derive(Deserialize)]
pub struct CreateOrgInput {
    pub org_name: String,
    pub org_slug: String,
    pub admin_email: String,
    pub admin_name: String,
}

pub async fn create_org(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    Extension(email_config): Extension<Option<Arc<EmailConfig>>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateOrgInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiError>)> {
    require_superuser(&superuser_key, &headers)?;

    let (org, user, api_key, raw_token) = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        let (org, user, api_key) = queries::create_org(
            &conn, &input.org_name, &input.org_slug, &input.admin_email, &input.admin_name,
        ).map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                (StatusCode::CONFLICT, Json(ApiError { error: "Organization slug already exists".to_string(), code: "slug_conflict".to_string() }))
            } else {
                db_err(e)
            }
        })?;
        let (raw_token, _) = queries::create_password_reset_token(&conn, &user.id).map_err(db_err)?;
        (org, user, api_key, raw_token)
    };

    if let Some(cfg) = email_config {
        let cfg = cfg.clone();
        let name = user.name.clone();
        let email = user.email.clone();
        let token = raw_token.clone();
        tokio::spawn(async move {
            if let Err(e) = send_password_setup(&cfg, &email, &name, &token).await {
                tracing::warn!("Failed to send org setup email to {email}: {e}");
            }
        });
    }

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "org": org, "user": user, "api_key": api_key }))))
}

#[derive(Deserialize)]
pub struct UpdateOrgInput {
    pub name: String,
}

pub async fn update_org(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
    Path(org_id): Path<String>,
    Json(input): Json<UpdateOrgInput>,
) -> Result<Json<Org>, (StatusCode, Json<ApiError>)> {
    require_superuser(&superuser_key, &headers)?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let org = queries::update_org_name(&conn, &org_id, &input.name).map_err(db_err)?;
    Ok(Json(org))
}

pub async fn list_org_users(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
    Path(org_id): Path<String>,
) -> Result<Json<Vec<User>>, (StatusCode, Json<ApiError>)> {
    require_superuser(&superuser_key, &headers)?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let users = queries::list_users(&conn, &org_id).map_err(db_err)?;
    Ok(Json(users))
}

pub async fn list_users(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<User>>, (StatusCode, Json<ApiError>)> {
    require_superuser(&superuser_key, &headers)?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let users = queries::list_all_users(&conn).map_err(db_err)?;
    Ok(Json(users))
}

#[derive(Deserialize)]
pub struct AuditParams {
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_audit(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<AuditParams>,
) -> Result<Json<Vec<AuditEntry>>, (StatusCode, Json<ApiError>)> {
    require_superuser(&superuser_key, &headers)?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0).max(0);
    let entries = queries::list_all_audit(
        &conn,
        params.action.as_deref(),
        params.resource_type.as_deref(),
        params.from.as_deref(),
        params.to.as_deref(),
        limit,
        offset,
    ).map_err(db_err)?;
    Ok(Json(entries))
}
