//! Factory intake wiring and watchdog (F3, ADR 55497338). Every query is
//! org-scoped except the worker's inventory of due sources and watchdogs.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

/// How often a source is polled.
pub const POLL_MINUTES: i64 = 15;
/// Automatic starts per org per UTC day (a floor Jev cannot lift).
pub const MAX_AUTO_STARTS_PER_DAY: i64 = 5;

/// A Slack channel or Sentry project feeding one issue-resolver agent.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IntakeSourceRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub project: String,
    pub resolver_definition_id: String,
    /// The resolver's repository; `None` if the agent has none.
    pub repository: Option<String>,
    pub base_ref: String,
    pub privacy_class: String,
    pub connector_id: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub last_polled_at: Option<String>,
    pub last_error: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A source the worker should poll now, with the org it belongs to.
#[derive(Clone, Debug)]
pub struct DueSource {
    pub org_id: String,
    pub source: IntakeSourceRow,
}

/// What a create or an update writes. Validation happens in the caller.
#[derive(Clone, Debug)]
pub struct SourceWrite {
    pub kind: String,
    pub name: String,
    pub project: String,
    pub resolver_definition_id: String,
    pub base_ref: String,
    pub privacy_class: String,
    pub connector_id: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

const SOURCE_COLUMNS: &str = "s.id,s.kind,s.name,s.project,s.resolver_definition_id,
     (SELECT json_extract(r.config_json,'$.repository') FROM autonomous_agent_definitions d
        JOIN autonomous_agent_revisions r ON r.definition_id=d.id AND r.revision=d.current_revision
       WHERE d.id=s.resolver_definition_id),
     s.base_ref,s.privacy_class,s.connector_id,s.config_json,s.enabled,s.last_polled_at,s.last_error,
     s.created_by,s.created_at,s.updated_at";

fn source_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IntakeSourceRow> {
    let config: String = row.get(9)?;
    Ok(IntakeSourceRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        project: row.get(3)?,
        resolver_definition_id: row.get(4)?,
        repository: row.get(5)?,
        base_ref: row.get(6)?,
        privacy_class: row.get(7)?,
        connector_id: row.get(8)?,
        config: serde_json::from_str(&config).unwrap_or_default(),
        enabled: row.get::<_, i64>(10)? == 1,
        last_polled_at: row.get(11)?,
        last_error: row.get(12)?,
        created_by: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

pub fn list_sources(conn: &Connection, org_id: &str) -> Result<Vec<IntakeSourceRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SOURCE_COLUMNS} FROM factory_intake_sources s WHERE s.org_id=?1 ORDER BY s.name"
    ))?;
    let rows = stmt.query_map(params![org_id], source_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_source(conn: &Connection, org_id: &str, id: &str) -> Result<Option<IntakeSourceRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {SOURCE_COLUMNS} FROM factory_intake_sources s WHERE s.org_id=?1 AND s.id=?2"),
            params![org_id, id],
            source_from_row,
        )
        .optional()?)
}

/// Why a source's references were refused.
#[derive(Debug, PartialEq)]
pub enum ReferenceError {
    Resolver,
    Connector,
}

/// The resolver must be this org's `github_issue_resolver`. The token must be
/// a secret connector created for this kind of intake
/// (`metadata.purpose = factory_intake`, `metadata.source_kind = kind`) and, for
/// Sentry, for this host (`metadata.sentry_host`), so a source can never send
/// another secret of the org (or a Sentry token) to a host of its choosing.
pub fn check_source_references(
    conn: &Connection,
    org_id: &str,
    resolver_definition_id: &str,
    connector_id: &str,
    kind: &str,
    sentry_host: Option<&str>,
) -> Result<std::result::Result<(), ReferenceError>> {
    let resolver: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM autonomous_agent_definitions
          WHERE id=?1 AND org_id=?2 AND template_key='github_issue_resolver')",
        params![resolver_definition_id, org_id],
        |row| row.get(0),
    )?;
    if !resolver {
        return Ok(Err(ReferenceError::Resolver));
    }
    let metadata: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM autonomous_agent_connectors
              WHERE id=?1 AND org_id=?2 AND kind='target_secret' AND health!='revoked'",
            params![connector_id, org_id],
            |row| row.get(0),
        )
        .optional()?;
    let metadata: serde_json::Value = metadata
        .and_then(|m| serde_json::from_str(&m).ok())
        .unwrap_or_default();
    Ok(if intake_connector_matches(&metadata, kind, sentry_host) {
        Ok(())
    } else {
        Err(ReferenceError::Connector)
    })
}

