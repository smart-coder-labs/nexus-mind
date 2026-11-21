//! Storage for runtime error monitoring: sources (one per connected GCP
//! project) and error groups (one per fingerprint). Raw events are not kept,
//! only the latest redacted sample of each group.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::monitor::{decide, Action, RuntimeError};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MonitorSource {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub name: String,
    pub service_account: String,
    pub slack_connector_id: Option<String>,
    /// Channel id for the server Slack MCP (`slack_connector_id = server_slack_mcp`).
    pub slack_channel: Option<String>,
    pub github_connector_id: Option<String>,
    pub github_repo: Option<String>,
    pub throttle_minutes: i64,
    pub enabled: bool,
    pub created_at: String,
    /// The audience the push subscription must be created with.
    pub audience: String,
    /// Executor of the error audit (`claude`, `codex`, `pi`, `nexus`); `None` = no audit.
    pub audit_executor: Option<String>,
    pub audit_model: Option<String>,
    /// The managed `error_audit` agent that runs the audits.
    pub audit_agent_id: Option<String>,
}

pub struct SourceWrite {
    pub project: String,
    pub name: String,
    pub service_account: String,
    pub slack_connector_id: Option<String>,
    /// Channel id for the server Slack MCP (`slack_connector_id = server_slack_mcp`).
    pub slack_channel: Option<String>,
    pub github_connector_id: Option<String>,
    pub github_repo: Option<String>,
    pub throttle_minutes: i64,
    pub enabled: bool,
    pub audit_executor: Option<String>,
    pub audit_model: Option<String>,
}

const SOURCE_COLUMNS: &str = "id,org_id,project,name,service_account,slack_connector_id,github_connector_id,github_repo,throttle_minutes,enabled,created_at,slack_channel,audit_executor,audit_model,audit_agent_id";

fn source_from_row(row: &rusqlite::Row) -> rusqlite::Result<MonitorSource> {
    let id: String = row.get(0)?;
    Ok(MonitorSource {
        audience: crate::monitor::oidc::audience(&id),
        id,
        org_id: row.get(1)?,
        project: row.get(2)?,
        name: row.get(3)?,
        service_account: row.get(4)?,
        slack_connector_id: row.get(5)?,
        github_connector_id: row.get(6)?,
        github_repo: row.get(7)?,
        throttle_minutes: row.get(8)?,
        enabled: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
        slack_channel: row.get(11)?,
        audit_executor: row.get(12)?,
        audit_model: row.get(13)?,
        audit_agent_id: row.get(14)?,
    })
}

/// `None` when the name is taken in this org.
pub fn create_source(c: &Connection, org_id: &str, user_id: &str, w: &SourceWrite) -> Result<Option<MonitorSource>> {
    let id = Uuid::new_v4().to_string();
    let inserted = c.execute(
        "INSERT OR IGNORE INTO monitor_sources
           (id,org_id,project,name,service_account,slack_connector_id,github_connector_id,github_repo,throttle_minutes,enabled,created_by,slack_channel,audit_executor,audit_model)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![id, org_id, w.project, w.name, w.service_account, w.slack_connector_id, w.github_connector_id,
                w.github_repo, w.throttle_minutes, w.enabled as i64, user_id, w.slack_channel, w.audit_executor, w.audit_model],
    )?;
    if inserted == 0 {
        return Ok(None);
    }
    get_source(c, org_id, &id)
}

/// `Ok(false)` when the source is missing or the new name is taken.
pub fn update_source(c: &Connection, org_id: &str, id: &str, w: &SourceWrite) -> Result<bool> {
    let updated = c.execute(
        "UPDATE OR IGNORE monitor_sources SET project=?3,name=?4,service_account=?5,slack_connector_id=?6,
           github_connector_id=?7,github_repo=?8,throttle_minutes=?9,enabled=?10,slack_channel=?11,
           audit_executor=?12,audit_model=?13
         WHERE org_id=?1 AND id=?2",
        params![org_id, id, w.project, w.name, w.service_account, w.slack_connector_id, w.github_connector_id,
                w.github_repo, w.throttle_minutes, w.enabled as i64, w.slack_channel, w.audit_executor, w.audit_model],
    )?;
    Ok(updated > 0)
}

