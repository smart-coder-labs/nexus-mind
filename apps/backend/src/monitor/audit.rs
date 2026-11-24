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
pub fn sync_agent(conn: &rusqlite::Connection, user_id: &str, source: &MonitorSource, selected: Option<&str>) -> anyhow::Result<Option<String>> {
    // Only an agent Monitoring created for this source is ever edited or disabled here.
    let existing = source.audit_agent_id.as_deref().filter(|_| source.audit_agent_managed).map(str::to_string);
    if let Some(selected) = selected {
        if let Some(own) = existing.as_deref().filter(|own| *own != selected) {
            queries::set_autonomous_agent_status(conn, &source.org_id, own, "disabled")?;
        }
        return Ok(Some(selected.to_string()));
    }
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
            // Harnesses chosen on the agent itself (approved by a person) survive a source save.
            if let Some(runtime) = queries::get_autonomous_agent_detail(conn, &source.org_id, &id)?.and_then(|d| d.revision.config.get("runtime").cloned()) {
                config["runtime"] = runtime;
            }
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
        if store_monitor::audits_today(&conn, &source.id)? >= source.audit_daily_limit {
            return Ok(false);
        }
    }
    let token = super::delivery::github_token(store, &source.org_id, connector_id).await?;
    let candidates = candidates(&token, &owner, fallback, group).await;
    let mut input = trigger(source, group, &candidates);
    input["trigger"]["recent_changes"] = json!(recent_changes(&token, &candidates, &group.first_seen).await);
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
    /// The commit from `trigger.recent_changes` the agent blames, and why.
    pub culprit_commit: String,
    pub culprit_reason: String,
}

/// A recent change the audit blames, checked against the commits Monitoring
/// handed it (the agent can't invent one).
#[derive(Debug, Clone, PartialEq)]
pub struct Culprit {
    pub repository: String,
    pub sha: String,
    pub title: String,
    pub author: String,
    pub pr: Option<i64>,
    pub reason: String,
}

impl Culprit {
    /// The PR when the commit names one, else the commit.
    pub fn url(&self) -> String {
        match self.pr {
            Some(n) => format!("https://github.com/{}/pull/{n}", self.repository),
            None => format!("https://github.com/{}/commit/{}", self.repository, self.sha),
        }
    }

    pub fn label(&self) -> String {
        let short: String = self.sha.chars().take(7).collect();
        match self.pr {
            Some(n) => format!("PR #{n} in {} ({short})", self.repository),
            None => format!("commit {short} in {}", self.repository),
        }
    }
}

/// The blamed commit, only when it is one of the recent changes handed to the
/// audit (matched by a sha prefix of at least 7 characters).
pub fn culprit(outcome: Option<&Outcome>, recent: Option<&Value>) -> Option<Culprit> {
    let o = outcome?;
    let sha = o.culprit_commit.trim().to_ascii_lowercase();
    if sha.len() < 7 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    recent?.as_array()?.iter().find_map(|change| {
        let full = change["sha"].as_str()?;
        full.to_ascii_lowercase().starts_with(&sha).then(|| Culprit {
            repository: change["repository"].as_str().unwrap_or_default().to_string(),
            sha: full.to_string(),
            title: change["title"].as_str().unwrap_or_default().to_string(),
            author: change["author"].as_str().unwrap_or_default().to_string(),
            pr: change["pr"].as_i64(),
            reason: o.culprit_reason.clone(),
        })
    })
}

/// The PR a commit title names: `Merge pull request #12 ...` or a squash `... (#12)`.
pub fn pr_number(title: &str) -> Option<i64> {
    static PR: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pr = PR.get_or_init(|| regex::Regex::new(r"(?:pull request #|\(#)(\d{1,7})").expect("valid pattern"));
    pr.captures(title)?.get(1)?.as_str().parse().ok()
}

/// Commits per candidate repository handed to the audit, and how many candidates.
const RECENT_COMMITS: usize = 8;
const RECENT_REPOSITORIES: usize = 3;

