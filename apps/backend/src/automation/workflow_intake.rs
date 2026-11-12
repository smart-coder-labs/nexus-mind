//! One intake contract for GitHub Issues, Linear, Slack and Sentry, targeting workflow IDs
//! rather than creating intermediary GitHub issues. Poll leases are persisted.
use crate::{
    db::workflows,
    factory::{
        contracts::PrivacyClass,
        intake::{self, IntakeTarget},
    },
    store::sqlite::SqliteStore,
};
use anyhow::{bail, Result};
use rusqlite::params;
use serde::Deserialize;
use futures_util::StreamExt;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub definition_id: String,
    pub kind: String,
    pub connector_id: String,
    pub config: Value,
    #[serde(default)]
    pub enabled: bool,
}
/// Which repositories one GitHub intake reads. `Selected` is an explicit list (the
/// legacy single `repository` maps to it); `Workspace` and `Organization` are
/// resolved against the confirmed workspace on every poll, so repositories added
/// later are picked up without editing the intake.
#[derive(Deserialize, Default, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
enum GithubScope {
    #[default]
    Selected,
    Workspace,
    Organization,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GithubConfig {
    #[serde(default)]
    scope: GithubScope,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    repositories: Vec<String>,
    #[serde(default)]
    organization: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    include_assigned: bool,
}
const MAX_GITHUB_INTAKE_REPOSITORIES: usize = 100;
fn github_config(config: &Value) -> Result<GithubConfig> {
    let mut value: GithubConfig = serde_json::from_value(config.clone())
        .map_err(|_| anyhow::anyhow!("invalid_github_intake_config"))?;
    if let Some(repository) = value.repository.take() {
        if !value.repositories.is_empty() || value.scope != GithubScope::Selected {
            bail!("invalid_github_intake_config")
        }
        value.repositories.push(repository);
    }
    match value.scope {
        GithubScope::Selected => {
            if value.repositories.is_empty()
                || value.repositories.len() > MAX_GITHUB_INTAKE_REPOSITORIES
                || value.organization.is_some()
            {
                bail!("invalid_github_intake_config")
            }
        }
        GithubScope::Workspace => {
            if !value.repositories.is_empty() || value.organization.is_some() {
                bail!("invalid_github_intake_config")
            }
        }
        GithubScope::Organization => {
            let owner = value.organization.as_deref().unwrap_or_default();
            if !value.repositories.is_empty()
                || owner.is_empty()
                || owner.len() > 39
                || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                bail!("invalid_github_intake_config")
            }
            value.organization = Some(owner.to_ascii_lowercase());
        }
    }
    for repository in &mut value.repositories {
        crate::automation::connectors::validate_repository(repository)?;
        *repository = repository.to_ascii_lowercase();
    }
    value.repositories.sort();
    value.repositories.dedup();
    if value.labels.len() > 20
        || value
            .labels
            .iter()
            .any(|l| l.trim().is_empty() || l.len() > 100 || l.chars().any(char::is_control))
    {
        bail!("invalid_github_intake_labels")
    }
    Ok(value)
}
/// The repositories (lowercase `owner/name`) and confirmed branches this intake reads
/// right now. Every target must be a confirmed workspace repository.
fn github_targets(config: &Value, workspace: &Value) -> Result<Vec<(String, String)>> {
    let settings = github_config(config)?;
    let confirmed: Vec<(String, Option<String>)> = workspace["repositories"]
        .as_array()
        .map(|repos| {
            repos
                .iter()
                .filter_map(|r| {
                    let name = crate::factory::workflow_publication::github_repository(
                        r["remote"].as_str()?,
                    )
                    .ok()?
                    .to_ascii_lowercase();
                    Some((name, r["branch"].as_str().map(str::to_string)))
                })
                .collect()
        })
        .unwrap_or_default();
    let selected: Vec<&(String, Option<String>)> = match settings.scope {
        GithubScope::Selected => settings
            .repositories
            .iter()
            .map(|wanted| {
                confirmed
                    .iter()
                    .find(|(name, _)| name == wanted)
                    .ok_or_else(|| anyhow::anyhow!("github_intake_repository_not_in_workspace"))
            })
            .collect::<Result<_>>()?,
        GithubScope::Workspace => confirmed.iter().collect(),
        GithubScope::Organization => {
            let owner = settings.organization.unwrap_or_default();
            confirmed
                .iter()
                .filter(|(name, _)| name.split('/').next() == Some(owner.as_str()))
                .collect()
        }
    };
    if selected.is_empty() {
        bail!("github_intake_repository_not_in_workspace")
    }
    if selected.len() > MAX_GITHUB_INTAKE_REPOSITORIES {
        bail!("github_issue_limit_exceeded")
    }
    let mut targets: Vec<(String, String)> = selected
        .into_iter()
        .map(|(name, branch)| {
            branch
                .clone()
                .map(|branch| (name.clone(), branch))
                .ok_or_else(|| anyhow::anyhow!("source_branch_not_confirmed"))
        })
        .collect::<Result<_>>()?;
    targets.sort();
    targets.dedup();
    Ok(targets)
}
fn github_tasks(
    issues: &[Value],
    config: &Value,
    repository: &str,
    base_ref: &str,
) -> Result<Vec<(String, Value)>> {
    let settings = github_config(config)?;
    let target = IntakeTarget {
        repository: repository.to_ascii_lowercase(),
        base_ref: base_ref.into(),
        privacy_class: PrivacyClass::Internal,
    };
    let mut tasks = Vec::new();
    for issue in issues {
        if !settings.include_assigned
            && (issue["assignees"].as_array().is_some_and(|a| !a.is_empty())
                || issue["assignee"].is_object())
        {
            continue;
        }
        if issue["number"].as_u64().filter(|n| *n > 0).is_none()
            || issue["state"] != "open"
            || !settings.labels.iter().all(|label| {
                issue["labels"].as_array().is_some_and(|labels| {
                    labels.iter().any(|l| {
                        l["name"]
                            .as_str()
                            .is_some_and(|s| s.eq_ignore_ascii_case(label))
                    })
                })
            })
        {
            continue;
        }
        if let Ok(Some(spec)) =
            intake::github_issue_to_task_spec(issue, &target, chrono::Utc::now())
        {
            let key = format!("github:{}", spec.source.reference);
            let mut task = serde_json::to_value(spec)?;
            task["origin_trust"] = json!("untrusted");
            tasks.push((key, task));
        }
    }
    Ok(tasks)
}
fn validate(kind: &str, config: &Value) -> Result<()> {
    if kind == "github" {
        github_config(config)?;
    } else if kind == "linear" {
        for key in ["team_id", "project_id"] {
            if config.get(key).is_some_and(|v| {
                !v.is_null() && (!v.is_string() || v.as_str().unwrap_or_default().len() > 100)
            }) {
                bail!("invalid_linear_filter")
            }
        }
    } else if ["slack", "sentry"].contains(&kind) {
        intake::parse_source_config(kind, config).map_err(anyhow::Error::msg)?;
    } else {
        bail!("invalid_intake_kind")
    }
    Ok(())
}
fn connector(
    c: &rusqlite::Connection,
    org: &str,
    id: &str,
    kind: &str,
    config: &Value,
) -> Result<String> {
    if kind == "github" && id == "server_gh_cli" {
        if !crate::factory::workflow_publication::backend_account_enabled(org) {
            bail!("backend_github_not_authorized")
        }
        github_config(config)?;
        return Ok(id.to_string());
    }
    let (connector, secret) =
        crate::db::queries::get_autonomous_agent_connector_secret(c, org, id)?
            .ok_or_else(|| anyhow::anyhow!("connector_unavailable"))?;
    let purpose = &connector.metadata["purpose"];
    if connector.kind != "target_secret"
        || !(purpose == "factory_workflow_intake" && connector.metadata["source_kind"] == kind
            || kind == "linear" && purpose == "factory_workflow_linear")
        || !connector
            .scopes
            .iter()
            .any(|s| s == &format!("{kind}:read"))
    {
        bail!("connector_intake_scope")
    }
    if kind == "github" {
        // A dedicated connector is repository-scoped: it may only serve an explicit
        // list inside its allowlist. Workspace/organization scopes need the backend
        // account, whose reach is bounded by the confirmed workspace instead.
        let settings = github_config(config)?;
        let allowed = |repository: &String| {
            connector.metadata["repositories"]
                .as_array()
                .is_some_and(|repos| {
                    repos.iter().any(|r| {
                        r.as_str()
                            .is_some_and(|s| s.eq_ignore_ascii_case(repository))
                    })
                })
        };
        if settings.scope != GithubScope::Selected || !settings.repositories.iter().all(allowed) {
            bail!("connector_intake_scope")
        }
    }
    if kind == "sentry" {
        let url = config["base_url"].as_str().unwrap_or_default();
        let host = intake::sentry_base_host(url)
            .ok_or_else(|| anyhow::anyhow!("invalid_sentry_base_url"))?;
        if connector.metadata["base_host"] != host {
            bail!("sentry_connector_host_mismatch")
        }
    }
    Ok(secret)
}
pub fn create(c: &rusqlite::Connection, org: &str, user: &str, b: &Source) -> Result<Value> {
    validate(&b.kind, &b.config)?;
    connector(c, org, &b.connector_id, &b.kind, &b.config)?;
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_workflow_definitions WHERE id=?1 AND org_id=?2)",
        params![b.definition_id, org],
        |r| r.get(0),
    )?;
    if !exists {
        bail!("not_found")
    }
    if b.kind == "github" {
        let workspace: String = c.query_row(
            "SELECT workspace_id FROM factory_workflow_definitions WHERE id=?1 AND org_id=?2",
            params![b.definition_id, org],
            |r| r.get(0),
        )?;
        github_targets(&b.config, &workflows::workspace_by_id(c, org, &workspace)?)?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    c.execute("INSERT INTO factory_workflow_intakes(id,org_id,definition_id,kind,connector_id,config_json,created_by,enabled) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id,org,b.definition_id,b.kind,b.connector_id,b.config.to_string(),user,b.enabled])?;
    Ok(json!({"id":id,"enabled":b.enabled}))
}
pub fn set_enabled(c: &rusqlite::Connection, org: &str, id: &str, enabled: bool) -> Result<()> {
    if c.execute("UPDATE factory_workflow_intakes SET enabled=?3,next_poll=0 WHERE id=?1 AND org_id=?2",params![id,org,enabled])? != 1 {
        bail!("not_found")
    }
    Ok(())
}

pub fn list(c: &rusqlite::Connection, org: &str) -> Result<Vec<Value>> {
    let mut s=c.prepare("SELECT id,definition_id,kind,connector_id,config_json,enabled,next_poll,last_error FROM factory_workflow_intakes WHERE org_id=?1 ORDER BY id")?;
    let rows=s.query_map([org],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,bool>(5)?,r.get::<_,i64>(6)?,r.get::<_,Option<String>>(7)?)))?.map(|r|{let(id,definition,kind,connector,config,enabled,next,error)=r?;Ok(json!({"id":id,"definition_id":definition,"kind":kind,"connector_id":connector,"config":serde_json::from_str::<Value>(&config)?,"enabled":enabled,"next_poll":next,"last_error":error}))}).collect();
    rows
}
static POLLING: std::sync::LazyLock<dashmap::DashSet<String>> =
    std::sync::LazyLock::new(dashmap::DashSet::new);
