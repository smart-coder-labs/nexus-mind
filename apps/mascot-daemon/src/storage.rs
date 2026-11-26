//! Local SQLite persistence for episodic events, audit log (Outcome Tape),
//! pending human approvals, resumable agent runs, proactive suggestions,
//! macOS OS-bridge jobs, and learned procedural skills.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutcomeEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub action_kind: String,
    pub decision_source: String,
    pub risk: f64,
    pub outcome: String, // "success" | "warning" | "error" | "held" | "cancelled"
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub run_id: Option<String>,
    pub summary: String,
    pub action_kind: String,
    pub target: Option<String>,
    pub command_preview: Option<String>,
    pub risk: f64,
    pub decision_source: String,
    pub reason: String,
    pub status: String, // "pending" | "approved" | "denied"
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpisodicEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub app_name: Option<String>,
    pub redacted_summary: String,
    pub category: String,
    pub importance: f64,
    pub consolidated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LearnedSkill {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub trigger_hint: String,
    pub steps_yaml: String,
    pub scope: String, // "global_user" | "device:<id>"
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRunRecord {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub objective: String,
    pub status: String, // "running" | "waiting_approval" | "completed" | "failed" | "cancelled"
    pub route: Option<String>,
    pub steps_executed: u32,
    pub tool_errors: u32,
    pub state_json: String,
    pub final_reply: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProactiveSuggestion {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub title: String,
    pub summary: String,
    pub proposed_skill_slug: Option<String>,
    pub delivery_mode: String, // "notify_now" | "queue_for_idle" | "silent_log"
    pub status: String,        // "queued" | "accepted" | "dismissed"
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OsBridgeJob {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub kind: String, // "ax_snapshot" | "ax_click" | "screenshot" | "key_combo" | "mouse_click"
    pub payload_json: String,
    pub status: String, // "pending" | "done" | "error"
    pub result_json: Option<String>,
}

pub struct LocalStore {
    conn: Mutex<Connection>,
}

pub struct NewApproval<'a> {
    pub run_id: Option<&'a str>,
    pub summary: &'a str,
    pub action_kind: &'a str,
    pub target: Option<&'a str>,
    pub command_preview: Option<&'a str>,
    pub risk: f64,
    pub decision_source: &'a str,
    pub reason: &'a str,
}

impl LocalStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS episodic_events (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                source TEXT NOT NULL,
                app_name TEXT,
                redacted_summary TEXT NOT NULL,
                category TEXT NOT NULL,
                importance REAL NOT NULL,
                consolidated INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS action_audit (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                summary TEXT NOT NULL,
                action_kind TEXT NOT NULL,
                decision_source TEXT NOT NULL,
                risk REAL NOT NULL,
                outcome TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pending_approvals (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                run_id TEXT,
                summary TEXT NOT NULL,
                action_kind TEXT NOT NULL,
                target TEXT,
                command_preview TEXT,
                risk REAL NOT NULL,
                decision_source TEXT NOT NULL,
                reason TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending'
            );

            CREATE TABLE IF NOT EXISTS learned_skills (
                id TEXT PRIMARY KEY,
                slug TEXT UNIQUE NOT NULL,
                title TEXT NOT NULL,
                trigger_hint TEXT NOT NULL,
                steps_yaml TEXT NOT NULL,
                scope TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_runs (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                objective TEXT NOT NULL,
                status TEXT NOT NULL,
                route TEXT,
                steps_executed INTEGER NOT NULL DEFAULT 0,
                tool_errors INTEGER NOT NULL DEFAULT 0,
                state_json TEXT NOT NULL,
                final_reply TEXT
            );

            CREATE TABLE IF NOT EXISTS proactive_suggestions (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                proposed_skill_slug TEXT,
                delivery_mode TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued'
            );

            CREATE TABLE IF NOT EXISTS os_bridge_jobs (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                result_json TEXT
            );
            ",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn record_outcome(
        &self,
        summary: &str,
        action_kind: &str,
        decision_source: &str,
        risk: f64,
        outcome: &str,
    ) -> Result<OutcomeEntry> {
        let entry = OutcomeEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            summary: summary.into(),
            action_kind: action_kind.into(),
            decision_source: decision_source.into(),
            risk,
            outcome: outcome.into(),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO action_audit (id, timestamp, summary, action_kind, decision_source, risk, outcome)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.id,
                entry.timestamp.to_rfc3339(),
                entry.summary,
                entry.action_kind,
                entry.decision_source,
                entry.risk,
                entry.outcome
            ],
        )?;
        Ok(entry)
    }

    pub fn outcome_tape(&self, limit: usize) -> Result<Vec<OutcomeEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, summary, action_kind, decision_source, risk, outcome
             FROM action_audit ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                let ts = parse_ts(r.get::<_, String>(1)?);
                Ok(OutcomeEntry {
                    id: r.get(0)?,
                    timestamp: ts,
                    summary: r.get(2)?,
                    action_kind: r.get(3)?,
                    decision_source: r.get(4)?,
                    risk: r.get(5)?,
                    outcome: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn enqueue_approval(&self, req: NewApproval<'_>) -> Result<PendingApproval> {
        let item = PendingApproval {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            run_id: req.run_id.map(str::to_string),
            summary: req.summary.into(),
            action_kind: req.action_kind.into(),
            target: req.target.map(str::to_string),
            command_preview: req.command_preview.map(str::to_string),
            risk: req.risk,
            decision_source: req.decision_source.into(),
            reason: req.reason.into(),
            status: "pending".into(),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO pending_approvals
             (id, created_at, run_id, summary, action_kind, target, command_preview, risk, decision_source, reason, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
            params![
                item.id,
                item.created_at.to_rfc3339(),
                item.run_id,
                item.summary,
                item.action_kind,
                item.target,
                item.command_preview,
                item.risk,
                item.decision_source,
                item.reason
            ],
        )?;
        Ok(item)
    }

    pub fn list_pending_approvals(&self) -> Result<Vec<PendingApproval>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, run_id, summary, action_kind, target, command_preview, risk, decision_source, reason, status
             FROM pending_approvals WHERE status = 'pending' ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], map_approval_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn resolve_approval(&self, id: &str, approved: bool) -> Result<Option<PendingApproval>> {
        let new_status = if approved { "approved" } else { "denied" };
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE pending_approvals SET status = ?1 WHERE id = ?2 AND status = 'pending'",
            params![new_status, id],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let mut stmt = conn.prepare(
            "SELECT id, created_at, run_id, summary, action_kind, target, command_preview, risk, decision_source, reason, status
             FROM pending_approvals WHERE id = ?1",
        )?;
        let item = stmt.query_row(params![id], map_approval_row)?;
        Ok(Some(item))
    }

    pub fn save_agent_run(&self, run: &AgentRunRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO agent_runs
             (id, created_at, updated_at, objective, status, route, steps_executed, tool_errors, state_json, final_reply)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               updated_at = excluded.updated_at,
               status = excluded.status,
               route = excluded.route,
               steps_executed = excluded.steps_executed,
               tool_errors = excluded.tool_errors,
               state_json = excluded.state_json,
               final_reply = excluded.final_reply",
            params![
                run.id,
                run.created_at.to_rfc3339(),
                run.updated_at.to_rfc3339(),
                run.objective,
                run.status,
                run.route,
                run.steps_executed as i64,
                run.tool_errors as i64,
                run.state_json,
                run.final_reply,
            ],
        )?;
        Ok(())
    }

    pub fn get_agent_run(&self, id: &str) -> Result<Option<AgentRunRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, updated_at, objective, status, route, steps_executed, tool_errors, state_json, final_reply
             FROM agent_runs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(AgentRunRecord {
                id: r.get(0)?,
                created_at: parse_ts(r.get::<_, String>(1)?),
                updated_at: parse_ts(r.get::<_, String>(2)?),
                objective: r.get(3)?,
                status: r.get(4)?,
                route: r.get(5)?,
                steps_executed: r.get::<_, i64>(6)? as u32,
                tool_errors: r.get::<_, i64>(7)? as u32,
                state_json: r.get(8)?,
                final_reply: r.get(9)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn record_episodic_event(
        &self,
        source: &str,
        app_name: Option<&str>,
        redacted_summary: &str,
        category: &str,
        importance: f64,
    ) -> Result<EpisodicEvent> {
        let ev = EpisodicEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            source: source.into(),
            app_name: app_name.map(str::to_string),
            redacted_summary: redacted_summary.into(),
            category: category.into(),
            importance,
            consolidated: false,
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO episodic_events
             (id, timestamp, source, app_name, redacted_summary, category, importance, consolidated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                ev.id,
                ev.timestamp.to_rfc3339(),
                ev.source,
                ev.app_name,
                ev.redacted_summary,
                ev.category,
                ev.importance
            ],
        )?;
        Ok(ev)
    }

    pub fn unconsolidated_events(&self, limit: usize) -> Result<Vec<EpisodicEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, source, app_name, redacted_summary, category, importance, consolidated
             FROM episodic_events
             WHERE consolidated = 0 AND category != 'ignore'
             ORDER BY timestamp ASC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(EpisodicEvent {
                    id: r.get(0)?,
                    timestamp: parse_ts(r.get::<_, String>(1)?),
                    source: r.get(2)?,
                    app_name: r.get(3)?,
                    redacted_summary: r.get(4)?,
                    category: r.get(5)?,
                    importance: r.get(6)?,
                    consolidated: r.get::<_, i64>(7)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn recent_episodic_events(&self, limit: usize) -> Result<Vec<EpisodicEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, source, app_name, redacted_summary, category, importance, consolidated
             FROM episodic_events
             WHERE category != 'ignore'
             ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(EpisodicEvent {
                    id: r.get(0)?,
                    timestamp: parse_ts(r.get::<_, String>(1)?),
                    source: r.get(2)?,
                    app_name: r.get(3)?,
                    redacted_summary: r.get(4)?,
                    category: r.get(5)?,
                    importance: r.get(6)?,
                    consolidated: r.get::<_, i64>(7)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn mark_events_consolidated(&self, ids: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        for id in ids {
            conn.execute(
                "UPDATE episodic_events SET consolidated = 1 WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }

    pub fn upsert_skill(
        &self,
        slug: &str,
        title: &str,
        trigger_hint: &str,
        steps_yaml: &str,
        scope: &str,
    ) -> Result<LearnedSkill> {
        let skill = LearnedSkill {
            id: Uuid::new_v4().to_string(),
            slug: slug.into(),
            title: title.into(),
            trigger_hint: trigger_hint.into(),
            steps_yaml: steps_yaml.into(),
            scope: scope.into(),
            updated_at: Utc::now(),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO learned_skills (id, slug, title, trigger_hint, steps_yaml, scope, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(slug) DO UPDATE SET
               title = excluded.title,
               trigger_hint = excluded.trigger_hint,
               steps_yaml = excluded.steps_yaml,
               scope = excluded.scope,
               updated_at = excluded.updated_at",
            params![
                skill.id,
                skill.slug,
                skill.title,
                skill.trigger_hint,
                skill.steps_yaml,
                skill.scope,
                skill.updated_at.to_rfc3339()
            ],
        )?;
        Ok(skill)
    }

    pub fn list_skills(&self) -> Result<Vec<LearnedSkill>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, slug, title, trigger_hint, steps_yaml, scope, updated_at
             FROM learned_skills ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(LearnedSkill {
                    id: r.get(0)?,
                    slug: r.get(1)?,
                    title: r.get(2)?,
                    trigger_hint: r.get(3)?,
                    steps_yaml: r.get(4)?,
                    scope: r.get(5)?,
                    updated_at: parse_ts(r.get::<_, String>(6)?),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn enqueue_suggestion(
        &self,
        title: &str,
        summary: &str,
        proposed_skill_slug: Option<&str>,
        delivery_mode: &str,
    ) -> Result<ProactiveSuggestion> {
        let item = ProactiveSuggestion {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            title: title.into(),
            summary: summary.into(),
            proposed_skill_slug: proposed_skill_slug.map(str::to_string),
            delivery_mode: delivery_mode.into(),
            status: "queued".into(),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO proactive_suggestions
             (id, created_at, title, summary, proposed_skill_slug, delivery_mode, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued')",
            params![
                item.id,
                item.created_at.to_rfc3339(),
                item.title,
                item.summary,
                item.proposed_skill_slug,
                item.delivery_mode
            ],
        )?;
        Ok(item)
    }

    pub fn list_suggestions(&self) -> Result<Vec<ProactiveSuggestion>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, title, summary, proposed_skill_slug, delivery_mode, status
             FROM proactive_suggestions WHERE status = 'queued' ORDER BY created_at DESC LIMIT 20",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ProactiveSuggestion {
                    id: r.get(0)?,
                    created_at: parse_ts(r.get::<_, String>(1)?),
                    title: r.get(2)?,
                    summary: r.get(3)?,
                    proposed_skill_slug: r.get(4)?,
                    delivery_mode: r.get(5)?,
                    status: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn enqueue_os_job(&self, kind: &str, payload_json: &str) -> Result<OsBridgeJob> {
        let job = OsBridgeJob {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            kind: kind.into(),
            payload_json: payload_json.into(),
            status: "pending".into(),
            result_json: None,
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO os_bridge_jobs (id, created_at, kind, payload_json, status)
             VALUES (?1, ?2, ?3, ?4, 'pending')",
            params![job.id, job.created_at.to_rfc3339(), job.kind, job.payload_json],
        )?;
        Ok(job)
    }

    pub fn list_pending_os_jobs(&self) -> Result<Vec<OsBridgeJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, kind, payload_json, status, result_json
             FROM os_bridge_jobs WHERE status = 'pending' ORDER BY created_at ASC LIMIT 10",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(OsBridgeJob {
                    id: r.get(0)?,
                    created_at: parse_ts(r.get::<_, String>(1)?),
                    kind: r.get(2)?,
                    payload_json: r.get(3)?,
                    status: r.get(4)?,
                    result_json: r.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn complete_os_job(&self, id: &str, ok: bool, result_json: &str) -> Result<bool> {
        let status = if ok { "done" } else { "error" };
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE os_bridge_jobs SET status = ?1, result_json = ?2 WHERE id = ?3",
            params![status, result_json, id],
        )?;
        Ok(changed > 0)
    }

    pub fn get_os_job(&self, id: &str) -> Result<Option<OsBridgeJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, kind, payload_json, status, result_json
             FROM os_bridge_jobs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(OsBridgeJob {
                id: r.get(0)?,
                created_at: parse_ts(r.get::<_, String>(1)?),
                kind: r.get(2)?,
                payload_json: r.get(3)?,
                status: r.get(4)?,
                result_json: r.get(5)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }
}

fn parse_ts(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn map_approval_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PendingApproval> {
    Ok(PendingApproval {
        id: r.get(0)?,
        created_at: parse_ts(r.get::<_, String>(1)?),
        run_id: r.get(2)?,
        summary: r.get(3)?,
        action_kind: r.get(4)?,
        target: r.get(5)?,
        command_preview: r.get(6)?,
        risk: r.get(7)?,
        decision_source: r.get(8)?,
        reason: r.get(9)?,
        status: r.get(10)?,
    })
}