/// Whether a connector's metadata binds it to this intake kind (and Sentry host).
pub fn intake_connector_matches(metadata: &serde_json::Value, kind: &str, sentry_host: Option<&str>) -> bool {
    let field = |name: &str| metadata.get(name).and_then(|v| v.as_str());
    field("purpose") == Some("factory_intake")
        && field("source_kind") == Some(kind)
        && (kind != "sentry" || (sentry_host.is_some() && field("sentry_host") == sentry_host))
}

/// Whether the resolver is enabled with a valid revision, so a run can start.
pub fn resolver_ready(conn: &Connection, org_id: &str, resolver_definition_id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM autonomous_agent_definitions d
           JOIN autonomous_agent_revisions r ON r.definition_id=d.id AND r.revision=d.current_revision
          WHERE d.id=?1 AND d.org_id=?2 AND d.status='enabled' AND r.validation_status='valid')",
        params![resolver_definition_id, org_id],
        |row| row.get(0),
    )?)
}

/// Pauses or resumes a source without re-validating it, so a broken source
/// can always be paused.
pub fn set_source_enabled(conn: &Connection, org_id: &str, id: &str, enabled: bool) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE factory_intake_sources SET enabled=?3,updated_at=datetime('now') WHERE org_id=?1 AND id=?2",
        params![org_id, id, enabled as i64],
    )? > 0)
}

/// Creates a source; `Ok(None)` when the name is taken.
pub fn create_source(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    write: &SourceWrite,
) -> Result<Option<IntakeSourceRow>> {
    let id = Uuid::new_v4().to_string();
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO factory_intake_sources
           (id,org_id,kind,name,project,resolver_definition_id,base_ref,privacy_class,connector_id,config_json,enabled,created_by)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            id,
            org_id,
            write.kind,
            write.name,
            write.project,
            write.resolver_definition_id,
            write.base_ref,
            write.privacy_class,
            write.connector_id,
            write.config.to_string(),
            write.enabled as i64,
            user_id
        ],
    )?;
    if inserted == 0 {
        return Ok(None);
    }
    get_source(conn, org_id, &id)
}

/// Replaces a source's settings (its kind never changes); `Ok(false)` when the
/// new name is taken by another source.
pub fn update_source(conn: &Connection, org_id: &str, id: &str, write: &SourceWrite) -> Result<bool> {
    let taken: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_intake_sources WHERE org_id=?1 AND name=?2 AND id!=?3)",
        params![org_id, write.name, id],
        |row| row.get(0),
    )?;
    if taken {
        return Ok(false);
    }
    conn.execute(
        "UPDATE factory_intake_sources SET name=?3,project=?4,resolver_definition_id=?5,base_ref=?6,
                privacy_class=?7,connector_id=?8,config_json=?9,enabled=?10,last_error=NULL,updated_at=datetime('now')
          WHERE org_id=?1 AND id=?2",
        params![
            org_id,
            id,
            write.name,
            write.project,
            write.resolver_definition_id,
            write.base_ref,
            write.privacy_class,
            write.connector_id,
            write.config.to_string(),
            write.enabled as i64
        ],
    )?;
    Ok(true)
}

pub fn delete_source(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM factory_intake_sources WHERE org_id=?1 AND id=?2",
        params![org_id, id],
    )? > 0)
}