pub fn get_source(c: &Connection, org_id: &str, id: &str) -> Result<Option<MonitorSource>> {
    Ok(c.query_row(
        &format!("SELECT {SOURCE_COLUMNS} FROM monitor_sources WHERE org_id=?1 AND id=?2"),
        params![org_id, id],
        source_from_row,
    )
    .optional()?)
}

/// For the public push endpoint, which only knows the source id.
pub fn get_source_by_id(c: &Connection, id: &str) -> Result<Option<MonitorSource>> {
    Ok(c.query_row(&format!("SELECT {SOURCE_COLUMNS} FROM monitor_sources WHERE id=?1"), params![id], source_from_row)
        .optional()?)
}

pub fn list_sources(c: &Connection, org_id: &str) -> Result<Vec<MonitorSource>> {
    let mut stmt = c.prepare(&format!("SELECT {SOURCE_COLUMNS} FROM monitor_sources WHERE org_id=?1 ORDER BY name"))?;
    let rows = stmt.query_map(params![org_id], source_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Pause or resume. `Ok(false)` when the source is missing.
pub fn set_source_enabled(c: &Connection, org_id: &str, id: &str, enabled: bool) -> Result<bool> {
    Ok(c.execute("UPDATE monitor_sources SET enabled=?3 WHERE org_id=?1 AND id=?2", params![org_id, id, enabled as i64])? > 0)
}

pub fn delete_source(c: &Connection, org_id: &str, id: &str) -> Result<bool> {
    Ok(c.execute("DELETE FROM monitor_sources WHERE org_id=?1 AND id=?2", params![org_id, id])? > 0)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MonitorGroup {
    pub id: String,
    pub source_id: String,
    pub service: String,
    pub title: String,
    pub severity: String,
    pub count: i64,
    pub first_seen: String,
    pub last_seen: String,
    pub sample: String,
    pub log_url: Option<String>,
    pub issue_number: Option<i64>,
    pub status: String,
    /// The error audit's agent run, its state (`pending`, `done`, `failed`) and conclusion.
    pub audit_run_id: Option<String>,
    pub audit_status: Option<String>,
    pub audit_summary: Option<String>,
    /// Where the group's issue lives (the audit may route it away from the
    /// source's default repository).
    pub issue_repo: Option<String>,
}

const GROUP_COLUMNS: &str =
    "id,source_id,service,title,severity,count,first_seen,last_seen,sample,log_url,issue_number,status,audit_run_id,audit_status,audit_summary,issue_repo";

fn group_from_row(row: &rusqlite::Row) -> rusqlite::Result<MonitorGroup> {
    Ok(MonitorGroup {
        id: row.get(0)?,
        source_id: row.get(1)?,
        service: row.get(2)?,
        title: row.get(3)?,
        severity: row.get(4)?,
        count: row.get(5)?,
        first_seen: row.get(6)?,
        last_seen: row.get(7)?,
        sample: row.get(8)?,
        log_url: row.get(9)?,
        issue_number: row.get(10)?,
        status: row.get(11)?,
        audit_run_id: row.get(12)?,
        audit_status: row.get(13)?,
        audit_summary: row.get(14)?,
        issue_repo: row.get(15)?,
    })
}

/// Records one occurrence and decides what it triggers, in one transaction:
/// concurrent pushes of the same new error produce exactly one `New`. Any
/// alerting action claims the throttle window right away.
pub fn record(
    c: &Connection,
    source: &MonitorSource,
    fingerprint: &str,
    error: &RuntimeError,
    now: DateTime<Utc>,
) -> Result<(Action, MonitorGroup)> {
    let tx = c.unchecked_transaction()?;
    let existing: Option<(String, String, Option<String>)> = tx
        .query_row(
            "SELECT id,status,last_alert_at FROM monitor_groups WHERE source_id=?1 AND fingerprint=?2",
            params![source.id, fingerprint],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let last_alert = existing
        .as_ref()
        .and_then(|(_, _, at)| at.as_deref())
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc));
    let action = decide(existing.as_ref().map(|(_, status, _)| (status.as_str(), last_alert)), source.throttle_minutes, now);
    let now_text = now.to_rfc3339();
    let alerted = (action != Action::Quiet).then(|| now_text.clone());
    let sample = serde_json::json!({
        "message": crate::factory::redact::redact(&error.message),
        "stack": error.stack.as_deref().map(crate::factory::redact::redact),
    })
    .to_string();
    let title = crate::factory::redact::redact(&error.title);
    let id = match existing {
        None => {
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO monitor_groups (id,org_id,source_id,fingerprint,service,title,severity,first_seen,last_seen,sample,log_url,last_alert_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?10,?8)",
                params![id, source.org_id, source.id, fingerprint, error.service, title, error.severity, now_text, sample, error.log_url],
            )?;
            id
        }
        Some((id, _, _)) => {
            tx.execute(
                "UPDATE monitor_groups SET count=count+1,last_seen=?2,sample=?3,log_url=COALESCE(?4,log_url),severity=?5,
                   last_alert_at=COALESCE(?6,last_alert_at),status=CASE WHEN status='resolved' THEN 'open' ELSE status END
                 WHERE id=?1",
                params![id, now_text, sample, error.log_url, error.severity, alerted],
            )?;
            id
        }
    };
    let group = tx.query_row(&format!("SELECT {GROUP_COLUMNS} FROM monitor_groups WHERE id=?1"), params![id], group_from_row)?;
    tx.commit()?;
    Ok((action, group))
}

pub fn set_issue(c: &Connection, group_id: &str, number: i64, repository: &str) -> Result<()> {
    c.execute("UPDATE monitor_groups SET issue_number=?2,issue_repo=?3 WHERE id=?1", params![group_id, number, repository])?;
    Ok(())
}

pub fn list_groups(c: &Connection, org_id: &str, source_id: Option<&str>, limit: i64) -> Result<Vec<MonitorGroup>> {
    let mut stmt = c.prepare(&format!(
        "SELECT {GROUP_COLUMNS} FROM monitor_groups WHERE org_id=?1 AND (?2 IS NULL OR source_id=?2)
         ORDER BY last_seen DESC LIMIT ?3"
    ))?;
    let rows = stmt.query_map(params![org_id, source_id, limit], group_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// `status`: open, resolved or muted. `Ok(false)` when the group is missing.
pub fn set_group_status(c: &Connection, org_id: &str, id: &str, status: &str) -> Result<bool> {
    Ok(c.execute("UPDATE monitor_groups SET status=?3 WHERE org_id=?1 AND id=?2", params![org_id, id, status])? > 0)
}

pub fn set_audit_agent(c: &Connection, org_id: &str, source_id: &str, agent_id: Option<&str>) -> Result<()> {
    c.execute("UPDATE monitor_sources SET audit_agent_id=?3 WHERE org_id=?1 AND id=?2", params![org_id, source_id, agent_id])?;
    Ok(())
}

/// Audits a source started since UTC midnight (the daily cap counts these).
pub fn audits_today(c: &Connection, source_id: &str) -> Result<i64> {
    Ok(c.query_row(
        "SELECT COUNT(*) FROM monitor_groups WHERE source_id=?1 AND audit_started_at >= date('now')",
        params![source_id],
        |r| r.get(0),
    )?)
}

/// Marks a group's audit as started, unless one is already pending.
pub fn start_audit(c: &Connection, group_id: &str, run_id: &str) -> Result<bool> {
    Ok(c.execute(
        "UPDATE monitor_groups SET audit_run_id=?2,audit_status='pending',audit_summary=NULL,audit_started_at=datetime('now')
         WHERE id=?1 AND COALESCE(audit_status,'')<>'pending'",
        params![group_id, run_id],
    )? > 0)
}

/// The group an audit run belongs to, inside its organization.
pub fn group_for_audit_run(c: &Connection, org_id: &str, run_id: &str) -> Result<Option<MonitorGroup>> {
    Ok(c.query_row(
        &format!("SELECT {GROUP_COLUMNS} FROM monitor_groups WHERE org_id=?1 AND audit_run_id=?2"),
        params![org_id, run_id],
        group_from_row,
    )
    .optional()?)
}

pub fn finish_audit(c: &Connection, group_id: &str, status: &str, summary: Option<&str>) -> Result<()> {
    c.execute(
        "UPDATE monitor_groups SET audit_status=?2,audit_summary=?3 WHERE id=?1",
        params![group_id, status, summary],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_all(&c).unwrap();
        c.execute("INSERT INTO organizations (id,name,slug) VALUES ('o1','Org','org')", []).unwrap();
        c
    }

    fn write(name: &str) -> SourceWrite {
        SourceWrite {
            project: "p".into(),
            name: name.into(),
            service_account: "push@p.iam.gserviceaccount.com".into(),
            slack_connector_id: None,
            slack_channel: None,
            github_connector_id: None,
            github_repo: None,
            throttle_minutes: 30,
            enabled: true,
            audit_executor: None,
            audit_model: None,
        }
    }

    fn error(message: &str) -> RuntimeError {
        RuntimeError {
            service: "sales".into(),
            severity: "ERROR".into(),
            title: message.into(),
            message: message.into(),
            stack: None,
            log_url: None,
            group_hint: None,
        }
    }

    #[test]
    fn sources_are_org_scoped_and_names_unique() {
        let c = conn();
        let source = create_source(&c, "o1", "u1", &write("gcp")).unwrap().unwrap();
        assert_eq!(source.audience, format!("nexusmind-monitor:{}", source.id));
        assert!(create_source(&c, "o1", "u1", &write("gcp")).unwrap().is_none());
        assert!(get_source(&c, "other", &source.id).unwrap().is_none());
        assert_eq!(get_source_by_id(&c, &source.id).unwrap().unwrap().id, source.id);
        assert!(!delete_source(&c, "other", &source.id).unwrap());
        assert!(delete_source(&c, "o1", &source.id).unwrap());
    }

    #[test]
    fn repeats_count_and_alert_once_per_window_then_regress_after_resolve() {
        let c = conn();
        let source = create_source(&c, "o1", "u1", &write("gcp")).unwrap().unwrap();
        let now = Utc::now();
        let (action, group) = record(&c, &source, "fp", &error("boom"), now).unwrap();
        assert_eq!((action, group.count), (Action::New, 1));
        let (action, group) = record(&c, &source, "fp", &error("boom"), now + chrono::Duration::minutes(1)).unwrap();
        assert_eq!((action, group.count), (Action::Quiet, 2));
        let (action, _) = record(&c, &source, "fp", &error("boom"), now + chrono::Duration::minutes(40)).unwrap();
        assert_eq!(action, Action::Notify);
        let (action, _) = record(&c, &source, "fp", &error("boom"), now + chrono::Duration::minutes(41)).unwrap();
        assert_eq!(action, Action::Quiet);
        assert!(set_group_status(&c, "o1", &group.id, "resolved").unwrap());
        let (action, group) = record(&c, &source, "fp", &error("boom"), now + chrono::Duration::minutes(42)).unwrap();
        assert_eq!((action, group.status.as_str(), group.count), (Action::Regressed, "open", 5));
        assert_eq!(list_groups(&c, "o1", Some(&source.id), 10).unwrap().len(), 1);
        assert!(list_groups(&c, "other", None, 10).unwrap().is_empty());
    }

    #[test]
    fn audits_start_once_and_count_per_day() {
        let c = conn();
        let source = create_source(&c, "o1", "u1", &write("gcp")).unwrap().unwrap();
        let (_, group) = record(&c, &source, "fp", &error("boom"), Utc::now()).unwrap();
        assert_eq!(audits_today(&c, &source.id).unwrap(), 0);
        assert!(start_audit(&c, &group.id, "run-1").unwrap());
        assert!(!start_audit(&c, &group.id, "run-2").unwrap(), "one pending audit per group");
        assert_eq!(audits_today(&c, &source.id).unwrap(), 1);
        assert!(group_for_audit_run(&c, "other", "run-1").unwrap().is_none());
        let found = group_for_audit_run(&c, "o1", "run-1").unwrap().unwrap();
        assert_eq!((found.audit_status.as_deref(), found.audit_run_id.as_deref()), (Some("pending"), Some("run-1")));
        finish_audit(&c, &group.id, "done", Some("cause")).unwrap();
        assert!(start_audit(&c, &group.id, "run-3").unwrap(), "a finished audit can be redone");
    }

    #[test]
    fn stored_samples_are_redacted() {
        let c = conn();
        let source = create_source(&c, "o1", "u1", &write("gcp")).unwrap().unwrap();
        let (_, group) = record(&c, &source, "fp", &error("auth failed token=ghp_abcdefghijklmnopqrstuvwxyz0123456789"), Utc::now()).unwrap();
        assert!(!group.sample.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789"));
        assert!(!group.title.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789"));
    }
}
