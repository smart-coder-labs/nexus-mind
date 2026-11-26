//! Local IPC server (`mascot-ipc-v1`): newline-delimited JSON-RPC 2.0 over a Unix Domain Socket.
//!
//! Designed for sub-millisecond communication between `apps/mascot-macos` (SwiftUI)
//! and `apps/mascot-daemon` (Rust).

use crate::agent::{resume_agent_run_after_approval, start_agent_run};
use crate::config::{AutonomyLevel, LlmProviderKind, MascotConfig};
use crate::executors::run_shell_command;
use crate::learning::run_idle_consolidation;
use crate::llm::LlmClient;
use crate::mcp::McpHost;
use crate::nexusmind::{
    NexusMindClient, PetConversationPayload, PetEventPayload, PetHeartbeatPayload,
};
use crate::policy::{evaluate_action, PolicyVerdict, ProposedAction};
use crate::redact::LocalRedactor;
use crate::secrets::SecretsStore;
use crate::storage::{AgentRunRecord, LocalStore, NewApproval, NewPetLog};
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
    pub secrets: RwLock<SecretsStore>,
    pub router: RwLock<SystemOneRouter>,
    pub store: LocalStore,
    pub llm: RwLock<LlmClient>,
    pub nexusmind: RwLock<NexusMindClient>,
    pub mcp: McpHost,
    pub kill_switch: AtomicBool,
    pub nexusmind_connected: AtomicBool,
    pub nexusmind_last_error: RwLock<Option<String>>,
    pub visual_state: RwLock<MascotVisualState>,
}

impl DaemonState {
    pub fn new(config: MascotConfig) -> Result<Arc<Self>> {
        let secrets = SecretsStore::load(config.resolved_secrets_path());
        let jev_key = secrets
            .get(&config.decision_engine.jev.api_key_env)
            .or_else(|| secrets.get("JEV_API_KEY"));
        let router = config.build_decision_router_with_key(jev_key)?;
        let store = LocalStore::open(&config.resolved_db_path())?;

        let mut llm_cfg = config.llm.clone();
        if llm_cfg.api_key_override.is_none() {
            llm_cfg.api_key_override = secrets.get(&llm_cfg.api_key_env);
        }
        let llm = LlmClient::new(llm_cfg)?;

        let nm_key = secrets.get(&config.nexusmind.api_key_env);
        let nexusmind = NexusMindClient::new(config.nexusmind.clone(), config.device_id.clone(), nm_key)?;
        let initial_state = if config.privacy.privacy_shield_enabled {
            MascotVisualState::PrivacyShield
        } else {
            MascotVisualState::Idle
        };
        Ok(Arc::new(Self {
            config: RwLock::new(config),
            secrets: RwLock::new(secrets),
            router: RwLock::new(router),
            store,
            llm: RwLock::new(llm),
            nexusmind: RwLock::new(nexusmind),
            mcp: McpHost::new(),
            kill_switch: AtomicBool::new(false),
            nexusmind_connected: AtomicBool::new(false),
            nexusmind_last_error: RwLock::new(None),
            visual_state: RwLock::new(initial_state),
        }))
    }

    pub fn is_nexusmind_connected(&self) -> bool {
        self.nexusmind_connected.load(Ordering::SeqCst)
    }

    pub fn require_nexusmind(&self) -> bool {
        self.config.read().unwrap().nexusmind.require_connection
    }

    pub fn hostname(&self) -> String {
        self.config.read().unwrap().device_id.clone()
    }

