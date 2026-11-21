//! Error audits: before a new (or regressed) error gets its GitHub issue, an
//! `error_audit` agent with the executor the source chose reads the code of the
//! candidate repositories, explains the root cause and names the repository
//! that causes it. Monitoring then opens the issue there.
//!
//! Trust: the error text is attacker-influenced. The agent only proposes a
//! repository; it is accepted only inside the source's GitHub owner and when the
//! source's credential can read it. Anything else falls back to the source's
//! default repository. The agent is read-only and never publishes.

use serde_json::{json, Value};

use crate::db::monitor::{self as store_monitor, MonitorGroup, MonitorSource};
use crate::db::queries;
use crate::store::sqlite::SqliteStore;

pub const TEMPLATE: &str = "error_audit";
/// Audits started per source per UTC day; beyond it the issue opens unaudited.
pub const DAILY_AUDITS: i64 = 10;
/// Repositories checked out for one audit (the first is the working directory).
const MAX_CANDIDATES: usize = 5;
pub const EXECUTORS: [&str; 4] = ["claude", "codex", "pi", "nexus"];
/// Words that say nothing about which repository a service comes from.
const GENERIC: [&str; 14] = [
    "beta", "prod", "production", "staging", "stage", "dev", "qa", "test", "service", "services", "stack", "api",
    "app", "svc",
];

// ---------------------------------------------------------------- candidates

/// Owner of the source's default repository: audits stay inside it.
pub fn owner_of(source: &MonitorSource) -> Option<String> {
    source.github_repo.as_deref().and_then(|repo| repo.split_once('/')).map(|(owner, _)| owner.to_string())
}

fn tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(str::to_string)
        .collect()
}

/// How well a repository name matches a service name, ignoring the owner and
/// environment words (`kasymir-beta-sales` ~ `kasymir-sales-service`).
pub fn name_score(repo_name: &str, service: &str, owner: &str) -> usize {
    let owner = owner.to_ascii_lowercase();
    let meaningful = |t: &String| !GENERIC.contains(&t.as_str()) && *t != owner;
    let repo: Vec<String> = tokens(repo_name).into_iter().filter(meaningful).collect();
    tokens(service).into_iter().filter(meaningful).filter(|t| repo.contains(t)).count()
}

/// Names that say which code an error belongs to: the service, and for
/// database errors the database (`db=analytics_service`).
pub fn hint_names(group: &MonitorGroup) -> Vec<String> {
    let mut names = vec![group.service.clone()];
    if let Some(db) = group.title.split(|c: char| c == ',' || c.is_whitespace()).find_map(|part| part.strip_prefix("db=")) {
        if db.len() >= 3 && db.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            names.push(db.to_string());
        }
    }
    names
}