/// The commits that landed in the top candidates in the two weeks before the
/// error first appeared, newest first, with the files each touched: the audit
/// says whether one of them introduced the error. Best effort: GitHub errors
/// just leave a repository out.
pub async fn recent_changes(token: &str, candidates: &[String], first_seen: &str) -> Vec<Value> {
    let Ok(first) = chrono::DateTime::parse_from_rfc3339(first_seen) else { return Vec::new() };
    let first = first.with_timezone(&chrono::Utc);
    let format = |t: chrono::DateTime<chrono::Utc>| t.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let (since, until) = (format(first - chrono::Duration::days(14)), format(first + chrono::Duration::hours(1)));
    let mut changes = Vec::new();
    for repo in candidates.iter().take(RECENT_REPOSITORIES) {
        let path = format!("/repos/{repo}/commits?since={since}&until={until}&per_page={RECENT_COMMITS}");
        let Ok(list) = crate::automation::connectors::github_get(token, &path).await else { continue };
        for commit in list.as_array().into_iter().flatten().take(RECENT_COMMITS) {
            let Some(sha) = commit["sha"].as_str().filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())) else { continue };
            let title: String = commit
                .pointer("/commit/message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(160)
                .collect();
            let files: Vec<String> = crate::automation::connectors::github_get(token, &format!("/repos/{repo}/commits/{sha}"))
                .await
                .ok()
                .map(|detail| {
                    detail["files"].as_array().into_iter().flatten().filter_map(|f| f["filename"].as_str().map(str::to_string)).take(25).collect()
                })
                .unwrap_or_default();
            changes.push(json!({
                "repository": repo,
                "sha": sha,
                "date": commit.pointer("/commit/committer/date").and_then(Value::as_str).unwrap_or_default(),
                "author": commit.pointer("/author/login").or_else(|| commit.pointer("/commit/author/name")).and_then(Value::as_str).unwrap_or_default(),
                "pr": pr_number(&title),
                "title": title,
                "files": files,
            }));
        }
    }
    changes
}

/// The agent's final JSON: an object, or (Claude's `--output-format json`) a
/// string holding it, possibly wrapped in prose or a code fence.
fn json_in_text(text: &str) -> Option<Value> {
    let parsed = serde_json::from_str::<Value>(text.trim()).ok().filter(Value::is_object);
    parsed.or_else(|| {
        let (start, end) = (text.find('{')?, text.rfind('}')?);
        serde_json::from_str::<Value>(text.get(start..=end)?).ok().filter(Value::is_object)
    })
}

/// Reads the agent's final JSON. `None` when it is not a usable conclusion.
pub fn parse_outcome(result: &Value) -> Option<Outcome> {
    // The worker hands over {"code","result":<claude envelope>}: read the envelope.
    if let Some(envelope) = result.get("result").filter(|v| v.get("result").is_some()) {
        return parse_outcome(envelope);
    }
    let inner = result.get("result");
    let from_text = inner.and_then(Value::as_str).and_then(json_in_text);
    let structured = inner.filter(|v| v.is_object()).or(from_text.as_ref()).unwrap_or(result);
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
        culprit_commit: text("culprit_commit", 40),
        culprit_reason: text("culprit_reason", 2000),
    })
}

/// An automatic fix only follows an audit sure of the cause: a guess (a
/// request line with no exception, say) becomes a PR that fixes the wrong
/// thing. Anything less waits for a person to press Fix.
pub fn auto_fixes(outcome: Option<&Outcome>) -> bool {
    outcome.is_some_and(|o| o.confidence == "high")
}

/// The repository the issue goes to: the agent's proposal when it is inside the
/// owner and carries evidence, else the source's default repository.
pub fn choose_repository(outcome: Option<&Outcome>, owner: &str, fallback: &str) -> (String, bool) {
    match outcome.and_then(|o| o.repository.as_deref().filter(|_| !o.routing_evidence.is_empty()).map(|r| (r, o))) {
        Some((repo, _)) if in_owner(repo, owner) => (repo.to_string(), true),
        _ => (fallback.to_string(), false),
    }
}

