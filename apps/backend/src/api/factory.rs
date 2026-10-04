//! Software factory control plane: per-action policies (docs/factory/PLAN.md §4).
//!
//! Policies decide how much autonomy agents get, so they are edited only here,
//! behind the explicit `factory_policy:*` permissions (no admin-role bypass), and
//! are never exposed through MCP.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use crate::{
    api::helpers::{require_explicit_permission, AppJson},
    db::factory_queries::{self, PolicyWriteError, StoredPolicy},
    factory::contracts::ActionPolicy,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

fn error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.into(),
            code: code.to_string(),
        }),
    )
}

fn internal(error_value: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        error_value.to_string(),
    )
}

fn lock_error() -> (StatusCode, Json<ApiError>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Database lock error",
    )
}

pub async fn list_policies(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<StoredPolicy>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    Ok(Json(
        factory_queries::list_factory_policies(&conn, &auth.org_id).map_err(internal)?,
    ))
}

/// Upserts the policy for `(action, scope)` and its audit record atomically. `version` must be 1 for a new policy
/// and exactly `current + 1` for an existing one; anything else is a 409, so two
/// admins editing the same policy cannot silently overwrite each other.
pub async fn put_policy(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(policy): AppJson<ActionPolicy>,
) -> ApiResult<Json<StoredPolicy>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    let stored =
        match factory_queries::upsert_factory_policy(&conn, &auth.org_id, &auth.user_id, &policy) {
            Ok((stored, _previous)) => stored,
            Err(PolicyWriteError::Invalid(reason)) => {
                return Err(error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_policy",
                    reason,
                ))
            }
            Err(PolicyWriteError::VersionConflict { current }) => {
                return Err(error(
                    StatusCode::CONFLICT,
                    "policy_version_conflict",
                    match current {
                        Some(current) => format!(
                            "Policy is at version {current}; send version {}",
                            current + 1
                        ),
                        None => "New policies start at version 1".to_string(),
                    },
                ))
            }
            Err(PolicyWriteError::Db(db_error)) => return Err(internal(db_error)),
        };
    Ok(Json(stored))
}

pub async fn delete_policy(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    match factory_queries::delete_factory_policy(&conn, &auth.org_id, &auth.user_id, &id)
        .map_err(internal)?
    {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(error(
            StatusCode::NOT_FOUND,
            "policy_not_found",
            "Policy not found",
        )),
    }
}

/// Status of this organization's sandbox bot (`null` until its first key).
pub async fn get_bot(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    let bot = factory_queries::get_factory_bot(&conn, &auth.org_id).map_err(internal)?;
    Ok(Json(serde_json::json!({ "bot": bot })))
}

/// Issues a new key for the sandbox bot, revoking the previous one. The raw key is
/// returned exactly once, never cached, and is meant only for the egress proxy's
/// secret: it is what sandboxed agents' NexusMind calls authenticate as.
pub async fn rotate_bot_key(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<(
    [(axum::http::HeaderName, &'static str); 1],
    Json<serde_json::Value>,
)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    let (bot, api_key) =
        match factory_queries::rotate_factory_bot_key(&conn, &auth.org_id, &auth.user_id) {
            Ok(result) => result,
            Err(failure) if failure.to_string() == "factory_bot_disabled" => {
                return Err(error(
                    StatusCode::CONFLICT,
                    "factory_bot_disabled",
                    "The nexus-bot user is disabled. Re-enable it in Users before issuing a key.",
                ))
            }
            Err(failure) => return Err(internal(failure)),
        };
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "bot": bot, "api_key": api_key })),
    ))
}

// ---------------------------------------------------------------- shadow (F3)

#[derive(serde::Serialize)]
pub struct ShadowReport {
    pub classes: Vec<factory_queries::ShadowClassReport>,
    pub min_settled_allows: i64,
    pub max_false_low_rate: f64,
}

/// `GET /v1/factory/shadow/report`: per-class false-low-risk rate of the
/// decision model in shadow, against the OD-5 bar.
pub async fn shadow_report(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<ShadowReport>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    Ok(Json(ShadowReport {
        classes: factory_queries::shadow_report(&conn, &auth.org_id).map_err(internal)?,
        min_settled_allows: factory_queries::OD5_MIN_SETTLED_ALLOWS,
        max_false_low_rate: factory_queries::OD5_MAX_FALSE_LOW_RATE,
    }))
}

