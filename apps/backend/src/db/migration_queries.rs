//! Knowledge-migration data layer: runs, candidate staging, and the human review gate.
//!
//! Everything here reads and writes the tables created by migration `run_v60`.
//! Kept separate from `queries.rs` for the same reason `usage_queries.rs` is:
//! that file is already 19k lines and shared by every other domain.
//!
//! # The two invariants this module exists to hold
//!
//! 1. **Nothing reaches a destination without a human approval.** Staging writes
//!    candidates and nothing else; only `status = 'approved'` is eligible for a
//!    commit, and only `apply_review_action` can set that status.
//! 2. **A reviewer always acts on a version they have seen.** Every action
//!    carries `expected_version`; a mismatch is recorded as `stale_version` and
//!    rejected, so two reviewers working the same queue cannot silently
//!    overwrite each other.
//!
//! Visibility reuses the existing predicates — `project_visibility` (the view
//! `run_v?` created for projects) and [`queries::user_can_view_client`]. There is
//! deliberately no second definition of "who may see this": client isolation has
//! one canonical implementation and duplicating it is how leaks ship.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::db::queries;
use crate::models::types::{
    CandidateInput, DestinationKind, MigrationCandidate, MigrationRun, ReviewVerdict, RunReport,
    RunReportEntry, SourceKind, StageResult,
};

// ── Row mapping ──────────────────────────────────────────────────────────────

fn json_or_empty(raw: String) -> serde_json::Value {
    serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
}

const RUN_COLUMNS: &str = "id, org_id, client_id, project_id, source_kind, status, source_ref, \
                           runner_version, attestation, created_by, created_at, updated_at";

fn map_run(row: &rusqlite::Row) -> rusqlite::Result<MigrationRun> {
    let source_kind: String = row.get(4)?;
    Ok(MigrationRun {
        id: row.get(0)?,
        org_id: row.get(1)?,
        client_id: row.get(2)?,
        project_id: row.get(3)?,
        // The CHECK on the column already restricts this to the accepted set, so
        // an unparseable value means the database was edited by hand. Falling
        // back to `Noop` would hide that; there is no sane fallback, so panic
        // is not right either — we surface it as Noop only because the type is
        // infallible here, and the migration guarantees it cannot happen.
        source_kind: source_kind.parse::<SourceKind>().unwrap_or(SourceKind::Noop),
        status: row.get(5)?,
        source_ref: row.get(6)?,
        runner_version: row.get(7)?,
        attestation: json_or_empty(row.get::<_, String>(8)?),
        created_by: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

const CANDIDATE_COLUMNS: &str =
    "id, run_id, source_identity, destination_kind, destination_hint, content, source_excerpt, \
     confidence, normalized_metadata, attestation, provenance_kind, status, version, \
     indexed_at, created_at, updated_at";

fn map_candidate(row: &rusqlite::Row) -> rusqlite::Result<MigrationCandidate> {
    let kind: String = row.get(3)?;
    Ok(MigrationCandidate {
        id: row.get(0)?,
        run_id: row.get(1)?,
        source_identity: row.get(2)?,
        destination_kind: kind
            .parse::<DestinationKind>()
            .unwrap_or(DestinationKind::Memory),
        destination_hint: json_or_empty(row.get::<_, String>(4)?),
        content: row.get(5)?,
        source_excerpt: row.get(6)?,
        confidence: row.get(7)?,
        normalized_metadata: json_or_empty(row.get::<_, String>(8)?),
        attestation: json_or_empty(row.get::<_, String>(9)?),
        provenance_kind: row.get(10)?,
        status: row.get(11)?,
        version: row.get(12)?,
        indexed_at: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

// ── Runs ─────────────────────────────────────────────────────────────────────

pub struct NewRun<'a> {
    pub org_id: &'a str,
    pub client_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub source_kind: SourceKind,
    pub source_ref: Option<&'a str>,
    pub runner_version: Option<&'a str>,
    pub attestation: serde_json::Value,
    pub created_by: &'a str,
}

/// Creates a run. The org/client and org/project coherence triggers installed by
/// `run_v60` are what actually reject a cross-org reference — this function does
/// not re-check them, because a check in application code that duplicates a
/// database constraint is one more place to forget.
pub fn create_run(conn: &Connection, new: &NewRun) -> Result<MigrationRun> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO migration_runs
            (id, org_id, client_id, project_id, source_kind, source_ref, runner_version,
             attestation, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            new.org_id,
            new.client_id,
            new.project_id,
            new.source_kind.as_str(),
            new.source_ref,
            new.runner_version,
            new.attestation.to_string(),
            new.created_by,
        ],
    )?;
    get_run(conn, new.org_id, &id)?
        .ok_or_else(|| anyhow::anyhow!("run vanished immediately after insert"))
}

pub fn get_run(conn: &Connection, org_id: &str, run_id: &str) -> Result<Option<MigrationRun>> {
    let sql = format!("SELECT {RUN_COLUMNS} FROM migration_runs WHERE org_id = ?1 AND id = ?2");
    Ok(conn
        .query_row(&sql, rusqlite::params![org_id, run_id], map_run)
        .optional()?)
}

/// Can `viewer` see this run? `None` = super_user, no restriction.
///
/// A run with neither client nor project is internal organization work and is
/// visible to any member of the org. A run scoped to a client or a project is
/// visible only to someone who can see that client or that project.
pub fn user_can_view_run(
    conn: &Connection,
    org_id: &str,
    run: &MigrationRun,
    viewer_user_id: Option<&str>,
) -> Result<bool> {
    let Some(vid) = viewer_user_id else {
        return Ok(true);
    };
    if let Some(client_id) = run.client_id.as_deref() {
        if queries::user_can_view_client(conn, org_id, client_id, Some(vid))? {
            return Ok(true);
        }
    }
    if let Some(project_id) = run.project_id.as_deref() {
        let visible: i64 = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM project_visibility
                             WHERE org_id = ?1 AND project_id = ?2 AND user_id = ?3)",
            rusqlite::params![org_id, project_id, vid],
            |r| r.get(0),
        )?;
        if visible != 0 {
            return Ok(true);
        }
    }
    // Neither scope set → internal org work.
    Ok(run.client_id.is_none() && run.project_id.is_none())
}

pub fn list_runs_visible(
    conn: &Connection,
    org_id: &str,
    viewer_user_id: Option<&str>,
    client_filter: Option<&str>,
    source_filter: Option<SourceKind>,
    limit: i64,
) -> Result<Vec<MigrationRun>> {
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM migration_runs
         WHERE org_id = ?1
           AND (?2 IS NULL OR client_id = ?2)
           AND (?3 IS NULL OR source_kind = ?3)
         ORDER BY created_at DESC, id DESC
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            rusqlite::params![
                org_id,
                client_filter,
                source_filter.map(|s| s.as_str()),
                limit
            ],
            map_run,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut out = Vec::with_capacity(rows.len());
    for run in rows {
        if user_can_view_run(conn, org_id, &run, viewer_user_id)? {
            out.push(run);
        }
    }
    Ok(out)
}

pub fn set_run_status(conn: &Connection, org_id: &str, run_id: &str, status: &str) -> Result<bool> {
    let n = conn.execute(
        "UPDATE migration_runs SET status = ?3, updated_at = datetime('now')
         WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, run_id, status],
    )?;
    Ok(n > 0)
}

