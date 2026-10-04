//! Factory operator queries (F3): what waits on a person, what the factory
//! costs, and the human verdict on one action. Every query is org-scoped.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

/// A merge the policy held for a person, not yet decided by one.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeldMerge {
    pub subject: String,
    pub reason: String,
    pub source: String,
    pub created_at: String,
}

/// A run that stopped short and needs someone to look.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BlockedRun {
    pub run_id: String,
    pub agent: String,
    pub template_key: String,
    pub status: String,
    pub reason: Option<String>,
    pub finished_at: Option<String>,
}

/// A factory task (a NexusMind task labelled `factory`) nobody started.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WaitingTask {
    pub id: String,
    pub project: String,
    pub title: String,
    pub status: String,
    pub created_at: String,
}

/// Everything waiting on a person, newest first in each list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HumanDigest {
    pub held_merges: Vec<HeldMerge>,
    pub blocked_runs: Vec<BlockedRun>,
    pub factory_tasks: Vec<WaitingTask>,
    /// Shadow decisions without a human label (OD-5 needs them); `allow`
    /// verdicts first, since only those enter the false-low-risk rate.
    pub unlabeled_shadow: i64,
    pub unlabeled_shadow_allows: i64,
}

const DIGEST_LIMIT: i64 = 50;

/// Statuses where a run stopped and a person has to decide what happens next.
const BLOCKED_STATUSES: &str = "'blocked_policy','blocked_runtime','dead_letter','budget_exhausted'";

