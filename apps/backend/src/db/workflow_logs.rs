//! Observable workflow execution on top of `factory_workflow_events`, without a
//! schema change: attributed runner events (attempt, revision and request id are
//! derived from the lease, never trusted from the producer), cursor-paginated
//! history, separate budgets for output and control events, correlated backend
//! rejections and retention of bulky output once a run is finished.
use anyhow::{bail, Result};
use regex::Regex;
use rusqlite::{params, types::ToSql, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::OnceLock;

/// High-volume output. Everything else is a control event (results, diagnostics,
/// cancellations) and keeps its own budget so output can never crowd it out.
pub const LOG_KINDS: &[&str] = &["log.chunk", "service.log", "dependency.log"];
const LOG_SQL: &str = "('log.chunk','service.log','dependency.log')";
const RUNNER_KINDS: &[&str] = &[
    "agent.started",
    "agent.tool",
    "command.started",
    "command.finished",
    "service.ready",
    "log.chunk",
    "service.log",
    "dependency.log",
    "runner.diagnostic",
    "runner.connectivity",
    "infra.incident",
];
pub const CONTROL_LIMIT: i64 = 3000;
pub const LOG_LIMIT: i64 = 6000;
const REJECTION_LIMIT: i64 = 100;
/// Retried sends are recent; a bounded window keeps de-duplication cheap.
const DEDUPE_WINDOW: i64 = 400;
const SOURCES: &[&str] = &[
    "runner",
    "agent",
    "check",
    "dependency",
    "service",
    "publication",
    "cleanup",
    "infrastructure",
];
const STREAMS: &[&str] = &["combined", "stdout", "stderr", "system"];
const SEVERITIES: &[&str] = &["debug", "info", "warning", "error"];
pub const DEFAULT_RETENTION_DAYS: i64 = 14;

static BOOT: OnceLock<String> = OnceLock::new();

/// Records when this backend process started, in SQLite `datetime('now')` form,
/// so a run can show that the control plane restarted while it was active.
pub fn mark_boot() -> &'static str {
    BOOT.get_or_init(|| chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
}

fn patterns() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            (r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----", "[REDACTED PRIVATE KEY]"),
            (r#"(?i)\b(authorization|proxy-authorization|cookie|set-cookie|x-api-key)(\s*[:=]\s*)[^\r\n"]+"#, "$1$2[REDACTED]"),
            (r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}", "Bearer [REDACTED]"),
            (r"([a-zA-Z][a-zA-Z0-9+.-]*://)[^/\s:@]+:[^/\s@]+@", "$1[REDACTED]@"),
            (r"\b(nmr_|nm_live_|ghp_|gho_|ghu_|ghs_|ghr_|github_pat_|sk-ant-|sk-|xox[abprs]-)[A-Za-z0-9_-]{16,}", "[REDACTED]"),
            (r"\bAKIA[0-9A-Z]{16}\b", "[REDACTED]"),
        ]
        .into_iter()
        .map(|(p, r)| (Regex::new(p).expect("static redaction pattern"), r))
        .collect()
    })
}

