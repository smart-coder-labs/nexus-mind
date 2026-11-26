//! Database operations for NexusMind desktop pets (mascots), conversations, and audit logs.

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MascotPet {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub user_name: Option<String>,
    pub device_id: String,
    pub pet_name: String,
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub arch: String,
    pub autonomy_level: String,
    pub decision_mode: String,
    pub visual_state: String,
    pub skills_count: i64,
    pub mcp_tools_count: i64,
    pub last_seen_at: String,
    pub created_at: String,
    pub conversations_count: i64,
    pub events_count: i64,
    pub recent_outcomes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HeartbeatWrite {
    pub device_id: String,
    pub pet_name: String,
    pub hostname: String,
    pub os_name: String,
    #[serde(default)]
    pub os_version: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default = "default_autonomy")]
    pub autonomy_level: String,
    #[serde(default = "default_decision")]
    pub decision_mode: String,
    #[serde(default = "default_visual")]
    pub visual_state: String,
    #[serde(default)]
    pub skills_count: i64,
    #[serde(default)]
    pub mcp_tools_count: i64,
}

fn default_autonomy() -> String {
    "supervised_autonomous".into()
}
fn default_decision() -> String {
    "hybrid".into()
}
fn default_visual() -> String {
    "idle".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PetConversation {
    pub id: String,
    pub org_id: String,
    pub pet_id: String,
    pub pet_name: String,
    pub hostname: String,
    pub os_name: String,
    pub user_id: String,
    pub run_id: String,
    pub objective: String,
    pub status: String,
    pub route: Option<String>,
    pub steps_executed: i64,
    pub tool_errors: i64,
    pub transcript_json: String,
    pub final_reply: Option<String>,
    pub started_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConversationWrite {
    pub device_id: String,
    pub run_id: String,
    pub objective: String,
    pub status: String,
    pub route: Option<String>,
    #[serde(default)]
    pub steps_executed: i64,
    #[serde(default)]
    pub tool_errors: i64,
    #[serde(default = "default_array_json")]
    pub transcript_json: String,
    pub final_reply: Option<String>,
}

fn default_array_json() -> String {
    "[]".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PetEvent {
    pub id: String,
    pub org_id: String,
    pub pet_id: String,
    pub pet_name: String,
    pub hostname: String,
    pub os_name: String,
    pub user_id: String,
    pub run_id: Option<String>,
    pub event_kind: String,
    pub action_kind: Option<String>,
    pub decision_source: Option<String>,
    pub risk: Option<f64>,
    pub outcome: Option<String>,
    pub summary: String,
    pub detail_json: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EventWrite {
    pub device_id: String,
    pub run_id: Option<String>,
    pub event_kind: String,
    pub action_kind: Option<String>,
    pub decision_source: Option<String>,
    pub risk: Option<f64>,
    pub outcome: Option<String>,
    pub summary: String,
    #[serde(default = "default_obj_json")]
    pub detail_json: String,
}

fn default_obj_json() -> String {
    "{}".into()
}

pub fn upsert_pet_heartbeat(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    input: &HeartbeatWrite,
) -> Result<MascotPet> {
    let now = Utc::now().to_rfc3339();
    let new_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO mascot_pets
         (id, org_id, user_id, device_id, pet_name, hostname, os_name, os_version, arch,
          autonomy_level, decision_mode, visual_state, skills_count, mcp_tools_count, last_seen_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
         ON CONFLICT(org_id, device_id) DO UPDATE SET
           user_id = excluded.user_id,
           pet_name = excluded.pet_name,
           hostname = excluded.hostname,
           os_name = excluded.os_name,
           os_version = excluded.os_version,
           arch = excluded.arch,
           autonomy_level = excluded.autonomy_level,
           decision_mode = excluded.decision_mode,
           visual_state = excluded.visual_state,
           skills_count = excluded.skills_count,
           mcp_tools_count = excluded.mcp_tools_count,
           last_seen_at = excluded.last_seen_at",
        params![
            new_id,
            org_id,
            user_id,
            input.device_id,
            input.pet_name,
            input.hostname,
            input.os_name,
            input.os_version,
            input.arch,
            input.autonomy_level,
            input.decision_mode,
            input.visual_state,
            input.skills_count,
            input.mcp_tools_count,
            now
        ],
    )?;

    get_pet_by_device(conn, org_id, &input.device_id)?
        .ok_or_else(|| anyhow::anyhow!("pet missing after upsert"))
}

pub fn get_pet_by_device(
    conn: &Connection,
    org_id: &str,
    device_id: &str,
) -> Result<Option<MascotPet>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.org_id, p.user_id, u.name, p.device_id, p.pet_name, p.hostname,
                p.os_name, p.os_version, p.arch, p.autonomy_level, p.decision_mode,
                p.visual_state, p.skills_count, p.mcp_tools_count, p.last_seen_at, p.created_at,
                (SELECT COUNT(*) FROM mascot_pet_conversations c WHERE c.pet_id = p.id),
                (SELECT COUNT(*) FROM mascot_pet_events e WHERE e.pet_id = p.id)
         FROM mascot_pets p
         LEFT JOIN users u ON u.id = p.user_id
         WHERE p.org_id = ?1 AND p.device_id = ?2",
    )?;
    let mut rows = stmt.query_map(params![org_id, device_id], map_pet_row)?;
    match rows.next() {
        Some(row) => {
            let mut pet = row?;
            pet.recent_outcomes = recent_outcomes_for_pet(conn, &pet.id, 14)?;
            Ok(Some(pet))
        }
        None => Ok(None),
    }
}

pub fn list_pets(conn: &Connection, org_id: &str) -> Result<Vec<MascotPet>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.org_id, p.user_id, u.name, p.device_id, p.pet_name, p.hostname,
                p.os_name, p.os_version, p.arch, p.autonomy_level, p.decision_mode,
                p.visual_state, p.skills_count, p.mcp_tools_count, p.last_seen_at, p.created_at,
                (SELECT COUNT(*) FROM mascot_pet_conversations c WHERE c.pet_id = p.id),
                (SELECT COUNT(*) FROM mascot_pet_events e WHERE e.pet_id = p.id)
         FROM mascot_pets p
         LEFT JOIN users u ON u.id = p.user_id
         WHERE p.org_id = ?1
         ORDER BY p.last_seen_at DESC",
    )?;
    let mut pets = stmt
        .query_map(params![org_id], map_pet_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for pet in &mut pets {
        pet.recent_outcomes = recent_outcomes_for_pet(conn, &pet.id, 14)?;
    }
    Ok(pets)
}

fn recent_outcomes_for_pet(conn: &Connection, pet_id: &str, limit: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT outcome FROM mascot_pet_events
         WHERE pet_id = ?1 AND outcome IS NOT NULL AND outcome != ''
         ORDER BY created_at DESC LIMIT ?2",
    )?;
    let mut list = stmt
        .query_map(params![pet_id, limit], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    list.reverse();
    Ok(list)
}

fn ensure_pet_id(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    device_id: &str,
) -> Result<String> {
    if let Some(pet) = get_pet_by_device(conn, org_id, device_id)? {
        return Ok(pet.id);
    }
    let created = upsert_pet_heartbeat(
        conn,
        org_id,
        user_id,
        &HeartbeatWrite {
            device_id: device_id.to_string(),
            pet_name: "Nexi".into(),
            hostname: device_id.to_string(),
            os_name: "unknown".into(),
            os_version: String::new(),
            arch: String::new(),
            autonomy_level: default_autonomy(),
            decision_mode: default_decision(),
            visual_state: default_visual(),
            skills_count: 0,
            mcp_tools_count: 0,
        },
    )?;
    Ok(created.id)
}

pub fn upsert_conversation(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    input: &ConversationWrite,
) -> Result<String> {
    let pet_id = ensure_pet_id(conn, org_id, user_id, &input.device_id)?;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO mascot_pet_conversations
         (id, org_id, pet_id, user_id, run_id, objective, status, route, steps_executed,
          tool_errors, transcript_json, final_reply, started_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
         ON CONFLICT(org_id, pet_id, run_id) DO UPDATE SET
           status = excluded.status,
           route = COALESCE(excluded.route, mascot_pet_conversations.route),
           steps_executed = excluded.steps_executed,
           tool_errors = excluded.tool_errors,
           transcript_json = excluded.transcript_json,
           final_reply = excluded.final_reply,
           updated_at = excluded.updated_at",
        params![
            id,
            org_id,
            pet_id,
            user_id,
            input.run_id,
            input.objective,
            input.status,
            input.route,
            input.steps_executed,
            input.tool_errors,
            input.transcript_json,
            input.final_reply,
            now
        ],
    )?;
    Ok(pet_id)
}

pub fn insert_event(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    input: &EventWrite,
) -> Result<(String, String)> {
    let pet_id = ensure_pet_id(conn, org_id, user_id, &input.device_id)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO mascot_pet_events
         (id, org_id, pet_id, user_id, run_id, event_kind, action_kind, decision_source,
          risk, outcome, summary, detail_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            org_id,
            pet_id,
            user_id,
            input.run_id,
            input.event_kind,
            input.action_kind,
            input.decision_source,
            input.risk,
            input.outcome,
            input.summary,
            input.detail_json,
            now
        ],
    )?;
    Ok((id, pet_id))
}

