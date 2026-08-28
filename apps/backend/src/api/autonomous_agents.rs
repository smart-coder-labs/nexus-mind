use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    api::helpers::{require_explicit_permission, AppJson},
    config::Config,
    db::queries,
    models::types::{
        ApiError, AuthContext, AutonomousAgentConnector, AutonomousAgentDefinition,
        AutonomousAgentDelivery, AutonomousAgentDetail, AutonomousAgentEvent,
        AutonomousAgentFinding, AutonomousAgentMetrics, AutonomousAgentOrgSettings,
        AutonomousAgentRun, AutonomousAgentSchedule, AutonomousAgentTarget,
        AutonomousAgentTemplate, CreateAutonomousAgentRequest, PatchAutonomousAgentFindingRequest,
        PatchAutonomousAgentOrgSettingsRequest, PutAutonomousAgentConnectorRequest,
        PutAutonomousAgentScheduleRequest, PutAutonomousAgentTargetRequest,
        UpdateAutonomousAgentRequest,
    },
    store::sqlite::SqliteStore,
};

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

fn lock_error() -> (StatusCode, Json<ApiError>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Database lock error",
    )
}

fn error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.to_string(),
            code: code.to_string(),
        }),
    )
}

fn store_error(value: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let message = value.to_string();
    let (status, code) = match message.as_str() {
        "invalid_template"
        | "invalid_name"
        | "invalid_configuration"
        | "invalid_status"
        | "invalid_schedule_kind"
        | "schedule_expression_required"
        | "interval_too_short"
        | "invalid_timezone"
        | "invalid_daily_time"
        | "invalid_misfire_policy" => (StatusCode::UNPROCESSABLE_ENTITY, message.as_str()),
        "invalid_connector_kind" | "invalid_connector" | "invalid_connector_secret" => {
            (StatusCode::UNPROCESSABLE_ENTITY, message.as_str())
        }
        "invalid_github_app_metadata"
        | "invalid_github_app_secret"
        | "invalid_slack_webhook"
        | "invalid_target_kind"
        | "invalid_target"
        | "invalid_target_connector"
        | "invalid_finding_status"
        | "invalid_retention_days" => (StatusCode::UNPROCESSABLE_ENTITY, message.as_str()),
        "encryption_required" => (StatusCode::SERVICE_UNAVAILABLE, message.as_str()),
        "validation_required"
        | "agent_archived"
        | "agent_not_enabled"
        | "agent_must_be_disabled"
        | "agent_has_active_runs" => (StatusCode::CONFLICT, message.as_str()),
        _ if message.contains("UNIQUE constraint failed") => {
            (StatusCode::CONFLICT, "agent_name_exists")
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    };
    error(
        status,
        code,
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            "Database error"
        } else {
            &message
        },
    )
}

fn not_found() -> (StatusCode, Json<ApiError>) {
    error(
        StatusCode::NOT_FOUND,
        "not_found",
        "Autonomous agent not found",
    )
}

