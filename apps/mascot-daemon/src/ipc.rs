//! Local IPC server (`mascot-ipc-v1`): newline-delimited JSON-RPC 2.0 over a Unix Domain Socket.
//!
//! Designed for sub-millisecond communication between `apps/mascot-macos` (SwiftUI)
//! and `apps/mascot-daemon` (Rust).

use crate::agent::{resume_agent_run_after_approval, start_agent_run};
use crate::config::{AutonomyLevel, MascotConfig};
use crate::executors::run_shell_command;
use crate::learning::run_idle_consolidation;
use crate::llm::LlmClient;
use crate::mcp::McpHost;
use crate::nexusmind::NexusMindClient;
use crate::policy::{evaluate_action, PolicyVerdict, ProposedAction};
use crate::redact::LocalRedactor;
use crate::storage::{LocalStore, NewApproval};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use systemone_client::decisions::MemoryEventInput;
use systemone_client::{DecisionMode, DecisionSource, SystemOneRouter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MascotVisualState {
    Idle,
    Listening,
    Deciding,
    Thinking,
    Acting,
    Held,
    Learning,
    PrivacyShield,
    Blocked,
}

pub struct DaemonState {
    pub config: RwLock<MascotConfig>,
    pub router: RwLock<SystemOneRouter>,
    pub store: LocalStore,
    pub llm: LlmClient,
    pub nexusmind: NexusMindClient,
    pub mcp: McpHost,
    pub kill_switch: AtomicBool,
    pub visual_state: RwLock<MascotVisualState>,
}

impl DaemonState {
    pub fn new(config: MascotConfig) -> Result<Arc<Self>> {
        let router = config.build_decision_router()?;
        let store = LocalStore::open(&config.resolved_db_path())?;
        let llm = LlmClient::new(config.llm.clone())?;
        let nexusmind = NexusMindClient::new(config.nexusmind.clone(), config.device_id.clone())?;
        let initial_state = if config.privacy.privacy_shield_enabled {
            MascotVisualState::PrivacyShield
        } else {
            MascotVisualState::Idle
        };
        Ok(Arc::new(Self {
            config: RwLock::new(config),
            router: RwLock::new(router),
            store,
            llm,
            nexusmind,
            mcp: McpHost::new(),
            kill_switch: AtomicBool::new(false),
            visual_state: RwLock::new(initial_state),
        }))
    }

    pub fn set_visual_state(&self, next: MascotVisualState) {
        if let Ok(mut guard) = self.visual_state.write() {
            *guard = next;
        }
    }
}

#[derive(Deserialize)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

pub async fn serve_unix_socket(socket_path: &Path, state: Arc<DaemonState>) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        let _ = std::fs::remove_file(socket_path);
    }

    let listener = UnixListener::bind(socket_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    }

    info!(socket = %socket_path.display(), "mascot-ipc-v1 listening");

    loop {
        let (stream, _) = listener.accept().await?;
        let st = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, st).await {
                warn!(error = %err, "IPC connection closed with error");
            }
        });
    }
}

async fn handle_connection(stream: UnixStream, state: Arc<DaemonState>) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<RpcRequest>(trimmed) {
            Ok(req) => dispatch_rpc(req, &state).await,
            Err(err) => RpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: format!("parse error: {err}"),
                }),
            },
        };
        let mut encoded = serde_json::to_string(&resp)?;
        encoded.push('\n');
        write_half.write_all(encoded.as_bytes()).await?;
    }

    Ok(())
}

pub async fn dispatch_rpc_json(raw: &str, state: &Arc<DaemonState>) -> String {
    let resp = match serde_json::from_str::<RpcRequest>(raw) {
        Ok(req) => dispatch_rpc(req, state).await,
        Err(err) => RpcResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            result: None,
            error: Some(RpcError {
                code: -32700,
                message: format!("parse error: {err}"),
            }),
        },
    };
    serde_json::to_string(&resp).unwrap_or_else(|_| "{}".into())
}