/// Defense in depth: the runner redacts known secret values before sending; the
/// backend also removes well-known credential shapes before persisting.
pub fn redact(text: &str) -> String {
    let mut out = text.to_owned();
    for (pattern, replacement) in patterns() {
        if pattern.is_match(&out) {
            out = pattern.replace_all(&out, *replacement).into_owned();
        }
    }
    out
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ingest {
    pub lease_token: String,
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub stream: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    /// Byte offset of this chunk in its producer stream; makes resends idempotent.
    #[serde(default)]
    pub offset: Option<u64>,
    #[serde(default)]
    pub origin_ts: Option<i64>,
    #[serde(default)]
    pub data: Option<Value>,
}

fn allowed(value: &Option<String>, set: &[&str]) -> Result<()> {
    match value {
        Some(v) if !set.contains(&v.as_str()) => bail!("invalid_event"),
        _ => Ok(()),
    }
}

fn class_count(c: &Connection, run: &str, logs: bool) -> Result<i64> {
    let sql = format!(
        "SELECT count(*) FROM factory_workflow_events WHERE run_id=?1 AND kind {} IN {LOG_SQL}",
        if logs { "" } else { "NOT" }
    );
    Ok(c.query_row(&sql, [run], |r| r.get(0))?)
}

#[allow(clippy::too_many_arguments)]
pub fn ingest(
    c: &Connection,
    org: &str,
    runner: &str,
    run: &str,
    step: &str,
    e: &Ingest,
    request_id: Option<&str>,
    now: i64,
) -> Result<Value> {
    if !RUNNER_KINDS.contains(&e.kind.as_str()) || e.message.len() > 4096 {
        bail!("invalid_event")
    }
    allowed(&e.source, SOURCES)?;
    allowed(&e.stream, STREAMS)?;
    allowed(&e.severity, SEVERITIES)?;
    if e
        .repository
        .as_ref()
        .is_some_and(|r| r.len() > 200 || r.chars().any(char::is_control))
        || e
            .data
            .as_ref()
            .is_some_and(|d| !d.is_object() || d.to_string().len() > 2048)
    {
        bail!("invalid_event")
    }
    let attempt = super::workflow_execution::lease(c, org, runner, run, step, &e.lease_token, now)?;
    let revision: u32 = c.query_row(
        "SELECT input_revision FROM factory_workflow_steps WHERE run_id=?1 AND step_id=?2",
        params![run, step],
        |r| r.get(0),
    )?;
    let stream = e.stream.as_deref().unwrap_or("combined");
    if let Some(offset) = e.offset {
        let duplicate: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM (SELECT step_id,kind,payload_json FROM factory_workflow_events WHERE run_id=?1 ORDER BY seq DESC LIMIT ?6)
               WHERE step_id=?2 AND kind=?3 AND json_extract(payload_json,'$.attempt')=?4
                 AND json_extract(payload_json,'$.offset')=?5 AND json_extract(payload_json,'$.stream')=?7)",
            params![run, step, e.kind, attempt, offset as i64, DEDUPE_WINDOW, stream],
            |r| r.get(0),
        )?;
        if duplicate {
            return Ok(json!({"recorded": true, "duplicate": true}));
        }
    }
    let logs = LOG_KINDS.contains(&e.kind.as_str());
    let limit = if logs { LOG_LIMIT } else { CONTROL_LIMIT };
    if class_count(c, run, logs)? >= limit {
        let class = if logs { "logs" } else { "control" };
        let marked: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM factory_workflow_events WHERE run_id=?1 AND kind='capture.truncated' AND json_extract(payload_json,'$.class')=?2)",
            params![run, class],
            |r| r.get(0),
        )?;
        if !marked {
            super::workflows::event(
                c,
                run,
                Some(step),
                "capture.truncated",
                json!({"class": class, "limit": limit, "attempt": attempt, "input_revision": revision}),
            )?;
        }
        bail!("event_quota")
    }
    let mut payload = Map::new();
    payload.insert("message".into(), redact(&e.message).into());
    payload.insert("attempt".into(), attempt.into());
    payload.insert("input_revision".into(), revision.into());
    payload.insert("source".into(), e.source.as_deref().unwrap_or("runner").into());
    payload.insert("stream".into(), stream.into());
    payload.insert("severity".into(), e.severity.as_deref().unwrap_or("info").into());
    payload.insert("received_at".into(), now.into());
    if let Some(r) = &e.repository {
        payload.insert("repository".into(), r.as_str().into());
    }
    if let Some(o) = e.offset {
        payload.insert("offset".into(), o.into());
    }
    if let Some(t) = e.origin_ts {
        payload.insert("origin_ts".into(), t.into());
    }
    if let Some(id) = request_id {
        payload.insert("request_id".into(), id.into());
    }
    if let Some(d) = &e.data {
        let cleaned = serde_json::from_str::<Value>(&redact(&d.to_string()))
            .unwrap_or_else(|_| json!({"redaction_failed": true}));
        payload.insert("data".into(), cleaned);
    }
    super::workflows::event(c, run, Some(step), &e.kind, Value::Object(payload))?;
    Ok(json!({"recorded": true}))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub after: Option<i64>,
    pub before: Option<i64>,
    pub limit: Option<u32>,
    pub step: Option<String>,
    pub attempt: Option<u32>,
    /// `logs` or `control`.
    pub class: Option<String>,
}

