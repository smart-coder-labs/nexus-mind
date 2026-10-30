//! Append-only F5 trajectory evidence. Entries are for analysis and audit only;
//! they are deliberately not consulted by routing, policy, or execution code.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanEditMetadata {
    /// Paths changed by a person after the agent run. File contents are never
    /// collected in this first slice.
    pub changed_files: Vec<String>,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TrajectoryEntry {
    pub id: String,
    pub run_id: String,
    pub entry_kind: String,
    pub outcome: Option<String>,
    pub actor_user_id: Option<String>,
    pub data: Value,
    pub created_at: String,
}

/// Record a compact, non-decisional snapshot when an autonomous run finishes.
/// The exact verification contract is copied when present so later verification
/// runs for the same head cannot erase the evidence this run saw.
pub fn record_run_outcome(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    status: &str,
    result: &Value,
) -> Result<()> {
    let specialist = result.get("specialist").or_else(|| result.pointer("/result/specialist"));
    let review_accepted = specialist
        .and_then(|value| value.pointer("/review/accept"))
        .and_then(Value::as_bool);
    let checks_passed = specialist
        .and_then(|value| value.pointer("/checks/passed"))
        .and_then(Value::as_bool);
    let code = result.get("code").and_then(Value::as_str);
    let outcome = specialist_outcome(review_accepted, checks_passed, code);

    let verification_report: Option<String> = conn
        .query_row(
            "SELECT report FROM factory_verification_reports
             WHERE org_id=?1 AND run_id=?2 ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![org_id, run_id],
            |row| row.get(0),
        )
        .optional()?;
    let verification_report = verification_report
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let data = json!({
        "run_status": status,
        "result_code": code,
        "specialist_review_accepted": review_accepted,
        "specialist_checks_passed": checks_passed,
        "verification_report": verification_report,
    });
    conn.execute(
        "INSERT OR IGNORE INTO factory_trajectory_entries
         (id, org_id, run_id, entry_kind, outcome, data_json)
         VALUES (?1, ?2, ?3, 'run_outcome', ?4, ?5)",
        params![uuid::Uuid::new_v4().to_string(), org_id, run_id, outcome, serde_json::to_string(&data)?],
    )?;
    Ok(())
}

fn specialist_outcome(
    review_accepted: Option<bool>,
    checks_passed: Option<bool>,
    code: Option<&str>,
) -> &'static str {
    if review_accepted == Some(true) && checks_passed == Some(true) {
        "accepted"
    } else if review_accepted == Some(false)
        || checks_passed == Some(false)
        || matches!(code, Some("specialist_review_rejected" | "specialist_checks_failed"))
    {
        "rejected"
    } else {
        "unknown"
    }
}

/// Append a human's accepted/rejected label and optional diff statistics. This
/// stores metadata only, never source text or diffs, and never affects a run.
pub fn record_human_feedback(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    user_id: &str,
    outcome: &str,
    edit: Option<&HumanEditMetadata>,
) -> Result<Option<TrajectoryEntry>> {
    if !matches!(outcome, "accepted" | "rejected") {
        anyhow::bail!("invalid_trajectory_outcome");
    }
    let status: Option<String> = conn.query_row(
        "SELECT status FROM autonomous_agent_runs WHERE org_id=?1 AND id=?2",
        params![org_id, run_id],
        |row| row.get(0),
    ).optional()?;
    let Some(status) = status else { return Ok(None) };
    if !matches!(status.as_str(), "succeeded" | "partial" | "failed" | "blocked_policy" | "blocked_runtime" | "budget_exhausted" | "dead_letter") {
        anyhow::bail!("run_not_terminal");
    }
    let data = json!({"human_edit": edit});
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO factory_trajectory_entries
         (id, org_id, run_id, entry_kind, outcome, actor_user_id, data_json)
         VALUES (?1, ?2, ?3, 'human_feedback', ?4, ?5, ?6)",
        params![id, org_id, run_id, outcome, user_id, serde_json::to_string(&data)?],
    )?;
    get_entry(conn, org_id, &id)
}

