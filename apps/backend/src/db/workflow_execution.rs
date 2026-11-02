//! Execution state, SHA-bound approvals and bounded artifact storage for workflows.
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn migrate(c: &Connection) -> Result<()> {
    if c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? >= 94 {
        return Ok(());
    }
    let t = Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    t.execute_batch("ALTER TABLE factory_workflow_runs ADD COLUMN heads_json TEXT NOT NULL DEFAULT '{}';
      ALTER TABLE factory_workflow_runs ADD COLUMN change_revision INTEGER NOT NULL DEFAULT 0;
      ALTER TABLE factory_workflow_runs ADD COLUMN repair_count INTEGER NOT NULL DEFAULT 0;
      ALTER TABLE factory_workflow_runs ADD COLUMN cost_usd REAL NOT NULL DEFAULT 0;
      ALTER TABLE factory_workflow_runs ADD COLUMN repair_feedback_json TEXT NOT NULL DEFAULT 'null';
      ALTER TABLE factory_workflow_steps ADD COLUMN input_revision INTEGER NOT NULL DEFAULT 0;
      CREATE TABLE factory_workflow_artifacts (
        id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES factory_workflow_runs(id) ON DELETE CASCADE,
        step_id TEXT NOT NULL, attempt INTEGER NOT NULL, name TEXT NOT NULL, mime TEXT NOT NULL,
        digest TEXT NOT NULL, content BLOB NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now')));
      CREATE INDEX workflow_artifact_run ON factory_workflow_artifacts(run_id,step_id);
      CREATE TABLE factory_workflow_approvals (
        run_id TEXT NOT NULL REFERENCES factory_workflow_runs(id) ON DELETE CASCADE,
        action TEXT NOT NULL, change_revision INTEGER NOT NULL, approved_by TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')), PRIMARY KEY(run_id,action,change_revision));
      CREATE TABLE factory_workflow_publications (
        run_id TEXT NOT NULL REFERENCES factory_workflow_runs(id) ON DELETE CASCADE,
        repository_path TEXT NOT NULL, head_sha TEXT NOT NULL, state TEXT NOT NULL,
        result_json TEXT NOT NULL DEFAULT '{}', PRIMARY KEY(run_id,repository_path));
      CREATE TABLE factory_workflow_intakes (
        id TEXT PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
        definition_id TEXT NOT NULL REFERENCES factory_workflow_definitions(id) ON DELETE CASCADE,
        kind TEXT NOT NULL CHECK(kind IN ('linear','slack','sentry')), connector_id TEXT NOT NULL,
        config_json TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, next_poll INTEGER NOT NULL DEFAULT 0,
        last_error TEXT, created_by TEXT NOT NULL);
      CREATE INDEX workflow_intake_due ON factory_workflow_intakes(org_id,enabled,next_poll);
      PRAGMA user_version=94;")?;
    // Upgrade active v93 runs without changing their immutable snapshots.
    let ids = {
        let mut s = t.prepare("SELECT id,snapshot_json FROM factory_workflow_runs")?;
        let rows = s
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (id, snapshot) in ids {
        let snapshot: Value = serde_json::from_str(&snapshot)?;
        let heads = base_heads(&snapshot)?;
        t.execute(
            "UPDATE factory_workflow_runs SET heads_json=?1 WHERE id=?2",
            params![heads.to_string(), id],
        )?;
    }
    t.commit()?;
    Ok(())
}
pub fn base_heads(snapshot: &Value) -> Result<Value> {
    let repos = snapshot["workspace"]["repositories"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid_snapshot"))?;
    let mut heads = serde_json::Map::new();
    for r in repos {
        heads.insert(
            r["path"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid_repository"))?
                .into(),
            r["head_sha"].clone(),
        );
    }
    Ok(Value::Object(heads))
}
pub fn metadata(c: &Connection, run: &str) -> Result<Value> {
    let (heads,revision,repairs,cost):(String,u32,u32,f64)=c.query_row("SELECT heads_json,change_revision,repair_count,cost_usd FROM factory_workflow_runs WHERE id=?1",[run],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    let mut s=c.prepare("SELECT repository_path,head_sha,state,result_json FROM factory_workflow_publications WHERE run_id=?1")?;
    let publications=s.query_map([run],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.map(|r|{let(path,head,state,result)=r?;Ok(json!({"path":path,"head_sha":head,"state":state,"result":serde_json::from_str::<Value>(&result)?}))}).collect::<Result<Vec<Value>>>()?;
    let feedback: String = c.query_row(
        "SELECT repair_feedback_json FROM factory_workflow_runs WHERE id=?1",
        [run],
        |r| r.get(0),
    )?;
    Ok(
        json!({"heads":serde_json::from_str::<Value>(&heads)?,"revision":revision,"repairs":repairs,"cost_usd":cost,"publications":publications,"repair_feedback":serde_json::from_str::<Value>(&feedback)?}),
    )
}
pub fn lease(
    c: &Connection,
    org: &str,
    runner: &str,
    run: &str,
    step: &str,
    fence: &str,
    now: i64,
) -> Result<u32> {
    c.query_row("SELECT s.attempt FROM factory_workflow_steps s JOIN factory_workflow_runs r ON r.id=s.run_id JOIN factory_runners a ON a.id=r.runner_id WHERE r.id=?1 AND r.org_id=?2 AND r.runner_id=?3 AND a.revoked=0 AND r.status IN ('running','blocked') AND s.step_id=?4 AND s.status='running' AND s.lease_token=?5 AND s.lease_until>?6",params![run,org,runner,step,fence,now],|r|r.get(0)).optional()?.ok_or_else(||anyhow::anyhow!("lease_conflict"))
}
pub fn artifacts(c: &Connection, run: &str) -> Result<Vec<Value>> {
    let mut s=c.prepare("SELECT id,step_id,attempt,name,mime,digest,length(content),created_at FROM factory_workflow_artifacts WHERE run_id=?1 ORDER BY created_at,id")?;
    let rows=s.query_map([run],|r|Ok(json!({"id":r.get::<_,String>(0)?,"step_id":r.get::<_,String>(1)?,"attempt":r.get::<_,u32>(2)?,"name":r.get::<_,String>(3)?,"mime":r.get::<_,String>(4)?,"digest":r.get::<_,String>(5)?,"size":r.get::<_,usize>(6)?,"created_at":r.get::<_,String>(7)?})))?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
pub fn put_artifact(
    c: &Connection,
    org: &str,
    runner: &str,
    run: &str,
    step: &str,
    fence: &str,
    name: &str,
    mime: &str,
    content: &[u8],
    now: i64,
) -> Result<Value> {
    if name.is_empty()
        || name.len() > 120
        || name.contains('/')
        || name.chars().any(char::is_control)
        || !["text/plain", "application/json", "image/png", "image/jpeg"].contains(&mime)
        || content.len() > 1024 * 1024
    {
        bail!("invalid_artifact")
    }
    let t = Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let attempt = lease(&t, org, runner, run, step, fence, now)?;
    let total: i64 = t.query_row(
        "SELECT COALESCE(sum(length(content)),0) FROM factory_workflow_artifacts WHERE run_id=?1",
        [run],
        |r| r.get(0),
    )?;
    if total + content.len() as i64 > 32 * 1024 * 1024 {
        bail!("artifact_quota")
    }
    let digest = hex::encode(Sha256::digest(content));
    let id = uuid::Uuid::new_v4().to_string();
    t.execute("INSERT INTO factory_workflow_artifacts(id,run_id,step_id,attempt,name,mime,digest,content) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id,run,step,attempt,name,mime,digest,content])?;
    super::workflows::event(
        &t,
        run,
        Some(step),
        "artifact.stored",
        json!({"id":id,"name":name,"digest":digest,"mime":mime}),
    )?;
    t.commit()?;
    Ok(json!({"id":id,"digest":digest}))
}
pub fn get_artifact(c: &Connection, org: &str, id: &str) -> Result<(String, Vec<u8>)> {
    c.query_row("SELECT a.mime,a.content FROM factory_workflow_artifacts a JOIN factory_workflow_runs r ON r.id=a.run_id WHERE a.id=?1 AND r.org_id=?2",params![id,org],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||anyhow::anyhow!("not_found"))
}
pub fn rotate_runner(c: &Connection, org: &str, runner: &str) -> Result<Value> {
    let t = Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let token = format!("nmr_{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    if t.execute(
        "UPDATE factory_runners SET token_hash=?1,revoked=0 WHERE id=?2 AND org_id=?3",
        params![super::workflows::token_hash(&token), runner, org],
    )? != 1
    {
        bail!("not_found")
    }
    let runs = {
        let mut s=t.prepare("SELECT id FROM factory_workflow_runs WHERE runner_id=?1 AND org_id=?2 AND status IN ('running','queued')")?;
        let ids = s
            .query_map(params![runner, org], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids
    };
    for run in runs {
        t.execute(
            "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
            [&run],
        )?;
        t.execute("UPDATE factory_workflow_steps SET status='blocked',lease_token=NULL,lease_until=NULL WHERE run_id=?1 AND status='running'",[&run])?;
        super::workflows::event(
            &t,
            &run,
            None,
            "runner.rotated",
            json!({"runner_id":runner}),
        )?;
    }
    t.commit()?;
    Ok(json!({"id":runner,"token":token}))
}
pub fn approve(
    c: &Connection,
    org: &str,
    user: &str,
    run: &str,
    action: &str,
    revision: u32,
) -> Result<()> {
    if !["fix", "publish", "open_pr", "reply", "notify"].contains(&action) {
        bail!("invalid_action")
    }
    let t = Transaction::new_unchecked(c, TransactionBehavior::Immediate)?;
    let current:Option<u32>=t.query_row("SELECT change_revision FROM factory_workflow_runs WHERE id=?1 AND org_id=?2 AND status!='cancelled'",params![run,org],|r|r.get(0)).optional()?;
    if current != Some(revision) {
        bail!("revision_conflict")
    }
    t.execute("INSERT OR REPLACE INTO factory_workflow_approvals(run_id,action,change_revision,approved_by) VALUES (?1,?2,?3,?4)",params![run,action,revision,user])?;
    super::workflows::event(
        &t,
        run,
        None,
        "action.approved",
        json!({"action":action,"revision":revision,"actor":user}),
    )?;
    // A policy hold performed no external write and does not consume a repair
    // attempt. Only an explicit operator approval can unlock such a hold.
    t.execute("UPDATE factory_workflow_steps SET status='pending',attempt=max(0,attempt-1),lease_token=NULL,lease_until=NULL WHERE run_id=?1 AND status='blocked' AND json_extract(result_json,'$.output.error')='policy_approval_required'",[run])?;
    t.commit()?;
    Ok(())
}
pub fn approved(c: &Connection, run: &str, action: &str, revision: u32) -> Result<bool> {
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_approvals WHERE run_id=?1 AND action=?2 AND change_revision=?3)",params![run,action,revision],|r|r.get(0))?)
}