/// Cancels everything still pending. Committed candidates are deliberately left
/// alone: they are already in a destination, and rewriting their status would
/// make the run report disagree with reality.
pub fn cancel_run(conn: &Connection, org_id: &str, run_id: &str) -> Result<usize> {
    // A completed run has nothing pending and relabelling it `cancelled` would
    // make the status lie about what happened to its candidates.
    if let Some(run) = get_run(conn, org_id, run_id)? {
        if run.status == "completed" {
            anyhow::bail!("run_already_completed");
        }
    }
    let cancelled = conn.execute(
        "UPDATE migration_candidates
            SET status = 'cancelled', updated_at = datetime('now')
          WHERE run_id = ?2
            AND status IN ('staged', 'approved')
            AND EXISTS (SELECT 1 FROM migration_runs r WHERE r.id = ?2 AND r.org_id = ?1)",
        rusqlite::params![org_id, run_id],
    )?;
    set_run_status(conn, org_id, run_id, "cancelled")?;
    Ok(cancelled)
}

/// Hard-deletes a run and everything that cascades from it — its candidates and
/// outcomes. This is the cleanup path for runs that never committed anything
/// (an aborted scan, a test run); the audit trail of committed knowledge is
/// protected by the database, not by this function.
///
/// `migration_provenance.candidate_id` references `migration_candidates` with
/// `ON DELETE RESTRICT`, and candidates cascade from the run. So the moment a
/// run has one committed candidate, deleting the run tries to delete a candidate
/// a provenance row still points at, the RESTRICT aborts the whole statement,
/// and this returns `run_has_provenance`. Cancel such a run instead — there is
/// deliberately no way to erase provenance from here.
///
/// Returns `Ok(true)` when a run was deleted, `Ok(false)` when none matched this
/// org (a not-found the caller turns into 404).
pub fn delete_run(conn: &Connection, org_id: &str, run_id: &str) -> Result<bool> {
    match conn.execute(
        "DELETE FROM migration_runs WHERE id = ?2 AND org_id = ?1",
        rusqlite::params![org_id, run_id],
    ) {
        Ok(n) => Ok(n > 0),
        // The provenance RESTRICT firing through the candidate cascade. Named so
        // the handler can answer with "cancel it instead" rather than a 500.
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            anyhow::bail!("run_has_provenance")
        }
        Err(e) => Err(e.into()),
    }
}

// ── Candidate staging ────────────────────────────────────────────────────────

/// Has this source identity already been rejected for this destination, in any
/// run of this org? A rejection is a decision, and re-proposing it unchanged
/// would make the reviewer answer the same question forever.
fn previously_rejected(
    conn: &Connection,
    org_id: &str,
    identity: &str,
    kind: DestinationKind,
) -> Result<bool> {
    let found: i64 = conn.query_row(
        "SELECT EXISTS (
             SELECT 1 FROM migration_candidates c
             JOIN migration_runs r ON r.id = c.run_id
             WHERE r.org_id = ?1
               AND c.source_identity = ?2
               AND c.destination_kind = ?3
               AND c.status = 'rejected')",
        rusqlite::params![org_id, identity, kind.as_str()],
        |r| r.get(0),
    )?;
    Ok(found != 0)
}

fn already_committed(
    conn: &Connection,
    org_id: &str,
    identity: &str,
    kind: DestinationKind,
) -> Result<bool> {
    let found: i64 = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM migration_provenance
                         WHERE org_id = ?1 AND source_identity = ?2 AND destination_kind = ?3)",
        rusqlite::params![org_id, identity, kind.as_str()],
        |r| r.get(0),
    )?;
    Ok(found != 0)
}

/// Stages a batch. Every candidate gets its own verdict; a malformed or
/// duplicate one is reported and skipped, never fatal. One bad row out of four
/// hundred must not discard the other three hundred and ninety-nine.
pub fn stage_candidates(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    inputs: &[CandidateInput],
) -> Result<Vec<StageResult>> {
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        let identity = input.source_identity.trim();
        if identity.is_empty() {
            out.push(StageResult::Rejected {
                reason: "empty_source_identity".to_string(),
            });
            continue;
        }
        if input.content.trim().is_empty() {
            out.push(StageResult::Rejected {
                reason: "empty_content".to_string(),
            });
            continue;
        }
        if already_committed(conn, org_id, identity, input.destination_kind)? {
            out.push(StageResult::Skipped {
                reason: "already_committed".to_string(),
            });
            continue;
        }
        if previously_rejected(conn, org_id, identity, input.destination_kind)? {
            out.push(StageResult::Skipped {
                reason: "previously_rejected".to_string(),
            });
            continue;
        }

        let id = Uuid::new_v4().to_string();
        let provenance = input
            .provenance_kind
            .as_deref()
            .filter(|p| *p == "verified_manifest" || *p == "client_attested")
            .unwrap_or("client_attested");

        let inserted = conn.execute(
            "INSERT INTO migration_candidates
                (id, run_id, source_identity, destination_kind, destination_hint, content,
                 source_excerpt, confidence, normalized_metadata, provenance_kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                id,
                run_id,
                identity,
                input.destination_kind.as_str(),
                if input.destination_hint.is_null() {
                    "{}".to_string()
                } else {
                    input.destination_hint.to_string()
                },
                input.content,
                input.source_excerpt,
                input.confidence,
                if input.normalized_metadata.is_null() {
                    "{}".to_string()
                } else {
                    input.normalized_metadata.to_string()
                },
                provenance,
            ],
        );

        match inserted {
            Ok(_) => out.push(StageResult::Staged { id }),
            // `UNIQUE(run_id, source_identity)` — the same source twice in one run.
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
                out.push(StageResult::Rejected {
                    reason: "duplicate_source_identity_in_run".to_string(),
                })
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(out)
}

pub fn list_candidates(
    conn: &Connection,
    run_id: &str,
    status_filter: Option<&str>,
    destination_filter: Option<DestinationKind>,
    limit: i64,
) -> Result<Vec<MigrationCandidate>> {
    let sql = format!(
        "SELECT {CANDIDATE_COLUMNS} FROM migration_candidates
         WHERE run_id = ?1
           AND (?2 IS NULL OR status = ?2)
           AND (?3 IS NULL OR destination_kind = ?3)
         ORDER BY confidence DESC NULLS LAST, id
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            rusqlite::params![
                run_id,
                status_filter,
                destination_filter.map(|d| d.as_str()),
                limit
            ],
            map_candidate,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_candidate(conn: &Connection, candidate_id: &str) -> Result<Option<MigrationCandidate>> {
    let sql =
        format!("SELECT {CANDIDATE_COLUMNS} FROM migration_candidates WHERE id = ?1");
    Ok(conn
        .query_row(&sql, [candidate_id], map_candidate)
        .optional()?)
}

// ── Review ───────────────────────────────────────────────────────────────────

/// Why a review action did not take effect. Every variant is also written to
/// `migration_review_actions`, because a refused action is evidence too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutcome {
    Applied { new_version: i64 },
    StaleVersion { actual_version: i64 },
    NotFound,
    NotReviewable { status: String },
}

/// One row of the append-only review trail. Grouped into a struct rather than
/// nine positional arguments — at that width a caller silently swapping two
/// `Option<&str>` still compiles, and the trail is evidence.
struct ActionRecord<'a> {
    run_id: &'a str,
    candidate_id: Option<&'a str>,
    actor_id: &'a str,
    action: &'a str,
    expected: Option<i64>,
    resulting: Option<i64>,
    reason: Option<&'a str>,
    correlation: Option<&'a str>,
}

fn record_action(conn: &Connection, rec: &ActionRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO migration_review_actions
            (id, run_id, candidate_id, actor_id, action, expected_version, resulting_version,
             reason, request_correlation_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            rec.run_id,
            rec.candidate_id,
            rec.actor_id,
            rec.action,
            rec.expected,
            rec.resulting,
            rec.reason,
            rec.correlation,
        ],
    )?;
    Ok(())
}

