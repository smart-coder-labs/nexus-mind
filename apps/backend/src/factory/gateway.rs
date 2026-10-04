//! Model Gateway (factory F3, ADR c2048d9b): which model a run uses.
//!
//! The router maps a task class to a tier (cheap, standard, frontier) and the
//! tier to a model of the provider that runs it. Frontier runs are capped per
//! org and day (OD-4); past the cap a run drops to standard and says so. The
//! Claude Code CLI is the first provider (subscription, already authenticated
//! in the sandbox through the egress proxy); the Codex CLI follows.

use crate::factory::contracts::TaskClass;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Cheap,
    Standard,
    Frontier,
}

impl Tier {
    pub fn parse(value: &str) -> Option<Tier> {
        match value {
            "cheap" => Some(Tier::Cheap),
            "standard" => Some(Tier::Standard),
            "frontier" => Some(Tier::Frontier),
            _ => None,
        }
    }
}

/// Frontier runs per org and day when `FACTORY_FRONTIER_RUNS_PER_DAY` is unset.
pub const DEFAULT_FRONTIER_RUNS_PER_DAY: i64 = 20;

pub fn frontier_runs_per_day() -> i64 {
    std::env::var("FACTORY_FRONTIER_RUNS_PER_DAY")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &i64| *n >= 0)
        .unwrap_or(DEFAULT_FRONTIER_RUNS_PER_DAY)
}

/// The tier a task class needs: changes that cannot break production run cheap,
/// those that can break shared or sensitive behavior run at the frontier.
pub fn tier_for_class(class: TaskClass) -> Tier {
    match class {
        TaskClass::Docs | TaskClass::Tests => Tier::Cheap,
        TaskClass::Migration | TaskClass::Infra | TaskClass::Security => Tier::Frontier,
        TaskClass::Ui
        | TaskClass::Backend
        | TaskClass::Bugfix
        | TaskClass::Refactor
        | TaskClass::Unknown => Tier::Standard,
    }
}

/// A task class read from issue labels (`security`, `docs`, `ci`, …), if any
/// label says. The most sensitive class wins.
pub fn class_from_labels(labels: &[String]) -> Option<TaskClass> {
    let has = |needles: &[&str]| {
        labels.iter().any(|label| {
            let label = label.to_ascii_lowercase();
            needles.iter().any(|needle| label.contains(needle))
        })
    };
    if has(&["security", "auth", "vulnerab"]) {
        Some(TaskClass::Security)
    } else if has(&["migration", "database", "schema"]) {
        Some(TaskClass::Migration)
    } else if has(&["infra", "ci", "deploy", "devops"]) {
        Some(TaskClass::Infra)
    } else if has(&["test"]) {
        Some(TaskClass::Tests)
    } else if has(&["doc"]) {
        Some(TaskClass::Docs)
    } else if has(&["ui", "design", "frontend", "a11y"]) {
        Some(TaskClass::Ui)
    } else if has(&["bug"]) {
        Some(TaskClass::Bugfix)
    } else {
        None
    }
}

/// The tier of a template when its run has no task class.
pub fn template_tier(template_key: &str) -> Tier {
    match template_key {
        "security_scan" | "security_dast" => Tier::Frontier,
        "content_manager" | "lead_generation" => Tier::Cheap,
        _ => Tier::Standard,
    }
}

/// Claude Code's model alias for a tier.
pub fn claude_model(tier: Tier) -> &'static str {
    match tier {
        Tier::Cheap => "haiku",
        Tier::Standard => "sonnet",
        Tier::Frontier => "opus",
    }
}

/// Explicit model names an agent may pin (`config.model`): Claude Code aliases
/// or full model ids. Anything else is ignored, never passed to the CLI.
fn pinned_model(value: &str) -> bool {
    matches!(value, "haiku" | "sonnet" | "opus")
        || (value.starts_with("claude-")
            && value.len() <= 64
            && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.'))
}

/// The model a run uses and why.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Choice {
    pub tier: Tier,
    pub model: String,
    pub reason: String,
}

/// What the router knows about a run when it starts.
#[derive(Clone, Debug, Default)]
pub struct RunFacts<'a> {
    pub template_key: &'a str,
    pub class: Option<TaskClass>,
    /// `config.model_tier`, set by an admin on the agent.
    pub configured_tier: Option<&'a str>,
    /// `config.model`, an explicit pin.
    pub configured_model: Option<&'a str>,
    /// Frontier runs this org already made today.
    pub frontier_used_today: i64,
    pub frontier_cap: i64,
}

