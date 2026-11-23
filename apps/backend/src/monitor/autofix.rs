//! Auto-fix: a source names the `github_issue_resolver` that fixes its issues,
//! either one Monitoring creates for it (managed) or any existing resolver of
//! the org. The resolver opens a draft PR that closes the issue, and the PR
//! (or why there is none) is posted in the thread of the Slack alert for a
//! human to review. Nothing is merged automatically.
//!
//! With `autofix` on, every issue Monitoring opens is handed over, up to the
//! source's daily limit; the "Resolve issue" button hands one over by hand,
//! with no limit. A managed resolver works only on what Monitoring hands it:
//! it is left out of webhook and reconciliation intake, and a run without a
//! monitor trigger is refused.

use serde_json::{json, Value};

use crate::db::monitor::{self as store_monitor, MonitorGroup, MonitorSource};
use crate::db::queries;
use crate::store::sqlite::SqliteStore;

pub const TEMPLATE: &str = "github_issue_resolver";

/// Which resolver fixes a source's issues.
#[derive(Debug, Clone, PartialEq)]
pub enum FixAgent {
    None,
    /// One Monitoring creates and keeps for this source (server `gh` only).
    Managed,
    /// An existing resolver of the org, used as it is.
    Existing(String),
}

/// Brings the source's resolver in line with the choice and returns the id to
/// store. A managed one uses the audit's executor and model when set, Claude
/// otherwise, and the server `gh`; a chosen one is never edited.
pub fn sync_agent(conn: &rusqlite::Connection, user_id: &str, source: &MonitorSource, choice: &FixAgent) -> anyhow::Result<Option<String>> {
    let existing = source.autofix_agent_id.as_deref().filter(|_| source.autofix_agent_managed).map(str::to_string);
    if *choice != FixAgent::Managed {
        if let Some(own) = existing.as_deref() {
            if !matches!(choice, FixAgent::Existing(id) if id == own) {
                queries::set_autonomous_agent_status(conn, &source.org_id, own, "disabled")?;
            }
        }
        return Ok(match choice {
            FixAgent::Existing(id) => Some(id.clone()),
            _ => None,
        });
    }
    let repository = source.github_repo.as_deref().ok_or_else(|| anyhow::anyhow!("autofix_needs_github_repository"))?;
    let mut config = json!({
        "executor": source.audit_executor.as_deref().unwrap_or("claude"),
        "repository": repository,
        "monitor_source_id": source.id,
        // Without it a resolver goes through the unwired factory model gateway
        // and every run blocks with factory_chatgpt_oauth_not_connected.
        "model_source": "agent",
    });
    if let Some(model) = source.audit_model.as_deref() {
        config["model"] = json!(model);
    }
    let id = match existing {
        Some(id) => {
            let update = crate::models::types::UpdateAutonomousAgentRequest { name: None, description: None, config: Some(config), budgets: None };
            queries::update_autonomous_agent_definition(conn, &source.org_id, user_id, &id, &update)?;
            id
        }
        None => {
            let budgets = crate::api::autonomous_agents::managed_templates()
                .into_iter()
                .find(|t| t.key == TEMPLATE)
                .map(|t| t.default_budgets)
                .ok_or_else(|| anyhow::anyhow!("resolver_template_missing"))?;
            let short: String = source.id.chars().take(8).collect();
            let name: String = format!("Auto-fix · {} · {short}", source.name).chars().take(120).collect();
            let create = crate::models::types::CreateAutonomousAgentRequest {
                name,
                description: Some("Managed by Monitoring: fixes the issues it opens. Change it from the Monitoring page.".into()),
                template_key: TEMPLATE.into(),
                config,
                budgets,
            };
            queries::create_autonomous_agent_definition(conn, &source.org_id, user_id, &create)?.definition.id
        }
    };
    let detail = queries::validate_autonomous_agent_definition(conn, &source.org_id, user_id, &id)?
        .ok_or_else(|| anyhow::anyhow!("autofix_agent_missing"))?;
    if detail.revision.validation_status != "valid" {
        anyhow::bail!("autofix_agent_invalid");
    }
    queries::set_autonomous_agent_status(conn, &source.org_id, &id, "enabled")?;
    Ok(Some(id))
}