/// Records a permission denial against the review trail. Called by the API layer
/// when a caller lacks `migration:review` — the refusal is part of the history,
/// not just an HTTP status that disappears into a log.
pub fn record_permission_denied(
    conn: &Connection,
    run_id: &str,
    candidate_id: Option<&str>,
    actor_id: &str,
    correlation: Option<&str>,
) -> Result<()> {
    record_action(
        conn,
        &ActionRecord {
            run_id,
            candidate_id,
            actor_id,
            action: "permission_denied",
            expected: None,
            resulting: None,
            reason: None,
            correlation,
        },
    )
}

/// Applies one review decision under optimistic concurrency.
///
/// The `WHERE version = ?expected` on the UPDATE is the actual guard: two
/// reviewers racing on the same candidate both read version 1, and only the
/// first UPDATE matches. The second gets zero rows and is recorded as
/// `stale_version` with both numbers, so the trail shows what happened rather
/// than silently taking the last write.
/// One reviewer decision. `expected_version` is mandatory by construction: the
/// optimistic-concurrency guard only works if every caller declares the version
/// it acted on.
pub struct ReviewRequest<'a> {
    pub run_id: &'a str,
    pub candidate_id: &'a str,
    pub actor_id: &'a str,
    pub verdict: ReviewVerdict,
    pub expected_version: i64,
    pub reason: Option<&'a str>,
    pub correlation: Option<&'a str>,
}

pub fn apply_review_action(conn: &Connection, req: &ReviewRequest) -> Result<ReviewOutcome> {
    let (run_id, candidate_id, actor_id) = (req.run_id, req.candidate_id, req.actor_id);
    let (expected_version, reason, correlation) =
        (req.expected_version, req.reason, req.correlation);

    let Some(candidate) = get_candidate(conn, candidate_id)? else {
        return Ok(ReviewOutcome::NotFound);
    };
    if candidate.run_id != run_id {
        return Ok(ReviewOutcome::NotFound);
    }

    let new_status = match req.verdict {
        ReviewVerdict::Approved => "approved",
        ReviewVerdict::Rejected => "rejected",
        ReviewVerdict::Restaged => "staged",
    };

    // A committed candidate is done; re-deciding it would desynchronize the
    // review trail from what is actually in the destination.
    if matches!(candidate.status.as_str(), "committed" | "committing") {
        record_action(
            conn,
            &ActionRecord {
                run_id,
                candidate_id: Some(candidate_id),
                actor_id,
                action: "not_approved",
                expected: Some(expected_version),
                resulting: Some(candidate.version),
                reason: Some("candidate is already committed"),
                correlation,
            },
        )?;
        return Ok(ReviewOutcome::NotReviewable {
            status: candidate.status,
        });
    }

    let updated = conn.execute(
        "UPDATE migration_candidates
            SET status = ?3, version = version + 1, updated_at = datetime('now')
          WHERE id = ?1 AND version = ?2",
        rusqlite::params![candidate_id, expected_version, new_status],
    )?;

    if updated == 0 {
        record_action(
            conn,
            &ActionRecord {
                run_id,
                candidate_id: Some(candidate_id),
                actor_id,
                action: "stale_version",
                expected: Some(expected_version),
                resulting: Some(candidate.version),
                reason,
                correlation,
            },
        )?;
        return Ok(ReviewOutcome::StaleVersion {
            actual_version: candidate.version,
        });
    }

    let new_version = expected_version + 1;
    let action = match req.verdict {
        ReviewVerdict::Approved => "approved",
        ReviewVerdict::Rejected => "rejected",
        ReviewVerdict::Restaged => "restaged",
    };
    record_action(
        conn,
        &ActionRecord {
            run_id,
            candidate_id: Some(candidate_id),
            actor_id,
            action,
            expected: Some(expected_version),
            resulting: Some(new_version),
            reason,
            correlation,
        },
    )?;
    Ok(ReviewOutcome::Applied { new_version })
}

/// Batch approval is allowed only when every candidate carries verified
/// provenance. A `client_attested` candidate is one whose truthfulness rests on
/// somebody's word; those get read one at a time.
pub fn batch_contains_attested(conn: &Connection, candidate_ids: &[String]) -> Result<Vec<String>> {
    let mut attested = Vec::new();
    for id in candidate_ids {
        if let Some(c) = get_candidate(conn, id)? {
            if c.provenance_kind == "client_attested" {
                attested.push(c.id);
            }
        }
    }
    Ok(attested)
}

// ── Reporting ────────────────────────────────────────────────────────────────

