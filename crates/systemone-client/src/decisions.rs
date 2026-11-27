//! Typed decision contracts for the NexusMind Desktop Mascot.
//!
//! Each contract builds a bounded `/v1/systemone` request, validates the response,
//! and produces a strongly typed answer along with routing metadata.

use crate::protocol::{parse_choice, parse_noul, parse_score, SystemOneRequest, SystemOneResponse};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;

// ---------------------------------------------------------------------------
// 1. Action Risk Gate (before executing shell / UI / browser / MCP action)
// ---------------------------------------------------------------------------

pub const ACTION_CLASSES: &[(&str, &str)] = &[
    ("read_only", "Reads local files, screen, or status without modifying state"),
    ("reversible_local", "Modifies local files/windows in a way easily undone"),
    ("app_automation", "Interacts with desktop apps or browser tabs"),
    ("external_communication", "Sends email, chat message, webhook, or network post"),
    ("system_config", "Changes OS settings, dotfiles, packages, or permissions"),
    ("destructive_or_financial", "Deletes data permanently, handles money, auth, or secrets"),
];

pub const REVERSIBILITY_CHOICES: &[(&str, &str)] = &[
    ("reversible", "Action can be cleanly undone without data loss"),
    ("partially_reversible", "Undo is possible but requires manual cleanup"),
    ("irreversible", "Action cannot be undone once executed"),
];