/// Hands the group's issue to the source's resolver: automatically (auto-fix
/// on, within the daily limit) or by hand (`manual`, no limit). `Ok(false)`:
/// no resolver, auto-fix off, capped, or a fix is already running.
pub fn start(conn: &rusqlite::Connection, source: &MonitorSource, group: &MonitorGroup, repository: &str, number: i64, manual: bool) -> anyhow::Result<bool> {
    let Some(agent_id) = source.autofix_agent_id.as_deref().filter(|_| manual || source.autofix) else {
        return Ok(false);
    };
    if !manual && store_monitor::fixes_today(conn, &source.id)? >= source.autofix_daily_limit {
        return Ok(false);
    }
    let input = json!({"trigger": {
        "explicit": true,
        "kind": "github_issue",
        "repository": repository,
        "number": number,
        "monitor_source_id": source.id,
        "monitor_group_id": group.id,
    }});
    let run = queries::enqueue_autonomous_agent_run(
        conn,
        &source.org_id,
        agent_id,
        "manual",
        &format!("monitor-fix:{}:{repository}#{number}:{}", group.id, uuid::Uuid::new_v4()),
        None,
        Some(&input),
    )?
    .ok_or_else(|| anyhow::anyhow!("autofix_agent_missing"))?;
    store_monitor::start_fix(conn, &group.id, &run.id, manual)
}

/// A resolver run Monitoring is involved in: one of its managed agents, or a
/// run it handed an issue to.
pub fn is_monitor_run(claim: &queries::ClaimedAutonomousRun) -> bool {
    claim.template_key == TEMPLATE
        && (claim.config.get("monitor_source_id").is_some() || claim.config.pointer("/trigger/monitor_source_id").is_some())
}

/// Points a resolver run at the issue Monitoring handed it. A managed resolver
/// may follow the issue to any repository of the default repository's owner
/// (the audit routes issues) and refuses every other run; a chosen resolver
/// only works on its own repositories.
pub fn scope_claim(store: &SqliteStore, claim: &queries::ClaimedAutonomousRun) -> anyhow::Result<queries::ClaimedAutonomousRun> {
    let managed_for = claim.config.get("monitor_source_id").and_then(Value::as_str);
    let source_id = claim.config.pointer("/trigger/monitor_source_id").and_then(Value::as_str);
    let source_id = match (managed_for, source_id) {
        (Some(owner), Some(handed)) if owner == handed => handed,
        (None, Some(handed)) => handed,
        _ => anyhow::bail!("autofix_needs_monitor_trigger"),
    };
    let source = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        store_monitor::get_source(&conn, &claim.org_id, source_id)?.ok_or_else(|| anyhow::anyhow!("autofix_source_missing"))?
    };
    if source.autofix_agent_id.as_deref() != Some(claim.run.definition_id.as_str()) {
        anyhow::bail!("autofix_source_mismatch");
    }
    let repository = claim.config.pointer("/trigger/repository").and_then(Value::as_str).unwrap_or_default();
    let allowed = if managed_for.is_some() {
        let owner = super::audit::owner_of(&source).ok_or_else(|| anyhow::anyhow!("autofix_needs_github_repository"))?;
        super::audit::in_owner(repository, &owner)
    } else {
        own_repositories(&claim.config).iter().any(|r| r.eq_ignore_ascii_case(repository))
    };
    if !allowed || claim.config.pointer("/trigger/number").and_then(Value::as_i64).is_none() {
        anyhow::bail!("autofix_issue_outside_agent_repositories");
    }
    let mut scoped = claim.clone();
    scoped.config["repository"] = json!(repository);
    Ok(scoped)
}