/// Counts plus a reason for every candidate that did not reach its destination.
/// A report that only counts is useless during an incident: the question is
/// always "why is that one not in".
pub fn run_report(conn: &Connection, run_id: &str) -> Result<RunReport> {
    let mut report = RunReport {
        run_id: run_id.to_string(),
        ..Default::default()
    };

    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) FROM migration_candidates WHERE run_id = ?1 GROUP BY status",
    )?;
    for row in stmt.query_map([run_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (status, n) = row?;
        let n = n as usize;
        match status.as_str() {
            "staged" => report.staged = n,
            "approved" => report.approved = n,
            "rejected" => report.rejected = n,
            "committed" => report.committed = n,
            "skipped" => report.skipped = n,
            "failed" => report.failed = n,
            _ => {}
        }
    }

    report.pending_index = conn.query_row(
        "SELECT COUNT(*) FROM migration_candidates
          WHERE run_id = ?1 AND status = 'committed' AND indexed_at IS NULL
            AND destination_kind = 'memory'",
        [run_id],
        |r| r.get::<_, i64>(0),
    )? as usize;

    // The latest outcome per candidate carries the error code; the candidate row
    // carries the status. Joining them is what turns "12 skipped" into twelve
    // answerable questions.
    let mut stmt = conn.prepare(
        "SELECT c.id, c.source_identity, c.destination_kind, c.status,
                (SELECT o.error_code FROM migration_outcomes o
                  WHERE o.candidate_id = c.id
                  ORDER BY o.created_at DESC, o.id DESC LIMIT 1)
           FROM migration_candidates c
          WHERE c.run_id = ?1 AND c.status <> 'committed'
          ORDER BY c.status, c.id",
    )?;
    report.outcomes = stmt
        .query_map([run_id], |r| {
            let kind: String = r.get(2)?;
            Ok(RunReportEntry {
                candidate_id: r.get(0)?,
                source_identity: r.get(1)?,
                destination_kind: kind
                    .parse::<DestinationKind>()
                    .unwrap_or(DestinationKind::Memory),
                status: r.get(3)?,
                reason: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    /// org1 with two clients, one project per client, and three users:
    /// `admin` (sees everything), `dev_a` (member of client A only),
    /// `dev_b` (member of client B only).
    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'U2S', 'u2s');
             INSERT INTO organizations (id, name, slug) VALUES ('org2', 'Other', 'other');
             INSERT INTO users (id, org_id, email, name) VALUES ('admin', 'org1', 'a@u2s.com', 'Admin');
             INSERT INTO users (id, org_id, email, name) VALUES ('dev_a', 'org1', 'a@dev.com', 'Dev A');
             INSERT INTO users (id, org_id, email, name) VALUES ('dev_b', 'org1', 'b@dev.com', 'Dev B');
             INSERT INTO clients (id, org_id, name, slug) VALUES ('cl_a', 'org1', 'Acme', 'acme');
             INSERT INTO clients (id, org_id, name, slug) VALUES ('cl_b', 'org1', 'Beta', 'beta');
             INSERT INTO clients (id, org_id, name, slug) VALUES ('cl_other', 'org2', 'Foreign', 'foreign');
             INSERT INTO client_members (client_id, user_id, role) VALUES ('cl_a', 'dev_a', 'member');
             INSERT INTO client_members (client_id, user_id, role) VALUES ('cl_b', 'dev_b', 'member');
             INSERT INTO projects (id, org_id, name, client_id) VALUES ('p_a', 'org1', 'acme-billing', 'cl_a');
             INSERT INTO projects (id, org_id, name) VALUES ('p_other_org', 'org2', 'foreign-proj');",
        )
        .unwrap();
        conn
    }

    fn run_for(conn: &Connection, client: Option<&str>, project: Option<&str>) -> MigrationRun {
        create_run(
            conn,
            &NewRun {
                org_id: "org1",
                client_id: client,
                project_id: project,
                source_kind: SourceKind::RepoDocs,
                source_ref: Some("./"),
                runner_version: Some("2.1.233"),
                attestation: serde_json::json!({}),
                created_by: "admin",
            },
        )
        .unwrap()
    }

    fn input(identity: &str, kind: DestinationKind) -> CandidateInput {
        CandidateInput {
            source_identity: identity.to_string(),
            destination_kind: kind,
            content: "body".to_string(),
            destination_hint: serde_json::json!({}),
            source_excerpt: Some("quoted from the source".to_string()),
            confidence: Some(0.8),
            normalized_metadata: serde_json::json!({}),
            provenance_kind: None,
        }
    }

    /// The reported bug: a committed memory must carry the run's project, not
    /// fall back to the org default. Memories key on the project *name*, so this
    /// exercises the id→name resolution the fix added.
    #[test]
    fn a_committed_memory_links_to_the_runs_project() {
        let conn = setup();
        let run = run_for(&conn, Some("cl_a"), Some("p_a"));
        stage_candidates(&conn, "org1", &run.id, &[input("src:m", DestinationKind::Memory)]).unwrap();
        let candidate = list_candidates(&conn, &run.id, None, None, 10)
            .unwrap()
            .into_iter()
            .find(|c| c.destination_kind == DestinationKind::Memory)
            .expect("the memory candidate is staged");

        let memory_id = write_destination(&conn, &run, &candidate).unwrap();

        let project_id: Option<String> = conn
            .query_row(
                "SELECT project_id FROM memories WHERE id = ?1",
                [&memory_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            project_id.as_deref(),
            Some("p_a"),
            "the committed memory inherits the run's routed project"
        );
    }

    /// An internal run (no project) must still commit an org-shared memory —
    /// the fallback must not invent a project where the run had none.
    #[test]
    fn a_committed_memory_from_an_internal_run_stays_org_shared() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run.id, &[input("src:m", DestinationKind::Memory)]).unwrap();
        let candidate = list_candidates(&conn, &run.id, None, None, 10)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let memory_id = write_destination(&conn, &run, &candidate).unwrap();

        let project_id: Option<String> = conn
            .query_row(
                "SELECT project_id FROM memories WHERE id = ?1",
                [&memory_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(project_id, None, "no run project means an org-shared memory");
    }

    // ── T-05: runs ───────────────────────────────────────────────────────────

    /// The coherence trigger, not application code, is what stops this.
    #[test]
    fn create_run_rejects_project_from_other_org() {
        let conn = setup();
        let err = create_run(
            &conn,
            &NewRun {
                org_id: "org1",
                client_id: None,
                project_id: Some("p_other_org"),
                source_kind: SourceKind::RepoDocs,
                source_ref: None,
                runner_version: None,
                attestation: serde_json::json!({}),
                created_by: "admin",
            },
        );
        assert!(err.is_err(), "a project of another org must be rejected");
    }

    #[test]
    fn create_run_rejects_client_from_other_org() {
        let conn = setup();
        let err = create_run(
            &conn,
            &NewRun {
                org_id: "org1",
                client_id: Some("cl_other"),
                project_id: None,
                source_kind: SourceKind::RepoDocs,
                source_ref: None,
                runner_version: None,
                attestation: serde_json::json!({}),
                created_by: "admin",
            },
        );
        assert!(err.is_err(), "a client of another org must be rejected");
    }

    #[test]
    fn create_run_accepts_null_client_as_internal() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        assert!(run.client_id.is_none(), "NULL client means internal work");
        assert_eq!(run.status, "staging");
        assert_eq!(run.source_kind, SourceKind::RepoDocs);
        // Internal work is visible to any member of the org.
        assert!(user_can_view_run(&conn, "org1", &run, Some("dev_b")).unwrap());
    }

    #[test]
    fn list_runs_hides_other_clients() {
        let conn = setup();
        let _a = run_for(&conn, Some("cl_a"), Some("p_a"));
        let _b = run_for(&conn, Some("cl_b"), None);

        let as_admin = list_runs_visible(&conn, "org1", None, None, None, 50).unwrap();
        assert_eq!(as_admin.len(), 2, "super_user sees every run");

        let as_dev_a = list_runs_visible(&conn, "org1", Some("dev_a"), None, None, 50).unwrap();
        assert_eq!(as_dev_a.len(), 1, "dev_a sees only client A's run");
        assert_eq!(as_dev_a[0].client_id.as_deref(), Some("cl_a"));

        let as_dev_b = list_runs_visible(&conn, "org1", Some("dev_b"), None, None, 50).unwrap();
        assert_eq!(as_dev_b.len(), 1);
        assert_eq!(as_dev_b[0].client_id.as_deref(), Some("cl_b"));
    }

    #[test]
    fn list_runs_filters_by_client_and_source() {
        let conn = setup();
        run_for(&conn, Some("cl_a"), None);
        create_run(
            &conn,
            &NewRun {
                org_id: "org1",
                client_id: Some("cl_b"),
                project_id: None,
                source_kind: SourceKind::GitHistory,
                source_ref: None,
                runner_version: None,
                attestation: serde_json::json!({}),
                created_by: "admin",
            },
        )
        .unwrap();

        let by_client = list_runs_visible(&conn, "org1", None, Some("cl_a"), None, 50).unwrap();
        assert_eq!(by_client.len(), 1);
        let by_source =
            list_runs_visible(&conn, "org1", None, None, Some(SourceKind::GitHistory), 50).unwrap();
        assert_eq!(by_source.len(), 1);
        assert_eq!(by_source[0].source_kind, SourceKind::GitHistory);
    }

    /// Cancelling is about the pending work. A candidate already written to its
    /// destination cannot be un-written, and pretending otherwise would make the
    /// report disagree with the database.
    #[test]
    fn cancel_run_leaves_committed_candidates_untouched() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(
            &conn,
            "org1",
            &run.id,
            &[
                input("src:a", DestinationKind::Memory),
                input("src:b", DestinationKind::Memory),
            ],
        )
        .unwrap();
        let staged = list_candidates(&conn, &run.id, None, None, 50).unwrap();
        conn.execute(
            "UPDATE migration_candidates SET status = 'committed' WHERE id = ?1",
            [&staged[0].id],
        )
        .unwrap();

        let cancelled = cancel_run(&conn, "org1", &run.id).unwrap();
        assert_eq!(cancelled, 1, "only the pending candidate is cancelled");

        let after = list_candidates(&conn, &run.id, None, None, 50).unwrap();
        let committed = after.iter().find(|c| c.id == staged[0].id).unwrap();
        assert_eq!(committed.status, "committed", "a committed candidate stays committed");
    }

    // ── T-06: staging ────────────────────────────────────────────────────────

    #[test]
    fn stage_rejects_duplicate_source_identity_in_run() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        let results = stage_candidates(
            &conn,
            "org1",
            &run.id,
            &[
                input("src:a", DestinationKind::Memory),
                input("src:a", DestinationKind::Memory),
            ],
        )
        .unwrap();
        assert!(matches!(results[0], StageResult::Staged { .. }));
        assert_eq!(
            results[1],
            StageResult::Rejected {
                reason: "duplicate_source_identity_in_run".to_string()
            }
        );
    }

    /// An unknown destination kind cannot reach this layer: `DestinationKind` is
    /// a closed enum, so the rejection happens at JSON deserialization. That is
    /// covered by `models::types::tests::destination_kind_rejects_unknown_string`
    /// and by the CHECK constraint (`run_v60_candidate_rejects_unknown_destination_kind`).
    /// This test pins the third gate: the same identity may target two DIFFERENT
    /// kinds, which the run-level uniqueness must not conflate.
    #[test]
    fn stage_allows_same_identity_for_different_destination_kinds() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        let results = stage_candidates(
            &conn,
            "org1",
            &run.id,
            &[
                input("src:a", DestinationKind::Memory),
                input("src:a#conv", DestinationKind::Convention),
            ],
        )
        .unwrap();
        assert!(results.iter().all(|r| matches!(r, StageResult::Staged { .. })));
    }

    #[test]
    fn stage_skips_previously_rejected_identity() {
        let conn = setup();
        let run1 = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run1.id, &[input("src:a", DestinationKind::Memory)])
            .unwrap();
        let c = list_candidates(&conn, &run1.id, None, None, 10).unwrap();
        apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run1.id,
                candidate_id: &c[0].id,
                actor_id: "admin",
                verdict: ReviewVerdict::Rejected,
                expected_version: 1,
                reason: Some("not team knowledge"),
                correlation: None,
            },
        )
        .unwrap();

        // A later run rescans the same unchanged source.
        let run2 = run_for(&conn, None, None);
        let results =
            stage_candidates(&conn, "org1", &run2.id, &[input("src:a", DestinationKind::Memory)])
                .unwrap();
        assert_eq!(
            results[0],
            StageResult::Skipped {
                reason: "previously_rejected".to_string()
            },
            "a rejection must not be re-asked on an unchanged source"
        );
    }

    #[test]
    fn stage_skips_already_committed_identity() {
        let conn = setup();
        let run1 = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run1.id, &[input("src:a", DestinationKind::Memory)])
            .unwrap();
        let c = list_candidates(&conn, &run1.id, None, None, 10).unwrap();
        conn.execute(
            "INSERT INTO migration_provenance (id, org_id, destination_kind, source_identity, candidate_id)
             VALUES ('pr1', 'org1', 'memory', 'src:a', ?1)",
            [&c[0].id],
        )
        .unwrap();

        let run2 = run_for(&conn, None, None);
        let results =
            stage_candidates(&conn, "org1", &run2.id, &[input("src:a", DestinationKind::Memory)])
                .unwrap();
        assert_eq!(
            results[0],
            StageResult::Skipped {
                reason: "already_committed".to_string()
            }
        );
    }

    /// One malformed row must not discard the rest of the batch.
    #[test]
    fn stage_partial_batch_reports_per_candidate() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        let mut empty_content = input("src:b", DestinationKind::Memory);
        empty_content.content = "   ".to_string();
        let mut empty_identity = input("   ", DestinationKind::Memory);
        empty_identity.content = "fine".to_string();

        let results = stage_candidates(
            &conn,
            "org1",
            &run.id,
            &[
                input("src:a", DestinationKind::Memory),
                empty_content,
                empty_identity,
                input("src:c", DestinationKind::Task),
            ],
        )
        .unwrap();

        assert!(matches!(results[0], StageResult::Staged { .. }));
        assert_eq!(results[1], StageResult::Rejected { reason: "empty_content".into() });
        assert_eq!(results[2], StageResult::Rejected { reason: "empty_source_identity".into() });
        assert!(matches!(results[3], StageResult::Staged { .. }));
        assert_eq!(list_candidates(&conn, &run.id, None, None, 50).unwrap().len(), 2);
    }

    // ── T-07: review ─────────────────────────────────────────────────────────

    #[test]
    fn review_increments_candidate_version() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run.id, &[input("src:a", DestinationKind::Memory)])
            .unwrap();
        let c = list_candidates(&conn, &run.id, None, None, 10).unwrap();
        assert_eq!(c[0].version, 1);

        let outcome = apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "admin",
                verdict: ReviewVerdict::Approved,
                expected_version: 1,
                reason: None,
                correlation: None,
            },
        )
        .unwrap();
        assert_eq!(outcome, ReviewOutcome::Applied { new_version: 2 });

        let after = get_candidate(&conn, &c[0].id).unwrap().unwrap();
        assert_eq!(after.status, "approved");
        assert_eq!(after.version, 2);
    }

    /// Two reviewers, one queue. The second must not silently win.
    #[test]
    fn review_with_stale_expected_version_is_rejected_and_recorded() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run.id, &[input("src:a", DestinationKind::Memory)])
            .unwrap();
        let c = list_candidates(&conn, &run.id, None, None, 10).unwrap();

        apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "dev_a",
                verdict: ReviewVerdict::Approved,
                expected_version: 1,
                reason: None,
                correlation: None,
            },
        )
        .unwrap();

        let stale = apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "dev_b",
                verdict: ReviewVerdict::Rejected,
                expected_version: 1,
                reason: None,
                correlation: None,
            },
        )
        .unwrap();
        assert_eq!(stale, ReviewOutcome::StaleVersion { actual_version: 2 });

        let after = get_candidate(&conn, &c[0].id).unwrap().unwrap();
        assert_eq!(after.status, "approved", "the stale action must not take effect");

        let (action, expected, resulting): (String, Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT action, expected_version, resulting_version
                   FROM migration_review_actions
                  WHERE candidate_id = ?1 AND action = 'stale_version'",
                [&c[0].id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("the refused action must be recorded");
        assert_eq!(action, "stale_version");
        assert_eq!(expected, Some(1));
        assert_eq!(resulting, Some(2), "the trail records what the version actually was");
    }

    #[test]
    fn review_records_actor_and_authorization() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run.id, &[input("src:a", DestinationKind::Memory)])
            .unwrap();
        let c = list_candidates(&conn, &run.id, None, None, 10).unwrap();
        apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "dev_a",
                verdict: ReviewVerdict::Approved,
                expected_version: 1,
                reason: Some("matches our house style"),
                correlation: Some("req-42"),
            },
        )
        .unwrap();

        let (actor, auth, reason, corr): (String, String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT actor_id, actor_authorization, reason, request_correlation_id
                   FROM migration_review_actions WHERE candidate_id = ?1",
                [&c[0].id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(actor, "dev_a");
        assert_eq!(auth, "{}");
        assert_eq!(reason.as_deref(), Some("matches our house style"));
        assert_eq!(corr.as_deref(), Some("req-42"));
    }

    /// A reversal is a new row. The original rejection stays visible, because
    /// "this was rejected once and then re-staged" is different information from
    /// "this was always staged".
    #[test]
    fn restage_appends_action_without_erasing_rejection() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run.id, &[input("src:a", DestinationKind::Memory)])
            .unwrap();
        let c = list_candidates(&conn, &run.id, None, None, 10).unwrap();

        apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "admin",
                verdict: ReviewVerdict::Rejected,
                expected_version: 1,
                reason: None,
                correlation: None,
            },
        )
            .unwrap();
        apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "admin",
                verdict: ReviewVerdict::Restaged,
                expected_version: 2,
                reason: None,
                correlation: None,
            },
        )
            .unwrap();

        let actions: Vec<String> = conn
            .prepare("SELECT action FROM migration_review_actions WHERE candidate_id = ?1 ORDER BY created_at, id")
            .unwrap()
            .query_map([&c[0].id], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(actions.contains(&"rejected".to_string()), "the rejection stays in the trail");
        assert!(actions.contains(&"restaged".to_string()));

        let after = get_candidate(&conn, &c[0].id).unwrap().unwrap();
        assert_eq!(after.status, "staged");
        assert_eq!(after.version, 3);
    }

    #[test]
    fn review_of_committed_candidate_is_refused_and_recorded() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(&conn, "org1", &run.id, &[input("src:a", DestinationKind::Memory)])
            .unwrap();
        let c = list_candidates(&conn, &run.id, None, None, 10).unwrap();
        conn.execute(
            "UPDATE migration_candidates SET status = 'committed' WHERE id = ?1",
            [&c[0].id],
        )
        .unwrap();

        let outcome = apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "admin",
                verdict: ReviewVerdict::Rejected,
                expected_version: 1,
                reason: None,
                correlation: None,
            },
        )
        .unwrap();
        assert_eq!(outcome, ReviewOutcome::NotReviewable { status: "committed".into() });

        let recorded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM migration_review_actions
                  WHERE candidate_id = ?1 AND action = 'not_approved'",
                [&c[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(recorded, 1);
    }

    #[test]
    fn batch_contains_attested_identifies_the_blockers() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        let mut verified = input("src:v", DestinationKind::Memory);
        verified.provenance_kind = Some("verified_manifest".to_string());
        stage_candidates(
            &conn,
            "org1",
            &run.id,
            &[verified, input("src:a", DestinationKind::Memory)],
        )
        .unwrap();
        let all = list_candidates(&conn, &run.id, None, None, 10).unwrap();
        let ids: Vec<String> = all.iter().map(|c| c.id.clone()).collect();

        let attested = batch_contains_attested(&conn, &ids).unwrap();
        assert_eq!(attested.len(), 1, "only the client_attested candidate blocks the batch");
    }

    // ── Reporting ────────────────────────────────────────────────────────────

    #[test]
    fn run_report_explains_every_non_committed_candidate() {
        let conn = setup();
        let run = run_for(&conn, None, None);
        stage_candidates(
            &conn,
            "org1",
            &run.id,
            &[
                input("src:a", DestinationKind::Memory),
                input("src:b", DestinationKind::Memory),
                input("src:c", DestinationKind::Task),
            ],
        )
        .unwrap();
        let c = list_candidates(&conn, &run.id, None, None, 10).unwrap();
        apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[0].id,
                actor_id: "admin",
                verdict: ReviewVerdict::Approved,
                expected_version: 1,
                reason: None,
                correlation: None,
            },
        )
            .unwrap();
        apply_review_action(
            &conn,
            &ReviewRequest {
                run_id: &run.id,
                candidate_id: &c[1].id,
                actor_id: "admin",
                verdict: ReviewVerdict::Rejected,
                expected_version: 1,
                reason: Some("stale doc"),
                correlation: None,
            },
        )
            .unwrap();

        let report = run_report(&conn, &run.id).unwrap();
        assert_eq!(report.approved, 1);
        assert_eq!(report.rejected, 1);
        assert_eq!(report.staged, 1);
        assert_eq!(report.committed, 0);
        assert_eq!(
            report.outcomes.len(),
            3,
            "every candidate that is not committed appears with its status"
        );
        assert!(report.outcomes.iter().all(|o| !o.status.is_empty()));
    }
}