/// Picks the run's model: an explicit pin, else the configured tier, else the
/// task class's tier, else the template's. A frontier choice past the daily cap
/// drops to standard.
pub fn choose(facts: &RunFacts<'_>) -> Choice {
    if let Some(model) = facts.configured_model.filter(|m| pinned_model(m)) {
        let tier = match model {
            "haiku" => Tier::Cheap,
            "opus" => Tier::Frontier,
            m if m.contains("opus") => Tier::Frontier,
            m if m.contains("haiku") => Tier::Cheap,
            _ => Tier::Standard,
        };
        return Choice { tier, model: model.to_string(), reason: "pinned by the agent".into() };
    }
    let (tier, mut reason) = if let Some(tier) = facts.configured_tier.and_then(Tier::parse) {
        (tier, "tier set on the agent".to_string())
    } else if let Some(class) = facts.class {
        (tier_for_class(class), format!("task class {class:?}").to_lowercase())
    } else {
        (template_tier(facts.template_key), format!("default for {}", facts.template_key))
    };
    let tier = if tier == Tier::Frontier && facts.frontier_used_today >= facts.frontier_cap {
        reason = format!("{reason}; frontier cap of {} runs today reached", facts.frontier_cap);
        Tier::Standard
    } else {
        tier
    };
    Choice { tier, model: claude_model(tier).to_string(), reason }
}

/// Frontier runs an org made today, from the run metrics (model names carry
/// the family). Runs still in flight are not counted, so the cap can be
/// overshot by the runs that start together.
pub fn frontier_runs_today(conn: &rusqlite::Connection, org_id: &str) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM factory_run_metrics
         WHERE org_id = ?1 AND date(created_at) = date('now') AND model LIKE '%opus%'",
        [org_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>() -> RunFacts<'a> {
        RunFacts { template_key: "github_issue_resolver", frontier_cap: 20, ..Default::default() }
    }

    #[test]
    fn classes_map_to_tiers_and_tiers_to_claude_models() {
        assert_eq!(tier_for_class(TaskClass::Docs), Tier::Cheap);
        assert_eq!(tier_for_class(TaskClass::Bugfix), Tier::Standard);
        assert_eq!(tier_for_class(TaskClass::Security), Tier::Frontier);
        assert_eq!(claude_model(Tier::Cheap), "haiku");
        assert_eq!(claude_model(Tier::Frontier), "opus");
    }

    #[test]
    fn labels_name_a_class_and_the_most_sensitive_wins() {
        let labels = |l: &[&str]| l.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(class_from_labels(&labels(&["docs", "security"])), Some(TaskClass::Security));
        assert_eq!(class_from_labels(&labels(&["Documentation"])), Some(TaskClass::Docs));
        assert_eq!(class_from_labels(&labels(&["bug", "ui"])), Some(TaskClass::Ui));
        assert_eq!(class_from_labels(&labels(&["good first issue"])), None);
    }

    #[test]
    fn a_pin_beats_a_tier_which_beats_the_class_and_template() {
        let mut f = facts();
        f.class = Some(TaskClass::Docs);
        assert_eq!(choose(&f).model, "haiku");
        f.configured_tier = Some("frontier");
        assert_eq!(choose(&f).model, "opus");
        f.configured_model = Some("claude-sonnet-5");
        let choice = choose(&f);
        assert_eq!((choice.model.as_str(), choice.tier), ("claude-sonnet-5", Tier::Standard));
        // A pin that is not a model name is ignored, never passed to the CLI.
        f.configured_model = Some("--dangerously-skip-permissions");
        assert_eq!(choose(&f).model, "opus");
        let template_only = RunFacts { template_key: "security_scan", frontier_cap: 20, ..Default::default() };
        assert_eq!(choose(&template_only).tier, Tier::Frontier);
    }

    #[test]
    fn past_the_daily_cap_a_frontier_run_drops_to_standard() {
        let mut f = facts();
        f.class = Some(TaskClass::Migration);
        f.frontier_used_today = 20;
        let choice = choose(&f);
        assert_eq!((choice.tier, choice.model.as_str()), (Tier::Standard, "sonnet"));
        assert!(choice.reason.contains("frontier cap of 20"), "{}", choice.reason);
        f.frontier_used_today = 19;
        assert_eq!(choose(&f).model, "opus");
    }
}
