//! Usage-metrics data layer: ingest, rollup, and session backfill.
//!
//! Kept deliberately separate from `queries.rs` (which the clients-UI work is
//! editing concurrently). Everything here reads the `usage_events` table created
//! by migration `run_v59`, and resolves the task → project → client hierarchy
//! read-only against the existing `tasks`/`projects`/`sessions` tables.

use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::{types::Value, Connection, OptionalExtension};

use crate::models::types::{
    UsageBucket, UsageIngestRequest, UsageSummaryResponse, UsageSummaryRow,
    UsageTimeseriesResponse,
};

/// The `WHERE`-clause inputs shared by every read in this module.
struct UsageFilters<'a> {
    from: Option<&'a str>,
    to: Option<&'a str>,
    client: Option<&'a str>,
    project: Option<&'a str>,
    /// `None` = org-wide (super_user); `Some(uid)` = restrict to that user's
    /// visible projects via the `project_visibility` view.
    viewer: Option<&'a str>,
}

/// Turns an inclusive `to` filter into the comparison SQLite must actually run.
///
/// `event_ts` is lexicographically sortable but NOT written in a single shape:
/// `datetime('now')` produces `'YYYY-MM-DD HH:MM:SS'` while an agent may ingest
/// ISO-8601 `'YYYY-MM-DDTHH:MM:SS'`. So a date-only bound needs care:
///
/// - `event_ts <= '2026-08-14'` drops the entire final day (any time-of-day
///   suffix sorts after the bare date).
/// - `event_ts <= '2026-08-14 23:59:59'` still drops the ISO rows, because
///   `'T'` (0x54) sorts after `' '` (0x20).
///
/// The next day's midnight as an **exclusive** bound is correct for both shapes.
/// A `to` that already carries a time component is passed through inclusive.
fn upper_bound(to: &str) -> (&'static str, String) {
    match NaiveDate::parse_from_str(to.trim(), "%Y-%m-%d") {
        Ok(day) => match day.succ_opt() {
            Some(next) => ("<", next.to_string()),
            // Only reachable at chrono's max date; degrade to inclusive.
            None => ("<=", to.to_string()),
        },
        Err(_) => ("<=", to.to_string()),
    }
}

/// Appends the shared `AND …` predicates to `sql`, binding into `params`.
///
/// `next` is the highest placeholder index used so far and is advanced in place,
/// so callers can keep binding after this returns.
fn push_filters(
    sql: &mut String,
    params: &mut Vec<Value>,
    next: &mut usize,
    org_id: &str,
    f: &UsageFilters<'_>,
) {
    if let Some(from) = f.from {
        *next += 1;
        sql.push_str(&format!(" AND e.event_ts >= ?{next}"));
        params.push(Value::Text(from.to_string()));
    }
    if let Some(to) = f.to {
        let (op, bound) = upper_bound(to);
        *next += 1;
        sql.push_str(&format!(" AND e.event_ts {op} ?{next}"));
        params.push(Value::Text(bound));
    }
    if let Some(cid) = f.client {
        *next += 1;
        sql.push_str(&format!(" AND e.client_id = ?{next}"));
        params.push(Value::Text(cid.to_string()));
    }
    if let Some(pid) = f.project {
        *next += 1;
        sql.push_str(&format!(" AND e.project_id = ?{next}"));
        params.push(Value::Text(pid.to_string()));
    }
    if let Some(uid) = f.viewer {
        *next += 1;
        let uparam = *next;
        *next += 1;
        let oparam = *next;
        sql.push_str(&format!(
            " AND e.project_id IN (SELECT pv.project_id FROM project_visibility pv \
              WHERE pv.user_id = ?{uparam} AND pv.org_id = ?{oparam})"
        ));
        params.push(Value::Text(uid.to_string()));
        params.push(Value::Text(org_id.to_string()));
    }
}