pub fn spawn_poll(store: SqliteStore, org: String, runner: Option<String>) {
    let key = format!("{org}:{}", runner.as_deref().unwrap_or("human"));
    if !POLLING.insert(key.clone()) {
        return;
    }
    tokio::spawn(async move {
        struct Guard(String);
        impl Drop for Guard {
            fn drop(&mut self) {
                POLLING.remove(&self.0);
            }
        }
        let _guard = Guard(key);
        let _ = poll(&store, &org, runner.as_deref()).await;
    });
}
pub async fn poll(store: &SqliteStore, org: &str, runner: Option<&str>) -> Result<Value> {
    let sources = {
        let handle = store.conn();
        let c = handle
            .lock()
            .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
        if !workflows::factory_enabled(&c, org)? {
            return Ok(json!({"queued":0,"paused":true}));
        }
        let mut s=c.prepare("SELECT i.id,i.definition_id,i.kind,i.connector_id,i.config_json FROM factory_workflow_intakes i JOIN factory_workflow_definitions d ON d.id=i.definition_id JOIN factory_workspaces w ON w.id=d.workspace_id WHERE i.org_id=?1 AND i.enabled=1 AND i.next_poll<=?2 AND (?3 IS NULL OR w.runner_id=?3) ORDER BY i.next_poll,i.id LIMIT 10")?;
        let rows = s
            .query_map(params![org, chrono::Utc::now().timestamp(), runner], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let mut started = 0;
    for (id, definition, kind, connector_id, text) in sources {
        let leased = {
            let h = store.conn();
            let c = h
                .lock()
                .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
            c.execute("UPDATE factory_workflow_intakes SET next_poll=?1 WHERE id=?2 AND org_id=?3 AND enabled=1 AND next_poll<=?4",params![chrono::Utc::now().timestamp()+300,id,org,chrono::Utc::now().timestamp()])?==1
        };
        if !leased {
            continue;
        }
        let config: Value = serde_json::from_str(&text)?;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            poll_one(store, org, &id, &definition, &kind, &connector_id, &config),
        )
        .await;
        let (count, error) = match result {
            Ok(Ok(count)) => (count, None),
            Ok(Err(e)) => (0, Some(e.to_string())),
            Err(_) => (0, Some("intake_timeout".into())),
        };
        started += count;
        if count > 0 {
            let run = {
                let h = store.conn();
                let c = h
                    .lock()
                    .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
                c.query_row("SELECT e.run_id FROM factory_workflow_events e JOIN factory_workflow_runs r ON r.id=e.run_id WHERE r.org_id=?1 AND e.kind='intake.received' AND json_extract(e.payload_json,'$.source_id')=?2 ORDER BY e.seq DESC LIMIT 1",params![org,id],|r|r.get::<_,String>(0)).ok()
            };
            if let Some(run) = run {
                let _ = crate::factory::workflow_policy::assess_intake(store, org, &run).await;
            }
        }
        let h = store.conn();
        let c = h
            .lock()
            .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
        // Store only controlled error codes, not upstream responses or tokens.
        let error = error.map(|s| {
            if [
                "connector_unavailable",
                "connector_changed",
                "connector_intake_scope",
                "sentry_connector_host_mismatch",
                "workspace_not_confirmed",
                "runtime_configuration_incomplete",
                "workflow_agent_unavailable",
                "workflow_agent_model_missing",
                "workflow_agent_template_incompatible",
                "workflow_agent_instructions_unsafe",
                "linear_issue_limit_exceeded",
                "linear_upstream_rejected",
                "linear_graphql_failed",
                "intake_timeout",
                "backend_github_not_authorized",
                "source_branch_not_confirmed",
                "github_issue_limit_exceeded",
                "github_intake_repository_not_in_workspace",
            ]
            .contains(&s.as_str())
            {
                s
            } else {
                "intake_upstream_failed".into()
            }
        });
        c.execute(
            "UPDATE factory_workflow_intakes SET last_error=?1 WHERE id=?2 AND org_id=?3",
            params![error, id, org],
        )?;
    }
    Ok(json!({"queued":started}))
}
async fn poll_one(
    store: &SqliteStore,
    org: &str,
    source: &str,
    definition: &str,
    kind: &str,
    connector_id: &str,
    config: &Value,
) -> Result<u32> {
    let (credential, workspace) = {
        let h = store.conn();
        let c = h
            .lock()
            .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
        let id: String = c.query_row(
            "SELECT workspace_id FROM factory_workflow_definitions WHERE id=?1 AND org_id=?2",
            params![definition, org],
            |r| r.get(0),
        )?;
        let active:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_runs WHERE org_id=?1 AND workspace_id=?2 AND status NOT IN ('succeeded','cancelled'))",params![org,id],|r|r.get(0))?;
        if active {
            return Ok(0);
        }
        (
            connector(&c, org, connector_id, kind, config)?,
            workflows::workspace_by_id(&c, org, &id)?,
        )
    };
    let token = if credential == "server_gh_cli" {
        crate::automation::worker::server_github_token().await?
    } else {
        credential.clone()
    };
    let mut tasks = Vec::new();
    if kind == "linear" {
        for issue in crate::factory::linear::assigned_issues(
            &token,
            config["team_id"].as_str(),
            config["project_id"].as_str(),
        )
        .await?
        {
            tasks.push((issue.source_key(), issue.task()));
        }
    } else if kind == "github" {
        // Read the repositories concurrently (bounded) so a workspace- or
        // organization-wide intake fits the poll timeout.
        let targets = github_targets(config, &workspace)?;
        let listed: Vec<Result<(String, String, Vec<Value>)>> =
            futures_util::stream::iter(targets.into_iter().map(|(repository, base_ref)| {
                let token = token.clone();
                async move {
                    let issues = crate::automation::connectors::list_labeled_github_issues_bounded(
                        &token,
                        &repository,
                        intake::INTAKE_LABEL,
                    )
                    .await?;
                    Ok((repository, base_ref, issues))
                }
            }))
            .buffered(8)
            .collect()
            .await;
        for result in listed {
            let (repository, base_ref, issues) = result?;
            tasks.extend(github_tasks(&issues, config, &repository, &base_ref)?);
        }
    } else {
        let remote = workspace["repositories"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|r| r["remote"].as_str())
            .ok_or_else(|| anyhow::anyhow!("workspace_not_confirmed"))?;
        let repository = crate::factory::workflow_publication::github_repository(remote)?;
        let fetcher = intake::intake_for(
            intake::parse_source_config(kind, config).map_err(anyhow::Error::msg)?,
            token.clone(),
            IntakeTarget {
                repository,
                base_ref: "main".into(),
                privacy_class: PrivacyClass::Internal,
            },
        );
        for spec in fetcher.fetch().await? {
            tasks.push((
                format!("{kind}:{}", spec.source.reference),
                serde_json::to_value(spec)?,
            ));
        }
    }
    let h = store.conn();
    let c = h
        .lock()
        .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
    if connector(&c, org, connector_id, kind, config)? != credential {
        bail!("connector_changed")
    }
    if kind == "github" {
        github_targets(
            config,
            &workflows::workspace_by_id(
                &c,
                org,
                workspace["id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("not_found"))?,
            )?,
        )?;
    }
    let busy:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_runs WHERE org_id=?1 AND workspace_id=?2 AND status NOT IN ('succeeded','cancelled'))",params![org,workspace["id"].as_str()],|r|r.get(0))?;
    if busy {
        return Ok(0);
    }
    for (key, mut task) in tasks {
        let known:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_runs WHERE org_id=?1 AND workspace_id=?2 AND source_key=?3)",params![org,workspace["id"].as_str(),key],|r|r.get(0))?;
        if known {
            continue;
        }
        task["origin_trust"] = json!("untrusted");
        let run = workflows::start_run(&c, org, definition, &key, &task)?;
        let id = run["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid_run"))?;
        let allowed = crate::factory::workflow_policy::decision(&c, org, id, "fix", &[], false)?
            ["allowed"]
            == true;
        if !allowed {
            c.execute(
                "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
                [id],
            )?;
        }
        workflows::event(
            &c,
            id,
            None,
            "intake.received",
            json!({"source_id":source,"kind":kind,"needs_approval":!allowed}),
        )?;
        return Ok(1);
    }
    Ok(0)
}