pub fn list_for_run(conn: &Connection, org_id: &str, run_id: &str) -> Result<Vec<TrajectoryEntry>> {
    let mut statement = conn.prepare(
        "SELECT id, run_id, entry_kind, outcome, actor_user_id, data_json, created_at
         FROM factory_trajectory_entries WHERE org_id=?1 AND run_id=?2
         ORDER BY created_at, rowid",
    )?;
    let rows = statement.query_map(params![org_id, run_id], entry_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

fn get_entry(conn: &Connection, org_id: &str, id: &str) -> Result<Option<TrajectoryEntry>> {
    conn.query_row(
        "SELECT id, run_id, entry_kind, outcome, actor_user_id, data_json, created_at
         FROM factory_trajectory_entries WHERE org_id=?1 AND id=?2",
        params![org_id, id],
        entry_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrajectoryEntry> {
    let data_json: String = row.get(5)?;
    Ok(TrajectoryEntry {
        id: row.get(0)?,
        run_id: row.get(1)?,
        entry_kind: row.get(2)?,
        outcome: row.get(3)?,
        actor_user_id: row.get(4)?,
        data: serde_json::from_str(&data_json).unwrap_or(Value::Null),
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Connection, String, String, String) {
        let conn = crate::db::connection::connect(":memory:").unwrap();
        crate::db::migrations::run_all(&conn).unwrap();
        let (org, user, _) = crate::db::queries::bootstrap(&conn, "Acme", "acme", "a@acme.test", "A").unwrap();
        conn.execute(
            "INSERT INTO autonomous_agent_definitions
             (id,org_id,template_key,template_version,name,status,current_revision,created_by)
             VALUES ('d1',?1,'github_issue_resolver',1,'Resolver','disabled',1,?2)",
            params![org.id, user.id],
        ).unwrap();
        conn.execute(
            "INSERT INTO autonomous_agent_revisions
             (id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,created_by)
             VALUES ('r1','d1',1,'{}','hash','{}','{}',?1)",
            [&user.id],
        ).unwrap();
        crate::db::queries::create_automation_run(&conn, "automation-1", &org.id, None, &user.id, "profile-v1", 0).unwrap();
        conn.execute(
            "INSERT INTO autonomous_agent_runs
             (id,org_id,definition_id,revision_id,automation_run_id,trigger_kind,occurrence_key,budget_json,status)
             VALUES ('run-1',?1,'d1','r1','automation-1','manual','occ-1','{}','succeeded')",
            [&org.id],
        ).unwrap();
        (conn, org.id, user.id, "run-1".into())
    }

    #[test]
    fn records_run_verdict_and_verification_as_an_idempotent_snapshot() {
        let (conn, org, _, run) = setup();
        let report = json!({"schema_version":1,"head_sha":"0123456789abcdef0123456789abcdef01234567","passed":true});
        conn.execute(
            "INSERT INTO factory_verification_reports(id,org_id,run_id,head_sha,passed,report)
             VALUES ('v1',?1,?2,'0123456789abcdef0123456789abcdef01234567',1,?3)",
            params![org, run, report.to_string()],
        ).unwrap();
        let result = json!({"specialist":{"review":{"accept":true},"checks":{"passed":true}}});
        record_run_outcome(&conn, &org, &run, "succeeded", &result).unwrap();
        record_run_outcome(&conn, &org, &run, "succeeded", &result).unwrap();
        let entries = list_for_run(&conn, &org, &run).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].outcome.as_deref(), Some("accepted"));
        assert_eq!(entries[0].data["verification_report"]["passed"], true);
        assert_eq!(specialist_outcome(Some(true), None, None), "unknown");
        assert_eq!(specialist_outcome(None, Some(true), None), "unknown");
    }

    #[test]
    fn rejected_runs_and_human_edit_metadata_are_separate_append_only_entries() {
        let (conn, org, user, run) = setup();
        record_run_outcome(
            &conn,
            &org,
            &run,
            "blocked_policy",
            &json!({"code":"specialist_review_rejected"}),
        ).unwrap();
        let edit = HumanEditMetadata { changed_files: vec!["src/lib.rs".into()], additions: 4, deletions: 2 };
        record_human_feedback(&conn, &org, &run, &user, "accepted", Some(&edit)).unwrap();
        let entries = list_for_run(&conn, &org, &run).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].outcome.as_deref(), Some("rejected"));
        assert_eq!(entries[1].outcome.as_deref(), Some("accepted"));
        assert_eq!(entries[1].data["human_edit"]["changed_files"][0], "src/lib.rs");
        assert!(conn.execute("UPDATE factory_trajectory_entries SET outcome='accepted' WHERE id=?1", [&entries[0].id]).is_err());
        assert!(conn.execute("DELETE FROM factory_trajectory_entries WHERE id=?1", [&entries[0].id]).is_err());
        conn.execute("DELETE FROM autonomous_agent_runs WHERE id=?1", [&run]).unwrap();
        assert!(list_for_run(&conn, &org, &run).unwrap().is_empty(), "parent retention deletes cascade through the audit table");
    }

    #[test]
    fn run_entries_are_org_scoped_and_feedback_requires_existing_run() {
        let (conn, org, user, run) = setup();
        assert!(list_for_run(&conn, "another-org", &run).unwrap().is_empty());
        assert!(record_human_feedback(&conn, &org, "missing", &user, "accepted", None).unwrap().is_none());
        assert!(record_human_feedback(&conn, &org, &run, &user, "unknown", None).is_err());
        conn.execute("UPDATE autonomous_agent_runs SET status='running' WHERE id=?1", [&run]).unwrap();
        assert!(record_human_feedback(&conn, &org, &run, &user, "accepted", None).is_err());
    }
}
