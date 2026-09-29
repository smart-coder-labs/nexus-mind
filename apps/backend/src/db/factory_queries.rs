//! Factory data layer: per-action policies (migration v78).
//!
//! Policies are control-plane state. They are written only through the admin API
//! behind `factory_policy:write` and never from repository or worker input. Every
//! write appends its audit record in the same transaction.

use std::num::NonZeroU32;

use anyhow::Result;
use rusqlite::{params, types::Type, Connection, OptionalExtension, Row};

use crate::db::queries::append_audit_chained;
use crate::factory::contracts::{
    Action, ActionPolicy, ActionVerdict, Contract, PolicyScope, SchemaV1, TaskClass,
};

/// A persisted policy: the wire contract plus its row identity.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct StoredPolicy {
    pub id: String,
    #[serde(flatten)]
    pub policy: ActionPolicy,
    pub updated_by: String,
    pub updated_at: String,
}

#[derive(Debug)]
pub enum PolicyWriteError {
    /// `version` must be 1 for a new row and `current + 1` for an existing one.
    VersionConflict {
        current: Option<u32>,
    },
    Invalid(String),
    Db(anyhow::Error),
}

impl From<rusqlite::Error> for PolicyWriteError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(error.into())
    }
}

const COLUMNS: &str = "id, action, mode, scope_project, scope_task_class, allow_json, stop_json, \
                       version, updated_by, updated_at";

/// Serde's wire name for a unit enum variant (`TaskClass::Docs` → `"docs"`), so
/// the stored text is exactly the contract's text.
fn wire<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn from_wire<T: serde::de::DeserializeOwned>(text: &str) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(text.to_string())).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn row_to_policy(row: &Row) -> rusqlite::Result<StoredPolicy> {
    let json_list = |index: usize| -> rusqlite::Result<Vec<String>> {
        let raw: String = row.get(index)?;
        serde_json::from_str(&raw).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
        })
    };
    let project: String = row.get(3)?;
    let task_class: String = row.get(4)?;
    let version: u32 = row.get(7)?;
    Ok(StoredPolicy {
        id: row.get(0)?,
        policy: ActionPolicy {
            schema_version: SchemaV1,
            action: from_wire(&row.get::<_, String>(1)?)?,
            mode: from_wire(&row.get::<_, String>(2)?)?,
            scope: PolicyScope {
                project: (!project.is_empty()).then_some(project),
                task_class: if task_class.is_empty() {
                    None
                } else {
                    Some(from_wire::<TaskClass>(&task_class)?)
                },
            },
            allow: json_list(5)?,
            stop: json_list(6)?,
            version: NonZeroU32::new(version)
                .ok_or(rusqlite::Error::IntegralValueOutOfRange(7, 0))?,
        },
        updated_by: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn list_factory_policies(conn: &Connection, org_id: &str) -> Result<Vec<StoredPolicy>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM factory_action_policies WHERE org_id = ?1
          ORDER BY action, scope_project, scope_task_class"
    ))?;
    let rows = statement
        .query_map([org_id], row_to_policy)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn find_by_scope(
    conn: &Connection,
    org_id: &str,
    policy: &ActionPolicy,
) -> rusqlite::Result<Option<StoredPolicy>> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM factory_action_policies
              WHERE org_id = ?1 AND action = ?2 AND scope_project = ?3 AND scope_task_class = ?4"
        ),
        params![
            org_id,
            wire(&policy.action),
            policy.scope.project.as_deref().unwrap_or(""),
            policy
                .scope
                .task_class
                .as_ref()
                .map(wire)
                .unwrap_or_default(),
        ],
        row_to_policy,
    )
    .optional()
}

