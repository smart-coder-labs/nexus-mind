//! Backend OAuth stays here. Runners relay model traffic, never provider credentials.
use crate::{
    api::{
        helpers::AppJson,
        workflows::{fail, locked, runner_auth},
    },
    db::{workflow_execution, workflows},
    models::types::ApiError,
    store::sqlite::SqliteStore,
};
use anyhow::{bail, Result};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    Json,
};
use futures_util::StreamExt;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

type Error = (StatusCode, Json<ApiError>);
const MAX_REQUESTS: u32 = 32;
const MAX_RESPONSE: usize = 32 * 1024 * 1024;
static STREAMS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    lease_token: String,
    request: Value,
}

fn enabled(org: &str) -> bool {
    std::env::var("FACTORY_BACKEND_MODEL_ORG_ID").is_ok_and(|id| !id.is_empty() && id == org)
}

pub async fn capabilities(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
) -> Result<Json<Value>, Error> {
    let (_, org) = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        runner_auth(&c, &headers)?
    };
    let ready = enabled(&org) && crate::automation::pi::preflight().await.is_ok();
    Ok(Json(
        json!({"harnesses":if ready {vec!["pi"]} else {vec![]},"authentication":"backend_oauth","ready":ready}),
    ))
}

// The immutable snapshot, not the caller, chooses the role, provider and model.
fn authorize(
    c: &Connection,
    org: &str,
    runner: &str,
    run: &str,
    step: &str,
    request: &Request,
    now: i64,
) -> Result<Value> {
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let attempt =
        workflow_execution::lease(&tx, org, runner, run, step, &request.lease_token, now)?;
    let snapshot: String = tx.query_row(
        "SELECT snapshot_json FROM factory_workflow_runs WHERE id=?1",
        [run],
        |r| r.get(0),
    )?;
    let snapshot: Value = serde_json::from_str(&snapshot)?;
    let definition = snapshot["definition"]["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|s| s["id"] == step))
        .ok_or_else(|| anyhow::anyhow!("model_step_not_allowed"))?;
    if !["agent", "review"]
        .iter()
        .any(|kind| definition["kind"] == *kind)
    {
        bail!("model_step_not_allowed")
    }
    let role = match definition["adapter"].as_str() {
        Some("isolated-investigator") => "investigation",
        Some("isolated-test-author") => "test_author",
        Some("isolated-implementer") => "implementation",
        Some("isolated-reviewer") => "reviewer",
        _ => bail!("model_step_not_allowed"),
    };
    let agent = snapshot["workspace"]["runtime"]["config"]["agents"]
        .as_array()
        .and_then(|agents| agents.iter().find(|a| a["role"] == role))
        .ok_or_else(|| anyhow::anyhow!("model_step_not_allowed"))?;
    if agent["harness"] != "pi"
        || agent["model_connection"] != "backend"
        || agent["network"] != "none"
    {
        bail!("model_step_not_allowed")
    }
    let expected = agent["model"]
        .as_str()
        .filter(|m| m.starts_with("gpt-") && m.len() <= 120)
        .ok_or_else(|| anyhow::anyhow!("model_not_supported"))?;
    let mut body = request.request.clone();
    if !body.is_object()
        || body["model"] != expected
        || body["stream"] != true
        || body.get("input").is_none()
    {
        bail!("model_request_invalid")
    }
    // No background jobs, server-side tools, stored responses, or premium tier escalation.
    if body.get("background").is_some_and(|v| v != false)
        || body.get("previous_response_id").is_some()
        || body.get("tools").is_some_and(|tools| {
            !tools.as_array().is_some_and(|tools| {
                tools
                    .iter()
                    .all(|t| ["function", "custom"].iter().any(|kind| t["type"] == *kind))
            })
        })
    {
        bail!("model_request_invalid")
    }
    let object = body.as_object_mut().unwrap();
    object.insert("store".into(), json!(false));
    object.remove("service_tier");
    let used:u32=tx.query_row("SELECT count(*) FROM factory_workflow_events WHERE run_id=?1 AND step_id=?2 AND kind='model.requested' AND json_extract(payload_json,'$.attempt')=?3",params![run,step,attempt],|r|r.get(0))?;
    if used >= MAX_REQUESTS {
        bail!("model_request_budget_exceeded")
    }
    workflows::event(
        &tx,
        run,
        Some(step),
        "model.requested",
        json!({"attempt":attempt,"model":expected,"authentication":"backend_oauth","billing":"subscription_not_invoice","request":used+1,"limit":MAX_REQUESTS}),
    )?;
    tx.commit()?;
    Ok(body)
}

pub async fn invoke(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(request): AppJson<Request>,
) -> Result<Response, Error> {
    let (runner, org, body) = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        let (runner, org) = runner_auth(&c, &headers)?;
        if !enabled(&org) {
            return Err(fail(anyhow::anyhow!("backend_model_not_authorized")));
        }
        let body = authorize(
            &c,
            &org,
            &runner,
            &run,
            &step,
            &request,
            chrono::Utc::now().timestamp(),
        )
        .map_err(fail)?;
        (runner, org, body)
    };
    let permit = STREAMS
        .try_acquire()
        .map_err(|_| fail(anyhow::anyhow!("backend_model_busy")))?;
    let mut command = tokio::process::Command::new("node");
    command
        .args([
            "--input-type=module",
            "-e",
            include_str!("../factory/workflow_model_auth.mjs"),
        ])
        .current_dir("/tmp")
        .kill_on_drop(true);
    crate::automation::worker::restrict_claude_environment(&mut command);
    command
        .env_remove("NEXUSMIND_API_KEY")
        .stdin(std::process::Stdio::null());
    // Do not log helper stdout/stderr: the output contains a provider credential.
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .map_err(|_| fail(anyhow::anyhow!("backend_model_auth_required")))?
        .map_err(|_| fail(anyhow::anyhow!("backend_model_auth_required")))?;
    let auth: Value = (if output.status.success() && output.stdout.len() <= 65536 {
        serde_json::from_slice(&output.stdout).ok()
    } else {
        None
    })
    .ok_or_else(|| fail(anyhow::anyhow!("backend_model_auth_required")))?;
    let token = auth["token"]
        .as_str()
        .ok_or_else(|| fail(anyhow::anyhow!("backend_model_auth_required")))?;
    let account = auth["account"]
        .as_str()
        .ok_or_else(|| fail(anyhow::anyhow!("backend_model_auth_required")))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|_| fail(anyhow::anyhow!("backend_model_unavailable")))?;
    {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        workflow_execution::lease(
            &c,
            &org,
            &runner,
            &run,
            &step,
            &request.lease_token,
            chrono::Utc::now().timestamp(),
        )
        .map_err(fail)?;
    }
    let sending = client
        .post("https://chatgpt.com/backend-api/codex/responses")
        .bearer_auth(token)
        .header("chatgpt-account-id", account)
        .header("OpenAI-Beta", "responses=experimental")
        .header("originator", "pi")
        .header("User-Agent", "nexusmind-workflow/pi-1.4.0")
        .header("Accept", "text/event-stream")
        .json(&body)
        .send();
    tokio::pin!(sending);
    let upstream = loop {
        tokio::select! {
            result=&mut sending=>break result.map_err(|_|fail(anyhow::anyhow!("backend_model_unavailable")))?,
            _=tokio::time::sleep(Duration::from_secs(1))=>{
                let h=store.conn();let c=h.lock().map_err(|_|locked())?;
                workflow_execution::lease(&c,&org,&runner,&run,&step,&request.lease_token,chrono::Utc::now().timestamp()).map_err(fail)?;
            }
        }
    };
    if !upstream.status().is_success() {
        return Err(fail(anyhow::anyhow!(
            if matches!(upstream.status().as_u16(), 401 | 403) {
                "backend_model_auth_required"
            } else {
                "backend_model_upstream_rejected"
            }
        )));
    }
    // Revalidate the fence while streaming: pause/revocation/cancellation stop access.
    let stream = futures_util::stream::unfold(
        (upstream.bytes_stream(), permit, 0usize),
        move |(mut source, permit, bytes)| {
            let (store, org, runner, run, step, fence) = (
                store.clone(),
                org.clone(),
                runner.clone(),
                run.clone(),
                step.clone(),
                request.lease_token.clone(),
            );
            async move {
                let chunk = loop {
                    let active = {
                        let h = store.conn();
                        let c = h.lock().ok()?;
                        workflow_execution::lease(
                            &c,
                            &org,
                            &runner,
                            &run,
                            &step,
                            &fence,
                            chrono::Utc::now().timestamp(),
                        )
                        .is_ok()
                    };
                    if !active {
                        return None;
                    }
                    tokio::select! {
                        chunk=source.next()=>break chunk?,
                        _=tokio::time::sleep(Duration::from_secs(1))=>{}
                    }
                };
                match chunk {
                    Ok(data) if bytes + data.len() <= MAX_RESPONSE => {
                        let count = bytes + data.len();
                        Some((Ok::<_, std::io::Error>(data), (source, permit, count)))
                    }
                    _ => None,
                }
            }
        },
    );
    Ok(Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-store")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Connection, Request) {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT,autonomous_agents_enabled INTEGER); CREATE TABLE factory_runners(id TEXT,revoked INTEGER); CREATE TABLE factory_workflow_runs(id TEXT,org_id TEXT,runner_id TEXT,status TEXT,snapshot_json TEXT); CREATE TABLE factory_workflow_steps(run_id TEXT,step_id TEXT,status TEXT,attempt INTEGER,lease_token TEXT,lease_until INTEGER); CREATE TABLE factory_workflow_events(run_id TEXT,step_id TEXT,kind TEXT,payload_json TEXT); INSERT INTO organizations VALUES('org',1); INSERT INTO factory_runners VALUES('runner',0); INSERT INTO factory_workflow_steps VALUES('run','implement','running',1,'fence',200);").unwrap();
        let snapshot = json!({"definition":{"steps":[{"id":"implement","kind":"agent","adapter":"isolated-implementer"}]},"workspace":{"runtime":{"config":{"agents":[{"role":"implementation","harness":"pi","model":"gpt-example","model_connection":"backend","network":"none"}]}}}});
        c.execute(
            "INSERT INTO factory_workflow_runs VALUES('run','org','runner','running',?1)",
            [snapshot.to_string()],
        )
        .unwrap();
        (
            c,
            Request {
                lease_token: "fence".into(),
                request: json!({"model":"gpt-example","stream":true,"input":[],"store":true,"service_tier":"priority"}),
            },
        )
    }
    #[test]
    fn pins_model_and_lease_and_enforces_persistent_request_budget() {
        let (c, mut request) = fixture();
        assert!(authorize(&c, "org", "other", "run", "implement", &request, 100).is_err());
        assert!(authorize(&c, "other", "runner", "run", "implement", &request, 100).is_err());
        assert!(authorize(&c, "org", "runner", "run", "implement", &request, 201).is_err());
        request.lease_token = "old-fence".into();
        assert!(authorize(&c, "org", "runner", "run", "implement", &request, 100).is_err());
        request.lease_token = "fence".into();
        for change in [
            json!({"model":"gpt-other","stream":true,"input":[]}),
            json!({"model":"gpt-example","stream":true,"input":[],"background":true}),
            json!({"model":"gpt-example","stream":true,"input":[],"tools":[{"type":"web_search"}]}),
        ] {
            let bad = Request {
                lease_token: "fence".into(),
                request: change,
            };
            assert!(authorize(&c, "org", "runner", "run", "implement", &bad, 100).is_err());
        }
        for _ in 0..MAX_REQUESTS {
            let body = authorize(&c, "org", "runner", "run", "implement", &request, 100).unwrap();
            assert_eq!(body["store"], false);
            assert!(body.get("service_tier").is_none());
        }
        assert_eq!(
            authorize(&c, "org", "runner", "run", "implement", &request, 100)
                .unwrap_err()
                .to_string(),
            "model_request_budget_exceeded"
        );
    }
    #[test]
    fn pause_revocation_and_cancellation_stop_model_access() {
        for sql in [
            "UPDATE organizations SET autonomous_agents_enabled=0",
            "UPDATE factory_runners SET revoked=1",
            "UPDATE factory_workflow_runs SET status='cancelled'",
            "UPDATE factory_workflow_steps SET lease_token='new-fence'",
            "UPDATE factory_workflow_steps SET status='succeeded'",
        ] {
            let (c, request) = fixture();
            c.execute(sql, []).unwrap();
            assert!(authorize(&c, "org", "runner", "run", "implement", &request, 100).is_err());
        }
    }
}