#[derive(serde::Deserialize)]
pub struct ShadowListQuery {
    pub limit: Option<i64>,
}

/// `GET /v1/factory/shadow/decisions`: newest shadow decisions, for review.
pub async fn list_shadow_decisions(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    axum::extract::Query(query): axum::extract::Query<ShadowListQuery>,
) -> ApiResult<Json<Vec<factory_queries::ShadowDecisionRow>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    Ok(Json(
        factory_queries::list_shadow_decisions(&conn, &auth.org_id, limit).map_err(internal)?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowLabelRequest {
    /// `low`, `high`, or null to clear.
    pub label: Option<String>,
}

/// `POST /v1/factory/shadow/decisions/:id/label`: a person's verdict on whether
/// the change was actually high risk. It overrides the automatic signals.
pub async fn label_shadow_decision(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<ShadowLabelRequest>,
) -> ApiResult<StatusCode> {
    if input.label.as_deref().is_some_and(|l| l != "low" && l != "high") {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_error",
            "label must be low, high or null",
        ));
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    let found = factory_queries::label_shadow_decision(
        &conn,
        &auth.org_id,
        &id,
        input.label.as_deref(),
        &auth.user_id,
    )
    .map_err(internal)?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(error(StatusCode::NOT_FOUND, "not_found", "Shadow decision not found"))
    }
}

// ---------------------------------------------------------------- operator (F3)

/// `GET /v1/factory/digest`: everything waiting on a person: merges the policy
/// held, runs that stopped short, unstarted factory tasks, and shadow decisions
/// still without a human label.
pub async fn human_digest(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<crate::db::factory_ops::HumanDigest>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    // Runs are shown only to those who may read agent runs; tasks follow the
    // project visibility of the task list.
    let include_runs =
        crate::api::helpers::require_permission(&conn, &auth, None, "autonomous_agent:read").is_ok();
    let viewer = (!auth.role.is_super_user()).then_some(auth.user_id.as_str());
    Ok(Json(
        crate::db::factory_ops::human_digest(
            &conn,
            &auth.org_id,
            &crate::db::factory_ops::DigestScope { viewer, include_runs },
        )
        .map_err(internal)?,
    ))
}

#[derive(serde::Deserialize)]
pub struct EconomicsQuery {
    pub days: Option<i64>,
}

