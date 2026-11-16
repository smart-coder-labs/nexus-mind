use crate::{
    api::{
        helpers::{require_explicit_permission, AppJson},
        workflows::{fail, locked, runner_auth},
    },
    db::{workflow_execution as execution, workflows},
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Extension, Json,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
type Response = Result<Json<Value>, (StatusCode, Json<ApiError>)>;
pub async fn rotate_runner(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<([(axum::http::HeaderName, &'static str); 1], Json<Value>), (StatusCode, Json<ApiError>)>
{
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(execution::rotate_runner(&c, &auth.org_id, &id).map_err(fail)?),
    ))
}
pub async fn create_intake(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(b): AppJson<crate::automation::workflow_intake::Source>,
) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
    Ok(Json(
        crate::automation::workflow_intake::create(&c, &auth.org_id, &auth.user_id, &b)
            .map_err(fail)?,
    ))
}
pub async fn delete_intake(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
    if c.execute(
        "DELETE FROM factory_workflow_intakes WHERE id=?1 AND org_id=?2",
        rusqlite::params![id, auth.org_id],
    )
    .map_err(|e| fail(e.into()))?
        != 1
    {
        return Err(fail(anyhow::anyhow!("not_found")));
    }
    Ok(Json(json!({"deleted":true})))
}
pub async fn poll_intakes(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Response {
    {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
    }
    crate::automation::workflow_intake::spawn_poll(store,auth.org_id,None);
    Ok(Json(json!({"polling":true})))
}
pub async fn runner_poll_intakes(State(store): State<SqliteStore>, headers: HeaderMap) -> Response {
    let (runner, org) = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        runner_auth(&c, &headers)?
    };
    crate::automation::workflow_intake::spawn_poll(store,org,Some(runner));
    Ok(Json(json!({"polling":true})))
}

pub async fn publish(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(request): AppJson<crate::factory::workflow_publication::Request>,
) -> Response {
    let (runner, org) = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        runner_auth(&c, &headers)?
    };
    Ok(Json(
        crate::factory::workflow_publication::publish(&store, &org, &runner, &run, &step, &request)
            .await
            .map_err(fail)?,
    ))
}
pub async fn runner_runs(State(store): State<SqliteStore>, headers: HeaderMap) -> Response {
    let handle = store.conn();
    let c = handle.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&c, &headers)?;
    let runs = workflows::list_runs(&c, &org)
        .map_err(fail)?
        .into_iter()
        .filter(|run| {
            c.query_row(
                "SELECT runner_id=?1 FROM factory_workflow_runs WHERE id=?2",
                rusqlite::params![runner, run["id"].as_str()],
                |r| r.get::<_, bool>(0),
            )
            .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"runs":runs})))
}
pub async fn runner_run(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path(run): Path<String>,
) -> Response {
    let handle = store.conn();
    let c = handle.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&c, &headers)?;
    let owned:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_runs WHERE id=?1 AND org_id=?2 AND runner_id=?3)",rusqlite::params![run,org,runner],|r|r.get(0)).map_err(|e|fail(e.into()))?;
    if !owned {
        return Err(fail(anyhow::anyhow!("not_found")));
    }
    workflows::expire_leases(&c, &org, chrono::Utc::now().timestamp()).map_err(fail)?;
    Ok(Json(workflows::run_detail(&c, &org, &run).map_err(fail)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    lease_token: String,
    name: String,
    mime: String,
    content_base64: String,
}
pub async fn put_artifact(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(b): AppJson<Artifact>,
) -> Response {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b.content_base64)
        .map_err(|_| fail(anyhow::anyhow!("invalid_artifact_encoding")))?;
    let handle = store.conn();
    let c = handle.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&c, &headers)?;
    Ok(Json(
        execution::put_artifact(
            &c,
            &org,
            &runner,
            &run,
            &step,
            &b.lease_token,
            &b.name,
            &b.mime,
            &bytes,
            chrono::Utc::now().timestamp(),
        )
        .map_err(fail)?,
    ))
}
pub async fn get_artifact(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<([(header::HeaderName, &'static str); 2], Json<Value>), (StatusCode, Json<ApiError>)> {
    let handle = store.conn();
    let c = handle.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:read")?;
    let (mime, bytes) = execution::get_artifact(&c, &auth.org_id, &id).map_err(fail)?;
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Json(
            json!({"mime":mime,"content_base64":base64::engine::general_purpose::STANDARD.encode(bytes)}),
        ),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    action: String,
    revision: u32,
}
pub async fn approve(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run): Path<String>,
    AppJson(b): AppJson<Approval>,
) -> Response {
    let handle = store.conn();
    let c = handle.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
    let d =
        crate::factory::workflow_policy::decision(&c, &auth.org_id, &run, &b.action, &[], false)
            .map_err(fail)?;
    if d["never"] == true {
        return Err(fail(anyhow::anyhow!("policy_never")));
    }
    execution::approve(&c, &auth.org_id, &auth.user_id, &run, &b.action, b.revision)
        .map_err(fail)?;
    let detail = workflows::run_detail(&c, &auth.org_id, &run).map_err(fail)?;
    if detail["status"] == "blocked" {
        workflows::control_run(&c, &auth.org_id, &run, "resume", None).map_err(fail)?;
    }
    Ok(Json(
        workflows::run_detail(&c, &auth.org_id, &run).map_err(fail)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    lease_token: String,
}
pub async fn lease(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(b): AppJson<Lease>,
) -> Response {
    let handle = store.conn();
    let c = handle.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&c, &headers)?;
    execution::lease(
        &c,
        &org,
        &runner,
        &run,
        &step,
        &b.lease_token,
        chrono::Utc::now().timestamp(),
    )
    .map_err(fail)?;
    Ok(Json(json!({"active":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    lease_token: String,
    kind: String,
    message: String,
}
pub async fn event(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(b): AppJson<Event>,
) -> Response {
    if ![
        "agent.started",
        "agent.tool",
        "command.started",
        "command.finished",
        "service.ready",
        "log.chunk",
    ]
    .contains(&b.kind.as_str())
        || b.message.len() > 4096
    {
        return Err(fail(anyhow::anyhow!("invalid_event")));
    }
    let handle = store.conn();
    let c = handle.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&c, &headers)?;
    execution::lease(
        &c,
        &org,
        &runner,
        &run,
        &step,
        &b.lease_token,
        chrono::Utc::now().timestamp(),
    )
    .map_err(fail)?;
    let count: i64 = c
        .query_row(
            "SELECT count(*) FROM factory_workflow_events WHERE run_id=?1",
            [&run],
            |r| r.get(0),
        )
        .map_err(|e| fail(e.into()))?;
    if count > 3000 {
        return Err(fail(anyhow::anyhow!("event_quota")));
    }
    workflows::event(&c, &run, Some(&step), &b.kind, json!({"message":b.message})).map_err(fail)?;
    Ok(Json(json!({"recorded":true})))
}