/// Inserts or replaces the policy for `(action, scope)`. Returns the stored row and
/// the previous row, if any, so the caller can audit the transition.
pub fn upsert_factory_policy(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    policy: &ActionPolicy,
) -> Result<(StoredPolicy, Option<StoredPolicy>), PolicyWriteError> {
    policy.validate().map_err(PolicyWriteError::Invalid)?;
    let tx = conn.unchecked_transaction()?;
    let previous = find_by_scope(&tx, org_id, policy)?;
    let expected = previous
        .as_ref()
        .map_or(1, |row| row.policy.version.get() + 1);
    if policy.version.get() != expected {
        return Err(PolicyWriteError::VersionConflict {
            current: previous.as_ref().map(|row| row.policy.version.get()),
        });
    }
    let allow = serde_json::to_string(&policy.allow).map_err(|e| PolicyWriteError::Db(e.into()))?;
    let stop = serde_json::to_string(&policy.stop).map_err(|e| PolicyWriteError::Db(e.into()))?;
    match &previous {
        Some(row) => {
            // `version = ?` in the WHERE makes a concurrent writer that raced past the
            // read above update zero rows instead of silently overwriting.
            let changed = tx.execute(
                "UPDATE factory_action_policies
                    SET mode = ?1, allow_json = ?2, stop_json = ?3, version = ?4,
                        updated_by = ?5, updated_at = datetime('now')
                  WHERE id = ?6 AND org_id = ?7 AND version = ?8",
                params![
                    wire(&policy.mode),
                    allow,
                    stop,
                    policy.version.get(),
                    user_id,
                    row.id,
                    org_id,
                    row.policy.version.get(),
                ],
            )?;
            if changed != 1 {
                return Err(PolicyWriteError::VersionConflict {
                    current: Some(row.policy.version.get()),
                });
            }
        }
        None => {
            tx.execute(
                "INSERT INTO factory_action_policies
                   (id, org_id, action, mode, scope_project, scope_task_class,
                    allow_json, stop_json, version, updated_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    org_id,
                    wire(&policy.action),
                    wire(&policy.mode),
                    policy.scope.project.as_deref().unwrap_or(""),
                    policy
                        .scope
                        .task_class
                        .as_ref()
                        .map(wire)
                        .unwrap_or_default(),
                    allow,
                    stop,
                    policy.version.get(),
                    user_id,
                ],
            )?;
        }
    }
    let stored = find_by_scope(&tx, org_id, policy)?
        .ok_or_else(|| PolicyWriteError::Db(anyhow::anyhow!("policy_missing_after_write")))?;
    // Same transaction: a policy change without its audit record must not exist.
    append_audit_chained(
        &tx,
        org_id,
        user_id,
        "factory_policy.upsert",
        "factory_policy",
        Some(&stored.id),
        serde_json::json!({
            "action": stored.policy.action,
            "scope": stored.policy.scope,
            "mode_before": previous.as_ref().map(|row| row.policy.mode),
            "mode_after": stored.policy.mode,
            "version": stored.policy.version,
        }),
        None,
    )
    .map_err(PolicyWriteError::Db)?;
    tx.commit()?;
    Ok((stored, previous))
}

/// Deletes one policy of this org. Returns the deleted row, or `None` when the id
/// does not exist in this org (another org's id is indistinguishable from none).
pub fn delete_factory_policy(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    id: &str,
) -> Result<Option<StoredPolicy>> {
    let tx = conn.unchecked_transaction()?;
    let existing = tx
        .query_row(
            &format!("SELECT {COLUMNS} FROM factory_action_policies WHERE id = ?1 AND org_id = ?2"),
            [id, org_id],
            row_to_policy,
        )
        .optional()?;
    if let Some(deleted) = &existing {
        tx.execute(
            "DELETE FROM factory_action_policies WHERE id = ?1 AND org_id = ?2",
            [id, org_id],
        )?;
        append_audit_chained(
            &tx,
            org_id,
            user_id,
            "factory_policy.delete",
            "factory_policy",
            Some(&deleted.id),
            serde_json::json!({
                "action": deleted.policy.action,
                "scope": deleted.policy.scope,
                "mode_before": deleted.policy.mode,
            }),
            None,
        )?;
    }
    tx.commit()?;
    Ok(existing)
}

/// One evaluated action, for the decision audit (false-low-risk dataset). Never
/// carries diff content or secrets.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct DecisionRecord {
    pub subject: String,
    pub action: Action,
    pub verdict: ActionVerdict,
    pub policy_version: Option<u32>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub confidence: Option<f64>,
    pub inputs: serde_json::Value,
}

pub fn record_decision(conn: &Connection, org_id: &str, record: &DecisionRecord) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO factory_decisions
           (id, org_id, subject, action, verdict, source, reason, policy_version,
            provider, model, confidence, inputs_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            org_id,
            record.subject,
            wire(&record.action),
            wire(&record.verdict.verdict),
            wire(&record.verdict.source),
            record.verdict.reason,
            record.policy_version,
            record.provider,
            record.model,
            record.confidence,
            serde_json::to_string(&record.inputs)?,
        ],
    )?;
    Ok(id)
}

