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
        match v::decision_key(&org) {
            Some(key) => {
                // One Jev call per batch of MAX_QUESTIONS, in parallel; a failed
                // batch runs its checks and never affects the other batches.
                let batches: Vec<&[v::CheckRef]> = open.chunks(v::MAX_QUESTIONS).collect();
                let answers = futures_util::future::join_all(
                    batches.iter().map(|batch| v::ask(&key, v::validation_request_owned(&task, &b.repositories, batch))),
                )
                .await;
                for (batch, answer) in batches.iter().zip(answers) {
                    match answer {
                        Ok(answer) => decisions.extend(v::apply_validation(batch, Some(&answer))),
                        Err(e) => {
                            jev_error = Some(e.to_string());
                            decisions.extend(v::apply_validation(batch, None));
                        }
                    }
                }
            }
            None => {
                jev_error = Some("jev_not_enabled".into());
                decisions.extend(v::apply_validation(&open, None));
            }
        }
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

/// Jev classifies the task once per run (`task.classified`). A confident
/// `docs` class tells agents the change is documentation-only.
pub async fn task_classification(store: &SqliteStore, org: &str, run: &str, task: &Value) -> Option<Value> {
    let key = v::decision_key(org)?;
    let cached: Option<String> = {
        let h = store.conn();
        let c = h.lock().ok()?;
        rusqlite::OptionalExtension::optional(c.query_row(
            "SELECT payload_json FROM factory_workflow_events WHERE run_id=?1 AND kind='task.classified' ORDER BY seq DESC LIMIT 1",
            [run],
            |r| r.get(0),
        ))
        .ok()?
    };
    let classified: Value = match cached {
        Some(payload) => serde_json::from_str(&payload).unwrap_or(Value::Null),
        None => {
            let title = task["title"].as_str().unwrap_or("");
            let description = task["acceptance_criteria"].as_str().unwrap_or("");
            let change = crate::factory::jev::Change { title, description, paths: &[] };
            let payload = match crate::factory::jev::decide(&reqwest::Client::new(), &key, &change).await {
                Ok((answer, _)) => json!({"task_class": answer.task_class, "class_confidence": answer.class_confidence, "risk": answer.risk}),
                Err(e) => json!({"error": e.to_string()}),
            };
            {
                let h = store.conn();
                let guard = h.lock();
                if let Ok(c) = &guard {
                    let _ = db::event(c, run, None, "task.classified", payload.clone());
                }
            }
            payload
        }
    };
    Some(classified)
}

/// After a step blocks: ask Jev which phase can resolve it and re-queue it.
/// Runs detached from the runner's completion request; failures are recorded.
/// Target, recorded decision and whether to use the static repair rule. An unsure
/// or unknown Jev answer must not strand the run: it falls back to the static
/// repair step exactly as when Jev is unavailable.
fn jev_recovery(answers: &Value, candidates: &[v::Candidate]) -> (Option<String>, Value, bool) {
    let (target, confidence) = v::apply_recovery(answers, candidates);
    let choice = answers.pointer("/answers/recovery/choice").cloned();
    let fallback = target.is_none();
    (target, json!({"source": "jev", "choice": choice, "confidence": confidence}), fallback)
}

pub async fn recover(store: SqliteStore, org: String, run: String, step: String) {
    // Base freshness is deterministic and must work without a decision model.
    {
        let h = store.conn();
        let Ok(c) = h.lock() else { return };
        match db::restart_after_base_move(&c, &org, &run, &step) {
            Ok(Some(next)) => {
                tracing::info!(run = %run, replacement = %next, "workflow scheduled fresh-base revalidation");
                return;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(run = %run, error = %error, "workflow base revalidation could not be scheduled");
                return;
            }
        }
    }
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
        Ok(answers) => jev_recovery(&answers, &context.candidates),
        Err(e) => (None, json!({"source": "fallback", "jev_error": e.to_string()}), true),
    };
    let h = store.conn();
    let Ok(c) = h.lock() else { return };
    if let Err(e) = db::apply_recovery(&c, &org, &run, &step, target.as_deref(), decision, fallback) {
        tracing::warn!(error = %e, run = %run, "workflow recovery failed");
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;

    fn candidates() -> Vec<v::Candidate> {
        vec![v::Candidate { id: "implement".into(), description: "Implement".into() }]
    }

    #[test]
    fn unsure_or_unknown_jev_answers_fall_back_to_the_static_repair_step() {
        let answer = |choice: &str, confidence: f64| {
            json!({"answers": {"recovery": {"choice": choice, "confidence": confidence}}})
        };
        let (target, decision, fallback) = jev_recovery(&answer("implement", 0.95), &candidates());
        assert_eq!(target.as_deref(), Some("implement"));
        assert!(!fallback);
        assert_eq!(decision["source"], "jev");
        // The production case: Jev picked a phase with low confidence.
        let (target, decision, fallback) = jev_recovery(&answer("tests", 0.52), &candidates());
        assert!(target.is_none());
        assert!(fallback, "a low-confidence answer must still queue a repair");
        assert_eq!(decision["choice"], "tests");
        let (target, _, fallback) = jev_recovery(&answer("nonexistent", 0.99), &candidates());
        assert!(target.is_none() && fallback);
        let (_, _, fallback) = jev_recovery(&json!({}), &candidates());
        assert!(fallback);
    }
}