/// Literal strings worth a GitHub code search: the route of a failing request,
/// the longest fixed part of the message, the file of the top stack frame.
pub fn search_terms(group: &MonitorGroup) -> Vec<String> {
    let mut terms = Vec::new();
    let title = group.title.trim();
    if let Some(rest) = title.strip_prefix("HTTP ") {
        // `HTTP 502 GET /api/live-view/occupancy after 5.9s`
        if let Some(path) = rest.split_whitespace().nth(2).filter(|p| p.starts_with('/')) {
            let segments: Vec<&str> = path
                .split('/')
                .filter(|s| !s.is_empty() && !s.chars().any(|c| c.is_ascii_digit()) && s.len() < 40)
                .collect();
            let tail = segments[segments.len().saturating_sub(2)..].join("/");
            if tail.len() >= 4 {
                terms.push(tail);
            }
        }
    } else {
        // A quoted identifier (a table, column or key) is the most searchable part.
        if let Some(identifier) = title
            .split('"')
            .skip(1)
            .step_by(2)
            .map(|quoted| quoted.rsplit('.').next().unwrap_or(quoted))
            .find(|id| id.len() >= 4 && id.len() <= 60 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        {
            terms.push(identifier.to_string());
        }
        let normalized = super::normalize_message(title);
        if let Some(fragment) = normalized
            .split(['<', '>'])
            .map(|part| part.trim_matches(|c: char| !c.is_ascii_alphanumeric()).trim())
            .filter(|part| part.len() >= 12 && !matches!(*part, "n" | "str" | "uuid" | "hex" | "email"))
            .max_by_key(|part| part.len())
        {
            terms.push(fragment.chars().take(80).collect());
        }
    }
    let sample: Value = serde_json::from_str(&group.sample).unwrap_or_default();
    if let Some(frame) = sample["stack"].as_str().and_then(super::top_frame) {
        let file = frame
            .rsplit(['/', '\\', '('])
            .next()
            .unwrap_or_default()
            .split(['.', '"', ':', ')'])
            .next()
            .unwrap_or_default()
            .to_string();
        if file.len() >= 4 && !matches!(file.as_str(), "index" | "main" | "server" | "app") {
            terms.push(file);
        }
    }
    terms.retain(|t| !t.contains('"'));
    terms.truncate(3);
    terms
}

/// Ranks names by score, best first, keeping only scored ones, then appends the
/// default repository; at most [`MAX_CANDIDATES`].
pub fn rank(scores: &std::collections::HashMap<String, usize>, fallback: &str) -> Vec<String> {
    let mut ranked: Vec<(&String, &usize)> = scores.iter().filter(|(_, s)| **s > 0).collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let mut out: Vec<String> = ranked.into_iter().map(|(name, _)| name.clone()).take(MAX_CANDIDATES).collect();
    if !out.iter().any(|r| r.eq_ignore_ascii_case(fallback)) {
        if out.len() == MAX_CANDIDATES {
            out.pop();
        }
        out.push(fallback.to_string());
    }
    out
}

async fn owner_repositories(token: &str, owner: &str) -> Vec<String> {
    for path in [format!("/orgs/{owner}/repos?per_page=100&type=all"), format!("/users/{owner}/repos?per_page=100")] {
        if let Ok(Value::Array(repos)) = crate::automation::connectors::github_get(token, &path).await {
            return repos
                .iter()
                .filter(|r| r["archived"].as_bool() != Some(true))
                .filter_map(|r| r["full_name"].as_str().map(str::to_string))
                .collect();
        }
    }
    Vec::new()
}

async fn code_search(token: &str, owner: &str, term: &str) -> Vec<String> {
    for qualifier in ["org", "user"] {
        let query = format!("\"{term}\" {qualifier}:{owner}");
        let Ok(url) = reqwest::Url::parse_with_params("https://api.github.com/search/code", &[("q", query.as_str()), ("per_page", "20")]) else {
            return Vec::new();
        };
        let path = format!("/search/code?{}", url.query().unwrap_or_default());
        if let Ok(found) = crate::automation::connectors::github_get(token, &path).await {
            return found["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item.pointer("/repository/full_name").and_then(Value::as_str).map(str::to_string))
                .collect();
        }
    }
    Vec::new()
}

/// Candidate repositories for one error, all inside `owner`: name match with
/// the service, plus repositories whose code contains the error's literals.
pub async fn candidates(token: &str, owner: &str, fallback: &str, group: &MonitorGroup) -> Vec<String> {
    let mut scores: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for repo in owner_repositories(token, owner).await {
        let name = repo.split_once('/').map(|(_, n)| n).unwrap_or(&repo).to_string();
        let score = hint_names(group).iter().map(|hint| name_score(&name, hint, owner)).max().unwrap_or(0) * 3;
        scores.insert(repo, score);
    }
    for term in search_terms(group) {
        for repo in code_search(token, owner, &term).await {
            *scores.entry(repo).or_default() += 5;
        }
    }
    scores.retain(|repo, _| in_owner(repo, owner));
    rank(&scores, fallback)
}

pub fn in_owner(repository: &str, owner: &str) -> bool {
    crate::automation::connectors::validate_repository(repository).is_ok()
        && repository.split_once('/').is_some_and(|(o, _)| o.eq_ignore_ascii_case(owner))
}

// ---------------------------------------------------------------- managed agent

/// Creates, updates or disables the source's managed `error_audit` agent so it
/// matches the source's audit settings. Returns the agent id to store.
pub fn sync_agent(conn: &rusqlite::Connection, user_id: &str, source: &MonitorSource) -> anyhow::Result<Option<String>> {
    let existing = match source.audit_agent_id.as_deref() {
        Some(id) => queries::get_autonomous_agent_detail(conn, &source.org_id, id)?.map(|_| id.to_string()),
        None => None,
    };
    let Some(executor) = source.audit_executor.as_deref() else {
        if let Some(id) = &existing {
            queries::set_autonomous_agent_status(conn, &source.org_id, id, "disabled")?;
        }
        return Ok(existing);
    };
    let owner = owner_of(source).ok_or_else(|| anyhow::anyhow!("audit_needs_github_repository"))?;
    let mut config = json!({"executor": executor, "github_owner": owner, "monitor_source_id": source.id});
    if let Some(model) = source.audit_model.as_deref() {
        config["model_source"] = json!("agent");
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
                .ok_or_else(|| anyhow::anyhow!("error_audit_template_missing"))?;
            let short: String = source.id.chars().take(8).collect();
            let name: String = format!("Error audit · {} · {short}", source.name).chars().take(120).collect();
            let create = crate::models::types::CreateAutonomousAgentRequest {
                name,
                description: Some("Managed by Monitoring. Change it from the Monitoring page.".into()),
                template_key: TEMPLATE.into(),
                config,
                budgets,
            };
            queries::create_autonomous_agent_definition(conn, &source.org_id, user_id, &create)?.definition.id
        }
    };
    let detail = queries::validate_autonomous_agent_definition(conn, &source.org_id, user_id, &id)?
        .ok_or_else(|| anyhow::anyhow!("error_audit_agent_missing"))?;
    if detail.revision.validation_status != "valid" {
        anyhow::bail!("error_audit_agent_invalid");
    }
    queries::set_autonomous_agent_status(conn, &source.org_id, &id, "enabled")?;
    Ok(Some(id))
}

// ---------------------------------------------------------------- start

/// The run input: the error (already redacted when stored) and the candidates.
pub fn trigger(source: &MonitorSource, group: &MonitorGroup, candidates: &[String]) -> Value {
    let sample: Value = serde_json::from_str(&group.sample).unwrap_or_default();
    json!({"trigger": {
        "kind": "monitor_error",
        "source_id": source.id,
        "group_id": group.id,
        "candidates": candidates,
        "error": {
            "service": group.service,
            "severity": group.severity,
            "title": group.title,
            "message": sample["message"],
            "stack": sample["stack"],
            "count": group.count,
            "first_seen": group.first_seen,
            "last_seen": group.last_seen,
        },
    }})
}

/// Starts an audit for a group when the source has one configured and its
/// daily cap allows it. `Ok(false)`: no audit; the caller opens the issue now.
pub async fn start(store: &SqliteStore, source: &MonitorSource, group: &MonitorGroup) -> anyhow::Result<bool> {
    let (Some(agent_id), Some(connector_id), Some(fallback)) =
        (source.audit_agent_id.as_deref(), source.github_connector_id.as_deref(), source.github_repo.as_deref())
    else {
        return Ok(false);
    };
    if source.audit_executor.is_none() {
        return Ok(false);
    }
    let owner = owner_of(source).ok_or_else(|| anyhow::anyhow!("audit_needs_github_repository"))?;
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        if store_monitor::audits_today(&conn, &source.id)? >= DAILY_AUDITS {
            return Ok(false);
        }
    }
    let token = super::delivery::github_token(store, &source.org_id, connector_id).await?;
    let candidates = candidates(&token, &owner, fallback, group).await;
    let input = trigger(source, group, &candidates);
    let db = store.conn();
    let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
    let run = queries::enqueue_autonomous_agent_run(
        &conn,
        &source.org_id,
        agent_id,
        "manual",
        &format!("monitor:{}:{}", group.id, group.count),
        None,
        Some(&input),
    )?
    .ok_or_else(|| anyhow::anyhow!("error_audit_agent_missing"))?;
    store_monitor::start_audit(&conn, &group.id, &run.id)
}