pub fn list_decisions(
    conn: &Connection,
    org_id: &str,
    subject: &str,
) -> Result<Vec<DecisionRecord>> {
    let mut statement = conn.prepare(
        "SELECT subject, action, verdict, source, reason, policy_version, provider, model,
                confidence, inputs_json
           FROM factory_decisions WHERE org_id = ?1 AND subject = ?2 ORDER BY rowid",
    )?;
    let rows = statement
        .query_map([org_id, subject], |row| {
            let inputs: String = row.get(9)?;
            Ok(DecisionRecord {
                subject: row.get(0)?,
                action: from_wire(&row.get::<_, String>(1)?)?,
                verdict: ActionVerdict {
                    verdict: from_wire(&row.get::<_, String>(2)?)?,
                    source: from_wire(&row.get::<_, String>(3)?)?,
                    reason: row.get(4)?,
                },
                policy_version: row.get(5)?,
                provider: row.get(6)?,
                model: row.get(7)?,
                confidence: row.get(8)?,
                inputs: serde_json::from_str(&inputs).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(9, Type::Text, Box::new(error))
                })?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// A merge that passed every gate and waits out the soak window (plan D17).
#[derive(Clone, Debug, PartialEq)]
pub struct MergeSoak {
    pub id: String,
    pub org_id: String,
    pub run_id: String,
    pub repository: String,
    pub pull_number: i64,
    pub head_sha: String,
    pub required_checks: Vec<String>,
    pub due_at: String,
}

/// Starts the soak for this PR, replacing any running one. Every new evaluation
/// restarts the window with its own run (publish authority) and required checks,
/// so a soak always reflects the latest review — never an outdated one.
#[allow(clippy::too_many_arguments)]
pub fn start_merge_soak(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    repository: &str,
    pull_number: i64,
    head_sha: &str,
    required_checks: &[String],
    soak_seconds: i64,
) -> Result<MergeSoak> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM factory_merge_soaks
          WHERE org_id = ?1 AND repository = ?2 AND pull_number = ?3",
        params![org_id, repository, pull_number],
    )?;
    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO factory_merge_soaks
           (id, org_id, run_id, repository, pull_number, head_sha, required_checks_json, due_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now', ?8))",
        params![
            id,
            org_id,
            run_id,
            repository,
            pull_number,
            head_sha,
            serde_json::to_string(required_checks)?,
            format!("{soak_seconds:+} seconds"),
        ],
    )?;
    let soak = tx.query_row(
        &format!("SELECT {SOAK_COLUMNS} FROM factory_merge_soaks WHERE id = ?1"),
        [&id],
        row_to_soak,
    )?;
    tx.commit()?;
    Ok(soak)
}

/// Soaks whose window has elapsed, across all orgs (the worker processes them).
pub fn due_merge_soaks(conn: &Connection, limit: i64) -> Result<Vec<MergeSoak>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {SOAK_COLUMNS} FROM factory_merge_soaks
          WHERE due_at <= datetime('now') ORDER BY due_at LIMIT ?1"
    ))?;
    let rows = statement
        .query_map([limit], row_to_soak)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Cancels the pending soak of this PR, if any. Called whenever a new review of
/// the PR does not end in `allow`: the soak must never outlive a later verdict.
pub fn cancel_merge_soak(
    conn: &Connection,
    org_id: &str,
    repository: &str,
    pull_number: i64,
) -> Result<bool> {
    let removed = conn.execute(
        "DELETE FROM factory_merge_soaks
          WHERE org_id = ?1 AND repository = ?2 AND pull_number = ?3",
        params![org_id, repository, pull_number],
    )?;
    Ok(removed > 0)
}

pub fn finish_merge_soak(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM factory_merge_soaks WHERE id = ?1", [id])?;
    Ok(())
}

const SOAK_COLUMNS: &str =
    "id, org_id, run_id, repository, pull_number, head_sha, required_checks_json, due_at";