async fn dispatch_rpc(req: RpcRequest, state: &Arc<DaemonState>) -> RpcResponse {
    let id = req.id.unwrap_or(Value::Null);
    let outcome = match req.method.as_str() {
        "ping" => Ok(json!({ "ok": true, "protocol": "mascot-ipc-v1" })),
        "get_status" => handle_get_status(state).await,
        "set_privacy_shield" => handle_set_privacy_shield(state, &req.params),
        "set_autonomy_level" => handle_set_autonomy(state, &req.params),
        "set_decision_mode" => handle_set_decision_mode(state, &req.params),
        "kill_switch" => handle_kill_switch(state, &req.params),
        "propose_action" => handle_propose_action(state, &req.params).await,
        "list_pending_approvals" => state
            .store
            .list_pending_approvals()
            .map(|items| json!({ "approvals": items })),
        "resolve_approval" => handle_resolve_approval(state, &req.params).await,
        "record_observation" => handle_record_observation(state, &req.params).await,
        "get_outcome_tape" => {
            let limit = req
                .params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(14) as usize;
            state
                .store
                .outcome_tape(limit)
                .map(|items| json!({ "entries": items }))
        }
        "list_skills" => state
            .store
            .list_skills()
            .map(|skills| json!({ "skills": skills })),
        "list_recent_learning" => {
            let limit = req
                .params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(15) as usize;
            state
                .store
                .recent_episodic_events(limit)
                .map(|events| json!({ "events": events }))
        }
        "list_suggestions" => state
            .store
            .list_suggestions()
            .map(|items| json!({ "suggestions": items })),
        "list_mcp_tools" => Ok(json!({ "tools": state.mcp.list_tools().await })),
        "poll_os_jobs" => state
            .store
            .list_pending_os_jobs()
            .map(|jobs| json!({ "jobs": jobs })),
        "complete_os_job" => handle_complete_os_job(state, &req.params),
        "run_consolidation" => run_idle_consolidation(state)
            .await
            .map(|r| serde_json::to_value(r).unwrap_or(Value::Null)),
        "submit_prompt" => handle_submit_prompt(state, &req.params).await,
        other => Err(anyhow::anyhow!("unknown method '{other}'")),
    };

    match outcome {
        Ok(val) => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(val),
            error: None,
        },
        Err(err) => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code: -32000,
                message: err.to_string(),
            }),
        },
    }
}

async fn handle_get_status(state: &Arc<DaemonState>) -> Result<Value> {
    let cfg = state.config.read().unwrap().clone();
    let vis = *state.visual_state.read().unwrap();
    let pending = state.store.list_pending_approvals()?.len();
    let skills = state.store.list_skills()?.len();
    let suggestions = state.store.list_suggestions()?.len();
    let mcp_tools = state.mcp.list_tools().await.len();

    Ok(json!({
        "mascot_name": cfg.mascot_name,
        "device_id": cfg.device_id,
        "visual_state": vis,
        "decision_mode": cfg.decision_engine.mode,
        "laya_endpoint": cfg.decision_engine.laya.endpoint,
        "autonomy_level": cfg.autonomy.level,
        "privacy_shield": cfg.privacy.privacy_shield_enabled,
        "kill_switch": state.kill_switch.load(Ordering::SeqCst),
        "llm_provider": cfg.llm.provider,
        "llm_configured": state.llm.configured(),
        "nexusmind_configured": state.nexusmind.configured(),
        "pending_approvals": pending,
        "learned_skills": skills,
        "proactive_suggestions": suggestions,
        "mcp_tools": mcp_tools,
    }))
}

fn handle_set_privacy_shield(state: &DaemonState, params: &Value) -> Result<Value> {
    let enabled = params
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("missing boolean 'enabled'"))?;
    state.config.write().unwrap().privacy.privacy_shield_enabled = enabled;
    state.set_visual_state(if enabled {
        MascotVisualState::PrivacyShield
    } else {
        MascotVisualState::Idle
    });
    Ok(json!({ "privacy_shield": enabled }))
}

fn handle_set_autonomy(state: &DaemonState, params: &Value) -> Result<Value> {
    let level: AutonomyLevel = serde_json::from_value(
        params
            .get("level")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing 'level'"))?,
    )?;
    state.config.write().unwrap().autonomy.level = level;
    Ok(json!({ "autonomy_level": level }))
}