/// One page of a run's history. Without `after` it returns the newest page
/// (optionally older than `before`); with `after` it returns what is newer, for
/// polling. Ordering is the backend ingestion sequence, never a producer clock.
pub fn list(c: &Connection, org: &str, run: &str, q: &Query) -> Result<Value> {
    let (created_at, runner): (String, Option<String>) = c
        .query_row(
            "SELECT created_at,runner_id FROM factory_workflow_runs WHERE id=?1 AND org_id=?2",
            params![run, org],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("not_found"))?;
    let limit = i64::from(q.limit.unwrap_or(200).clamp(1, 500));
    let mut sql = String::from(
        "SELECT seq,step_id,kind,payload_json,created_at FROM factory_workflow_events WHERE run_id=?",
    );
    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(run.to_owned())];
    if let Some(step) = &q.step {
        sql.push_str(" AND step_id=?");
        args.push(Box::new(step.clone()));
    }
    if let Some(attempt) = q.attempt {
        // Events from before attribution existed carry no attempt; keep them
        // visible (the panel labels them) rather than silently hiding history.
        sql.push_str(" AND coalesce(json_extract(payload_json,'$.attempt'),?)=?");
        args.push(Box::new(attempt));
        args.push(Box::new(attempt));
    }
    match q.class.as_deref() {
        Some("logs") => sql.push_str(&format!(" AND kind IN {LOG_SQL}")),
        Some("control") => sql.push_str(&format!(" AND kind NOT IN {LOG_SQL}")),
        Some(_) => bail!("invalid_query"),
        None => {}
    }
    let ascending = q.after.is_some();
    if let Some(after) = q.after {
        sql.push_str(" AND seq>?");
        args.push(Box::new(after));
    } else if let Some(before) = q.before {
        sql.push_str(" AND seq<?");
        args.push(Box::new(before));
    }
    sql.push_str(if ascending { " ORDER BY seq ASC LIMIT ?" } else { " ORDER BY seq DESC LIMIT ?" });
    args.push(Box::new(limit + 1));
    let mut s = c.prepare(&sql)?;
    let mut events = s
        .query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .map(|r| {
            let (seq, step, kind, payload, at) = r?;
            Ok(json!({"seq":seq,"step_id":step,"kind":kind,"payload":serde_json::from_str::<Value>(&payload)?,"created_at":at}))
        })
        .collect::<Result<Vec<Value>>>()?;
    let more = events.len() as i64 > limit;
    events.truncate(limit as usize);
    if !ascending {
        events.reverse();
    }
    let runner = match runner {
        Some(id) => c
            .query_row(
                "SELECT name,last_seen,revoked FROM factory_runners WHERE id=?1",
                [&id],
                |r| Ok(json!({"name":r.get::<_,String>(0)?,"last_seen":r.get::<_,Option<String>>(1)?,"revoked":r.get::<_,bool>(2)?})),
            )
            .optional()?,
        None => None,
    };
    Ok(json!({
        "events": events,
        "has_older": !ascending && more,
        "has_newer": ascending && more,
        "coverage": coverage(c, run, q.step.as_deref(), q.attempt)?,
        "context": {"run_created_at": created_at, "backend_started_at": mark_boot(), "runner": runner},
    }))
}

