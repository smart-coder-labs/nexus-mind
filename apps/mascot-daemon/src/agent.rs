//! Autonomous multi-step ReAct Agent Loop (System 2 + System 1 gates + resumable approvals).

use crate::config::ModelTier;
use crate::executors::{
    fs_list_dir, fs_move_to_trash, fs_read_text, fs_write_text, run_applescript, run_shell_command,
};
use crate::ipc::{
    audit_and_sync_event, sync_conversation_record, DaemonState, MascotVisualState,
};
use crate::learning::persist_learned_skill;
use crate::llm::{AgentMessage, ToolCall, ToolSpec};
use crate::nexusmind::{format_memories_for_prompt, MemoryPayload};
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
use systemone_client::decisions::{CompletionJudgeInput, MemoryEventInput, RouteTaskInput};
use systemone_client::DecisionSource;
use uuid::Uuid;

/// Automatically resume the most recent conversation if updated within this window.
const AUTO_RESUME_WINDOW_SECS: i64 = 4 * 60 * 60;
/// Maximum recent conversation messages kept in active LLM context window.
const MAX_HISTORY_MESSAGES: usize = 24;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunResult {
    pub run_id: String,
    pub status: String, // "completed" | "waiting_approval" | "blocked" | "failed"
    pub route: Option<String>,
    pub model_tier: String,
    pub model_used: String,
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
    #[serde(default)]
    active_tier: ModelTier,
    pending_tool_call: Option<ToolCall>,
    remaining_tool_calls: Vec<ToolCall>,
    evidence: Vec<String>,
}