// ---------------------------------------------------------------- worker hooks

/// The source an audit run belongs to, checked against the run's own agent.
fn run_source(conn: &rusqlite::Connection, claim: &queries::ClaimedAutonomousRun) -> anyhow::Result<MonitorSource> {
    let source_id = claim.config.pointer("/trigger/source_id").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("audit_source_missing"))?;
    let source = store_monitor::get_source(conn, &claim.org_id, source_id)?.ok_or_else(|| anyhow::anyhow!("audit_source_missing"))?;
    if source.audit_agent_id.as_deref() != Some(claim.run.definition_id.as_str()) {
        anyhow::bail!("audit_source_mismatch");
    }
    Ok(source)
}

/// Points an audit run at its candidates: the first is checked out, the others
/// are read-only context. Only repositories inside the agent's owner survive.
pub fn scope_claim(store: &SqliteStore, claim: &queries::ClaimedAutonomousRun) -> anyhow::Result<queries::ClaimedAutonomousRun> {
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        run_source(&conn, claim)?;
    }
    let owner = claim.config["github_owner"].as_str().ok_or_else(|| anyhow::anyhow!("github_owner_required"))?;
    let mut repos: Vec<String> = claim
        .config
        .pointer("/trigger/candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|r| in_owner(r, owner))
        .map(str::to_string)
        .collect();
    repos.dedup();
    repos.truncate(MAX_CANDIDATES);
    let primary = repos.first().cloned().ok_or_else(|| anyhow::anyhow!("audit_candidates_missing"))?;
    let mut scoped = claim.clone();
    scoped.config["repository"] = json!(primary);
    scoped.config["context_repos"] = json!(repos[1..]);
    Ok(scoped)
}