pub fn list_conversations(
    conn: &Connection,
    org_id: &str,
    pet_id: Option<&str>,
    limit: i64,
) -> Result<Vec<PetConversation>> {
    let capped = limit.clamp(1, 200);
    let mut stmt = conn.prepare(
        "SELECT c.id, c.org_id, c.pet_id, p.pet_name, p.hostname, p.os_name,
                c.user_id, c.run_id, c.objective, c.status, c.route, c.steps_executed,
                c.tool_errors, c.transcript_json, c.final_reply, c.started_at, c.updated_at
         FROM mascot_pet_conversations c
         JOIN mascot_pets p ON p.id = c.pet_id
         WHERE c.org_id = ?1 AND (?2 IS NULL OR c.pet_id = ?2)
         ORDER BY c.updated_at DESC
         LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![org_id, pet_id, capped], |r| {
            Ok(PetConversation {
                id: r.get(0)?,
                org_id: r.get(1)?,
                pet_id: r.get(2)?,
                pet_name: r.get(3)?,
                hostname: r.get(4)?,
                os_name: r.get(5)?,
                user_id: r.get(6)?,
                run_id: r.get(7)?,
                objective: r.get(8)?,
                status: r.get(9)?,
                route: r.get(10)?,
                steps_executed: r.get(11)?,
                tool_errors: r.get(12)?,
                transcript_json: r.get(13)?,
                final_reply: r.get(14)?,
                started_at: r.get(15)?,
                updated_at: r.get(16)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_events(
    conn: &Connection,
    org_id: &str,
    pet_id: Option<&str>,
    event_kind: Option<&str>,
    limit: i64,
) -> Result<Vec<PetEvent>> {
    let capped = limit.clamp(1, 500);
    let mut stmt = conn.prepare(
        "SELECT e.id, e.org_id, e.pet_id, p.pet_name, p.hostname, p.os_name,
                e.user_id, e.run_id, e.event_kind, e.action_kind, e.decision_source,
                e.risk, e.outcome, e.summary, e.detail_json, e.created_at
         FROM mascot_pet_events e
         JOIN mascot_pets p ON p.id = e.pet_id
         WHERE e.org_id = ?1
           AND (?2 IS NULL OR e.pet_id = ?2)
           AND (?3 IS NULL OR e.event_kind = ?3)
         ORDER BY e.created_at DESC
         LIMIT ?4",
    )?;
    let rows = stmt
        .query_map(params![org_id, pet_id, event_kind, capped], |r| {
            Ok(PetEvent {
                id: r.get(0)?,
                org_id: r.get(1)?,
                pet_id: r.get(2)?,
                pet_name: r.get(3)?,
                hostname: r.get(4)?,
                os_name: r.get(5)?,
                user_id: r.get(6)?,
                run_id: r.get(7)?,
                event_kind: r.get(8)?,
                action_kind: r.get(9)?,
                decision_source: r.get(10)?,
                risk: r.get(11)?,
                outcome: r.get(12)?,
                summary: r.get(13)?,
                detail_json: r.get(14)?,
                created_at: r.get(15)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn map_pet_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MascotPet> {
    Ok(MascotPet {
        id: r.get(0)?,
        org_id: r.get(1)?,
        user_id: r.get(2)?,
        user_name: r.get(3)?,
        device_id: r.get(4)?,
        pet_name: r.get(5)?,
        hostname: r.get(6)?,
        os_name: r.get(7)?,
        os_version: r.get(8)?,
        arch: r.get(9)?,
        autonomy_level: r.get(10)?,
        decision_mode: r.get(11)?,
        visual_state: r.get(12)?,
        skills_count: r.get(13)?,
        mcp_tools_count: r.get(14)?,
        last_seen_at: r.get(15)?,
        created_at: r.get(16)?,
        conversations_count: r.get(17)?,
        events_count: r.get(18)?,
        recent_outcomes: Vec::new(),
    })
}
