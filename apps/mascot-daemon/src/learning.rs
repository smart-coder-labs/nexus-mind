//! Continuous Learning Engine:
//! - Persists learned procedural skills to SQLite + `~/.nexusmind/skills/<slug>.yaml` + NexusMind.
//! - Idle consolidation ("Sleep Cycle"): distills episodic events into durable memories.
//! - Repetition detector: spots recurring user patterns and queues proactive automation proposals
//!   gated by System 1 `proactive_interrupt`.

use crate::ipc::{DaemonState, MascotVisualState};
use crate::nexusmind::MemoryPayload;
use crate::redact::LocalRedactor;
use crate::storage::{LearnedSkill, ProactiveSuggestion};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use systemone_client::decisions::ProactiveInterruptInput;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsolidationReport {
    pub consolidated_events: usize,
    pub synced_to_nexusmind: usize,
    pub suggestions_created: usize,
}

#[derive(Serialize)]
struct SkillFileDoc<'a> {
    slug: &'a str,
    title: &'a str,
    trigger_hint: &'a str,
    scope: &'a str,
    steps_yaml: &'a str,
}

pub async fn persist_learned_skill(
    state: &Arc<DaemonState>,
    slug: &str,
    title: &str,
    trigger_hint: &str,
    steps_yaml: &str,
    scope: &str,
) -> Result<LearnedSkill> {
    let mut redactor = LocalRedactor::new();
    let safe_steps = redactor.redact(steps_yaml);
    let safe_title = redactor.redact(title);
    let safe_trigger = redactor.redact(trigger_hint);

    let skill = state
        .store
        .upsert_skill(slug, &safe_title, &safe_trigger, &safe_steps, scope)?;

    // Write human-readable YAML file in ~/.nexusmind/skills/<slug>.yaml
    let skills_dir = state.config.read().unwrap().resolved_skills_dir();
    std::fs::create_dir_all(&skills_dir)?;
    let safe_slug: String = slug
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let doc = SkillFileDoc {
        slug: &safe_slug,
        title: &safe_title,
        trigger_hint: &safe_trigger,
        scope,
        steps_yaml: &safe_steps,
    };
    let yaml_path = skills_dir.join(format!("{safe_slug}.yaml"));
    std::fs::write(&yaml_path, serde_yaml::to_string(&doc)?)?;

    if state.nexusmind.configured() {
        let _ = state
            .nexusmind
            .store_memory(&MemoryPayload {
                title: format!("Skill: {safe_title}"),
                memory_type: "pattern".into(),
                content: format!(
                    "**Trigger:** {safe_trigger}\n\n```yaml\n{safe_steps}\n```"
                ),
                topic_key: format!("skill/{safe_slug}"),
                scope: scope.into(),
            })
            .await;
    }

    state.set_visual_state(MascotVisualState::Learning);
    Ok(skill)
}

pub async fn run_idle_consolidation(state: &Arc<DaemonState>) -> Result<ConsolidationReport> {
    let events = state.store.unconsolidated_events(100)?;
    if events.is_empty() {
        return Ok(ConsolidationReport {
            consolidated_events: 0,
            synced_to_nexusmind: 0,
            suggestions_created: 0,
        });
    }

    state.set_visual_state(MascotVisualState::Learning);
    let mut synced = 0_usize;
    let mut processed_ids = Vec::with_capacity(events.len());
    let mut pattern_counts: HashMap<String, (usize, Option<String>)> = HashMap::new();

    for ev in &events {
        // Track repeated app/action sequences to suggest automation skills
        let norm_key = ev.redacted_summary.trim().to_ascii_lowercase();
        if !norm_key.is_empty() {
            let entry = pattern_counts
                .entry(norm_key)
                .or_insert_with(|| (0, ev.app_name.clone()));
            entry.0 += 1;
        }

        if ev.importance >= 0.5 && state.nexusmind.configured() {
            let mem_type = match ev.category.as_str() {
                "preference" => "preference",
                "procedural_skill" => "pattern",
                "project_knowledge" => "discovery",
                _ => "manual",
            };
            let scope = if ev.category == "preference" || ev.category == "procedural_skill" {
                "global_user"
            } else {
                "device"
            };
            let slug = format!(
                "{}-{}",
                ev.category,
                ev.id.chars().take(8).collect::<String>()
            );
            let payload = MemoryPayload {
                title: format!(
                    "Mascot learned ({}): {}",
                    ev.category,
                    ev.redacted_summary.chars().take(60).collect::<String>()
                ),
                memory_type: mem_type.into(),
                content: format!(
                    "**Source:** {}\n**App:** {}\n**Observed:** {}",
                    ev.source,
                    ev.app_name.as_deref().unwrap_or("desktop"),
                    ev.redacted_summary
                ),
                topic_key: slug,
                scope: scope.into(),
            };
            if state.nexusmind.store_memory(&payload).await.is_ok() {
                synced += 1;
            }
        }
        processed_ids.push(ev.id.clone());
    }

    // Queue proactive automation proposals for workflows repeated >= 3 times
    let mut suggestions_created = 0_usize;
    let router = state.router.read().unwrap().clone();
    for (pattern, (count, app)) in pattern_counts {
        if count >= 3 {
            let delivery = router
                .evaluate_proactive_interrupt(&ProactiveInterruptInput {
                    suggestion_summary: format!("Automate repeated task ({count}x): {pattern}"),
                    active_app: app.clone(),
                    user_in_call_or_fullscreen: false,
                    seconds_since_last_input: 120,
                })
                .await
                .map(|e| e.answer.action)
                .unwrap_or_else(|_| "queue_for_idle".into());

            let slug = format!("auto-{}", uuid::Uuid::new_v4().to_string().chars().take(6).collect::<String>());
            let _: ProactiveSuggestion = state.store.enqueue_suggestion(
                "Automatizar rutina repetitiva",
                &format!(
                    "Detecté que realizaste esta acción {count} veces ({}). ¿Quieres que la convierta en una Skill ejecutable en 1 clic?",
                    pattern.chars().take(90).collect::<String>()
                ),
                Some(&slug),
                &delivery,
            )?;
            suggestions_created += 1;
        }
    }

    state.store.mark_events_consolidated(&processed_ids)?;
    state.set_visual_state(MascotVisualState::Idle);

    Ok(ConsolidationReport {
        consolidated_events: processed_ids.len(),
        synced_to_nexusmind: synced,
        suggestions_created,
    })
}