pub async fn start_agent_run(
    state: &Arc<DaemonState>,
    objective: &str,
    active_app: Option<&str>,
    explicit_conversation_id: Option<&str>,
    force_new: bool,
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

    // 1. System 1 routing (Laya/Jev): executor route + fast vs heavy model tier
    state.set_visual_state(MascotVisualState::Deciding);
    let skill_slugs: Vec<String> = skills.iter().map(|s| s.slug.clone()).collect();
    let routing = router
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
        .ok();
    let route = routing.as_ref().map(|env| env.answer.route.clone());

    let llm_snapshot = state.current_llm();
    let (selected_tier, tier_reason) = select_model_tier(
        &safe_objective,
        route.as_deref(),
        routing.as_ref().map(|e| e.answer.model_tier.as_str()),
        routing.as_ref().map(|e| e.answer.model_tier_confidence),
        llm_snapshot.auto_escalate_enabled(),
    );
    let selected_model = llm_snapshot.model_for_tier(selected_tier).to_string();
    state.set_active_model(selected_tier, selected_model.clone());

    // 2. Pull org-wide NexusMind memories relevant to this message
    let mut context_snippets = Vec::new();
    let nm = state.current_nexusmind();
    if nm.configured() {
        if let Ok(found) = nm.search_memory(&safe_objective, 6).await {
            if let Some(formatted) = format_memories_for_prompt(&found, 2400) {
                context_snippets.push(format!("NexusMind memories:\n{formatted}"));
            }
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
         You maintain continuous multi-turn conversations with the user and have access to NexusMind shared memory across all projects.\n\
         You can use tools to inspect and act on the computer safely. Always prefer structured Tier 1/2 tools (fs, shell, applescript, mcp) before visual UI actions.\n\
         When you need prior facts, contacts, or project details, call `memory_search`. When you learn a durable user preference, contact, or workflow, call `memory_store` or `skill_save`.\n\
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

    // 3. Resolve existing active conversation thread unless `force_new` is set
    let existing = if force_new {
        None
    } else {
        resolve_continuable_conversation(state, explicit_conversation_id)?
    };

    let now = Utc::now();
    let (record, loop_state) = if let Some(mut prev_record) = existing {
        let mut prev_state: PersistedLoopState = serde_json::from_str(&prev_record.state_json)
            .unwrap_or_else(|_| PersistedLoopState {
                system_prompt: system_prompt.clone(),
                messages: Vec::new(),
                active_app: active_app.map(str::to_string),
                active_tier: selected_tier,
                pending_tool_call: None,
                remaining_tool_calls: Vec::new(),
                evidence: Vec::new(),
            });

        // Recover cleanly if a previous turn had an unresolved tool call before this follow-up
        prev_state.pending_tool_call = None;
        prev_state.remaining_tool_calls.clear();
        prev_state.system_prompt = system_prompt;
        prev_state.active_app = active_app.map(str::to_string);
        prev_state.active_tier = selected_tier;
        trim_history_window(&mut prev_state.messages, MAX_HISTORY_MESSAGES);
        // Never leave a dangling Assistant tool_use without a matching ToolResult before adding User text
        sanitize_dangling_tool_calls(&mut prev_state.messages);
        prev_state.messages.push(AgentMessage::User {
            text: safe_objective.clone(),
        });

        prev_record.updated_at = now;
        prev_record.status = "running".into();
        if route.is_some() {
            prev_record.route = route;
        }
        prev_record.state_json = serde_json::to_string(&prev_state)?;
        (prev_record, prev_state)
    } else {
        let run_id = Uuid::new_v4().to_string();
        let loop_state = PersistedLoopState {
            system_prompt,
            messages: vec![AgentMessage::User {
                text: safe_objective.clone(),
            }],
            active_app: active_app.map(str::to_string),
            active_tier: selected_tier,
            pending_tool_call: None,
            remaining_tool_calls: Vec::new(),
            evidence: Vec::new(),
        };
        let record = AgentRunRecord {
            id: run_id,
            created_at: now,
            updated_at: now,
            objective: safe_objective.clone(),
            status: "running".into(),
            route,
            steps_executed: 0,
            tool_errors: 0,
            state_json: serde_json::to_string(&loop_state)?,
            final_reply: None,
        };
        (record, loop_state)
    };

    state.set_active_conversation_id(Some(record.id.clone()));
    state.store.save_agent_run(&record)?;
    sync_conversation_record(state, &record);
    audit_and_sync_event(
        state,
        Some(&record.id),
        "prompt",
        Some("chat"),
        None,
        None,
        Some("success"),
        &format!(
            "Prompt recibido [{} · {}]: {}",
            selected_tier.as_str(),
            selected_model,
            preview(&safe_objective, 110)
        ),
        json!({
            "active_app": active_app,
            "route": record.route,
            "model_tier": selected_tier,
            "model_selected": selected_model,
            "tier_reason": tier_reason,
            "messages_in_thread": loop_state.messages.len(),
        }),
    );
    if selected_tier == ModelTier::Heavy {
        audit_and_sync_event(
            state,
            Some(&record.id),
            "decision",
            Some("model_escalation"),
            Some("system_router"),
            None,
            Some("success"),
            &format!("Escalado a modelo potente ({selected_model}): {tier_reason}"),
            json!({ "model": selected_model, "reason": tier_reason }),
        );
    }

    drive_loop(state, record, loop_state).await
}

fn resolve_continuable_conversation(
    state: &Arc<DaemonState>,
    explicit_id: Option<&str>,
) -> Result<Option<AgentRunRecord>> {
    if let Some(id) = explicit_id {
        return state.store.get_agent_run(id);
    }
    if let Some(active_id) = state.get_active_conversation_id() {
        if let Some(rec) = state.store.get_agent_run(&active_id)? {
            return Ok(Some(rec));
        }
    }
    // Auto-resume most recent conversation after daemon restart if within AUTO_RESUME_WINDOW_SECS
    if let Some(latest) = state.store.list_agent_runs(1)?.into_iter().next() {
        let age_secs = (Utc::now() - latest.updated_at).num_seconds();
        if age_secs <= AUTO_RESUME_WINDOW_SECS {
            return Ok(Some(latest));
        }
    }
    Ok(None)
}

fn trim_history_window(messages: &mut Vec<AgentMessage>, max_len: usize) {
    if messages.len() <= max_len {
        return;
    }
    let start = messages.len() - max_len;
    // Advance to the first User message so we never slice in the middle of a ToolCall/ToolResult pair
    let aligned = messages[start..]
        .iter()
        .position(|m| matches!(m, AgentMessage::User { .. }))
        .map(|offset| start + offset)
        .unwrap_or(start);
    messages.drain(0..aligned);
}

fn sanitize_dangling_tool_calls(messages: &mut Vec<AgentMessage>) {
    while let Some(AgentMessage::Assistant { tool_calls, .. }) = messages.last() {
        if tool_calls.is_empty() {
            break;
        }
        messages.pop();
    }
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
    sync_conversation_record(state, &record);
    audit_and_sync_event(
        state,
        Some(&record.id),
        "approval",
        Some("human_decision"),
        Some("human"),
        None,
        Some(if approved { "success" } else { "cancelled" }),
        if approved {
            "Usuario aprobó acción retenida; reanudando ejecución"
        } else {
            "Usuario denegó acción retenida"
        },
        json!({ "approved": approved }),
    );

    drive_loop(state, record, loop_state).await
}

async fn drive_loop(
    state: &Arc<DaemonState>,
    mut record: AgentRunRecord,
    mut loop_state: PersistedLoopState,
) -> Result<AgentRunResult> {
    let llm_client = state.current_llm();
    if !llm_client.configured() {
        let key_env = state.config.read().unwrap().llm.api_key_env.clone();
        let reply = format!(
            "Recibido: \"{}\". Configura `{key_env}` para activar el bucle autónomo System 2 con herramientas.",
            record.objective
        );
        let active_model = llm_client.model_for_tier(loop_state.active_tier).to_string();
        record.status = "completed".into();
        record.final_reply = Some(reply.clone());
        state.store.save_agent_run(&record)?;
        state.set_visual_state(MascotVisualState::Idle);
        return Ok(AgentRunResult {
            run_id: record.id,
            status: "completed".into(),
            route: record.route,
            model_tier: loop_state.active_tier.as_str().into(),
            model_used: active_model,
            reply,
            steps_executed: record.steps_executed,
            pending_approval: None,
            completion_verdict: None,
        });
    }

    let max_steps = state.config.read().unwrap().autonomy.max_steps_per_task;
    let initial_steps = record.steps_executed;
    let initial_errors = record.tool_errors;
    let step_budget_limit = initial_steps.saturating_add(max_steps);

    loop {
        if state.kill_switch.load(Ordering::SeqCst) {
            let active_model = state
                .current_llm()
                .model_for_tier(loop_state.active_tier)
                .to_string();
            record.status = "cancelled".into();
            record.updated_at = Utc::now();
            record.final_reply = Some("Detenido inmediatamente por Kill Switch.".into());
            state.store.save_agent_run(&record)?;
            state.set_visual_state(MascotVisualState::Blocked);
            return Ok(AgentRunResult {
                run_id: record.id,
                status: "blocked".into(),
                route: record.route,
                model_tier: loop_state.active_tier.as_str().into(),
                model_used: active_model,
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

        // Mid-loop automatic escalation: if a fast-tier task gets complex (>=4 steps or >=2 errors),
        // upgrade the remaining reasoning steps to the provider's heavy model!
        let turn_steps = record.steps_executed.saturating_sub(initial_steps);
        let turn_errors = record.tool_errors.saturating_sub(initial_errors);
        let llm = state.current_llm();
        if loop_state.active_tier == ModelTier::Fast
            && llm.auto_escalate_enabled()
            && (turn_steps >= 4 || turn_errors >= 2)
        {
            loop_state.active_tier = ModelTier::Heavy;
            let heavy_name = llm.model_for_tier(ModelTier::Heavy).to_string();
            state.set_active_model(ModelTier::Heavy, heavy_name.clone());
            audit_and_sync_event(
                state,
                Some(&record.id),
                "decision",
                Some("model_escalation"),
                Some("mid_loop_complexity"),
                None,
                Some("success"),
                &format!(
                    "Auto-escalado a modelo potente ({heavy_name}) tras {turn_steps} pasos y {turn_errors} errores"
                ),
                json!({ "model": heavy_name, "turn_steps": turn_steps, "turn_errors": turn_errors }),
            );
        }

        let active_model = llm.model_for_tier(loop_state.active_tier).to_string();
        state.set_active_model(loop_state.active_tier, active_model.clone());

        if record.steps_executed >= step_budget_limit {
            let reply = format!(
                "Alcancé el límite de seguridad de {max_steps} pasos en este turno. Revisa el estado o pídeme continuar."
            );
            record.status = "completed".into();
            record.updated_at = Utc::now();
            record.final_reply = Some(reply.clone());
            record.state_json = serde_json::to_string(&loop_state)?;
            state.store.save_agent_run(&record)?;
            sync_conversation_record(state, &record);
            state.set_visual_state(MascotVisualState::Idle);
            return Ok(AgentRunResult {
                run_id: record.id,
                status: "completed".into(),
                route: record.route,
                model_tier: loop_state.active_tier.as_str().into(),
                model_used: active_model,
                reply,
                steps_executed: record.steps_executed,
                pending_approval: None,
                completion_verdict: Some("step_budget_reached".into()),
            });
        }

        state.set_visual_state(MascotVisualState::Thinking);
        let tools = build_tool_catalog(state).await;
        let turn = match llm
            .chat_turn_with_model(
                Some(&active_model),
                &loop_state.system_prompt,
                &loop_state.messages,
                &tools,
            )
            .await
        {
            Ok(t) => t,
            Err(err) => {
                record.status = "failed".into();
                record.updated_at = Utc::now();
                record.final_reply = Some(format!("Error LLM: {err}"));
                let _ = state.store.save_agent_run(&record);
                sync_conversation_record(state, &record);
                audit_and_sync_event(
                    state,
                    Some(&record.id),
                    "error",
                    Some("llm"),
                    None,
                    None,
                    Some("error"),
                    &format!("Error en System 2 ({active_model}): {err}"),
                    json!({ "model": active_model, "error": err.to_string() }),
                );
                state.set_visual_state(MascotVisualState::Blocked);
                return Err(err);
            }
        };

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
            sync_conversation_record(state, &record);
            audit_and_sync_event(
                state,
                Some(&record.id),
                "reply",
                Some("chat"),
                None,
                None,
                Some("success"),
                &format!(
                    "Respuesta completada [{} · {} · {} pasos]: {}",
                    loop_state.active_tier.as_str(),
                    turn.model,
                    record.steps_executed,
                    preview(&turn.text, 110)
                ),
                json!({
                    "steps": record.steps_executed,
                    "model": turn.model,
                    "model_tier": loop_state.active_tier,
                    "completion_verdict": completion_verdict,
                }),
            );
            // Automatically distill and persist this conversation turn as a NexusMind Memory!
            auto_persist_turn_memory(state, &record, &loop_state, &turn.text).await;
            state.set_visual_state(MascotVisualState::Idle);

            return Ok(AgentRunResult {
                run_id: record.id,
                status: "completed".into(),
                route: record.route,
                model_tier: loop_state.active_tier.as_str().into(),
                model_used: turn.model,
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
                    audit_and_sync_event(
                        state,
                        Some(&record.id),
                        "action",
                        Some(&proposed.action_kind),
                        Some(source_label(source)),
                        Some(risk),
                        Some("success"),
                        &proposed.summary,
                        json!({ "tool": tc.name, "arguments": tc.arguments, "output_preview": preview(&out, 400) }),
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
            audit_and_sync_event(
                state,
                Some(&record.id),
                "approval",
                Some(&proposed.action_kind),
                Some(source_label(source)),
                Some(risk),
                Some("held"),
                &format!("Acción retenida: {}", proposed.summary),
                json!({ "reason": reason, "tool": tc.name, "arguments": tc.arguments }),
            );
            loop_state.pending_tool_call = Some(tc);
            record.status = "waiting_approval".into();
            record.updated_at = Utc::now();
            record.state_json = serde_json::to_string(&loop_state)?;
            state.store.save_agent_run(record)?;
            sync_conversation_record(state, record);
            state.set_visual_state(MascotVisualState::Held);

            let active_model = state
                .current_llm()
                .model_for_tier(loop_state.active_tier)
                .to_string();
            Ok(Some(AgentRunResult {
                run_id: record.id.clone(),
                status: "waiting_approval".into(),
                route: record.route.clone(),
                model_tier: loop_state.active_tier.as_str().into(),
                model_used: active_model,
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

/// Decides between the fast daily model (Haiku / Flash / 4.1-mini) and the heavy reasoning
/// model (Opus / Pro / 4.1) using System 1 (`Laya`/`Jev`) + deterministic complexity rules.
fn select_model_tier(
    objective: &str,
    route: Option<&str>,
    system1_tier: Option<&str>,
    system1_conf: Option<f64>,
    auto_escalate: bool,
) -> (ModelTier, &'static str) {
    if !auto_escalate {
        return (ModelTier::Fast, "auto_escalate_disabled");
    }
    if system1_tier == Some("heavy") && system1_conf.unwrap_or(0.0) >= 0.70 {
        return (ModelTier::Heavy, "system1_classified_heavy");
    }
    if matches!(route, Some("coding_nexus_cli" | "ui_computer_use")) {
        return (ModelTier::Heavy, "complex_execution_route");
    }

    let low = objective.to_ascii_lowercase();
    let heavy_cues = [
        "powerpoint",
        "presentación",
        "presentacion",
        "slides",
        "deck",
        "crear documento",
        "crea un documento",
        "genera un documento",
        "redactar documento",
        "informe completo",
        "reporte completo",
        "crear una app",
        "crea una app",
        "trabajar en la app",
        "trabaja en la app",
        "trabajar sobre una app",
        "trabajar sobre un app",
        "desarrollar",
        "implementar",
        "refactor",
        "arquitectura",
        "diseño",
        "diseno",
        "diseñar",
        "disenar",
        "mockup",
        "figma",
        "prototipo",
        "ui/ux",
        "spreadsheet",
        "excel",
        "contrato",
        "propuesta comercial",
        "build an app",
        "create a document",
        "create presentation",
        "design system",
        "codebase",
    ];
    if heavy_cues.iter().any(|cue| low.contains(cue)) {
        return (ModelTier::Heavy, "heavy_task_keywords");
    }

    (ModelTier::Fast, "daily_conversation_or_command")
}

/// Automatically persists every meaningful conversation turn to NexusMind `/v1/memory/store`
/// using an evolving `topic_key` (`conv/<run_id>`) so multi-turn threads stay cleanly updated
/// as searchable NexusMind memories instead of disappearing.
async fn auto_persist_turn_memory(
    state: &Arc<DaemonState>,
    record: &AgentRunRecord,
    loop_state: &PersistedLoopState,
    latest_reply: &str,
) {
    let nm = state.current_nexusmind();
    if !nm.configured() {
        return;
    }

    let last_user_msg = loop_state
        .messages
        .iter()
        .rev()
        .find_map(|m| match m {
            AgentMessage::User { text } => Some(text.as_str()),
            _ => None,
        })
        .unwrap_or(&record.objective);

    // Ask System 1 (Laya/Jev) what kind of memory this turn represents
    let router = state.router.read().unwrap().clone();
    let mem_type = match router
        .evaluate_memory_worthiness(
            &MemoryEventInput {
                source: "conversation".into(),
                app_name: loop_state.active_app.clone(),
                redacted_summary: format!("User: {} | Assistant: {}", preview(last_user_msg, 300), preview(latest_reply, 300)),
                repeat_count: 1,
            },
            false,
        )
        .await
    {
        Ok(env) => match env.answer.category.as_str() {
            "preference" => "preference",
            "project_knowledge" => "discovery",
            "procedural_skill" => "pattern",
            _ => "session_summary",
        },
        Err(_) => "session_summary",
    };

    let mut transcript_digest = String::new();
    for msg in loop_state.messages.iter().rev().take(8).rev() {
        match msg {
            AgentMessage::User { text } => {
                transcript_digest.push_str(&format!("**Usuario:** {}\n", preview(text, 400)));
            }
            AgentMessage::Assistant { text, .. } if !text.trim().is_empty() => {
                transcript_digest.push_str(&format!("**Mascota:** {}\n", preview(text, 500)));
            }
            _ => {}
        }
    }

    let mut redactor = LocalRedactor::new();
    let safe_title = redactor.redact(&format!(
        "{}: {}",
        state.config.read().unwrap().mascot_name,
        preview(&record.objective, 68)
    ));
    let safe_content = redactor.redact(&format!(
        "## Conversación ({})\n{}\n## Evidencia / Acciones\n{}",
        state.hostname(),
        transcript_digest,
        if loop_state.evidence.is_empty() {
            "Sin herramientas ejecutadas.".into()
        } else {
            loop_state.evidence.join("\n- ")
        }
    ));

    let payload = MemoryPayload {
        title: safe_title.clone(),
        memory_type: mem_type.into(),
        content: safe_content,
        topic_key: format!("conv/{}", record.id),
        scope: "global_user".into(),
    };

    match nm.store_memory(&payload).await {
        Ok(_) => {
            audit_and_sync_event(
                state,
                Some(&record.id),
                "learning",
                Some("memory_store"),
                None,
                None,
                Some("success"),
                &format!("Memoria guardada en NexusMind ({mem_type}): {safe_title}"),
                json!({ "topic_key": payload.topic_key, "memory_type": mem_type }),
            );
        }
        Err(err) => {
            audit_and_sync_event(
                state,
                Some(&record.id),
                "error",
                Some("memory_store"),
                None,
                None,
                Some("error"),
                &format!("Error guardando memoria en NexusMind: {err}"),
                json!({ "topic_key": payload.topic_key, "error": err.to_string() }),
            );
        }
    }
}