/// `GET /v1/factory/economics?days=30`: spend by model, the share of runs that
/// avoided the frontier tier, and cost per proposed change.
pub async fn economics(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    axum::extract::Query(query): axum::extract::Query<EconomicsQuery>,
) -> ApiResult<Json<crate::db::factory_ops::Economics>> {
    let days = query.days.unwrap_or(30).clamp(1, 365);
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    Ok(Json(
        crate::db::factory_ops::economics(&conn, &auth.org_id, days).map_err(internal)?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionRequest {
    /// `owner/repo#number@sha`, the exact head the decision is about.
    pub subject: String,
    /// Only `merge` is decided by a person through this endpoint so far.
    pub action: String,
    pub approve: bool,
    pub reason: Option<String>,
}

/// `POST /v1/factory/decisions`: a person approves or rejects one merge on one
/// exact head (D12). Only a head the policy or the decision model held can be
/// approved (never a policy `never` or a floor). Approval starts the soak: when
/// it elapses the worker re-runs every gate, the verification report and the
/// publish authority, and merges only if all still pass. Rejection cancels any
/// pending soak for the pull request.
pub async fn record_human_decision(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<HumanDecisionRequest>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    use crate::factory::contracts::{Action, ActionVerdict, Verdict, VerdictSource};
    if input.action != "merge" {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", "action must be merge"));
    }
    if !crate::db::factory_ops::valid_merge_subject(&input.subject) {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_error",
            "subject must be owner/repo#number@<40-hex sha>",
        ));
    }
    let reason: String = input
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .unwrap_or(if input.approve { "approved by a person" } else { "rejected by a person" })
        .chars()
        .take(500)
        .collect();
    let (repository, pull_number, head_sha) = crate::db::factory_ops::parse_merge_subject(&input.subject)
        .ok_or_else(|| error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", "invalid subject"))?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    let held = crate::db::factory_ops::held_for_a_person(&conn, &auth.org_id, &input.subject).map_err(internal)?;
    let soak_target = match (&held, input.approve) {
        (None, true) => {
            return Err(error(
                StatusCode::CONFLICT,
                "nothing_held",
                "This head is not held for a person: only a merge the policy or the decision model held can be approved",
            ))
        }
        (Some((None, _)), true) => {
            return Err(error(
                StatusCode::CONFLICT,
                "hold_predates_approval",
                "This hold was recorded before approvals could re-run the merge; re-run the review to hold it again",
            ))
        }
        (Some((Some(run_id), required)), true) => Some((run_id.clone(), required.clone())),
        _ => None,
    };
    // Soak first, decision second: a soak without an approval re-runs the gates
    // and stays held, so a failure between the two steps merges nothing.
    let merges_after = match &soak_target {
        Some((run_id, required)) => Some(
            factory_queries::start_merge_soak(
                &conn,
                &auth.org_id,
                run_id,
                &repository,
                pull_number,
                &head_sha,
                required,
                crate::db::factory_ops::APPROVAL_SOAK_SECONDS,
            )
            .map_err(internal)?
            .due_at,
        ),
        None => None,
    };
    let id = factory_queries::record_decision(
        &conn,
        &auth.org_id,
        &factory_queries::DecisionRecord {
            subject: input.subject.clone(),
            action: Action::Merge,
            verdict: ActionVerdict {
                verdict: if input.approve { Verdict::Allow } else { Verdict::Deny },
                reason,
                source: VerdictSource::Human,
            },
            policy_version: None,
            provider: None,
            model: None,
            confidence: None,
            inputs: serde_json::json!({"decided_by": auth.user_id}),
        },
    )
    .map_err(internal)?;
    if !input.approve {
        factory_queries::cancel_merge_soak(&conn, &auth.org_id, &repository, pull_number).map_err(internal)?;
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": id, "merges_after": merges_after}))))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryTaskRequest {
    pub project: String,
    pub title: String,
    pub description: Option<String>,
    /// Optional task class label (docs, tests, ui, backend, bugfix, …).
    pub task_class: Option<String>,
}

/// Classes a submitted task may carry as a label; intake reads them.
const FACTORY_TASK_CLASSES: [&str; 9] = [
    "docs", "tests", "ui", "backend", "bugfix", "migration", "infra", "security", "refactor",
];

/// `POST /v1/factory/tasks`: a task for the factory: a NexusMind task labelled
/// `factory` (and its class), created atomically in the backlog.
pub async fn submit_factory_task(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<FactoryTaskRequest>,
) -> ApiResult<(StatusCode, Json<crate::models::types::Task>)> {
    let title = input.title.trim();
    let project = input.project.trim();
    if title.is_empty() || title.chars().count() > 300 || project.is_empty() || project.chars().count() > 200 {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_error",
            "project and a title of 1-300 characters are required",
        ));
    }
    if input.description.as_ref().is_some_and(|d| d.len() > 65_536) {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", "description is too long"));
    }
    let class = match input.task_class.as_deref() {
        None => None,
        Some(class) if FACTORY_TASK_CLASSES.contains(&class) => Some(class),
        Some(_) => {
            return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", "unknown task_class"))
        }
    };
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    crate::api::helpers::require_permission(&conn, &auth, Some(project), "task:write")?;
    let tx = conn.unchecked_transaction().map_err(|e| internal(e.into()))?;
    let task = crate::db::queries::create_task(
        &tx,
        &auth.org_id,
        &auth.user_id,
        &crate::models::types::CreateTaskRequest {
            project: project.to_string(),
            title: title.to_string(),
            description: input.description.clone(),
            status: Some("backlog".into()),
            priority: None,
            due_date: None,
            parent_id: None,
            sprint_id: None,
        },
    )
    .map_err(internal)?;
    crate::db::queries::add_task_label(&tx, &task.id, crate::factory::intake::INTAKE_LABEL).map_err(internal)?;
    if let Some(class) = class {
        crate::db::queries::add_task_label(&tx, &task.id, class).map_err(internal)?;
    }
    tx.commit().map_err(|e| internal(e.into()))?;
    let task = crate::db::queries::get_task(&conn, &auth.org_id, &task.id)
        .map_err(internal)?
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "task vanished"))?;
    Ok((StatusCode::CREATED, Json(task)))
}