pub fn managed_templates() -> Vec<AutonomousAgentTemplate> {
    vec![
        AutonomousAgentTemplate {
            key: "lead_generation".into(),
            version: 1,
            name: "Lead Generation".into(),
            description: "Finds companies matching your ICP via web search and drafts personalized outreach emails for your review — it never sends anything.".into(),
            capabilities: vec![
                "web:search".into(),
                "lead:write".into(),
                "delivery:write".into(),
            ],
            default_budgets: serde_json::json!({"wall_time_seconds": 1800, "max_attempts": 1, "max_cost_usd": 10, "max_definition_concurrency": 1, "max_organization_concurrency": 4}),
            config_schema: serde_json::json!({"outputs":{"type":"array","items":["nexusmind","slack"]},"product":{"type":"string","required":true,"description":"What you sell — name plus a one-line value prop or URL"},"icp":{"type":"string","required":true,"description":"Ideal customer profile: industry, size, role, geography to target"},"count":{"type":"integer","default":10,"description":"How many leads to find per run"},"custom_instructions":{"type":"string"}}),
            workflow: vec![
                "search".into(),
                "qualify".into(),
                "draft_outreach".into(),
                "record_leads".into(),
            ],
        },
        AutonomousAgentTemplate {
            key: "qa".into(),
            version: 1,
            name: "QA".into(),
            description: "Runs bounded tests and records canonical findings.".into(),
            capabilities: vec![
                "repository:read".into(),
                "tests:run".into(),
                "finding:write".into(),
                "delivery:write".into(),
            ],
            default_budgets: serde_json::json!({"wall_time_seconds": 1800, "max_attempts": 2, "max_cost_usd": 10, "max_definition_concurrency": 1, "max_repository_concurrency": 1, "max_organization_concurrency": 4}),
            config_schema: serde_json::json!({"outputs":{"type":"array","items":["nexusmind","slack","github_issue"]},"repository":{"type":"owner/repo"},"server_integrations":{"github":"gh_cli","slack":"claude_mcp:slack"},"test_adapter":{"enum":["playwright","allowlisted_command"]},"test_command":{"type":"argv","shell":false}}),
            workflow: vec![
                "checkout".into(),
                "health_check".into(),
                "test".into(),
                "evaluate".into(),
                "record_findings".into(),
                "deliver".into(),
            ],
        },
        AutonomousAgentTemplate {
            key: "github_issue_resolver".into(),
            version: 1,
            name: "GitHub Issue Resolver".into(),
            description: "Implements eligible issues and opens tested draft pull requests.".into(),
            capabilities: vec![
                "repository:read".into(),
                "repository:branch".into(),
                "tests:run".into(),
                "github:draft_pr".into(),
            ],
            default_budgets: serde_json::json!({"wall_time_seconds": 3600, "max_attempts": 2, "max_cost_usd": 20, "max_changed_files": 20, "max_changed_lines": 800, "max_definition_concurrency": 1, "max_repository_concurrency": 1, "max_organization_concurrency": 4}),
            config_schema: serde_json::json!({"repository":{"type":"owner/repo","required":true},"github_auth":{"const":"server_gh_cli"},"base_branch":{"type":"string","default":"main"},"context_repos":{"type":"array","items":{"type":"owner/repo"},"description":"Additional repos of the same project cloned read-only for cross-repo context"},"custom_instructions":{"type":"string","description":"Optional free-text guidance for how to approach the issue; cannot expand scope"},"labels":{"type":"array"},"excluded_paths":{"type":"array"},"limits":{"type":"object"}}),
            workflow: vec![
                "eligible_issue".into(),
                "pinned_checkout".into(),
                "bounded_edit".into(),
                "verify".into(),
                "authority_recheck".into(),
                "draft_pr".into(),
            ],
        },
        AutonomousAgentTemplate {
            key: "github_pr_reviewer".into(),
            version: 1,
            name: "GitHub PR Reviewer".into(),
            description: "Reviews a pinned pull-request head without approving or merging.".into(),
            capabilities: vec![
                "repository:read".into(),
                "tests:run".into(),
                "github:review".into(),
            ],
            default_budgets: serde_json::json!({"wall_time_seconds": 1800, "max_attempts": 1, "max_cost_usd": 10, "max_changed_lines": 1200, "max_definition_concurrency": 1, "max_repository_concurrency": 1, "max_organization_concurrency": 4}),
            config_schema: serde_json::json!({"repository":{"type":"owner/repo","required":true},"github_auth":{"const":"server_gh_cli"},"publish":{"enum":["comment_or_request_changes"]},"include_drafts":{"type":"boolean","default":false},"custom_instructions":{"type":"string","description":"Optional free-text guidance for what the review should focus on; cannot approve, merge, or publish"}}),
            workflow: vec![
                "pin_head".into(),
                "bounded_diff".into(),
                "optional_checks".into(),
                "evaluate".into(),
                "head_recheck".into(),
                "comment_or_request_changes".into(),
            ],
        },
        AutonomousAgentTemplate {
            key: "judge".into(),
            version: 1,
            name: "Judge".into(),
            description: "Verifies whether the given PRs/issues actually delivered their claim, testing only what they touch against the live application, and records findings with evidence.".into(),
            capabilities: vec![
                "repository:read".into(),
                "tests:run".into(),
                "finding:write".into(),
                "delivery:write".into(),
                "github:review".into(),
            ],
            default_budgets: serde_json::json!({"wall_time_seconds": 1800, "max_attempts": 1, "max_cost_usd": 12, "max_definition_concurrency": 1, "max_repository_concurrency": 1, "max_organization_concurrency": 4}),
            config_schema: serde_json::json!({"outputs":{"type":"array","items":["nexusmind","slack"]},"repository":{"type":"owner/repo","required":true,"description":"Repo the PRs/issues live in — read via gh to scope what each claim touches"},"github_auth":{"const":"server_gh_cli"},"judge_targets":{"type":"array","required":true,"items":{"type":{"enum":["pr","issue"]},"number":{"type":"integer"}},"description":"The PRs/issues to judge this run"},"publish":{"enum":["none","comment"],"default":"none","description":"Whether to post the verdict as a GitHub comment on each target"},"server_integrations":{"github":"gh_cli","slack":"claude_mcp:slack"},"custom_instructions":{"type":"string","description":"Optional guidance for what to prioritize; cannot expand scope"}}),
            workflow: vec![
                "select_pr_issue".into(),
                "scope_to_diff".into(),
                "drive_live_app".into(),
                "evaluate".into(),
                "record_findings".into(),
                "deliver".into(),
            ],
        },
    ]
}

