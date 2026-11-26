//! Autonomous multi-step ReAct Agent Loop (System 2 + System 1 gates + resumable approvals).

use crate::executors::{
    fs_list_dir, fs_move_to_trash, fs_read_text, fs_write_text, run_applescript, run_shell_command,
};
use crate::ipc::{DaemonState, MascotVisualState};
use crate::learning::persist_learned_skill;
use crate::llm::{AgentMessage, ToolCall, ToolSpec};
use crate::nexusmind::MemoryPayload;
use crate::policy::{evaluate_action, PolicyVerdict, ProposedAction};
use crate::redact::LocalRedactor;
use crate::storage::{AgentRunRecord, NewApproval, PendingApproval};
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use systemone_client::decisions::{CompletionJudgeInput, RouteTaskInput};
use systemone_client::DecisionSource;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunResult {
    pub run_id: String,
    pub status: String, // "completed" | "waiting_approval" | "blocked" | "failed"
    pub route: Option<String>,
    pub reply: String,
    pub steps_executed: u32,
    pub pending_approval: Option<PendingApproval>,
    pub completion_verdict: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedLoopState {
    system_prompt: String,
    messages: Vec<AgentMessage>,
    active_app: Option<String>,
    pending_tool_call: Option<ToolCall>,
    remaining_tool_calls: Vec<ToolCall>,
    evidence: Vec<String>,
}

pub async fn start_agent_run(
    state: &Arc<DaemonState>,
    objective: &str,
    active_app: Option<&str>,
) -> Result<AgentRunResult> {
    let mut redactor = LocalRedactor::new();
    let safe_objective = redactor.redact(objective);

    let (mascot_name, skills, router) = {
        let cfg = state.config.read().unwrap();
        (
            cfg.mascot_name.clone(),
            state.store.list_skills().unwrap_or_default(),
            state.router.read().unwrap().clone(),
        )
    };

    // 1. System 1 routing (Laya/Jev)
    state.set_visual_state(MascotVisualState::Deciding);
    let skill_slugs: Vec<String> = skills.iter().map(|s| s.slug.clone()).collect();
    let route = router
        .route_task(
            &RouteTaskInput {
                user_request: safe_objective.clone(),
                active_app: active_app.map(str::to_string),
                matching_skills: skill_slugs.clone(),
                inside_git_repo: false,
            },
            false,
        )
        .await
        .ok()
        .map(|env| env.answer.route);

    // 2. Pull NexusMind context if configured
    let mut context_snippets = Vec::new();
    let nm = state.current_nexusmind();
    if nm.configured() {
        if let Ok(found) = nm.search_memory(&safe_objective, 4).await {
            let compact = serde_json::to_string(&found).unwrap_or_default();
            context_snippets.push(compact.chars().take(1200).collect::<String>());
        }
    }
    if !skills.is_empty() {
        let list = skills
            .iter()
            .take(8)
            .map(|s| format!("- {}: {} (trigger: {})", s.slug, s.title, s.trigger_hint))
            .collect::<Vec<_>>()
            .join("\n");
        context_snippets.push(format!("Learned local skills:\n{list}"));
    }

    let system_prompt = format!(
        "You are {mascot_name}, the autonomous desktop mascot for NexusMind on macOS.\n\
         You can use tools to inspect and act on the computer safely. Always prefer structured Tier 1/2 tools (fs, shell, applescript, mcp) before visual UI actions.\n\
         When you complete a repeatable workflow or learn a durable user preference, call `skill_save` or `memory_store`.\n\
         Active app: {}.\n\
         Suggested System 1 route: {}.\n\
         {}",
        active_app.unwrap_or("unknown"),
        route.as_deref().unwrap_or("general_llm"),
        if context_snippets.is_empty() {
            String::new()
        } else {
            format!("Relevant context:\n{}", context_snippets.join("\n"))
        }
    );

    let run_id = Uuid::new_v4().to_string();
    let loop_state = PersistedLoopState {
        system_prompt,
        messages: vec![AgentMessage::User {
            text: safe_objective.clone(),
        }],
        active_app: active_app.map(str::to_string),
        pending_tool_call: None,
        remaining_tool_calls: Vec::new(),
        evidence: Vec::new(),
    };

    let now = Utc::now();
    let record = AgentRunRecord {
        id: run_id.clone(),
        created_at: now,
        updated_at: now,
        objective: safe_objective,
        status: "running".into(),
        route,
        steps_executed: 0,
        tool_errors: 0,
        state_json: serde_json::to_string(&loop_state)?,
        final_reply: None,
    };
    state.store.save_agent_run(&record)?;

    drive_loop(state, record, loop_state).await
}

pub async fn resume_agent_run_after_approval(
    state: &Arc<DaemonState>,
    run_id: &str,
    approved: bool,
) -> Result<AgentRunResult> {
    let mut record = state
        .store
        .get_agent_run(run_id)?
        .ok_or_else(|| anyhow!("agent run '{run_id}' not found"))?;
    let mut loop_state: PersistedLoopState = serde_json::from_str(&record.state_json)?;

    if let Some(pending_tc) = loop_state.pending_tool_call.take() {
        if approved {
            state.set_visual_state(MascotVisualState::Acting);
            match execute_tool_unchecked(state, &pending_tc).await {
                Ok(out) => {
                    record.steps_executed += 1;
                    loop_state
                        .evidence
                        .push(format!("{}: {}", pending_tc.name, preview(&out, 200)));
                    loop_state.messages.push(AgentMessage::ToolResult {
                        tool_call_id: pending_tc.id,
                        tool_name: pending_tc.name,
                        content: out,
                        is_error: false,
                    });
                }
                Err(err) => {
                    record.steps_executed += 1;
                    record.tool_errors += 1;
                    loop_state.messages.push(AgentMessage::ToolResult {
                        tool_call_id: pending_tc.id,
                        tool_name: pending_tc.name,
                        content: format!("Error: {err}"),
                        is_error: true,
                    });
                }
            }
        } else {
            loop_state.messages.push(AgentMessage::ToolResult {
                tool_call_id: pending_tc.id,
                tool_name: pending_tc.name,
                content: "User denied permission for this action. Explain politely or propose a safer alternative.".into(),
                is_error: true,
            });
        }
    }

    record.status = "running".into();
    record.updated_at = Utc::now();
    record.state_json = serde_json::to_string(&loop_state)?;
    state.store.save_agent_run(&record)?;

    drive_loop(state, record, loop_state).await
}

async fn drive_loop(
    state: &Arc<DaemonState>,
    mut record: AgentRunRecord,
    mut loop_state: PersistedLoopState,
) -> Result<AgentRunResult> {
    if !state.current_llm().configured() {
        let key_env = state.config.read().unwrap().llm.api_key_env.clone();
        let reply = format!(
            "Recibido: \"{}\". Configura `{key_env}` para activar el bucle autónomo System 2 con herramientas.",
            record.objective
        );
        record.status = "completed".into();
        record.final_reply = Some(reply.clone());
        state.store.save_agent_run(&record)?;
        state.set_visual_state(MascotVisualState::Idle);
        return Ok(AgentRunResult {
            run_id: record.id,
            status: "completed".into(),
            route: record.route,
            reply,
            steps_executed: record.steps_executed,
            pending_approval: None,
            completion_verdict: None,
        });
    }

    let max_steps = state.config.read().unwrap().autonomy.max_steps_per_task;

    loop {
        if state.kill_switch.load(Ordering::SeqCst) {
            record.status = "cancelled".into();
            record.updated_at = Utc::now();
            record.final_reply = Some("Detenido inmediatamente por Kill Switch.".into());
            state.store.save_agent_run(&record)?;
            state.set_visual_state(MascotVisualState::Blocked);
            return Ok(AgentRunResult {
                run_id: record.id,
                status: "blocked".into(),
                route: record.route,
                reply: "Detenido inmediatamente por Kill Switch.".into(),
                steps_executed: record.steps_executed,
                pending_approval: None,
                completion_verdict: None,
            });
        }

        // Drain any remaining queued tool calls from the previous assistant turn first
        if !loop_state.remaining_tool_calls.is_empty() {
            let next_tc = loop_state.remaining_tool_calls.remove(0);
            if let Some(held) =
                process_single_tool_call(state, &mut record, &mut loop_state, next_tc).await?
            {
                return Ok(held);
            }
            continue;
        }

        if record.steps_executed >= max_steps {
            let reply = format!(
                "Alcancé el límite de seguridad de {max_steps} pasos. Revisa el estado o pídeme continuar."
            );
            record.status = "completed".into();
            record.final_reply = Some(reply.clone());
            state.store.save_agent_run(&record)?;
            state.set_visual_state(MascotVisualState::Idle);
            return Ok(AgentRunResult {
                run_id: record.id,
                status: "completed".into(),
                route: record.route,
                reply,
                steps_executed: record.steps_executed,
                pending_approval: None,
                completion_verdict: Some("step_budget_reached".into()),
            });
        }

        state.set_visual_state(MascotVisualState::Thinking);
        let tools = build_tool_catalog(state).await;
        let llm = state.current_llm();
        let turn = llm
            .chat_turn(&loop_state.system_prompt, &loop_state.messages, &tools)
            .await?;

        loop_state.messages.push(AgentMessage::Assistant {
            text: turn.text.clone(),
            tool_calls: turn.tool_calls.clone(),
        });

        if turn.tool_calls.is_empty() {
            // Optional System 1 completion check when tools were executed
            let completion_verdict = if record.steps_executed > 0 {
                let router = state.router.read().unwrap().clone();
                router
                    .judge_completion(
                        &CompletionJudgeInput {
                            objective: record.objective.clone(),
                            steps_executed: record.steps_executed,
                            tool_errors: record.tool_errors,
                            evidence_summary: loop_state.evidence.join(" | "),
                        },
                        false,
                    )
                    .await
                    .ok()
                    .map(|e| e.answer.next_action)
            } else {
                None
            };

            record.status = "completed".into();
            record.updated_at = Utc::now();
            record.final_reply = Some(turn.text.clone());
            record.state_json = serde_json::to_string(&loop_state)?;
            state.store.save_agent_run(&record)?;
            state.set_visual_state(MascotVisualState::Idle);

            return Ok(AgentRunResult {
                run_id: record.id,
                status: "completed".into(),
                route: record.route,
                reply: turn.text,
                steps_executed: record.steps_executed,
                pending_approval: None,
                completion_verdict,
            });
        }

        loop_state.remaining_tool_calls = turn.tool_calls;
        let next_tc = loop_state.remaining_tool_calls.remove(0);
        if let Some(held) =
            process_single_tool_call(state, &mut record, &mut loop_state, next_tc).await?
        {
            return Ok(held);
        }
    }
}

async fn process_single_tool_call(
    state: &Arc<DaemonState>,
    record: &mut AgentRunRecord,
    loop_state: &mut PersistedLoopState,
    tc: ToolCall,
) -> Result<Option<AgentRunResult>> {
    let proposed = tool_call_to_proposed_action(&tc);
    let is_read_only = matches!(
        tc.name.as_str(),
        "fs_read" | "fs_list" | "memory_search"
    );

    let (autonomy, privacy, router) = {
        let cfg = state.config.read().unwrap();
        (
            cfg.autonomy.clone(),
            cfg.privacy.clone(),
            state.router.read().unwrap().clone(),
        )
    };
    let ks = state.kill_switch.load(Ordering::SeqCst);

    state.set_visual_state(MascotVisualState::Deciding);
    let verdict = if is_read_only && !ks {
        // Still check deterministic floors (e.g. ~/.ssh, .env) before allowing read-only!
        match crate::policy::evaluate_deterministic_floor(&proposed, &privacy, ks) {
            Some(v) => v,
            None => PolicyVerdict::Allow {
                reason: "Read-only tool auto-approved after passing deterministic floor.".into(),
                risk: 0.0,
                source: DecisionSource::DeterministicFallback,
            },
        }
    } else {
        evaluate_action(&proposed, &autonomy, &privacy, &router, ks, false).await
    };

    match verdict {
        PolicyVerdict::Allow { risk, source, .. } => {
            state.set_visual_state(MascotVisualState::Acting);
            match execute_tool_unchecked(state, &tc).await {
                Ok(out) => {
                    record.steps_executed += 1;
                    loop_state
                        .evidence
                        .push(format!("{}: {}", tc.name, preview(&out, 200)));
                    let _ = state.store.record_outcome(
                        &proposed.summary,
                        &proposed.action_kind,
                        source_label(source),
                        risk,
                        "success",
                    );
                    loop_state.messages.push(AgentMessage::ToolResult {
                        tool_call_id: tc.id,
                        tool_name: tc.name,
                        content: out,
                        is_error: false,
                    });
                }
                Err(err) => {
                    record.steps_executed += 1;
                    record.tool_errors += 1;
                    let _ = state.store.record_outcome(
                        &proposed.summary,
                        &proposed.action_kind,
                        source_label(source),
                        risk,
                        "error",
                    );
                    loop_state.messages.push(AgentMessage::ToolResult {
                        tool_call_id: tc.id,
                        tool_name: tc.name,
                        content: format!("Tool error: {err}"),
                        is_error: true,
                    });
                }
            }
            record.updated_at = Utc::now();
            record.state_json = serde_json::to_string(&loop_state)?;
            state.store.save_agent_run(record)?;
            Ok(None)
        }
        PolicyVerdict::HoldForHuman {
            reason,
            risk,
            source,
        } => {
            let approval = state.store.enqueue_approval(NewApproval {
                run_id: Some(&record.id),
                summary: &proposed.summary,
                action_kind: &proposed.action_kind,
                target: proposed.target_app_or_path.as_deref(),
                command_preview: proposed.command_preview.as_deref(),
                risk,
                decision_source: source_label(source),
                reason: &reason,
            })?;
            let _ = state.store.record_outcome(
                &proposed.summary,
                &proposed.action_kind,
                source_label(source),
                risk,
                "held",
            );
            loop_state.pending_tool_call = Some(tc);
            record.status = "waiting_approval".into();
            record.updated_at = Utc::now();
            record.state_json = serde_json::to_string(&loop_state)?;
            state.store.save_agent_run(record)?;
            state.set_visual_state(MascotVisualState::Held);

            Ok(Some(AgentRunResult {
                run_id: record.id.clone(),
                status: "waiting_approval".into(),
                route: record.route.clone(),
                reply: format!("Acción retenida para tu aprobación: {} ({reason})", proposed.summary),
                steps_executed: record.steps_executed,
                pending_approval: Some(approval),
                completion_verdict: None,
            }))
        }
        PolicyVerdict::Block { reason } => {
            record.tool_errors += 1;
            let _ = state.store.record_outcome(
                &proposed.summary,
                &proposed.action_kind,
                "deterministic_fallback",
                1.0,
                "error",
            );
            loop_state.messages.push(AgentMessage::ToolResult {
                tool_call_id: tc.id,
                tool_name: tc.name,
                content: format!("Blocked by safety floor: {reason}"),
                is_error: true,
            });
            record.updated_at = Utc::now();
            record.state_json = serde_json::to_string(&loop_state)?;
            state.store.save_agent_run(record)?;
            Ok(None)
        }
    }
}

fn tool_call_to_proposed_action(tc: &ToolCall) -> ProposedAction {
    match tc.name.as_str() {
        "run_shell" => {
            let cmd = tc
                .arguments
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            ProposedAction {
                action_kind: "shell".into(),
                summary: format!("Run shell command: {}", preview(&cmd, 120)),
                target_app_or_path: None,
                command_preview: Some(cmd),
            }
        }
        "fs_read" | "fs_list" | "fs_write" | "fs_trash" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            ProposedAction {
                action_kind: "filesystem".into(),
                summary: format!("{} on {}", tc.name, path),
                target_app_or_path: Some(path),
                command_preview: None,
            }
        }
        "run_applescript" => {
            let script = tc
                .arguments
                .get("script")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            ProposedAction {
                action_kind: "applescript".into(),
                summary: format!("Run AppleScript: {}", preview(&script, 120)),
                target_app_or_path: Some("macOS".into()),
                command_preview: Some(script),
            }
        }
        "os_ui_action" => {
            let kind = tc
                .arguments
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("ui")
                .to_string();
            let app = tc
                .arguments
                .get("target_app")
                .and_then(Value::as_str)
                .map(str::to_string);
            ProposedAction {
                action_kind: "ui".into(),
                summary: format!("macOS UI action: {kind}"),
                target_app_or_path: app,
                command_preview: Some(tc.arguments.to_string()),
            }
        }
        other => ProposedAction {
            action_kind: if other.starts_with("mcp__") {
                "mcp".into()
            } else {
                "tool".into()
            },
            summary: format!("Invoke tool {other}"),
            target_app_or_path: None,
            command_preview: Some(preview(&tc.arguments.to_string(), 240)),
        },
    }
}

