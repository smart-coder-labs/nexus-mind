//! Organization-scoped durable workflow state. SQLite transactions serialize claims.
use crate::factory::workflows::{Definition, Evidence, EvidenceStatus, Repository, StepKind};
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 93 {
        return Ok(());
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let version: i32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 93 {
        tx.commit()?;
        return Ok(());
    }
    tx.execute_batch("
      CREATE TABLE factory_runners (
        id TEXT PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
        name TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, revoked INTEGER NOT NULL DEFAULT 0,
        capabilities_json TEXT NOT NULL DEFAULT '[]', last_seen TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')));
      CREATE TABLE factory_workspaces (
        id TEXT PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
        runner_id TEXT NOT NULL REFERENCES factory_runners(id), name TEXT NOT NULL, root_path TEXT NOT NULL,
        discovered_json TEXT NOT NULL DEFAULT '[]', repositories_json TEXT NOT NULL DEFAULT '[]',
        inventory_revision INTEGER NOT NULL DEFAULT 0, confirmed_revision INTEGER,
        created_at TEXT NOT NULL DEFAULT (datetime('now')));
      CREATE TABLE factory_workflow_definitions (
        id TEXT PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
        workspace_id TEXT NOT NULL REFERENCES factory_workspaces(id), definition_json TEXT NOT NULL,
        created_by TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now')));
      CREATE TABLE factory_workflow_runs (
        id TEXT PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
        definition_id TEXT NOT NULL REFERENCES factory_workflow_definitions(id),
        workspace_id TEXT NOT NULL REFERENCES factory_workspaces(id), runner_id TEXT NOT NULL REFERENCES factory_runners(id),
        source_key TEXT NOT NULL, task_json TEXT NOT NULL, snapshot_json TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','blocked','succeeded','cancelled')),
        created_at TEXT NOT NULL DEFAULT (datetime('now')), UNIQUE(org_id, workspace_id, source_key));
      CREATE TABLE factory_workflow_steps (
        run_id TEXT NOT NULL REFERENCES factory_workflow_runs(id) ON DELETE CASCADE,
        step_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending'
          CHECK(status IN ('pending','running','blocked','succeeded','cancelled')),
        attempt INTEGER NOT NULL DEFAULT 0, lease_token TEXT, lease_until INTEGER,
        result_json TEXT, PRIMARY KEY(run_id, step_id));
      CREATE TABLE factory_workflow_events (
        seq INTEGER PRIMARY KEY AUTOINCREMENT, run_id TEXT NOT NULL REFERENCES factory_workflow_runs(id) ON DELETE CASCADE,
        step_id TEXT, kind TEXT NOT NULL, payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')));
      CREATE INDEX factory_workflow_queue ON factory_workflow_runs(runner_id, status);
      CREATE INDEX factory_workflow_event_run ON factory_workflow_events(run_id, seq);
      CREATE INDEX factory_workflow_leases ON factory_workflow_steps(status,lease_until) WHERE status='running';
      PRAGMA user_version = 93;")?;
    tx.commit()?;
    Ok(())
}
fn id() -> String {
    Uuid::new_v4().to_string()
}
pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
fn parse(s: String) -> Result<Value> {
    Ok(serde_json::from_str(&s)?)
}
pub(crate) fn event(
    conn: &Connection,
    run: &str,
    step: Option<&str>,
    kind: &str,
    payload: Value,
) -> Result<()> {
    conn.execute("INSERT INTO factory_workflow_events(run_id,step_id,kind,payload_json) VALUES (?1,?2,?3,?4)", params![run,step,kind,payload.to_string()])?;
    Ok(())
}
pub fn register_runner(conn: &Connection, org: &str, name: &str) -> Result<Value> {
    if name.trim().is_empty() || name.len() > 120 {
        bail!("invalid_runner_name");
    }
    let runner = id();
    let token = format!("nmr_{}{}", id(), id());
    conn.execute(
        "INSERT INTO factory_runners(id,org_id,name,token_hash) VALUES (?1,?2,?3,?4)",
        params![runner, org, name, token_hash(&token)],
    )?;
    Ok(json!({"id":runner,"name":name,"token":token}))
}
pub fn authenticate_runner(conn: &Connection, token: &str) -> Result<(String, String)> {
    if !token.starts_with("nmr_") || token.len() > 200 {
        bail!("runner_unauthorized");
    }
    conn.query_row(
        "SELECT id,org_id FROM factory_runners WHERE token_hash=?1 AND revoked=0",
        [token_hash(token)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("runner_unauthorized"))
}
pub fn list_runners(conn: &Connection, org: &str) -> Result<Vec<Value>> {
    let mut s = conn.prepare("SELECT id,name,revoked,capabilities_json,last_seen FROM factory_runners WHERE org_id=?1 ORDER BY created_at DESC")?;
    let rows = s.query_map([org], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    rows.map(|r| { let (id,name,revoked,caps,last_seen)=r?; Ok(json!({"id":id,"name":name,"revoked":revoked,"capabilities":parse(caps)?,"last_seen":last_seen})) }).collect()
}
pub fn revoke_runner(conn: &Connection, org: &str, runner: &str) -> Result<()> {
    if conn.execute(
        "UPDATE factory_runners SET revoked=1 WHERE id=?1 AND org_id=?2",
        params![runner, org],
    )? != 1
    {
        bail!("not_found");
    }
    Ok(())
}
pub fn create_workspace(
    conn: &Connection,
    org: &str,
    runner: &str,
    name: &str,
    root: &str,
) -> Result<Value> {
    if name.trim().is_empty()
        || name.len() > 120
        || root.len() > 2048
        || !root.starts_with('/')
        || root.chars().any(char::is_control)
    {
        bail!("invalid_workspace");
    }
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_runners WHERE id=?1 AND org_id=?2 AND revoked=0)",
        params![runner, org],
        |r| r.get(0),
    )?;
    if !exists {
        bail!("not_found");
    }
    let workspace = id();
    conn.execute("INSERT INTO factory_workspaces(id,org_id,runner_id,name,root_path) VALUES (?1,?2,?3,?4,?5)",params![workspace,org,runner,name,root])?;
    workspace_by_id(conn, org, &workspace)
}
pub fn list_workspaces(conn: &Connection, org: &str, runner: Option<&str>) -> Result<Vec<Value>> {
    let mut s=conn.prepare("SELECT id FROM factory_workspaces WHERE org_id=?1 AND (?2 IS NULL OR runner_id=?2) ORDER BY created_at DESC")?;
    let ids = s
        .query_map(params![org, runner], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.iter()
        .map(|id| workspace_by_id(conn, org, id))
        .collect()
}
pub fn workspace_by_id(conn: &Connection, org: &str, workspace: &str) -> Result<Value> {
    let row=conn.query_row("SELECT runner_id,name,root_path,discovered_json,repositories_json,inventory_revision,confirmed_revision FROM factory_workspaces WHERE id=?1 AND org_id=?2",params![workspace,org],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,i64>(5)?,r.get::<_,Option<i64>>(6)?))).optional()?.ok_or_else(||anyhow::anyhow!("not_found"))?;
    Ok(
        json!({"id":workspace,"runner_id":row.0,"name":row.1,"root_path":row.2,"discovered":parse(row.3)?,"repositories":parse(row.4)?,"inventory_revision":row.5,"confirmed_revision":row.6,"runtime":super::workflow_runtime::metadata(conn,workspace)?}),
    )
}
pub fn report_inventory(
    conn: &Connection,
    org: &str,
    runner: &str,
    workspace: &str,
    repos: &[Repository],
) -> Result<()> {
    crate::factory::workflows::validate_repositories(repos).map_err(anyhow::Error::msg)?;
    if repos.iter().any(|r| r.writable) {
        bail!("discovery_cannot_grant_write");
    }
    let inventory = serde_json::to_string(repos)?;
    // Identical polling does not invalidate operator approval.
    let n=conn.execute("UPDATE factory_workspaces SET inventory_revision=inventory_revision+CASE WHEN discovered_json!=?1 THEN 1 ELSE 0 END, discovered_json=?1 WHERE id=?2 AND org_id=?3 AND runner_id=?4",params![inventory,workspace,org,runner])?;
    if n != 1 {
        bail!("not_found");
    }
    Ok(())
}
pub fn confirm_inventory(
    conn: &Connection,
    org: &str,
    workspace: &str,
    revision: i64,
    repos: &[Repository],
) -> Result<()> {
    crate::factory::workflows::validate_repositories(repos).map_err(anyhow::Error::msg)?;
    let w = workspace_by_id(conn, org, workspace)?;
    if w["inventory_revision"].as_i64() != Some(revision) {
        bail!("inventory_conflict");
    }
    let discovered: Vec<Repository> = serde_json::from_value(w["discovered"].clone())?;
    for r in repos {
        if !discovered
            .iter()
            .any(|d| d.path == r.path && d.remote == r.remote && d.head_sha == r.head_sha)
        {
            bail!("repository_not_discovered");
        }
    }
    if conn.execute("UPDATE factory_workspaces SET repositories_json=?1,confirmed_revision=?2 WHERE id=?3 AND org_id=?4 AND inventory_revision=?2",params![serde_json::to_string(repos)?,revision,workspace,org])?!=1 { bail!("inventory_conflict"); }
    Ok(())
}
pub fn create_definition(
    conn: &Connection,
    org: &str,
    user: &str,
    d: &Definition,
) -> Result<Value> {
    let w = workspace_by_id(conn, org, &d.workspace_id)?;
    if w["confirmed_revision"].is_null() || w["confirmed_revision"] != w["inventory_revision"] {
        bail!("workspace_not_confirmed");
    }
    let repos: Vec<Repository> = serde_json::from_value(w["repositories"].clone())?;
    d.validate(&repos).map_err(anyhow::Error::msg)?;
    let definition = id();
    conn.execute("INSERT INTO factory_workflow_definitions(id,org_id,workspace_id,definition_json,created_by) VALUES (?1,?2,?3,?4,?5)",params![definition,org,d.workspace_id,serde_json::to_string(d)?,user])?;
    Ok(json!({"id":definition,"definition":d}))
}
pub fn list_definitions(conn: &Connection, org: &str) -> Result<Vec<Value>> {
    let mut s=conn.prepare("SELECT id,definition_json FROM factory_workflow_definitions WHERE org_id=?1 ORDER BY created_at DESC")?;
    let rows = s.query_map([org], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.map(|r| {
        let (id, d) = r?;
        Ok(json!({"id":id,"definition":parse(d)?}))
    })
    .collect()
}
pub fn start_run(
    conn: &Connection,
    org: &str,
    definition: &str,
    source_key: &str,
    task: &Value,
) -> Result<Value> {
    if source_key.is_empty() || source_key.len() > 512 || task.to_string().len() > 65536 {
        bail!("invalid_task");
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let text: String = tx
        .query_row(
            "SELECT definition_json FROM factory_workflow_definitions WHERE id=?1 AND org_id=?2",
            params![definition, org],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("not_found"))?;
    let d: Definition = serde_json::from_str(&text)?;
    let mut w = workspace_by_id(&tx, org, &d.workspace_id)?;
    if w["confirmed_revision"].is_null() || w["confirmed_revision"] != w["inventory_revision"] {
        bail!("workspace_not_confirmed");
    }
    d.validate(&serde_json::from_value::<Vec<Repository>>(
        w["repositories"].clone(),
    )?)
    .map_err(anyhow::Error::msg)?;
    let active_runner: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_runners WHERE id=?1 AND org_id=?2 AND revoked=0)",
        params![w["runner_id"].as_str(), org],
        |r| r.get(0),
    )?;
    if !active_runner {
        bail!("runner_revoked");
    }
    let existing:Option<String>=tx.query_row("SELECT id FROM factory_workflow_runs WHERE org_id=?1 AND workspace_id=?2 AND source_key=?3",params![org,d.workspace_id,source_key],|r|r.get(0)).optional()?;
    if let Some(existing) = existing {
        tx.commit()?;
        return run_detail(conn, org, &existing);
    }
    if !factory_enabled(&tx, org)? {
        bail!("factory_paused")
    }
    if w["runtime"]["revision"].as_u64().unwrap_or(0) > 0
        && w["runtime"]["ready"] != true
        && d.steps
            .iter()
            .any(|s| !matches!(s.kind, StepKind::Prepare | StepKind::Notify))
    {
        bail!("runtime_configuration_incomplete")
    }
    if w["runtime"]["revision"].as_u64().unwrap_or(0) > 0
        && d.steps
            .iter()
            .any(|s| matches!(s.kind, StepKind::Agent | StepKind::Review))
    {
        let config: super::workflow_runtime::RuntimeConfig =
            serde_json::from_value(w["runtime"]["config"].clone())?;
        let resolved = super::workflow_runtime::resolve_bindings(&tx, org, &config)?;
        super::workflow_runtime::validate(
            &resolved,
            &serde_json::from_value::<Vec<Repository>>(w["repositories"].clone())?
                .into_iter()
                .map(|r| r.path)
                .collect::<Vec<_>>(),
        )?;
        w["runtime"]["config"] = serde_json::to_value(resolved)?;
    }
    let run = id();
    let snapshot = json!({"definition":d,"workspace":w});
    tx.execute("INSERT INTO factory_workflow_runs(id,org_id,definition_id,workspace_id,runner_id,source_key,task_json,snapshot_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![run,org,definition,d.workspace_id,w["runner_id"].as_str(),source_key,task.to_string(),snapshot.to_string()])?;
    tx.execute(
        "UPDATE factory_workflow_runs SET heads_json=?1 WHERE id=?2",
        params![
            super::workflow_execution::base_heads(&snapshot)?.to_string(),
            run
        ],
    )?;
    for step in &d.steps {
        tx.execute(
            "INSERT INTO factory_workflow_steps(run_id,step_id) VALUES (?1,?2)",
            params![run, step.id],
        )?;
    }
    event(
        &tx,
        &run,
        None,
        "run.queued",
        json!({"source_key":source_key}),
    )?;
    tx.commit()?;
    run_detail(conn, org, &run)
}
pub fn factory_enabled(conn: &Connection, org: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT autonomous_agents_enabled FROM organizations WHERE id=?1",
        [org],
        |r| r.get::<_, bool>(0),
    )?)
}
/// Revoke active native leases atomically with the shared organization switch.
/// Queued work and pending human decisions are retained, not deleted.
pub fn pause_active_runs(conn: &Connection, org: &str) -> Result<()> {
    let ids=conn.prepare("SELECT DISTINCT r.id FROM factory_workflow_runs r JOIN factory_workflow_steps s ON s.run_id=r.id WHERE r.org_id=?1 AND r.status IN ('queued','running','blocked') AND s.status='running'")?.query_map([org],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        conn.execute(
            "UPDATE factory_workflow_runs SET status='cancelled' WHERE id=?1",
            [&id],
        )?;
        conn.execute("UPDATE factory_workflow_steps SET status='cancelled',lease_token=NULL,lease_until=NULL WHERE run_id=?1 AND status!='succeeded'",[&id])?;
        event(
            conn,
            &id,
            None,
            "run.cancelled",
            json!({"reason":"organization_kill_switch"}),
        )?;
    }
    Ok(())
}
pub fn list_findings(conn: &Connection, org: &str) -> Result<Vec<Value>> {
    let mut q=conn.prepare("SELECT r.id,s.step_id,r.status,json_extract(r.snapshot_json,'$.definition.name'),CAST(f.value AS TEXT),r.change_revision FROM factory_workflow_runs r JOIN factory_workflow_steps s ON s.run_id=r.id JOIN json_each(CASE WHEN json_type(s.result_json,'$.output.findings')='array' THEN json_extract(s.result_json,'$.output.findings') ELSE '[]' END) f WHERE r.org_id=?1 AND s.status IN ('succeeded','blocked') AND json_extract(s.result_json,'$.input_revision')=r.change_revision ORDER BY r.created_at DESC,s.step_id LIMIT 100")?;
    let rows = q
        .query_map([org], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, u32>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for (run, step, status, name, text, revision) in rows {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if v.is_object() {
                out.push(json!({"run_id":run,"step_id":step,"run_status":status,"definition_name":name,"revision":revision,"title":v["title"].as_str().unwrap_or("Observación del workflow").chars().take(300).collect::<String>(),"severity":v["severity"].as_str().unwrap_or("info").chars().take(30).collect::<String>(),"summary":v["summary"].as_str().or_else(||v["description"].as_str()).unwrap_or("").chars().take(2000).collect::<String>()}));
            }
        }
    }
    Ok(out)
}
pub fn list_pending_runs(conn: &Connection, org: &str) -> Result<Vec<Value>> {
    let mut q=conn.prepare("SELECT r.id,r.source_key,r.created_at,json_extract(r.task_json,'$.title'),json_extract(r.snapshot_json,'$.definition.name'),json_extract(r.snapshot_json,'$.workspace.name'),(SELECT kind FROM factory_workflow_events e WHERE e.run_id=r.id ORDER BY seq DESC LIMIT 1) FROM factory_workflow_runs r WHERE r.org_id=?1 AND r.status='blocked' ORDER BY r.created_at LIMIT 100")?;
    let rows=q.query_map([org],|r|Ok(json!({"id":r.get::<_,String>(0)?,"source_key":r.get::<_,String>(1)?,"created_at":r.get::<_,String>(2)?,"title":r.get::<_,Option<String>>(3)?,"definition_name":r.get::<_,Option<String>>(4)?,"workspace_name":r.get::<_,Option<String>>(5)?,"last_event":r.get::<_,Option<String>>(6)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn list_runs(conn: &Connection, org: &str) -> Result<Vec<Value>> {
    let mut s=conn.prepare("SELECT id,status,source_key,definition_id,created_at FROM factory_workflow_runs WHERE org_id=?1 ORDER BY created_at DESC LIMIT 100")?;
    let rows=s.query_map([org],|r|Ok(json!({"id":r.get::<_,String>(0)?,"status":r.get::<_,String>(1)?,"source_key":r.get::<_,String>(2)?,"definition_id":r.get::<_,String>(3)?,"created_at":r.get::<_,String>(4)?})))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
pub fn run_detail(conn: &Connection, org: &str, run: &str) -> Result<Value> {
    let row=conn.query_row("SELECT status,task_json,snapshot_json,source_key FROM factory_workflow_runs WHERE id=?1 AND org_id=?2",params![run,org],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).optional()?.ok_or_else(||anyhow::anyhow!("not_found"))?;
    let mut s = conn.prepare(
        "SELECT step_id,status,attempt,result_json FROM factory_workflow_steps WHERE run_id=?1",
    )?;
    let steps=s.query_map([run],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u32>(2)?,r.get::<_,Option<String>>(3)?)))?.map(|r|{let(id,status,attempt,result)=r?;Ok(json!({"id":id,"status":status,"attempt":attempt,"result":result.map(parse).transpose()?}))}).collect::<Result<Vec<Value>>>()?;
    let mut s=conn.prepare("SELECT seq,step_id,kind,payload_json,created_at FROM factory_workflow_events WHERE run_id=?1 ORDER BY seq DESC LIMIT 1000")?;
    let events=s.query_map([run],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?)))?.map(|r|{let(seq,step,kind,payload,at)=r?;Ok(json!({"seq":seq,"step_id":step,"kind":kind,"payload":parse(payload)?,"created_at":at}))}).collect::<Result<Vec<Value>>>()?;
    Ok(
        json!({"id":run,"status":row.0,"task":parse(row.1)?,"snapshot":parse(row.2)?,"source_key":row.3,"steps":steps,"events":events,"execution":super::workflow_execution::metadata(conn,run)?,"artifacts":super::workflow_execution::artifacts(conn,run)?}),
    )
}

/// Clock-driven housekeeping also runs on UI reads, so a disconnected runner
/// cannot leave a workflow appearing active indefinitely.
pub fn expire_leases(conn: &Connection, org: &str, now: i64) -> Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let mut query = tx.prepare("SELECT s.run_id,s.step_id FROM factory_workflow_steps s JOIN factory_workflow_runs r ON r.id=s.run_id WHERE r.org_id=?1 AND r.status IN ('queued','running','blocked') AND s.status='running' AND s.lease_until<=?2")?;
    let expired = query
        .query_map(params![org, now], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(query);
    for (run, step) in expired {
        tx.execute("UPDATE factory_workflow_steps SET status='blocked',lease_token=NULL WHERE run_id=?1 AND step_id=?2",params![run,step])?;
        tx.execute(
            "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
            [&run],
        )?;
        event(
            &tx,
            &run,
            Some(&step),
            "step.lease_expired",
            json!({"reason":"reconcile_external_effects_before_retry"}),
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Leases are deliberately not auto-reclaimed: a crashed publisher may have performed
/// an external write. An operator must reconcile and explicitly retry with a new fence.
pub fn claim(
    conn: &Connection,
    org: &str,
    runner: &str,
    capabilities: &[String],
    now: i64,
) -> Result<Option<Value>> {
    if capabilities.len() > 128 || capabilities.iter().any(|s| s.len() > 100) {
        bail!("invalid_capabilities");
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    tx.execute("UPDATE factory_runners SET last_seen=datetime('now'),capabilities_json=?1 WHERE id=?2 AND org_id=?3 AND revoked=0",params![serde_json::to_string(capabilities)?,runner,org])?;
    if !factory_enabled(&tx, org)? {
        tx.commit()?;
        return Ok(None);
    }
    let mut s=tx.prepare("SELECT id,snapshot_json,task_json FROM factory_workflow_runs WHERE org_id=?1 AND runner_id=?2 AND status IN ('queued','running') ORDER BY created_at,id LIMIT 100")?;
    let runs = s
        .query_map(params![org, runner], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(s);
    let mut job = None;
    'runs: for (run, snapshot, task) in runs {
        let snapshot = parse(snapshot)?;
        let d: Definition = serde_json::from_value(snapshot["definition"].clone())?;
        let execution = super::workflow_execution::metadata(&tx, &run)?;
        if execution["cost_usd"].as_f64().unwrap_or(0.0) >= d.max_cost_usd {
            tx.execute(
                "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
                [&run],
            )?;
            event(
                &tx,
                &run,
                None,
                "run.budget_exhausted",
                json!({"max_cost_usd":d.max_cost_usd}),
            )?;
            continue;
        }
        let mut s = tx.prepare(
            "SELECT step_id,status,attempt,lease_until FROM factory_workflow_steps WHERE run_id=?1",
        )?;
        let states = s
            .query_map([&run], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(s);
        if let Some((step, _, _, _)) = states
            .iter()
            .find(|(_, status, _, until)| status == "running" && until.is_some_and(|t| t <= now))
        {
            tx.execute("UPDATE factory_workflow_steps SET status='blocked',lease_token=NULL WHERE run_id=?1 AND step_id=?2",params![run,step])?;
            tx.execute(
                "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
                [&run],
            )?;
            event(
                &tx,
                &run,
                Some(step),
                "step.lease_expired",
                json!({"reason":"reconcile_external_effects_before_retry"}),
            )?;
            continue;
        }
        for step in &d.steps {
            let state = states
                .iter()
                .find(|s| s.0 == step.id)
                .ok_or_else(|| anyhow::anyhow!("step_missing"))?;
            if state.1 != "pending"
                || !step
                    .depends_on
                    .iter()
                    .all(|dep| states.iter().any(|s| &s.0 == dep && s.1 == "succeeded"))
            {
                continue;
            }
            if !step.capabilities.iter().all(|c| capabilities.contains(c))
                || !capabilities.contains(&format!("adapter:{}", step.adapter))
            {
                let already_reported:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_events WHERE run_id=?1 AND step_id=?2 AND kind='step.waiting_for_capabilities')",params![run,step.id],|r|r.get(0))?;
                if !already_reported {
                    event(
                        &tx,
                        &run,
                        Some(&step.id),
                        "step.waiting_for_capabilities",
                        json!({"adapter":step.adapter,"required":step.capabilities}),
                    )?;
                }
                continue;
            }
            // Writers are serialized against every other step. Read-only QA may
            // fan out, but no check can race a mutation of its input revision.
            let any_running = states.iter().any(|s| s.1 == "running");
            let writer_running = d.steps.iter().any(|s| {
                !s.write_repositories.is_empty()
                    && states
                        .iter()
                        .any(|state| state.0 == s.id && state.1 == "running")
            });
            if writer_running || (!step.write_repositories.is_empty() && any_running) {
                continue;
            }
            if !step.write_repositories.is_empty() || matches!(step.kind, StepKind::Publish) {
                let actions = if matches!(step.kind, StepKind::Publish) {
                    vec!["publish", "open_pr"]
                } else {
                    vec!["fix"]
                };
                let verified = d.steps.iter().any(|s| {
                    matches!(s.kind, StepKind::Gate)
                        && states
                            .iter()
                            .any(|state| state.0 == s.id && state.1 == "succeeded")
                });
                let held = actions.iter().find_map(|action| {
                    crate::factory::workflow_policy::require(&tx, org, &run, action, &[], verified)
                        .err()
                });
                if let Some(error) = held {
                    tx.execute("UPDATE factory_workflow_steps SET status='blocked' WHERE run_id=?1 AND step_id=?2",params![run,step.id])?;
                    tx.execute(
                        "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
                        [&run],
                    )?;
                    event(
                        &tx,
                        &run,
                        Some(&step.id),
                        "action.held",
                        json!({"reason":error.to_string(),"actions":actions,"revision":execution["revision"]}),
                    )?;
                    continue 'runs;
                }
            }
            let fence = id();
            let until = now + i64::from(step.timeout_seconds) + 30;
            tx.execute("UPDATE factory_workflow_steps SET status='running',attempt=attempt+1,lease_token=?1,lease_until=?2,input_revision=?5 WHERE run_id=?3 AND step_id=?4 AND status='pending'",params![fence,until,run,step.id,execution["revision"].as_u64()])?;
            tx.execute(
                "UPDATE factory_workflow_runs SET status='running' WHERE id=?1",
                [&run],
            )?;
            event(
                &tx,
                &run,
                Some(&step.id),
                "step.started",
                json!({"attempt":state.2+1,"lease_until":until}),
            )?;
            let mut dependencies = serde_json::Map::new();
            let mut results=tx.prepare("SELECT step_id,result_json FROM factory_workflow_steps WHERE run_id=?1 AND result_json IS NOT NULL")?;
            let results = results
                .query_map([&run], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, text) in results {
                let include = if matches!(step.kind, StepKind::Review) {
                    d.steps.iter().any(|s| {
                        s.id == id
                            && matches!(
                                s.kind,
                                StepKind::Check | StepKind::Gate | StepKind::Publish
                            )
                    })
                } else {
                    true
                };
                if include {
                    dependencies.insert(id, parse(text)?);
                }
            }
            job = Some(
                json!({"run_id":run,"step":step,"attempt":state.2+1,"lease_token":fence,"lease_until":until,"snapshot":snapshot,"task":parse(task)?,"execution":execution,"dependency_results":dependencies}),
            );
            break 'runs;
        }
    }
    tx.commit()?;
    Ok(job)
}

#[allow(clippy::too_many_arguments)]
pub fn complete(
    conn: &Connection,
    org: &str,
    runner: &str,
    run: &str,
    step_id: &str,
    fence: &str,
    success: bool,
    evidence: &[Evidence],
    now: i64,
) -> Result<()> {
    complete_with_outcome(
        conn, org, runner, run, step_id, fence, success, evidence, None, now,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn complete_with_outcome(
    conn: &Connection,
    org: &str,
    runner: &str,
    run: &str,
    step_id: &str,
    fence: &str,
    success: bool,
    evidence: &[Evidence],
    outcome: Option<&crate::factory::workflows::Outcome>,
    now: i64,
) -> Result<()> {
    if evidence.len() > 100 {
        bail!("too_much_evidence");
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let snapshot:String=tx.query_row("SELECT snapshot_json FROM factory_workflow_runs WHERE id=?1 AND org_id=?2 AND runner_id=?3 AND status IN ('running','blocked')",params![run,org,runner],|r|r.get(0)).optional()?.ok_or_else(||anyhow::anyhow!("lease_conflict"))?;
    let snapshot = parse(snapshot)?;
    let d: Definition = serde_json::from_value(snapshot["definition"].clone())?;
    let step = d
        .steps
        .iter()
        .find(|s| s.id == step_id)
        .ok_or_else(|| anyhow::anyhow!("not_found"))?;
    super::workflow_execution::lease(&tx, org, runner, run, step_id, fence, now)?;
    let execution = super::workflow_execution::metadata(&tx, run)?;
    let current: std::collections::BTreeMap<String, String> =
        serde_json::from_value(execution["heads"].clone())?;
    let revision = execution["revision"].as_u64().unwrap_or(0) as u32;
    let fallback = crate::factory::workflows::Outcome {
        input_revision: revision,
        ..Default::default()
    };
    let strict = outcome.is_some();
    let outcome = outcome.unwrap_or(&fallback);
    let claimed: u32 = tx.query_row(
        "SELECT input_revision FROM factory_workflow_steps WHERE run_id=?1 AND step_id=?2",
        params![run, step_id],
        |r| r.get(0),
    )?;
    if outcome.input_revision != revision || claimed != revision {
        bail!("revision_conflict");
    }
    if !outcome.cost_usd.is_finite()
        || outcome.cost_usd < 0.0
        || outcome.cost_usd > 1000.0
        || outcome.output.to_string().len() > 16384
    {
        bail!("invalid_outcome");
    }
    let commits = if outcome.heads.is_empty() {
        current.clone()
    } else {
        outcome.heads.clone()
    };
    if commits.len() != current.len()
        || commits.iter().any(|(path, sha)| {
            !current.contains_key(path)
                || !crate::factory::workflows::valid_sha(sha)
                || (current.get(path) != Some(sha) && !step.write_repositories.contains(path))
        })
    {
        bail!("unauthorized_change_set");
    }
    let changed = commits != current;
    if changed && !success {
        bail!("failed_step_cannot_change_heads");
    }
    if matches!(step.kind, StepKind::Gate) {
        for dependency in &step.depends_on {
            let text:String=tx.query_row("SELECT result_json FROM factory_workflow_steps WHERE run_id=?1 AND step_id=?2 AND status='succeeded'",params![run,dependency],|r|r.get(0))?;
            let result: Value = serde_json::from_str(&text)?;
            if result["heads"] != serde_json::to_value(&commits)?
                || result["evidence"].as_array().is_none_or(|e| e.is_empty())
            {
                bail!("stale_gate_evidence");
            }
        }
    }
    if matches!(step.kind, StepKind::Publish) && success {
        let base = super::workflow_execution::base_heads(&snapshot)?;
        for (path, head) in &commits {
            if base[path] != *head {
                let published:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_publications WHERE run_id=?1 AND repository_path=?2 AND head_sha=?3 AND state='published')",params![run,path,head],|r|r.get(0))?;
                if !published {
                    bail!("publication_missing");
                }
            }
        }
    }
    for e in evidence {
        if strict {
            let stored:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_artifacts a JOIN factory_workflow_steps s ON s.run_id=a.run_id AND s.step_id=a.step_id WHERE a.run_id=?1 AND a.step_id=?2 AND a.attempt=s.attempt AND a.digest=?3)",params![run,step_id,e.artifact_digest],|r|r.get(0))?;
            if !stored {
                bail!("artifact_missing");
            }
        }
        if e.check.is_empty()
            || e.check.len() > 120
            || e.artifact_digest.len() != 64
            || !e.artifact_digest.bytes().all(|b| b.is_ascii_hexdigit())
            || e.commits != commits
        {
            bail!("invalid_evidence");
        }
    }
    let required: Vec<String> = serde_json::from_value(
        step.options
            .get("required_checks")
            .cloned()
            .unwrap_or(json!([])),
    )
    .map_err(|_| anyhow::anyhow!("invalid_required_checks"))?;
    let optional = step
        .options
        .get("allow_not_applicable")
        .and_then(Value::as_bool)
        == Some(true);
    let applicable = |e: &Evidence| {
        e.status == EvidenceStatus::Pass
            || (optional
                && e.status == EvidenceStatus::NotApplicable
                && outcome.output["not_applicable_reason"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty()))
    };
    let verified = (!matches!(
        step.kind,
        StepKind::Check | StepKind::Gate | StepKind::Review
    ) || evidence.iter().any(applicable))
        && required
            .iter()
            .all(|name| evidence.iter().any(|e| &e.check == name && applicable(e)));
    let clean_review = outcome.output["verdict"] == "accept"
        && outcome.output["findings"]
            .as_array()
            .is_some_and(|findings| {
                findings.iter().all(|f| {
                    f["blocking"] != true
                        && !matches!(f["severity"].as_str(), Some("high" | "critical"))
                })
            });
    let passed = success
        && (!matches!(step.kind, StepKind::Review) || clean_review)
        && verified
        && !evidence
            .iter()
            .any(|e| matches!(e.status, EvidenceStatus::Fail | EvidenceStatus::Blocked));
    let status = if passed { "succeeded" } else { "blocked" };
    let result = json!({"evidence":evidence,"heads":commits,"input_revision":revision,"output":outcome.output,"cost_usd":outcome.cost_usd,"reason":if success && !verified {Some("missing_passing_evidence")} else {outcome.output["error"].as_str()}});
    let n=tx.execute("UPDATE factory_workflow_steps SET status=?1,result_json=?2,lease_token=NULL,lease_until=NULL WHERE run_id=?3 AND step_id=?4 AND status='running' AND lease_token=?5 AND lease_until>?6",params![status,result.to_string(),run,step_id,fence,now])?;
    if n != 1 {
        bail!("lease_conflict");
    }
    tx.execute(
        "UPDATE factory_workflow_runs SET cost_usd=cost_usd+?1 WHERE id=?2",
        params![outcome.cost_usd, run],
    )?;
    if changed {
        tx.execute("UPDATE factory_workflow_runs SET heads_json=?1,change_revision=change_revision+1 WHERE id=?2",params![serde_json::to_string(&commits)?,run])?;
        event(
            &tx,
            run,
            Some(step_id),
            "change_set.updated",
            json!({"revision":revision+1,"heads":commits}),
        )?;
    }
    if !passed {
        tx.execute(
            "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
            [run],
        )?;
    } else {
        let remaining: i64 = tx.query_row(
            "SELECT count(*) FROM factory_workflow_steps WHERE run_id=?1 AND status!='succeeded'",
            [run],
            |r| r.get(0),
        )?;
        if remaining == 0 {
            tx.execute(
                "UPDATE factory_workflow_runs SET status='succeeded' WHERE id=?1",
                [run],
            )?;
            event(&tx, run, None, "run.succeeded", json!({}))?;
        }
    }
    event(
        &tx,
        run,
        Some(step_id),
        if passed {
            "step.succeeded"
        } else {
            "step.blocked"
        },
        result.clone(),
    )?;
    if !passed
        && matches!(step.kind, StepKind::Check | StepKind::Review)
        && outcome.output.get("hold").and_then(Value::as_bool) != Some(true)
    {
        if let Some(target) = &d.repair_step {
            let repairs = execution["repairs"].as_u64().unwrap_or(0);
            if repairs < u64::from(d.max_repairs) {
                let mut reset = std::collections::BTreeSet::from([target.clone()]);
                loop {
                    let before = reset.len();
                    for s in &d.steps {
                        if s.depends_on.iter().any(|id| reset.contains(id)) {
                            reset.insert(s.id.clone());
                        }
                    }
                    if before == reset.len() {
                        break;
                    }
                }
                let mut eligible = reset.contains(step_id);
                for id in &reset {
                    let attempts: u32 = tx.query_row(
                        "SELECT attempt FROM factory_workflow_steps WHERE run_id=?1 AND step_id=?2",
                        params![run, id],
                        |r| r.get(0),
                    )?;
                    if d.steps
                        .iter()
                        .find(|s| &s.id == id)
                        .is_some_and(|s| attempts >= s.max_attempts)
                    {
                        eligible = false;
                    }
                }
                if eligible {
                    for id in &reset {
                        tx.execute("UPDATE factory_workflow_steps SET status='pending',lease_token=NULL,lease_until=NULL,result_json=NULL WHERE run_id=?1 AND step_id=?2",params![run,id])?;
                    }
                    tx.execute("UPDATE factory_workflow_runs SET status='queued',repair_count=repair_count+1,repair_feedback_json=?1 WHERE id=?2",params![result.to_string(),run])?;
                    event(
                        &tx,
                        run,
                        Some(target),
                        "repair.queued",
                        json!({"failed_step":step_id,"cycle":repairs+1}),
                    )?;
                }
            }
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn control_run(
    conn: &Connection,
    org: &str,
    run: &str,
    action: &str,
    step_id: Option<&str>,
) -> Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let detail = run_detail(&tx, org, run)?;
    if action == "cancel" {
        if !matches!(
            detail["status"].as_str(),
            Some("queued" | "running" | "blocked")
        ) {
            bail!("run_conflict");
        }
        tx.execute(
            "UPDATE factory_workflow_runs SET status='cancelled' WHERE id=?1",
            [run],
        )?;
        tx.execute("UPDATE factory_workflow_steps SET status='cancelled',lease_token=NULL WHERE run_id=?1 AND status!='succeeded'",[run])?;
    } else if action == "resume" {
        if detail["status"] != "blocked" {
            bail!("run_conflict");
        }
        tx.execute("UPDATE factory_workflow_steps SET status='pending',lease_token=NULL,lease_until=NULL WHERE run_id=?1 AND status='blocked' AND attempt=0",[run])?;
        let blocked: i64 = tx.query_row(
            "SELECT count(*) FROM factory_workflow_steps WHERE run_id=?1 AND status='blocked'",
            [run],
            |r| r.get(0),
        )?;
        if blocked == 0 {
            tx.execute(
                "UPDATE factory_workflow_runs SET status='queued' WHERE id=?1",
                [run],
            )?;
        }
    } else if action == "retry" {
        if detail["status"] != "blocked" {
            bail!("run_conflict");
        }
        let step_id = step_id.ok_or_else(|| anyhow::anyhow!("step_required"))?;
        let d: Definition = serde_json::from_value(detail["snapshot"]["definition"].clone())?;
        let step = d
            .steps
            .iter()
            .find(|s| s.id == step_id)
            .ok_or_else(|| anyhow::anyhow!("not_found"))?;
        let n=tx.execute("UPDATE factory_workflow_steps SET status='pending',lease_token=NULL,lease_until=NULL,result_json=NULL WHERE run_id=?1 AND step_id=?2 AND status='blocked' AND attempt<?3",params![run,step_id,step.max_attempts])?;
        if n != 1 {
            bail!("retry_limit_or_step_conflict");
        }
        let blocked: i64 = tx.query_row(
            "SELECT count(*) FROM factory_workflow_steps WHERE run_id=?1 AND status='blocked'",
            [run],
            |r| r.get(0),
        )?;
        if blocked == 0 {
            let running: i64 = tx.query_row(
                "SELECT count(*) FROM factory_workflow_steps WHERE run_id=?1 AND status='running'",
                [run],
                |r| r.get(0),
            )?;
            tx.execute(
                "UPDATE factory_workflow_runs SET status=?1 WHERE id=?2",
                params![if running > 0 { "running" } else { "queued" }, run],
            )?;
        }
    } else {
        bail!("invalid_action");
    }
    event(
        &tx,
        run,
        step_id,
        &format!("run.{action}"),
        json!({"operator_reconciled":action=="retry"}),
    )?;
    tx.commit()?;
    Ok(())
}
