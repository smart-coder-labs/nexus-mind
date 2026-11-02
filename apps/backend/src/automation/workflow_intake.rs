//! One intake contract for Linear, Slack and Sentry, targeting workflow IDs
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
}
fn validate(kind: &str, config: &Value) -> Result<()> {
    if kind == "linear" {
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
    let id = uuid::Uuid::new_v4().to_string();
    c.execute("INSERT INTO factory_workflow_intakes(id,org_id,definition_id,kind,connector_id,config_json,created_by) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![id,org,b.definition_id,b.kind,b.connector_id,b.config.to_string(),user])?;
    Ok(json!({"id":id}))
}
pub fn list(c: &rusqlite::Connection, org: &str) -> Result<Vec<Value>> {
    let mut s=c.prepare("SELECT id,definition_id,kind,connector_id,config_json,enabled,next_poll,last_error FROM factory_workflow_intakes WHERE org_id=?1 ORDER BY id")?;
    let rows=s.query_map([org],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,bool>(5)?,r.get::<_,i64>(6)?,r.get::<_,Option<String>>(7)?)))?.map(|r|{let(id,definition,kind,connector,config,enabled,next,error)=r?;Ok(json!({"id":id,"definition_id":definition,"kind":kind,"connector_id":connector,"config":serde_json::from_str::<Value>(&config)?,"enabled":enabled,"next_poll":next,"last_error":error}))}).collect();
    rows
}
static POLLING:std::sync::LazyLock<dashmap::DashSet<String>>=std::sync::LazyLock::new(dashmap::DashSet::new);
pub fn spawn_poll(store:SqliteStore,org:String,runner:Option<String>) {
    let key=format!("{org}:{}",runner.as_deref().unwrap_or("human"));
    if !POLLING.insert(key.clone()){return}
    tokio::spawn(async move {
        struct Guard(String);impl Drop for Guard {fn drop(&mut self){POLLING.remove(&self.0);}}
        let _guard=Guard(key);
        let _=poll(&store,&org,runner.as_deref()).await;
    });
}
pub async fn poll(store: &SqliteStore, org: &str, runner: Option<&str>) -> Result<Value> {
    let sources = {
        let handle = store.conn();
        let c = handle
            .lock()
            .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
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
        if count>0 {
            let run={let h=store.conn();let c=h.lock().map_err(|_|anyhow::anyhow!("storage_unavailable"))?;
                c.query_row("SELECT e.run_id FROM factory_workflow_events e JOIN factory_workflow_runs r ON r.id=e.run_id WHERE r.org_id=?1 AND e.kind='intake.received' AND json_extract(e.payload_json,'$.source_id')=?2 ORDER BY e.seq DESC LIMIT 1",params![org,id],|r|r.get::<_,String>(0)).ok()};
            if let Some(run)=run {let _=crate::factory::workflow_policy::assess_intake(store,org,&run).await;}
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
                "linear_issue_limit_exceeded",
                "linear_upstream_rejected",
                "linear_graphql_failed",
                "intake_timeout",
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
    let (token, workspace) = {
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
    if connector(&c, org, connector_id, kind, config)? != token {
        bail!("connector_changed")
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