/// Enabled sources not polled in the last [`POLL_MINUTES`], in orgs with
/// autonomous agents on.
pub fn due_sources(conn: &Connection, limit: i64) -> Result<Vec<DueSource>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SOURCE_COLUMNS},s.org_id FROM factory_intake_sources s
           JOIN organizations o ON o.id=s.org_id
          WHERE s.enabled=1 AND o.autonomous_agents_enabled=1
            AND (s.last_polled_at IS NULL OR s.last_polled_at <= datetime('now', ?1))
          ORDER BY s.last_polled_at IS NOT NULL, s.last_polled_at
          LIMIT ?2"
    ))?;
    let window = format!("-{POLL_MINUTES} minutes");
    let rows = stmt.query_map(params![window, limit], |row| {
        Ok(DueSource { source: source_from_row(row)?, org_id: row.get(16)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Records a poll; `error` is kept short and never holds secrets.
pub fn mark_polled(conn: &Connection, org_id: &str, id: &str, error: Option<&str>) -> Result<()> {
    let error: Option<String> = error.map(|e| e.chars().take(300).collect());
    conn.execute(
        "UPDATE factory_intake_sources SET last_polled_at=datetime('now'),last_error=?3 WHERE org_id=?1 AND id=?2",
        params![org_id, id, error],
    )?;
    Ok(())
}

pub fn item_exists(conn: &Connection, org_id: &str, task_id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_intake_items WHERE org_id=?1 AND task_id=?2)",
        params![org_id, task_id],
        |row| row.get(0),
    )?)
}

/// Whether this source already produced an item for this source reference
/// (whatever its target), so changing a source's resolver never re-ingests.
pub fn source_item_exists(conn: &Connection, org_id: &str, source_id: &str, source_ref: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_intake_items WHERE org_id=?1 AND source_id=?2 AND source_ref=?3)",
        params![org_id, source_id, source_ref],
        |row| row.get(0),
    )?)
}

/// Automatic starts recorded today (UTC).
pub fn auto_starts_today(conn: &Connection, org_id: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM factory_intake_items
          WHERE org_id=?1 AND start_decision='started' AND created_at >= date('now')",
        params![org_id],
        |row| row.get(0),
    )?)
}

/// One intake item and what the factory decided about it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IntakeItem {
    pub task_id: String,
    pub source_id: Option<String>,
    pub source_ref: String,
    pub nexus_task_id: Option<String>,
    pub task_class: String,
    pub origin_trust: String,
    pub repository: String,
    pub start_decision: String,
    pub start_reason: String,
    pub jev: Option<serde_json::Value>,
    pub issue_number: Option<i64>,
    pub run_id: Option<String>,
    pub created_at: String,
}

pub fn record_item(conn: &Connection, org_id: &str, item: &IntakeItem) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO factory_intake_items
           (org_id,task_id,source_id,source_ref,nexus_task_id,task_class,origin_trust,repository,
            start_decision,start_reason,jev_json,issue_number,run_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            org_id,
            item.task_id,
            item.source_id,
            item.source_ref,
            item.nexus_task_id,
            item.task_class,
            item.origin_trust,
            item.repository,
            item.start_decision,
            item.start_reason,
            item.jev.as_ref().map(|v| v.to_string()),
            item.issue_number,
            item.run_id
        ],
    )?;
    Ok(())
}

