//! Durable QA scenario outcomes; deliberately has no run foreign key so run cleanup cannot erase coverage.
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Published main owns versions through 98. Unpublished QA migrations 91–100
/// are registered here as 99–108; actor and chronological backfill follow as 109–110.
pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 99 {
        conn.execute_batch("BEGIN IMMEDIATE;
      CREATE TABLE IF NOT EXISTS qa_scenario_results (
        id INTEGER PRIMARY KEY,
        org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
        agent_id TEXT NOT NULL,
        project_id TEXT,
        application TEXT NOT NULL,
        environment TEXT NOT NULL,
        run_id TEXT NOT NULL,
        scenario_key TEXT NOT NULL,
        description TEXT NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('passed','failed','blocked')),
        evidence_json TEXT NOT NULL DEFAULT '[]',
        tested_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(org_id, run_id, scenario_key)
      );
      CREATE INDEX IF NOT EXISTS qa_scenario_history
        ON qa_scenario_results(org_id, agent_id, application, environment, tested_at DESC);
      PRAGMA user_version = 99;
      COMMIT;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 100 {
        let has_destination: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('autonomous_agent_deliveries') WHERE name='destination_repository')",
            [], |row| row.get(0),
        )?;
        let has_deliveries: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='autonomous_agent_deliveries')",
            [], |row| row.get(0),
        )?;
        if has_deliveries && !has_destination {
            conn.execute_batch("ALTER TABLE autonomous_agent_deliveries ADD COLUMN destination_repository TEXT;")?;
        }
        conn.execute_batch("PRAGMA user_version = 100;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 101 {
        let has_redaction_marker: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('qa_scenario_results') WHERE name='redaction_verified')",
            [], |row| row.get(0),
        )?;
        if !has_redaction_marker {
            // Existing rows predate complete secret-boundary guarantees; quarantine rather than claim they are clean.
            conn.execute_batch("ALTER TABLE qa_scenario_results ADD COLUMN redaction_verified INTEGER NOT NULL DEFAULT 0 CHECK(redaction_verified IN (0,1));")?;
        }
        conn.execute_batch("PRAGMA user_version = 101;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 102 {
        let has_transcript: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='autonomous_agent_run_transcript')",
            [], |row| row.get(0),
        )?;
        let has_marker = if has_transcript {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('autonomous_agent_run_transcript') WHERE name='redaction_verified')",
                [], |row| row.get(0),
            )?
        } else { true };
        if has_transcript && !has_marker {
            // Historical transcript payloads cannot be safely re-redacted after credentials rotate.
            conn.execute_batch("ALTER TABLE autonomous_agent_run_transcript ADD COLUMN redaction_verified INTEGER NOT NULL DEFAULT 0 CHECK(redaction_verified IN (0,1));")?;
        }
        conn.execute_batch("PRAGMA user_version = 102;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 103 {
        conn.execute_batch("BEGIN IMMEDIATE;
          CREATE TABLE IF NOT EXISTS qa_repository_grants (
            id TEXT PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            repository TEXT NOT NULL, can_read INTEGER NOT NULL CHECK(can_read IN (0,1)),
            can_publish INTEGER NOT NULL CHECK(can_publish IN (0,1)), credential_identity TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)), granted_by TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT(datetime('now')), revoked_at TEXT,
            UNIQUE(org_id,project_id,repository));
          CREATE TABLE IF NOT EXISTS qa_repository_snapshots (
            id TEXT PRIMARY KEY, org_id TEXT NOT NULL, project_id TEXT NOT NULL,
            run_id TEXT NOT NULL, revision_id TEXT NOT NULL, snapshot_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT(datetime('now')), UNIQUE(org_id,run_id));
          CREATE INDEX IF NOT EXISTS qa_grants_project ON qa_repository_grants(org_id,project_id,active);
          PRAGMA user_version=103; COMMIT;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 104 {
        let events: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='autonomous_agent_events')", [], |r| r.get(0))?;
        if events {
            let has: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('autonomous_agent_events') WHERE name='redaction_verified')", [], |r| r.get(0))?;
            if !has { conn.execute_batch("ALTER TABLE autonomous_agent_events ADD COLUMN redaction_verified INTEGER NOT NULL DEFAULT 0 CHECK(redaction_verified IN (0,1));")?; }
        }
        let findings: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='autonomous_agent_findings')", [], |r| r.get(0))?;
        if findings {
            let has: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('autonomous_agent_findings') WHERE name='redaction_verified')", [], |r| r.get(0))?;
            if !has { conn.execute_batch("ALTER TABLE autonomous_agent_findings ADD COLUMN redaction_verified INTEGER NOT NULL DEFAULT 0 CHECK(redaction_verified IN (0,1));")?; }
        }
        conn.execute_batch("PRAGMA user_version=104;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 105 {
        conn.execute_batch("BEGIN IMMEDIATE;
          CREATE TABLE IF NOT EXISTS qa_scenario_checkpoints (
            id INTEGER PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL, project_id TEXT, application TEXT NOT NULL, environment TEXT NOT NULL,
            run_id TEXT NOT NULL, attempt_id TEXT NOT NULL, scenario_key TEXT NOT NULL,
            description TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('passed','failed','blocked')),
            evidence_json TEXT NOT NULL, tested_at TEXT NOT NULL DEFAULT(datetime('now')),
            redaction_verified INTEGER NOT NULL CHECK(redaction_verified IN (0,1)),
            UNIQUE(org_id,run_id,attempt_id,scenario_key));
          CREATE INDEX IF NOT EXISTS qa_checkpoint_history ON qa_scenario_checkpoints(org_id,agent_id,application,environment,tested_at DESC);
          PRAGMA user_version=105; COMMIT;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 106 {
        conn.execute_batch("BEGIN IMMEDIATE;
          CREATE TABLE IF NOT EXISTS qa_scenario_inventory (
            id INTEGER PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL, project_id TEXT, application TEXT NOT NULL, environment TEXT NOT NULL,
            module_key TEXT NOT NULL, scenario_key TEXT NOT NULL, description TEXT NOT NULL,
            critical INTEGER NOT NULL DEFAULT 0 CHECK(critical IN (0,1)),
            last_status TEXT CHECK(last_status IN ('passed','failed','blocked')),
            blocker_fingerprint TEXT, last_blocker_fingerprint TEXT, last_tested_at TEXT,
            UNIQUE(org_id,agent_id,project_id,application,environment,scenario_key));
          CREATE TABLE IF NOT EXISTS qa_batch_plans (
            id INTEGER PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL, project_id TEXT, application TEXT NOT NULL, environment TEXT NOT NULL,
            run_id TEXT NOT NULL, attempt_id TEXT NOT NULL, scenario_key TEXT NOT NULL,
            description TEXT NOT NULL, reason TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending','not_run','passed','failed','blocked')),
            blocker_fingerprint TEXT, planned_at TEXT NOT NULL DEFAULT(datetime('now')),
            UNIQUE(org_id,run_id,attempt_id,scenario_key));
          CREATE UNIQUE INDEX IF NOT EXISTS qa_inventory_identity ON qa_scenario_inventory(org_id,agent_id,ifnull(project_id,''),application,environment,scenario_key);
          CREATE INDEX IF NOT EXISTS qa_inventory_scope ON qa_scenario_inventory(org_id,agent_id,project_id,application,environment);
          CREATE INDEX IF NOT EXISTS qa_plan_scope ON qa_batch_plans(org_id,agent_id,application,environment,planned_at DESC,id DESC);
          INSERT OR IGNORE INTO qa_scenario_inventory(org_id,agent_id,project_id,application,environment,module_key,scenario_key,description,last_status,last_tested_at)
            SELECT org_id,agent_id,project_id,application,environment,CASE WHEN instr(scenario_key,'-')>0 THEN substr(scenario_key,1,instr(scenario_key,'-')-1) ELSE 'general' END,scenario_key,description,status,tested_at FROM qa_scenario_results WHERE redaction_verified=1;
          PRAGMA user_version=106; COMMIT;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 107 {
        conn.execute_batch("BEGIN IMMEDIATE;
          CREATE TABLE qa_scenario_results_v107 (
            id INTEGER PRIMARY KEY, org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL, project_id TEXT, application TEXT NOT NULL, environment TEXT NOT NULL,
            run_id TEXT NOT NULL, attempt_id TEXT NOT NULL, scenario_key TEXT NOT NULL, description TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('passed','failed','blocked')), evidence_json TEXT NOT NULL DEFAULT '[]',
            tested_at TEXT NOT NULL DEFAULT(datetime('now')), redaction_verified INTEGER NOT NULL DEFAULT 0 CHECK(redaction_verified IN (0,1)),
            UNIQUE(org_id,run_id,attempt_id,scenario_key));
          INSERT INTO qa_scenario_results_v107(id,org_id,agent_id,project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,tested_at,redaction_verified)
            SELECT id,org_id,agent_id,project_id,application,environment,run_id,'legacy',scenario_key,description,status,evidence_json,tested_at,redaction_verified FROM qa_scenario_results;
          DROP TABLE qa_scenario_results;
          ALTER TABLE qa_scenario_results_v107 RENAME TO qa_scenario_results;
          CREATE INDEX qa_scenario_history ON qa_scenario_results(org_id,agent_id,application,environment,tested_at DESC);
          CREATE UNIQUE INDEX qa_scenario_attempt_identity ON qa_scenario_results(org_id,run_id,attempt_id,scenario_key);
          PRAGMA user_version=107; COMMIT;")?;
    }
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 108 {
        let has_checkpoint_blocker: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('qa_scenario_checkpoints') WHERE name='blocker_fingerprint')", [], |row|row.get(0))?;
        if !has_checkpoint_blocker { conn.execute_batch("ALTER TABLE qa_scenario_checkpoints ADD COLUMN blocker_fingerprint TEXT;")?; }
        let has_delivery_claim: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('autonomous_agent_deliveries') WHERE name='claim_token')", [], |row|row.get(0))?;
        let has_deliveries: bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='autonomous_agent_deliveries')",[],|row|row.get(0))?;
        if has_deliveries && !has_delivery_claim {
            conn.execute_batch("ALTER TABLE autonomous_agent_deliveries ADD COLUMN claim_token TEXT; ALTER TABLE autonomous_agent_deliveries ADD COLUMN claim_expires_at TEXT;")?;
        }
        conn.execute_batch("PRAGMA user_version=108;")?;
    }
    conn.execute_batch("CREATE TABLE IF NOT EXISTS qa_observation_order(id INTEGER PRIMARY KEY AUTOINCREMENT);")?;
    for table in ["qa_scenario_checkpoints", "qa_scenario_results", "qa_scenario_inventory"] {
        let has: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name='observation_order')", [table], |row| row.get(0))?;
        if !has { conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN observation_order INTEGER;"))?; }
    }
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RepositoryGrant {
    pub repository: String,
    pub can_read: bool,
    pub can_publish: bool,
    pub credential_identity: String,
}

fn canonical_repository(value: &str) -> Option<String> {
    let value=value.trim();
    let path=if value.starts_with("https://github.com/") { value.trim_start_matches("https://github.com/").split(['?','#']).next()? }
        else if let Some(path)=value.strip_prefix("git@github.com:") { path }
        else { value };
    let path=path.strip_suffix(".git").unwrap_or(path).trim_end_matches('/');
    crate::automation::connectors::validate_repository(path).ok().map(|_|path.to_string())
}

/// Candidate links are informational only; authorization requires an active grant.
pub fn linked_repositories(conn: &Connection, org: &str, project: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT repo_url FROM code_projects WHERE org_id=?1 AND project_id=?2 AND archived_at IS NULL AND repo_url IS NOT NULL ORDER BY repo_url")?;
    let rows=stmt.query_map(params![org, project], |row| row.get::<_, String>(0))?;
    Ok(rows.filter_map(|row|row.ok()).filter_map(|url|canonical_repository(&url)).collect())
}

