//! Factory operator queries (F3): what waits on a person, what the factory
//! costs, and the human verdict on one action. Every query is org-scoped.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

/// A merge the policy held for a person, not yet decided by one. Only the
/// latest head of each pull request is listed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeldMerge {
    pub subject: String,
    pub reason: String,
    pub source: String,
    pub created_at: String,
}

/// A merge a person approved: it merges once its soak re-runs every gate.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ApprovedMerge {
    pub subject: String,
    pub approved_at: String,
    /// When the soak re-checks and merges; `None` if no soak is pending (it
    /// already ran: merged or declined).
    pub merges_after: Option<String>,
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
    pub approved_merges: Vec<ApprovedMerge>,
    /// `None` when the viewer may not see agent runs.
    pub blocked_runs: Option<Vec<BlockedRun>>,
    pub factory_tasks: Vec<WaitingTask>,
    /// Shadow decisions without a human label (OD-5 needs them); `allow`
    /// verdicts are the ones the false-low-risk rate counts.
    pub unlabeled_shadow: i64,
    pub unlabeled_shadow_allows: i64,
}

const DIGEST_LIMIT: i64 = 50;

/// Statuses where a run stopped and a person has to decide what happens next.
const BLOCKED_STATUSES: &str = "'blocked_policy','blocked_runtime','dead_letter','budget_exhausted'";

/// `owner/repo#number@sha` → its parts, if well formed.
pub fn parse_merge_subject(subject: &str) -> Option<(String, i64, String)> {
    let (repo_pull, sha) = subject.rsplit_once('@')?;
    let (repo, number) = repo_pull.rsplit_once('#')?;
    let number: i64 = number.parse().ok().filter(|n| *n > 0)?;
    let full_sha = sha.len() == 40 && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (full_sha && crate::automation::connectors::validate_repository(repo).is_ok())
        .then(|| (repo.to_string(), number, sha.to_string()))
}

/// A merge subject as the worker writes it: `owner/repo#number@sha`.
pub fn valid_merge_subject(subject: &str) -> bool {
    parse_merge_subject(subject).is_some()
}

/// What the digest may show a viewer.
pub struct DigestScope<'a> {
    /// `None` for super users (every project), else the viewer's id.
    pub viewer: Option<&'a str>,
    pub include_runs: bool,
}