pub fn list_items(conn: &Connection, org_id: &str, limit: i64) -> Result<Vec<IntakeItem>> {
    let mut stmt = conn.prepare(
        "SELECT task_id,source_id,source_ref,nexus_task_id,task_class,origin_trust,repository,start_decision,
                start_reason,jev_json,issue_number,run_id,created_at
           FROM factory_intake_items WHERE org_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![org_id, limit], |row| {
        let jev: Option<String> = row.get(9)?;
        Ok(IntakeItem {
            task_id: row.get(0)?,
            source_id: row.get(1)?,
            source_ref: row.get(2)?,
            nexus_task_id: row.get(3)?,
            task_class: row.get(4)?,
            origin_trust: row.get(5)?,
            repository: row.get(6)?,
            start_decision: row.get(7)?,
            start_reason: row.get(8)?,
            jev: jev.and_then(|j| serde_json::from_str(&j).ok()),
            issue_number: row.get(10)?,
            run_id: row.get(11)?,
            created_at: row.get(12)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Creates the backlog factory task (labelled `factory`, its class, and
/// `untrusted` for untrusted text) and the item, still undecided, in one
/// transaction: a failure later never makes the next poll create it again.
pub fn begin_item(
    conn: &Connection,
    org_id: &str,
    created_by: &str,
    project: &str,
    title: &str,
    description: &str,
    item: &IntakeItem,
) -> Result<String> {
    let tx = conn.unchecked_transaction()?;
    let task = crate::db::queries::create_task(
        &tx,
        org_id,
        created_by,
        &crate::models::types::CreateTaskRequest {
            project: project.to_string(),
            title: title.to_string(),
            description: Some(description.to_string()),
            status: Some("backlog".into()),
            priority: None,
            due_date: None,
            parent_id: None,
            sprint_id: None,
        },
    )?;
    crate::db::queries::add_task_label(&tx, &task.id, crate::factory::intake::INTAKE_LABEL)?;
    if item.task_class != "unknown" {
        crate::db::queries::add_task_label(&tx, &task.id, &item.task_class)?;
    }
    if item.origin_trust == "untrusted" {
        crate::db::queries::add_task_label(&tx, &task.id, crate::factory::intake::UNTRUSTED_LABEL)?;
    }
    let item = IntakeItem { nexus_task_id: Some(task.id.clone()), ..item.clone() };
    record_item(&tx, org_id, &item)?;
    tx.commit()?;
    Ok(task.id)
}

/// Records the start decision on an item created by [`begin_item`].
pub fn finish_item(conn: &Connection, org_id: &str, item: &IntakeItem) -> Result<()> {
    conn.execute(
        "UPDATE factory_intake_items SET task_class=?3,start_decision=?4,start_reason=?5,jev_json=?6,
                issue_number=?7,run_id=?8
          WHERE org_id=?1 AND task_id=?2",
        params![
            org_id,
            item.task_id,
            item.task_class,
            item.start_decision,
            item.start_reason,
            item.jev.as_ref().map(|v| v.to_string()),
            item.issue_number,
            item.run_id
        ],
    )?;
    Ok(())
}

/// A started factory task leaves the digest's "not started" list.
pub fn mark_task_started(conn: &Connection, org_id: &str, task_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET status='in_progress', updated_at=datetime('now')
          WHERE org_id=?1 AND id=?2 AND status IN ('backlog','todo')",
        params![org_id, task_id],
    )?;
    Ok(())
}

/// Whether a pull request was opened by a run started from an untrusted intake
/// item: such a PR is never merged without a person.
pub fn untrusted_intake_pull(conn: &Connection, org_id: &str, repository: &str, number: i64) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM factory_intake_items i
             JOIN autonomous_agent_output_links l ON l.run_id=i.run_id AND l.org_id=i.org_id
            WHERE i.org_id=?1 AND i.origin_trust='untrusted' AND lower(i.repository)=lower(?2)
              AND l.kind='draft_pr' AND l.external_id=?3)",
        params![org_id, repository, number.to_string()],
        |row| row.get(0),
    )?)
}

/// Whether `issue_number` in `repository` was opened by the factory from
/// untrusted intake: every resolver branch for it ends in that number.
pub fn untrusted_intake_issue(conn: &Connection, org_id: &str, repository: &str, issue_number: i64) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_intake_items
          WHERE org_id=?1 AND origin_trust='untrusted' AND lower(repository)=lower(?2) AND issue_number=?3)",
        params![org_id, repository, issue_number],
        |row| row.get(0),
    )?)
}

// ---------------------------------------------------------------- Watchdog

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Watchdog {
    pub slack_connector_id: Option<String>,
    pub enabled: bool,
    pub daily_hour_utc: i64,
    pub last_sent_at: Option<String>,
    pub last_daily_on: Option<String>,
}

pub fn get_watchdog(conn: &Connection, org_id: &str) -> Result<Option<Watchdog>> {
    Ok(conn
        .query_row(
            "SELECT slack_connector_id,enabled,daily_hour_utc,last_sent_at,last_daily_on
               FROM factory_watchdog WHERE org_id=?1",
            params![org_id],
            |row| {
                Ok(Watchdog {
                    slack_connector_id: row.get(0)?,
                    enabled: row.get::<_, i64>(1)? == 1,
                    daily_hour_utc: row.get(2)?,
                    last_sent_at: row.get(3)?,
                    last_daily_on: row.get(4)?,
                })
            },
        )
        .optional()?)
}