pub async fn list_templates(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<AutonomousAgentTemplate>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(managed_templates()))
}

pub async fn list_definitions(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<AutonomousAgentDefinition>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::list_autonomous_agent_definitions(&conn, &auth.org_id).map_err(store_error)?,
    ))
}

pub async fn get_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentDetail>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::get_autonomous_agent_detail(&conn, &auth.org_id, &id)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn create_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(req): AppJson<CreateAutonomousAgentRequest>,
) -> ApiResult<(StatusCode, Json<AutonomousAgentDetail>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:create")?;
    let value =
        queries::create_autonomous_agent_definition(&conn, &auth.org_id, &auth.user_id, &req)
            .map_err(store_error)?;
    Ok((StatusCode::CREATED, Json(value)))
}

pub async fn update_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(req): AppJson<UpdateAutonomousAgentRequest>,
) -> ApiResult<Json<AutonomousAgentDetail>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:update")?;
    Ok(Json(
        queries::update_autonomous_agent_definition(&conn, &auth.org_id, &auth.user_id, &id, &req)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn validate_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentDetail>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:enable")?;
    Ok(Json(
        queries::validate_autonomous_agent_definition(&conn, &auth.org_id, &auth.user_id, &id)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

async fn set_status(
    store: SqliteStore,
    auth: AuthContext,
    id: String,
    status: &str,
) -> ApiResult<Json<AutonomousAgentDetail>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(
        &conn,
        &auth,
        None,
        if status == "archived" {
            "autonomous_agent:update"
        } else {
            "autonomous_agent:enable"
        },
    )?;
    Ok(Json(
        queries::set_autonomous_agent_status(&conn, &auth.org_id, &id, status)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn enable_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentDetail>> {
    set_status(store, auth, id, "enabled").await
}
pub async fn disable_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentDetail>> {
    set_status(store, auth, id, "disabled").await
}
pub async fn archive_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentDetail>> {
    set_status(store, auth, id, "archived").await
}

pub async fn get_runtime_health(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<crate::automation::runtime::RuntimeHealth>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    let value = queries::get_autonomous_runtime_health(&conn)
        .map_err(store_error)?
        .unwrap_or(crate::automation::runtime::RuntimeHealth {
            status: "unavailable".into(),
            reason_code: Some("runtime_not_checked".into()),
            claude_version: None,
            checked_at: None,
            last_success_at: None,
            last_failure_at: None,
        });
    Ok(Json(value))
}

pub async fn check_runtime_health(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Extension(config): Extension<Arc<Config>>,
) -> ApiResult<Json<crate::automation::runtime::RuntimeHealth>> {
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_error())?;
        require_explicit_permission(&conn, &auth, None, "autonomous_agent:enable")?;
    }
    let probed = crate::automation::runtime::probe_claude(&config.claude_code_bin).await;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    queries::save_autonomous_runtime_health(&conn, &probed).map_err(store_error)?;
    Ok(Json(
        queries::get_autonomous_runtime_health(&conn)
            .map_err(store_error)?
            .unwrap_or(probed),
    ))
}

pub async fn get_org_settings(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<AutonomousAgentOrgSettings>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::get_autonomous_agent_org_settings(&conn, &auth.org_id)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn get_metrics(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<AutonomousAgentMetrics>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::get_autonomous_agent_metrics(&conn, &auth.org_id).map_err(store_error)?,
    ))
}

pub async fn patch_org_settings(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(req): AppJson<PatchAutonomousAgentOrgSettingsRequest>,
) -> ApiResult<Json<AutonomousAgentOrgSettings>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:enable")?;
    Ok(Json(
        queries::patch_autonomous_agent_org_settings(
            &conn,
            &auth.org_id,
            req.enabled,
            req.retention_days,
        )
        .map_err(store_error)?
        .ok_or_else(not_found)?,
    ))
}

pub async fn get_schedule(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentSchedule>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::get_autonomous_agent_schedule(&conn, &auth.org_id, &id)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn put_schedule(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(req): AppJson<PutAutonomousAgentScheduleRequest>,
) -> ApiResult<Json<AutonomousAgentSchedule>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:update")?;
    Ok(Json(
        queries::put_autonomous_agent_schedule(&conn, &auth.org_id, &id, &req)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn run_now(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<(StatusCode, Json<AutonomousAgentRun>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:run")?;
    let occurrence = format!("manual:{}", uuid::Uuid::new_v4());
    let run = queries::enqueue_autonomous_agent_run(
        &conn,
        &auth.org_id,
        &id,
        "manual",
        &occurrence,
        None,
    )
    .map_err(store_error)?
    .ok_or_else(not_found)?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

#[derive(Deserialize)]
pub struct RunFilters {
    definition_id: Option<String>,
}

pub async fn list_runs(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(filters): Query<RunFilters>,
) -> ApiResult<Json<Vec<AutonomousAgentRun>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::list_autonomous_agent_runs(&conn, &auth.org_id, filters.definition_id.as_deref())
            .map_err(store_error)?,
    ))
}

pub async fn get_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentRun>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::get_autonomous_agent_run(&conn, &auth.org_id, &id)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn cancel_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentRun>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:cancel")?;
    Ok(Json(
        queries::cancel_autonomous_agent_run(&conn, &auth.org_id, &id)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn list_run_events(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<AutonomousAgentEvent>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    if queries::get_autonomous_agent_run(&conn, &auth.org_id, &id)
        .map_err(store_error)?
        .is_none()
    {
        return Err(not_found());
    }
    Ok(Json(
        queries::list_autonomous_agent_events(&conn, &auth.org_id, &id).map_err(store_error)?,
    ))
}

#[derive(serde::Deserialize)]
pub struct TranscriptQuery {
    /// Return turns with sequence strictly greater than this (0 = from start).
    #[serde(default)]
    pub after: i64,
    /// Max turns to return in this page (clamped server-side).
    pub limit: Option<i64>,
}

/// Full turn-by-turn transcript of a run's Claude conversation, paginated by
/// sequence so the UI can poll incrementally while the run streams.
pub async fn list_run_transcript(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Query(query): Query<TranscriptQuery>,
) -> ApiResult<Json<Vec<AutonomousAgentEvent>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    if queries::get_autonomous_agent_run(&conn, &auth.org_id, &id)
        .map_err(store_error)?
        .is_none()
    {
        return Err(not_found());
    }
    let limit = query.limit.unwrap_or(2000).clamp(1, 5000);
    Ok(Json(
        queries::list_autonomous_agent_transcript(
            &conn,
            &auth.org_id,
            &id,
            query.after.max(0),
            limit,
        )
        .map_err(store_error)?,
    ))
}

pub async fn list_findings(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<AutonomousAgentFinding>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::list_autonomous_agent_findings(&conn, &auth.org_id).map_err(store_error)?,
    ))
}

pub async fn patch_finding(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(req): AppJson<PatchAutonomousAgentFindingRequest>,
) -> ApiResult<Json<AutonomousAgentFinding>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:update")?;
    Ok(Json(
        queries::patch_autonomous_agent_finding(&conn, &auth.org_id, &id, &req.status)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn list_deliveries(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<AutonomousAgentDelivery>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::list_autonomous_agent_deliveries(&conn, &auth.org_id).map_err(store_error)?,
    ))
}

pub async fn retry_delivery(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<AutonomousAgentDelivery>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:run")?;
    Ok(Json(
        queries::retry_autonomous_agent_delivery(&conn, &auth.org_id, &id)
            .map_err(store_error)?
            .ok_or_else(not_found)?,
    ))
}

pub async fn list_targets(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<AutonomousAgentTarget>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    if queries::get_autonomous_agent_definition(&conn, &auth.org_id, &id)
        .map_err(store_error)?
        .is_none()
    {
        return Err(not_found());
    }
    Ok(Json(
        queries::list_autonomous_agent_targets(&conn, &auth.org_id, &id).map_err(store_error)?,
    ))
}

pub async fn put_target(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(req): AppJson<PutAutonomousAgentTargetRequest>,
) -> ApiResult<(StatusCode, Json<AutonomousAgentTarget>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:update")?;
    let value = queries::put_autonomous_agent_target(&conn, &auth.org_id, &id, &req)
        .map_err(store_error)?
        .ok_or_else(not_found)?;
    Ok((StatusCode::CREATED, Json(value)))
}

pub async fn list_connectors(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<AutonomousAgentConnector>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        queries::list_autonomous_agent_connectors(&conn, &auth.org_id).map_err(store_error)?,
    ))
}

pub async fn put_connector(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(req): AppJson<PutAutonomousAgentConnectorRequest>,
) -> ApiResult<Json<AutonomousAgentConnector>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:manage_connectors")?;
    Ok(Json(
        queries::put_autonomous_agent_connector(&conn, &auth.org_id, &auth.user_id, &req)
            .map_err(store_error)?,
    ))
}

