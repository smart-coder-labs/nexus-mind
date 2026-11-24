//! Runtime error monitoring API: the public intake endpoints (GCP Pub/Sub
//! push, Supabase/Vercel log drains) and the admin endpoints for sources and
//! error groups.

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
    monitor::delivery::{SERVER_GITHUB, SERVER_SLACK},
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
    .filter(|s| s.provider == "gcp")
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
    record_and_deliver(&store, &config, &source, vec![runtime_error])?;
    Ok(StatusCode::NO_CONTENT)
}

/// Groups each error and hands the alert/issue work to the background.
fn record_and_deliver(
    store: &SqliteStore,
    config: &Config,
    source: &MonitorSource,
    errors: Vec<crate::monitor::RuntimeError>,
) -> ApiResult<()> {
    let recorded = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_error())?;
        errors
            .iter()
            .map(|e| {
                let fingerprint = crate::monitor::fingerprint(&source.id, e);
                store_monitor::record(&conn, source, &fingerprint, e, chrono::Utc::now())
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(internal)?
    };
    for (action, group) in recorded {
        tokio::spawn(crate::monitor::delivery::deliver(
            store.clone(),
            source.clone(),
            action,
            group,
            config.app_base_url.clone(),
            config.claude_code_bin.clone(),
        ));
    }
    Ok(())
}

fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// `POST /v1/monitor/drain/:source_id`: a Supabase or Vercel log drain batch,
/// authenticated by the source's bearer token. Always 200 once authenticated:
/// Vercel disables a drain whose deliveries keep failing.
pub async fn drain_push(
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
    .filter(|s| s.provider != "gcp")
    .ok_or_else(not_found)?;
    let presented = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token_hash(token.trim()));
    // Both sides are hashes of the same length, so the comparison leaks nothing useful.
    if presented.is_none() || presented != source.ingest_token_hash {
        tracing::warn!(source = %source.id, "Monitor drain rejected: bad token");
        return Err(error(StatusCode::UNAUTHORIZED, "unauthorized", "Invalid drain token"));
    }
    if !source.enabled {
        return Ok(StatusCode::OK);
    }
    let gzip = headers
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("gzip"));
    let events = match crate::monitor::drains::events(&body, gzip) {
        Ok(events) => events,
        Err(reason) => {
            tracing::warn!(source = %source.id, "Monitor drain dropped: {reason}");
            return Ok(StatusCode::OK);
        }
    };
    let errors = crate::monitor::drains::errors(&source.provider, source.provider_ref.as_deref().unwrap_or_default(), &events);
    record_and_deliver(&store, &config, &source, errors)?;
    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------- sources

#[derive(Deserialize)]
pub struct SourceRequest {
    /// `gcp` (default), `supabase` or `vercel`; fixed after creation.
    pub provider: Option<String>,
    pub project: String,
    /// Supabase project ref or Vercel team slug (log links only).
    pub provider_ref: Option<String>,
    pub name: String,
    /// GCP only: the push subscription's service account.
    #[serde(default)]
    pub service_account: String,
    pub slack_connector_id: Option<String>,
    pub slack_channel: Option<String>,
    pub github_connector_id: Option<String>,
    pub github_repo: Option<String>,
    pub throttle_minutes: Option<i64>,
    pub enabled: Option<bool>,
    /// `claude`, `codex`, `pi` or `nexus`; absent or empty = no error audit.
    pub audit_executor: Option<String>,
    pub audit_model: Option<String>,
    /// The audit agent: `new` (or absent, with `audit_executor`) = one
    /// Monitoring creates; an id = an existing `error_audit` agent of the org.
    pub audit_agent: Option<String>,
    /// Hand each opened issue to the fix agent automatically.
    pub autofix: Option<bool>,
    /// The agent that resolves issues (automatically or from the "Resolve
    /// issue" button): absent = none, `new` = one Monitoring creates (server
    /// `gh` only), an id = an existing `github_issue_resolver` of the org.
    pub autofix_agent: Option<String>,
    /// Automatic audits and fixes per day (1-500, default 10). Manual fixes are not capped.
    pub audit_daily_limit: Option<i64>,
    pub autofix_daily_limit: Option<i64>,
}