/// The issue's lead, written like a person's report: What / Why / Possible fix.
/// Model text is markdown made safe by `safe_markdown` (it read logs an
/// attacker can write to); links survive only into `owner`'s repositories.
pub fn issue_section(outcome: Option<&Outcome>, culprit: Option<&Culprit>, routed: bool, executor: &str, run_link: &str, owner: &str) -> String {
    let Some(o) = outcome else {
        return format!("## What\nThe automatic audit ({executor}) did not reach a conclusion; see [the run]({run_link}). The log evidence is below.\n");
    };
    let md = |t: &str| super::delivery::safe_markdown(t, owner);
    let what = if o.summary.is_empty() { String::new() } else { format!("## What\n{}\n\n", md(&o.summary)) };
    let doubt = if o.confidence == "low" {
        "> [!NOTE]\n> Low confidence: the audit could not confirm the cause. What it could not check is in **Why**.\n\n"
    } else {
        ""
    };
    let suspect = culprit
        .map(|c| {
            let by = if c.author.is_empty() { String::new() } else { format!(" by `{}`", c.author.replace('`', "")) };
            format!(
                "> [!WARNING]\n> **Suspected change:** [{}]({}){by}: `{}`\n\n{}\n\n",
                c.label(),
                c.url(),
                c.title.replace('`', "'"),
                md(&c.reason),
            )
        })
        .unwrap_or_default();
    let fix = if o.suggested_fix.is_empty() { String::new() } else { format!("## Possible fix\n{}\n\n", md(&o.suggested_fix)) };
    let routing = if routed {
        format!("## Why this repository\n{}\n\n", md(&o.routing_evidence))
    } else {
        "## Why this repository\nThe audit did not name a repository it could support, so this is the source's default repository.\n\n".into()
    };
    format!(
        "{what}{doubt}## Why\n{}\n\n{suspect}{fix}{routing}<sub>Audit by {executor} · confidence: {} · [run]({run_link})</sub>\n",
        md(&o.root_cause),
        o.confidence,
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
    let summary = outcome.as_ref().map(|o| if o.summary.is_empty() { o.root_cause.chars().take(300).collect() } else { o.summary.clone() });
    // Recorded before any GitHub call: a credential error must not leave the group pending.
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        store_monitor::finish_audit(&conn, &group.id, if outcome.is_some() { "done" } else { "failed" }, summary.as_deref())?;
    }
    let connector = source.github_connector_id.clone().unwrap_or_default();
    let token = worker_token(store, &source, &connector).await?;
    // The proposal must be a repository the credential can actually reach.
    if routed && crate::automation::connectors::get_github_repository(&token, &repository).await.is_err() {
        (repository, routed) = (fallback.clone(), false);
    }
    let run_link = format!("{}/factory/runs?agent_run={}", config.app_base_url.trim_end_matches('/'), claim.run.id);
    let executor = source.audit_executor.clone().unwrap_or_else(|| "agent".into());
    let suspect = culprit(outcome.as_ref(), claim.config.pointer("/trigger/recent_changes"));
    let section = issue_section(outcome.as_ref(), suspect.as_ref(), routed, &executor, &run_link, &owner);
    let title = outcome.as_ref().map(|o| o.summary.as_str()).filter(|s| !s.is_empty());
    let number = super::delivery::open_issue_in(store, &source, &group, &token, &repository, &section, title, auto_fixes(outcome.as_ref())).await?;
    let mut group = group;
    group.issue_number = Some(number);
    group.issue_repo = Some(repository);
    let suspect_line = suspect.as_ref().map(|c| format!("\n⚠️ Suspected change: {} {}", c.label(), c.url())).unwrap_or_default();
    let headline = match &summary {
        Some(s) => format!("🔎 Audit: {}{suspect_line}", super::delivery::mcp_safe(s)),
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
            slack_ts: None,
            fix_run_id: None,
            fix_status: None,
            fix_pr_url: None,
            context_log_url: None,
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
            culprit_commit: String::new(),
            culprit_reason: String::new(),
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
    fn the_issue_section_reads_like_a_report_with_safe_model_text() {
        let section = issue_section(Some(&outcome("k/r", "see [iam.ts](https://github.com/k/r/blob/x/iam.ts#L3) @team")), None, true, "codex", "https://x/run", "k");
        assert!(section.starts_with("## What\ns\n\n## Why\nthe cause\n"));
        assert!(section.contains("## Why this repository\nsee [iam.ts](https://github.com/k/r/blob/x/iam.ts#L3) ＠team"));
        assert!(section.contains("confidence: high · [run](https://x/run)"));
        assert!(!section.contains("Suspected change") && !section.contains("Low confidence"));
        let mut low = outcome("k/r", "e");
        low.confidence = "low".into();
        assert!(issue_section(Some(&low), None, false, "pi", "https://x/run", "k").contains("Low confidence"));
        assert!(issue_section(None, None, false, "pi", "https://x/run", "k").contains("did not reach a conclusion"));
    }

    #[test]
    fn only_a_sure_audit_starts_a_fix_on_its_own() {
        let mut o = outcome("k/r", "e");
        assert!(auto_fixes(Some(&o)));
        o.confidence = "medium".into();
        assert!(!auto_fixes(Some(&o)));
        assert!(!auto_fixes(None));
    }

    #[test]
    fn claudes_json_string_result_is_parsed() {
        // What `claude -p --output-format json` returns: the agent's JSON as a string.
        let text = r#"{"confidence":"high","repository":"kasymir/kasymir-iam-service","root_cause":"signIn throws a 500","routing_evidence":"iam.service.ts:1645","summary":"s","culprit_commit":"abc1234","culprit_reason":"it added the throw"}"#;
        let parsed = parse_outcome(&json!({"type": "result", "result": text})).unwrap();
        assert_eq!(parsed.repository.as_deref(), Some("kasymir/kasymir-iam-service"));
        assert_eq!(parsed.culprit_commit, "abc1234");
        let fenced = format!("Here it is:\n```json\n{text}\n```");
        assert_eq!(parse_outcome(&json!({"result": fenced})).unwrap().routing_evidence, "iam.service.ts:1645");
        assert_eq!(parse_outcome(&json!({"result": "no json here"})), None);
        // What the worker actually passes to `finish`: the envelope wrapped in its run result.
        let wrapped = json!({"code": "completed", "result": {"type": "result", "result": text}, "stream": {}});
        assert_eq!(parse_outcome(&wrapped).unwrap().repository.as_deref(), Some("kasymir/kasymir-iam-service"));
    }

    #[test]
    fn only_a_handed_commit_can_be_blamed_and_it_is_marked_in_the_issue() {
        let recent = json!([
            {"repository": "kasymir/iam", "sha": "abc1234def5678901234567890123456789abcde", "title": "Throw on bad login (#42)", "author": "dev1", "pr": 42, "files": ["src/iam.ts"]},
            {"repository": "kasymir/iam", "sha": "1111111222222233333334444444555555566666", "title": "docs", "author": "dev2", "pr": null, "files": []}
        ]);
        let mut o = outcome("kasymir/iam", "e");
        o.culprit_commit = "ABC1234".into();
        o.culprit_reason = "added the throw".into();
        let c = culprit(Some(&o), Some(&recent)).unwrap();
        assert_eq!((c.pr, c.url()), (Some(42), "https://github.com/kasymir/iam/pull/42".to_string()));
        let section = issue_section(Some(&o), Some(&c), true, "claude", "https://x/run", "kasymir");
        assert!(section.contains("**Suspected change:** [PR #42 in kasymir/iam (abc1234)](https://github.com/kasymir/iam/pull/42) by `dev1`"));
        o.culprit_commit = "1111111".into();
        assert_eq!(culprit(Some(&o), Some(&recent)).unwrap().url(), "https://github.com/kasymir/iam/commit/1111111222222233333334444444555555566666");
        for invented in ["deadbee", "abc12", "zzzzzzz", ""] {
            o.culprit_commit = invented.into();
            assert!(culprit(Some(&o), Some(&recent)).is_none(), "{invented}");
        }
    }

    #[test]
    fn pr_numbers_come_from_merge_and_squash_titles() {
        assert_eq!(pr_number("Merge pull request #12 from acme/fix"), Some(12));
        assert_eq!(pr_number("fix(iam): login errors (#345)"), Some(345));
        assert_eq!(pr_number("refs issue #9 in text"), None);
    }

    #[test]
    fn candidates_stay_inside_the_owner() {
        assert!(in_owner("kasymir/sales", "Kasymir"));
        assert!(!in_owner("other/sales", "kasymir"));
        assert!(!in_owner("kasymir", "kasymir"));
    }
}