// ── Commit ───────────────────────────────────────────────────────────────────

use crate::models::types::{
    CreateConventionRequest, CreateHarnessConfigReviewRequest, CreateHarnessRequest,
    CreateTaskRequest, PublishHarnessVersionRequest, SaveArtifactRequest, StoreMemoryRequest,
};

/// What happened to one candidate during a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed { destination_id: String },
    Skipped { reason: String },
    Failed { error_code: String },
}

/// Maps a destination failure onto a short, stable code. The codes end up in
/// `migration_outcomes.error_code` and in the run report, so a reviewer can tell
/// "the manifest was invalid" from "the project does not exist" without reading
/// a stack trace.
fn error_code(e: &anyhow::Error) -> String {
    let msg = e.to_string();
    for known in [
        "invalid_manifest",
        "missing_capability",
        "harness_not_found",
        "owner_not_in_org",
        "missing_redaction_report",
        "artifact_too_large",
        "invalid_kind",
        "validation_error",
    ] {
        if msg.contains(known) {
            return known.to_string();
        }
    }
    if msg.contains("project") {
        return "unknown_project".to_string();
    }
    "destination_error".to_string()
}

fn hint_str<'a>(hint: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    hint.get(key).and_then(|v| v.as_str())
}

/// Writes one candidate to its destination and returns the destination's id.
///
/// Every arm goes through the same persistence function the corresponding
/// first-class API uses. There is deliberately no SQL here: a parallel write
/// path would skip the destination's own scoping, audit and validation, which is
/// exactly the failure mode "Destination Persistence Reuse" exists to forbid.
/// The run's project as a NAME, for destinations that scope by project name.
///
/// A run carries a project *id* (the routed backend project), but memories,
/// tasks and SDD artifacts key on the human project *name* — the same string an
/// agent passes as `project`. Resolving the id to its name here is what links a
/// committed memory to the project the run was routed to; without it the memory
/// falls back to the org default and looks unlinked. Returns `None` for an
/// internal run (no project) or an id that no longer resolves.
fn run_project_name(conn: &Connection, run: &MigrationRun) -> Option<String> {
    run.project_id
        .as_deref()
        .and_then(|pid| queries::get_project_by_id(conn, &run.org_id, pid).ok().flatten())
        .map(|p| p.name)
}