pub async fn revoke_connector(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "autonomous_agent:manage_connectors")?;
    if !queries::revoke_autonomous_agent_connector(&conn, &auth.org_id, &id).map_err(store_error)? {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    fn fixture() -> (rusqlite::Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        (conn, org.id, user.id)
    }

    fn request() -> CreateAutonomousAgentRequest {
        CreateAutonomousAgentRequest {
            name: "Daily QA".into(),
            description: Some("QA agent".into()),
            template_key: "qa".into(),
            config: serde_json::json!({"outputs": ["nexusmind"]}),
            budgets: serde_json::json!({"wall_time_seconds": 300}),
        }
    }

    #[test]
    fn create_is_disabled_and_requires_exact_revision_validation() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        assert_eq!(created.definition.status, "disabled");
        assert_eq!(created.revision.validation_status, "pending");
        assert!(queries::set_autonomous_agent_status(
            &conn,
            &org_id,
            &created.definition.id,
            "enabled"
        )
        .is_err());

        let validated = queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap()
        .unwrap();
        assert_eq!(validated.revision.validation_status, "valid");
        let enabled =
            queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
                .unwrap()
                .unwrap();
        assert_eq!(enabled.definition.status, "enabled");
    }

    #[test]
    fn material_edit_appends_revision_and_disables() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        let edited = queries::update_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
            &UpdateAutonomousAgentRequest {
                name: None,
                description: None,
                config: Some(serde_json::json!({"outputs": ["nexusmind", "slack"]})),
                budgets: None,
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(edited.definition.status, "disabled");
        assert_eq!(edited.definition.current_revision, 2);
        assert_eq!(edited.revision.validation_status, "pending");
    }

    #[test]
    fn cross_org_lookup_is_not_found() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        assert!(
            queries::get_autonomous_agent_detail(&conn, "other-org", &created.definition.id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn occurrence_finding_and_cancellation_are_idempotent() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        let run = queries::enqueue_autonomous_agent_run(
            &conn,
            &org_id,
            &created.definition.id,
            "manual",
            "same-occurrence",
            None,
        )
        .unwrap()
        .unwrap();
        assert!(queries::enqueue_autonomous_agent_run(
            &conn,
            &org_id,
            &created.definition.id,
            "manual",
            "same-occurrence",
            None
        )
        .is_err());
        let first = queries::upsert_autonomous_agent_finding(
            &conn,
            &org_id,
            &created.definition.id,
            &run.id,
            "stable",
            "Failure",
            "high",
            "Summary",
            &serde_json::json!({"bounded":true}),
        )
        .unwrap();
        let second = queries::upsert_autonomous_agent_finding(
            &conn,
            &org_id,
            &created.definition.id,
            &run.id,
            "stable",
            "Failure",
            "high",
            "Summary",
            &serde_json::json!({"bounded":true}),
        )
        .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.occurrence_count, 2);
        assert_eq!(
            queries::cancel_autonomous_agent_run(&conn, &org_id, &run.id)
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
        assert_eq!(
            queries::cancel_autonomous_agent_run(&conn, &org_id, &run.id)
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
    }

    #[test]
    fn github_delivery_replay_is_idempotent_and_connector_scope_is_enforced() {
        let (conn, org_id, user_id) = fixture();
        conn.execute(
            "INSERT INTO autonomous_agent_connectors
             (id,org_id,kind,name,secret_ciphertext,metadata_json,scopes_json,health,created_by)
             VALUES('github-test',?1,'github_app','GitHub','cipher','{}','[]','ready',?2)",
            rusqlite::params![org_id, user_id],
        )
        .unwrap();
        assert!(queries::record_github_webhook_delivery(
            &conn,
            &org_id,
            "github-test",
            "delivery-1",
            "issues",
            Some("opened"),
            Some("acme/api"),
            "hash-a",
        )
        .unwrap());
        assert!(!queries::record_github_webhook_delivery(
            &conn,
            &org_id,
            "github-test",
            "delivery-1",
            "issues",
            Some("opened"),
            Some("acme/api"),
            "hash-a",
        )
        .unwrap());
        assert!(queries::record_github_webhook_delivery(
            &conn,
            &org_id,
            "github-test",
            "delivery-1",
            "issues",
            Some("opened"),
            Some("acme/api"),
            "hash-b",
        )
        .is_err());
        conn.execute(
            "INSERT INTO organizations(id,name,slug) VALUES('other-org','Other','other')",
            [],
        )
        .unwrap();
        assert!(queries::record_github_webhook_delivery(
            &conn,
            "other-org",
            "github-test",
            "delivery-other",
            "issues",
            Some("opened"),
            Some("acme/api"),
            "hash-c",
        )
        .is_err());
    }

    #[test]
    fn no_op_edit_keeps_revision_and_archive_requires_disabled_without_active_runs() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        let unchanged = queries::update_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
            &UpdateAutonomousAgentRequest {
                name: Some(created.definition.name.clone()),
                description: created.definition.description.clone(),
                config: Some(created.revision.config.clone()),
                budgets: Some(created.revision.budgets.clone()),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(unchanged.definition.current_revision, 1);
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        assert!(queries::set_autonomous_agent_status(
            &conn,
            &org_id,
            &created.definition.id,
            "archived"
        )
        .is_err());
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "disabled")
            .unwrap();
        assert_eq!(
            queries::set_autonomous_agent_status(
                &conn,
                &org_id,
                &created.definition.id,
                "archived"
            )
            .unwrap()
            .unwrap()
            .definition
            .status,
            "archived"
        );
    }

    #[test]
    fn lease_token_binds_start_and_heartbeat() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        queries::enqueue_autonomous_agent_run(
            &conn,
            &org_id,
            &created.definition.id,
            "manual",
            "lease-test",
            None,
        )
        .unwrap();
        queries::save_autonomous_runtime_health(
            &conn,
            &crate::automation::runtime::RuntimeHealth {
                status: "ready".into(),
                reason_code: None,
                claude_version: Some("test".into()),
                checked_at: None,
                last_success_at: None,
                last_failure_at: None,
            },
        )
        .unwrap();
        let claim = queries::claim_next_autonomous_agent_run(&conn, "worker", 60)
            .unwrap()
            .unwrap();
        assert!(!queries::start_autonomous_agent_run(
            &conn,
            &org_id,
            &claim.run.id,
            &claim.attempt_id,
            "wrong"
        )
        .unwrap());
        assert!(queries::start_autonomous_agent_run(
            &conn,
            &org_id,
            &claim.run.id,
            &claim.attempt_id,
            &claim.claim_token
        )
        .unwrap());
        assert!(!queries::heartbeat_autonomous_agent_run(
            &conn,
            &org_id,
            &claim.run.id,
            &claim.attempt_id,
            "wrong",
            60
        )
        .unwrap());
        assert!(queries::heartbeat_autonomous_agent_run(
            &conn,
            &org_id,
            &claim.run.id,
            &claim.attempt_id,
            &claim.claim_token,
            60
        )
        .unwrap());
    }

    #[test]
    fn scheduler_collapses_run_once_misfires_and_skips_skip_policy() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        let schedule = PutAutonomousAgentScheduleRequest {
            kind: "daily".into(),
            expression: Some("06:00".into()),
            timezone: "America/Bogota".into(),
            misfire_policy: "run_once".into(),
            enabled: true,
        };
        queries::put_autonomous_agent_schedule(&conn, &org_id, &created.definition.id, &schedule)
            .unwrap();
        conn.execute("UPDATE autonomous_agent_schedules SET next_run_at='2020-01-01 11:00:00' WHERE definition_id=?1",[&created.definition.id]).unwrap();
        assert_eq!(
            queries::enqueue_due_autonomous_agent_runs(&conn).unwrap(),
            1
        );
        assert_eq!(
            queries::enqueue_due_autonomous_agent_runs(&conn).unwrap(),
            0
        );
        let skip = PutAutonomousAgentScheduleRequest {
            misfire_policy: "skip".into(),
            ..schedule
        };
        queries::put_autonomous_agent_schedule(&conn, &org_id, &created.definition.id, &skip)
            .unwrap();
        conn.execute("UPDATE autonomous_agent_schedules SET next_run_at='2020-01-02 11:00:00' WHERE definition_id=?1",[&created.definition.id]).unwrap();
        assert_eq!(
            queries::enqueue_due_autonomous_agent_runs(&conn).unwrap(),
            0
        );
    }

    #[test]
    fn concurrent_scheduler_scanners_create_one_occurrence() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scheduler.db");
        let path_text = path.to_string_lossy().to_string();
        let conn = connect(&path_text).unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org.id, &user.id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org.id,
            &user.id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org.id, &created.definition.id, "enabled")
            .unwrap();
        queries::put_autonomous_agent_schedule(
            &conn,
            &org.id,
            &created.definition.id,
            &PutAutonomousAgentScheduleRequest {
                kind: "daily".into(),
                expression: Some("06:00".into()),
                timezone: "America/Bogota".into(),
                misfire_policy: "run_once".into(),
                enabled: true,
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE autonomous_agent_schedules SET next_run_at='2020-01-01 11:00:00'",
            [],
        )
        .unwrap();
        drop(conn);

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let barrier = barrier.clone();
            let path = path_text.clone();
            handles.push(std::thread::spawn(move || {
                let conn = connect(&path).unwrap();
                conn.busy_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                barrier.wait();
                queries::enqueue_due_autonomous_agent_runs(&conn).unwrap()
            }));
        }
        let created_count: usize = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .sum();
        assert_eq!(created_count, 1);

        let conn = connect(&path_text).unwrap();
        let run_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM autonomous_agent_runs WHERE trigger_kind='schedule'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(run_count, 1);
    }

    #[test]
    fn reauth_required_creates_no_attempt_and_ready_resumes_durable_run() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        queries::enqueue_autonomous_agent_run(
            &conn,
            &org_id,
            &created.definition.id,
            "manual",
            "auth-pause",
            None,
        )
        .unwrap();
        for status in ["reauth_required", "unavailable"] {
            queries::save_autonomous_runtime_health(
                &conn,
                &crate::automation::runtime::RuntimeHealth {
                    status: status.into(),
                    reason_code: Some("test".into()),
                    claude_version: None,
                    checked_at: None,
                    last_success_at: None,
                    last_failure_at: None,
                },
            )
            .unwrap();
            assert!(
                queries::claim_next_autonomous_agent_run(&conn, "worker", 60)
                    .unwrap()
                    .is_none()
            );
        }
        let attempts: i64 = conn
            .query_row("SELECT COUNT(*) FROM automation_attempts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(attempts, 0);
        queries::save_autonomous_runtime_health(
            &conn,
            &crate::automation::runtime::RuntimeHealth {
                status: "ready".into(),
                reason_code: None,
                claude_version: Some("test".into()),
                checked_at: None,
                last_success_at: None,
                last_failure_at: None,
            },
        )
        .unwrap();
        assert!(
            queries::claim_next_autonomous_agent_run(&conn, "worker", 60)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn expired_lease_is_reclaimed_with_a_new_bound_token() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        queries::enqueue_autonomous_agent_run(
            &conn,
            &org_id,
            &created.definition.id,
            "manual",
            "reclaim",
            None,
        )
        .unwrap();
        queries::save_autonomous_runtime_health(
            &conn,
            &crate::automation::runtime::RuntimeHealth {
                status: "ready".into(),
                reason_code: None,
                claude_version: None,
                checked_at: None,
                last_success_at: None,
                last_failure_at: None,
            },
        )
        .unwrap();
        let first = queries::claim_next_autonomous_agent_run(&conn, "worker-a", 60)
            .unwrap()
            .unwrap();
        conn.execute(
            "UPDATE autonomous_agent_leases SET expires_at='2000-01-01 00:00:00' WHERE run_id=?1",
            [&first.run.id],
        )
        .unwrap();
        let second = queries::claim_next_autonomous_agent_run(&conn, "worker-b", 60)
            .unwrap()
            .unwrap();
        assert_eq!(first.run.id, second.run.id);
        assert_ne!(first.claim_token, second.claim_token);
    }

    #[test]
    fn claim_enforces_repository_concurrency_without_dropping_queued_work() {
        let (conn, org_id, user_id) = fixture();
        let create_enabled = |name: &str, repository: &str| {
            let mut input = request();
            input.name = name.into();
            input.config = serde_json::json!({"outputs":["nexusmind"],"repository":repository});
            input.budgets = serde_json::json!({
                "wall_time_seconds":300,
                "max_definition_concurrency":1,
                "max_repository_concurrency":1,
                "max_organization_concurrency":4
            });
            let created =
                queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &input)
                    .unwrap();
            queries::validate_autonomous_agent_definition(
                &conn,
                &org_id,
                &user_id,
                &created.definition.id,
            )
            .unwrap();
            queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
                .unwrap();
            created.definition.id
        };
        let first_definition = create_enabled("QA one", "acme/shared");
        let second_definition = create_enabled("QA two", "acme/shared");
        let other_definition = create_enabled("QA other", "acme/other");
        for (definition, occurrence) in [
            (&first_definition, "repo-1"),
            (&second_definition, "repo-2"),
            (&other_definition, "repo-3"),
        ] {
            queries::enqueue_autonomous_agent_run(
                &conn, &org_id, definition, "manual", occurrence, None,
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE autonomous_agent_runs SET created_at=CASE occurrence_key
                WHEN 'repo-1' THEN '2026-01-01 00:00:00'
                WHEN 'repo-2' THEN '2026-01-01 00:00:01'
                ELSE '2026-01-01 00:00:02' END
             WHERE org_id=?1",
            [&org_id],
        )
        .unwrap();
        queries::save_autonomous_runtime_health(
            &conn,
            &crate::automation::runtime::RuntimeHealth {
                status: "ready".into(),
                reason_code: None,
                claude_version: None,
                checked_at: None,
                last_success_at: None,
                last_failure_at: None,
            },
        )
        .unwrap();

        let first = queries::claim_next_autonomous_agent_run(&conn, "worker-a", 60)
            .unwrap()
            .unwrap();
        let second = queries::claim_next_autonomous_agent_run(&conn, "worker-b", 60)
            .unwrap()
            .unwrap();
        assert_ne!(first.run.definition_id, second.run.definition_id);
        assert_eq!(second.run.definition_id, other_definition);
        assert_eq!(
            queries::list_autonomous_agent_runs(&conn, &org_id, Some(&second_definition)).unwrap()
                [0]
            .status,
            "queued"
        );
    }

    #[test]
    fn organization_kill_switch_stops_leasing_and_cancels_active_work() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        queries::enqueue_autonomous_agent_run(
            &conn,
            &org_id,
            &created.definition.id,
            "manual",
            "kill",
            None,
        )
        .unwrap();
        queries::save_autonomous_runtime_health(
            &conn,
            &crate::automation::runtime::RuntimeHealth {
                status: "ready".into(),
                reason_code: None,
                claude_version: None,
                checked_at: None,
                last_success_at: None,
                last_failure_at: None,
            },
        )
        .unwrap();
        let claim = queries::claim_next_autonomous_agent_run(&conn, "worker", 60)
            .unwrap()
            .unwrap();
        queries::start_autonomous_agent_run(
            &conn,
            &org_id,
            &claim.run.id,
            &claim.attempt_id,
            &claim.claim_token,
        )
        .unwrap();
        assert!(queries::record_automation_callback(
            &conn,
            &org_id,
            &claim.attempt_id,
            "callback-1",
            "payload-a",
        )
        .unwrap());
        assert!(!queries::record_automation_callback(
            &conn,
            &org_id,
            &claim.attempt_id,
            "callback-1",
            "payload-a",
        )
        .unwrap());
        assert!(queries::record_automation_callback(
            &conn,
            &org_id,
            &claim.attempt_id,
            "callback-1",
            "payload-b",
        )
        .is_err());
        queries::patch_autonomous_agent_org_settings(&conn, &org_id, Some(false), None).unwrap();
        assert_eq!(
            queries::get_autonomous_agent_run(&conn, &org_id, &claim.run.id)
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
        assert!(
            queries::claim_next_autonomous_agent_run(&conn, "worker", 60)
                .unwrap()
                .is_none()
        );
        assert!(queries::record_automation_callback(
            &conn,
            &org_id,
            &claim.attempt_id,
            "callback-after-stop",
            "payload-c",
        )
        .is_err());
        assert!(queries::finish_autonomous_agent_run(
            &conn,
            &org_id,
            &claim.run.id,
            &claim.attempt_id,
            "succeeded",
            &serde_json::json!({}),
        )
        .is_err());
        assert_eq!(
            queries::get_autonomous_agent_run(&conn, &org_id, &claim.run.id)
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
    }

    #[test]
    fn retention_removes_old_terminal_runs_and_resolved_findings() {
        let (conn, org_id, user_id) = fixture();
        let created =
            queries::create_autonomous_agent_definition(&conn, &org_id, &user_id, &request())
                .unwrap();
        queries::validate_autonomous_agent_definition(
            &conn,
            &org_id,
            &user_id,
            &created.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org_id, &created.definition.id, "enabled")
            .unwrap();
        let run = queries::enqueue_autonomous_agent_run(
            &conn,
            &org_id,
            &created.definition.id,
            "manual",
            "retention",
            None,
        )
        .unwrap()
        .unwrap();
        let finding = queries::upsert_autonomous_agent_finding(
            &conn,
            &org_id,
            &created.definition.id,
            &run.id,
            "old",
            "Old",
            "low",
            "Old",
            &serde_json::json!({}),
        )
        .unwrap();
        queries::patch_autonomous_agent_finding(&conn, &org_id, &finding.id, "resolved").unwrap();
        conn.execute("UPDATE autonomous_agent_runs SET status='succeeded',finished_at='2000-01-01 00:00:00' WHERE id=?1",[&run.id]).unwrap();
        conn.execute(
            "UPDATE autonomous_agent_findings SET updated_at='2000-01-01 00:00:00' WHERE id=?1",
            [&finding.id],
        )
        .unwrap();
        assert_eq!(
            queries::cleanup_autonomous_agent_retention(&conn).unwrap(),
            1
        );
        assert!(queries::get_autonomous_agent_run(&conn, &org_id, &run.id)
            .unwrap()
            .is_none());
    }
}