    pub fn os_name(&self) -> &'static str {
        std::env::consts::OS
    }

    pub fn arch_name(&self) -> &'static str {
        std::env::consts::ARCH
    }

    pub fn current_llm(&self) -> LlmClient {
        self.llm.read().unwrap().clone()
    }

    pub fn current_nexusmind(&self) -> NexusMindClient {
        self.nexusmind.read().unwrap().clone()
    }

    pub fn reload_clients(&self) -> Result<()> {
        let cfg = self.config.read().unwrap().clone();
        let secrets = self.secrets.read().unwrap().clone();

        let jev_key = secrets
            .get(&cfg.decision_engine.jev.api_key_env)
            .or_else(|| secrets.get("JEV_API_KEY"));
        let new_router = cfg.build_decision_router_with_key(jev_key)?;

        let mut llm_cfg = cfg.llm.clone();
        if llm_cfg.api_key_override.is_none() {
            llm_cfg.api_key_override = secrets.get(&llm_cfg.api_key_env);
        }
        let new_llm = LlmClient::new(llm_cfg)?;

        let nm_key = secrets.get(&cfg.nexusmind.api_key_env);
        let new_nm = NexusMindClient::new(cfg.nexusmind.clone(), cfg.device_id.clone(), nm_key)?;

        *self.router.write().unwrap() = new_router;
        *self.llm.write().unwrap() = new_llm;
        *self.nexusmind.write().unwrap() = new_nm;
        Ok(())
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
        "get_settings" => handle_get_settings(state),
        "save_settings" => handle_save_settings(state, &req.params).await,
        "test_nexusmind_connection" => handle_test_nexusmind(state).await,
        "list_conversations" => {
            let limit = req.params.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            state
                .store
                .list_agent_runs(limit)
                .map(|runs| json!({ "conversations": runs }))
        }
        "list_activity_logs" => {
            let limit = req.params.get("limit").and_then(Value::as_u64).unwrap_or(120) as usize;
            let kind = req
                .params
                .get("event_kind")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && *s != "all");
            state
                .store
                .list_pet_logs(kind, limit)
                .map(|logs| json!({ "logs": logs }))
        }
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
        "llm_configured": state.current_llm().configured(),
        "nexusmind_configured": state.current_nexusmind().configured(),
        "nexusmind_required": cfg.nexusmind.require_connection,
        "nexusmind_connected": state.is_nexusmind_connected(),
        "nexusmind_error": state.nexusmind_last_error.read().unwrap().clone(),
        "hostname": cfg.device_id,
        "os_name": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "pending_approvals": pending,
        "learned_skills": skills,
        "proactive_suggestions": suggestions,
        "mcp_tools": mcp_tools,
    }))
}

/// Sends a live authenticated heartbeat to NexusMind and updates connection state.
pub async fn verify_and_heartbeat(state: &Arc<DaemonState>) -> Result<Value> {
    let (cfg, vis, skills_count, mcp_count) = {
        let cfg = state.config.read().unwrap().clone();
        let vis = *state.visual_state.read().unwrap();
        let skills_count = state.store.list_skills().map(|s| s.len() as i64).unwrap_or(0);
        let mcp_count = state.mcp.list_tools().await.len() as i64;
        (cfg, vis, skills_count, mcp_count)
    };

    let nm = state.current_nexusmind();
    let payload = PetHeartbeatPayload {
        device_id: cfg.device_id.clone(),
        pet_name: cfg.mascot_name.clone(),
        hostname: cfg.device_id.clone(),
        os_name: std::env::consts::OS.to_string(),
        os_version: os_version_string(),
        arch: std::env::consts::ARCH.to_string(),
        autonomy_level: enum_snake(&cfg.autonomy.level),
        decision_mode: enum_snake(&cfg.decision_engine.mode),
        visual_state: enum_snake(&vis),
        skills_count,
        mcp_tools_count: mcp_count,
    };

    match nm.send_heartbeat(&payload).await {
        Ok(ack) => {
            state.nexusmind_connected.store(true, Ordering::SeqCst);
            *state.nexusmind_last_error.write().unwrap() = None;
            Ok(ack)
        }
        Err(err) => {
            state.nexusmind_connected.store(false, Ordering::SeqCst);
            *state.nexusmind_last_error.write().unwrap() = Some(err.to_string());
            Err(err)
        }
    }
}

/// Enforces mandatory NexusMind connection before running prompts or actions.
pub async fn ensure_nexusmind_ready(state: &Arc<DaemonState>) -> Result<()> {
    if !state.require_nexusmind() {
        return Ok(());
    }
    if state.is_nexusmind_connected() {
        return Ok(());
    }
    match verify_and_heartbeat(state).await {
        Ok(_) => Ok(()),
        Err(err) => Err(anyhow::anyhow!(
            "Conexión obligatoria a NexusMind requerida para usar la mascota ({err}). Abre Configuración → API Keys, conecta NexusMind y vuelve a intentar."
        )),
    }
}