/// The agents a source's settings ask for, applied after the source is saved.
pub struct AgentChoice {
    /// An existing audit agent to use as it is (`None`: managed or off).
    pub audit: Option<String>,
    pub fix: crate::monitor::autofix::FixAgent,
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

fn validate(conn: &rusqlite::Connection, org_id: &str, provider: &str, input: SourceRequest) -> ApiResult<(SourceWrite, AgentChoice)> {
    let invalid = |message: &str| error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", message);
    let name = input.name.trim().to_string();
    let project = input.project.trim().to_string();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(invalid("name must be 1-100 characters"));
    }
    if project.is_empty() || project.chars().count() > 100 {
        return Err(invalid("project must be 1-100 characters"));
    }
    if !crate::monitor::drains::PROVIDERS.contains(&provider) {
        return Err(invalid("provider must be gcp, supabase or vercel"));
    }
    // Drains authenticate with their token: no service account.
    let service_account = if provider == "gcp" { input.service_account.trim().to_ascii_lowercase() } else { String::new() };
    if provider == "gcp" && !valid_service_account(&service_account) {
        return Err(invalid("service_account must be a Google service account email"));
    }
    let provider_ref = blank_to_none(input.provider_ref.clone()).filter(|_| provider != "gcp");
    if provider_ref.as_ref().is_some_and(|r| r.len() > 100 || !r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')) {
        return Err(invalid("provider_ref must be a Supabase project ref or Vercel team slug"));
    }
    let throttle_minutes = input.throttle_minutes.unwrap_or(30);
    if !(1..=1440).contains(&throttle_minutes) {
        return Err(invalid("throttle_minutes must be between 1 and 1440"));
    }
    let limit = |value: Option<i64>, what: &str| -> ApiResult<i64> {
        let value = value.unwrap_or(10);
        if (1..=500).contains(&value) { Ok(value) } else { Err(invalid(&format!("{what} must be between 1 and 500"))) }
    };
    let audit_daily_limit = limit(input.audit_daily_limit, "audit_daily_limit")?;
    let autofix_daily_limit = limit(input.autofix_daily_limit, "autofix_daily_limit")?;
    // An existing agent of this org, enabled, of the given template.
    let agent = |id: &str, template: &str| -> ApiResult<crate::models::types::AutonomousAgentDetail> {
        crate::db::queries::get_autonomous_agent_detail(conn, org_id, id)
            .map_err(internal)?
            .filter(|a| a.definition.template_key == template && a.definition.status == "enabled")
            .ok_or_else(|| invalid(&format!("the chosen agent must be an enabled {template} agent of this organization")))
    };
    let audit_choice = blank_to_none(input.audit_agent).filter(|v| v != "new");
    let mut audit_executor = blank_to_none(input.audit_executor);
    let mut audit_model = blank_to_none(input.audit_model);
    if let Some(id) = &audit_choice {
        let chosen = agent(id, crate::monitor::audit::TEMPLATE)?;
        let owner = input.github_repo.as_deref().and_then(|r| r.trim().split_once('/')).map(|(o, _)| o.to_ascii_lowercase());
        if chosen.revision.config["github_owner"].as_str().map(str::to_ascii_lowercase) != owner {
            return Err(invalid("the chosen audit agent works on another GitHub owner than the issues repository"));
        }
        // Shown on the card; the chosen agent keeps its own settings.
        audit_executor = Some(chosen.revision.config["executor"].as_str().unwrap_or("claude").to_string());
        audit_model = None;
    }
    if let Some(executor) = &audit_executor {
        if !crate::monitor::audit::EXECUTORS.contains(&executor.as_str()) {
            return Err(invalid("audit_executor must be claude, codex, pi or nexus"));
        }
        if input.github_connector_id.as_deref().is_none_or(|v| v.trim().is_empty()) || input.github_repo.as_deref().is_none_or(|v| v.trim().is_empty()) {
            return Err(invalid("an error audit needs GitHub issues: its default repository sets the owner it may audit"));
        }
    }
    if audit_model.as_ref().is_some_and(|m| m.len() > 80 || audit_executor.is_none()) {
        return Err(invalid("audit_model needs an audit_executor and at most 80 characters"));
    }
    let slack_connector_id = blank_to_none(input.slack_connector_id);
    let server_allowed = crate::monitor::delivery::server_integrations_allowed(org_id);
    let not_authorized = || {
        error(
            StatusCode::FORBIDDEN,
            "server_integration_not_authorized",
            "The server's Slack and GitHub accounts are only available to the operator's organization",
        )
    };
    let github_connector_id = blank_to_none(input.github_connector_id);
    let github_repo = blank_to_none(input.github_repo);
    let connector_kind = |id: &str| -> ApiResult<Option<(String, serde_json::Value)>> {
        Ok(crate::db::queries::get_autonomous_agent_connector_secret(conn, org_id, id)
            .map_err(internal)?
            .map(|(c, _)| (c.kind, c.metadata)))
    };
    let mut slack_channel = None;
    if slack_connector_id.as_deref() == Some(SERVER_SLACK) {
        if !server_allowed {
            return Err(not_authorized());
        }
        let channel = blank_to_none(input.slack_channel).unwrap_or_default();
        if !crate::monitor::delivery::valid_slack_channel_id(&channel) {
            return Err(invalid("slack_channel must be a Slack channel ID like C0123ABCDE"));
        }
        slack_channel = Some(channel);
    } else if let Some(id) = &slack_connector_id {
        if !matches!(connector_kind(id)?, Some((kind, _)) if kind == "slack") {
            return Err(invalid("slack_connector_id must be a Slack connector of this organization"));
        }
    }
    match (&github_connector_id, &github_repo) {
        (None, None) => {}
        (Some(id), Some(repo)) if id == SERVER_GITHUB => {
            if !server_allowed {
                return Err(not_authorized());
            }
            if crate::automation::connectors::validate_repository(repo).is_err() {
                return Err(invalid("github_repo must be owner/repo"));
            }
        }
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
    let autofix = input.autofix.unwrap_or(false);
    use crate::monitor::autofix::FixAgent;
    let fix = match blank_to_none(input.autofix_agent).as_deref() {
        None if autofix => FixAgent::Managed,
        None => FixAgent::None,
        Some("new") => FixAgent::Managed,
        Some(id) => FixAgent::Existing(agent(id, crate::monitor::autofix::TEMPLATE)?.definition.id),
    };
    if fix != FixAgent::None && github_repo.is_none() {
        return Err(invalid("resolving issues needs GitHub issues"));
    }
    // A managed resolver publishes with the server's gh account: never on another org's credential.
    if fix == FixAgent::Managed && github_connector_id.as_deref() != Some(SERVER_GITHUB) {
        return Err(invalid("a new fix agent needs GitHub issues through the server's gh account; choose an existing resolver instead"));
    }
    let write = SourceWrite {
        project,
        name,
        service_account,
        slack_connector_id,
        slack_channel,
        github_connector_id,
        github_repo,
        throttle_minutes,
        enabled: input.enabled.unwrap_or(true),
        audit_executor,
        audit_model,
        provider: provider.to_string(),
        provider_ref,
        autofix,
        audit_daily_limit,
        autofix_daily_limit,
    };
    Ok((write, AgentChoice { audit: audit_choice, fix }))
}

/// Brings the source's audit and fix agents in line with its settings.
fn sync_audit(conn: &rusqlite::Connection, user_id: &str, source: &MonitorSource, choice: &AgentChoice) -> ApiResult<MonitorSource> {
    let agent = crate::monitor::audit::sync_agent(conn, user_id, source, choice.audit.as_deref()).map_err(|e| {
        error(StatusCode::UNPROCESSABLE_ENTITY, "audit_agent_invalid", format!("The error audit could not be set up: {e}"))
    })?;
    store_monitor::set_audit_agent(conn, &source.org_id, &source.id, agent.as_deref()).map_err(internal)?;
    let fixer = crate::monitor::autofix::sync_agent(conn, user_id, source, &choice.fix).map_err(|e| {
        error(StatusCode::UNPROCESSABLE_ENTITY, "autofix_agent_invalid", format!("Auto-fix could not be set up: {e}"))
    })?;
    store_monitor::set_autofix_agent(conn, &source.org_id, &source.id, fixer.as_deref()).map_err(internal)?;
    store_monitor::get_source(conn, &source.org_id, &source.id).map_err(internal)?.ok_or_else(not_found)
}

fn write_of(source: &MonitorSource) -> SourceWrite {
    SourceWrite {
        project: source.project.clone(),
        name: source.name.clone(),
        service_account: source.service_account.clone(),
        slack_connector_id: source.slack_connector_id.clone(),
        slack_channel: source.slack_channel.clone(),
        github_connector_id: source.github_connector_id.clone(),
        github_repo: source.github_repo.clone(),
        throttle_minutes: source.throttle_minutes,
        enabled: source.enabled,
        audit_executor: source.audit_executor.clone(),
        audit_model: source.audit_model.clone(),
        provider: source.provider.clone(),
        provider_ref: source.provider_ref.clone(),
        autofix: source.autofix,
        audit_daily_limit: source.audit_daily_limit,
        autofix_daily_limit: source.autofix_daily_limit,
    }
}

fn name_taken() -> (StatusCode, Json<ApiError>) {
    error(StatusCode::CONFLICT, "name_taken", "A monitor source with this name already exists")
}

/// `GET /v1/monitor/options`: what the connect form may offer this org.
pub async fn options(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    let server = crate::monitor::delivery::server_integrations_allowed(&auth.org_id);
    Ok(Json(serde_json::json!({"server_slack_mcp": server, "server_github": server})))
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
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    let provider = input.provider.clone().map(|p| p.trim().to_ascii_lowercase()).filter(|p| !p.is_empty()).unwrap_or_else(|| "gcp".into());
    let (write, choice) = validate(&conn, &auth.org_id, &provider, input)?;
    // A drain's bearer token is shown once, here; only its hash is stored.
    let token = (provider != "gcp").then(|| format!("nmd_{}", hex::encode(rand::random::<[u8; 24]>())));
    let source = store_monitor::create_source(&conn, &auth.org_id, &auth.user_id, &write, token.as_deref().map(token_hash).as_deref())
        .map_err(internal)?
        .ok_or_else(name_taken)?;
    match sync_audit(&conn, &auth.user_id, &source, &choice) {
        Ok(source) => {
            let mut body = serde_json::to_value(source).map_err(|e| internal(e.into()))?;
            if let Some(token) = token {
                body["ingest_token"] = token.into();
            }
            Ok((StatusCode::CREATED, Json(body)))
        }
        Err(error) => {
            // No half-configured source: it is created with its audit or not at all.
            let _ = store_monitor::delete_source(&conn, &auth.org_id, &source.id);
            Err(error)
        }
    }
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
    let previous = store_monitor::get_source(&conn, &auth.org_id, &id).map_err(internal)?.ok_or_else(not_found)?;
    let (write, choice) = validate(&conn, &auth.org_id, &previous.provider, input)?;
    if !store_monitor::update_source(&conn, &auth.org_id, &id, &write).map_err(internal)? {
        return Err(name_taken());
    }
    let source = store_monitor::get_source(&conn, &auth.org_id, &id).map_err(internal)?.ok_or_else(not_found)?;
    match sync_audit(&conn, &auth.user_id, &source, &choice) {
        Ok(source) => Ok(Json(source)),
        Err(error) => {
            // Keep the settings that still match the agent.
            let _ = store_monitor::update_source(&conn, &auth.org_id, &id, &write_of(&previous));
            Err(error)
        }
    }
}

#[derive(Deserialize)]
pub struct SourceEnabled {
    pub enabled: bool,
}

/// `PATCH /v1/monitor/sources/:id {enabled}`: pause or resume without
/// re-validating, so a source whose connectors broke can still be paused.
/// A paused source acknowledges pushes and drops them: no group, alert or issue.
pub async fn set_source_enabled(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<SourceEnabled>,
) -> ApiResult<Json<MonitorSource>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, PERMISSION)?;
    if !store_monitor::set_source_enabled(&conn, &auth.org_id, &id, input.enabled).map_err(internal)? {
        return Err(not_found());
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
    // Its managed agents stop with it (kept for the run history).
    if let Some(source) = store_monitor::get_source(&conn, &auth.org_id, &id).map_err(internal)? {
        for agent in [source.audit_agent_id, source.autofix_agent_id].into_iter().flatten() {
            let _ = crate::db::queries::set_autonomous_agent_status(&conn, &auth.org_id, &agent, "disabled");
        }
    }
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
        (Some(id), _) if id == SERVER_SLACK => {
            let text = format!(
                "✅ NexusMind monitoring test for *{}*. Errors from this source will be posted here.",
                crate::monitor::delivery::mcp_safe(&source.name)
            );
            let allowed = crate::monitor::delivery::server_integrations_allowed(&auth.org_id);
            match source.slack_channel.as_deref() {
                _ if !allowed => "not_authorized".into(),
                None => "channel_missing".into(),
                Some(channel) => match crate::monitor::delivery::send_slack_via_mcp(&config.claude_code_bin, channel, &text, None).await {
                    Ok(_) => "ok".into(),
                    Err(_) => "send_failed".into(),
                },
            }
        }
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

/// `POST /v1/monitor/groups/:id/fix`: the "Resolve issue" button. Hands the
/// error's issue (opened first if it has none) to the source's fix agent,
/// which opens a draft PR. By hand there is no daily limit.
pub async fn fix_group(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<MonitorGroup>> {
    let (source, mut group) = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_error())?;
        require_permission(&conn, &auth, None, PERMISSION)?;
        let group = store_monitor::get_group(&conn, &auth.org_id, &id).map_err(internal)?.ok_or_else(not_found)?;
        let source = store_monitor::get_source(&conn, &auth.org_id, &group.source_id).map_err(internal)?.ok_or_else(not_found)?;
        (source, group)
    };
    let invalid = |message: &str| error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", message);
    if source.autofix_agent_id.is_none() {
        return Err(invalid("choose the agent that resolves issues in the project settings first"));
    }
    let (Some(connector), Some(default_repo)) = (source.github_connector_id.clone(), source.github_repo.clone()) else {
        return Err(invalid("resolving issues needs GitHub issues"));
    };
    let repository = crate::monitor::delivery::issue_repository(&group, &source).unwrap_or(&default_repo).to_string();
    if group.issue_number.is_none() {
        let token = crate::monitor::delivery::github_token(&store, &auth.org_id, &connector).await.map_err(|e| {
            error(StatusCode::BAD_GATEWAY, "github_unavailable", format!("GitHub could not be reached: {e}"))
        })?;
        let number = crate::monitor::delivery::open_issue_in(&store, &source, &group, &token, &repository, "", None, true).await.map_err(|e| {
            error(StatusCode::BAD_GATEWAY, "issue_create_failed", format!("The issue could not be opened: {e:#}"))
        })?;
        group.issue_number = Some(number);
        group.issue_repo = Some(repository.clone());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    let number = group.issue_number.unwrap_or_default();
    let started = crate::monitor::autofix::start(&conn, &source, &group, &repository, number, true).map_err(internal)?;
    let current = store_monitor::get_group(&conn, &auth.org_id, &id).map_err(internal)?.ok_or_else(not_found)?;
    // Opening the issue may already have started an automatic fix.
    if !started && current.fix_status.as_deref() != Some("pending") {
        return Err(error(StatusCode::CONFLICT, "fix_not_started", "A fix for this error could not be started"));
    }
    Ok(Json(current))
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