async fn execute_tool_unchecked(state: &Arc<DaemonState>, tc: &ToolCall) -> Result<String> {
    match tc.name.as_str() {
        "run_shell" => {
            let cmd = tc
                .arguments
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'command'"))?;
            let out = run_shell_command(cmd, 30).await?;
            Ok(serde_json::to_string_pretty(&out)?)
        }
        "fs_read" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'path'"))?;
            fs_read_text(path, 16 * 1024)
        }
        "fs_list" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'path'"))?;
            let entries = fs_list_dir(path, 100)?;
            Ok(entries.join("\n"))
        }
        "fs_write" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'path'"))?;
            let content = tc
                .arguments
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'content'"))?;
            fs_write_text(path, content)
        }
        "fs_trash" => {
            let path = tc
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'path'"))?;
            fs_move_to_trash(path)
        }
        "run_applescript" => {
            let script = tc
                .arguments
                .get("script")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'script'"))?;
            let out = run_applescript(script, 20).await?;
            Ok(serde_json::to_string_pretty(&out)?)
        }
        "os_ui_action" => {
            let kind = tc
                .arguments
                .get("kind")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'kind'"))?;
            let job = state
                .store
                .enqueue_os_job(kind, &tc.arguments.to_string())?;
            // Wait up to 8 seconds for the native macOS SwiftUI client to fulfill the OS job
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if let Some(updated) = state.store.get_os_job(&job.id)? {
                    if updated.status == "done" {
                        return Ok(updated.result_json.unwrap_or_else(|| "{}".into()));
                    }
                    if updated.status == "error" {
                        return Err(anyhow!(
                            "macOS OS bridge error: {}",
                            updated.result_json.unwrap_or_default()
                        ));
                    }
                }
            }
            Ok(json!({ "queued_job_id": job.id, "note": "macOS native bridge did not respond within 8s" }).to_string())
        }
        "memory_store" => {
            let title = tc
                .arguments
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Mascot note");
            let content = tc
                .arguments
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("");
            let memory_type = tc
                .arguments
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("preference");
            let scope = tc
                .arguments
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("global_user");
            let topic_key = tc
                .arguments
                .get("topic_key")
                .and_then(Value::as_str)
                .unwrap_or("general");

            let mut redactor = LocalRedactor::new();
            let safe_content = redactor.redact(content);
            state.store.record_episodic_event(
                "agent",
                None,
                &format!("{title}: {safe_content}"),
                memory_type,
                0.9,
            )?;
            let nm = state.current_nexusmind();
            if nm.configured() {
                let _ = nm
                    .store_memory(&MemoryPayload {
                        title: title.into(),
                        memory_type: memory_type.into(),
                        content: safe_content,
                        topic_key: topic_key.into(),
                        scope: scope.into(),
                    })
                    .await;
            }
            Ok("Saved redacted memory locally and queued NexusMind sync.".into())
        }
        "memory_search" => {
            let query = tc
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("");
            let nm = state.current_nexusmind();
            if nm.configured() {
                let res = nm.search_memory(query, 5).await?;
                Ok(serde_json::to_string_pretty(&res)?)
            } else {
                let recent = state.store.recent_episodic_events(10)?;
                Ok(serde_json::to_string_pretty(&recent)?)
            }
        }
        "skill_save" => {
            let slug = tc
                .arguments
                .get("slug")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing 'slug'"))?;
            let title = tc
                .arguments
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(slug);
            let trigger_hint = tc
                .arguments
                .get("trigger_hint")
                .and_then(Value::as_str)
                .unwrap_or("");
            let steps_yaml = tc
                .arguments
                .get("steps_yaml")
                .and_then(Value::as_str)
                .unwrap_or("");
            let scope = tc
                .arguments
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("global_user");
            let skill =
                persist_learned_skill(state, slug, title, trigger_hint, steps_yaml, scope).await?;
            Ok(format!("Saved reusable skill '{}' ({})", skill.title, skill.slug))
        }
        name if name.starts_with("mcp__") => {
            let tools = state.mcp.list_tools().await;
            let found = tools
                .into_iter()
                .find(|t| t.qualified_name == name)
                .ok_or_else(|| anyhow!("unknown MCP tool '{name}'"))?;
            state
                .mcp
                .call_tool(&found.server_id, &found.name, tc.arguments.clone())
                .await
        }
        other => Err(anyhow!("unknown tool '{other}'")),
    }
}