/// Records an auditable event locally AND ships it asynchronously to NexusMind `/v1/pets/events`.
#[allow(clippy::too_many_arguments)]
pub fn audit_and_sync_event(
    state: &Arc<DaemonState>,
    run_id: Option<&str>,
    event_kind: &str,
    action_kind: Option<&str>,
    decision_source: Option<&str>,
    risk: Option<f64>,
    outcome: Option<&str>,
    summary: &str,
    detail: Value,
) {
    let mut redactor = LocalRedactor::new();
    let safe_summary = redactor.redact(summary);
    let safe_detail = redactor.redact(&detail.to_string());

    let _ = state.store.record_pet_log(NewPetLog {
        run_id,
        event_kind,
        action_kind,
        decision_source,
        risk,
        outcome,
        summary: &safe_summary,
        detail_json: &safe_detail,
    });

    let nm = state.current_nexusmind();
    if nm.configured() {
        let payload = PetEventPayload {
            device_id: state.hostname(),
            run_id: run_id.map(str::to_string),
            event_kind: event_kind.to_string(),
            action_kind: action_kind.map(str::to_string),
            decision_source: decision_source.map(str::to_string),
            risk,
            outcome: outcome.map(str::to_string),
            summary: safe_summary,
            detail_json: safe_detail,
        };
        tokio::spawn(async move {
            let _ = nm.push_event(&payload).await;
        });
    }
}

/// Ships a full redacted conversation transcript to NexusMind `/v1/pets/conversations`.
pub fn sync_conversation_record(state: &Arc<DaemonState>, record: &AgentRunRecord) {
    let nm = state.current_nexusmind();
    if !nm.configured() {
        return;
    }
    let mut redactor = LocalRedactor::new();
    let payload = PetConversationPayload {
        device_id: state.hostname(),
        run_id: record.id.clone(),
        objective: redactor.redact(&record.objective),
        status: record.status.clone(),
        route: record.route.clone(),
        steps_executed: record.steps_executed as i64,
        tool_errors: record.tool_errors as i64,
        transcript_json: redactor.redact(&record.state_json),
        final_reply: record.final_reply.as_deref().map(|s| redactor.redact(s)),
    };
    tokio::spawn(async move {
        let _ = nm.push_conversation(&payload).await;
    });
}

fn enum_snake<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|val| val.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".into())
}

fn os_version_string() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
        {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
    }
    String::new()
}

async fn handle_test_nexusmind(state: &Arc<DaemonState>) -> Result<Value> {
    match verify_and_heartbeat(state).await {
        Ok(ack) => Ok(json!({ "connected": true, "ack": ack })),
        Err(err) => Ok(json!({ "connected": false, "error": err.to_string() })),
    }
}

fn handle_get_settings(state: &DaemonState) -> Result<Value> {
    let cfg = state.config.read().unwrap().clone();
    let secrets = state.secrets.read().unwrap().clone();
    Ok(json!({
        "mascot_name": cfg.mascot_name,
        "decision_mode": cfg.decision_engine.mode,
        "laya_endpoint": cfg.decision_engine.laya.endpoint,
        "llm_provider": cfg.llm.provider,
        "llm_model": cfg.llm.model,
        "nexusmind_enabled": cfg.nexusmind.enabled,
        "nexusmind_base_url": cfg.nexusmind.base_url,
        "nexusmind_project": cfg.nexusmind.project,
        "has_anthropic_key": secrets.has("ANTHROPIC_API_KEY"),
        "has_gemini_key": secrets.has("GEMINI_API_KEY"),
        "has_openai_key": secrets.has("OPENAI_API_KEY"),
        "has_jev_key": secrets.has("TYPESAFE_API_KEY") || secrets.has("JEV_API_KEY"),
        "has_nexusmind_key": secrets.has("NEXUSMIND_API_KEY"),
    }))
}

