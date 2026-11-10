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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GithubConfig {
    repository: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    include_assigned: bool,
}
fn github_config(config: &Value) -> Result<GithubConfig> {
    let mut value: GithubConfig = serde_json::from_value(config.clone())
        .map_err(|_| anyhow::anyhow!("invalid_github_intake_config"))?;
    crate::automation::connectors::validate_repository(&value.repository)?;
    value.repository = value.repository.to_ascii_lowercase();
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
fn github_target(config: &Value, workspace: &Value) -> Result<String> {
    let repository = github_config(config)?.repository;
    let permitted = workspace["repositories"].as_array().is_some_and(|repos| {
        repos.iter().any(|r| {
            r["remote"]
                .as_str()
                .and_then(|s| crate::factory::workflow_publication::github_repository(s).ok())
                .is_some_and(|s| s.eq_ignore_ascii_case(&repository))
        })
    });
    if !permitted {
        bail!("github_intake_repository_not_in_workspace")
    }
    Ok(repository)
}
fn github_tasks(issues: &[Value], config: &Value, base_ref: &str) -> Result<Vec<(String, Value)>> {
    let settings = github_config(config)?;
    let target = IntakeTarget {
        repository: settings.repository,
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
        let repository = github_config(config)?.repository;
        if !connector.metadata["repositories"]
            .as_array()
            .is_some_and(|repos| {
                repos.iter().any(|r| {
                    r.as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case(&repository))
                })
            })
        {
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
        github_target(&b.config, &workflows::workspace_by_id(c, org, &workspace)?)?;
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
        let repository = github_target(config, &workspace)?;
        let base_ref=workspace["repositories"].as_array().and_then(|items|items.iter().find(|r|r["remote"].as_str().and_then(|s|crate::factory::workflow_publication::github_repository(s).ok()).is_some_and(|s|s.eq_ignore_ascii_case(&repository)))).and_then(|r|r["branch"].as_str()).ok_or_else(||anyhow::anyhow!("source_branch_not_confirmed"))?;
        let issues = crate::automation::connectors::list_labeled_github_issues_bounded(
            &token,
            &repository,
            intake::INTAKE_LABEL,
        )
        .await?;
        tasks = github_tasks(&issues, config, base_ref)?;
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
        github_target(
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
        assert!(github_tasks(&[assigned.clone()], &config(), "main")
            .unwrap()
            .is_empty());
        assert_eq!(
            github_tasks(
                &[assigned],
                &json!({"repository":"acme/web","labels":["bug"],"include_assigned":true}),
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
        let workspace = json!({"repositories":[{"remote":"git@github.com:ACME/Web.git"}]});
        assert_eq!(github_target(&config(), &workspace).unwrap(), "acme/web");
        assert!(github_target(
            &config(),
            &json!({"repositories":[{"remote":"https://github.com/other/web.git"}]})
        )
        .is_err());
    }
    #[test]
    fn github_issues_require_factory_and_all_filters_and_never_include_prs_or_closed_work() {
        let original = issue();
        let tasks = github_tasks(&[original.clone()], &config(), "main").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].0, "github:acme/web#42");
        assert_eq!(tasks[0].1["origin_trust"], "untrusted");
        assert_eq!(tasks[0].1["repository"]["base_ref"], "main");
        assert_eq!(
            tasks[0].0,
            github_tasks(
                &[original.clone()],
                &json!({"repository":"acme/WEB","labels":[]}),
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
                github_tasks(&[candidate], &config(), "main").unwrap().is_empty(),
                "{field}"
            );
        }
        assert!(github_tasks(
            &[original],
            &json!({"repository":"acme/web","labels":["security"]}),
            "main"
        )
        .unwrap()
        .is_empty());
    }
}