pub fn list_repository_grants(conn: &Connection, org: &str, project: &str) -> Result<Vec<RepositoryGrant>> {
    let mut stmt = conn.prepare("SELECT repository,can_read,can_publish,credential_identity FROM qa_repository_grants WHERE org_id=?1 AND project_id=?2 AND active=1 ORDER BY repository")?;
    let rows = stmt.query_map(params![org, project], |row| Ok(RepositoryGrant {
        repository: row.get(0)?, can_read: row.get::<_, i64>(1)? != 0,
        can_publish: row.get::<_, i64>(2)? != 0, credential_identity: row.get(3)?,
    }))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ponytail: grant authorization fields stay explicit at this database boundary.
#[allow(clippy::too_many_arguments, reason = "grant persistence uses separately validated scope, capability, identity, and actor inputs")]
pub fn set_repository_grant(conn: &Connection, org: &str, project: &str, repository: &str, can_read: bool, can_publish: bool, identity: &str, actor: &str) -> Result<()> {
    if !linked_repositories(conn,org,project)?.iter().any(|linked|linked==repository) { bail!("repository_not_linked_to_project") }
    if !can_read && can_publish { bail!("publish_requires_read") }
    conn.execute("INSERT INTO qa_repository_grants(id,org_id,project_id,repository,can_read,can_publish,credential_identity,active,granted_by) VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8) ON CONFLICT(org_id,project_id,repository) DO UPDATE SET can_read=excluded.can_read,can_publish=excluded.can_publish,credential_identity=excluded.credential_identity,active=1,granted_by=excluded.granted_by,created_at=datetime('now'),revoked_at=NULL", params![uuid::Uuid::new_v4().to_string(),org,project,repository,can_read as i64,can_publish as i64,identity,actor])?;
    Ok(())
}

pub fn revoke_repository_grant(conn: &Connection, org: &str, project: &str, repository: &str) -> Result<bool> {
    Ok(conn.execute("UPDATE qa_repository_grants SET active=0,revoked_at=datetime('now') WHERE org_id=?1 AND project_id=?2 AND repository=?3 AND active=1", params![org,project,repository])? != 0)
}

/// Insert once per run; callers must validate live access before using this snapshot.
pub fn create_repository_snapshot(conn: &Connection, org: &str, project: &str, run: &str, revision: &str, grants: &[RepositoryGrant]) -> Result<()> {
    let snapshot = serde_json::to_string(grants)?;
    let existing: Option<(String,String,String)> = conn.query_row("SELECT project_id,revision_id,snapshot_json FROM qa_repository_snapshots WHERE org_id=?1 AND run_id=?2", params![org,run], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    if let Some((old_project, old_revision, old_snapshot)) = existing {
        if old_project != project || old_revision != revision || old_snapshot != snapshot { bail!("qa_repository_snapshot_immutable") }
        return Ok(())
    }
    conn.execute("INSERT INTO qa_repository_snapshots(id,org_id,project_id,run_id,revision_id,snapshot_json) VALUES(?1,?2,?3,?4,?5,?6)", params![uuid::Uuid::new_v4().to_string(),org,project,run,revision,snapshot])?;
    Ok(())
}

pub fn snapshot_grant(conn: &Connection, org: &str, project: &str, run: &str, repository: &str, publish: bool) -> Result<Option<RepositoryGrant>> {
    Ok(snapshot_repositories(conn,org,project,run,publish)?.into_iter().find(|grant|grant.repository==repository))
}

pub fn snapshot_repositories(conn: &Connection, org: &str, project: &str, run: &str, publish: bool) -> Result<Vec<RepositoryGrant>> {
    let json: Option<String> = conn.query_row("SELECT snapshot_json FROM qa_repository_snapshots WHERE org_id=?1 AND project_id=?2 AND run_id=?3", params![org,project,run], |row| row.get(0)).optional()?;
    let Some(json) = json else { return Ok(Vec::new()) };
    let grants: Vec<RepositoryGrant> = serde_json::from_str(&json)?;
    Ok(grants.into_iter().filter(|grant|grant.can_read && (!publish || grant.can_publish)).collect())
}

fn scope(config: &Value) -> (Option<&str>, String, String) {
    let target = config.get("targets").and_then(Value::as_array).into_iter().flatten()
        .find(|target| target.get("kind").and_then(Value::as_str) == Some("web_application")
            && target.get("enabled").and_then(Value::as_bool) == Some(true));
    let label = config.get("application").or_else(|| config.get("target_name"))
        .and_then(Value::as_str)
        .or_else(|| target.and_then(|value| value.get("name")).and_then(Value::as_str))
        .unwrap_or("web application");
    // ponytail: retain the base path; collapsing to host aliases separately deployed apps.
    let target_identity = target.and_then(|value| {
        let url = value.pointer("/config/url").and_then(Value::as_str)?;
        let (scheme, rest) = url.split_once("://")?;
        let (path, _) = rest.split_once('?').unwrap_or((rest, ""));
        let (path, _) = path.split_once('#').unwrap_or((path, ""));
        let (authority, base_path) = path.split_once('/').unwrap_or((path, ""));
        let authority = authority.rsplit('@').next()?;
        let base_path = base_path.trim_end_matches('/');
        Some(format!("{}://{}{}{}", scheme.to_ascii_lowercase(), authority.to_ascii_lowercase(), if base_path.is_empty() { "" } else { "/" }, base_path))
    });
    let application = match target_identity {
        Some(identity) => format!("{label} · {identity}"),
        None => label.to_string(),
    };
    let environment = config.get("environment").and_then(Value::as_str)
        .or_else(|| target.and_then(|value| value.pointer("/config/environment")).and_then(Value::as_str))
        .unwrap_or("unspecified").to_string();
    (config.get("project_id").and_then(Value::as_str), application, environment)
}

/// Prioritize known scenarios deterministically within the same project and target.
pub fn context_for_scope(conn: &Connection, org: &str, agent: &str, config: &Value, limit: u32) -> Result<Vec<Value>> {
    let (project, application, environment) = scope(config);
    let budget = limit.min(50);
    if budget == 0 { return Ok(Vec::new()) }
    let order = "ORDER BY (SELECT COUNT(*) FROM qa_batch_plans plan WHERE plan.org_id=inventory.org_id AND plan.agent_id=inventory.agent_id AND plan.project_id IS inventory.project_id AND plan.application=inventory.application AND plan.environment=inventory.environment AND plan.scenario_key=inventory.scenario_key),
        CASE WHEN last_status IS NULL THEN 0 WHEN last_status='failed' THEN 1 WHEN last_status='blocked' THEN 2 ELSE 3 END,
        CASE WHEN scenario_key LIKE '%combo%' OR scenario_key LIKE '%limit%' OR scenario_key LIKE '%regression%' THEN 1 ELSE 0 END,
        COALESCE(last_tested_at,'') ASC,module_key,scenario_key";
    let select = |critical_only: bool, count: u32| -> Result<Vec<Value>> {
        let sql = format!("SELECT scenario_key,description,module_key,last_status,last_tested_at,critical FROM qa_scenario_inventory inventory
          WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5
            AND (COALESCE(last_status,'')!='blocked' OR blocker_fingerprint IS NOT last_blocker_fingerprint)
            AND critical={}
          {order} LIMIT ?6", if critical_only { 1 } else { 0 });
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![org, agent, project, application, environment, count], |row| {
            let status: Option<String> = row.get(3)?;
            let critical: bool = row.get(5)?;
            let reason = if critical { "recheck critical scenario" } else { match status.as_deref() { None => "known scenario not yet executed", Some("failed") => "recheck known failed scenario", _ => "rotate a previously passed scenario" } };
            Ok(json!({"key":row.get::<_,String>(0)?,"description":row.get::<_,String>(1)?,"module":row.get::<_,String>(2)?,"last_status":status,"tested_at":row.get::<_,Option<String>>(4)?,"critical":critical,"reason":reason}))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    };
    // ponytail: cap critical reservation at 20%; the progressive remainder retains discovery capacity.
    let reserved = budget.div_ceil(5);
    let mut selected = select(true, reserved)?;
    selected.extend(select(false, budget - selected.len() as u32)?);
    Ok(selected)
}

// ponytail: do not bundle the separately validated progress payloads into a one-use context type.
#[allow(clippy::too_many_arguments, reason = "progress persistence requires scope, two payloads, and redaction secrets")]
pub fn persist_progress(conn:&Connection,org:&str,agent:&str,run:&str,attempt:&str,config:&Value,inventory:&[Value],plan:&[Value],secrets:&[String])->Result<()> {
    if inventory.len()>200||plan.len()>50{bail!("qa_progress_too_large")}
    let (project,application,environment)=scope(config);
    if application.len()>200||environment.len()>200||attempt.is_empty(){bail!("invalid_qa_scope")}
    let mut clean_inventory=Vec::with_capacity(inventory.len());
    for item in inventory {
        let mut item=item.clone();
        if !redact(&mut item,secrets) { bail!("qa_redaction_key_collision") }
        if contains_secret(&item,secrets) { bail!("qa_redaction_unverified") }
        let key=item.get("key").and_then(Value::as_str).filter(|key|canonical_scenario_key(key)).ok_or_else(||anyhow::anyhow!("invalid_qa_inventory_item"))?;
        let description=item.get("description").and_then(Value::as_str).filter(|value|!value.trim().is_empty()&&value.len()<=2000).ok_or_else(||anyhow::anyhow!("invalid_qa_inventory_item"))?;
        let module=item.get("module").and_then(Value::as_str).filter(|value|!value.trim().is_empty()&&value.len()<=200).unwrap_or_else(||key.split('-').next().unwrap_or("general"));
        let critical=item.get("critical").and_then(Value::as_bool).unwrap_or(false);
        let blocker=item.get("blocker_fingerprint").and_then(Value::as_str).filter(|value|!value.trim().is_empty()&&value.len()<=200).map(str::to_string);
        clean_inventory.push((key.to_string(),description.to_string(),module.to_string(),critical,blocker));
    }
    let mut clean_plan=Vec::with_capacity(plan.len());
    let mut planned=std::collections::HashSet::new();
    for item in plan {
        let mut item=item.clone();
        if !redact(&mut item,secrets) { bail!("qa_redaction_key_collision") }
        if contains_secret(&item,secrets) { bail!("qa_redaction_unverified") }
        let key=item.get("key").and_then(Value::as_str).filter(|key|canonical_scenario_key(key)).ok_or_else(||anyhow::anyhow!("invalid_qa_plan_item"))?.to_string();
        let description=item.get("description").and_then(Value::as_str).filter(|value|!value.trim().is_empty()&&value.len()<=2000).ok_or_else(||anyhow::anyhow!("invalid_qa_plan_item"))?.to_string();
        let reason=item.get("reason").and_then(Value::as_str).filter(|value|!value.trim().is_empty()&&value.len()<=1000).ok_or_else(||anyhow::anyhow!("invalid_qa_plan_item"))?.to_string();
        if !planned.insert(key.clone()){bail!("invalid_qa_plan_item")}
        clean_plan.push((key,description,reason));
    }
    conn.execute_batch("SAVEPOINT qa_progress_write")?;
    let write=(||->Result<()> {
        for (key,description,module,critical,blocker) in clean_inventory {
            conn.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,project_id,application,environment,module_key,scenario_key,description,critical,blocker_fingerprint) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT DO UPDATE SET module_key=excluded.module_key,description=excluded.description,critical=excluded.critical,blocker_fingerprint=excluded.blocker_fingerprint",params![org,agent,project,application,environment,module,key,description,critical,blocker])?;
        }
        conn.execute("UPDATE qa_batch_plans SET status='not_run' WHERE org_id=?1 AND run_id=?2 AND attempt_id=?3 AND status='pending'",params![org,run,attempt])?;
        for (key,description,reason) in clean_plan {
            conn.execute("INSERT INTO qa_batch_plans(org_id,agent_id,project_id,application,environment,run_id,attempt_id,scenario_key,description,reason,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending') ON CONFLICT(org_id,run_id,attempt_id,scenario_key) DO UPDATE SET description=excluded.description,reason=excluded.reason,status=COALESCE((SELECT status FROM qa_scenario_checkpoints checkpoint WHERE checkpoint.org_id=excluded.org_id AND checkpoint.run_id=excluded.run_id AND checkpoint.attempt_id=excluded.attempt_id AND checkpoint.scenario_key=excluded.scenario_key),'pending')",params![org,agent,project,application,environment,run,attempt,key,description,reason])?;
            conn.execute("UPDATE qa_scenario_inventory SET last_blocker_fingerprint=blocker_fingerprint WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5 AND scenario_key=?6",params![org,agent,project,application,environment,key])?;
        }
        Ok(())
    })();
    match write{Ok(())=>{conn.execute_batch("RELEASE qa_progress_write")?;Ok(())},Err(error)=>{let _=conn.execute_batch("ROLLBACK TO qa_progress_write; RELEASE qa_progress_write;");Err(error)}}
}

pub fn persist_batch_plan(conn: &Connection, org: &str, agent: &str, run: &str, attempt: &str, config: &Value, plan: &[Value]) -> Result<()> {
    let (project,application,environment) = scope(config);
    for item in plan.iter().take(50) {
        let key = item.get("key").and_then(Value::as_str).unwrap_or_default();
        let description = item.get("description").and_then(Value::as_str).unwrap_or_default();
        let reason = item.get("reason").and_then(Value::as_str).unwrap_or("selected by coverage rotation");
        conn.execute("INSERT OR IGNORE INTO qa_batch_plans(org_id,agent_id,project_id,application,environment,run_id,attempt_id,scenario_key,description,reason,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending')",params![org,agent,project,application,environment,run,attempt,key,description,reason])?;
        conn.execute("UPDATE qa_scenario_inventory SET last_blocker_fingerprint=blocker_fingerprint WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5 AND scenario_key=?6",params![org,agent,project,application,environment,key])?;
    }
    Ok(())
}

/// Validate executed QA and require every finding to cite an evidenced failed scenario.
pub fn validate_result(result: &Value) -> Result<()> {
    if let Some(plan) = result.get("plan") {
        let plan = plan.as_array().ok_or_else(|| anyhow::anyhow!("invalid_qa_plan"))?;
        if plan.len() > 50 { bail!("too_many_qa_plan_items") }
        let mut planned = std::collections::HashSet::new();
        for item in plan {
            let key = item.get("key").and_then(Value::as_str).unwrap_or_default();
            let description = item.get("description").and_then(Value::as_str).unwrap_or_default();
            let reason = item.get("reason").and_then(Value::as_str).unwrap_or_default();
            if !canonical_scenario_key(key) || description.trim().is_empty() || description.len() > 2000 || reason.trim().is_empty() || reason.len() > 1000 || !planned.insert(key) {
                bail!("invalid_qa_plan_item")
            }
        }
    }
    let scenarios = result
        .get("scenarios")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty())
        .ok_or_else(|| anyhow::anyhow!("qa_scenarios_missing"))?;
    if scenarios.len() > 108 {
        bail!("too_many_qa_scenarios")
    }
    let mut failed = std::collections::HashSet::new();
    let mut seen = std::collections::HashSet::new();
    for scenario in scenarios {
        let key = scenario.get("key").and_then(Value::as_str).unwrap_or_default();
        let description = scenario
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let status = scenario.get("status").and_then(Value::as_str).unwrap_or_default();
        let evidence = scenario
            .get("evidence")
            .and_then(Value::as_array)
            .filter(|items| !items.is_empty());
        if !canonical_scenario_key(key)
            || !seen.insert(key)
            || key.len() > 200
            || description.trim().is_empty()
            || description.len() > 2000
            || !matches!(status, "passed" | "failed" | "blocked")
            || !valid_blocker_fingerprint(scenario.get("blocker_fingerprint"))
            || !meaningful_evidence(evidence)
        {
            bail!("invalid_qa_scenario")
        }
        if status == "failed" {
            failed.insert(key);
        }
    }
    if let Some(inventory)=result.get("inventory").and_then(Value::as_array) {
        if inventory.len()>200 { bail!("too_many_qa_inventory_items") }
        let mut keys=HashSet::new();
        for item in inventory {
            let key=item.get("key").and_then(Value::as_str).unwrap_or_default();
            let description=item.get("description").and_then(Value::as_str).unwrap_or_default();
            if !canonical_scenario_key(key) || description.trim().is_empty() || description.len()>2000 || !keys.insert(key) || !valid_blocker_fingerprint(item.get("blocker_fingerprint")) { bail!("invalid_qa_inventory_item") }
            if item.get("module").and_then(Value::as_str).is_some_and(|module|module.trim().is_empty() || module.len()>200) { bail!("invalid_qa_inventory_item") }
        }
    }
    let findings = result
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("result_findings_missing"))?;
    if findings.len() > 108 {
        bail!("too_many_findings")
    }
    for finding in findings {
        let refs = finding
            .get("scenario_keys")
            .and_then(Value::as_array)
            .filter(|keys| !keys.is_empty())
            .ok_or_else(|| anyhow::anyhow!("qa_finding_scenario_missing"))?;
        if !meaningful_evidence(finding.get("evidence").and_then(Value::as_array)) {
            bail!("qa_finding_evidence_missing")
        }
        if refs
            .iter()
            .any(|key| key.as_str().is_none_or(|key| !failed.contains(key)))
        {
            bail!("qa_finding_scenario_not_failed")
        }
    }
    Ok(())
}

fn canonical_scenario_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 200 && key.as_bytes().first()!=Some(&b'-') && key.as_bytes().last()!=Some(&b'-')
        && key.bytes().all(|byte|byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte==b'-')
        && !key.contains("--")
}