pub fn write_destination(
    conn: &Connection,
    run: &MigrationRun,
    candidate: &MigrationCandidate,
) -> Result<String> {
    let hint = &candidate.destination_hint;
    match candidate.destination_kind {
        DestinationKind::Memory => {
            let req = StoreMemoryRequest {
                // Scope to the run's project unless the candidate named its own.
                project: hint_str(hint, "project")
                    .map(str::to_string)
                    .or_else(|| run_project_name(conn, run)),
                tool: hint_str(hint, "tool").unwrap_or("migration").to_string(),
                content: candidate.content.clone(),
                tags: hint.get("tags").and_then(|t| {
                    serde_json::from_value::<Vec<String>>(t.clone()).ok()
                }),
                title: hint_str(hint, "title").map(str::to_string),
                memory_type: hint_str(hint, "type").map(str::to_string),
                scope: hint_str(hint, "scope").map(str::to_string),
                topic_key: hint_str(hint, "topic_key").map(str::to_string),
                session_id: None,
            };
            let memory = queries::store_memory_with_audit(conn, &run.org_id, &run.created_by, &req)?;
            Ok(memory.id)
        }
        DestinationKind::Convention => {
            let req = CreateConventionRequest {
                title: hint_str(hint, "title")
                    .unwrap_or("Migrated convention")
                    .to_string(),
                content: candidate.content.clone(),
                category: hint_str(hint, "category").map(str::to_string),
                weight: hint.get("weight").and_then(|w| w.as_i64()),
                tags: hint
                    .get("tags")
                    .and_then(|t| serde_json::from_value::<Vec<String>>(t.clone()).ok()),
                // A convention inherits the run's project scope unless the hint
                // overrides it. v56 forbade project-scoped conventions; v60
                // allows them, because `conventions.project_id` always did.
                project_id: hint_str(hint, "project_id")
                    .map(str::to_string)
                    .or_else(|| run.project_id.clone()),
            };
            let convention = queries::create_convention(conn, &run.org_id, &req)?;
            Ok(convention.id.to_string())
        }
        DestinationKind::Task => {
            let req = CreateTaskRequest {
                project: hint_str(hint, "project")
                    .map(str::to_string)
                    .or_else(|| run_project_name(conn, run))
                    .unwrap_or_else(|| "default".to_string()),
                title: hint_str(hint, "title")
                    .unwrap_or_else(|| first_line(&candidate.content))
                    .to_string(),
                description: Some(candidate.content.clone()),
                // Migrated work starts in the backlog. Anything else would be
                // the migration deciding somebody's sprint for them.
                status: Some("backlog".to_string()),
                priority: hint_str(hint, "priority").map(str::to_string),
                due_date: None,
                parent_id: None,
                sprint_id: None,
            };
            let task = queries::create_task(conn, &run.org_id, &run.created_by, &req)?;
            Ok(task.id)
        }
        DestinationKind::SddArtifact => {
            let kind = hint_str(hint, "kind").unwrap_or("proposal").to_string();
            let capability = hint_str(hint, "capability").map(str::to_string);
            // `save_sdd_artifact` requires a capability for `spec`, and the
            // capability name outlives the change that introduced it. Guessing
            // it is worse than failing the candidate and asking a human.
            if kind == "spec" && capability.as_deref().unwrap_or("").trim().is_empty() {
                anyhow::bail!("missing_capability");
            }
            let req = SaveArtifactRequest {
                project: hint_str(hint, "project")
                    .map(str::to_string)
                    .or_else(|| run_project_name(conn, run))
                    .unwrap_or_else(|| "default".to_string()),
                change_name: hint_str(hint, "change_name")
                    .unwrap_or("migrated")
                    .to_string(),
                kind,
                capability,
                content: candidate.content.clone(),
                path: hint_str(hint, "path").map(str::to_string),
                git_commit: hint_str(hint, "git_commit").map(str::to_string),
                git_ref: hint_str(hint, "git_ref").map(str::to_string),
                source: Some("import".to_string()),
            };
            let (artifact, _created) =
                queries::upsert_sdd_artifact(conn, &run.org_id, &run.created_by, &req, "import")?;
            Ok(artifact.id)
        }
        DestinationKind::Harness => {
            let manifest = hint
                .get("manifest")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("invalid_manifest: no manifest in hint"))?;
            // The harness validator is the authority on manifest shape. Running
            // it here means an invalid manifest fails the candidate instead of
            // creating a half-built harness with no publishable version.
            crate::models::types::validate_typed_harness_manifest(&manifest)
                .map_err(|e| anyhow::anyhow!("invalid_manifest: {e}"))?;

            let slug = hint_str(hint, "slug")
                .ok_or_else(|| anyhow::anyhow!("validation_error: harness slug is required"))?;

            // A harness is two writes, and a harness with no published version is
            // not a harness — nobody can install it and nothing points at it. If
            // publishing fails, the catalog row must go with it. Neither
            // `create_harness` nor `publish_harness_version` opens a transaction
            // of its own, so wrapping them here is safe (unlike the destinations
            // that do — see `commit_approved`).
            let tx = conn.unchecked_transaction()?;
            let harness = queries::create_harness(
                &tx,
                &run.org_id,
                &run.created_by,
                &CreateHarnessRequest {
                    slug: slug.to_string(),
                    name: hint_str(hint, "name").unwrap_or(slug).to_string(),
                    description: hint_str(hint, "description").map(str::to_string),
                    project_id: run.project_id.clone(),
                    visibility: Some("org".to_string()),
                    owner_user_id: None,
                },
            )?;
            queries::publish_harness_version(
                &tx,
                &run.org_id,
                &run.created_by,
                &harness.id,
                &PublishHarnessVersionRequest {
                    // Migrated harnesses all start at 0.1.0. Inventing a version
                    // history that never existed is worse than starting at zero.
                    version: hint_str(hint, "version").unwrap_or("0.1.0").to_string(),
                    manifest,
                    manifest_hash: None,
                },
            )?;
            tx.commit()?;
            Ok(harness.id)
        }
        DestinationKind::HarnessConfigReview => {
            let review = queries::create_harness_config_review(
                conn,
                &run.org_id,
                &run.created_by,
                &CreateHarnessConfigReviewRequest {
                    source_tool: hint_str(hint, "source_tool").unwrap_or("claude").to_string(),
                    redacted_config: hint
                        .get("redacted_config")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                    redaction_report: hint
                        .get("redaction_report")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                    content_hash: hint_str(hint, "content_hash")
                        .unwrap_or(&candidate.source_identity)
                        .to_string(),
                    status: None,
                },
            )?;
            Ok(review.id)
        }
    }
}

