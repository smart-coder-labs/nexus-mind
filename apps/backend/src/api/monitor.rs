//! Runtime error monitoring API: the public Pub/Sub push endpoint and the
//! admin endpoints for sources and error groups.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::{require_permission, AppJson},
    config::Config,
    db::monitor::{self as store_monitor, MonitorGroup, MonitorSource, SourceWrite},
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: message.into(), code: code.to_string() }))
}

fn internal(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    tracing::error!("monitor: {e:#}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Internal error")
}

fn lock_error() -> (StatusCode, Json<ApiError>) {
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock failed")
}

/// Monitoring is org configuration: `settings:write` (admins pass it).
const PERMISSION: &str = "settings:write";

fn not_found() -> (StatusCode, Json<ApiError>) {
    error(StatusCode::NOT_FOUND, "not_found", "Not found")
}

// ---------------------------------------------------------------- push intake

/// `POST /v1/monitor/gcp/:source_id`: a Pub/Sub push. Pub/Sub retries any
/// non-2xx, so a body that can never be processed is acknowledged with 204.
pub async fn gcp_push(
    State(store): State<SqliteStore>,
    Extension(config): Extension<Arc<Config>>,
    Path(source_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<StatusCode> {
    let source = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_error())?;
        store_monitor::get_source_by_id(&conn, &source_id).map_err(internal)?
    }
    .ok_or_else(not_found)?;
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "unauthorized", "Missing push token"))?;
    crate::monitor::oidc::verify(token, &source.audience, &source.service_account).await.map_err(|reason| {
        tracing::warn!(source = %source.id, "Monitor push rejected: {reason}");
        error(StatusCode::UNAUTHORIZED, "unauthorized", "Invalid push token")
    })?;
    if !source.enabled {
        return Ok(StatusCode::NO_CONTENT);
    }
    let parsed = serde_json::from_slice(&body).map_err(|_| "body_not_json").and_then(|body| crate::monitor::parse_push(&body));
    let runtime_error = match parsed {
        Ok(Some(e)) => e,
        Ok(None) => return Ok(StatusCode::NO_CONTENT),
        Err(reason) => {
            tracing::warn!(source = %source.id, "Monitor push dropped: {reason}");
            return Ok(StatusCode::NO_CONTENT);
        }
    };
    let fingerprint = crate::monitor::fingerprint(&source.id, &runtime_error);
    let (action, group) = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_error())?;
        store_monitor::record(&conn, &source, &fingerprint, &runtime_error, chrono::Utc::now()).map_err(internal)?
    };
    tokio::spawn(crate::monitor::delivery::deliver(store.clone(), source, action, group, config.app_base_url.clone()));
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- sources

#[derive(Deserialize)]
pub struct SourceRequest {
    pub project: String,
    pub name: String,
    pub service_account: String,
    pub slack_connector_id: Option<String>,
    pub github_connector_id: Option<String>,
    pub github_repo: Option<String>,
    pub throttle_minutes: Option<i64>,
    pub enabled: Option<bool>,
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

pub fn valid_service_account(email: &str) -> bool {
    let Some((name, domain)) = email.split_once('@') else { return false };
    !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && (domain.ends_with(".iam.gserviceaccount.com") || domain == "developer.gserviceaccount.com")
        && domain.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
}

fn validate(conn: &rusqlite::Connection, org_id: &str, input: SourceRequest) -> ApiResult<SourceWrite> {
    let invalid = |message: &str| error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", message);
    let name = input.name.trim().to_string();
    let project = input.project.trim().to_string();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(invalid("name must be 1-100 characters"));
    }
    if project.is_empty() || project.chars().count() > 100 {
        return Err(invalid("project must be 1-100 characters"));
    }
    let service_account = input.service_account.trim().to_ascii_lowercase();
    if !valid_service_account(&service_account) {
        return Err(invalid("service_account must be a Google service account email"));
    }
    let throttle_minutes = input.throttle_minutes.unwrap_or(30);
    if !(1..=1440).contains(&throttle_minutes) {
        return Err(invalid("throttle_minutes must be between 1 and 1440"));
    }
    let slack_connector_id = blank_to_none(input.slack_connector_id);
    let github_connector_id = blank_to_none(input.github_connector_id);
    let github_repo = blank_to_none(input.github_repo);
    let connector_kind = |id: &str| -> ApiResult<Option<(String, serde_json::Value)>> {
        Ok(crate::db::queries::get_autonomous_agent_connector_secret(conn, org_id, id)
            .map_err(internal)?
            .map(|(c, _)| (c.kind, c.metadata)))
    };
    if let Some(id) = &slack_connector_id {
        if !matches!(connector_kind(id)?, Some((kind, _)) if kind == "slack") {
            return Err(invalid("slack_connector_id must be a Slack connector of this organization"));
        }
    }
    match (&github_connector_id, &github_repo) {
        (None, None) => {}
        (Some(id), Some(repo)) => {
            let github = matches!(connector_kind(id)?, Some((kind, meta))
                if kind == "github_app" || (kind == "target_secret" && meta["purpose"] == "github_token"));
            if !github {
                return Err(invalid("github_connector_id must be a GitHub App or token connector of this organization"));
            }
            if crate::automation::connectors::validate_repository(repo).is_err() {
                return Err(invalid("github_repo must be owner/repo"));
            }
        }
        _ => return Err(invalid("github_connector_id and github_repo go together")),
    }
    Ok(SourceWrite {
        project,
        name,
        service_account,
        slack_connector_id,
        github_connector_id,
        github_repo,
        throttle_minutes,
        enabled: input.enabled.unwrap_or(true),
    })
}