fn valid_blocker_fingerprint(value: Option<&Value>) -> bool {
    value.is_none_or(Value::is_null) || value.and_then(Value::as_str).is_some_and(canonical_scenario_key)
}

fn meaningful_evidence(items: Option<&Vec<Value>>) -> bool {
    items.is_some_and(|items| {
        items
            .iter()
            .any(|item| item.as_str().is_some_and(|text| !text.trim().is_empty()))
    })
}

fn contains_secret(value: &Value, secrets: &[String]) -> bool {
    match value {
        Value::String(text) => secrets.iter().any(|secret| !secret.is_empty() && text.contains(secret)),
        Value::Array(values) => values.iter().any(|value| contains_secret(value, secrets)),
        Value::Object(values) => values.iter().any(|(key, value)| secrets.iter().any(|secret| !secret.is_empty() && key.contains(secret)) || contains_secret(value, secrets)),
        _ => false,
    }
}

fn redact(value: &mut Value, secrets: &[String]) -> bool {
    match value {
        Value::String(text) => {
            for secret in secrets.iter().filter(|secret| !secret.is_empty()) { *text = text.replace(secret, "[REDACTED]"); }
            let trimmed=text.trim();
            let candidate=trimmed.strip_prefix("```json").or_else(||trimmed.strip_prefix("```"))
                .and_then(|body|body.strip_suffix("```" )).map(str::trim).unwrap_or(trimmed);
            if let Ok(mut nested)=serde_json::from_str::<Value>(candidate) {
                if nested.is_object()||nested.is_array() {
                    if !redact(&mut nested,secrets) { return false; }
                    if let Ok(serialized)=serde_json::to_string(&nested){*text=serialized;}
                }
            }
            true
        }
        Value::Array(values) => {
            let mut success = true;
            for value in values {
                success &= redact(value, secrets);
            }
            success
        }
        Value::Object(values) => {
            let mut clean = serde_json::Map::new();
            for (key, mut value) in std::mem::take(values) {
                let mut key = key;
                for secret in secrets.iter().filter(|secret| !secret.is_empty()) { key = key.replace(secret, "[REDACTED]"); }
                if !redact(&mut value, secrets) || clean.insert(key, value).is_some() { return false; }
            }
            *values = clean;
            true
        }
        _ => true
    }
}

fn next_observation_order(conn: &Connection) -> Result<i64> {
    conn.execute("INSERT INTO qa_observation_order DEFAULT VALUES", [])?;
    Ok(conn.last_insert_rowid())
}

/// Persist one streamed scenario and its inventory/plan effects atomically.
// ponytail: retain explicit scope, scenario, and secret-redaction inputs for the atomic write.
#[allow(clippy::too_many_arguments, reason = "checkpoint persistence binds run scope and redaction secrets")]
pub fn persist_checkpoint(conn: &Connection, org: &str, agent: &str, run: &str, attempt: &str, config: &Value, scenario: &Value, secrets: &[String]) -> Result<bool> {
    let single = json!({"scenarios":[scenario],"findings":[]});
    validate_result(&single)?;
    let mut clean = scenario.clone();
    if !redact(&mut clean, secrets) { bail!("qa_redaction_key_collision") }
    if contains_secret(&clean, secrets) { bail!("qa_redaction_unverified") }
    let evidence_json = serde_json::to_string(&clean["evidence"])?;
    if evidence_json.len() > 16_000 { bail!("qa_evidence_too_large") }
    let blocker=clean.get("blocker_fingerprint").and_then(Value::as_str).filter(|value|!value.trim().is_empty() && value.len()<=200).map(str::to_owned);
    let (project, application, environment) = scope(config);
    if application.len() > 200 || environment.len() > 200 || attempt.is_empty() { bail!("invalid_qa_scope") }
    conn.execute_batch("SAVEPOINT qa_checkpoint_write")?;
    let write=(||->Result<bool>{
        conn.execute("INSERT OR IGNORE INTO qa_scenario_checkpoints(org_id,agent_id,project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,redaction_verified,blocker_fingerprint) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1,?12)", params![org,agent,project,application,environment,run,attempt,clean["key"].as_str(),clean["description"].as_str(),clean["status"].as_str(),evidence_json,blocker])?;
        let inserted=conn.changes()==1;
        if inserted {
            let order=next_observation_order(conn)?;
            conn.execute("UPDATE qa_scenario_checkpoints SET observation_order=?1 WHERE org_id=?2 AND run_id=?3 AND attempt_id=?4 AND scenario_key=?5",params![order,org,run,attempt,clean["key"].as_str()])?;
        }
        // Replays use the first fact's immutable content, but repair its derived state.
        let key:String=conn.query_row("SELECT scenario_key FROM qa_scenario_checkpoints WHERE org_id=?1 AND run_id=?2 AND attempt_id=?3 AND scenario_key=?4",params![org,run,attempt,clean["key"].as_str()],|row|row.get(0))?;
        let finalized:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM qa_scenario_results WHERE org_id=?1 AND run_id=?2 AND attempt_id=?3 AND scenario_key=?4 AND redaction_verified=1)",params![org,run,attempt,key],|row|row.get(0))?;
        let (description,status,blocker,tested_at,order):(String,String,Option<String>,String,i64)=conn.query_row("SELECT description,status,blocker_fingerprint,tested_at,observation_order FROM (SELECT description,status,blocker_fingerprint,tested_at,observation_order FROM qa_scenario_checkpoints WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5 AND scenario_key=?6 UNION ALL SELECT description,status,NULL,tested_at,observation_order FROM qa_scenario_results WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5 AND scenario_key=?6 AND redaction_verified=1) ORDER BY observation_order DESC LIMIT 1",params![org,agent,project,application,environment,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        conn.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,project_id,application,environment,module_key,scenario_key,description,last_status,blocker_fingerprint,last_blocker_fingerprint,last_tested_at,observation_order) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?12) ON CONFLICT DO UPDATE SET description=excluded.description,last_status=excluded.last_status,blocker_fingerprint=excluded.blocker_fingerprint,last_tested_at=excluded.last_tested_at,observation_order=excluded.observation_order WHERE COALESCE(qa_scenario_inventory.observation_order,0)<=excluded.observation_order", params![org,agent,project,application,environment,key.split('-').next().unwrap_or("general"),key,description,status,blocker,tested_at,order])?;
        if !finalized {
            conn.execute("UPDATE qa_batch_plans SET status=?1,blocker_fingerprint=?2 WHERE org_id=?3 AND run_id=?4 AND attempt_id=?5 AND scenario_key=?6 AND status='pending'",params![status,blocker,org,run,attempt,key])?;
        }
        Ok(inserted)
    })();
    match write {
        Ok(inserted)=>{conn.execute_batch("RELEASE qa_checkpoint_write")?;Ok(inserted)}
        Err(error)=>{let _=conn.execute_batch("ROLLBACK TO qa_checkpoint_write; RELEASE qa_checkpoint_write;");Err(error)}
    }
}