async fn build_tool_catalog(state: &Arc<DaemonState>) -> Vec<ToolSpec> {
    let mut specs = vec![
        ToolSpec {
            name: "fs_read".into(),
            description: "Read a UTF-8 text file (redacted automatically).".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "fs_list".into(),
            description: "List files and directories at a given path.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "fs_write".into(),
            description: "Write text content to a file.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolSpec {
            name: "fs_trash".into(),
            description: "Safely move a file or folder to reversible macOS Trash instead of deleting.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "run_shell".into(),
            description: "Execute a shell command (zsh on macOS). Gated by Laya/Jev risk policy.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "command": { "type": "string" } },
                "required": ["command"]
            }),
        },
        ToolSpec {
            name: "run_applescript".into(),
            description: "Run AppleScript / JXA to automate native macOS apps (Finder, Notes, Calendar, Mail, Safari).".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "script": { "type": "string" } },
                "required": ["script"]
            }),
        },
        ToolSpec {
            name: "os_ui_action".into(),
            description: "Dispatch a native macOS Accessibility (AXUIElement), ScreenCaptureKit, or CGEvent action via the desktop companion.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["ax_snapshot", "ax_click", "screenshot", "mouse_click", "type_text"] },
                    "target_app": { "type": "string" },
                    "label": { "type": "string" },
                    "text": { "type": "string" },
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                },
                "required": ["kind"]
            }),
        },
        ToolSpec {
            name: "memory_store".into(),
            description: "Persist a durable user preference, project discovery, or correction to NexusMind.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "content": { "type": "string" },
                    "type": { "type": "string", "enum": ["preference", "project", "discovery", "feedback"] },
                    "topic_key": { "type": "string" },
                    "scope": { "type": "string", "enum": ["global_user", "device"] }
                },
                "required": ["title", "content"]
            }),
        },
        ToolSpec {
            name: "memory_search".into(),
            description: "Search prior memories in NexusMind or recent local observations.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "skill_save".into(),
            description: "Save a repeatable multi-step workflow as a reusable YAML skill synced to NexusMind.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "slug": { "type": "string" },
                    "title": { "type": "string" },
                    "trigger_hint": { "type": "string" },
                    "steps_yaml": { "type": "string" },
                    "scope": { "type": "string", "enum": ["global_user", "device"] }
                },
                "required": ["slug", "title", "steps_yaml"]
            }),
        },
    ];

    for mcp_tool in state.mcp.list_tools().await {
        specs.push(ToolSpec {
            name: mcp_tool.qualified_name,
            description: format!("[MCP:{}] {}", mcp_tool.server_id, mcp_tool.description),
            input_schema: mcp_tool.input_schema,
        });
    }
    specs
}

fn source_label(src: DecisionSource) -> &'static str {
    match src {
        DecisionSource::LayaLocal => "laya_local",
        DecisionSource::JevCloud => "jev_cloud",
        DecisionSource::DeterministicFallback => "deterministic_fallback",
    }
}

fn preview(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}