pub const ACTION_RISK_RUBRIC: &[&str] = &[
    "Low: read-only inspection, harmless search, or trivial reversible local edit",
    "Medium: modifies user workspace, opens/clicks apps, or runs local scripts",
    "High: external communication, bulk file mutation, or system/package changes",
    "Critical: destructive deletion, credentials/secrets, security settings, or financial actions",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionRiskInput {
    pub action_kind: String,
    pub summary: String,
    pub target_app_or_path: Option<String>,
    pub command_preview: Option<String>,
    pub is_private_context: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActionRiskAnswer {
    pub action_class: String,
    pub class_confidence: f64,
    pub reversibility: String,
    pub reversibility_confidence: f64,
    /// Normalized risk `0.0..=1.0`.
    pub risk: f64,
    pub risk_confidence: f64,
}

pub fn build_action_risk_request(model: &str, input: &ActionRiskInput) -> SystemOneRequest {
    let summary: String = input.summary.chars().take(600).collect();
    let preview: Option<String> = input
        .command_preview
        .as_ref()
        .map(|s| s.chars().take(600).collect());
    SystemOneRequest::new(
        model,
        json!({
            "action_kind": input.action_kind,
            "summary": summary,
            "target": input.target_app_or_path,
            "command_preview": preview,
        }),
    )
    .with_choice(
        "action_class",
        "Classify what kind of desktop action the assistant is about to execute.",
        ACTION_CLASSES,
    )
    .with_choice(
        "reversibility",
        "How reversible is this action if the assistant makes a mistake?",
        REVERSIBILITY_CHOICES,
    )
    .with_score(
        "risk",
        "How risky is it to execute this action autonomously without asking the user first?",
        ACTION_RISK_RUBRIC,
    )
}

pub fn parse_action_risk_response(resp: &SystemOneResponse) -> Result<ActionRiskAnswer> {
    let allowed_classes: Vec<&str> = ACTION_CLASSES.iter().map(|(k, _)| *k).collect();
    let allowed_rev: Vec<&str> = REVERSIBILITY_CHOICES.iter().map(|(k, _)| *k).collect();
    let class = parse_choice(resp, "action_class", &allowed_classes)?;
    let rev = parse_choice(resp, "reversibility", &allowed_rev)?;
    let risk = parse_score(resp, "risk", ACTION_RISK_RUBRIC.len())?;

    Ok(ActionRiskAnswer {
        action_class: class.choice,
        class_confidence: class.confidence,
        reversibility: rev.choice,
        reversibility_confidence: rev.confidence,
        risk: risk.normalized,
        risk_confidence: risk.confidence,
    })
}

// ---------------------------------------------------------------------------
// 2. Memory Worthiness (continuous learning noise filter)
// ---------------------------------------------------------------------------

pub const MEMORY_WORTHINESS_CHOICES: &[(&str, &str)] = &[
    ("ignore", "Transient noise; not worth remembering"),
    ("ephemeral", "Useful only within the next few hours of this session"),
    ("preference", "Reveals a durable user preference, habit, or style"),
    ("project_knowledge", "Reveals durable project paths, architecture, or commands"),
    ("procedural_skill", "Repeatable multi-step workflow worth saving as a skill"),
];

pub const MEMORY_IMPORTANCE_RUBRIC: &[&str] = &[
    "Zero: pure noise or one-off throwaway action",
    "Low: minor contextual hint",
    "Medium: useful recurring fact or preference",
    "High: critical rule, correction, or reusable workflow",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEventInput {
    pub source: String,
    pub app_name: Option<String>,
    pub redacted_summary: String,
    pub repeat_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryWorthinessAnswer {
    pub category: String,
    pub category_confidence: f64,
    pub importance: f64,
    pub importance_confidence: f64,
}

pub fn build_memory_worthiness_request(
    model: &str,
    input: &MemoryEventInput,
) -> SystemOneRequest {
    let summary: String = input.redacted_summary.chars().take(1000).collect();
    SystemOneRequest::new(
        model,
        json!({
            "source": input.source,
            "app_name": input.app_name,
            "summary": summary,
            "repeat_count": input.repeat_count,
        }),
    )
    .with_choice(
        "worthiness",
        "Should this observed event be discarded or persisted into long-term memory?",
        MEMORY_WORTHINESS_CHOICES,
    )
    .with_score(
        "importance",
        "How valuable will remembering this be in future sessions?",
        MEMORY_IMPORTANCE_RUBRIC,
    )
}

pub fn parse_memory_worthiness_response(
    resp: &SystemOneResponse,
) -> Result<MemoryWorthinessAnswer> {
    let allowed: Vec<&str> = MEMORY_WORTHINESS_CHOICES.iter().map(|(k, _)| *k).collect();
    let cat = parse_choice(resp, "worthiness", &allowed)?;
    let imp = parse_score(resp, "importance", MEMORY_IMPORTANCE_RUBRIC.len())?;
    Ok(MemoryWorthinessAnswer {
        category: cat.choice,
        category_confidence: cat.confidence,
        importance: imp.normalized,
        importance_confidence: imp.confidence,
    })
}

// ---------------------------------------------------------------------------
// 3. Executor Router (choose fastest reliable execution tier)
// ---------------------------------------------------------------------------

pub const EXECUTOR_ROUTES: &[(&str, &str)] = &[
    ("saved_skill", "Matches an existing learned skill or shortcut directly"),
    ("direct_os_tool", "Simple shell, filesystem, Spotlight, or AppleScript action"),
    ("browser_automation", "Web navigation, scraping, or form filling in browser"),
    ("coding_nexus_cli", "Software engineering task inside a git repository"),
    ("ui_computer_use", "Requires Accessibility tree or visual desktop UI control"),
    ("general_llm", "Conversational answer, drafting, or multi-step reasoning"),
];

pub const MODEL_TIER_CHOICES: &[(&str, &str)] = &[
    (
        "fast",
        "Everyday conversation, quick Q&A, memory lookup, calendar checks, or simple single-step commands",
    ),
    (
        "heavy",
        "Complex tasks: building or editing an app/codebase, creating rich documents/presentations/reports, UI/architecture design, or deep multi-app workflows",
    ),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteTaskInput {
    pub user_request: String,
    pub active_app: Option<String>,
    pub matching_skills: Vec<String>,
    pub inside_git_repo: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteTaskAnswer {
    pub route: String,
    pub confidence: f64,
    pub model_tier: String,
    pub model_tier_confidence: f64,
}

pub fn build_route_task_request(model: &str, input: &RouteTaskInput) -> SystemOneRequest {
    let req: String = input.user_request.chars().take(1000).collect();
    SystemOneRequest::new(
        model,
        json!({
            "user_request": req,
            "active_app": input.active_app,
            "matching_skills": input.matching_skills,
            "inside_git_repo": input.inside_git_repo,
        }),
    )
    .with_choice(
        "route",
        "Select the primary execution path best suited to fulfill the user request.",
        EXECUTOR_ROUTES,
    )
    .with_choice(
        "model_tier",
        "Choose whether this request can be handled by the fast model or requires the heavy reasoning model.",
        MODEL_TIER_CHOICES,
    )
}

pub fn parse_route_task_response(resp: &SystemOneResponse) -> Result<RouteTaskAnswer> {
    let allowed: Vec<&str> = EXECUTOR_ROUTES.iter().map(|(k, _)| *k).collect();
    let c = parse_choice(resp, "route", &allowed)?;
    let tier_allowed: Vec<&str> = MODEL_TIER_CHOICES.iter().map(|(k, _)| *k).collect();
    let (model_tier, model_tier_confidence) =
        match parse_choice(resp, "model_tier", &tier_allowed) {
            Ok(t) => (t.choice, t.confidence),
            Err(_) => ("fast".to_string(), 0.0),
        };
    Ok(RouteTaskAnswer {
        route: c.choice,
        confidence: c.confidence,
        model_tier,
        model_tier_confidence,
    })
}

// ---------------------------------------------------------------------------
// 4. Proactive Interrupt Gate (should the mascot speak up right now?)
// ---------------------------------------------------------------------------

pub const INTERRUPT_CHOICES: &[(&str, &str)] = &[
    ("notify_now", "Urgent or immediately helpful without disrupting focus"),
    ("queue_for_idle", "Useful suggestion, wait until the user pauses or finishes"),
    ("silent_log", "Record quietly; do not interrupt the user"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProactiveInterruptInput {
    pub suggestion_summary: String,
    pub active_app: Option<String>,
    pub user_in_call_or_fullscreen: bool,
    pub seconds_since_last_input: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProactiveInterruptAnswer {
    pub action: String,
    pub confidence: f64,
}

pub fn build_proactive_interrupt_request(
    model: &str,
    input: &ProactiveInterruptInput,
) -> SystemOneRequest {
    SystemOneRequest::new(
        model,
        json!({
            "suggestion_summary": input.suggestion_summary.chars().take(400).collect::<String>(),
            "active_app": input.active_app,
            "user_in_call_or_fullscreen": input.user_in_call_or_fullscreen,
            "seconds_since_last_input": input.seconds_since_last_input,
        }),
    )
    .with_choice(
        "interrupt_action",
        "Decide whether the desktop mascot should notify the user now or wait.",
        INTERRUPT_CHOICES,
    )
}

pub fn parse_proactive_interrupt_response(
    resp: &SystemOneResponse,
) -> Result<ProactiveInterruptAnswer> {
    let allowed: Vec<&str> = INTERRUPT_CHOICES.iter().map(|(k, _)| *k).collect();
    let c = parse_choice(resp, "interrupt_action", &allowed)?;
    Ok(ProactiveInterruptAnswer {
        action: c.choice,
        confidence: c.confidence,
    })
}

// ---------------------------------------------------------------------------
// 5. Completion Judge (verify multi-step desktop task finished cleanly)
// ---------------------------------------------------------------------------

pub const COMPLETION_CHOICES: &[(&str, &str)] = &[
    ("finish", "Objective achieved and verified with concrete evidence"),
    ("retry", "A step failed and another attempt can fix it"),
    ("retrieve_more", "Missing context or screen/file state to proceed"),
    ("human_review", "Ambiguous outcome or requires human confirmation"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionJudgeInput {
    pub objective: String,
    pub steps_executed: u32,
    pub tool_errors: u32,
    pub evidence_summary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompletionJudgeAnswer {
    pub next_action: String,
    pub confidence: f64,
    pub incomplete_probability: f64,
}

pub fn build_completion_judge_request(
    model: &str,
    input: &CompletionJudgeInput,
) -> SystemOneRequest {
    SystemOneRequest::new(
        model,
        json!({
            "objective": input.objective.chars().take(600).collect::<String>(),
            "steps_executed": input.steps_executed,
            "tool_errors": input.tool_errors,
            "evidence_summary": input.evidence_summary.chars().take(1200).collect::<String>(),
        }),
    )
    .with_choice(
        "next_action",
        "Based only on the supplied evidence, what should the agent do next?",
        COMPLETION_CHOICES,
    )
    .with_noul(
        "task_is_incomplete",
        "Does the evidence indicate the objective is still incomplete?",
        Some("Work remains or verification failed"),
        Some("Objective is fully satisfied"),
    )
}

pub fn parse_completion_judge_response(resp: &SystemOneResponse) -> Result<CompletionJudgeAnswer> {
    let allowed: Vec<&str> = COMPLETION_CHOICES.iter().map(|(k, _)| *k).collect();
    let next = parse_choice(resp, "next_action", &allowed)?;
    let inc = parse_noul(resp, "task_is_incomplete")?;
    Ok(CompletionJudgeAnswer {
        next_action: next.choice,
        confidence: next.confidence,
        incomplete_probability: inc.probability,
    })
}