pub fn human_digest(conn: &Connection, org_id: &str) -> Result<HumanDigest> {
    let mut held = conn.prepare(
        "SELECT d.subject, d.reason, d.source, d.created_at
         FROM factory_decisions d
         WHERE d.org_id = ?1 AND d.action = 'merge' AND d.verdict = 'hold'
           AND d.created_at >= datetime('now', '-14 days')
           AND NOT EXISTS (SELECT 1 FROM factory_decisions x
                           WHERE x.org_id = d.org_id AND x.subject = d.subject AND x.action = 'merge'
                             AND (x.created_at > d.created_at
                                  OR (x.created_at = d.created_at AND x.rowid > d.rowid)))
         ORDER BY d.created_at DESC LIMIT ?2",
    )?;
    let held_merges = held
        .query_map(params![org_id, DIGEST_LIMIT], |r| {
            Ok(HeldMerge { subject: r.get(0)?, reason: r.get(1)?, source: r.get(2)?, created_at: r.get(3)? })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut blocked = conn.prepare(&format!(
        "SELECT r.id, d.name, d.template_key, r.status,
                (SELECT COALESCE(json_extract(e.payload_json, '$.code'), json_extract(e.payload_json, '$.reason'))
                 FROM autonomous_agent_events e
                 WHERE e.run_id = r.id AND e.kind = 'run.finished'
                 ORDER BY e.sequence DESC LIMIT 1),
                r.finished_at
         FROM autonomous_agent_runs r
         JOIN autonomous_agent_definitions d ON d.id = r.definition_id
         WHERE r.org_id = ?1 AND r.status IN ({BLOCKED_STATUSES}) AND r.archived_at IS NULL
           AND COALESCE(r.finished_at, r.created_at) >= datetime('now', '-7 days')
         ORDER BY COALESCE(r.finished_at, r.created_at) DESC LIMIT ?2"
    ))?;
    let blocked_runs = blocked
        .query_map(params![org_id, DIGEST_LIMIT], |r| {
            Ok(BlockedRun {
                run_id: r.get(0)?,
                agent: r.get(1)?,
                template_key: r.get(2)?,
                status: r.get(3)?,
                reason: r.get::<_, Option<String>>(4)?,
                finished_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut tasks = conn.prepare(
        "SELECT t.id, t.project, t.title, t.status, t.created_at
         FROM tasks t JOIN task_labels l ON l.task_id = t.id
         WHERE t.org_id = ?1 AND l.label = ?3 AND t.archived_at IS NULL
           AND t.status IN ('backlog','todo')
         ORDER BY t.created_at DESC LIMIT ?2",
    )?;
    let factory_tasks = tasks
        .query_map(params![org_id, DIGEST_LIMIT, crate::factory::intake::INTAKE_LABEL], |r| {
            Ok(WaitingTask { id: r.get(0)?, project: r.get(1)?, title: r.get(2)?, status: r.get(3)?, created_at: r.get(4)? })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let (unlabeled_shadow, unlabeled_shadow_allows): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(verdict = 'allow'), 0) FROM factory_shadow_decisions
         WHERE org_id = ?1 AND human_label IS NULL AND outcome IN ('pending','clean','high_risk')",
        [org_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    Ok(HumanDigest { held_merges, blocked_runs, factory_tasks, unlabeled_shadow, unlabeled_shadow_allows })
}

/// Runs and cost of one model over the window.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModelSpend {
    pub model: Option<String>,
    pub runs: i64,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// What the factory cost over the last `days`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Economics {
    pub days: i64,
    pub runs: i64,
    pub cost_usd: f64,
    pub by_model: Vec<ModelSpend>,
    /// Model choices by tier (from `model.selected` events).
    pub tier_choices: Vec<(String, i64)>,
    /// Share of tiered runs that avoided the frontier tier.
    pub frontier_avoidance: Option<f64>,
    /// Pull requests the factory opened.
    pub proposed_changes: i64,
    pub cost_per_proposed_change: Option<f64>,
    /// Whether accepted (merged) changes are measured yet; they are not, so
    /// cost per accepted change is not reported.
    pub accepted_changes_tracked: bool,
}

pub fn economics(conn: &Connection, org_id: &str, days: i64) -> Result<Economics> {
    let since = format!("-{days} days");
    let mut by = conn.prepare(
        "SELECT model, COUNT(*), COALESCE(SUM(cost_usd), 0), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0)
         FROM factory_run_metrics
         WHERE org_id = ?1 AND created_at >= datetime('now', ?2)
         GROUP BY model ORDER BY 3 DESC",
    )?;
    let by_model: Vec<ModelSpend> = by
        .query_map(params![org_id, since], |r| {
            Ok(ModelSpend { model: r.get(0)?, runs: r.get(1)?, cost_usd: r.get(2)?, input_tokens: r.get(3)?, output_tokens: r.get(4)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let runs = by_model.iter().map(|m| m.runs).sum();
    let cost_usd: f64 = by_model.iter().map(|m| m.cost_usd).sum();

    let mut tiers = conn.prepare(
        "SELECT json_extract(payload_json, '$.tier'), COUNT(*) FROM autonomous_agent_events
         WHERE org_id = ?1 AND kind = 'model.selected' AND created_at >= datetime('now', ?2)
         GROUP BY 1 ORDER BY 1",
    )?;
    let tier_choices: Vec<(String, i64)> = tiers
        .query_map(params![org_id, since], |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let tiered: i64 = tier_choices.iter().map(|(_, n)| n).sum();
    let frontier: i64 = tier_choices.iter().filter(|(t, _)| t == "frontier").map(|(_, n)| n).sum();
    let frontier_avoidance = (tiered > 0).then(|| (tiered - frontier) as f64 / tiered as f64);

    let proposed_changes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM autonomous_agent_output_links
         WHERE org_id = ?1 AND kind IN ('draft_pr','pull_request') AND created_at >= datetime('now', ?2)",
        params![org_id, since],
        |r| r.get(0),
    )?;
    Ok(Economics {
        days,
        runs,
        cost_usd,
        by_model,
        tier_choices,
        frontier_avoidance,
        proposed_changes,
        cost_per_proposed_change: (proposed_changes > 0).then(|| cost_usd / proposed_changes as f64),
        accepted_changes_tracked: false,
    })
}

/// A merge subject as the worker writes it: `owner/repo#number@sha`.
pub fn valid_merge_subject(subject: &str) -> bool {
    let Some((repo_pull, sha)) = subject.rsplit_once('@') else { return false };
    let Some((repo, number)) = repo_pull.rsplit_once('#') else { return false };
    sha.len() == 40
        && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && number.parse::<u64>().is_ok_and(|n| n > 0)
        && crate::automation::connectors::validate_repository(repo).is_ok()
}

/// The latest human verdict on an action for a subject, if a person decided:
/// `Some(true)` approved, `Some(false)` rejected.
pub fn latest_human_verdict(conn: &Connection, org_id: &str, subject: &str, action: &str) -> Result<Option<bool>> {
    let mut stmt = conn.prepare(
        "SELECT verdict FROM factory_decisions
         WHERE org_id = ?1 AND subject = ?2 AND action = ?3 AND source = 'human'
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![org_id, subject, action])?;
    Ok(match rows.next()? {
        Some(row) => Some(row.get::<_, String>(0)? == "allow"),
        None => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, factory_queries, migrations, queries};
    use crate::factory::contracts::{Action, ActionVerdict, Verdict, VerdictSource};

    fn setup() -> (Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "A").unwrap();
        (conn, org.id, user.id)
    }

    fn decide(conn: &Connection, org: &str, subject: &str, verdict: Verdict, source: VerdictSource) {
        factory_queries::record_decision(
            conn,
            org,
            &factory_queries::DecisionRecord {
                subject: subject.into(),
                action: Action::Merge,
                verdict: ActionVerdict { verdict, reason: "test".into(), source },
                policy_version: None,
                provider: None,
                model: None,
                confidence: None,
                inputs: serde_json::json!({}),
            },
        )
        .unwrap();
    }

    const SUBJECT: &str = "acme/app#7@0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn merge_subjects_must_name_a_repo_pull_and_full_sha() {
        assert!(valid_merge_subject(SUBJECT));
        assert!(!valid_merge_subject("acme/app#7@abc"));
        assert!(!valid_merge_subject("acme/app#0@0123456789abcdef0123456789abcdef01234567"));
        assert!(!valid_merge_subject("../x#7@0123456789abcdef0123456789abcdef01234567"));
        assert!(!valid_merge_subject("acme/app@0123456789abcdef0123456789abcdef01234567"));
    }

    #[test]
    fn a_held_merge_waits_until_a_person_decides_it() {
        let (conn, org, _) = setup();
        decide(&conn, &org, SUBJECT, Verdict::Hold, VerdictSource::Policy);
        let digest = human_digest(&conn, &org).unwrap();
        assert_eq!(digest.held_merges.len(), 1);
        assert_eq!(latest_human_verdict(&conn, &org, SUBJECT, "merge").unwrap(), None);
        // The person's decision is newer (even within the same second), so the
        // hold is no longer waiting.
        decide(&conn, &org, SUBJECT, Verdict::Allow, VerdictSource::Human);
        assert!(human_digest(&conn, &org).unwrap().held_merges.is_empty());
        assert_eq!(latest_human_verdict(&conn, &org, SUBJECT, "merge").unwrap(), Some(true));
        assert!(human_digest(&conn, "other-org").unwrap().held_merges.is_empty());
    }

    #[test]
    fn factory_tasks_in_the_digest_are_unstarted_labelled_tasks() {
        let (conn, org, user) = setup();
        let mut make = |title: &str, status: &str, label: Option<&str>| {
            let task = queries::create_task(
                &conn,
                &org,
                &user,
                &crate::models::types::CreateTaskRequest {
                    project: "app".into(),
                    title: title.into(),
                    description: None,
                    status: Some(status.into()),
                    priority: None,
                    due_date: None,
                    parent_id: None,
                    sprint_id: None,
                },
            )
            .unwrap();
            if let Some(label) = label {
                queries::add_task_label(&conn, &task.id, label).unwrap();
            }
        };
        make("waiting", "backlog", Some("factory"));
        make("in progress", "in_progress", Some("factory"));
        make("not factory", "todo", None);
        let digest = human_digest(&conn, &org).unwrap();
        let titles: Vec<&str> = digest.factory_tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["waiting"]);
    }

    #[test]
    fn economics_sums_spend_and_the_frontier_share() {
        let (conn, org, _) = setup();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        for (run, model, cost) in [("r1", "claude-opus-5", 1.0), ("r2", "claude-sonnet-5", 0.25), ("r3", "claude-sonnet-5", 0.25)] {
            conn.execute(
                "INSERT INTO factory_run_metrics (id, org_id, run_id, template_key, provider, model, cost_usd, outcome)
                 VALUES (?1, ?2, ?3, 'github_issue_resolver', 'claude-code', ?4, ?5, 'succeeded')",
                params![uuid::Uuid::new_v4().to_string(), org, run, model, cost],
            )
            .unwrap();
        }
        for (seq, tier) in [(1, "frontier"), (2, "standard"), (3, "standard"), (4, "cheap")] {
            conn.execute(
                "INSERT INTO autonomous_agent_events (id, org_id, run_id, sequence, kind, payload_json)
                 VALUES (?1, ?2, 'r1', ?3, 'model.selected', ?4)",
                params![uuid::Uuid::new_v4().to_string(), org, seq, serde_json::json!({"tier": tier}).to_string()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO autonomous_agent_output_links (id, org_id, run_id, kind, external_id) VALUES ('l1', ?1, 'r1', 'draft_pr', '12')",
            [&org],
        )
        .unwrap();
        let e = economics(&conn, &org, 30).unwrap();
        assert_eq!((e.runs, e.proposed_changes), (3, 1));
        assert!((e.cost_usd - 1.5).abs() < 1e-9);
        assert_eq!(e.by_model[0].model.as_deref(), Some("claude-opus-5"));
        assert_eq!(e.frontier_avoidance, Some(0.75));
        assert_eq!(e.cost_per_proposed_change, Some(1.5));
        assert!(!e.accepted_changes_tracked);
    }
}