/// Persist only validated, executed outcomes atomically. Repeated finalization is idempotent.
// ponytail: retain explicit scope, result payload, and secret-redaction inputs for the atomic write.
#[allow(clippy::too_many_arguments, reason = "result persistence binds run scope and redaction secrets")]
pub fn persist_result(conn: &Connection, org: &str, agent: &str, run: &str, attempt: &str, config: &Value, result: &Value, secrets: &[String]) -> Result<usize> {
    validate_result(result)?;
    let scenarios = result.get("scenarios").and_then(Value::as_array).expect("validated scenarios");
    let (project, application, environment) = scope(config);
    if application.len() > 200 || environment.len() > 200 { bail!("invalid_qa_scope") }
    let mut rows = Vec::with_capacity(scenarios.len());
    for scenario in scenarios {
        let mut clean = scenario.clone();
        if !redact(&mut clean, secrets) { bail!("qa_redaction_key_collision") }
        if contains_secret(&clean, secrets) { bail!("qa_redaction_unverified") }
        let key = clean["key"].as_str().unwrap_or_default();
        let description = clean["description"].as_str().unwrap_or_default();
        let status = clean["status"].as_str().unwrap_or_default();
        let evidence_json = serde_json::to_string(&clean["evidence"])?;
        if evidence_json.len() > 16_000 { bail!("qa_evidence_too_large") }
        let blocker = clean.get("blocker_fingerprint").and_then(Value::as_str).filter(|value| value.len() <= 200).map(str::to_owned);
        rows.push((key.to_string(), description.to_string(), status.to_string(), evidence_json, blocker));
    }
    conn.execute_batch("SAVEPOINT qa_coverage_write")?;
    let write = (|| -> Result<usize> {
        let mut saved = 0;
        // The model's selected plan is authoritative for this attempt; retain the
        // pre-run candidates only as pending until this validated plan arrives.
        if let Some(plan) = result.get("plan").and_then(Value::as_array) {
            conn.execute("UPDATE qa_batch_plans SET status='not_run' WHERE org_id=?1 AND run_id=?2 AND attempt_id=?3 AND status='pending'", params![org,run,attempt])?;
            for item in plan.iter().take(50) {
                let mut clean = item.clone();
                if !redact(&mut clean, secrets) { bail!("qa_redaction_key_collision") }
                if contains_secret(&clean, secrets) { bail!("qa_redaction_unverified") }
                let key = clean["key"].as_str().unwrap_or_default();
                let description = clean["description"].as_str().unwrap_or_default();
                let reason = clean["reason"].as_str().unwrap_or_default();
                conn.execute("INSERT INTO qa_batch_plans(org_id,agent_id,project_id,application,environment,run_id,attempt_id,scenario_key,description,reason,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending') ON CONFLICT(org_id,run_id,attempt_id,scenario_key) DO UPDATE SET description=excluded.description,reason=excluded.reason,status=COALESCE((SELECT status FROM qa_scenario_checkpoints checkpoint WHERE checkpoint.org_id=excluded.org_id AND checkpoint.run_id=excluded.run_id AND checkpoint.attempt_id=excluded.attempt_id AND checkpoint.scenario_key=excluded.scenario_key),'pending')", params![org,agent,project,application,environment,run,attempt,key,description,reason])?;
            }
        }
        if let Some(inventory) = result.get("inventory").and_then(Value::as_array) {
            if inventory.len() > 200 { bail!("too_many_qa_inventory_items") }
            for item in inventory {
                let mut clean = item.clone();
                if !redact(&mut clean, secrets) { bail!("qa_redaction_key_collision") }
                if contains_secret(&clean, secrets) { bail!("qa_redaction_unverified") }
                let key = clean.get("key").and_then(Value::as_str).filter(|v| !v.trim().is_empty() && v.len() <= 200).ok_or_else(|| anyhow::anyhow!("invalid_qa_inventory_item"))?;
                let description = clean.get("description").and_then(Value::as_str).filter(|v| !v.trim().is_empty() && v.len() <= 2000).ok_or_else(|| anyhow::anyhow!("invalid_qa_inventory_item"))?;
                let module = clean.get("module").and_then(Value::as_str).filter(|v| !v.trim().is_empty() && v.len() <= 200).unwrap_or_else(|| key.split('-').next().unwrap_or("general"));
                let critical = clean.get("critical").and_then(Value::as_bool).unwrap_or(false);
                let blocker = clean.get("blocker_fingerprint").and_then(Value::as_str).filter(|v| v.len() <= 200);
                conn.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,project_id,application,environment,module_key,scenario_key,description,critical,blocker_fingerprint) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT DO UPDATE SET module_key=excluded.module_key,description=excluded.description,critical=excluded.critical,blocker_fingerprint=excluded.blocker_fingerprint", params![org,agent,project,application,environment,module,key,description,critical,blocker])?;
            }
        }
        if let Some(plan)=result.get("plan").and_then(Value::as_array) {
            for item in plan.iter().take(50) {
                if let Some(key)=item.get("key").and_then(Value::as_str) {
                    conn.execute("UPDATE qa_scenario_inventory SET last_blocker_fingerprint=blocker_fingerprint WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5 AND scenario_key=?6",params![org,agent,project,application,environment,key])?;
                }
            }
        }
        for (key, description, status, evidence_json, _blocker) in &rows {
            saved += conn.execute("INSERT INTO qa_scenario_results(org_id,agent_id,project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,redaction_verified)
              VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1)
              ON CONFLICT(org_id,run_id,attempt_id,scenario_key) DO UPDATE SET description=excluded.description,status=excluded.status,evidence_json=excluded.evidence_json,redaction_verified=1",
              params![org,agent,project,application,environment,run,attempt,key,description,status,evidence_json])?;
            let order: Option<i64> = conn.query_row("SELECT observation_order FROM qa_scenario_results WHERE org_id=?1 AND run_id=?2 AND attempt_id=?3 AND scenario_key=?4",params![org,run,attempt,key],|row|row.get(0))?;
            if order.is_none() {
                let order=next_observation_order(conn)?;
                conn.execute("UPDATE qa_scenario_results SET observation_order=?1 WHERE org_id=?2 AND run_id=?3 AND attempt_id=?4 AND scenario_key=?5",params![order,org,run,attempt,key])?;
            }
            let (inventory_description,inventory_status,inventory_blocker,inventory_tested_at,inventory_order):(String,String,Option<String>,String,i64)=conn.query_row("SELECT description,status,blocker_fingerprint,tested_at,observation_order FROM (SELECT description,status,blocker_fingerprint,tested_at,observation_order FROM qa_scenario_checkpoints WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5 AND scenario_key=?6 AND redaction_verified=1 UNION ALL SELECT description,status,NULL,tested_at,observation_order FROM qa_scenario_results WHERE org_id=?1 AND agent_id=?2 AND project_id IS ?3 AND application=?4 AND environment=?5 AND scenario_key=?6 AND redaction_verified=1) ORDER BY observation_order DESC LIMIT 1",params![org,agent,project,application,environment,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
            let module = key.split('-').next().filter(|part| !part.is_empty()).unwrap_or("general");
            conn.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,project_id,application,environment,module_key,scenario_key,description,last_status,blocker_fingerprint,last_blocker_fingerprint,last_tested_at,observation_order)
              VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?12)
              ON CONFLICT DO UPDATE SET description=excluded.description,last_status=excluded.last_status,blocker_fingerprint=excluded.blocker_fingerprint,last_tested_at=excluded.last_tested_at,observation_order=excluded.observation_order WHERE COALESCE(qa_scenario_inventory.observation_order,0)<=excluded.observation_order",
              params![org,agent,project,application,environment,module,key,inventory_description,inventory_status,inventory_blocker,inventory_tested_at,inventory_order])?;
            conn.execute("UPDATE qa_batch_plans SET status=?1 WHERE org_id=?2 AND run_id=?3 AND attempt_id=?4 AND scenario_key=?5 AND status='pending'",params![status,org,run,attempt,key])?;
        }
        conn.execute("UPDATE qa_batch_plans SET status='not_run' WHERE org_id=?1 AND run_id=?2 AND attempt_id=?3 AND status='pending'",params![org,run,attempt])?;
        Ok(saved)
    })();
    match write {
        Ok(saved) => { conn.execute_batch("RELEASE qa_coverage_write")?; Ok(saved) }
        Err(error) => { let _ = conn.execute_batch("ROLLBACK TO qa_coverage_write; RELEASE qa_coverage_write;"); Err(error) }
    }
}

pub fn finalize_attempt_plan(conn:&Connection,org:&str,run:&str,attempt:&str)->Result<usize>{
    Ok(conn.execute("UPDATE qa_batch_plans SET status='not_run' WHERE org_id=?1 AND run_id=?2 AND attempt_id=?3 AND status='pending'",params![org,run,attempt])?)
}

// ponytail: keep pagination and optional QA scope filters explicit at this query boundary.
#[allow(clippy::too_many_arguments, reason = "the query accepts independent scope filters and pagination inputs")]
pub fn list_checkpoints_for_agent_page(conn:&Connection,org:&str,agent:&str,project:Option<&str>,application:Option<&str>,environment:Option<&str>,cursor:Option<&str>,limit:u32)->Result<(Vec<Value>,Option<String>)>{
    let cursor=cursor.map(|value|value.parse::<i64>().map_err(|_|anyhow::anyhow!("invalid_qa_cursor"))).transpose()?;
    let mut stmt=conn.prepare("SELECT id,project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,tested_at FROM qa_scenario_checkpoints checkpoint WHERE org_id=?1 AND agent_id=?2 AND redaction_verified=1 AND ((?3 IS NULL AND project_id IS NULL) OR project_id=?3) AND (?4 IS NULL OR application=?4) AND (?5 IS NULL OR environment=?5) AND (?6 IS NULL OR id<?6) AND NOT EXISTS(SELECT 1 FROM qa_scenario_results result WHERE result.org_id=checkpoint.org_id AND result.run_id=checkpoint.run_id AND result.attempt_id=checkpoint.attempt_id AND result.scenario_key=checkpoint.scenario_key AND result.redaction_verified=1) ORDER BY id DESC LIMIT ?7")?;
    let rows=stmt.query_map(params![org,agent,project,application,environment,cursor,limit.clamp(1,108)+1],|r|{
        let evidence:String=r.get(9)?;
        Ok((r.get::<_,i64>(0)?,json!({"project_id":r.get::<_,Option<String>>(1)?,"application":r.get::<_,String>(2)?,"environment":r.get::<_,String>(3)?,"run_id":r.get::<_,String>(4)?,"attempt_id":r.get::<_,String>(5)?,"key":r.get::<_,String>(6)?,"description":r.get::<_,String>(7)?,"status":r.get::<_,String>(8)?,"evidence":serde_json::from_str::<Value>(&evidence).unwrap_or_else(|_|json!([])),"tested_at":r.get::<_,String>(10)?,"checkpoint":true})))
    })?;
    let mut rows=rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let next=if rows.len()>limit.clamp(1,108) as usize {rows.pop();rows.last().map(|(id,_)|id.to_string())}else{None};
    Ok((rows.into_iter().map(|(_,row)|row).collect(),next))
}