fn row_to_soak(row: &Row) -> rusqlite::Result<MergeSoak> {
    let checks: String = row.get(6)?;
    Ok(MergeSoak {
        id: row.get(0)?,
        org_id: row.get(1)?,
        run_id: row.get(2)?,
        repository: row.get(3)?,
        pull_number: row.get(4)?,
        head_sha: row.get(5)?,
        required_checks: serde_json::from_str(&checks).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(error))
        })?,
        due_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations, queries};
    use crate::factory::contracts::{PolicyMode, Verdict, VerdictSource};

    fn setup() -> (Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        (conn, org.id, user.id)
    }

    fn policy(action: Action, mode: PolicyMode, version: u32) -> ActionPolicy {
        ActionPolicy {
            schema_version: SchemaV1,
            action,
            mode,
            scope: PolicyScope::default(),
            allow: if mode == PolicyMode::Criteria {
                vec!["docs/tests paths only".into()]
            } else {
                vec![]
            },
            stop: vec![],
            version: NonZeroU32::new(version).unwrap(),
        }
    }

    #[test]
    fn migration_creates_the_table_and_grants_only_the_super_user_template() {
        let (conn, _, _) = setup();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert!(version >= 78);
        let grants = |template: &str| -> Vec<String> {
            let raw: String = conn
                .query_row(
                    "SELECT permissions FROM roles WHERE id = ?1",
                    [template],
                    |r| r.get(0),
                )
                .unwrap();
            serde_json::from_str(&raw).unwrap()
        };
        let super_user = grants("super_user_template");
        assert!(super_user.iter().any(|p| p == "factory_policy:read"));
        assert!(super_user.iter().any(|p| p == "factory_policy:write"));
        assert!(!grants("admin_template")
            .iter()
            .any(|p| p.starts_with("factory_policy:")));
    }

    #[test]
    fn upsert_creates_then_requires_the_next_version() {
        let (conn, org, user) = setup();
        let (created, previous) = upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Merge, PolicyMode::Manual, 1),
        )
        .unwrap();
        assert!(previous.is_none());
        assert_eq!(created.policy.mode, PolicyMode::Manual);

        let (updated, previous) = upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Merge, PolicyMode::Criteria, 2),
        )
        .unwrap();
        assert_eq!(
            updated.id, created.id,
            "same (action, scope) is the same row"
        );
        assert_eq!(previous.unwrap().policy.mode, PolicyMode::Manual);
        assert_eq!(list_factory_policies(&conn, &org).unwrap(), vec![updated]);
    }

    #[test]
    fn stale_or_skipped_versions_conflict() {
        let (conn, org, user) = setup();
        upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Fix, PolicyMode::Manual, 1),
        )
        .unwrap();
        for version in [1, 3] {
            match upsert_factory_policy(
                &conn,
                &org,
                &user,
                &policy(Action::Fix, PolicyMode::Never, version),
            ) {
                Err(PolicyWriteError::VersionConflict { current: Some(1) }) => {}
                other => panic!("version {version}: expected conflict, got {other:?}"),
            }
        }
        // A brand-new row must start at 1.
        assert!(matches!(
            upsert_factory_policy(
                &conn,
                &org,
                &user,
                &policy(Action::Deploy, PolicyMode::Never, 2)
            ),
            Err(PolicyWriteError::VersionConflict { current: None })
        ));
    }

    #[test]
    fn invalid_policy_is_rejected_before_touching_the_database() {
        let (conn, org, user) = setup();
        let mut bad = policy(Action::Merge, PolicyMode::Criteria, 1);
        bad.allow.clear();
        assert!(matches!(
            upsert_factory_policy(&conn, &org, &user, &bad),
            Err(PolicyWriteError::Invalid(_))
        ));
        assert!(list_factory_policies(&conn, &org).unwrap().is_empty());
    }

    fn audited(conn: &Connection) -> Vec<(String, serde_json::Value)> {
        conn.prepare(
            "SELECT action, metadata FROM audit_logs WHERE resource_type = 'factory_policy' ORDER BY rowid",
        )
        .unwrap()
        .query_map([], |row| {
            let raw: String = row.get(1)?;
            Ok((row.get(0)?, serde_json::from_str(&raw).unwrap()))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }

    #[test]
    fn every_write_is_audited_with_the_mode_transition() {
        let (conn, org, user) = setup();
        upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Merge, PolicyMode::Manual, 1),
        )
        .unwrap();
        let (stored, _) = upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Merge, PolicyMode::Criteria, 2),
        )
        .unwrap();
        delete_factory_policy(&conn, &org, &user, &stored.id).unwrap();
        let rows = audited(&conn);
        let actions: Vec<&str> = rows.iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(
            actions,
            [
                "factory_policy.upsert",
                "factory_policy.upsert",
                "factory_policy.delete"
            ]
        );
        assert_eq!(rows[1].1["mode_before"], "manual");
        assert_eq!(rows[1].1["mode_after"], "criteria");
        assert_eq!(rows[2].1["mode_before"], "criteria");
    }

    #[test]
    fn a_failed_audit_rolls_back_the_change() {
        let (conn, org, user) = setup();
        // An unknown actor violates the audit_logs.user_id foreign key.
        assert!(matches!(
            upsert_factory_policy(
                &conn,
                &org,
                "ghost",
                &policy(Action::Fix, PolicyMode::Manual, 1)
            ),
            Err(PolicyWriteError::Db(_))
        ));
        assert!(list_factory_policies(&conn, &org).unwrap().is_empty());

        let (created, _) = upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Fix, PolicyMode::Manual, 1),
        )
        .unwrap();
        assert!(delete_factory_policy(&conn, &org, "ghost", &created.id).is_err());
        assert_eq!(list_factory_policies(&conn, &org).unwrap().len(), 1);
    }

    #[test]
    fn decisions_are_recorded_per_org_and_subject() {
        let (conn, org, _) = setup();
        let record = DecisionRecord {
            subject: "acme/web#42@0123456789abcdef0123456789abcdef01234567".into(),
            action: Action::Merge,
            verdict: ActionVerdict {
                verdict: Verdict::Hold,
                reason: "decision_model_not_configured".into(),
                source: VerdictSource::Policy,
            },
            policy_version: Some(2),
            provider: None,
            model: None,
            confidence: None,
            inputs: serde_json::json!({"task_class": "docs", "floors": []}),
        };
        record_decision(&conn, &org, &record).unwrap();
        assert_eq!(
            list_decisions(&conn, &org, &record.subject).unwrap(),
            vec![record.clone()]
        );
        assert!(list_decisions(&conn, "other-org", &record.subject)
            .unwrap()
            .is_empty());
    }

    const SHA_A: &str = "0123456789abcdef0123456789abcdef01234567";
    const SHA_B: &str = "fedcba9876543210fedcba9876543210fedcba98";

    #[test]
    fn every_new_evaluation_restarts_the_soak_with_its_own_run() {
        let (conn, org, _) = setup();
        let first =
            start_merge_soak(&conn, &org, "run-1", "acme/web", 42, SHA_A, &[], 600).unwrap();
        let checks = vec!["ci".to_string()];
        let again =
            start_merge_soak(&conn, &org, "run-2", "acme/web", 42, SHA_A, &checks, 600).unwrap();
        assert_ne!(again.id, first.id, "a re-review restarts the window");
        assert_eq!(again.run_id, "run-2", "authority follows the latest run");
        assert_eq!(again.required_checks, checks);
        let moved =
            start_merge_soak(&conn, &org, "run-3", "acme/web", 42, SHA_B, &checks, 600).unwrap();
        assert_eq!(moved.head_sha, SHA_B);
        assert_eq!(due_merge_soaks(&conn, 10).unwrap().len(), 0);
    }

    #[test]
    fn cancelling_removes_only_that_pull_requests_soak() {
        let (conn, org, _) = setup();
        start_merge_soak(&conn, &org, "run-1", "acme/web", 1, SHA_A, &[], -1).unwrap();
        let other = start_merge_soak(&conn, &org, "run-2", "acme/web", 2, SHA_B, &[], -1).unwrap();
        assert!(cancel_merge_soak(&conn, &org, "acme/web", 1).unwrap());
        assert!(!cancel_merge_soak(&conn, "other-org", "acme/web", 2).unwrap());
        assert_eq!(due_merge_soaks(&conn, 10).unwrap(), vec![other]);
    }

    #[test]
    fn only_elapsed_soaks_are_due_and_finishing_removes_them() {
        let (conn, org, _) = setup();
        let checks = vec![];
        start_merge_soak(&conn, &org, "run-1", "acme/web", 1, SHA_A, &checks, 600).unwrap();
        let due =
            start_merge_soak(&conn, &org, "run-2", "acme/web", 2, SHA_B, &checks, -1).unwrap();
        assert_eq!(due_merge_soaks(&conn, 10).unwrap(), vec![due.clone()]);
        finish_merge_soak(&conn, &due.id).unwrap();
        assert!(due_merge_soaks(&conn, 10).unwrap().is_empty());
    }

    #[test]
    fn scopes_are_distinct_rows() {
        let (conn, org, user) = setup();
        let mut docs = policy(Action::Merge, PolicyMode::Criteria, 1);
        docs.scope.task_class = Some(TaskClass::Docs);
        upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Merge, PolicyMode::Manual, 1),
        )
        .unwrap();
        upsert_factory_policy(&conn, &org, &user, &docs).unwrap();
        assert_eq!(list_factory_policies(&conn, &org).unwrap().len(), 2);
    }

    #[test]
    fn delete_is_scoped_to_the_org() {
        let (conn, org, user) = setup();
        let (created, _) = upsert_factory_policy(
            &conn,
            &org,
            &user,
            &policy(Action::Fix, PolicyMode::Manual, 1),
        )
        .unwrap();
        assert!(
            delete_factory_policy(&conn, "other-org", &user, &created.id)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            delete_factory_policy(&conn, &org, &user, &created.id).unwrap(),
            Some(created)
        );
        assert!(list_factory_policies(&conn, &org).unwrap().is_empty());
    }
}