/// Whether the run must branch from its repository's default branch: a managed
/// resolver follows the issue to any repository of the owner (`main`, `master`…),
/// so a fixed base never fits; a chosen one keeps the base it was configured with.
pub fn needs_default_branch(config: &Value) -> bool {
    config.get("monitor_source_id").is_some() || config.get("base_branch").and_then(Value::as_str).is_none()
}

/// The repositories a chosen resolver works on: its own and, when it is
/// project-scoped, the project's.
pub fn own_repositories(config: &Value) -> Vec<String> {
    config
        .get("repository")
        .into_iter()
        .chain(config.get("project_repositories").and_then(Value::as_array).into_iter().flatten())
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The draft PR a resolver run published, if any.
pub fn pull_request(result: &Value) -> Option<(i64, String)> {
    let pr = result.pointer("/published/draft_pull_request")?;
    let url = pr.get("html_url").and_then(Value::as_str).filter(|u| u.starts_with("https://github.com/"))?;
    Some((pr.get("number").and_then(Value::as_i64).unwrap_or_default(), url.to_string()))
}

/// The worker hook when a managed resolver run ends: records the PR and posts
/// it in the alert's thread. Never fails the run.
pub async fn finish(store: &SqliteStore, config: &crate::config::Config, claim: &queries::ClaimedAutonomousRun, status: &str, result: &Value) {
    if let Err(error) = finish_inner(store, config, claim, status, result).await {
        tracing::warn!(run = %claim.run.id, "Monitor auto-fix follow-up failed: {error:#}");
    }
}

async fn finish_inner(
    store: &SqliteStore,
    config: &crate::config::Config,
    claim: &queries::ClaimedAutonomousRun,
    status: &str,
    result: &Value,
) -> anyhow::Result<()> {
    let (source, group) = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        let Some(group) = store_monitor::group_for_fix_run(&conn, &claim.org_id, &claim.run.id)? else {
            return Ok(());
        };
        let source = store_monitor::get_source(&conn, &claim.org_id, &group.source_id)?.ok_or_else(|| anyhow::anyhow!("autofix_source_missing"))?;
        (source, group)
    };
    let pr = (status == "succeeded").then(|| pull_request(result)).flatten();
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        store_monitor::finish_fix(&conn, &group.id, if pr.is_some() { "pr_open" } else { "failed" }, pr.as_ref().map(|(_, url)| url.as_str()))?;
    }
    let run_link = format!("{}/factory/runs?agent_run={}", config.app_base_url.trim_end_matches('/'), claim.run.id);
    let headline = match &pr {
        Some((number, url)) => format!("🛠️ Fix ready for review: PR #{number} {url} (it closes the issue when merged). Review it on GitHub before merging."),
        None if result.get("no_op").and_then(Value::as_bool) == Some(true) => {
            format!("🛠️ The agent found nothing to change for this error and left a comment on the issue. Run: {run_link}")
        }
        None => format!("🛠️ The agent could not open a fix ({}). Run: {run_link}", super::delivery::mcp_safe(status)),
    };
    super::delivery::alert_followup(store, &source, &group, &headline, &config.app_base_url, &config.claude_code_bin).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_published_github_pr_counts() {
        let published = json!({"published": {"draft_pull_request": {"number": 9, "html_url": "https://github.com/acme/app/pull/9"}}});
        assert_eq!(pull_request(&published), Some((9, "https://github.com/acme/app/pull/9".into())));
        assert_eq!(pull_request(&json!({"published": {"draft_pull_request": {"number": 9, "html_url": "https://evil.example/pull/9"}}})), None);
        assert_eq!(pull_request(&json!({"no_op": true})), None);
    }

    #[test]
    fn a_managed_resolver_always_uses_the_repository_default_branch() {
        assert!(needs_default_branch(&json!({"monitor_source_id": "s", "base_branch": "main"})));
        assert!(needs_default_branch(&json!({"repository": "acme/app"})));
        assert!(!needs_default_branch(&json!({"repository": "acme/app", "base_branch": "develop"})));
    }
}