pub fn inventory_summary_for_agent_scope(conn:&Connection,org:&str,agent:&str,project:Option<&str>,application:Option<&str>,environment:Option<&str>)->Result<Vec<Value>>{
    let mut stmt=conn.prepare("SELECT application,environment,COUNT(*),SUM(CASE WHEN last_status IS NULL THEN 1 ELSE 0 END),SUM(CASE WHEN last_status='passed' THEN 1 ELSE 0 END),SUM(CASE WHEN last_status='failed' THEN 1 ELSE 0 END),SUM(CASE WHEN last_status='blocked' THEN 1 ELSE 0 END),SUM(CASE WHEN last_status='blocked' AND blocker_fingerprint IS NOT last_blocker_fingerprint THEN 1 ELSE 0 END) FROM qa_scenario_inventory WHERE org_id=?1 AND agent_id=?2 AND ((?3 IS NULL AND project_id IS NULL) OR project_id=?3) AND (?4 IS NULL OR application=?4) AND (?5 IS NULL OR environment=?5) GROUP BY application,environment ORDER BY application,environment")?;
    let rows=stmt.query_map(params![org,agent,project,application,environment],|row|Ok(json!({"application":row.get::<_,String>(0)?,"environment":row.get::<_,String>(1)?,"total":row.get::<_,i64>(2)?,"never_tested":row.get::<_,i64>(3)?,"passed":row.get::<_,i64>(4)?,"failed":row.get::<_,i64>(5)?,"blocked":row.get::<_,i64>(6)?,"blocker_changed":row.get::<_,i64>(7)?})))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ponytail: keep pagination and optional QA scope filters explicit at this query boundary.
#[allow(clippy::too_many_arguments, reason = "the query accepts independent scope filters and pagination inputs")]
pub fn list_inventory_for_agent_page(conn:&Connection,org:&str,agent:&str,project:Option<&str>,application:Option<&str>,environment:Option<&str>,cursor:Option<&str>,limit:u32)->Result<(Vec<Value>,Option<String>)>{
    let cursor=cursor.map(|value|value.parse::<i64>().map_err(|_|anyhow::anyhow!("invalid_qa_cursor"))).transpose()?;
    let mut stmt=conn.prepare("SELECT id,project_id,application,environment,module_key,scenario_key,description,critical,last_status,last_tested_at,blocker_fingerprint,last_blocker_fingerprint FROM qa_scenario_inventory WHERE org_id=?1 AND agent_id=?2 AND ((?3 IS NULL AND project_id IS NULL) OR project_id=?3) AND (?4 IS NULL OR application=?4) AND (?5 IS NULL OR environment=?5) AND (?6 IS NULL OR id>?6) ORDER BY id LIMIT ?7")?;
    let rows=stmt.query_map(params![org,agent,project,application,environment,cursor,limit.clamp(1,108)+1],|r|Ok((r.get::<_,i64>(0)?,json!({"project_id":r.get::<_,Option<String>>(1)?,"application":r.get::<_,String>(2)?,"environment":r.get::<_,String>(3)?,"module":r.get::<_,String>(4)?,"key":r.get::<_,String>(5)?,"description":r.get::<_,String>(6)?,"critical":r.get::<_,bool>(7)?,"last_status":r.get::<_,Option<String>>(8)?,"last_tested_at":r.get::<_,Option<String>>(9)?,"blocker_changed":r.get::<_,Option<String>>(10)?!=r.get::<_,Option<String>>(11)?}))))?;
    let mut rows=rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let next=if rows.len()>limit.clamp(1,108) as usize {rows.pop();rows.last().map(|(id,_)|id.to_string())}else{None};
    Ok((rows.into_iter().map(|(_,row)|row).collect(),next))
}

// ponytail: keep pagination and optional QA scope filters explicit at this query boundary.
#[allow(clippy::too_many_arguments, reason = "the query accepts independent scope filters and pagination inputs")]
pub fn list_plans_for_agent_page(conn:&Connection,org:&str,agent:&str,project:Option<&str>,application:Option<&str>,environment:Option<&str>,cursor:Option<&str>,limit:u32)->Result<(Vec<Value>,Option<String>)>{
    let cursor=cursor.map(|value|value.parse::<i64>().map_err(|_|anyhow::anyhow!("invalid_qa_cursor"))).transpose()?;
    let mut stmt=conn.prepare("SELECT id,run_id,attempt_id,scenario_key,description,reason,status,planned_at FROM qa_batch_plans WHERE org_id=?1 AND agent_id=?2 AND ((?3 IS NULL AND project_id IS NULL) OR project_id=?3) AND (?4 IS NULL OR application=?4) AND (?5 IS NULL OR environment=?5) AND (?6 IS NULL OR id<?6) ORDER BY id DESC LIMIT ?7")?;
    let rows=stmt.query_map(params![org,agent,project,application,environment,cursor,limit.clamp(1,108)+1],|r|Ok((r.get::<_,i64>(0)?,json!({"run_id":r.get::<_,String>(1)?,"attempt_id":r.get::<_,String>(2)?,"key":r.get::<_,String>(3)?,"description":r.get::<_,String>(4)?,"reason":r.get::<_,String>(5)?,"status":r.get::<_,String>(6)?,"planned_at":r.get::<_,String>(7)?}))))?;
    let mut rows=rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let next=if rows.len()>limit.clamp(1,108) as usize {rows.pop();rows.last().map(|(id,_)|id.to_string())}else{None};
    Ok((rows.into_iter().map(|(_,row)|row).collect(),next))
}

pub fn list_checkpoints_for_agent(conn: &Connection, org: &str, agent: &str, project: Option<&str>, application: Option<&str>, environment: Option<&str>) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,tested_at FROM qa_scenario_checkpoints WHERE org_id=?1 AND agent_id=?2 AND redaction_verified=1 AND ((?3 IS NULL AND project_id IS NULL) OR project_id=?3) AND (?4 IS NULL OR application=?4) AND (?5 IS NULL OR environment=?5) ORDER BY tested_at DESC,id DESC LIMIT 108")?;
    let rows = stmt.query_map(params![org,agent,project,application,environment], |r| {
        let evidence: String = r.get(8)?;
        Ok(json!({"project_id":r.get::<_,Option<String>>(0)?,"application":r.get::<_,String>(1)?,"environment":r.get::<_,String>(2)?,"run_id":r.get::<_,String>(3)?,"attempt_id":r.get::<_,String>(4)?,"key":r.get::<_,String>(5)?,"description":r.get::<_,String>(6)?,"status":r.get::<_,String>(7)?,"evidence":serde_json::from_str::<Value>(&evidence).unwrap_or_else(|_|json!([])),"tested_at":r.get::<_,String>(9)?}))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_plans_for_run(conn:&Connection,org:&str,run:&str)->Result<Vec<Value>>{
    let mut stmt=conn.prepare("SELECT attempt_id,scenario_key,description,reason,status,planned_at FROM qa_batch_plans WHERE org_id=?1 AND run_id=?2 ORDER BY id")?;
    let rows=stmt.query_map(params![org,run],|row|Ok(json!({"attempt_id":row.get::<_,String>(0)?,"key":row.get::<_,String>(1)?,"description":row.get::<_,String>(2)?,"reason":row.get::<_,String>(3)?,"status":row.get::<_,String>(4)?,"planned_at":row.get::<_,String>(5)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn list_for_run(conn: &Connection, org: &str, run: &str) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,tested_at FROM qa_scenario_results WHERE org_id=?1 AND run_id=?2 AND redaction_verified=1 ORDER BY id")?;
    let rows = stmt.query_map(params![org,run], |r| {
        let evidence: String = r.get(8)?;
        Ok(json!({"project_id":r.get::<_,Option<String>>(0)?,"application":r.get::<_,String>(1)?,"environment":r.get::<_,String>(2)?,"run_id":r.get::<_,String>(3)?,"attempt_id":r.get::<_,String>(4)?,"key":r.get::<_,String>(5)?,"description":r.get::<_,String>(6)?,"status":r.get::<_,String>(7)?,"evidence":serde_json::from_str::<Value>(&evidence).unwrap_or_else(|_|json!([])),"tested_at":r.get::<_,String>(9)?}))
    })?;
    let mut results = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let completed:HashSet<(String,String)>=results.iter().filter_map(|row|Some((row["attempt_id"].as_str()?.to_owned(),row["key"].as_str()?.to_owned()))).collect();
    let mut stmt = conn.prepare("SELECT project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,tested_at FROM qa_scenario_checkpoints WHERE org_id=?1 AND run_id=?2 AND redaction_verified=1 ORDER BY id")?;
    let checkpoints = stmt.query_map(params![org,run], |r| {
        let evidence: String = r.get(8)?;
        Ok(json!({"project_id":r.get::<_,Option<String>>(0)?,"application":r.get::<_,String>(1)?,"environment":r.get::<_,String>(2)?,"run_id":r.get::<_,String>(3)?,"attempt_id":r.get::<_,String>(4)?,"key":r.get::<_,String>(5)?,"description":r.get::<_,String>(6)?,"status":r.get::<_,String>(7)?,"evidence":serde_json::from_str::<Value>(&evidence).unwrap_or_else(|_|json!([])),"tested_at":r.get::<_,String>(9)?,"checkpoint":true}))
    })?;
    results.extend(checkpoints.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter(|row|!completed.contains(&(row["attempt_id"].as_str().unwrap_or_default().to_owned(),row["key"].as_str().unwrap_or_default().to_owned()))));
    Ok(results)
}

// ponytail: keep pagination and optional QA scope filters explicit at this query boundary.
#[allow(clippy::too_many_arguments, reason = "the query accepts independent scope filters and pagination inputs")]
pub fn list_for_agent_page(conn: &Connection, org: &str, agent: &str, project: Option<&str>, application: Option<&str>, environment: Option<&str>, cursor: Option<&str>, limit: u32) -> Result<(Vec<Value>, Option<String>)> {
    let parsed = cursor.map(|value| value.rsplit_once(':').ok_or_else(|| anyhow::anyhow!("invalid_qa_cursor"))).transpose()?;
    let (cursor_time, cursor_id) = match parsed {
        Some((time, id)) => (Some(time), Some(id.parse::<i64>().map_err(|_| anyhow::anyhow!("invalid_qa_cursor"))?)),
        None => (None, None),
    };
    let mut stmt = conn.prepare("SELECT id,project_id,application,environment,run_id,attempt_id,scenario_key,description,status,evidence_json,tested_at FROM qa_scenario_results
      WHERE org_id=?1 AND agent_id=?2 AND redaction_verified=1 AND ((?3 IS NULL AND project_id IS NULL) OR project_id=?3) AND (?4 IS NULL OR application=?4) AND (?5 IS NULL OR environment=?5)
      AND (?6 IS NULL OR tested_at<?6 OR (tested_at=?6 AND id<?7)) ORDER BY tested_at DESC,id DESC LIMIT ?8")?;
    let rows = stmt.query_map(params![org,agent,project,application,environment,cursor_time,cursor_id,limit.clamp(1,108)+1], |r| {
        let evidence: String = r.get(9)?;
        Ok((r.get::<_,i64>(0)?, json!({"project_id":r.get::<_,Option<String>>(1)?,"application":r.get::<_,String>(2)?,"environment":r.get::<_,String>(3)?,"run_id":r.get::<_,String>(4)?,"attempt_id":r.get::<_,String>(5)?,"key":r.get::<_,String>(6)?,"description":r.get::<_,String>(7)?,"status":r.get::<_,String>(8)?,"evidence":serde_json::from_str::<Value>(&evidence).unwrap_or_else(|_|json!([])),"tested_at":r.get::<_,String>(10)?})))
    })?;
    let mut rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let next = if rows.len() > limit.clamp(1,108) as usize {
        rows.pop();
        rows.last().map(|(id,row)| format!("{}:{id}",row["tested_at"].as_str().unwrap_or_default()))
    } else { None };
    Ok((rows.into_iter().map(|(_,row)| row).collect(), next))
}

pub fn list_for_agent(conn: &Connection, org: &str, agent: &str, limit: u32) -> Result<Vec<Value>> {
    Ok(list_for_agent_page(conn,org,agent,None,None,None,None,limit)?.0)
}

pub fn list_plans_for_agent(conn: &Connection, org: &str, agent: &str, project: Option<&str>, application: Option<&str>, environment: Option<&str>) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT run_id,attempt_id,scenario_key,description,reason,status,planned_at FROM qa_batch_plans WHERE org_id=?1 AND agent_id=?2 AND ((?3 IS NULL AND project_id IS NULL) OR project_id=?3) AND (?4 IS NULL OR application=?4) AND (?5 IS NULL OR environment=?5) ORDER BY planned_at DESC,id DESC LIMIT 108")?;
    let rows = stmt.query_map(params![org,agent,project,application,environment], |r| Ok(json!({"run_id":r.get::<_,String>(0)?,"attempt_id":r.get::<_,String>(1)?,"key":r.get::<_,String>(2)?,"description":r.get::<_,String>(3)?,"reason":r.get::<_,String>(4)?,"status":r.get::<_,String>(5)?,"planned_at":r.get::<_,String>(6)?})))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_grants_are_link_scoped_and_snapshots_immutable() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); CREATE TABLE projects(id TEXT PRIMARY KEY); CREATE TABLE code_projects(id INTEGER PRIMARY KEY,org_id TEXT,project_id TEXT,repo_url TEXT,archived_at TEXT);").unwrap();
        c.execute("INSERT INTO organizations VALUES('o')",[]).unwrap();
        c.execute("INSERT INTO projects VALUES('p')",[]).unwrap();
        c.execute("INSERT INTO code_projects VALUES(1,'o','p','acme/web',NULL)",[]).unwrap();
        migrate(&c).unwrap();
        set_repository_grant(&c,"o","p","acme/web",true,true,"bot","admin").unwrap();
        c.execute("INSERT INTO code_projects VALUES(2,'o','p','acme/api',NULL)",[]).unwrap();
        set_repository_grant(&c,"o","p","acme/api",true,false,"bot","admin").unwrap();
        let grants=list_repository_grants(&c,"o","p").unwrap();
        assert_eq!(grants.len(),2);
        create_repository_snapshot(&c,"o","p","r","rev",&grants).unwrap();
        assert_eq!(snapshot_repositories(&c,"o","p","r",false).unwrap().len(),2);
        assert_eq!(snapshot_repositories(&c,"o","p","r",true).unwrap().iter().map(|g|g.repository.as_str()).collect::<Vec<_>>(),vec!["acme/web"]);
        assert!(snapshot_grant(&c,"o","p","r","acme/web",true).unwrap().is_some());
        revoke_repository_grant(&c,"o","p","acme/web").unwrap();
        let live=list_repository_grants(&c,"o","p").unwrap();
        assert_eq!(live.len(),1);
        assert_eq!(live[0].repository,"acme/api");
        assert!(create_repository_snapshot(&c,"o","p","r","rev",&[]).is_err());
        assert!(set_repository_grant(&c,"o","p","other/repo",true,false,"bot","admin").is_err());
    }

    #[test]
    fn coverage_scope_tracks_project_target_origin_and_environment() {
        let config = json!({
            "project_id":"p",
            "targets":[{"kind":"web_application","enabled":true,"name":"Store","config":{"url":"https://user:secret@shop.example/path?token=hidden","environment":"staging"}}]
        });
        let (project, application, environment) = scope(&config);
        assert_eq!(project, Some("p"));
        assert_eq!(application, "Store · https://shop.example/path");
        let backoffice = json!({"application":"Store","targets":[{"kind":"web_application","enabled":true,"name":"Store","config":{"url":"https://shop.example/backoffice"}}]});
        assert_ne!(application, scope(&backoffice).1);
        assert_eq!(environment, "staging");
    }

    #[test]
    fn checkpoint_fact_inventory_and_plan_commit_atomically_and_replay_repairs_derived_rows() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"project_id":"p","application":"web","environment":"staging"});
        let scenario=json!({"key":"checkout-login","description":"login is blocked","status":"blocked","evidence":["login service unavailable"],"blocker_fingerprint":"auth-service-down"});
        persist_batch_plan(&c,"o","a","r","attempt-1",&cfg,&[json!({"key":"checkout-login","description":"login","reason":"critical"})]).unwrap();
        c.execute_batch("CREATE TRIGGER reject_checkpoint_inventory BEFORE INSERT ON qa_scenario_inventory WHEN NEW.scenario_key='checkout-login' BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&scenario,&[]).is_err());
        assert_eq!(c.query_row("SELECT count(*) FROM qa_scenario_checkpoints WHERE run_id='r'",[],|row|row.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(c.query_row("SELECT status FROM qa_batch_plans WHERE run_id='r'",[],|row|row.get::<_,String>(0)).unwrap(),"pending");
        c.execute_batch("DROP TRIGGER reject_checkpoint_inventory;").unwrap();
        assert!(persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&scenario,&[]).unwrap());
        assert!(!persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&json!({"key":"checkout-login","description":"replayed mutable wording","status":"passed","evidence":["other"]}),&[]).unwrap());
        c.execute("DELETE FROM qa_scenario_inventory WHERE scenario_key='checkout-login'",[]).unwrap();
        c.execute("UPDATE qa_batch_plans SET status='pending' WHERE run_id='r'",[]).unwrap();
        assert!(!persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&json!({"key":"checkout-login","description":"replayed mutable wording","status":"passed","evidence":["other"]}),&[]).unwrap());
        let (status,blocker):(String,Option<String>)=c.query_row("SELECT status,blocker_fingerprint FROM qa_scenario_checkpoints WHERE run_id='r'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!((status.as_str(),blocker.as_deref()),("blocked",Some("auth-service-down")));
        let (description,last_status,last_blocker):(String,String,Option<String>)=c.query_row("SELECT description,last_status,last_blocker_fingerprint FROM qa_scenario_inventory WHERE scenario_key='checkout-login'",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!((description.as_str(),last_status.as_str(),last_blocker.as_deref()),("login is blocked","blocked",Some("auth-service-down")));
        assert_eq!(c.query_row("SELECT status FROM qa_batch_plans WHERE run_id='r'",[],|row|row.get::<_,String>(0)).unwrap(),"blocked");
    }

    #[test]
    fn target_identity_includes_base_path_even_with_explicit_application_name() {
        let store = json!({"application":"Portal","targets":[{"kind":"web_application","enabled":true,"config":{"url":"https://example.com/app"}}]});
        let admin = json!({"application":"Portal","targets":[{"kind":"web_application","enabled":true,"config":{"url":"https://example.com/admin"}}]});
        assert_ne!(scope(&store).1, scope(&admin).1);
    }

    #[test]
    fn persists_idempotently_and_keeps_blocked_separate() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY);") .unwrap();
        c.execute("INSERT INTO organizations VALUES('o')",[]).unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"project_id":"p","application":"web","environment":"staging"});
        let result = json!({"findings":[],"scenarios":[
            {"key":"checkout-submit","description":"Submit checkout","status":"blocked","evidence":["login unavailable"]},
            {"key":"catalog-search","description":"Search catalog","status":"passed","evidence":["results shown"]},
            {"key":"cart-total","description":"Check cart total","status":"failed","evidence":["mismatch"]}
        ]});
        assert_eq!(persist_result(&c, "o", "a", "r", "attempt-1", &cfg, &result, &[]).unwrap(), 3);
        assert_eq!(persist_result(&c, "o", "a", "r", "attempt-1", &cfg, &result, &[]).unwrap(), 3);
        let rows = list_for_agent_page(&c, "o", "a", Some("p"), None, None, None, 10).unwrap().0;
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|row| row["key"] == "checkout-submit" && row["status"] == "blocked"));
        assert!(rows.iter().any(|row| row["key"] == "catalog-search" && row["status"] == "passed"));
        assert!(rows.iter().all(|row| row["project_id"] == "p"));
        let history = context_for_scope(&c, "o", "a", &cfg, 10).unwrap();
        assert_eq!(history.iter().map(|row| row["key"].as_str().unwrap()).collect::<Vec<_>>(), vec!["cart-total", "catalog-search"]);
        assert!(context_for_scope(&c, "o", "a", &json!({"project_id":"other","application":"web","environment":"staging"}), 10).unwrap().is_empty());
        let unexecuted = json!({"scenarios":[{"key":"not-run","description":"Not attempted","status":"planned"}]});
        assert!(persist_result(&c, "o", "a", "r2", "attempt-1", &cfg, &unexecuted, &[]).is_err());
        assert!(validate_result(&json!({"findings":[],"scenarios":[{"key":"Cart Total","description":"same behavior","status":"passed","evidence":["observed"]}]})).is_err());
        let duplicate = json!({"findings":[{"scenario_keys":["login"],"evidence":["observed failure"]}],"scenarios":[
            {"key":"login","description":"failed login","status":"failed","evidence":["error"]},
            {"key":"login","description":"passed login","status":"passed","evidence":["success"]}
        ]});
        assert_eq!(validate_result(&duplicate).unwrap_err().to_string(), "invalid_qa_scenario");
        let invalid_batch = json!({"findings":[],"scenarios":[
            {"key":"atomic-good","description":"first","status":"passed","evidence":["observed"]},
            {"key":"","description":"second","status":"passed","evidence":["observed"]}
        ]});
        assert!(persist_result(&c, "o", "a", "r3", "attempt-1", &cfg, &invalid_batch, &[]).is_err());
        assert!(!list_for_agent_page(&c, "o", "a", Some("p"), None, None, None, 20).unwrap().0.iter().any(|row| row["run_id"] == "r3"));
        c.execute_batch("CREATE TRIGGER reject_second_qa BEFORE INSERT ON qa_scenario_results WHEN NEW.scenario_key='sql-second' BEGIN SELECT RAISE(ABORT,'injected write failure'); END;").unwrap();
        let sql_failure = json!({"findings":[],"scenarios":[
            {"key":"sql-first","description":"first","status":"passed","evidence":["observed"]},
            {"key":"sql-second","description":"second","status":"passed","evidence":["observed"]}
        ]});
        assert!(persist_result(&c,"o","a","sql-fail","attempt-1",&cfg,&sql_failure,&[]).is_err());
        assert!(!list_for_agent_page(&c,"o","a",Some("p"),None,None,None,20).unwrap().0.iter().any(|row| row["run_id"] == "sql-fail"));
        let secret = "known-secret-value".to_string();
        let secret_result = json!({"findings":[],"scenarios":[{"key":"secret","description":"page","status":"passed","evidence":["value known-secret-value"]}]});
        persist_result(&c, "o", "a", "r4", "attempt-1", &cfg, &secret_result, &[secret]).unwrap();
        assert!(!serde_json::to_string(&list_for_run(&c, "o", "r4").unwrap()).unwrap().contains("known-secret-value"));
        assert!(!serde_json::to_string(&list_for_agent_page(&c, "o", "a", Some("p"), None, None, None, 20).unwrap().0).unwrap().contains("known-secret-value"));
        let short_secret = "pw";
        let short_result = json!({"findings":[],"scenarios":[{"key":"short-secret","description":"page","status":"passed","evidence":["credential pw hidden"]}]});
        persist_result(&c, "o", "a", "r5", "attempt-1", &cfg, &short_result, &[short_secret.to_string()]).unwrap();
        assert!(!serde_json::to_string(&list_for_run(&c, "o", "r5").unwrap()).unwrap().contains("pw"));
        c.execute("UPDATE qa_scenario_results SET redaction_verified=0 WHERE run_id='r4'", []).unwrap();
        assert!(list_for_run(&c, "o", "r4").unwrap().is_empty());
        assert!(!list_for_agent_page(&c, "o", "a", Some("p"), None, None, None, 20).unwrap().0.iter().any(|row| row["run_id"] == "r4"));
    }

    #[test]
    fn streamed_checkpoints_are_validated_idempotent_attempt_facts_and_survive_run_cleanup() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o'); CREATE TABLE autonomous_agent_runs(id TEXT PRIMARY KEY);").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"project_id":"p","application":"web","environment":"staging"});
        let scenario=json!({"key":"products-list","description":"List products","status":"passed","evidence":["loaded known-secret"]});
        assert!(persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&scenario,&["known-secret".into()]).unwrap());
        assert!(!persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&scenario,&["known-secret".into()]).unwrap());
        assert!(persist_checkpoint(&c,"o","a","r","attempt-2",&cfg,&scenario,&["known-secret".into()]).unwrap());
        let conflicting_replay=json!({"key":"products-list","description":"changed replay","status":"failed","evidence":["different content"]});
        assert!(!persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&conflicting_replay,&[]).unwrap());
        let first: String=c.query_row("SELECT description FROM qa_scenario_checkpoints WHERE run_id='r' AND attempt_id='attempt-1'",[],|r|r.get(0)).unwrap();
        assert_eq!(first,"List products");
        let encoded=serde_json::to_string(&list_checkpoints_for_agent(&c,"o","a",Some("p"),None,None).unwrap()).unwrap();
        assert!(!encoded.contains("known-secret"));
        assert_eq!(encoded.matches("products-list").count(),2);
        c.execute("INSERT INTO autonomous_agent_runs VALUES('r')",[]).unwrap();
        c.execute("DELETE FROM autonomous_agent_runs WHERE id='r'",[]).unwrap();
        assert_eq!(list_for_run(&c,"o","r").unwrap().len(),2);
    }

    #[test]
    fn redaction_key_collision_rejects_qa_progress_checkpoints_and_results() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"sandbox"});
        let canary="pod-only-canary-1028";
        let nested=format!(r#"[{{"[REDACTED]":"prior","\u0070od-only-canary-1028":"collision"}},"{canary}"]"#);
        let inventory=[json!({"key":"collision-case","description":nested,"module":"auth"})];
        let plan=[json!({"key":"collision-case","description":"Check redaction","reason":"known collision"})];
        assert_eq!(persist_progress(&c,"o","a","progress-run","attempt-1",&cfg,&inventory,&plan,&[canary.to_string()]).unwrap_err().to_string(),"qa_redaction_key_collision");
        let scenario=json!({"key":"collision-case","description":"Check redaction","status":"passed","evidence":[nested,canary]});
        assert_eq!(persist_checkpoint(&c,"o","a","checkpoint-run","attempt-1",&cfg,&scenario,&[canary.to_string()]).unwrap_err().to_string(),"qa_redaction_key_collision");
        let result=json!({"findings":[],"scenarios":[scenario],"plan":plan,"inventory":inventory});
        assert_eq!(persist_result(&c,"o","a","result-run","attempt-1",&cfg,&result,&[canary.to_string()]).unwrap_err().to_string(),"qa_redaction_key_collision");
        for table in ["qa_scenario_checkpoints","qa_scenario_results","qa_scenario_inventory","qa_batch_plans"] {
            let count:i64=c.query_row(&format!("SELECT count(*) FROM {table}"),[],|row|row.get(0)).unwrap();
            assert_eq!(count,0,"{table} must not retain partial content");
        }
    }

    #[test]
    fn sandbox_secret_canary_is_redacted_from_every_durable_qa_copy() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"sandbox"});
        let secret="sandbox-canary-rotate-9182";
        let result=json!({
            "findings":[],
            "plan":[{"key":"canary-case","description":format!("Check {secret}"),"reason":format!("selected {secret}")}],
            "inventory":[{"module":"auth","key":"canary-case","description":format!("Found {secret}")}],
            "scenarios":[{"key":"canary-case","description":format!("Ran {secret}"),"status":"passed","evidence":[format!("Observed {secret}")]}]
        });
        persist_result(&c,"o","a","sandbox-run","sandbox-attempt",&cfg,&result,&[secret.to_string()]).unwrap();
        let results: String=c.query_row("SELECT group_concat(description || ':' || evidence_json) FROM qa_scenario_results",[],|r|r.get(0)).unwrap();
        let inventory: String=c.query_row("SELECT group_concat(description) FROM qa_scenario_inventory",[],|r|r.get(0)).unwrap();
        let plans: String=c.query_row("SELECT group_concat(description || ':' || reason) FROM qa_batch_plans",[],|r|r.get(0)).unwrap();
        assert!(!results.contains(secret));
        assert!(!inventory.contains(secret));
        assert!(!plans.contains(secret));
        let serialized=serde_json::to_string(&list_for_run(&c,"o","sandbox-run").unwrap()).unwrap();
        assert!(serialized.contains("[REDACTED]"));
        assert!(!serialized.contains(secret));
        let escaped=json!({"key":"nested-canary","description":"Nested canary","status":"failed","evidence":[r#"{"echo":"\u0073andbox-canary-rotate-9182"}"#]});
        persist_checkpoint(&c,"o","a","sandbox-checkpoint","attempt-1",&cfg,&escaped,&[secret.to_string()]).unwrap();
        let checkpoint:String=c.query_row("SELECT evidence_json FROM qa_scenario_checkpoints WHERE run_id='sandbox-checkpoint'",[],|row|row.get(0)).unwrap();
        assert!(!checkpoint.contains(secret));
        assert!(checkpoint.contains("[REDACTED]"));
        let nested=r#"{"echo":"\u0073andbox-canary-rotate-9182"}"#;
        persist_progress(&c,"o","a","progress-run","progress-attempt",&cfg,&[json!({"key":"progress-nested","description":nested,"module":"auth"})],&[json!({"key":"progress-nested","description":"Recheck","reason":nested})],&[secret.to_string()]).unwrap();
        let progress:String=c.query_row("SELECT group_concat(description || ':' || reason) FROM qa_batch_plans WHERE run_id='progress-run'",[],|row|row.get(0)).unwrap();
        assert!(!progress.contains(secret));
        assert!(progress.contains("[REDACTED]"));
        let old=json!({"key":"inventory-replay","description":"Old checkpoint","status":"passed","evidence":["old"]});
        persist_checkpoint(&c,"o","a","old-run","old-attempt",&cfg,&old,&[]).unwrap();
        c.execute("UPDATE qa_scenario_checkpoints SET tested_at='2020-01-01 00:00:00' WHERE run_id='old-run'",[]).unwrap();
        let newer=json!({"findings":[],"plan":[],"inventory":[],"scenarios":[{"key":"inventory-replay","description":"New result","status":"failed","evidence":["new"]}]});
        persist_result(&c,"o","a","new-run","new-attempt",&cfg,&newer,&[]).unwrap();
        let newer_time:String=c.query_row("SELECT last_tested_at FROM qa_scenario_inventory WHERE scenario_key='inventory-replay'",[],|row|row.get(0)).unwrap();
        assert!(!persist_checkpoint(&c,"o","a","old-run","old-attempt",&cfg,&old,&[]).unwrap());
        let current:(String,String)=c.query_row("SELECT last_status,last_tested_at FROM qa_scenario_inventory WHERE scenario_key='inventory-replay'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(current, ("failed".to_owned(), newer_time));
    }

    #[test]
    fn effective_plan_and_completion_are_scoped_to_attempt() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"staging"});
        let candidates=vec![json!({"key":"products-list","description":"Browse products","reason":"untested"}),json!({"key":"sales-create","description":"Create sale","reason":"untested"})];
        persist_batch_plan(&c,"o","a","r","attempt-1",&cfg,&candidates).unwrap();
        persist_batch_plan(&c,"o","a","r","attempt-2",&cfg,&candidates).unwrap();
        let actual=json!({"findings":[],"inventory":[],"plan":[{"key":"sales-create","description":"Create a sale","reason":"selected: products passed"}],"scenarios":[{"key":"sales-create","description":"Create a sale","status":"passed","evidence":["sale appeared"]}]});
        persist_result(&c,"o","a","r","attempt-1",&cfg,&actual,&[]).unwrap();
        let plans=list_plans_for_agent(&c,"o","a",None,None,None).unwrap();
        assert!(plans.iter().any(|row| row["attempt_id"]=="attempt-1" && row["key"]=="sales-create" && row["status"]=="passed" && row["reason"]=="selected: products passed"));
        assert!(plans.iter().any(|row| row["attempt_id"]=="attempt-1" && row["key"]=="products-list" && row["status"]=="not_run"));
        assert!(plans.iter().any(|row| row["attempt_id"]=="attempt-2" && row["key"]=="sales-create" && row["status"]=="pending"));
        assert!(validate_result(&json!({"plan":[{"key":"x","description":"dup","reason":"a"},{"key":"x","description":"dup","reason":"b"}],"scenarios":[{"key":"x","description":"x","status":"passed","evidence":["ok"]}]})).is_err());
    }

    #[test]
    fn final_outcomes_keep_attempt_and_original_target_scope() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let original=json!({"project_id":"p","application":"store","environment":"prod"});
        let retry=json!({"project_id":"other","application":"backoffice","environment":"staging"});
        let failed=json!({"findings":[],"scenarios":[{"key":"checkout-submit","description":"Checkout","status":"failed","evidence":["error shown"]}]});
        let passed=json!({"findings":[],"scenarios":[{"key":"checkout-submit","description":"Checkout","status":"passed","evidence":["completed"]}]});
        persist_result(&c,"o","a","same-run","attempt-1",&original,&failed,&[]).unwrap();
        persist_result(&c,"o","a","same-run","attempt-2",&retry,&passed,&[]).unwrap();
        let rows=list_for_run(&c,"o","same-run").unwrap();
        assert_eq!(rows.len(),2);
        assert!(rows.iter().any(|r|r["attempt_id"]=="attempt-1" && r["project_id"]=="p" && r["application"]=="store" && r["status"]=="failed"));
        assert!(rows.iter().any(|r|r["attempt_id"]=="attempt-2" && r["project_id"]=="other" && r["application"]=="backoffice" && r["status"]=="passed"));
    }

    #[test]
    fn replayed_checkpoint_cannot_overwrite_final_inventory(){
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"staging"});
        let checkpoint=json!({"key":"cart-total","description":"Cart total","status":"passed","evidence":["12 shown"]});
        persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&checkpoint,&[]).unwrap();
        let final_result=json!({"findings":[],"scenarios":[{"key":"cart-total","description":"Cart total","status":"failed","evidence":["14 shown"]}]});
        persist_result(&c,"o","a","r","attempt-1",&cfg,&final_result,&[]).unwrap();
        assert!(!persist_checkpoint(&c,"o","a","r","attempt-1",&cfg,&checkpoint,&[]).unwrap());
        let summary=inventory_summary_for_agent_scope(&c,"o","a",None,None,None).unwrap();
        assert_eq!(summary[0]["failed"],1);assert_eq!(summary[0]["passed"],0);
        assert_eq!(list_for_run(&c,"o","r").unwrap().len(),1);
        assert_eq!(list_checkpoints_for_agent_page(&c,"o","a",None,None,None,None,50).unwrap().0.len(),0);
    }

    #[test]
    fn same_second_replay_and_repair_keep_newer_failed_observation() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"staging"});
        let old=json!({"key":"same-second","description":"first","status":"passed","evidence":["old"]});
        let new=json!({"findings":[],"scenarios":[{"key":"same-second","description":"later failed","status":"failed","evidence":["new"]}]});
        persist_checkpoint(&c,"o","a","old-run","old-attempt",&cfg,&old,&[]).unwrap();
        persist_result(&c,"o","a","new-run","new-attempt",&cfg,&new,&[]).unwrap();
        let times:(String,String)=c.query_row("SELECT (SELECT tested_at FROM qa_scenario_checkpoints WHERE run_id='old-run'),(SELECT tested_at FROM qa_scenario_results WHERE run_id='new-run')",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(times.0,times.1,"reproduction must share the actual SQLite second");
        assert!(!persist_checkpoint(&c,"o","a","old-run","old-attempt",&cfg,&old,&[]).unwrap());
        assert_eq!(c.query_row("SELECT last_status FROM qa_scenario_inventory WHERE scenario_key='same-second'",[],|r|r.get::<_,String>(0)).unwrap(),"failed");
        c.execute("DELETE FROM qa_scenario_inventory WHERE scenario_key='same-second'",[]).unwrap();
        assert!(!persist_checkpoint(&c,"o","a","old-run","old-attempt",&cfg,&old,&[]).unwrap());
        assert_eq!(c.query_row("SELECT last_status FROM qa_scenario_inventory WHERE scenario_key='same-second'",[],|r|r.get::<_,String>(0)).unwrap(),"failed");
    }

    #[test]
    fn replayed_final_keeps_original_order_and_inventory_precedence() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"staging"});
        let old=json!({"plan":[{"key":"cart-total","description":"Cart total","reason":"critical checkout"}],"findings":[],"scenarios":[{"key":"cart-total","description":"Cart total","status":"passed","evidence":["old"]}]});
        let new=json!({"findings":[],"scenarios":[{"key":"cart-total","description":"Cart total","status":"failed","evidence":["new"]}]});
        persist_result(&c,"o","a","old-run","attempt-1",&cfg,&old,&[]).unwrap();
        let old_order:i64=c.query_row("SELECT observation_order FROM qa_scenario_results WHERE run_id='old-run'",[],|r|r.get(0)).unwrap();
        persist_result(&c,"o","a","new-run","attempt-1",&cfg,&new,&[]).unwrap();
        assert!(c.query_row("SELECT observation_order FROM qa_scenario_results WHERE run_id='new-run'",[],|r|r.get::<_,i64>(0)).unwrap()>old_order);
        persist_result(&c,"o","a","old-run","attempt-1",&cfg,&old,&[]).unwrap();
        assert_eq!(c.query_row("SELECT observation_order FROM qa_scenario_results WHERE run_id='old-run'",[],|r|r.get::<_,i64>(0)).unwrap(),old_order);
        assert_eq!(c.query_row("SELECT last_status FROM qa_scenario_inventory WHERE scenario_key='cart-total'",[],|r|r.get::<_,String>(0)).unwrap(),"failed");
        assert_eq!(c.query_row("SELECT status FROM qa_batch_plans WHERE run_id='old-run' AND attempt_id='attempt-1' AND scenario_key='cart-total'",[],|r|r.get::<_,String>(0)).unwrap(),"passed");
        let facts_before:String=c.query_row("SELECT group_concat(id||'|'||org_id||'|'||agent_id||'|'||COALESCE(project_id,'<null>')||'|'||application||'|'||environment||'|'||run_id||'|'||attempt_id||'|'||scenario_key||'|'||description||'|'||status||'|'||evidence_json||'|'||tested_at||'|'||redaction_verified||'|'||observation_order,';') FROM (SELECT * FROM qa_scenario_results ORDER BY id)",[],|r|r.get(0)).unwrap();
        let sequence_before:i64=c.query_row("SELECT count(*) FROM qa_observation_order",[],|r|r.get(0)).unwrap();
        let expected:(String,String,i64) = c.query_row("SELECT last_status,last_tested_at,observation_order FROM qa_scenario_inventory WHERE scenario_key='cart-total'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        c.execute("DELETE FROM qa_scenario_inventory WHERE scenario_key='cart-total'",[]).unwrap();
        persist_result(&c,"o","a","old-run","attempt-1",&cfg,&old,&[]).unwrap();
        let repaired:(String,String,i64)=c.query_row("SELECT last_status,last_tested_at,observation_order FROM qa_scenario_inventory WHERE scenario_key='cart-total'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(repaired,expected);
        assert_eq!(c.query_row("SELECT status FROM qa_batch_plans WHERE run_id='old-run' AND attempt_id='attempt-1' AND scenario_key='cart-total'",[],|r|r.get::<_,String>(0)).unwrap(),"passed");

        let facts_after:String=c.query_row("SELECT group_concat(id||'|'||org_id||'|'||agent_id||'|'||COALESCE(project_id,'<null>')||'|'||application||'|'||environment||'|'||run_id||'|'||attempt_id||'|'||scenario_key||'|'||description||'|'||status||'|'||evidence_json||'|'||tested_at||'|'||redaction_verified||'|'||observation_order,';') FROM (SELECT * FROM qa_scenario_results ORDER BY id)",[],|r|r.get(0)).unwrap();
        assert_eq!(facts_after,facts_before);
        assert_eq!(c.query_row("SELECT count(*) FROM qa_observation_order",[],|r|r.get::<_,i64>(0)).unwrap(),sequence_before);
    }

    #[test]
    fn progress_persists_plan_and_inventory_without_final_scenarios(){
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"project_id":"p","application":"web","environment":"staging"});
        let inventory=[json!({"key":"orders-checkout","module":"orders","description":"Checkout orders","critical":false,"blocker_fingerprint":null})];
        let plan=[json!({"key":"orders-checkout","description":"Verify order creation","reason":"newly discovered area"})];
        persist_progress(&c,"o","a","r","attempt-1",&cfg,&inventory,&plan,&[]).unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM qa_scenario_results WHERE run_id='r'",[],|row|row.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(c.query_row("SELECT status FROM qa_batch_plans WHERE run_id='r'",[],|row|row.get::<_,String>(0)).unwrap(),"pending");
        assert_eq!(c.query_row("SELECT description FROM qa_scenario_inventory WHERE scenario_key='orders-checkout'",[],|row|row.get::<_,String>(0)).unwrap(),"Checkout orders");
    }

    #[test]
    fn selector_prioritizes_untested_known_areas_and_never_repeats_blockers() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"staging"});
        for (key,module,status) in [("products-list","products",None),("customers-list","customers",None),("sales-create","sales",None),("sales-combo","sales",None),("products-old","products",Some("failed")),("login-auth","auth",Some("blocked")),("orders-check","orders",Some("passed"))] {
            c.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,application,environment,module_key,scenario_key,description,last_status,last_tested_at,blocker_fingerprint,last_blocker_fingerprint) VALUES('o','a',?1,'staging',?2,?3,?3,?4,'2026-01-01',CASE WHEN ?3='login-auth' THEN 'auth-down' END,CASE WHEN ?3='login-auth' THEN 'auth-down' END)",params!["web",module,key,status]).unwrap();
        }
        let selected=context_for_scope(&c,"o","a",&cfg,20).unwrap();
        let keys=selected.iter().map(|row|row["key"].as_str().unwrap()).collect::<Vec<_>>();
        assert_eq!(&keys[..4], &["customers-list","products-list","sales-create","sales-combo"]);
        assert_eq!(keys[4],"products-old");
        assert!(!keys.contains(&"login-auth"));
        assert!(keys.contains(&"orders-check"));
        persist_batch_plan(&c,"o","a","blocked-run","attempt-1",&cfg,&[json!({"key":"login-auth","description":"check login","reason":"blocker changed"})]).unwrap();
        assert!(!context_for_scope(&c,"o","a",&cfg,20).unwrap().iter().any(|row| row["key"] == "login-auth"));
        c.execute("UPDATE qa_scenario_inventory SET blocker_fingerprint='login-service-restored' WHERE scenario_key='login-auth'",[]).unwrap();
        assert!(context_for_scope(&c,"o","a",&cfg,20).unwrap().iter().any(|row| row["key"] == "login-auth"));
        persist_batch_plan(&c,"o","a","retry-run","attempt-1",&cfg,&[json!({"key":"login-auth","description":"retry login","reason":"changed blocker"})]).unwrap();
        assert!(!context_for_scope(&c,"o","a",&cfg,20).unwrap().iter().any(|row| row["key"] == "login-auth"));
    }

    #[test]
    fn selector_reserves_a_bounded_critical_share_without_starving_discovery(){
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"staging"});
        c.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,application,environment,module_key,scenario_key,description,critical,last_status,last_tested_at) VALUES('o','a','web','staging','payments','payments-critical','Critical payment control',1,'failed','2020-01-01')",[]).unwrap();
        for batch in 0..4 {
            for index in 0..50 {
                let key=format!("discovery-{batch:02}-{index:02}");
                c.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,application,environment,module_key,scenario_key,description,last_status) VALUES('o','a','web','staging','sales',?1,?1,NULL)",[key]).unwrap();
            }
            let run=format!("critical-run-{batch}");
            let selected=context_for_scope(&c,"o","a",&cfg,50).unwrap();
            assert!(selected.iter().any(|item|item["key"]=="payments-critical" && item["critical"]==true && item["reason"]=="recheck critical scenario"));
            assert!(selected.iter().any(|item|item["key"].as_str().unwrap().starts_with("discovery-")));
            persist_batch_plan(&c,"o","a",&run,"attempt-1",&cfg,&selected).unwrap();
        }

        for index in 0..50 {
            let key=format!("passed-critical-{index:02}");
            c.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,application,environment,module_key,scenario_key,description,critical,last_status,last_tested_at) VALUES('o','a','web','staging','core',?1,?1,1,'passed','2020-01-01')",[key]).unwrap();
        }
        for batch in 0..4 {
            let sales=format!("sales-create-{batch:02}");
            let combo=format!("orders-combo-{batch:02}");
            c.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,application,environment,module_key,scenario_key,description,last_status) VALUES('o','a','web','staging','sales',?1,?1,NULL),('o','a','web','staging','orders',?2,?2,NULL)",params![sales,combo]).unwrap();
            let run=format!("discovery-run-{batch}");
            let selected=context_for_scope(&c,"o","a",&cfg,50).unwrap();
            assert!(selected.iter().any(|item|item["key"]==sales));
            assert!(selected.iter().any(|item|item["key"]==combo));
            assert!(selected.iter().filter(|item|item["critical"]==true).count()<=10);
            persist_batch_plan(&c,"o","a",&run,"attempt-1",&cfg,&selected).unwrap();
        }
        assert!(context_for_scope(&c,"o","a",&cfg,0).unwrap().is_empty());
        assert_eq!(context_for_scope(&c,"o","a",&cfg,1).unwrap().len(),1);
        let no_critical=Connection::open_in_memory().unwrap();
        no_critical.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&no_critical).unwrap();
        no_critical.execute("INSERT INTO qa_scenario_inventory(org_id,agent_id,application,environment,module_key,scenario_key,description) VALUES('o','a','web','staging','sales','sales-new','New sale')",[]).unwrap();
        let one=context_for_scope(&no_critical,"o","a",&cfg,1).unwrap();
        assert_eq!(one.len(),1);
        assert_eq!(one[0]["critical"],false);
    }

    #[test]
    fn retained_history_uses_stable_cursor_over_more_than_one_hundred_rows() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg=json!({"application":"web","environment":"staging"});
        for index in 0..125 {
            let result=json!({"findings":[],"scenarios":[{"key":format!("case-{index}"),"description":"observed","status":"passed","evidence":["ok"]}]});
            let run=format!("run-{index}");
            persist_result(&c,"o","a",&run,"attempt-1",&cfg,&result,&[]).unwrap();
            let scenario=&result["scenarios"][0];
            persist_batch_plan(&c,"o","a",&run,"attempt-1",&cfg,&[json!({"key":scenario["key"],"description":"observed","reason":"rotation"})]).unwrap();
            persist_checkpoint(&c,"o","a",&run,"attempt-checkpoint",&cfg,scenario,&[]).unwrap();
        }
        let (first,cursor)=list_for_agent_page(&c,"o","a",None,None,None,None,50).unwrap();
        let (second,next)=list_for_agent_page(&c,"o","a",None,None,None,cursor.as_deref(),50).unwrap();
        let (third,end)=list_for_agent_page(&c,"o","a",None,None,None,next.as_deref(),50).unwrap();
        assert_eq!(first.len()+second.len()+third.len(),125);
        assert!(end.is_none());
        assert_eq!(first[0]["status"],"passed");
        let (plans,plan_cursor)=list_plans_for_agent_page(&c,"o","a",None,None,None,None,50).unwrap();
        let (plans2,plan_next)=list_plans_for_agent_page(&c,"o","a",None,None,None,plan_cursor.as_deref(),50).unwrap();
        let (plans3,plan_end)=list_plans_for_agent_page(&c,"o","a",None,None,None,plan_next.as_deref(),50).unwrap();
        assert_eq!(plans.len()+plans2.len()+plans3.len(),125);
        assert!(plan_end.is_none());
        let (checkpoints,checkpoint_cursor)=list_checkpoints_for_agent_page(&c,"o","a",None,None,None,None,50).unwrap();
        let (checkpoints2,checkpoint_next)=list_checkpoints_for_agent_page(&c,"o","a",None,None,None,checkpoint_cursor.as_deref(),50).unwrap();
        let (checkpoints3,checkpoint_end)=list_checkpoints_for_agent_page(&c,"o","a",None,None,None,checkpoint_next.as_deref(),50).unwrap();
        assert_eq!(checkpoints.len()+checkpoints2.len()+checkpoints3.len(),125);
        assert!(checkpoint_end.is_none());
        let (inventory,inventory_cursor)=list_inventory_for_agent_page(&c,"o","a",None,None,None,None,50).unwrap();
        let (inventory2,inventory_next)=list_inventory_for_agent_page(&c,"o","a",None,None,None,inventory_cursor.as_deref(),50).unwrap();
        let (inventory3,inventory_end)=list_inventory_for_agent_page(&c,"o","a",None,None,None,inventory_next.as_deref(),50).unwrap();
        assert_eq!(inventory.len()+inventory2.len()+inventory3.len(),125);
        assert!(inventory_end.is_none());
        let summary=inventory_summary_for_agent_scope(&c,"o","a",None,None,None).unwrap();
        assert_eq!(summary.len(),1);assert_eq!(summary[0]["total"],125);assert_eq!(summary[0]["passed"],125);assert_eq!(summary[0]["never_tested"],0);
        let reconciled=list_for_run(&c,"o","run-0").unwrap();
        assert_eq!(reconciled.len(),2);
        assert!(reconciled.iter().any(|row|row["status"]=="passed"&&row.get("checkpoint").is_none()));
        assert!(reconciled.iter().any(|row|row["checkpoint"]==true));
    }

    #[test]
    fn migration_quarantines_legacy_transcripts_and_allows_redacted_appends() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); CREATE TABLE autonomous_agent_run_transcript(id TEXT PRIMARY KEY,org_id TEXT NOT NULL,run_id TEXT NOT NULL,sequence INTEGER NOT NULL,kind TEXT NOT NULL,payload_json TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT (datetime('now')),UNIQUE(run_id,sequence)); INSERT INTO organizations VALUES('o'); INSERT INTO autonomous_agent_run_transcript(id,org_id,run_id,sequence,kind,payload_json) VALUES('legacy','o','r',1,'assistant','{\"text\":\"rotated-secret\"}');").unwrap();
        migrate(&c).unwrap();
        assert!(crate::db::queries::list_autonomous_agent_transcript(&c,"o","r",0,108).unwrap().is_empty());
        crate::db::queries::append_autonomous_agent_transcript_turn(&c,"o","r",2,"assistant","{\"text\":\"[REDACTED]\"}").unwrap();
        let rows = crate::db::queries::list_autonomous_agent_transcript(&c,"o","r",0,108).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].payload["text"], "[REDACTED]");
    }

    #[test]
    fn checkpoint_is_attempt_scoped_validated_redacted_and_retained_after_run_cleanup() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY); INSERT INTO organizations VALUES('o');").unwrap();
        migrate(&c).unwrap();
        let cfg = json!({"project_id":"p","application":"web","environment":"test"});
        let checkpoint = json!({"key":"cart-total","description":"Verify cart total","status":"failed","evidence":["Observed 10 instead of 9 known-token"]});
        assert!(persist_checkpoint(&c,"o","a","run","attempt-1",&cfg,&checkpoint,&["known-token".into()]).unwrap());
        assert!(!persist_checkpoint(&c,"o","a","run","attempt-1",&cfg,&checkpoint,&["known-token".into()]).unwrap());
        assert!(persist_checkpoint(&c,"o","a","run","attempt-2",&cfg,&checkpoint,&["known-token".into()]).unwrap());
        let rows: i64 = c.query_row("SELECT count(*) FROM qa_scenario_checkpoints",[],|r|r.get(0)).unwrap();
        assert_eq!(rows,2);
        let evidence: String = c.query_row("SELECT evidence_json FROM qa_scenario_checkpoints LIMIT 1",[],|r|r.get(0)).unwrap();
        assert!(!evidence.contains("known-token"));
        let invalid = json!({"key":"invalid","description":"not executed","status":"planned","evidence":["planned"]});
        assert!(persist_checkpoint(&c,"o","a","run","attempt-1",&cfg,&invalid,&[]).is_err());
        let duplicate_key = json!({"key":"","description":"invalid","status":"passed","evidence":["observed"]});
        assert!(persist_checkpoint(&c,"o","a","run","attempt-1",&cfg,&duplicate_key,&[]).is_err());
        c.execute_batch("CREATE TABLE autonomous_agent_runs(id TEXT); INSERT INTO autonomous_agent_runs VALUES('run'); DELETE FROM autonomous_agent_runs WHERE id='run';").unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM qa_scenario_checkpoints",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    }

    #[test]
    fn v110_backfill_orders_checkpoint_and_result_events_globally() {
        let c=Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE qa_scenario_checkpoints(id INTEGER PRIMARY KEY, tested_at TEXT); CREATE TABLE qa_scenario_results(id INTEGER PRIMARY KEY, tested_at TEXT); CREATE TABLE qa_scenario_inventory(id INTEGER PRIMARY KEY); INSERT INTO qa_scenario_checkpoints VALUES(1,'2026-11-02 00:00:00'),(2,'2026-11-01 00:00:00'),(3,'2026-11-03 00:00:00'); INSERT INTO qa_scenario_results VALUES(1,'2026-11-01 00:00:00'),(2,'2026-11-04 00:00:00'); PRAGMA user_version=109;").unwrap();
        crate::db::migrations::run_v110(&c).unwrap();
        let order=|table:&str,id:i64|c.query_row(&format!("SELECT observation_order FROM {table} WHERE id=?1"),[id],|r|r.get::<_,i64>(0)).unwrap();
        assert!(order("qa_scenario_checkpoints",2)<order("qa_scenario_results",1),"final wins same-time ties");
        assert!(order("qa_scenario_results",1)<order("qa_scenario_checkpoints",1));
        assert!(order("qa_scenario_checkpoints",1)<order("qa_scenario_checkpoints",3));
        assert!(order("qa_scenario_checkpoints",3)<order("qa_scenario_results",2));
        let before:i64=c.query_row("SELECT count(*) FROM qa_observation_order",[],|r|r.get(0)).unwrap();
        crate::db::migrations::run_v110(&c).unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM qa_observation_order",[],|r|r.get::<_,i64>(0)).unwrap(),before);
        assert_eq!(c.query_row("PRAGMA user_version",[],|r|r.get::<_,i32>(0)).unwrap(),110);
    }

    #[test]
    fn migration_quarantines_legacy_qa_evidence() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE organizations(id TEXT PRIMARY KEY);").unwrap();
        c.execute("INSERT INTO organizations VALUES('o')", []).unwrap();
        migrate(&c).unwrap();
        let cfg = json!({"application":"web"});
        let result = json!({"findings":[],"scenarios":[{"key":"legacy","description":"legacy","status":"passed","evidence":["potential-secret"]}]});
        persist_result(&c,"o","a","old","attempt-1",&cfg,&result,&[]).unwrap();
        c.execute("UPDATE qa_scenario_results SET redaction_verified=0 WHERE run_id='old'", []).unwrap();
        assert!(list_for_run(&c,"o","old").unwrap().is_empty());
        crate::db::migrations::run_v110(&c).unwrap();
        assert_eq!(c.query_row("PRAGMA user_version", [], |r| r.get::<_,i32>(0)).unwrap(), 110);
    }
}