/// An audit reads code with the monitor source's own GitHub credential.
pub async fn github_token_for_run(store: &SqliteStore, claim: &queries::ClaimedAutonomousRun) -> anyhow::Result<String> {
    let source = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        run_source(&conn, claim)?
    };
    let connector = source.github_connector_id.as_deref().ok_or_else(|| anyhow::anyhow!("github_connector_required"))?;
    worker_token(store, &source, connector).await
}

/// The source's GitHub token inside the autonomous worker. The operator's org
/// authorization (`FACTORY_GITHUB_PUBLISH_ORG_ID`) is set only on the backend
/// container, which checked it in `start` before creating this audit run; the
/// worker cannot re-check it and must not fail the run for that.
async fn worker_token(store: &SqliteStore, source: &MonitorSource, connector: &str) -> anyhow::Result<String> {
    if connector == super::delivery::SERVER_GITHUB {
        return crate::automation::worker::server_github_token().await;
    }
    super::delivery::github_token(store, &source.org_id, connector).await
}

/// What an audit concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub summary: String,
    pub root_cause: String,
    pub repository: Option<String>,
    pub routing_evidence: String,
    pub suggested_fix: String,
    pub confidence: String,
}

/// Reads the agent's final JSON. `None` when it is not a usable conclusion.
pub fn parse_outcome(result: &Value) -> Option<Outcome> {
    let structured = result.get("result").filter(|v| v.is_object()).unwrap_or(result);
    let text = |key: &str, max: usize| -> String {
        structured.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default().chars().take(max).collect()
    };
    let root_cause = text("root_cause", 4000);
    if root_cause.is_empty() {
        return None;
    }
    let confidence = match text("confidence", 10).to_ascii_lowercase().as_str() {
        c @ ("high" | "medium" | "low") => c.to_string(),
        _ => "low".into(),
    };
    Some(Outcome {
        summary: text("summary", 500),
        root_cause,
        repository: Some(text("repository", 140)).filter(|r| !r.is_empty()),
        routing_evidence: text("routing_evidence", 2000),
        suggested_fix: text("suggested_fix", 4000),
        confidence,
    })
}

/// The repository the issue goes to: the agent's proposal when it is inside the
/// owner and carries evidence, else the source's default repository.
pub fn choose_repository(outcome: Option<&Outcome>, owner: &str, fallback: &str) -> (String, bool) {
    match outcome.and_then(|o| o.repository.as_deref().filter(|_| !o.routing_evidence.is_empty()).map(|r| (r, o))) {
        Some((repo, _)) if in_owner(repo, owner) => (repo.to_string(), true),
        _ => (fallback.to_string(), false),
    }
}

/// The issue section with the audit's conclusion (fenced: model output that
/// read attacker-influenced logs).
pub fn issue_section(outcome: Option<&Outcome>, routed: bool, executor: &str, run_link: &str) -> String {
    let Some(o) = outcome else {
        return format!("\n## Audit\nThe automatic audit ({executor}) did not reach a conclusion; see [the run]({run_link}).\n");
    };
    let fence = |t: &str| format!("```\n{}\n```", t.replace("```", "'''"));
    let routing = if routed {
        format!("**Why this repository**\n{}\n", fence(&o.routing_evidence))
    } else {
        "The audit did not name a repository it could support, so this is the source's default repository.\n".into()
    };
    let fix = if o.suggested_fix.is_empty() { String::new() } else { format!("\n**Suggested fix**\n{}\n", fence(&o.suggested_fix)) };
    format!(
        "\n## Audit ({executor}, confidence: {})\n**Root cause**\n{}\n\n{routing}{fix}\n[Audit run]({run_link})\n",
        o.confidence,
        fence(&o.root_cause),
    )
}