fn handle_set_decision_mode(state: &DaemonState, params: &Value) -> Result<Value> {
    let mode: DecisionMode = serde_json::from_value(
        params
            .get("mode")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing 'mode'"))?,
    )?;
    let new_router = {
        let mut cfg = state.config.write().unwrap();
        cfg.decision_engine.mode = mode;
        cfg.build_decision_router()?
    };
    *state.router.write().unwrap() = new_router;
    Ok(json!({ "decision_mode": mode }))
}

fn handle_kill_switch(state: &DaemonState, params: &Value) -> Result<Value> {
    let active = params
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    state.kill_switch.store(active, Ordering::SeqCst);
    state.set_visual_state(if active {
        MascotVisualState::Blocked
    } else {
        MascotVisualState::Idle
    });
    let _ = state.store.record_outcome(
        if active {
            "Emergency Kill Switch activated"
        } else {
            "Emergency Kill Switch cleared"
        },
        "safety",
        "deterministic_fallback",
        0.0,
        if active { "cancelled" } else { "success" },
    );
    Ok(json!({ "kill_switch": active }))
}

fn handle_complete_os_job(state: &DaemonState, params: &Value) -> Result<Value> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing job 'id'"))?;
    let ok = params.get("ok").and_then(Value::as_bool).unwrap_or(true);
    let result_json = params
        .get("result")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".into());
    let updated = state.store.complete_os_job(id, ok, &result_json)?;
    Ok(json!({ "updated": updated }))
}

async fn handle_propose_action(state: &DaemonState, params: &Value) -> Result<Value> {
    let action = ProposedAction {
        action_kind: params
            .get("action_kind")
            .and_then(Value::as_str)
            .unwrap_or("shell")
            .to_string(),
        summary: params
            .get("summary")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing 'summary'"))?
            .to_string(),
        target_app_or_path: params
            .get("target")
            .and_then(Value::as_str)
            .map(str::to_string),
        command_preview: params
            .get("command_preview")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    let is_private = params
        .get("is_private_context")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    state.set_visual_state(MascotVisualState::Deciding);
    let (autonomy, privacy, router) = {
        let cfg = state.config.read().unwrap();
        (
            cfg.autonomy.clone(),
            cfg.privacy.clone(),
            state.router.read().unwrap().clone(),
        )
    };
    let ks = state.kill_switch.load(Ordering::SeqCst);

    let verdict = evaluate_action(&action, &autonomy, &privacy, &router, ks, is_private).await;

    match &verdict {
        PolicyVerdict::Allow {
            risk,
            source,
            reason,
        } => {
            state.set_visual_state(MascotVisualState::Acting);
            let exec_out = if action.action_kind == "shell" {
                if let Some(cmd) = action.command_preview.as_deref() {
                    Some(run_shell_command(cmd, 30).await?)
                } else {
                    None
                }
            } else {
                None
            };
            let outcome = match &exec_out {
                Some(r) if r.exit_code != 0 => "error",
                _ => "success",
            };
            state.store.record_outcome(
                &action.summary,
                &action.action_kind,
                source_name(*source),
                *risk,
                outcome,
            )?;
            state.set_visual_state(MascotVisualState::Idle);
            Ok(json!({
                "verdict": verdict,
                "reason": reason,
                "execution": exec_out,
            }))
        }
        PolicyVerdict::HoldForHuman {
            reason,
            risk,
            source,
        } => {
            let approval = state.store.enqueue_approval(NewApproval {
                run_id: None,
                summary: &action.summary,
                action_kind: &action.action_kind,
                target: action.target_app_or_path.as_deref(),
                command_preview: action.command_preview.as_deref(),
                risk: *risk,
                decision_source: source_name(*source),
                reason,
            })?;
            state.store.record_outcome(
                &action.summary,
                &action.action_kind,
                source_name(*source),
                *risk,
                "held",
            )?;
            state.set_visual_state(MascotVisualState::Held);
            Ok(json!({
                "verdict": verdict,
                "approval": approval,
            }))
        }
        PolicyVerdict::Block { reason } => {
            state.store.record_outcome(
                &action.summary,
                &action.action_kind,
                "deterministic_fallback",
                1.0,
                "error",
            )?;
            state.set_visual_state(MascotVisualState::Blocked);
            Ok(json!({
                "verdict": verdict,
                "reason": reason,
            }))
        }
    }
}

async fn handle_resolve_approval(state: &Arc<DaemonState>, params: &Value) -> Result<Value> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing approval 'id'"))?;
    let approved = params
        .get("approved")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("missing boolean 'approved'"))?;

    let item = state
        .store
        .resolve_approval(id, approved)?
        .ok_or_else(|| anyhow::anyhow!("approval '{id}' not found or already resolved"))?;

    // If tied to a suspended ReAct agent run, resume that exact run!
    if let Some(run_id) = item.run_id.as_deref() {
        let resumed = resume_agent_run_after_approval(state, run_id, approved).await?;
        return Ok(json!({
            "approval": item,
            "resumed_run": resumed,
        }));
    }

    let mut exec_out = None;
    if approved {
        state.set_visual_state(MascotVisualState::Acting);
        if item.action_kind == "shell" {
            if let Some(cmd) = item.command_preview.as_deref() {
                exec_out = Some(run_shell_command(cmd, 30).await?);
            }
        }
        let outcome = match &exec_out {
            Some(r) if r.exit_code != 0 => "error",
            _ => "success",
        };
        state.store.record_outcome(
            &item.summary,
            &item.action_kind,
            &item.decision_source,
            item.risk,
            outcome,
        )?;
    } else {
        state.store.record_outcome(
            &item.summary,
            &item.action_kind,
            &item.decision_source,
            item.risk,
            "cancelled",
        )?;
    }

    let remaining = state.store.list_pending_approvals()?.len();
    state.set_visual_state(if remaining > 0 {
        MascotVisualState::Held
    } else {
        MascotVisualState::Idle
    });

    Ok(json!({
        "approval": item,
        "execution": exec_out,
    }))
}

