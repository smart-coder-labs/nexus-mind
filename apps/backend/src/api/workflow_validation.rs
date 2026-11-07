//! Runner-facing validation plan and post-failure recovery, both decided by Jev
//! on top of deterministic floors. See `factory::validation`.
use crate::{
    api::{
        helpers::AppJson,
        workflows::{fail, locked, runner_auth},
    },
    db::{workflow_execution, workflows as db},
    factory::validation as v,
    models::types::ApiError,
    store::sqlite::SqliteStore,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

type Response = Result<Json<Value>, (StatusCode, Json<ApiError>)>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    lease_token: String,
    checks: Vec<v::CheckRef>,
    #[serde(default)]
    repositories: Vec<v::RepoInfo>,
}

pub async fn plan(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(b): AppJson<PlanRequest>,
) -> Response {
    if b.checks.is_empty() || b.checks.len() > 500 || b.repositories.len() > 200 {
        return Err(fail(anyhow::anyhow!("invalid_validation_plan_request")));
    }
    let (org, task, attempt) = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        let (runner, org) = runner_auth(&c, &headers)?;
        let attempt = workflow_execution::lease(&c, &org, &runner, &run, &step, &b.lease_token, chrono::Utc::now().timestamp()).map_err(fail)?;
        let (snapshot, task): (String, String) = c
            .query_row("SELECT snapshot_json,task_json FROM factory_workflow_runs WHERE id=?1", [&run], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| fail(e.into()))?;
        let snapshot: Value = serde_json::from_str(&snapshot).map_err(|e| fail(e.into()))?;
        let is_check = snapshot["definition"]["steps"]
            .as_array()
            .and_then(|steps| steps.iter().find(|s| s["id"] == step))
            .is_some_and(|s| s["kind"] == "check");
        if !is_check {
            return Err(fail(anyhow::anyhow!("validation_plan_step_not_check")));
        }
        (org, serde_json::from_str::<Value>(&task).unwrap_or(Value::Null), attempt)
    };
    let (mut decisions, open) = v::floors(&b.checks, &b.repositories);
    let mut jev_error = None;
    if !open.is_empty() {
        let answers = match v::decision_key(&org) {
            Some(key) => match v::ask(&key, &v::validation_request(&task, &b.repositories, &open)).await {
                Ok(answers) => Some(answers),
                Err(e) => {
                    jev_error = Some(e.to_string());
                    None
                }
            },
            None => {
                jev_error = Some("jev_not_enabled".into());
                None
            }
        };
        decisions.extend(v::apply_validation(&open, answers.as_ref()));
    }
    {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        db::event(
            &c,
            &run,
            Some(&step),
            "validation.planned",
            json!({"attempt": attempt, "decisions": decisions, "summary": v::summary(&decisions), "jev_error": jev_error}),
        )
        .map_err(fail)?;
    }
    Ok(Json(json!({"decisions": decisions})))
}

/// After a step blocks: ask Jev which phase can resolve it and re-queue it.
/// Runs detached from the runner's completion request; failures are recorded.
pub async fn recover(store: SqliteStore, org: String, run: String, step: String) {
    let Some(key) = v::decision_key(&org) else { return };
    let context = {
        let h = store.conn();
        let Ok(c) = h.lock() else { return };
        match db::recovery_context(&c, &org, &run, &step) {
            Ok(Some(context)) => context,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!(error = %e, run = %run, "workflow recovery context failed");
                return;
            }
        }
    };
    let request = v::recovery_request(&context.task, &step, &context.failure, &context.candidates);
    let (target, decision, fallback) = match v::ask(&key, &request).await {
        Ok(answers) => {
            let (target, confidence) = v::apply_recovery(&answers, &context.candidates);
            let choice = answers.pointer("/answers/recovery/choice").cloned();
            (target, json!({"source": "jev", "choice": choice, "confidence": confidence}), false)
        }
        Err(e) => (None, json!({"source": "fallback", "jev_error": e.to_string()}), true),
    };
    let h = store.conn();
    let Ok(c) = h.lock() else { return };
    if let Err(e) = db::apply_recovery(&c, &org, &run, &step, target.as_deref(), decision, fallback) {
        tracing::warn!(error = %e, run = %run, "workflow recovery failed");
    }
}