pub fn human_digest(conn: &Connection, org_id: &str, scope: &DigestScope<'_>) -> Result<HumanDigest> {
    // The latest merge decision of each pull request (any head): a newer head
    // or a person's decision replaces an older hold.
    let mut latest = conn.prepare(
        "WITH ranked AS (
             SELECT subject, verdict, source, reason, created_at,
                    ROW_NUMBER() OVER (
                        PARTITION BY substr(subject, 1, instr(subject, '@') - 1)
                        ORDER BY created_at DESC, rowid DESC) AS rn
             FROM factory_decisions
             WHERE org_id = ?1 AND action = 'merge' AND created_at >= datetime('now', '-14 days'))
         SELECT subject, verdict, source, reason, created_at FROM ranked
         WHERE rn = 1 ORDER BY created_at DESC LIMIT ?2",
    )?;
    let rows: Vec<(String, String, String, String, String)> = latest
        .query_map(params![org_id, DIGEST_LIMIT * 2], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut held_merges = Vec::new();
    let mut approved_merges = Vec::new();
    for (subject, verdict, source, reason, created_at) in rows {
        match (verdict.as_str(), source.as_str()) {
            ("hold", "policy" | "decision_model") => {
                held_merges.push(HeldMerge { subject, reason, source, created_at })
            }
            ("allow", "human") => {
                let merges_after = match parse_merge_subject(&subject) {
                    Some((repo, number, sha)) => conn
                        .query_row(
                            "SELECT due_at FROM factory_merge_soaks
                             WHERE org_id = ?1 AND repository = ?2 AND pull_number = ?3 AND head_sha = ?4",
                            params![org_id, repo, number, sha],
                            |r| r.get(0),
                        )
                        .ok(),
                    None => None,
                };
                approved_merges.push(ApprovedMerge { subject, approved_at: created_at, merges_after })
            }
            _ => {}
        }
    }

    let blocked_runs = if scope.include_runs {
        let mut blocked = conn.prepare(&format!(
            "SELECT r.id, d.name, d.template_key, r.status,
                    (SELECT CAST(COALESCE(json_extract(e.payload_json, '$.code'), json_extract(e.payload_json, '$.reason')) AS TEXT)
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
        let runs = blocked
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
        Some(runs)
    } else {
        None
    };

    // Same visibility as the task list: a viewer sees tasks of the projects
    // they can see.
    let filters = crate::db::queries::TaskListFilters {
        label: Some(crate::factory::intake::INTAKE_LABEL.to_string()),
        ..Default::default()
    };
    let factory_tasks = crate::db::queries::list_tasks(conn, org_id, scope.viewer, &filters, 200, 0)?
        .into_iter()
        .filter(|t| matches!(t.status.as_str(), "backlog" | "todo"))
        .take(DIGEST_LIMIT as usize)
        .map(|t| WaitingTask { id: t.id, project: t.project, title: t.title, status: t.status, created_at: t.created_at })
        .collect();

    let (unlabeled_shadow, unlabeled_shadow_allows): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(verdict = 'allow'), 0) FROM factory_shadow_decisions
         WHERE org_id = ?1 AND human_label IS NULL AND outcome IN ('pending','clean','high_risk')",
        [org_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    Ok(HumanDigest {
        held_merges,
        approved_merges,
        blocked_runs,
        factory_tasks,
        unlabeled_shadow,
        unlabeled_shadow_allows,
    })
}

/// The hold a person may lift on this exact head: the latest merge decision of
/// the subject, when the policy or the decision model held it. Returns the run
/// and required checks the soak needs (absent on decisions recorded before
/// they were kept).
pub fn held_for_a_person(conn: &Connection, org_id: &str, subject: &str) -> Result<Option<(Option<String>, Vec<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT verdict, source, inputs_json FROM factory_decisions
         WHERE org_id = ?1 AND subject = ?2 AND action = 'merge'
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![org_id, subject])?;
    let Some(row) = rows.next()? else { return Ok(None) };
    let (verdict, source, inputs): (String, String, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
    if verdict != "hold" || !matches!(source.as_str(), "policy" | "decision_model") {
        return Ok(None);
    }
    let inputs: serde_json::Value = serde_json::from_str(&inputs).unwrap_or_default();
    let run_id = inputs.get("run_id").and_then(|v| v.as_str()).map(str::to_string);
    let required = inputs
        .get("required_checks")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    Ok(Some((run_id, required)))
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
        "SELECT json_extract(payload_json, '$.tier'), COUNT(DISTINCT run_id) FROM autonomous_agent_events
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

/// The latest human decision on an action for a subject, if a person decided:
/// its id and whether it approved.
pub fn latest_human_decision(conn: &Connection, org_id: &str, subject: &str, action: &str) -> Result<Option<(String, bool)>> {
    let mut stmt = conn.prepare(
        "SELECT id, verdict FROM factory_decisions
         WHERE org_id = ?1 AND subject = ?2 AND action = ?3 AND source = 'human'
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![org_id, subject, action])?;
    Ok(match rows.next()? {
        Some(row) => Some((row.get(0)?, row.get::<_, String>(1)? == "allow")),
        None => None,
    })
}

/// [`latest_human_decision`] without the id.
pub fn latest_human_verdict(conn: &Connection, org_id: &str, subject: &str, action: &str) -> Result<Option<bool>> {
    Ok(latest_human_decision(conn, org_id, subject, action)?.map(|(_, approved)| approved))
}

/// Soak a person's approval waits before the worker re-checks and merges
/// (plan D17, same as an automatic allow).
pub const APPROVAL_SOAK_SECONDS: i64 = 600;

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
                inputs: serde_json::json!({"run_id": "run-1", "required_checks": ["build"]}),
            },
        )
        .unwrap();
    }

    const SUBJECT: &str = "acme/app#7@0123456789abcdef0123456789abcdef01234567";
    const NEWER: &str = "acme/app#7@fedcba9876543210fedcba9876543210fedcba98";

    fn all(conn: &Connection, org: &str) -> HumanDigest {
        human_digest(conn, org, &DigestScope { viewer: None, include_runs: true }).unwrap()
    }

    #[test]
    fn merge_subjects_must_name_a_repo_pull_and_full_sha() {
        assert_eq!(parse_merge_subject(SUBJECT), Some(("acme/app".into(), 7, SUBJECT[11..].into())));
        assert!(!valid_merge_subject("acme/app#7@abc"));
        assert!(!valid_merge_subject("acme/app#0@0123456789abcdef0123456789abcdef01234567"));
        assert!(!valid_merge_subject("../x#7@0123456789abcdef0123456789abcdef01234567"));
        assert!(!valid_merge_subject("acme/app@0123456789abcdef0123456789abcdef01234567"));
    }

    #[test]
    fn only_a_policy_hold_can_be_lifted_and_it_carries_the_soak_inputs() {
        let (conn, org, _) = setup();
        assert_eq!(held_for_a_person(&conn, &org, SUBJECT).unwrap(), None);
        decide(&conn, &org, SUBJECT, Verdict::Hold, VerdictSource::Policy);
        assert_eq!(
            held_for_a_person(&conn, &org, SUBJECT).unwrap(),
            Some((Some("run-1".into()), vec!["build".into()]))
        );
        // A policy `never` (deny) or a floor is not a person's to lift.
        decide(&conn, &org, SUBJECT, Verdict::Deny, VerdictSource::Policy);
        assert_eq!(held_for_a_person(&conn, &org, SUBJECT).unwrap(), None);
        decide(&conn, &org, SUBJECT, Verdict::Hold, VerdictSource::Floor);
        assert_eq!(held_for_a_person(&conn, &org, SUBJECT).unwrap(), None);
    }

    #[test]
    fn the_digest_lists_the_latest_head_of_each_pr_held_or_approved() {
        let (conn, org, _) = setup();
        decide(&conn, &org, SUBJECT, Verdict::Hold, VerdictSource::Policy);
        decide(&conn, &org, NEWER, Verdict::Hold, VerdictSource::Policy);
        let digest = all(&conn, &org);
        assert_eq!(digest.held_merges.len(), 1, "an older head of the same PR is not listed");
        assert_eq!(digest.held_merges[0].subject, NEWER);
        // A person approves the latest head: it moves to "approved".
        decide(&conn, &org, NEWER, Verdict::Allow, VerdictSource::Human);
        let digest = all(&conn, &org);
        assert!(digest.held_merges.is_empty());
        assert_eq!(digest.approved_merges.len(), 1);
        assert_eq!(digest.approved_merges[0].merges_after, None, "no soak recorded here");
        assert_eq!(latest_human_verdict(&conn, &org, NEWER, "merge").unwrap(), Some(true));
        assert!(all(&conn, "other-org").held_merges.is_empty());
        // Runs are hidden from viewers who may not read them.
        let scoped = human_digest(&conn, &org, &DigestScope { viewer: None, include_runs: false }).unwrap();
        assert_eq!(scoped.blocked_runs, None);
    }

    #[test]
    fn factory_tasks_in_the_digest_are_unstarted_labelled_tasks() {
        let (conn, org, user) = setup();
        let make = |title: &str, status: &str, label: Option<&str>| {
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
        let digest = all(&conn, &org);
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
        // One run per choice, plus a retry of r1 that must not count twice.
        for (run, seq, tier) in [("r1", 1, "frontier"), ("r1", 2, "frontier"), ("r2", 1, "standard"), ("r3", 1, "standard"), ("r4", 1, "cheap")] {
            conn.execute(
                "INSERT INTO autonomous_agent_events (id, org_id, run_id, sequence, kind, payload_json)
                 VALUES (?1, ?2, ?3, ?4, 'model.selected', ?5)",
                params![uuid::Uuid::new_v4().to_string(), org, run, seq, serde_json::json!({"tier": tier}).to_string()],
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
