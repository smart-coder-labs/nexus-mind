//! Control plane for starting the frozen specialist eval sets.
//! Every accepted run gets its own immutable task inputs and resumable outputs.

use axum::{extract::{Path, State}, http::StatusCode, Extension, Json};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::path::{Path as FsPath, PathBuf};
use uuid::Uuid;

use crate::{api::helpers::{require_explicit_permission, AppJson}, models::types::{ApiError, AuthContext}, store::sqlite::SqliteStore};

const EVAL_DIR: &str = "/data/factory-evals";
const ALLOWED_SETS: [&str; 3] = ["docs", "tests", "ui"];
type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;
fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: message.into(), code: code.to_owned() }))
}
fn internal(error: impl std::fmt::Display) -> (StatusCode, Json<ApiError>) {
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", error.to_string())
}

#[derive(Clone, Serialize)]
pub struct EvaluationSet {
    pub id: String,
    pub name: String,
    pub status: String,
    pub task_count: Option<usize>,
    pub completed_tasks: usize,
    pub base_commit: Option<String>,
    pub summary: Option<serde_json::Value>,
}
#[derive(Serialize)]
pub struct EvalRun {
    pub id: String,
    pub set_ids: Vec<String>,
    pub status: String,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub sets: Vec<EvaluationSet>,
}
#[derive(Serialize)]
pub struct EvaluationsResponse { pub sets: Vec<EvaluationSet>, pub runs: Vec<EvalRun> }
#[derive(Serialize)]
pub struct StartEvalResponse { pub run: EvalRun }
#[derive(Deserialize)]
pub struct StartEval { pub set_ids: Vec<String> }