/// Worker hook: an audit run ended. Opens the issue (in the routed repository)
/// and posts the outcome to Slack, whatever the run's status.
pub async fn finish(
    store: &SqliteStore,
    config: &crate::config::Config,
    claim: &queries::ClaimedAutonomousRun,
    status: &str,
    result: &Value,
) {
    if let Err(error) = finish_inner(store, config, claim, status, result).await {
        tracing::warn!(run_id = %claim.run.id, "Monitor audit completion failed: {error:#}");
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
        let Some(group) = store_monitor::group_for_audit_run(&conn, &claim.org_id, &claim.run.id)? else {
            return Ok(());
        };
        (run_source(&conn, claim)?, group)
    };
    let outcome = (status == "succeeded").then(|| parse_outcome(result)).flatten();
    let owner = owner_of(&source).unwrap_or_default();
    let fallback = source.github_repo.clone().unwrap_or_default();
    let (mut repository, mut routed) = choose_repository(outcome.as_ref(), &owner, &fallback);
    let connector = source.github_connector_id.clone().unwrap_or_default();
    let token = worker_token(store, &source, &connector).await?;
    // The proposal must be a repository the credential can actually reach.
    if routed && crate::automation::connectors::get_github_repository(&token, &repository).await.is_err() {
        (repository, routed) = (fallback.clone(), false);
    }
    let run_link = format!("{}/factory/runs?agent_run={}", config.app_base_url.trim_end_matches('/'), claim.run.id);
    let executor = source.audit_executor.clone().unwrap_or_else(|| "agent".into());
    let section = issue_section(outcome.as_ref(), routed, &executor, &run_link);
    let summary = outcome.as_ref().map(|o| if o.summary.is_empty() { o.root_cause.chars().take(300).collect() } else { o.summary.clone() });
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        store_monitor::finish_audit(&conn, &group.id, if outcome.is_some() { "done" } else { "failed" }, summary.as_deref())?;
    }
    let number = super::delivery::open_issue_in(store, &source, &group, &token, &repository, &section).await?;
    let mut group = group;
    group.issue_number = Some(number);
    group.issue_repo = Some(repository);
    let headline = match &summary {
        Some(s) => format!("🔎 Audit: {}", super::delivery::mcp_safe(s)),
        None => "🔎 The audit did not reach a conclusion; the issue has the details.".into(),
    };
    super::delivery::alert_followup(store, &source, &group, &headline, &config.app_base_url, &config.claude_code_bin).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(title: &str, stack: Option<&str>) -> MonitorGroup {
        MonitorGroup {
            id: "g1".into(),
            source_id: "s1".into(),
            service: "kasymir-beta-sales".into(),
            title: title.into(),
            severity: "ERROR".into(),
            count: 1,
            first_seen: String::new(),
            last_seen: String::new(),
            sample: json!({"message": title, "stack": stack}).to_string(),
            log_url: None,
            issue_number: None,
            status: "open".into(),
            audit_run_id: None,
            audit_status: None,
            audit_summary: None,
            issue_repo: None,
        }
    }

    #[test]
    fn service_names_match_repositories_without_owner_or_environment_words() {
        assert_eq!(name_score("kasymir-sales-service", "kasymir-beta-sales", "kasymir"), 1);
        assert_eq!(name_score("kasymir-app-ui", "kasymir-app-ui-beta", "kasymir"), 1);
        assert_eq!(name_score("kasymir-locations-service-stack", "kasymir-beta-locations", "kasymir"), 1);
        assert_eq!(name_score("kasymir-iam", "kasymir-beta-sales", "kasymir"), 0);
    }

    #[test]
    fn search_terms_come_from_routes_messages_and_stack_files() {
        assert_eq!(search_terms(&group("HTTP 502 GET /api/live-view/occupancy after 5.9s", None)), vec!["live-view/occupancy"]);
        assert_eq!(search_terms(&group("HTTP 500 POST /store/MN/3f2b8c1e-1d2a-4b5c-9d8e-7f6a5b4c3d2e/checkout after 1s", None)), vec!["MN/checkout"]);
        let terms = search_terms(&group("Order 42 could not be reserved for tenant", Some("Error: x\n    at reserve (/app/dist/inventory-reservations.js:10:5)")));
        assert_eq!(terms, vec!["could not be reserved for tenant", "inventory-reservations"]);
        assert!(search_terms(&group("boom", None)).is_empty());
    }

    #[test]
    fn database_errors_point_at_their_database_and_table() {
        let mut db_error = group("db=analytics_service,user=kb_analytics ERROR:  relation \"public.internalUser\" does not exist at character 66", None);
        db_error.service = "kasymir-beta-postgres".into();
        assert_eq!(hint_names(&db_error), vec!["kasymir-beta-postgres", "analytics_service"]);
        assert_eq!(name_score("kasymir-analytics-service-stack", "analytics_service", "kasymir"), 1);
        assert_eq!(search_terms(&db_error)[0], "internalUser");
    }

    #[test]
    fn ranking_keeps_the_best_and_always_the_default_repository() {
        let scores: std::collections::HashMap<String, usize> =
            [("k/a", 3), ("k/b", 8), ("k/c", 0), ("k/d", 5), ("k/e", 1), ("k/f", 2), ("k/g", 4)].into_iter().map(|(r, s)| (r.to_string(), s)).collect();
        assert_eq!(rank(&scores, "k/docs"), vec!["k/b", "k/d", "k/g", "k/a", "k/docs"]);
        assert_eq!(rank(&scores, "k/b"), vec!["k/b", "k/d", "k/g", "k/a", "k/f"]);
        assert_eq!(rank(&Default::default(), "k/docs"), vec!["k/docs"]);
    }

    fn outcome(repo: &str, evidence: &str) -> Outcome {
        Outcome {
            summary: "s".into(),
            root_cause: "the cause".into(),
            repository: Some(repo.into()),
            routing_evidence: evidence.into(),
            suggested_fix: String::new(),
            confidence: "high".into(),
        }
    }

    #[test]
    fn the_issue_goes_to_the_proposed_repository_only_inside_the_owner_and_with_evidence() {
        assert_eq!(choose_repository(Some(&outcome("kasymir/sales", "orders.ts:10 throws")), "kasymir", "kasymir/docs"), ("kasymir/sales".into(), true));
        assert_eq!(choose_repository(Some(&outcome("evil/repo", "trust me")), "kasymir", "kasymir/docs"), ("kasymir/docs".into(), false));
        assert_eq!(choose_repository(Some(&outcome("kasymir/sales", "")), "kasymir", "kasymir/docs"), ("kasymir/docs".into(), false));
        assert_eq!(choose_repository(Some(&outcome("not a repo", "x")), "kasymir", "kasymir/docs"), ("kasymir/docs".into(), false));
        assert_eq!(choose_repository(None, "kasymir", "kasymir/docs"), ("kasymir/docs".into(), false));
    }

    #[test]
    fn outcomes_parse_from_the_agent_result_and_need_a_root_cause() {
        let result = json!({"result": {"summary": "s", "root_cause": "c", "repository": "k/r", "routing_evidence": "e", "confidence": "HIGH"}});
        let parsed = parse_outcome(&result).unwrap();
        assert_eq!((parsed.repository.as_deref(), parsed.confidence.as_str()), (Some("k/r"), "high"));
        assert_eq!(parse_outcome(&json!({"summary": "s", "confidence": "sure"})), None);
        assert_eq!(parse_outcome(&json!({"root_cause": "c", "confidence": "sure"})).unwrap().confidence, "low");
    }

    #[test]
    fn the_issue_section_fences_model_text() {
        let section = issue_section(Some(&outcome("k/r", "``` @team")), true, "codex", "https://x/run");
        assert!(section.contains("## Audit (codex, confidence: high)"));
        assert!(section.contains("```\n''' @team\n```"));
        assert!(issue_section(None, false, "pi", "https://x/run").contains("did not reach a conclusion"));
    }

    #[test]
    fn candidates_stay_inside_the_owner() {
        assert!(in_owner("kasymir/sales", "Kasymir"));
        assert!(!in_owner("other/sales", "kasymir"));
        assert!(!in_owner("kasymir", "kasymir"));
    }
}