/// Whether the recorded history is complete. "Complete" only when nothing was
/// dropped, truncated by a budget or removed by retention.
pub fn coverage(c: &Connection, run: &str, step: Option<&str>, attempt: Option<u32>) -> Result<Value> {
    let (logs, control): (i64, i64) = c.query_row(
        &format!("SELECT coalesce(sum(kind IN {LOG_SQL}),0),coalesce(sum(kind NOT IN {LOG_SQL}),0) FROM factory_workflow_events WHERE run_id=?1"),
        [run],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut s = c.prepare("SELECT json_extract(payload_json,'$.class') FROM factory_workflow_events WHERE run_id=?1 AND kind='capture.truncated'")?;
    let truncated = s
        .query_map([run], |r| r.get::<_, Option<String>>(0))?
        .filter_map(|r| r.ok().flatten())
        .collect::<Vec<_>>();
    let pruned: i64 = c.query_row(
        "SELECT coalesce(sum(json_extract(payload_json,'$.deleted')),0) FROM factory_workflow_events WHERE run_id=?1 AND kind='capture.pruned'",
        [run],
        |r| r.get(0),
    )?;
    let (dropped, gaps): (i64, i64) = c.query_row(
        "SELECT coalesce(sum(json_extract(payload_json,'$.data.dropped_events')),0),coalesce(sum(json_extract(payload_json,'$.data.gap_bytes')),0)
           FROM factory_workflow_events WHERE run_id=?1 AND kind IN ('command.finished','runner.connectivity')
            AND (?2 IS NULL OR step_id=?2) AND (?3 IS NULL OR json_extract(payload_json,'$.attempt')=?3)",
        params![run, step, attempt],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(json!({
        "log_events": logs, "log_limit": LOG_LIMIT,
        "control_events": control, "control_limit": CONTROL_LIMIT,
        "truncated": truncated, "pruned_events": pruned,
        "dropped_events": dropped, "gap_bytes": gaps,
        "complete": truncated.is_empty() && pruned == 0 && dropped == 0 && gaps == 0,
    }))
}

/// Correlates a backend rejection of a runner call with its step, so the panel
/// can show "the backend refused X with code Y" next to the request id the runner
/// reported. Only the run's own (non-revoked) runner can attribute events to it.
#[allow(clippy::too_many_arguments)]
pub fn record_rejection(
    c: &Connection,
    token_hash: &str,
    run: &str,
    step: &str,
    route: &str,
    status: u16,
    code: &str,
    request_id: Option<&str>,
) -> Result<bool> {
    let attempt: Option<u32> = c
        .query_row(
            "SELECT s.attempt FROM factory_workflow_runs r JOIN factory_runners a ON a.id=r.runner_id
               JOIN factory_workflow_steps s ON s.run_id=r.id AND s.step_id=?3
              WHERE r.id=?1 AND a.token_hash=?2 AND a.revoked=0",
            params![run, token_hash, step],
            |r| r.get(0),
        )
        .optional()?;
    let Some(attempt) = attempt else { return Ok(false) };
    let used: i64 = c.query_row(
        "SELECT count(*) FROM factory_workflow_events WHERE run_id=?1 AND kind='backend.rejected'",
        [run],
        |r| r.get(0),
    )?;
    if used >= REJECTION_LIMIT {
        return Ok(false);
    }
    super::workflows::event(
        c,
        run,
        Some(step),
        "backend.rejected",
        json!({"route": route, "status": status, "code": code, "request_id": request_id,
               "attempt": attempt, "source": "backend",
               "severity": if status >= 500 { "error" } else { "warning" }}),
    )?;
    Ok(true)
}

/// Removes bulky output of finished runs after the retention period. Results,
/// diagnostics, evidence and artifacts are kept; a marker records the removal so
/// the history is never presented as complete afterwards.
pub fn prune(c: &Connection, retention_days: i64) -> Result<i64> {
    let cutoff = format!("-{} days", retention_days.max(1));
    let t = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let runs = {
        let mut s = t.prepare(&format!(
            "SELECT e.run_id,count(*) FROM factory_workflow_events e JOIN factory_workflow_runs r ON r.id=e.run_id
              WHERE r.status IN ('succeeded','cancelled') AND e.kind IN {LOG_SQL} AND e.created_at<datetime('now',?1)
              GROUP BY e.run_id"
        ))?;
        let rows = s
            .query_map([&cutoff], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let mut total = 0;
    for (run, count) in runs {
        t.execute(
            &format!("DELETE FROM factory_workflow_events WHERE run_id=?1 AND kind IN {LOG_SQL} AND created_at<datetime('now',?2)"),
            params![run, cutoff],
        )?;
        super::workflows::event(
            &t,
            &run,
            None,
            "capture.pruned",
            json!({"deleted": count, "retention_days": retention_days}),
        )?;
        total += count;
    }
    t.commit()?;
    Ok(total)
}

pub fn retention_days() -> i64 {
    std::env::var("FACTORY_WORKFLOW_LOG_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|d: &i64| *d > 0)
        .unwrap_or(DEFAULT_RETENTION_DAYS)
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_credential_shapes_without_breaking_json() {
        let text = r#"{"h":"Authorization: Bearer abcdefghijklmnop","u":"https://x-access-token:ghs_1234567890abcdefghij@github.com/a/b","k":"sk-ant-api03-abcdefghijklmnopqrstu"}"#;
        let cleaned = redact(text);
        assert!(serde_json::from_str::<serde_json::Value>(&cleaned).is_ok(), "{cleaned}");
        for secret in ["abcdefghijklmnop", "ghs_1234567890", "sk-ant-api03"] {
            assert!(!cleaned.contains(secret), "{cleaned}");
        }
        assert_eq!(redact("plain build output: 3 passed"), "plain build output: 3 passed");
        assert_eq!(redact("nmr_0123456789abcdef0123"), "[REDACTED]");
    }
}