/// Whether `connector_id` is one of this org's Slack webhook connectors.
pub fn is_slack_connector(conn: &Connection, org_id: &str, connector_id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM autonomous_agent_connectors
          WHERE id=?1 AND org_id=?2 AND kind='slack' AND health!='revoked')",
        params![connector_id, org_id],
        |row| row.get(0),
    )?)
}

pub fn upsert_watchdog(
    conn: &Connection,
    org_id: &str,
    slack_connector_id: Option<&str>,
    enabled: bool,
    daily_hour_utc: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO factory_watchdog (org_id,slack_connector_id,enabled,daily_hour_utc)
         VALUES (?1,?2,?3,?4)
         ON CONFLICT(org_id) DO UPDATE SET slack_connector_id=excluded.slack_connector_id,
             enabled=excluded.enabled,daily_hour_utc=excluded.daily_hour_utc,updated_at=datetime('now')",
        params![org_id, slack_connector_id, enabled as i64, daily_hour_utc],
    )?;
    Ok(())
}

/// A watchdog the worker should evaluate.
#[derive(Clone, Debug)]
pub struct ActiveWatchdog {
    pub org_id: String,
    pub slack_connector_id: String,
    pub daily_hour_utc: i64,
    pub seen: serde_json::Value,
    pub last_sent_at: Option<String>,
    pub last_daily_on: Option<String>,
}

