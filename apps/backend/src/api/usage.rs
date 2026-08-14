//! Usage-metrics HTTP surface: ingest, summary rollup, and session backfill.
//!
//! Auth gates (design.md §API):
//! - `POST /v1/usage`          → `require_permission(project, "memory:write")`
//! - `GET  /v1/usage/summary`  → privileged (admin or super_user); admin is
//!   scoped to visible projects, super_user is org-wide.
//! - `POST /v1/usage/backfill` → super_user only.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::{require_permission, AppJson},
    db::usage_queries,
    models::types::{ApiError, AuthContext, UsageIngestRequest, UsageSummaryResponse},
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

fn bad_request(msg: &str, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: msg.to_string(),
            code: code.to_string(),
        }),
    )
}

fn forbidden() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "Access denied".to_string(),
            code: "forbidden".to_string(),
        }),
    )
}

/// `POST /v1/usage` — record one usage event. Returns `201 { id }`.
pub async fn ingest(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(req): AppJson<UsageIngestRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    // Same authority an agent already needs to write to that project.
    require_permission(&conn, &auth, req.project.as_deref(), "memory:write")?;

    let id = usage_queries::insert_usage_event(&conn, &auth.org_id, &auth.user_id, &req)
        .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

fn default_level() -> String {
    "project".to_string()
}

#[derive(Deserialize)]
pub struct SummaryParams {
    #[serde(default = "default_level")]
    pub level: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub client_id: Option<String>,
    pub project_id: Option<String>,
}

const SUMMARY_LEVELS: [&str; 4] = ["task", "project", "client", "org"];

/// `GET /v1/usage/summary` — aggregated rollup at the requested level.
pub async fn summary(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<UsageSummaryResponse>, (StatusCode, Json<ApiError>)> {
    if !SUMMARY_LEVELS.contains(&params.level.as_str()) {
        return Err(bad_request(
            &format!("level must be one of {}", SUMMARY_LEVELS.join(", ")),
            "invalid_level",
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // Summary is admin/super_user only. Admin stays membership-scoped; only
    // super_user reads org-wide.
    if !auth.role.is_privileged() {
        return Err(forbidden());
    }
    let viewer = if auth.role.is_super_user() {
        None
    } else {
        Some(auth.user_id.as_str())
    };

    let resp = usage_queries::usage_summary(
        &conn,
        &auth.org_id,
        &params.level,
        params.from.as_deref(),
        params.to.as_deref(),
        params.client_id.as_deref(),
        params.project_id.as_deref(),
        viewer,
    )
    .map_err(db_err)?;
    Ok(Json(resp))
}

/// `POST /v1/usage/backfill` — derive one usage row per session lacking one.
pub async fn backfill(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    if !auth.role.is_super_user() {
        return Err(forbidden());
    }

    let inserted = usage_queries::backfill_from_sessions(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(serde_json::json!({ "inserted": inserted })))
}