async fn handle_save_settings(state: &Arc<DaemonState>, params: &Value) -> Result<Value> {
    {
        let mut cfg = state.config.write().unwrap();
        if let Some(name) = params.get("mascot_name").and_then(Value::as_str) {
            if !name.trim().is_empty() {
                cfg.mascot_name = name.trim().to_string();
            }
        }
        if let Some(mode_val) = params.get("decision_mode") {
            if let Ok(mode) = serde_json::from_value::<DecisionMode>(mode_val.clone()) {
                cfg.decision_engine.mode = mode;
            }
        }
        if let Some(laya_ep) = params.get("laya_endpoint").and_then(Value::as_str) {
            if !laya_ep.trim().is_empty() {
                cfg.decision_engine.laya.endpoint = laya_ep.trim().to_string();
            }
        }
        if let Some(prov_val) = params.get("llm_provider") {
            if let Ok(prov) = serde_json::from_value::<LlmProviderKind>(prov_val.clone()) {
                cfg.llm.provider = prov;
                cfg.llm.api_key_env = match prov {
                    LlmProviderKind::Claude => "ANTHROPIC_API_KEY".into(),
                    LlmProviderKind::Gemini => "GEMINI_API_KEY".into(),
                    LlmProviderKind::OpenAi => "OPENAI_API_KEY".into(),
                };
            }
        }
        if let Some(model) = params.get("llm_model").and_then(Value::as_str) {
            if !model.trim().is_empty() {
                cfg.llm.model = model.trim().to_string();
            }
        }
        if let Some(nm_en) = params.get("nexusmind_enabled").and_then(Value::as_bool) {
            cfg.nexusmind.enabled = nm_en;
        }
        if let Some(nm_url) = params.get("nexusmind_base_url").and_then(Value::as_str) {
            if !nm_url.trim().is_empty() {
                cfg.nexusmind.base_url = nm_url.trim().to_string();
            }
        }
        if let Some(nm_proj) = params.get("nexusmind_project").and_then(Value::as_str) {
            if !nm_proj.trim().is_empty() {
                cfg.nexusmind.project = nm_proj.trim().to_string();
            }
        }
        let cfg_path = cfg.resolved_config_path();
        cfg.write_to(&cfg_path)?;
    }

    {
        let mut secrets = state.secrets.write().unwrap();
        let key_fields = [
            ("anthropic_api_key", "ANTHROPIC_API_KEY"),
            ("gemini_api_key", "GEMINI_API_KEY"),
            ("openai_api_key", "OPENAI_API_KEY"),
            ("jev_api_key", "TYPESAFE_API_KEY"),
            ("nexusmind_api_key", "NEXUSMIND_API_KEY"),
        ];
        for (field, env_name) in key_fields {
            if let Some(val) = params.get(field).and_then(Value::as_str) {
                if !val.trim().is_empty() {
                    secrets.set_and_persist(env_name, val)?;
                }
            }
        }
    }

    state.reload_clients()?;
    let heartbeat_err = verify_and_heartbeat(state).await.err().map(|e| e.to_string());
    let mut settings = handle_get_settings(state)?;
    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            "nexusmind_connected".into(),
            json!(state.is_nexusmind_connected()),
        );
        obj.insert("nexusmind_error".into(), json!(heartbeat_err));
    }
    Ok(settings)
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
    {
        let mut cfg = state.config.write().unwrap();
        cfg.autonomy.level = level;
        let path = cfg.resolved_config_path();
        let _ = cfg.write_to(&path);
    }
    Ok(json!({ "autonomy_level": level }))
}

fn handle_set_decision_mode(state: &DaemonState, params: &Value) -> Result<Value> {
    let mode: DecisionMode = serde_json::from_value(
        params
            .get("mode")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing 'mode'"))?,
    )?;
    {
        let mut cfg = state.config.write().unwrap();
        cfg.decision_engine.mode = mode;
        let path = cfg.resolved_config_path();
        let _ = cfg.write_to(&path);
    }
    state.reload_clients()?;
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

async fn handle_propose_action(state: &Arc<DaemonState>, params: &Value) -> Result<Value> {
    ensure_nexusmind_ready(state).await?;
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
    ensure_nexusmind_ready(state).await?;
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
    ensure_nexusmind_ready(state).await?;
    let prompt = params
        .get("prompt")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt'"))?;
    let active_app = params.get("active_app").and_then(Value::as_str);

    let run = start_agent_run(state, prompt, active_app).await?;
    let llm = state.current_llm();
    Ok(json!({
        "run": run,
        "reply": {
            "text": run.reply,
            "provider": llm.provider_name(),
            "model": llm.model_name(),
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