fn first_line(content: &str) -> &str {
    content.lines().next().unwrap_or("Migrated task").trim()
}

fn record_outcome(
    conn: &Connection,
    candidate: &MigrationCandidate,
    outcome_status: &str,
    error: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO migration_outcomes
            (id, run_id, candidate_id, expected_version, candidate_status, outcome_status, error_code)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            candidate.run_id,
            candidate.id,
            candidate.version,
            candidate.status,
            outcome_status,
            error,
        ],
    )?;
    Ok(())
}

/// Commits every approved candidate of a run.
///
/// # Isolated per candidate, resumable per batch
///
/// A failure affects **that** candidate and nothing else, so a batch of forty
/// where the seventeenth is malformed leaves sixteen committed, one failed and
/// twenty-three still to process — and re-running picks up exactly what is left,
/// because `migration_provenance` refuses a second commit of an already-committed
/// source. All-or-nothing was considered and rejected: it lets one bad candidate
/// hold thirty-nine good ones hostage, with no way forward except deleting it.
///
/// # Why there is no outer transaction (correction to design.md §4.4)
///
/// The design called for one transaction per candidate wrapping the destination
/// write plus the bookkeeping. That is **not possible**, and the reason is worth
/// recording: several destination functions open their own transaction —
/// `log_audit` (via `insert_audit_log_chained`) and `upsert_sdd_artifact` both
/// call `unchecked_transaction()`. SQLite has no nested transactions, so the
/// inner `BEGIN` fails. In `store_memory_with_audit` that failure is swallowed by
/// `let _ = log_audit(..)`, which means the outer transaction did not make the
/// commit atomic — it silently *disabled the audit trail*. A migrated memory
/// would have landed with no audit row at all.
///
/// So the ordering carries the guarantee instead, and it is chosen to fail safe:
///
/// 1. write the destination (each destination manages its own atomicity);
/// 2. only then write provenance + status + outcome, together.
///
/// A destination failure therefore leaves **no provenance row**, which is the
/// invariant that matters: a failed candidate stays retryable. The opposite
/// window — a destination written but provenance not — is narrow (the only
/// expected failure is the UNIQUE violation, which is itself the idempotency
/// signal) and is reported as `provenance_write_failed` rather than hidden.
pub fn commit_approved(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
) -> Result<Vec<(MigrationCandidate, CommitOutcome)>> {
    let Some(run) = get_run(conn, org_id, run_id)? else {
        anyhow::bail!("run_not_found");
    };
    let approved = list_candidates(conn, run_id, Some("approved"), None, 10_000)?;
    let mut results = Vec::with_capacity(approved.len());

    for candidate in approved {
        // Fast path only. The real guarantee is the UNIQUE on
        // `migration_provenance`: two commits racing would both pass this SELECT
        // and one would still lose at INSERT, which is the point.
        if already_committed(
            conn,
            org_id,
            &candidate.source_identity,
            candidate.destination_kind,
        )? {
            record_outcome(conn, &candidate, "skipped", Some("already_committed"))?;
            set_candidate_status(conn, &candidate.id, "skipped")?;
            results.push((
                candidate,
                CommitOutcome::Skipped {
                    reason: "already_committed".to_string(),
                },
            ));
            continue;
        }

        // Step 1 — the destination. If this fails, nothing else has happened.
        let destination_id = match write_destination(conn, &run, &candidate) {
            Ok(id) => id,
            Err(e) => {
                let code = error_code(&e);
                record_outcome(conn, &candidate, "failed", Some(&code))?;
                set_candidate_status(conn, &candidate.id, "failed")?;
                results.push((candidate, CommitOutcome::Failed { error_code: code }));
                continue;
            }
        };

        // Step 2 — the bookkeeping, atomically. None of these three statements
        // calls anything that opens a transaction of its own, so this one holds.
        let tx = conn.unchecked_transaction()?;
        let booked = (|| -> Result<()> {
            tx.execute(
                "INSERT INTO migration_provenance
                    (id, org_id, destination_kind, source_identity, candidate_id, destination_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    org_id,
                    candidate.destination_kind.as_str(),
                    candidate.source_identity,
                    candidate.id,
                    destination_id,
                ],
            )?;
            tx.execute(
                "UPDATE migration_candidates
                    SET status = 'committed', updated_at = datetime('now')
                  WHERE id = ?1",
                [&candidate.id],
            )?;
            record_outcome_in(&tx, &candidate, "committed", None)?;
            Ok(())
        })();

        match booked {
            Ok(()) => {
                tx.commit()?;
                results.push((
                    candidate,
                    CommitOutcome::Committed {
                        destination_id: destination_id.clone(),
                    },
                ));
            }
            Err(e) => {
                drop(tx);
                let raced = e
                    .to_string()
                    .contains("UNIQUE constraint failed: migration_provenance");
                if raced {
                    // Another commit won the race. The destination write just
                    // performed is a duplicate; say so loudly rather than
                    // reporting a clean skip.
                    tracing::warn!(
                        "migration: candidate {} lost the provenance race after writing \
                         destination {destination_id}; that destination record is orphaned",
                        candidate.id
                    );
                    record_outcome(conn, &candidate, "skipped", Some("already_committed"))?;
                    set_candidate_status(conn, &candidate.id, "skipped")?;
                    results.push((
                        candidate,
                        CommitOutcome::Skipped {
                            reason: "already_committed".to_string(),
                        },
                    ));
                } else {
                    tracing::error!(
                        "migration: candidate {} wrote destination {destination_id} but its \
                         provenance row failed ({e}); a re-run would duplicate it",
                        candidate.id
                    );
                    record_outcome(conn, &candidate, "failed", Some("provenance_write_failed"))?;
                    set_candidate_status(conn, &candidate.id, "failed")?;
                    results.push((
                        candidate,
                        CommitOutcome::Failed {
                            error_code: "provenance_write_failed".to_string(),
                        },
                    ));
                }
            }
        }
    }

    Ok(results)
}