/// Inserts one `source='ingest'` usage event, resolving the hierarchy server-side.
///
/// Resolution rule (design.md): a caller-supplied `project` name wins; otherwise,
/// if `task_id` is given, the project is derived from that task's `project` name.
/// The project name is resolved to an existing `project_id` — never auto-created.
/// `client_id` is a snapshot of that project's current `client_id`. Any id that
/// does not resolve within the org is stored as NULL rather than rejected, so a
/// stale or bogus id can never turn telemetry into a 500.
pub fn insert_usage_event(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    req: &UsageIngestRequest,
) -> Result<String> {
    // Keep task_id only if it belongs to this org.
    let task_id: Option<String> = match req.task_id.as_deref() {
        Some(tid) => conn
            .query_row(
                "SELECT id FROM tasks WHERE id = ?1 AND org_id = ?2",
                rusqlite::params![tid, org_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?,
        None => None,
    };

    // Project NAME: caller-supplied wins; else derive from the resolved task.
    let mut project_name: Option<String> = req.project.clone();
    if project_name.is_none() {
        if let Some(tid) = task_id.as_deref() {
            project_name = conn
                .query_row(
                    "SELECT project FROM tasks WHERE id = ?1 AND org_id = ?2",
                    rusqlite::params![tid, org_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?;
        }
    }

    // Project NAME -> existing project_id (never auto-create).
    let project_id: Option<String> = match project_name.as_deref() {
        Some(name) => conn
            .query_row(
                "SELECT id FROM projects WHERE org_id = ?1 AND name = ?2",
                rusqlite::params![org_id, name],
                |r| r.get::<_, String>(0),
            )
            .optional()?,
        None => None,
    };

    // client_id = that project's current client_id (snapshot; may be NULL for
    // internal projects).
    let client_id: Option<String> = match project_id.as_deref() {
        Some(pid) => conn
            .query_row(
                "SELECT client_id FROM projects WHERE id = ?1",
                rusqlite::params![pid],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten(),
        None => None,
    };

    // Keep session_id only if it belongs to this org.
    let session_id: Option<String> = match req.session_id.as_deref() {
        Some(sid) => conn
            .query_row(
                "SELECT id FROM sessions WHERE id = ?1 AND org_id = ?2",
                rusqlite::params![sid, org_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?,
        None => None,
    };

    let id = uuid::Uuid::new_v4().to_string();
    let tokens_in = req.tokens_in.max(0);
    let tokens_out = req.tokens_out.max(0);
    let duration_ms = req.duration_ms.max(0);

    conn.execute(
        "INSERT INTO usage_events
            (id, org_id, user_id, client_id, project_id, task_id, session_id,
             model, tokens_in, tokens_out, duration_ms, source, event_ts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'ingest',
                 COALESCE(?12, datetime('now')))",
        rusqlite::params![
            id,
            org_id,
            user_id,
            client_id,
            project_id,
            task_id,
            session_id,
            req.model,
            tokens_in,
            tokens_out,
            duration_ms,
            req.event_ts,
        ],
    )?;
    Ok(id)
}

/// Aggregates usage at the requested rollup level.
///
/// `viewer_user_id`:
/// - `None` — super_user / org-wide, no visibility restriction.
/// - `Some(uid)` — restrict to events whose `project_id` is visible to `uid`
///   through the `project_visibility` view (project or client membership).
///
/// `level` must be one of `task | project | client | org | model | user`
/// (validated by the caller).
pub fn usage_summary(
    conn: &Connection,
    org_id: &str,
    level: &str,
    from: Option<&str>,
    to: Option<&str>,
    filter_client: Option<&str>,
    filter_project: Option<&str>,
    viewer_user_id: Option<&str>,
) -> Result<UsageSummaryResponse> {
    let (group_col, name_sql, name_join) = match level {
        "task" => (
            "e.task_id",
            "COALESCE(t.title, '(unassigned)')",
            "LEFT JOIN tasks t ON t.id = e.task_id",
        ),
        "project" => (
            "e.project_id",
            "COALESCE(p.name, '(no project)')",
            "LEFT JOIN projects p ON p.id = e.project_id",
        ),
        "client" => (
            "e.client_id",
            "COALESCE(c.name, '(internal)')",
            "LEFT JOIN clients c ON c.id = e.client_id",
        ),
        "org" => (
            "e.org_id",
            "COALESCE(o.name, e.org_id)",
            "LEFT JOIN organizations o ON o.id = e.org_id",
        ),
        // `model` groups on the free-text column itself, so key_id IS the model
        // name. No join exists to resolve it — the value is whatever the agent
        // reported, and NULL means the agent did not report one.
        "model" => (
            "e.model",
            "COALESCE(NULLIF(TRIM(e.model), ''), '(unreported)')",
            "",
        ),
        // `user` attributes usage to the operator. Backfilled rows carry no
        // user_id (sessions have no author), hence the '(system)' bucket.
        "user" => (
            "e.user_id",
            "COALESCE(NULLIF(TRIM(u.name), ''), u.email, '(system)')",
            "LEFT JOIN users u ON u.id = e.user_id",
        ),
        other => anyhow::bail!("invalid usage summary level: {other}"),
    };

    let mut sql = format!(
        "SELECT {group_col} AS key_id,
                {name_sql} AS key_name,
                COALESCE(SUM(e.tokens_in), 0),
                COALESCE(SUM(e.tokens_out), 0),
                COALESCE(SUM(e.duration_ms), 0),
                COUNT(*)
           FROM usage_events e
           {name_join}
          WHERE e.org_id = ?1"
    );

    let mut params: Vec<Value> = vec![Value::Text(org_id.to_string())];
    let mut next = 1;

    push_filters(
        &mut sql,
        &mut params,
        &mut next,
        org_id,
        &UsageFilters {
            from,
            to,
            client: filter_client,
            project: filter_project,
            viewer: viewer_user_id,
        },
    );

    sql.push_str(&format!(
        " GROUP BY {group_col} ORDER BY (COALESCE(SUM(e.tokens_in),0) + COALESCE(SUM(e.tokens_out),0)) DESC"
    ));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
            let tokens_in: i64 = r.get(2)?;
            let tokens_out: i64 = r.get(3)?;
            Ok(UsageSummaryRow {
                key_id: r.get::<_, Option<String>>(0)?,
                key_name: r.get::<_, String>(1)?,
                tokens_in,
                tokens_out,
                tokens_total: tokens_in + tokens_out,
                duration_ms: r.get(4)?,
                event_count: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let totals = rows.iter().fold(
        UsageSummaryRow {
            key_id: None,
            key_name: "total".to_string(),
            tokens_in: 0,
            tokens_out: 0,
            tokens_total: 0,
            duration_ms: 0,
            event_count: 0,
        },
        |mut acc, r| {
            acc.tokens_in += r.tokens_in;
            acc.tokens_out += r.tokens_out;
            acc.tokens_total += r.tokens_total;
            acc.duration_ms += r.duration_ms;
            acc.event_count += r.event_count;
            acc
        },
    );

    Ok(UsageSummaryResponse { rows, totals })
}

/// Aggregates usage into time buckets for the trend chart.
///
/// `bucket` is one of `hour | day | week` (validated by the caller). The bucket
/// key is derived with string/date functions rather than `strftime` on a parsed
/// timestamp so that both stored shapes of `event_ts` (`'… HH:MM:SS'` from
/// `datetime('now')` and ISO `'…THH:MM:SS'` from an agent) collapse into the
/// same bucket — `replace(…, 'T', ' ')` is what makes the hour case agree.
///
/// Only non-empty buckets are returned; the caller gap-fills, since it is the
/// side that knows the requested range. `viewer_user_id` scopes exactly as in
/// [`usage_summary`].
pub fn usage_timeseries(
    conn: &Connection,
    org_id: &str,
    bucket: &str,
    from: Option<&str>,
    to: Option<&str>,
    filter_client: Option<&str>,
    filter_project: Option<&str>,
    viewer_user_id: Option<&str>,
) -> Result<UsageTimeseriesResponse> {
    let bucket_expr = match bucket {
        "hour" => "replace(substr(e.event_ts, 1, 13), 'T', ' ')",
        "day" => "substr(e.event_ts, 1, 10)",
        // Monday-anchored week. `-6 days` then `weekday 1` lands on the Monday
        // of the event's own week (a Monday stays put, a Sunday walks back).
        "week" => "date(substr(e.event_ts, 1, 10), '-6 days', 'weekday 1')",
        other => anyhow::bail!("invalid usage timeseries bucket: {other}"),
    };

    let mut sql = format!(
        "SELECT {bucket_expr} AS bucket_ts,
                COALESCE(SUM(e.tokens_in), 0),
                COALESCE(SUM(e.tokens_out), 0),
                COALESCE(SUM(e.duration_ms), 0),
                COUNT(*)
           FROM usage_events e
          WHERE e.org_id = ?1"
    );

    let mut params: Vec<Value> = vec![Value::Text(org_id.to_string())];
    let mut next = 1;

    push_filters(
        &mut sql,
        &mut params,
        &mut next,
        org_id,
        &UsageFilters {
            from,
            to,
            client: filter_client,
            project: filter_project,
            viewer: viewer_user_id,
        },
    );

    sql.push_str(" GROUP BY bucket_ts ORDER BY bucket_ts ASC");

    let mut stmt = conn.prepare(&sql)?;
    let buckets = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
            let tokens_in: i64 = r.get(1)?;
            let tokens_out: i64 = r.get(2)?;
            Ok(UsageBucket {
                bucket_ts: r.get::<_, String>(0)?,
                tokens_in,
                tokens_out,
                tokens_total: tokens_in + tokens_out,
                duration_ms: r.get(3)?,
                event_count: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(UsageTimeseriesResponse {
        bucket: bucket.to_string(),
        buckets,
    })
}

/// Best-effort backfill: one `source='backfill'` usage row per org session that
/// does not already have one. Sessions carry no token data, so only time and a
/// count are derivable — tokens stay 0.
///
/// Duration is `max(0, ended_at - started_at)` in milliseconds when `ended_at`
/// is present, else 0. `project_id` (and its `client_id`) are resolved from the
/// session's `project` name; unresolved names leave both NULL. Idempotent via
/// the partial-unique `idx_usage_backfill_session` index + `INSERT OR IGNORE`;
/// returns the number of rows actually inserted this call.
pub fn backfill_from_sessions(conn: &Connection, org_id: &str) -> Result<i64> {
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO usage_events
            (id, org_id, user_id, client_id, project_id, task_id, session_id,
             model, tokens_in, tokens_out, duration_ms, source, event_ts)
         SELECT
            lower(hex(randomblob(16))),
            s.org_id,
            NULL,
            p.client_id,
            p.id,
            NULL,
            s.id,
            NULL,
            0,
            0,
            CASE
                WHEN s.ended_at IS NULL THEN 0
                ELSE MAX(0, CAST((julianday(s.ended_at) - julianday(s.started_at)) * 86400000 AS INTEGER))
            END,
            'backfill',
            s.started_at
         FROM sessions s
         LEFT JOIN projects p ON p.org_id = s.org_id AND p.name = s.project
         WHERE s.org_id = ?1",
        rusqlite::params![org_id],
    )?;
    Ok(inserted as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::db::migrations::run_all;

    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        run_all(&conn).unwrap();
        conn
    }

    fn seed_org(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, ?1, ?1)",
            rusqlite::params![id],
        )
        .unwrap();
    }

    fn seed_user(conn: &Connection, id: &str, org: &str) {
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES (?1, ?2, ?1 || '@x', ?1)",
            rusqlite::params![id, org],
        )
        .unwrap();
    }

    fn seed_client(conn: &Connection, id: &str, org: &str, slug: &str) {
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES (?1, ?2, ?1, ?3)",
            rusqlite::params![id, org, slug],
        )
        .unwrap();
    }

    fn seed_project(conn: &Connection, id: &str, org: &str, name: &str, client: Option<&str>) {
        conn.execute(
            "INSERT INTO projects (id, org_id, name, client_id) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, org, name, client],
        )
        .unwrap();
    }

    fn seed_task(conn: &Connection, id: &str, org: &str, project: &str, creator: &str) {
        conn.execute(
            "INSERT INTO tasks (id, org_id, project, title, created_by)
             VALUES (?1, ?2, ?3, ?1, ?4)",
            rusqlite::params![id, org, project, creator],
        )
        .unwrap();
    }

    #[test]
    fn ingest_resolves_project_and_client_from_task() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_user(&conn, "u1", "org1");
        seed_client(&conn, "cl1", "org1", "acme");
        seed_project(&conn, "p1", "org1", "acme-web", Some("cl1"));
        seed_task(&conn, "t1", "org1", "acme-web", "u1");

        let req = UsageIngestRequest {
            task_id: Some("t1".to_string()),
            tokens_in: 100,
            tokens_out: 50,
            duration_ms: 1234,
            ..Default::default()
        };
        let id = insert_usage_event(&conn, "org1", "u1", &req).unwrap();

        let (pid, cid, tin, tout): (Option<String>, Option<String>, i64, i64) = conn
            .query_row(
                "SELECT project_id, client_id, tokens_in, tokens_out FROM usage_events WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(pid.as_deref(), Some("p1"), "project derived from task");
        assert_eq!(cid.as_deref(), Some("cl1"), "client snapshot from project");
        assert_eq!(tin, 100);
        assert_eq!(tout, 50);
    }

    #[test]
    fn ingest_stores_unresolvable_ids_as_null() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_user(&conn, "u1", "org1");

        let req = UsageIngestRequest {
            project: Some("does-not-exist".to_string()),
            task_id: Some("ghost".to_string()),
            session_id: Some("ghost".to_string()),
            tokens_in: 10,
            ..Default::default()
        };
        // Must not error even though every id is bogus.
        let id = insert_usage_event(&conn, "org1", "u1", &req).unwrap();
        let (pid, tid, sid): (Option<String>, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT project_id, task_id, session_id FROM usage_events WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(pid.is_none() && tid.is_none() && sid.is_none());
    }

    #[test]
    fn summary_rolls_up_at_project_level() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_user(&conn, "u1", "org1");
        seed_project(&conn, "p1", "org1", "alpha", None);
        seed_project(&conn, "p2", "org1", "beta", None);

        for (proj, tin, tout, dur) in [
            ("alpha", 100, 40, 1000),
            ("alpha", 50, 10, 500),
            ("beta", 200, 100, 2000),
        ] {
            let req = UsageIngestRequest {
                project: Some(proj.to_string()),
                tokens_in: tin,
                tokens_out: tout,
                duration_ms: dur,
                ..Default::default()
            };
            insert_usage_event(&conn, "org1", "u1", &req).unwrap();
        }

        let resp = usage_summary(&conn, "org1", "project", None, None, None, None, None).unwrap();
        assert_eq!(resp.rows.len(), 2, "one row per project");

        let alpha = resp
            .rows
            .iter()
            .find(|r| r.key_id.as_deref() == Some("p1"))
            .unwrap();
        assert_eq!(alpha.tokens_in, 150);
        assert_eq!(alpha.tokens_out, 50);
        assert_eq!(alpha.tokens_total, 200);
        assert_eq!(alpha.duration_ms, 1500);
        assert_eq!(alpha.event_count, 2);

        assert_eq!(resp.totals.tokens_total, 200 + 300);
        assert_eq!(resp.totals.event_count, 3);
    }

    #[test]
    fn summary_scopes_out_invisible_projects_for_viewer() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_user(&conn, "viewer", "org1");
        // p1 is visible to `viewer` (direct project membership); p2 is not.
        seed_project(&conn, "p1", "org1", "visible", None);
        seed_project(&conn, "p2", "org1", "hidden", None);
        conn.execute(
            "INSERT INTO project_members (id, project_id, user_id, role)
             VALUES ('pm1', 'p1', 'viewer', 'member')",
            [],
        )
        .unwrap();

        for proj in ["visible", "hidden"] {
            let req = UsageIngestRequest {
                project: Some(proj.to_string()),
                tokens_in: 100,
                ..Default::default()
            };
            insert_usage_event(&conn, "org1", "viewer", &req).unwrap();
        }

        // Org-wide (super_user) sees both.
        let wide = usage_summary(&conn, "org1", "project", None, None, None, None, None).unwrap();
        assert_eq!(wide.rows.len(), 2);

        // Scoped viewer sees only the project they are a member of.
        let scoped =
            usage_summary(&conn, "org1", "project", None, None, None, None, Some("viewer")).unwrap();
        assert_eq!(scoped.rows.len(), 1, "hidden project must be excluded");
        assert_eq!(scoped.rows[0].key_id.as_deref(), Some("p1"));
        assert_eq!(scoped.totals.event_count, 1);
    }

    #[test]
    fn summary_scoping_via_client_membership() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_user(&conn, "viewer", "org1");
        seed_client(&conn, "cl1", "org1", "acme");
        // Project belongs to a client the viewer is a member of — visible via
        // the client branch of project_visibility.
        seed_project(&conn, "p1", "org1", "acme-web", Some("cl1"));
        conn.execute(
            "INSERT INTO client_members (id, client_id, user_id, role)
             VALUES ('cm1', 'cl1', 'viewer', 'member')",
            [],
        )
        .unwrap();

        let req = UsageIngestRequest {
            project: Some("acme-web".to_string()),
            tokens_in: 42,
            ..Default::default()
        };
        insert_usage_event(&conn, "org1", "viewer", &req).unwrap();

        let scoped =
            usage_summary(&conn, "org1", "project", None, None, None, None, Some("viewer")).unwrap();
        assert_eq!(scoped.rows.len(), 1, "client membership grants visibility");
        assert_eq!(scoped.rows[0].tokens_in, 42);
    }

    /// Inserts a raw event at an explicit timestamp, bypassing `insert_usage_event`
    /// so the test can choose the exact stored `event_ts` shape.
    fn seed_event_at(conn: &Connection, id: &str, org: &str, ts: &str, tokens_in: i64) {
        conn.execute(
            "INSERT INTO usage_events
                (id, org_id, tokens_in, tokens_out, duration_ms, source, event_ts)
             VALUES (?1, ?2, ?3, 0, 0, 'ingest', ?4)",
            rusqlite::params![id, org, tokens_in, ts],
        )
        .unwrap();
    }

    #[test]
    fn to_filter_includes_the_whole_final_day_in_both_ts_shapes() {
        let conn = setup();
        seed_org(&conn, "org1");
        // `datetime('now')` shape and the ISO-8601 shape an agent may ingest.
        seed_event_at(&conn, "e1", "org1", "2026-08-14 10:00:00", 10);
        seed_event_at(&conn, "e2", "org1", "2026-08-14T23:59:00", 20);
        // Day after the bound — must stay excluded.
        seed_event_at(&conn, "e3", "org1", "2026-08-15 00:00:01", 40);

        let resp = usage_summary(
            &conn,
            "org1",
            "org",
            Some("2026-08-14"),
            Some("2026-08-14"),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            resp.totals.tokens_in, 30,
            "both shapes of the final day are included, the next day is not"
        );
    }

    #[test]
    fn summary_rolls_up_by_model_and_user() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_user(&conn, "u1", "org1");

        for (model, tin) in [(Some("opus"), 100), (Some("opus"), 50), (None, 7)] {
            let req = UsageIngestRequest {
                model: model.map(str::to_string),
                tokens_in: tin,
                ..Default::default()
            };
            insert_usage_event(&conn, "org1", "u1", &req).unwrap();
        }

        let by_model = usage_summary(&conn, "org1", "model", None, None, None, None, None).unwrap();
        assert_eq!(by_model.rows.len(), 2, "one row per model plus the NULL bucket");
        let opus = by_model.rows.iter().find(|r| r.key_name == "opus").unwrap();
        assert_eq!(opus.tokens_in, 150);
        assert!(
            by_model.rows.iter().any(|r| r.key_name == "(unreported)"),
            "events with no model land in a labelled bucket, not a blank one"
        );

        let by_user = usage_summary(&conn, "org1", "user", None, None, None, None, None).unwrap();
        assert_eq!(by_user.rows.len(), 1);
        assert_eq!(by_user.rows[0].key_id.as_deref(), Some("u1"));
        assert_eq!(by_user.rows[0].tokens_in, 157);
    }

    #[test]
    fn timeseries_buckets_by_day_across_ts_shapes() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_event_at(&conn, "e1", "org1", "2026-08-12 09:00:00", 10);
        seed_event_at(&conn, "e2", "org1", "2026-08-12T15:00:00", 5);
        seed_event_at(&conn, "e3", "org1", "2026-08-14 09:00:00", 100);

        let ts =
            usage_timeseries(&conn, "org1", "day", None, None, None, None, None).unwrap();
        assert_eq!(ts.bucket, "day");
        // 08-13 has no events and is NOT emitted — the client gap-fills.
        assert_eq!(ts.buckets.len(), 2, "only non-empty buckets are returned");
        assert_eq!(ts.buckets[0].bucket_ts, "2026-08-12");
        assert_eq!(
            ts.buckets[0].tokens_total, 15,
            "both ts shapes collapse into the same day bucket"
        );
        assert_eq!(ts.buckets[0].event_count, 2);
        assert_eq!(ts.buckets[1].bucket_ts, "2026-08-14");
    }

    #[test]
    fn timeseries_hour_bucket_normalizes_the_iso_separator() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_event_at(&conn, "e1", "org1", "2026-08-12 09:15:00", 10);
        seed_event_at(&conn, "e2", "org1", "2026-08-12T09:45:00", 5);

        let ts = usage_timeseries(&conn, "org1", "hour", None, None, None, None, None).unwrap();
        assert_eq!(ts.buckets.len(), 1, "same hour, different ts shape → one bucket");
        assert_eq!(ts.buckets[0].bucket_ts, "2026-08-12 09");
        assert_eq!(ts.buckets[0].tokens_total, 15);
    }

    #[test]
    fn timeseries_week_bucket_anchors_on_monday() {
        let conn = setup();
        seed_org(&conn, "org1");
        // 2026-08-10 is a Monday; 2026-08-16 is the Sunday of that same week.
        seed_event_at(&conn, "e1", "org1", "2026-08-10 09:00:00", 10);
        seed_event_at(&conn, "e2", "org1", "2026-08-16 22:00:00", 5);
        // Next Monday starts a new bucket.
        seed_event_at(&conn, "e3", "org1", "2026-08-17 01:00:00", 1);

        let ts = usage_timeseries(&conn, "org1", "week", None, None, None, None, None).unwrap();
        assert_eq!(ts.buckets.len(), 2);
        assert_eq!(ts.buckets[0].bucket_ts, "2026-08-10");
        assert_eq!(ts.buckets[0].tokens_total, 15, "Mon..Sun collapse into one week");
        assert_eq!(ts.buckets[1].bucket_ts, "2026-08-17");
    }

    #[test]
    fn timeseries_respects_viewer_scoping() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_user(&conn, "viewer", "org1");
        seed_project(&conn, "p1", "org1", "visible", None);
        seed_project(&conn, "p2", "org1", "hidden", None);
        conn.execute(
            "INSERT INTO project_members (id, project_id, user_id, role)
             VALUES ('pm1', 'p1', 'viewer', 'member')",
            [],
        )
        .unwrap();

        for proj in ["visible", "hidden"] {
            let req = UsageIngestRequest {
                project: Some(proj.to_string()),
                tokens_in: 100,
                event_ts: Some("2026-08-14 10:00:00".to_string()),
                ..Default::default()
            };
            insert_usage_event(&conn, "org1", "viewer", &req).unwrap();
        }

        let scoped =
            usage_timeseries(&conn, "org1", "day", None, None, None, None, Some("viewer")).unwrap();
        assert_eq!(scoped.buckets.len(), 1);
        assert_eq!(
            scoped.buckets[0].tokens_total, 100,
            "the hidden project's tokens must not leak into the trend"
        );
    }

    #[test]
    fn backfill_is_idempotent() {
        let conn = setup();
        seed_org(&conn, "org1");
        seed_project(&conn, "p1", "org1", "alpha", None);
        conn.execute(
            "INSERT INTO sessions (id, org_id, project, started_at, ended_at)
             VALUES ('s1', 'org1', 'alpha', '2026-08-14T10:00:00', '2026-08-14T10:00:05')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions (id, org_id, project, started_at, ended_at)
             VALUES ('s2', 'org1', 'unknown-proj', '2026-08-14T11:00:00', NULL)",
            [],
        )
        .unwrap();

        let first = backfill_from_sessions(&conn, "org1").unwrap();
        assert_eq!(first, 2, "one backfill row per session");

        let second = backfill_from_sessions(&conn, "org1").unwrap();
        assert_eq!(second, 0, "re-running inserts nothing");

        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE source = 'backfill'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 2, "row count stable after a second run");

        // s1: 5 seconds -> 5000ms, project resolved. s2: NULL end -> 0ms, no project.
        let (dur1, pid1): (i64, Option<String>) = conn
            .query_row(
                "SELECT duration_ms, project_id FROM usage_events WHERE session_id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(dur1, 5000);
        assert_eq!(pid1.as_deref(), Some("p1"));

        let (dur2, pid2): (i64, Option<String>) = conn
            .query_row(
                "SELECT duration_ms, project_id FROM usage_events WHERE session_id = 's2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(dur2, 0, "NULL ended_at clamps to 0");
        assert_eq!(pid2, None, "unresolved project name stays NULL");
    }
}