fn staged_set(dir: &FsPath, id: &str) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut matches = entries.filter_map(Result::ok).filter_map(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        (entry.path().is_file() && name.ends_with(".jsonl") && !name.ends_with(".results.jsonl")
            && !name.starts_with('.') && (name == format!("{id}.jsonl") || name.ends_with(&format!("-{id}.jsonl"))))
            .then_some(name)
    });
    let first = matches.next()?;
    if matches.next().is_some() { None } else { Some(first) }
}
fn dataset_info(id: &str, dir: &FsPath, summary: Option<serde_json::Value>) -> EvaluationSet {
    let Some(file) = staged_set(dir, id) else { return EvaluationSet { id: id.to_owned(), name: set_name(id), status: "missing".into(), task_count: None, completed_tasks: 0, base_commit: None, summary: None } };
    let lines = std::fs::read_to_string(dir.join(&file)).unwrap_or_default();
    let tasks: Vec<serde_json::Value> = lines.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
    let bases: std::collections::HashSet<String> = tasks.iter().filter_map(|task| task.get("base_sha").and_then(|v| v.as_str()).map(str::to_owned)).collect();
    let mut latest: std::collections::HashMap<(String, String), bool> = std::collections::HashMap::new();
    let results_path = dir.join(format!("{}.results.jsonl", file.trim_end_matches(".jsonl")));
    for outcome in std::fs::read_to_string(results_path).unwrap_or_default().lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok()) {
        let (Some(task), Some(arm)) = (outcome.get("id").and_then(|v| v.as_str()), outcome.get("arm").and_then(|v| v.as_str())) else { continue };
        let failed = outcome.get("reasons").and_then(|v| v.as_array()).is_some_and(|reasons|
            reasons.iter().filter_map(|v| v.as_str()).any(|reason| reason.starts_with("error:")));
        latest.insert((task.to_owned(), arm.to_owned()), !failed);
    }
    let task_ids: std::collections::HashSet<String> = latest.keys().map(|(task, _)| task.clone()).collect();
    let completed_tasks = task_ids.iter().filter(|task|
        latest.get(&(task.to_string(), "baseline".to_owned())) == Some(&true)
            && latest.get(&(task.to_string(), "specialist".to_owned())) == Some(&true)
    ).count();
    EvaluationSet {
        id: id.to_owned(), name: set_name(id), status: "ready".into(), task_count: Some(tasks.len()), completed_tasks,
        base_commit: (bases.len() == 1).then(|| bases.into_iter().next()).flatten(), summary: summary.or_else(|| {
            let stem = file.trim_end_matches(".jsonl");
            std::fs::read_to_string(dir.join(format!("{stem}.summary.json"))).ok().and_then(|json| serde_json::from_str(&json).ok())
        }),
    }
}
fn set_name(id: &str) -> String { match id { "docs" => "Docs", "tests" => "Tests", "ui" => "UI", _ => id }.to_owned() }
fn result_summaries(json: Option<String>) -> serde_json::Value {
    json.and_then(|value| serde_json::from_str(&value).ok()).unwrap_or(serde_json::Value::Null)
}
fn run_dir(id: &str) -> PathBuf { FsPath::new(EVAL_DIR).join("runs").join(id) }
fn list_for_org(conn: &rusqlite::Connection, org: &str) -> anyhow::Result<Vec<EvalRun>> {
    let mut stmt = conn.prepare("SELECT id,set_ids_json,status,error,started_at,COALESCE(finished_at,started_at,created_at),finished_at,summary_json
        FROM factory_eval_runs WHERE org_id=?1 ORDER BY created_at DESC LIMIT 100")?;
    let rows = stmt.query_map([org], |row| {
        let id: String = row.get(0)?;
        let set_ids: Vec<String> = serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default();
        let summaries = result_summaries(row.get(7)?);
        let dir = run_dir(&id);
        let sets = set_ids.iter().map(|set_id| {
            let summary = if set_ids.len() == 1 && summaries.get("arms").is_some() { Some(summaries.clone()) }
                else { summaries.get(set_id).cloned() };
            dataset_info(set_id, &dir, summary)
        }).collect();
        Ok(EvalRun { id, set_ids, status: row.get(2)?, error: row.get(3)?, started_at: row.get(4)?, updated_at: row.get(5)?, finished_at: row.get(6)?, sets })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub async fn list_runs(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>) -> ApiResult<Json<EvaluationsResponse>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock error"))?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    let runs = list_for_org(&conn, &auth.org_id).map_err(internal)?;
    let root = FsPath::new(EVAL_DIR);
    let sets = ALLOWED_SETS.iter().map(|id| {
        let latest = runs.iter().flat_map(|run| run.sets.iter()).find(|set| set.id == *id && set.summary.as_ref().is_some_and(|summary| !summary.is_null())).and_then(|set| set.summary.clone());
        dataset_info(id, root, latest)
    }).collect();
    Ok(Json(EvaluationsResponse { sets, runs }))
}

/// Freeze selected inputs into a run-specific directory before enqueueing. The
/// copy excludes all prior results and completion markers, so every new run is
/// a fresh evaluation and never mutates the original frozen source files.
pub async fn start_run(
    State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>, AppJson(input): AppJson<StartEval>,
) -> ApiResult<(StatusCode, Json<StartEvalResponse>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock error"))?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    if input.set_ids.is_empty() || input.set_ids.iter().any(|id| !ALLOWED_SETS.contains(&id.as_str())) {
        return Err(api_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_eval_sets", "set_ids must contain one or more of docs, tests, ui"));
    }
    let mut set_ids = input.set_ids;
    set_ids.sort(); set_ids.dedup();
    let source_dir = FsPath::new(EVAL_DIR);
    let runner_path = source_dir.join("run.mjs");
    if !runner_path.is_file() {
        return Err(api_error(StatusCode::CONFLICT, "eval_runner_unavailable", "The frozen eval runner is not installed on the worker volume"));
    }
    let runner_source = std::fs::read_to_string(&runner_path).map_err(internal)?;
    if !runner_source.contains("key === '--set'") {
        return Err(api_error(StatusCode::CONFLICT, "eval_runner_update_required", "The persistent runner must be updated to support selecting docs, tests, and UI sets"));
    }
    let mut source_files = Vec::new();
    for id in &set_ids {
        let Some(file) = staged_set(source_dir, id) else {
            return Err(api_error(StatusCode::CONFLICT, "eval_set_unavailable", format!("Eval set {id} is not staged or has an ambiguous filename")));
        };
        source_files.push(file);
    }
    let active: Option<String> = conn.query_row("SELECT id FROM factory_eval_runs WHERE status IN ('queued','running') LIMIT 1", [], |row| row.get(0)).optional().map_err(internal)?;
    if let Some(id) = active { return Err(api_error(StatusCode::CONFLICT, "eval_already_active", format!("Eval run {id} is already queued or running"))); }

    let id = Uuid::new_v4().to_string();
    let runs_dir = source_dir.join("runs");
    std::fs::create_dir_all(&runs_dir).map_err(internal)?;
    let staging_dir = runs_dir.join(format!(".{id}.staging"));
    let final_dir = run_dir(&id);
    std::fs::create_dir(&staging_dir).map_err(internal)?;
    let copy_result = (|| -> std::io::Result<()> {
        std::fs::copy(&runner_path, staging_dir.join("run.mjs"))?;
        for file in &source_files { std::fs::copy(source_dir.join(file), staging_dir.join(file))?; }
        std::fs::rename(&staging_dir, &final_dir)
    })();
    if let Err(error) = copy_result {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(internal(error));
    }

    let set_json = serde_json::to_string(&set_ids).map_err(internal)?;
    if let Err(error) = conn.execute("INSERT INTO factory_eval_runs(id,org_id,created_by,set_ids_json,status) VALUES(?1,?2,?3,?4,'queued')", rusqlite::params![id, auth.org_id, auth.user_id, set_json]) {
        let _ = std::fs::remove_dir_all(&final_dir);
        return Err(internal(error));
    }
    let run = list_for_org(&conn, &auth.org_id).map_err(internal)?.into_iter().find(|run| run.id == id).expect("inserted eval run is queryable");
    Ok((StatusCode::ACCEPTED, Json(StartEvalResponse { run })))
}

pub async fn get_run(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>, Path(id): Path<String>) -> ApiResult<Json<EvalRun>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock error"))?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    list_for_org(&conn, &auth.org_id).map_err(internal)?.into_iter().find(|run| run.id == id).map(Json)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "eval_run_not_found", "Eval run not found"))
}