fn record_outcome_in(
    tx: &rusqlite::Transaction,
    candidate: &MigrationCandidate,
    outcome_status: &str,
    error: Option<&str>,
) -> Result<()> {
    tx.execute(
        "INSERT INTO migration_outcomes
            (id, run_id, candidate_id, expected_version, candidate_status, outcome_status, error_code)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            candidate.run_id,
            candidate.id,
            candidate.version,
            candidate.status,
            outcome_status,
            error,
        ],
    )?;
    Ok(())
}

fn set_candidate_status(conn: &Connection, candidate_id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE migration_candidates SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![candidate_id, status],
    )?;
    Ok(())
}

/// Marks a candidate's destination as vectorized. `indexed_at` staying NULL is a
/// legitimate state, not a bug: the artifact is persisted and correct, it is
/// just not searchable by similarity yet.
pub fn set_candidate_indexed(conn: &Connection, candidate_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE migration_candidates SET indexed_at = datetime('now') WHERE id = ?1",
        [candidate_id],
    )?;
    Ok(())
}

/// How many candidates are committed but not yet vectorized.
///
/// A COUNT, not `list_pending_index(..).len()`: a capped list reports its own
/// limit as the backlog, which reads as "exactly 10 000 pending" forever.
pub fn count_pending_index(conn: &Connection, org_id: &str) -> Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM migration_candidates c
           JOIN migration_runs r ON r.id = c.run_id
          WHERE r.org_id = ?1 AND c.status = 'committed' AND c.indexed_at IS NULL
            AND c.destination_kind = 'memory'",
        [org_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// Candidates that are committed but not yet vectorized, oldest first.
///
/// Restricted to `memory` destinations because they are the only ones with a
/// vector store (`memory_embeddings`). A task or a harness is not "pending
/// indexing" — it is simply not a thing that gets embedded, and counting it as
/// pending would make the backlog permanently non-zero and therefore ignored.
pub fn list_pending_index(conn: &Connection, org_id: &str, limit: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM migration_candidates c
           JOIN migration_runs r ON r.id = c.run_id
          WHERE r.org_id = ?1 AND c.status = 'committed' AND c.indexed_at IS NULL
            AND c.destination_kind = 'memory'
          ORDER BY c.created_at
          LIMIT ?2",
    )?;
    let ids = stmt
        .query_map(rusqlite::params![org_id, limit], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}