async fn handle_record_observation(state: &DaemonState, params: &Value) -> Result<Value> {
    let privacy = state.config.read().unwrap().privacy.clone();
    if privacy.privacy_shield_enabled {
        return Ok(json!({ "recorded": false, "reason": "privacy_shield_enabled" }));
    }

    let app_name = params.get("app_name").and_then(Value::as_str);
    if let Some(app) = app_name {
        let low = app.to_ascii_lowercase();
        if privacy
            .blocked_apps
            .iter()
            .any(|b| low.contains(&b.to_ascii_lowercase()))
        {
            return Ok(json!({ "recorded": false, "reason": "blocked_app" }));
        }
    }

    let source = params
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("active_window");
    let raw_summary = params
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing 'summary'"))?;
    let repeat_count = params
        .get("repeat_count")
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;

    let mut redactor = LocalRedactor::new();
    let redacted = redactor.redact(raw_summary);

    let router = state.router.read().unwrap().clone();
    let (category, importance) = match router
        .evaluate_memory_worthiness(
            &MemoryEventInput {
                source: source.into(),
                app_name: app_name.map(str::to_string),
                redacted_summary: redacted.clone(),
                repeat_count,
            },
            true, // passive observations always stay local-first
        )
        .await
    {
        Ok(env) => (env.answer.category, env.answer.importance),
        Err(_) => ("ephemeral".into(), 0.25),
    };

    if category == "ignore" {
        return Ok(json!({ "recorded": false, "category": "ignore" }));
    }

    let ev = state
        .store
        .record_episodic_event(source, app_name, &redacted, &category, importance)?;
    if importance >= 0.6 {
        state.set_visual_state(MascotVisualState::Learning);
    }
    Ok(json!({ "recorded": true, "event": ev }))
}

async fn handle_submit_prompt(state: &Arc<DaemonState>, params: &Value) -> Result<Value> {
    let prompt = params
        .get("prompt")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt'"))?;
    let active_app = params.get("active_app").and_then(Value::as_str);

    let run = start_agent_run(state, prompt, active_app).await?;
    Ok(json!({
        "run": run,
        "reply": {
            "text": run.reply,
            "provider": state.llm.provider_name(),
            "model": state.llm.model_name(),
        }
    }))
}

fn source_name(src: DecisionSource) -> &'static str {
    match src {
        DecisionSource::LayaLocal => "laya_local",
        DecisionSource::JevCloud => "jev_cloud",
        DecisionSource::DeterministicFallback => "deterministic_fallback",
    }
}