fn name_taken() -> (StatusCode, Json<ApiError>) {
    error(StatusCode::CONFLICT, "name_taken", "A monitor source with this name already exists")
}

pub async fn list_sources(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<MonitorSource>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    store_monitor::list_sources(&conn, &auth.org_id).map(Json).map_err(internal)
}

pub async fn create_source(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<SourceRequest>,
) -> ApiResult<(StatusCode, Json<MonitorSource>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    let write = validate(&conn, &auth.org_id, input)?;
    let source = store_monitor::create_source(&conn, &auth.org_id, &auth.user_id, &write)
        .map_err(internal)?
        .ok_or_else(name_taken)?;
    Ok((StatusCode::CREATED, Json(source)))
}

pub async fn update_source(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<SourceRequest>,
) -> ApiResult<Json<MonitorSource>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    store_monitor::get_source(&conn, &auth.org_id, &id).map_err(internal)?.ok_or_else(not_found)?;
    let write = validate(&conn, &auth.org_id, input)?;
    if !store_monitor::update_source(&conn, &auth.org_id, &id, &write).map_err(internal)? {
        return Err(name_taken());
    }
    store_monitor::get_source(&conn, &auth.org_id, &id).map_err(internal)?.map(Json).ok_or_else(not_found)
}

pub async fn delete_source(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    if store_monitor::delete_source(&conn, &auth.org_id, &id).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// `POST /v1/monitor/sources/:id/test`: sends a Slack message and checks the
/// GitHub repository is readable, without creating a group or an issue.
pub async fn test_source(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Extension(config): Extension<Arc<Config>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let (source, webhook) = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_error())?;
        require_permission(&conn, &auth, None, PERMISSION)?;
        let source = store_monitor::get_source(&conn, &auth.org_id, &id).map_err(internal)?.ok_or_else(not_found)?;
        let webhook = match source.slack_connector_id.as_deref() {
            Some(cid) => crate::db::queries::get_autonomous_agent_connector_secret(&conn, &auth.org_id, cid)
                .map_err(internal)?
                .filter(|(c, _)| c.kind == "slack")
                .map(|(_, secret)| secret),
            None => None,
        };
        (source, webhook)
    };
    let slack = match (&source.slack_connector_id, webhook) {
        (None, _) => "not_configured".to_string(),
        (Some(_), None) => "connector_unavailable".to_string(),
        (Some(_), Some(webhook)) => {
            let text = format!(
                "✅ NexusMind monitoring test for *{}*. Errors from this source will be posted here.",
                crate::monitor::delivery::slack_escape(&source.name)
            );
            let link = format!("{}/monitoring", config.app_base_url.trim_end_matches('/'));
            match crate::automation::connectors::send_slack(&webhook, &text, &link).await {
                Ok(()) => "ok".into(),
                Err(_) => "send_failed".into(),
            }
        }
    };
    let github = match (source.github_connector_id.as_deref(), source.github_repo.as_deref()) {
        (Some(cid), Some(repo)) => {
            match crate::monitor::delivery::github_token(&store, &auth.org_id, cid).await {
                Ok(token) => match crate::automation::connectors::get_github_repository(&token, repo).await {
                    Ok(_) => "ok",
                    Err(_) => "repository_unreadable",
                },
                Err(_) => "connector_unavailable",
            }
        }
        _ => "not_configured",
    };
    Ok(Json(serde_json::json!({"slack": slack, "github": github})))
}

// ---------------------------------------------------------------- groups

#[derive(Deserialize)]
pub struct GroupQuery {
    pub source_id: Option<String>,
    pub limit: Option<i64>,
}

pub async fn list_groups(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<GroupQuery>,
) -> ApiResult<Json<Vec<MonitorGroup>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    store_monitor::list_groups(&conn, &auth.org_id, query.source_id.as_deref(), limit).map(Json).map_err(internal)
}

#[derive(Deserialize)]
pub struct GroupStatus {
    pub status: String,
}

pub async fn set_group_status(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<GroupStatus>,
) -> ApiResult<StatusCode> {
    if !matches!(input.status.as_str(), "open" | "resolved" | "muted") {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", "status must be open, resolved or muted"));
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    if store_monitor::set_group_status(&conn, &auth.org_id, &id, &input.status).map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

#[cfg(test)]
mod tests {
    use super::valid_service_account;

    #[test]
    fn only_google_service_accounts_are_accepted() {
        assert!(valid_service_account("push@my-proj.iam.gserviceaccount.com"));
        assert!(valid_service_account("123-compute@developer.gserviceaccount.com"));
        assert!(!valid_service_account("me@gmail.com"));
        assert!(!valid_service_account("push@evil.com.iam.gserviceaccount.com.evil"));
        assert!(!valid_service_account("@x.iam.gserviceaccount.com"));
        assert!(!valid_service_account("a b@x.iam.gserviceaccount.com"));
    }
}