#[cfg(test)]
mod github_tests {
    use super::*;
    fn config() -> Value {
        json!({"repository":"Acme/Web","labels":["bug"]})
    }
    fn issue() -> Value {
        json!({"number":42,"state":"open","title":"Fix button","body":"Reproduce this bug","author_association":"OWNER","html_url":"https://github.com/acme/web/issues/42","labels":[{"name":"factory"},{"name":"BUG"}]})
    }
    #[test]
    fn github_sources_default_to_paused_and_can_be_toggled_per_tenant() {
        let source: Source=serde_json::from_value(json!({"definition_id":"d","kind":"github","connector_id":"server_gh_cli","config":{"repository":"kasymir/repo","labels":["factory"]}})).unwrap();
        assert!(!source.enabled);
        let c=rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE factory_workflow_intakes(id TEXT,org_id TEXT,enabled INTEGER,next_poll INTEGER);INSERT INTO factory_workflow_intakes VALUES('source','org',0,123)").unwrap();
        set_enabled(&c,"org","source",true).unwrap();
        assert_eq!(c.query_row("SELECT enabled FROM factory_workflow_intakes",[],|r|r.get::<_,bool>(0)).unwrap(),true);
        assert_eq!(c.query_row("SELECT next_poll FROM factory_workflow_intakes",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert!(set_enabled(&c,"other","source",true).is_err());
    }

    #[test]
    fn assigned_issues_require_an_explicit_opt_in_to_avoid_competing_with_resolvers() {
        let mut assigned = issue();
        assigned["assignees"] = json!([{"login":"another-person"}]);
        assert!(github_tasks(&[assigned.clone()], &config(), "acme/web", "main")
            .unwrap()
            .is_empty());
        assert_eq!(
            github_tasks(
                &[assigned],
                &json!({"repository":"acme/web","labels":["bug"],"include_assigned":true}),
                "acme/web",
                "main"
            )
            .unwrap()
            .len(),
            1
        );
    }
    #[test]
    fn github_config_is_bounded_and_rejects_credentials_and_external_repositories() {
        assert!(validate("github", &config()).is_ok());
        for bad in [
            json!({"repository":"https://evil.test/web"}),
            json!({"repository":"../web"}),
            json!({"repository":"acme/web","token":"fixture"}),
            json!({"repository":"acme/web","labels":[""]}),
        ] {
            assert!(validate("github", &bad).is_err());
        }
        let workspace =
            json!({"repositories":[{"remote":"git@github.com:ACME/Web.git","branch":"master"}]});
        assert_eq!(
            github_targets(&config(), &workspace).unwrap(),
            [("acme/web".to_string(), "master".to_string())]
        );
        assert!(github_targets(
            &config(),
            &json!({"repositories":[{"remote":"https://github.com/other/web.git","branch":"main"}]})
        )
        .is_err());
        assert_eq!(
            github_targets(
                &config(),
                &json!({"repositories":[{"remote":"https://github.com/acme/web.git"}]})
            )
            .unwrap_err()
            .to_string(),
            "source_branch_not_confirmed"
        );
    }
    fn workspace() -> Value {
        json!({"repositories":[
            {"remote":"https://github.com/acme/web.git","branch":"main"},
            {"remote":"https://github.com/acme/api.git","branch":"develop"},
            {"remote":"https://github.com/other/tools.git","branch":"master"},
            {"remote":"/local/only","branch":"main"}
        ]})
    }
    #[test]
    fn one_intake_can_read_selected_repositories_the_workspace_or_an_organization() {
        let names = |config: Value| {
            github_targets(&config, &workspace())
                .unwrap()
                .into_iter()
                .map(|(name, branch)| format!("{name}@{branch}"))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(json!({"repositories":["Acme/API","acme/web","acme/web"]})),
            ["acme/api@develop", "acme/web@main"]
        );
        assert_eq!(
            names(json!({"scope":"workspace"})),
            ["acme/api@develop", "acme/web@main", "other/tools@master"]
        );
        assert_eq!(
            names(json!({"scope":"organization","organization":"ACME"})),
            ["acme/api@develop", "acme/web@main"]
        );
        // A selected repository outside the workspace, or an organization with no
        // confirmed repositories, fails instead of silently reading nothing.
        for config in [
            json!({"repositories":["acme/web","acme/missing"]}),
            json!({"scope":"organization","organization":"nobody"}),
        ] {
            assert_eq!(
                github_targets(&config, &workspace()).unwrap_err().to_string(),
                "github_intake_repository_not_in_workspace"
            );
        }
        for bad in [
            json!({"repositories":[]}),
            json!({"repository":"acme/web","repositories":["acme/api"]}),
            json!({"scope":"workspace","repositories":["acme/web"]}),
            json!({"scope":"organization"}),
            json!({"scope":"organization","organization":"acme/web"}),
            json!({"scope":"organization","organization":"acme","repositories":["acme/web"]}),
            json!({"scope":"everything"}),
        ] {
            assert!(validate("github", &bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn tasks_target_the_repository_the_issue_came_from() {
        let mut api_issue = issue();
        api_issue["html_url"] = json!("https://github.com/acme/api/issues/42");
        let config = json!({"scope":"organization","organization":"acme"});
        let web = github_tasks(&[issue()], &config, "acme/web", "main").unwrap();
        let api = github_tasks(&[api_issue], &config, "acme/api", "develop").unwrap();
        assert_eq!(web[0].0, "github:acme/web#42");
        assert_eq!(api[0].0, "github:acme/api#42");
        assert_eq!(api[0].1["repository"]["remote"], "acme/api");
        assert_eq!(api[0].1["repository"]["base_ref"], "develop");
    }
    #[test]
    fn github_issues_require_factory_and_all_filters_and_never_include_prs_or_closed_work() {
        let original = issue();
        let tasks = github_tasks(&[original.clone()], &config(), "acme/web", "main").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].0, "github:acme/web#42");
        assert_eq!(tasks[0].1["origin_trust"], "untrusted");
        assert_eq!(tasks[0].1["repository"]["base_ref"], "main");
        assert_eq!(
            tasks[0].0,
            github_tasks(
                &[original.clone()],
                &json!({"repository":"acme/WEB","labels":[]}),
                "acme/web",
                "main"
            )
            .unwrap()[0]
                .0
        );
        for field in ["pull_request", "state", "labels", "number", "title"] {
            let mut candidate = original.clone();
            candidate[field] = match field {
                "pull_request" => json!({"url":"https://api.github.com/pr"}),
                "state" => json!("closed"),
                "labels" => json!([{"name":"bug"}]),
                "number" => json!(-1),
                _ => json!(""),
            };
            assert!(
                github_tasks(&[candidate], &config(), "acme/web", "main").unwrap().is_empty(),
                "{field}"
            );
        }
        assert!(github_tasks(
            &[original],
            &json!({"repository":"acme/web","labels":["security"]}),
            "acme/web",
            "main"
        )
        .unwrap()
        .is_empty());
    }
}