pub fn active_watchdogs(conn: &Connection) -> Result<Vec<ActiveWatchdog>> {
    let mut stmt = conn.prepare(
        "SELECT w.org_id,w.slack_connector_id,w.daily_hour_utc,w.seen_json,w.last_sent_at,w.last_daily_on
           FROM factory_watchdog w
           JOIN organizations o ON o.id=w.org_id
           JOIN autonomous_agent_connectors c ON c.id=w.slack_connector_id AND c.org_id=w.org_id
          WHERE w.enabled=1 AND c.kind='slack' AND c.health!='revoked' 
          ORDER BY w.org_id LIMIT 200",
    )?;
    let rows = stmt.query_map([], |row| {
        let seen: String = row.get(3)?;
        Ok(ActiveWatchdog {
            org_id: row.get(0)?,
            slack_connector_id: row.get(1)?,
            daily_hour_utc: row.get(2)?,
            seen: serde_json::from_str(&seen).unwrap_or_default(),
            last_sent_at: row.get(4)?,
            last_daily_on: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Saves what the watchdog has seen, and when it last sent.
pub fn save_watchdog_state(
    conn: &Connection,
    org_id: &str,
    seen: &serde_json::Value,
    sent: bool,
    daily_on: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE factory_watchdog SET seen_json=?2,
                last_sent_at=CASE WHEN ?3 THEN datetime('now') ELSE last_sent_at END,
                last_daily_on=COALESCE(?4,last_daily_on)
          WHERE org_id=?1",
        params![org_id, seen.to_string(), sent, daily_on],
    )?;
    Ok(())
}

/// Whether the last message went out less than `minutes` ago.
pub fn sent_within(conn: &Connection, last_sent_at: Option<&str>, minutes: i64) -> Result<bool> {
    let Some(last) = last_sent_at else {
        return Ok(false);
    };
    Ok(conn.query_row(
        "SELECT ?1 > datetime('now', ?2)",
        params![last, format!("-{minutes} minutes")],
        |row| row.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_all;

    fn setup() -> (Connection, String, String) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_all(&conn).unwrap();
        let org = "org-1".to_string();
        let user = "user-1".to_string();
        conn.execute(
            "INSERT INTO organizations (id,name,slug,autonomous_agents_enabled) VALUES (?1,'Acme','acme',1)",
            params![org],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id,org_id,name,email,role) VALUES (?1,?2,'Ana','ana@acme.test','admin')",
            params![user, org],
        )
        .unwrap();
        (conn, org, user)
    }

    fn resolver(conn: &Connection, org: &str, user: &str) -> String {
        conn.execute(
            "INSERT INTO autonomous_agent_definitions (id,org_id,template_key,template_version,name,status,current_revision,created_by)
             VALUES ('def-1',?1,'github_issue_resolver',1,'Resolver','enabled',1,?2)",
            params![org, user],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO autonomous_agent_revisions
               (id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,validation_status,created_by)
             VALUES ('rev-1','def-1',1,'{\"repository\":\"acme/app\"}','h','{}','{}','valid',?1)",
            params![user],
        )
        .unwrap();
        "def-1".into()
    }

    fn connector(conn: &Connection, org: &str, id: &str, kind: &str) {
        connector_with(conn, org, id, kind, serde_json::json!({}));
    }

    fn connector_with(conn: &Connection, org: &str, id: &str, kind: &str, metadata: serde_json::Value) {
        conn.execute(
            "INSERT INTO autonomous_agent_connectors (id,org_id,kind,name,health,metadata_json,created_by)
             VALUES (?1,?2,?3,?1,'ready',?4,'user-1')",
            params![id, org, kind, metadata.to_string()],
        )
        .unwrap();
    }

    fn write(resolver: &str, connector: &str) -> SourceWrite {
        SourceWrite {
            kind: "slack".into(),
            name: "bugs channel".into(),
            project: "app".into(),
            resolver_definition_id: resolver.into(),
            base_ref: "main".into(),
            privacy_class: "internal".into(),
            connector_id: connector.into(),
            config: serde_json::json!({"channel_id": "C0123ABCD", "allowed_reactors": ["ULEAD0001"]}),
            enabled: true,
        }
    }

    #[test]
    fn sources_check_their_references_and_come_due_once_per_window() {
        let (conn, org, user) = setup();
        let def = resolver(&conn, &org, &user);
        connector_with(&conn, &org, "conn-secret", "target_secret", serde_json::json!({"purpose": "factory_intake", "source_kind": "slack"}));
        connector_with(
            &conn,
            &org,
            "conn-sentry",
            "target_secret",
            serde_json::json!({"purpose": "factory_intake", "source_kind": "sentry", "sentry_host": "sentry.io"}),
        );
        connector(&conn, &org, "conn-qa", "target_secret");
        connector(&conn, &org, "conn-hook", "slack");
        let check = |resolver: &str, connector: &str, kind: &str, host: Option<&str>| {
            check_source_references(&conn, &org, resolver, connector, kind, host).unwrap()
        };
        assert_eq!(check(&def, "conn-secret", "slack", None), Ok(()));
        assert_eq!(check(&def, "conn-sentry", "sentry", Some("sentry.io")), Ok(()));
        // Another secret of the org, a token made for another kind or another
        // host, and a webhook connector are all refused.
        for (connector, kind, host) in [
            ("conn-qa", "slack", None),
            ("conn-secret", "sentry", Some("sentry.io")),
            ("conn-sentry", "sentry", Some("evil.example.com")),
            ("conn-sentry", "sentry", None),
            ("conn-hook", "slack", None),
        ] {
            assert_eq!(check(&def, connector, kind, host), Err(ReferenceError::Connector), "{connector} {kind} {host:?}");
        }
        assert_eq!(check("nope", "conn-secret", "slack", None), Err(ReferenceError::Resolver));
        assert!(resolver_ready(&conn, &org, &def).unwrap());
        assert!(!resolver_ready(&conn, "org-2", &def).unwrap());
        conn.execute("UPDATE autonomous_agent_definitions SET status='disabled' WHERE id=?1", params![def]).unwrap();
        assert!(!resolver_ready(&conn, &org, &def).unwrap());

        let source = create_source(&conn, &org, &user, &write(&def, "conn-secret")).unwrap().unwrap();
        assert_eq!(source.repository.as_deref(), Some("acme/app"));
        assert!(create_source(&conn, &org, &user, &write(&def, "conn-secret")).unwrap().is_none());

        let due = due_sources(&conn, 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].org_id, org);
        assert_eq!(due[0].source.id, source.id);
        mark_polled(&conn, &org, &source.id, Some("slack_error:not_in_channel")).unwrap();
        assert!(due_sources(&conn, 10).unwrap().is_empty());
        assert_eq!(
            get_source(&conn, &org, &source.id).unwrap().unwrap().last_error.as_deref(),
            Some("slack_error:not_in_channel")
        );

        assert!(set_source_enabled(&conn, &org, &source.id, false).unwrap());
        assert!(!set_source_enabled(&conn, "org-2", &source.id, true).unwrap());
        conn.execute("UPDATE factory_intake_sources SET last_polled_at=NULL", []).unwrap();
        assert!(due_sources(&conn, 10).unwrap().is_empty());
        assert!(delete_source(&conn, &org, &source.id).unwrap());
        assert!(!delete_source(&conn, &org, &source.id).unwrap());
    }

    #[test]
    fn items_dedupe_count_starts_and_mark_untrusted_pulls() {
        let (conn, org, user) = setup();
        let mut item = IntakeItem {
            task_id: "t-1".into(),
            source_id: None,
            source_ref: "C0123ABCD:1.1".into(),
            nexus_task_id: None,
            task_class: "bugfix".into(),
            origin_trust: "untrusted".into(),
            repository: "acme/app".into(),
            start_decision: "started".into(),
            start_reason: "decision_model_allow".into(),
            jev: Some(serde_json::json!({"risk": 0.0})),
            issue_number: Some(12),
            run_id: Some("run-1".into()),
            created_at: String::new(),
        };
        item.start_decision = "backlog".into();
        let task = begin_item(&conn, &org, &user, "app", "Fix refunds", "untrusted text", &item).unwrap();
        let labels: Vec<String> = conn
            .prepare("SELECT label FROM task_labels WHERE task_id=?1 ORDER BY label")
            .unwrap()
            .query_map(params![task], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(labels, ["bugfix", "factory", "untrusted"]);
        assert!(item_exists(&conn, &org, "t-1").unwrap());
        assert_eq!(auto_starts_today(&conn, &org).unwrap(), 0);
        item.start_decision = "started".into();
        finish_item(&conn, &org, &item).unwrap();
        assert_eq!(auto_starts_today(&conn, &org).unwrap(), 1);
        assert_eq!(list_items(&conn, &org, 10).unwrap().len(), 1);

        mark_task_started(&conn, &org, &task).unwrap();
        let status: String =
            conn.query_row("SELECT status FROM tasks WHERE id=?1", params![task], |r| r.get(0)).unwrap();
        assert_eq!(status, "in_progress");

        assert!(untrusted_intake_issue(&conn, &org, "Acme/App", 12).unwrap());
        assert!(!untrusted_intake_issue(&conn, &org, "acme/app", 13).unwrap());
        assert!(!untrusted_intake_issue(&conn, "org-2", "acme/app", 12).unwrap());
        assert!(!untrusted_intake_pull(&conn, &org, "acme/app", 34).unwrap());
        conn.execute("PRAGMA foreign_keys=OFF", []).unwrap();
        conn.execute(
            "INSERT INTO autonomous_agent_output_links (id,org_id,run_id,kind,external_id) VALUES ('l1',?1,'run-1','draft_pr','34')",
            params![org],
        )
        .unwrap();
        assert!(untrusted_intake_pull(&conn, &org, "Acme/App", 34).unwrap());
        assert!(!untrusted_intake_pull(&conn, &org, "acme/other", 34).unwrap());
        assert!(!untrusted_intake_pull(&conn, "org-2", "acme/app", 34).unwrap());
    }

    #[test]
    fn the_watchdog_keeps_its_state_and_throttle() {
        let (conn, org, _) = setup();
        connector(&conn, &org, "conn-hook", "slack");
        connector(&conn, &org, "conn-secret", "target_secret");
        assert!(is_slack_connector(&conn, &org, "conn-hook").unwrap());
        assert!(!is_slack_connector(&conn, &org, "conn-secret").unwrap());
        upsert_watchdog(&conn, &org, Some("conn-hook"), true, 13).unwrap();
        let active = active_watchdogs(&conn).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].seen, serde_json::json!({}));
        save_watchdog_state(&conn, &org, &serde_json::json!({"held": ["x"]}), true, Some("2026-10-04")).unwrap();
        let watchdog = get_watchdog(&conn, &org).unwrap().unwrap();
        assert!(sent_within(&conn, watchdog.last_sent_at.as_deref(), 15).unwrap());
        assert!(!sent_within(&conn, None, 15).unwrap());
        assert_eq!(watchdog.last_daily_on.as_deref(), Some("2026-10-04"));
        upsert_watchdog(&conn, &org, None, true, 13).unwrap();
        assert!(active_watchdogs(&conn).unwrap().is_empty());
    }
}
