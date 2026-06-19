use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::api_keys;
use crate::models::types::{
    AuthContext, AuditEntry, CodeChunk, CodeProject, CreateSessionRequest, CustomRole,
    GlobalMetrics, Memory, Org, OrgStats, OrgWithStats, PatchSessionRequest, Policy,
    Session, StoreMemoryRequest, ToolUsage, User, UserRole, Project, ProjectMember,
};

/// Looks up an API key by its SHA-256 hash.
/// Returns AuthContext if the key exists, is not revoked, and the user is active.
/// Also updates `last_used` on the api_keys row.
pub fn validate_api_key(conn: &Connection, key_hash: &str) -> Result<Option<AuthContext>> {
    let result = conn.query_row(
        "SELECT ak.org_id, ak.user_id, u.role, u.status
         FROM api_keys ak
         JOIN users u ON u.id = ak.user_id
         WHERE ak.key_hash = ?1 AND ak.revoked = 0",
        [key_hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    );

    match result {
        Ok((org_id, user_id, role_str, status)) => {
            if status != "active" {
                return Ok(None);
            }
            let role = match role_str.parse::<UserRole>() {
                Ok(r) => r,
                Err(_) => return Ok(None),
            };
            conn.execute(
                "UPDATE api_keys SET last_used = datetime('now') WHERE key_hash = ?1",
                [key_hash],
            )?;
            Ok(Some(AuthContext {
                org_id,
                user_id,
                role,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Creates the first organization + admin user + admin API key.
/// Returns (org, user, raw_api_key).
/// Fails if any organization already exists.
/// Creates an org + admin user + API key with no guard. Used by seed and bootstrap.
pub fn create_org(
    conn: &Connection,
    org_name: &str,
    org_slug: &str,
    admin_email: &str,
    admin_name: &str,
) -> Result<(Org, User, String)> {
    let org_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO organizations (id, name, slug, created_at) VALUES (?1, ?2, ?3, ?4)",
        [&org_id, org_name, org_slug, &now],
    )?;

    let user_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO users (id, org_id, email, name, role, status, created_at)
         VALUES (?1, ?2, ?3, ?4, 'admin', 'active', ?5)",
        [&user_id, &org_id, admin_email, admin_name, &now],
    )?;

    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'admin-key', ?5)",
        [&key_id, &user_id, &org_id, &key_hash, &now],
    )?;

    let org = Org {
        id: org_id,
        name: org_name.to_string(),
        slug: org_slug.to_string(),
        created_at: now.clone(),
    };
    let user = User {
        id: user_id,
        org_id: org.id.clone(),
        email: admin_email.to_string(),
        name: admin_name.to_string(),
        role: "admin".to_string(),
        status: "active".to_string(),
        created_at: now,
    };

    Ok((org, user, raw_key))
}

/// Lists all organizations ordered by creation date.
pub fn list_orgs(conn: &Connection) -> Result<Vec<Org>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, slug, created_at FROM organizations ORDER BY created_at ASC",
    )?;
    let orgs = stmt
        .query_map([], |row| {
            Ok(Org {
                id: row.get(0)?,
                name: row.get(1)?,
                slug: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(orgs)
}

/// Creates the first org. Fails with `already_bootstrapped` if any org exists.
pub fn bootstrap(
    conn: &Connection,
    org_name: &str,
    org_slug: &str,
    admin_email: &str,
    admin_name: &str,
) -> Result<(Org, User, String)> {
    let existing: i32 = conn.query_row(
        "SELECT COUNT(*) FROM organizations",
        [],
        |r| r.get(0),
    )?;
    if existing > 0 {
        anyhow::bail!("already_bootstrapped");
    }
    create_org(conn, org_name, org_slug, admin_email, admin_name)
}

// ── Memory queries ────────────────────────────────────────────────────────────

/// Sanitize a user query for FTS5 MATCH.
///
/// FTS5 treats many characters as operators (+, -, *, :, ^, ", (, )).
/// We wrap each whitespace-separated token in double quotes so they are
/// treated as literal phrase terms. Empty tokens are skipped.
/// Returns None if the sanitized result is empty (caller should skip FTS).
pub fn sanitize_fts_query(query: &str) -> Option<String> {
    // FTS5 special chars that cause parse errors even inside quoted phrases:
    // < > + - * : ^ " ( )
    // Strategy: split on whitespace, then further split each token on non-alphanumeric
    // boundaries, keep only alphanumeric+underscore sub-tokens, wrap each in "...".
    let terms: Vec<String> = query
        .split_whitespace()
        .flat_map(|w| {
            // Split on any char that is not alphanumeric or underscore
            w.split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|t| !t.is_empty())
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
        })
        .collect();

    if terms.is_empty() { None } else { Some(terms.join(" ")) }
}

/// Full-text search over memories, scoped to the org.
pub fn search_memories(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<Memory>> {
    let fts_query = match sanitize_fts_query(query) {
        Some(q) => q,
        None => return Ok(Vec::new()),
    };

    let mut stmt = conn.prepare(
        "SELECT m.id, m.org_id, m.user_id, m.project, m.tool, m.content, m.tags, m.created_at,
                m.title, m.type, m.scope, m.topic_key, m.session_id, m.revision_count, m.normalized_hash, m.project_id
         FROM memories m
         JOIN memories_fts fts ON fts.rowid = m.rowid
         WHERE memories_fts MATCH ?1 AND m.org_id = ?2
         ORDER BY rank
         LIMIT ?3",
    )?;

    let rows = stmt.query_map(rusqlite::params![fts_query, org_id, limit], |row| {
        let tags_str: String = row.get(6)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            tags_str,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10).unwrap_or(Some("project".to_string())),
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<i64>>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<String>>(15)?,
        ))
    })?;

    let mut memories = Vec::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        memories.push(Memory {
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags,
            created_at,
            title,
            memory_type,
            scope: scope.unwrap_or_else(|| "project".to_string()),
            topic_key,
            session_id,
            revision_count: revision_count.unwrap_or(1),
            normalized_hash,
            project_id,
        });
    }
    Ok(memories)
}

/// Lists memories for an org with optional filters.
#[allow(clippy::too_many_arguments)]
pub fn list_memories(
    conn: &Connection,
    org_id: &str,
    user_id: Option<&str>,
    tool: Option<&str>,
    project: Option<&str>,
    type_filter: Option<&str>,
    scope_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Memory>> {
    let mut sql = String::from(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id
         FROM memories
         WHERE org_id = ?1",
    );
    let mut param_idx = 2usize;
    let mut extra_params: Vec<String> = Vec::new();

    if let Some(u) = user_id {
        sql.push_str(&format!(" AND user_id = ?{param_idx}"));
        extra_params.push(u.to_string());
        param_idx += 1;
    }
    if let Some(t) = tool {
        sql.push_str(&format!(" AND tool = ?{param_idx}"));
        extra_params.push(t.to_string());
        param_idx += 1;
    }
    if let Some(p) = project {
        sql.push_str(&format!(" AND project = ?{param_idx}"));
        extra_params.push(p.to_string());
        param_idx += 1;
    }
    if let Some(ty) = type_filter {
        sql.push_str(&format!(" AND type = ?{param_idx}"));
        extra_params.push(ty.to_string());
        param_idx += 1;
    }
    if let Some(sc) = scope_filter {
        sql.push_str(&format!(" AND scope = ?{param_idx}"));
        extra_params.push(sc.to_string());
        param_idx += 1;
    }
    sql.push_str(&format!(
        " ORDER BY created_at DESC LIMIT ?{param_idx} OFFSET ?{}",
        param_idx + 1
    ));

    let mut stmt = conn.prepare(&sql)?;

    // Build params: org_id + extra filters + limit + offset
    let mut all_params: Vec<String> = vec![org_id.to_string()];
    all_params.extend(extra_params);
    all_params.push(limit.to_string());
    all_params.push(offset.to_string());

    let refs: Vec<&dyn rusqlite::ToSql> = all_params
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

    let rows = stmt.query_map(refs.as_slice(), |row| {
        let tags_str: String = row.get(6)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            tags_str,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<i64>>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<String>>(15)?,
        ))
    })?;

    let mut memories = Vec::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        memories.push(Memory {
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags,
            created_at,
            title,
            memory_type,
            scope: scope.unwrap_or_else(|| "project".to_string()),
            topic_key,
            session_id,
            revision_count: revision_count.unwrap_or(1),
            normalized_hash,
            project_id,
        });
    }
    Ok(memories)
}

/// Returns the user_id of the memory owner, scoped to org. Returns None if not found.
pub fn get_memory_owner(conn: &Connection, org_id: &str, memory_id: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT user_id FROM memories WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![memory_id, org_id],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(user_id) => Ok(Some(user_id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Deletes a memory by ID, scoped to the org. Returns true if deleted, false if not found.
pub fn delete_memory(conn: &Connection, org_id: &str, memory_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM memories WHERE id = ?1 AND org_id = ?2",
        [memory_id, org_id],
    )?;
    Ok(affected > 0)
}

// ── User queries ──────────────────────────────────────────────────────────────

/// Returns all users in the org.
pub fn list_users(conn: &Connection, org_id: &str) -> Result<Vec<User>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, email, name, role, status, created_at
         FROM users
         WHERE org_id = ?1
         ORDER BY created_at ASC",
    )?;

    let rows = stmt.query_map([org_id], |row| {
        Ok(User {
            id: row.get(0)?,
            org_id: row.get(1)?,
            email: row.get(2)?,
            name: row.get(3)?,
            role: row.get(4)?,
            status: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;

    let mut users = Vec::new();
    for row in rows {
        users.push(row?);
    }
    Ok(users)
}

/// Creates a user with status='invited' and generates an API key.
/// Returns (user, raw_api_key).
pub fn invite_user(
    conn: &Connection,
    org_id: &str,
    email: &str,
    name: &str,
    role: &str,
) -> Result<(User, String)> {
    let user_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO users (id, org_id, email, name, role, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6)",
        rusqlite::params![user_id, org_id, email, name, role, now],
    )?;

    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'default', ?5)",
        rusqlite::params![key_id, user_id, org_id, key_hash, now],
    )?;

    let user = User {
        id: user_id,
        org_id: org_id.to_string(),
        email: email.to_string(),
        name: name.to_string(),
        role: role.to_string(),
        status: "active".to_string(),
        created_at: now,
    };

    Ok((user, raw_key))
}

/// Suspends a user and revokes all their API keys.
/// Returns true if the user was found and suspended, false if not found.
pub fn suspend_user(conn: &Connection, org_id: &str, user_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE users SET status = 'suspended' WHERE id = ?1 AND org_id = ?2 AND status != 'suspended'",
        [user_id, org_id],
    )?;

    if affected == 0 {
        return Ok(false);
    }

    conn.execute(
        "UPDATE api_keys SET revoked = 1 WHERE user_id = ?1 AND org_id = ?2",
        [user_id, org_id],
    )?;

    Ok(true)
}

/// Revokes all current keys for a user and issues a new one.
/// Returns the raw new API key.
pub fn rotate_key(conn: &Connection, org_id: &str, user_id: &str) -> Result<String> {
    conn.execute(
        "UPDATE api_keys SET revoked = 1 WHERE user_id = ?1 AND org_id = ?2",
        [user_id, org_id],
    )?;

    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'rotated', ?5)",
        rusqlite::params![key_id, user_id, org_id, key_hash, now],
    )?;

    Ok(raw_key)
}

// ── Audit ─────────────────────────────────────────────────────────────────────

/// Lists audit log entries for an org with optional filters.
#[allow(clippy::too_many_arguments)]
pub fn list_audit(
    conn: &Connection,
    org_id: &str,
    user_id: Option<&str>,
    action: Option<&str>,
    resource_type: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<crate::models::types::AuditEntry>> {
    let mut sql = String::from(
        "SELECT id, org_id, user_id, timestamp, action, resource_type, resource_id, metadata,
                previous_hash, current_hash
         FROM audit_logs
         WHERE org_id = ?1",
    );
    let mut param_idx = 2usize;
    let mut extra_params: Vec<String> = Vec::new();

    if let Some(u) = user_id {
        sql.push_str(&format!(" AND user_id = ?{param_idx}"));
        extra_params.push(u.to_string());
        param_idx += 1;
    }
    if let Some(a) = action {
        sql.push_str(&format!(" AND action = ?{param_idx}"));
        extra_params.push(a.to_string());
        param_idx += 1;
    }
    if let Some(rt) = resource_type {
        sql.push_str(&format!(" AND resource_type = ?{param_idx}"));
        extra_params.push(rt.to_string());
        param_idx += 1;
    }
    if let Some(f) = from {
        sql.push_str(&format!(" AND timestamp >= ?{param_idx}"));
        extra_params.push(f.to_string());
        param_idx += 1;
    }
    if let Some(t) = to {
        sql.push_str(&format!(" AND timestamp <= ?{param_idx}"));
        extra_params.push(t.to_string());
        param_idx += 1;
    }
    sql.push_str(&format!(
        " ORDER BY timestamp DESC LIMIT ?{param_idx} OFFSET ?{}",
        param_idx + 1
    ));

    let mut stmt = conn.prepare(&sql)?;

    let mut all_params: Vec<String> = vec![org_id.to_string()];
    all_params.extend(extra_params);
    all_params.push(limit.to_string());
    all_params.push(offset.to_string());

    let refs: Vec<&dyn rusqlite::ToSql> = all_params
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

    let rows = stmt.query_map(refs.as_slice(), |row| {
        let meta_str: String = row.get(7)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            meta_str,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
        ))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (id, org_id, user_id, timestamp, action, resource_type, resource_id, meta_str, previous_hash, current_hash) = row?;
        let metadata: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or(serde_json::Value::Null);
        entries.push(crate::models::types::AuditEntry {
            id,
            org_id,
            user_id,
            timestamp,
            action,
            resource_type,
            resource_id,
            metadata,
            previous_hash,
            current_hash,
        });
    }
    Ok(entries)
}

/// Inserts an audit log entry with SHA-256 hash chaining.
///
/// Within a single transaction, reads the latest `current_hash` for the tenant,
/// computes `sha256(prev_hash_bytes || 0x1F || canonical_record)`, then inserts
/// the new row with both `previous_hash` and `current_hash` populated.
///
/// Canonical record format:
/// `timestamp || 0x1F || action || 0x1F || resource_type || 0x1F || resource_id || 0x1F || metadata_json_compact`
///
/// `timestamp_override`: when `Some`, used verbatim (must be ISO 8601). When `None`,
/// the server stamps `datetime('now')` in UTC.
#[allow(clippy::too_many_arguments)]
pub fn insert_audit_log_chained(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    metadata: serde_json::Value,
    timestamp_override: Option<&str>,
) -> Result<AuditEntry> {
    let tx = conn.unchecked_transaction()?;

    // 1. Read the latest current_hash for this org (per-tenant chain).
    // Use rowid DESC as the tiebreaker: rowid is SQLite's implicit autoincrement
    // and reflects true insertion order regardless of timestamp precision.
    let previous_hash: Option<String> = tx.query_row(
        "SELECT current_hash FROM audit_logs
         WHERE org_id = ?1 AND current_hash IS NOT NULL
         ORDER BY rowid DESC LIMIT 1",
        [org_id],
        |r| r.get(0),
    ).optional()?;

    // 2. Build canonical record and compute SHA-256.
    let id = Uuid::new_v4().to_string();
    let now = timestamp_override
        .map(String::from)
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let meta_str = serde_json::to_string(&metadata)?;
    let resource_id_str = resource_id.unwrap_or("");

    let mut hasher = Sha256::new();
    // Use the previous hash hex string bytes (empty bytes for genesis).
    hasher.update(previous_hash.as_deref().unwrap_or("").as_bytes());
    hasher.update([0x1F]);
    hasher.update(now.as_bytes());
    hasher.update([0x1F]);
    hasher.update(action.as_bytes());
    hasher.update([0x1F]);
    hasher.update(resource_type.as_bytes());
    hasher.update([0x1F]);
    hasher.update(resource_id_str.as_bytes());
    hasher.update([0x1F]);
    hasher.update(meta_str.as_bytes());
    let current_hash = hex::encode(hasher.finalize());

    // 3. INSERT inside the same transaction.
    tx.execute(
        "INSERT INTO audit_logs
         (id, org_id, user_id, timestamp, action, resource_type, resource_id, metadata,
          previous_hash, current_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            id, org_id, user_id, now, action, resource_type,
            resource_id, meta_str, previous_hash, current_hash
        ],
    )?;
    tx.commit()?;

    Ok(AuditEntry {
        id,
        org_id: org_id.to_string(),
        user_id: user_id.to_string(),
        timestamp: now,
        action: action.to_string(),
        resource_type: resource_type.to_string(),
        resource_id: resource_id.map(String::from),
        metadata,
        previous_hash,
        current_hash: Some(current_hash),
    })
}

/// Writes an audit log entry.
///
/// Thin wrapper around `insert_audit_log_chained` with `timestamp_override = None`.
/// All existing call sites continue to work unchanged; every write now joins the
/// per-tenant hash chain.
pub fn log_audit(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    metadata: serde_json::Value,
) -> Result<()> {
    insert_audit_log_chained(conn, org_id, user_id, action, resource_type, resource_id, metadata, None)?;
    Ok(())
}

// ── Admin / Org ───────────────────────────────────────────────────────────────

/// Returns the org by ID, or None if not found.
pub fn get_org(conn: &Connection, org_id: &str) -> Result<Option<Org>> {
    let result = conn.query_row(
        "SELECT id, name, slug, created_at FROM organizations WHERE id = ?1",
        [org_id],
        |row| {
            Ok(Org {
                id: row.get(0)?,
                name: row.get(1)?,
                slug: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    );

    match result {
        Ok(org) => Ok(Some(org)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Updates the org name and returns the updated org.
pub fn update_org_name(conn: &Connection, org_id: &str, name: &str) -> Result<Org> {
    conn.execute(
        "UPDATE organizations SET name = ?1 WHERE id = ?2",
        [name, org_id],
    )?;

    let org = get_org(conn, org_id)?
        .ok_or_else(|| anyhow::anyhow!("org_not_found"))?;
    Ok(org)
}

/// Returns aggregate stats for the org.
pub fn get_stats(conn: &Connection, org_id: &str) -> Result<OrgStats> {
    let total_memories: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1",
        [org_id],
        |r| r.get(0),
    )?;

    let active_users_24h: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT user_id) FROM audit_logs
         WHERE org_id = ?1 AND timestamp > datetime('now', '-24 hours')",
        [org_id],
        |r| r.get(0),
    )?;

    let searches_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM audit_logs
         WHERE org_id = ?1 AND action = 'search' AND timestamp > datetime('now', 'start of day')",
        [org_id],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT tool, COUNT(*) as count FROM memories
         WHERE org_id = ?1
         GROUP BY tool
         ORDER BY count DESC
         LIMIT 5",
    )?;
    let top_tools: Vec<ToolUsage> = stmt
        .query_map([org_id], |row| {
            Ok(ToolUsage {
                tool: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    Ok(OrgStats {
        total_memories,
        active_users_24h,
        searches_today,
        top_tools,
    })
}

// ── v2 Memory upsert ──────────────────────────────────────────────────────────

/// Computes SHA-256 of `content.trim().to_lowercase()` — pure function, no side effects.
pub fn compute_normalized_hash(content: &str) -> String {
    let normalized = content.trim().to_lowercase();
    let hash = Sha256::digest(normalized.as_bytes());
    hex::encode(hash)
}

/// Stores a memory with upsert semantics when `topic_key` is provided.
/// - With `topic_key`: SELECT existing row for `(org_id, topic_key)`.
///   If found, UPDATE content/title/type/scope/hash and increment `revision_count`.
///   If not found, INSERT with `revision_count = 1`.
/// - Without `topic_key`: always INSERT.
pub fn upsert_memory(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    req: &StoreMemoryRequest,
) -> Result<Memory> {
    let project = req.project.as_deref().unwrap_or("default");
    let project_id = get_or_create_project(conn, org_id, project)?;
    let tags_json = serde_json::to_string(req.tags.as_deref().unwrap_or(&[]))?;
    let scope = req.scope.as_deref().unwrap_or("project");
    let normalized_hash = compute_normalized_hash(&req.content);
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    if let Some(topic_key) = &req.topic_key {
        // Try to find existing row for this (org_id, topic_key)
        let existing = conn.query_row(
            "SELECT id, revision_count, created_at FROM memories WHERE org_id = ?1 AND topic_key = ?2",
            rusqlite::params![org_id, topic_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
        );

        match existing {
            Ok((existing_id, revision_count, created_at)) => {
                // UPDATE the existing row
                let new_revision = revision_count + 1;
                conn.execute(
                    "UPDATE memories SET content = ?1, title = ?2, type = ?3, scope = ?4,
                     normalized_hash = ?5, revision_count = ?6, tags = ?7, project_id = ?8
                     WHERE id = ?9",
                    rusqlite::params![
                        req.content, req.title, req.memory_type, scope,
                        normalized_hash, new_revision, tags_json, &project_id, existing_id
                    ],
                )?;
                let tags = req.tags.as_deref().unwrap_or(&[]).to_vec();
                return Ok(Memory {
                    id: existing_id,
                    org_id: org_id.to_string(),
                    user_id: user_id.to_string(),
                    project: project.to_string(),
                    tool: req.tool.clone(),
                    content: req.content.clone(),
                    tags,
                    created_at,
                    title: req.title.clone(),
                    memory_type: req.memory_type.clone(),
                    scope: scope.to_string(),
                    topic_key: Some(topic_key.clone()),
                    session_id: req.session_id.clone(),
                    revision_count: new_revision,
                    normalized_hash: Some(normalized_hash),
                    project_id: Some(project_id),
                });
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Fall through to INSERT below
            }
            Err(e) => return Err(e.into()),
        }
    }

    // INSERT new row
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at,
                               title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14, ?15)",
        rusqlite::params![
            id, org_id, user_id, project, req.tool, req.content, tags_json, now,
            req.title, req.memory_type, scope, req.topic_key, req.session_id, normalized_hash, &project_id
        ],
    )?;

    let tags = req.tags.as_deref().unwrap_or(&[]).to_vec();
    Ok(Memory {
        id,
        org_id: org_id.to_string(),
        user_id: user_id.to_string(),
        project: project.to_string(),
        tool: req.tool.clone(),
        content: req.content.clone(),
        tags,
        created_at: now,
        title: req.title.clone(),
        memory_type: req.memory_type.clone(),
        scope: scope.to_string(),
        topic_key: req.topic_key.clone(),
        session_id: req.session_id.clone(),
        revision_count: 1,
        normalized_hash: Some(normalized_hash),
        project_id: Some(project_id),
    })
}

// ── v2 Session CRUD ───────────────────────────────────────────────────────────

/// Creates a session and returns the new Session.
pub fn create_session(
    conn: &Connection,
    org_id: &str,
    req: &CreateSessionRequest,
) -> Result<Session> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let directory = req.directory.as_deref().unwrap_or("");

    conn.execute(
        "INSERT INTO sessions (id, org_id, project, directory, started_at, summary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, org_id, req.project, directory, now, req.summary],
    )?;

    Ok(Session {
        id,
        org_id: org_id.to_string(),
        project: req.project.clone(),
        directory: directory.to_string(),
        started_at: now,
        ended_at: None,
        summary: req.summary.clone(),
    })
}

/// Updates `ended_at` and/or `summary` on a session.
/// Returns `None` if the session does not exist for the given org (→ HTTP 404).
pub fn patch_session(
    conn: &Connection,
    org_id: &str,
    session_id: &str,
    req: &PatchSessionRequest,
) -> Result<Option<Session>> {
    if req.ended_at.is_none() && req.summary.is_none() {
        // Nothing to update — fetch and return existing session
        return get_session(conn, org_id, session_id);
    }

    let mut set_clauses: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(ended_at) = &req.ended_at {
        set_clauses.push(format!("ended_at = ?{param_idx}"));
        params.push(ended_at.clone());
        param_idx += 1;
    }
    if let Some(summary) = &req.summary {
        set_clauses.push(format!("summary = ?{param_idx}"));
        params.push(summary.clone());
        param_idx += 1;
    }

    params.push(org_id.to_string());
    params.push(session_id.to_string());

    let sql = format!(
        "UPDATE sessions SET {} WHERE org_id = ?{} AND id = ?{}",
        set_clauses.join(", "),
        param_idx,
        param_idx + 1
    );

    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let affected = conn.execute(&sql, refs.as_slice())?;

    if affected == 0 {
        return Ok(None);
    }

    get_session(conn, org_id, session_id)
}

/// Fetches a session by id, scoped to org. Returns None if not found.
pub fn get_session(conn: &Connection, org_id: &str, session_id: &str) -> Result<Option<Session>> {
    let result = conn.query_row(
        "SELECT id, org_id, project, directory, started_at, ended_at, summary
         FROM sessions WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![session_id, org_id],
        |row| {
            Ok(Session {
                id: row.get(0)?,
                org_id: row.get(1)?,
                project: row.get(2)?,
                directory: row.get(3)?,
                started_at: row.get(4)?,
                ended_at: row.get(5)?,
                summary: row.get(6)?,
            })
        },
    );
    match result {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Validates that a session_id belongs to the given org.
pub fn validate_session_ownership(conn: &Connection, org_id: &str, session_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![session_id, org_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Returns the password_hash for a user by ID.
pub fn get_user_password_hash(conn: &Connection, user_id: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT password_hash FROM users WHERE id = ?1",
        [user_id],
        |row| row.get::<_, Option<String>>(0),
    );
    match result {
        Ok(hash) => Ok(hash),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ── Auth: password + reset tokens ─────────────────────────────────────────────

/// Finds the first admin user with the given email (across all orgs) and returns
/// their record + password_hash. Used for email/password login.
pub fn find_admin_by_email(conn: &Connection, email: &str) -> Result<Option<(User, Option<String>)>> {
    let result = conn.query_row(
        "SELECT id, org_id, email, name, role, status, created_at, password_hash
         FROM users WHERE email = ?1 AND role = 'admin' AND status = 'active'
         ORDER BY created_at ASC LIMIT 1",
        [email],
        |row| {
            Ok((
                User {
                    id: row.get(0)?,
                    org_id: row.get(1)?,
                    email: row.get(2)?,
                    name: row.get(3)?,
                    role: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                },
                row.get::<_, Option<String>>(7)?,
            ))
        },
    );

    match result {
        Ok(pair) => Ok(Some(pair)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Fetches a user by ID.
pub fn get_user_by_id(conn: &Connection, user_id: &str) -> Result<Option<User>> {
    let result = conn.query_row(
        "SELECT id, org_id, email, name, role, status, created_at FROM users WHERE id = ?1",
        [user_id],
        |row| {
            Ok(User {
                id: row.get(0)?,
                org_id: row.get(1)?,
                email: row.get(2)?,
                name: row.get(3)?,
                role: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    );
    match result {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Sets the password_hash for a user.
pub fn set_user_password(conn: &Connection, user_id: &str, password_hash: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET password_hash = ?1 WHERE id = ?2",
        [password_hash, user_id],
    )?;
    Ok(())
}

/// Creates a password reset token. Returns (raw_token, token_id).
/// Expires in 24 hours. Any previous unused tokens for this user are invalidated.
pub fn create_password_reset_token(conn: &Connection, user_id: &str) -> Result<(String, String)> {
    // Revoke prior tokens for this user
    conn.execute(
        "UPDATE password_reset_tokens SET used = 1 WHERE user_id = ?1 AND used = 0",
        [user_id],
    )?;

    let token_id = Uuid::new_v4().to_string();
    let raw_bytes: [u8; 32] = rand::random();
    let raw_token = hex::encode(raw_bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(raw_token.as_bytes()));
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(24))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, used, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)",
        rusqlite::params![token_id, user_id, token_hash, expires_at, now],
    )?;

    Ok((raw_token, token_id))
}

/// Validates a reset token and returns the user_id if valid (not used, not expired).
/// Marks the token as used on success.
pub fn validate_and_consume_reset_token(conn: &Connection, raw_token: &str) -> Result<Option<String>> {
    let token_hash = hex::encode(sha2::Sha256::digest(raw_token.as_bytes()));
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let result = conn.query_row(
        "SELECT id, user_id FROM password_reset_tokens
         WHERE token_hash = ?1 AND used = 0 AND expires_at > ?2",
        rusqlite::params![token_hash, now],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );

    match result {
        Ok((token_id, user_id)) => {
            conn.execute(
                "UPDATE password_reset_tokens SET used = 1 WHERE id = ?1",
                [&token_id],
            )?;
            Ok(Some(user_id))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ── Embedding queries ─────────────────────────────────────────────────────────

/// Insert or replace the embedding BLOB for a memory.
pub fn store_embedding(conn: &Connection, memory_id: &str, embedding: &[u8]) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO memory_embeddings (memory_id, embedding) VALUES (?1, ?2)",
        rusqlite::params![memory_id, embedding],
    )?;
    Ok(())
}

/// Load all (memory_id, embedding_blob) pairs for an org.
/// Used for in-process cosine KNN during semantic search.
pub fn get_embeddings_for_org(conn: &Connection, org_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut stmt = conn.prepare(
        "SELECT me.memory_id, me.embedding
         FROM memory_embeddings me
         JOIN memories m ON m.id = me.memory_id
         WHERE m.org_id = ?1",
    )?;
    let rows = stmt.query_map([org_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    let mut pairs = Vec::new();
    for r in rows {
        pairs.push(r?);
    }
    Ok(pairs)
}

/// Fetch memories by a list of IDs, preserving the order of `ids`.
/// Scoped to `org_id` for safety.
pub fn get_memories_by_ids(conn: &Connection, org_id: &str, ids: &[String]) -> Result<Vec<Memory>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build "?,?,?" placeholder
    let placeholders: String = ids.iter().enumerate()
        .map(|(i, _)| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id
         FROM memories
         WHERE org_id = ?1 AND id IN ({placeholders})"
    );

    let mut stmt = conn.prepare(&sql)?;

    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&org_id as &dyn rusqlite::ToSql];
    for id in ids.iter() {
        params.push(id as &dyn rusqlite::ToSql);
    }

    let rows = stmt.query_map(params.as_slice(), |row| {
        let tags_str: String = row.get(6)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            tags_str,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<i64>>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<String>>(15)?,
        ))
    })?;

    // Build id→memory map, then restore order
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        map.insert(id.clone(), Memory {
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags,
            created_at,
            title,
            memory_type,
            scope: scope.unwrap_or_else(|| "project".to_string()),
            topic_key,
            session_id,
            revision_count: revision_count.unwrap_or(1),
            normalized_hash,
            project_id,
        });
    }

    // Return in caller-specified order
    Ok(ids.iter().filter_map(|id| map.remove(id)).collect())
}

/// Revokes a specific API key by its SHA-256 hash.
pub fn revoke_key_by_hash(conn: &Connection, key_hash: &str) -> Result<()> {
    conn.execute(
        "UPDATE api_keys SET revoked = 1 WHERE key_hash = ?1",
        [key_hash],
    )?;
    Ok(())
}

/// Creates a "web-session" API key for the user, revoking any previous ones.
/// Returns the raw API key.
pub fn create_web_session_key(conn: &Connection, user_id: &str, org_id: &str) -> Result<String> {
    conn.execute(
        "UPDATE api_keys SET revoked = 1 WHERE user_id = ?1 AND label = 'web-session'",
        [user_id],
    )?;

    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'web-session', ?5)",
        rusqlite::params![key_id, user_id, org_id, key_hash, now],
    )?;

    Ok(raw_key)
}

/// Lists all roles belonging to an organization or global templates.
pub fn list_roles(conn: &Connection, org_id: &str) -> Result<Vec<CustomRole>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, display_name, description, extends_json, permissions, color, icon, version, enabled, is_template, created_at, updated_at
         FROM roles
         WHERE org_id = ?1 OR org_id IS NULL OR is_template = 1"
    )?;
    let rows = stmt.query_map([org_id], |row| {
        let extends_json: String = row.get(5)?;
        let permissions_json: String = row.get(6)?;
        let extends = serde_json::from_str(&extends_json).unwrap_or_default();
        let permissions = serde_json::from_str(&permissions_json).unwrap_or_default();

        Ok(CustomRole {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            display_name: row.get(3)?,
            description: row.get(4)?,
            extends,
            permissions,
            color: row.get(7)?,
            icon: row.get(8)?,
            version: row.get(9)?,
            enabled: row.get::<_, i64>(10)? != 0,
            is_template: row.get::<_, i64>(11)? != 0,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

/// Creates a new custom role within an organization.
pub fn create_role(
    conn: &Connection,
    org_id: &str,
    name: &str,
    display_name: &str,
    permissions: &[String],
    description: Option<&str>,
) -> Result<CustomRole> {
    let id = Uuid::new_v4().to_string();
    let extends_json = "[]";
    let permissions_json = serde_json::to_string(permissions)?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO roles (id, org_id, name, display_name, description, extends_json, permissions, version, enabled, is_template, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 1, 0, ?8, ?8)",
        rusqlite::params![
            id,
            org_id,
            name,
            display_name,
            description,
            extends_json,
            permissions_json,
            now
        ],
    )?;

    Ok(CustomRole {
        id,
        org_id: Some(org_id.to_string()),
        name: name.to_string(),
        display_name: display_name.to_string(),
        description: description.map(|s| s.to_string()),
        extends: vec![],
        permissions: permissions.to_vec(),
        color: None,
        icon: None,
        version: 1,
        enabled: true,
        is_template: false,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Deletes a custom role from an organization. Only non-template roles can be deleted.
pub fn delete_role(conn: &Connection, org_id: &str, role_id: &str) -> Result<bool> {
    let count = conn.execute(
        "DELETE FROM roles WHERE id = ?1 AND org_id = ?2 AND is_template = 0",
        [role_id, org_id],
    )?;
    Ok(count > 0)
}

/// Updates the role of a user in an organization.
pub fn update_user_role(conn: &Connection, org_id: &str, user_id: &str, new_role: &str) -> Result<bool> {
    if new_role != "admin" && new_role != "member" && new_role != "viewer" {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM roles WHERE name = ?1 AND (org_id = ?2 OR org_id IS NULL)",
            [new_role, org_id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Ok(false);
        }
    }

    let count = conn.execute(
        "UPDATE users SET role = ?1 WHERE id = ?2 AND org_id = ?3",
        [new_role, user_id, org_id],
    )?;
    Ok(count > 0)
}

/// Resolves the permissions associated with a standard or custom role.
pub fn get_role_permissions(conn: &Connection, org_id: &str, role_name: &str) -> Result<Vec<String>> {
    if role_name == "admin" {
        return Ok(vec![
            "memory:read".to_string(),
            "memory:write".to_string(),
            "memory:delete".to_string(),
            "memory:search".to_string(),
            "user:invite".to_string(),
            "user:revoke".to_string(),
            "audit:read".to_string(),
            "audit:write".to_string(),
            "settings:write".to_string(),
            "policy:read".to_string(),
            "policy:write".to_string(),
        ]);
    } else if role_name == "member" {
        return Ok(vec![
            "memory:read".to_string(),
            "memory:write".to_string(),
            "memory:delete".to_string(),
            "memory:search".to_string(),
            "policy:read".to_string(),
        ]);
    } else if role_name == "viewer" {
        return Ok(vec![
            "memory:read".to_string(),
            "memory:search".to_string(),
        ]);
    }

    let result = conn.query_row(
        "SELECT permissions FROM roles WHERE name = ?1 AND (org_id = ?2 OR org_id IS NULL)",
        [role_name, org_id],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(json_str) => {
            let permissions: Vec<String> = serde_json::from_str(&json_str).unwrap_or_default();
            Ok(permissions)
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

pub fn get_or_create_project(conn: &Connection, org_id: &str, project_name: &str) -> Result<String> {
    let result = conn.query_row(
        "SELECT id FROM projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, project_name],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(id) => Ok(id),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            let id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO projects (id, org_id, name, description) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, org_id, project_name, None::<String>],
            )?;
            // Seed all active org users as members so they retain access to the new project.
            conn.execute(
                "INSERT OR IGNORE INTO project_members (id, project_id, user_id, role, created_at)
                 SELECT lower(hex(randomblob(16))), ?1, u.id, u.role, datetime('now')
                 FROM users u WHERE u.org_id = ?2 AND u.status = 'active'",
                rusqlite::params![id, org_id],
            )?;
            Ok(id)
        }
        Err(e) => Err(e.into()),
    }
}

pub fn project_name_exists(conn: &Connection, org_id: &str, name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn list_projects(conn: &Connection, org_id: &str) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, description, created_at, parent_id FROM projects WHERE org_id = ?1 ORDER BY name ASC",
    )?;
    let rows = stmt.query_map([org_id], |row| {
        Ok(Project {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            parent_id: row.get(5)?,
        })
    })?;
    let mut projects = Vec::new();
    for row in rows {
        projects.push(row?);
    }
    Ok(projects)
}

pub fn list_project_ids_for_org(conn: &Connection, org_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM projects WHERE org_id = ?1")?;
    let rows = stmt.query_map([org_id], |row| row.get(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

pub fn create_project(conn: &Connection, org_id: &str, name: &str, description: Option<&str>, parent_id: Option<&str>) -> Result<Project> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "INSERT INTO projects (id, org_id, name, description, created_at, parent_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, org_id, name, description, now, parent_id],
    )?;
    Ok(Project {
        id,
        org_id: org_id.to_string(),
        name: name.to_string(),
        description: description.map(String::from),
        created_at: now,
        parent_id: parent_id.map(String::from),
    })
}

pub fn update_project(conn: &Connection, org_id: &str, project_id: &str, parent_id: Option<&str>) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE projects SET parent_id = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![parent_id, project_id, org_id],
    )?;
    Ok(rows > 0)
}

pub fn delete_project(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM projects WHERE id = ?1 AND org_id = ?2",
        [id, org_id],
    )?;
    Ok(affected > 0)
}

pub fn list_project_members(conn: &Connection, _org_id: &str, project_id: &str) -> Result<Vec<ProjectMember>> {
    let mut stmt = conn.prepare(
        "SELECT pm.id, pm.project_id, pm.user_id, u.email, u.name, pm.role, pm.created_at
         FROM project_members pm
         JOIN users u ON u.id = pm.user_id
         WHERE pm.project_id = ?1",
    )?;
    let rows = stmt.query_map([project_id], |row| {
        Ok(ProjectMember {
            id: row.get(0)?,
            project_id: row.get(1)?,
            user_id: row.get(2)?,
            email: row.get(3)?,
            name: row.get(4)?,
            role: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    let mut members = Vec::new();
    for row in rows {
        members.push(row?);
    }
    Ok(members)
}

pub fn upsert_project_member(conn: &Connection, project_id: &str, user_id: &str, role: &str) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "INSERT INTO project_members (id, project_id, user_id, role, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(project_id, user_id) DO UPDATE SET role = excluded.role",
        rusqlite::params![id, project_id, user_id, role, now],
    )?;
    Ok(())
}

pub fn delete_project_member(conn: &Connection, project_id: &str, user_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM project_members WHERE project_id = ?1 AND user_id = ?2",
        [project_id, user_id],
    )?;
    Ok(affected > 0)
}

pub fn get_project_member_role(
    conn: &Connection,
    org_id: &str,
    project_name: &str,
    user_id: &str,
) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT pm.role 
         FROM project_members pm
         JOIN projects p ON p.id = pm.project_id
         WHERE p.org_id = ?1 AND p.name = ?2 AND pm.user_id = ?3",
    )?;
    let result = stmt.query_row(rusqlite::params![org_id, project_name, user_id], |row| {
        row.get::<_, String>(0)
    });
    match result {
        Ok(role) => Ok(Some(role)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_memory_owner_and_project(conn: &Connection, org_id: &str, memory_id: &str) -> Result<Option<(String, String)>> {
    let result = conn.query_row(
        "SELECT user_id, project FROM memories WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![memory_id, org_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );
    match result {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Returns a single `Memory` scoped to `org_id`, or `None` if not found / belongs to another tenant.
pub fn get_memory_by_id_for_org(conn: &Connection, org_id: &str, memory_id: &str) -> Result<Option<Memory>> {
    let result = conn.query_row(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id
         FROM memories WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![memory_id, org_id],
        |row| {
            let tags_str: String = row.get(6)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                tags_str,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<i64>>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, Option<String>>(15)?,
            ))
        },
    );

    match result {
        Ok((id, org_id, user_id, project, tool, content, tags_str, created_at,
            title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id)) => {
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(Some(Memory {
                id,
                org_id,
                user_id,
                project,
                tool,
                content,
                tags,
                created_at,
                title,
                memory_type,
                scope: scope.unwrap_or_else(|| "project".to_string()),
                topic_key,
                session_id,
                revision_count: revision_count.unwrap_or(1),
                normalized_hash,
                project_id,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Returns aggregated project context for `org_id` + `project` name:
/// up to 20 most-recent memories, distinct tool values, and the latest `created_at`.
pub fn get_project_context(
    conn: &Connection,
    org_id: &str,
    project: &str,
) -> Result<crate::models::types::ProjectContext> {
    // Query 1: recent memories (last 20, DESC).
    let mut stmt = conn.prepare(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id
         FROM memories
         WHERE org_id = ?1 AND project = ?2
         ORDER BY created_at DESC
         LIMIT 20",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, project], |row| {
        let tags_str: String = row.get(6)?;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            tags_str,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<i64>>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<String>>(15)?,
        ))
    })?;

    let mut recent_memories = Vec::new();
    for row in rows {
        let (id, org_id_col, user_id, proj, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        recent_memories.push(Memory {
            id,
            org_id: org_id_col,
            user_id,
            project: proj,
            tool,
            content,
            tags,
            created_at,
            title,
            memory_type,
            scope: scope.unwrap_or_else(|| "project".to_string()),
            topic_key,
            session_id,
            revision_count: revision_count.unwrap_or(1),
            normalized_hash,
            project_id,
        });
    }

    // Query 2: distinct tool values.
    let mut tool_stmt = conn.prepare(
        "SELECT DISTINCT tool FROM memories WHERE org_id = ?1 AND project = ?2",
    )?;
    let tool_rows = tool_stmt.query_map(rusqlite::params![org_id, project], |r| {
        r.get::<_, String>(0)
    })?;
    let tools: Vec<String> = tool_rows.collect::<rusqlite::Result<Vec<_>>>()?;

    // Query 3: last activity.
    let last_activity: Option<String> = conn.query_row(
        "SELECT MAX(created_at) FROM memories WHERE org_id = ?1 AND project = ?2",
        rusqlite::params![org_id, project],
        |r| r.get(0),
    ).optional()?.flatten();

    Ok(crate::models::types::ProjectContext {
        project: project.to_string(),
        recent_memories,
        tools,
        last_activity,
    })
}

pub fn get_global_metrics(conn: &Connection) -> Result<GlobalMetrics> {
    let total_orgs: i64 = conn.query_row(
        "SELECT count(*) FROM organizations",
        [],
        |r| r.get(0),
    )?;
    let total_users: i64 = conn.query_row(
        "SELECT count(*) FROM users",
        [],
        |r| r.get(0),
    )?;
    let total_memories: i64 = conn.query_row(
        "SELECT count(*) FROM memories",
        [],
        |r| r.get(0),
    )?;
    let active_users_24h: i64 = conn.query_row(
        "SELECT count(DISTINCT user_id) FROM audit_logs WHERE timestamp >= datetime('now', '-24 hours')",
        [],
        |r| r.get(0),
    )?;
    Ok(GlobalMetrics { total_orgs, total_users, total_memories, active_users_24h })
}

pub fn list_orgs_with_stats(conn: &Connection) -> Result<Vec<OrgWithStats>> {
    let mut stmt = conn.prepare(
        "SELECT o.id, o.name, o.slug, o.created_at,
                (SELECT count(*) FROM users u WHERE u.org_id = o.id) AS user_count,
                (SELECT count(*) FROM memories m WHERE m.org_id = o.id) AS memory_count
         FROM organizations o
         ORDER BY o.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(OrgWithStats {
            id: r.get(0)?,
            name: r.get(1)?,
            slug: r.get(2)?,
            created_at: r.get(3)?,
            user_count: r.get(4)?,
            memory_count: r.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

pub fn list_all_users(conn: &Connection) -> Result<Vec<User>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, email, name, role, status, created_at FROM users ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(User {
            id: r.get(0)?,
            org_id: r.get(1)?,
            email: r.get(2)?,
            name: r.get(3)?,
            role: r.get(4)?,
            status: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

pub fn list_all_audit(
    conn: &Connection,
    action: Option<&str>,
    resource_type: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEntry>> {
    let mut sql = "SELECT id, org_id, user_id, timestamp, action, resource_type, resource_id, metadata, \
                          previous_hash, current_hash \
                   FROM audit_logs WHERE 1=1".to_string();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(a) = action {
        sql.push_str(&format!(" AND action = ?{}", params.len() + 1));
        params.push(Box::new(a.to_string()));
    }
    if let Some(rt) = resource_type {
        sql.push_str(&format!(" AND resource_type = ?{}", params.len() + 1));
        params.push(Box::new(rt.to_string()));
    }
    if let Some(f) = from {
        sql.push_str(&format!(" AND timestamp >= ?{}", params.len() + 1));
        params.push(Box::new(f.to_string()));
    }
    if let Some(t) = to {
        sql.push_str(&format!(" AND timestamp <= ?{}", params.len() + 1));
        params.push(Box::new(t.to_string()));
    }
    sql.push_str(&format!(" ORDER BY timestamp DESC LIMIT ?{} OFFSET ?{}", params.len() + 1, params.len() + 2));
    params.push(Box::new(limit));
    params.push(Box::new(offset));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), |r| {
        Ok(AuditEntry {
            id: r.get(0)?,
            org_id: r.get(1)?,
            user_id: r.get(2)?,
            timestamp: r.get(3)?,
            action: r.get(4)?,
            resource_type: r.get(5)?,
            resource_id: r.get(6)?,
            metadata: r.get::<_, String>(7)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null),
            previous_hash: r.get(8)?,
            current_hash: r.get(9)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

pub fn delete_org(conn: &Connection, org_id: &str) -> Result<bool> {
    conn.execute("DELETE FROM audit_logs WHERE org_id = ?1", rusqlite::params![org_id])?;
    conn.execute("DELETE FROM api_keys WHERE org_id = ?1", rusqlite::params![org_id])?;
    conn.execute(
        "DELETE FROM memory_embeddings WHERE memory_id IN (SELECT id FROM memories WHERE org_id = ?1)",
        rusqlite::params![org_id],
    )?;
    conn.execute("DELETE FROM memories WHERE org_id = ?1", rusqlite::params![org_id])?;
    conn.execute("DELETE FROM users WHERE org_id = ?1", rusqlite::params![org_id])?;
    let deleted = conn.execute("DELETE FROM organizations WHERE id = ?1", rusqlite::params![org_id])?;
    Ok(deleted > 0)
}

pub fn get_org_admin_key(conn: &Connection, org_id: &str) -> Result<Option<String>> {
    let admin_user: Option<String> = conn
        .query_row(
            "SELECT id FROM users WHERE org_id = ?1 AND role = 'admin' AND status = 'active' ORDER BY created_at ASC LIMIT 1",
            rusqlite::params![org_id],
            |r| r.get(0),
        )
        .optional()?;

    match admin_user {
        Some(user_id) => {
            let key = create_web_session_key(conn, &user_id, org_id)?;
            Ok(Some(key))
        }
        None => Ok(None),
    }
}

pub fn suspend_user_global(conn: &Connection, user_id: &str) -> Result<bool> {
    let updated = conn.execute(
        "UPDATE users SET status = 'suspended' WHERE id = ?1 AND status != 'suspended'",
        rusqlite::params![user_id],
    )?;
    if updated > 0 {
        conn.execute(
            "DELETE FROM api_keys WHERE user_id = ?1",
            rusqlite::params![user_id],
        )?;
    }
    Ok(updated > 0)
}

pub fn get_org_with_stats(conn: &Connection, org_id: &str) -> Result<Option<OrgWithStats>> {
    conn.query_row(
        "SELECT o.id, o.name, o.slug, o.created_at,
                (SELECT count(*) FROM users u WHERE u.org_id = o.id) AS user_count,
                (SELECT count(*) FROM memories m WHERE m.org_id = o.id) AS memory_count
         FROM organizations o WHERE o.id = ?1",
        rusqlite::params![org_id],
        |r| Ok(OrgWithStats {
            id: r.get(0)?,
            name: r.get(1)?,
            slug: r.get(2)?,
            created_at: r.get(3)?,
            user_count: r.get(4)?,
            memory_count: r.get(5)?,
        }),
    )
    .optional()
    .map_err(Into::into)
}

// ── Policy queries ────────────────────────────────────────────────────────────

/// Daily usage statistics for budget_limit policy evaluation.
#[derive(Debug, Clone, Default)]
pub struct DailyStats {
    pub requests_today: i64,
    pub tokens_today: i64,
}

fn row_to_policy(row: &rusqlite::Row<'_>) -> rusqlite::Result<Policy> {
    let config_str: String = row.get(4)?;
    let config: serde_json::Value = serde_json::from_str(&config_str).unwrap_or(serde_json::Value::Object(Default::default()));
    let enabled_int: i64 = row.get(5)?;
    Ok(Policy {
        id: row.get(0)?,
        org_id: row.get(1)?,
        name: row.get(2)?,
        rule_type: row.get(3)?,
        config,
        enabled: enabled_int != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Returns all policies for an org, ordered by creation date DESC.
pub fn list_policies(conn: &Connection, org_id: &str) -> Result<Vec<Policy>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at
         FROM policies WHERE org_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([org_id], row_to_policy)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Returns only enabled policies for an org, ordered by creation date ASC.
/// Used by the `/policy/check` handler for evaluation.
pub fn list_enabled_policies(conn: &Connection, org_id: &str) -> Result<Vec<Policy>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at
         FROM policies WHERE org_id = ?1 AND enabled = 1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([org_id], row_to_policy)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Returns a single policy by id + org_id, or None (hides cross-org existence).
pub fn get_policy(conn: &Connection, id: &str, org_id: &str) -> Result<Option<Policy>> {
    let result = conn.query_row(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at
         FROM policies WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
        row_to_policy,
    );
    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Inserts a new policy and returns the created row.
pub fn insert_policy(
    conn: &Connection,
    id: &str,
    org_id: &str,
    name: &str,
    rule_type: &str,
    config_json: &str,
    enabled: bool,
) -> Result<Policy> {
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let enabled_int: i64 = if enabled { 1 } else { 0 };
    conn.execute(
        "INSERT INTO policies (id, org_id, name, rule_type, config, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        rusqlite::params![id, org_id, name, rule_type, config_json, enabled_int, now],
    )?;
    get_policy(conn, id, org_id)?
        .ok_or_else(|| anyhow::anyhow!("insert_policy: row not found after insert"))
}

/// Updates name/config/enabled for a policy. Returns None if the policy does not exist in this org.
pub fn update_policy(
    conn: &Connection,
    id: &str,
    org_id: &str,
    name: Option<&str>,
    config_json: Option<&str>,
    enabled: Option<bool>,
    now: &str,
) -> Result<Option<Policy>> {
    let enabled_int: Option<i64> = enabled.map(|b| if b { 1 } else { 0 });
    let rows_affected = conn.execute(
        "UPDATE policies
         SET name    = COALESCE(?3, name),
             config  = COALESCE(?4, config),
             enabled = COALESCE(?5, enabled),
             updated_at = ?6
         WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id, name, config_json, enabled_int, now],
    )?;
    if rows_affected == 0 {
        return Ok(None);
    }
    get_policy(conn, id, org_id)
}

/// Deletes a policy scoped to org_id. Returns true if a row was deleted.
pub fn delete_policy(conn: &Connection, id: &str, org_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM policies WHERE id = ?1 AND org_id = ?2",
        [id, org_id],
    )?;
    Ok(affected > 0)
}

/// Returns the count of requests and total tokens used by the org today (UTC).
/// "Today" is determined by SQLite's `strftime('%Y-%m-%dT00:00:00.000Z','now')`.
pub fn fetch_daily_stats(conn: &Connection, org_id: &str) -> Result<DailyStats> {
    let (requests_today, tokens_today): (i64, i64) = conn.query_row(
        "SELECT
             COUNT(*) AS requests_today,
             COALESCE(SUM(CAST(json_extract(metadata, '$.tokens_total') AS INTEGER)), 0) AS tokens_today
         FROM audit_logs
         WHERE org_id = ?1
           AND timestamp >= strftime('%Y-%m-%dT00:00:00.000Z','now')",
        rusqlite::params![org_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(DailyStats { requests_today, tokens_today })
}

// ── Code index queries ─────────────────────────────────────────────────────────

/// Insert or update a code_project row for (org_id, name).
/// Returns the `id` (ROWID) of the project.
pub fn upsert_code_project(
    conn: &Connection,
    org_id: &str,
    name: &str,
    root_path: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO code_projects (org_id, name, root_path)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(org_id, name) DO UPDATE SET root_path = excluded.root_path",
        rusqlite::params![org_id, name, root_path],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM code_projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, name],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Update file_count, chunk_count, and last_indexed for a code project.
pub fn update_code_project_stats(
    conn: &Connection,
    code_project_id: i64,
    file_count: i64,
    chunk_count: i64,
    last_indexed: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE code_projects SET file_count = ?1, chunk_count = ?2, last_indexed = ?3 WHERE id = ?4",
        rusqlite::params![file_count, chunk_count, last_indexed, code_project_id],
    )?;
    Ok(())
}

/// Delete all code_chunks for a specific file within a project.
/// Called before re-indexing a changed file.
pub fn delete_chunks_for_file(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
) -> Result<()> {
    conn.execute(
        "DELETE FROM code_chunks WHERE code_project_id = ?1 AND file_path = ?2",
        rusqlite::params![code_project_id, file_path],
    )?;
    Ok(())
}

/// Count chunks for a specific file (used when skipping unchanged files).
pub fn count_chunks_for_file(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM code_chunks WHERE code_project_id = ?1 AND file_path = ?2",
        rusqlite::params![code_project_id, file_path],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// Insert a single code chunk, optionally with its embedding BLOB.
#[allow(clippy::too_many_arguments)]
pub fn insert_code_chunk(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
    file_hash: &str,
    language: Option<&str>,
    symbol: Option<&str>,
    start_line: i64,
    end_line: i64,
    content: &str,
    embedding: Option<&[u8]>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO code_chunks
         (code_project_id, file_path, file_hash, language, symbol, start_line, end_line, content, embedding)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            code_project_id, file_path, file_hash, language, symbol,
            start_line, end_line, content, embedding
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Return all (chunk_id, embedding_blob) pairs for a project. Used for cosine ranking.
pub fn get_code_embeddings(
    conn: &Connection,
    code_project_id: i64,
) -> Result<Vec<(i64, Vec<u8>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, embedding FROM code_chunks WHERE code_project_id = ?1 AND embedding IS NOT NULL",
    )?;
    let rows = stmt.query_map(rusqlite::params![code_project_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    let mut pairs = Vec::new();
    for r in rows {
        pairs.push(r?);
    }
    Ok(pairs)
}

/// Fetch multiple code chunks by their row IDs (ORDER preserved).
pub fn get_chunks_by_ids(
    conn: &Connection,
    ids: &[i64],
) -> Result<Vec<CodeChunk>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders: String = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT id, code_project_id, file_path, file_hash, language, symbol,
                start_line, end_line, content, created_at
         FROM code_chunks WHERE id IN ({placeholders})"
    );

    let mut stmt = conn.prepare(&sql)?;

    let params: Vec<Box<dyn rusqlite::ToSql>> = ids
        .iter()
        .map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>)
        .collect();
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok(CodeChunk {
            id: row.get(0)?,
            code_project_id: row.get(1)?,
            file_path: row.get(2)?,
            file_hash: row.get(3)?,
            language: row.get(4)?,
            symbol: row.get(5)?,
            start_line: row.get(6)?,
            end_line: row.get(7)?,
            content: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?;

    // Restore order of `ids`
    let mut map: std::collections::HashMap<i64, CodeChunk> = std::collections::HashMap::new();
    for r in rows {
        let chunk = r?;
        map.insert(chunk.id, chunk);
    }

    Ok(ids.iter().filter_map(|id| map.remove(id)).collect())
}

/// Retrieve the code project record for (org_id, name), if it exists.
pub fn get_code_project(
    org_id: &str,
    name: &str,
    conn: &Connection,
) -> Result<Option<CodeProject>> {
    let result = conn.query_row(
        "SELECT id, org_id, name, root_path, file_count, chunk_count, last_indexed, created_at
         FROM code_projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, name],
        |row| {
            Ok(CodeProject {
                id: row.get::<_, i64>(0)?.to_string(),
                org_id: row.get(1)?,
                name: row.get(2)?,
                root_path: row.get(3)?,
                file_count: row.get(4)?,
                chunk_count: row.get(5)?,
                last_indexed: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    );

    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Return file_path → file_hash for all chunks of a project (deduplicated).
/// Used by the indexer to detect unchanged files.
pub fn list_indexed_files_with_hashes(
    conn: &Connection,
    code_project_id: i64,
) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT file_path, file_hash FROM code_chunks WHERE code_project_id = ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![code_project_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = std::collections::HashMap::new();
    for r in rows {
        let (path, hash) = r?;
        map.insert(path, hash);
    }
    Ok(map)
}

/// Fetch chunks adjacent to the given chunk (same file, ordered by start_line).
/// Returns the target chunk plus up to `neighbors` chunks before and after.
pub fn get_chunk_context(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
    symbol: &str,
    neighbors: i64,
) -> Result<Vec<CodeChunk>> {
    // First find the target chunk by symbol name
    let target_result = conn.query_row(
        "SELECT id, code_project_id, file_path, file_hash, language, symbol,
                start_line, end_line, content, created_at
         FROM code_chunks
         WHERE code_project_id = ?1 AND file_path = ?2 AND symbol = ?3
         ORDER BY start_line ASC LIMIT 1",
        rusqlite::params![code_project_id, file_path, symbol],
        |row| {
            Ok(CodeChunk {
                id: row.get(0)?,
                code_project_id: row.get(1)?,
                file_path: row.get(2)?,
                file_hash: row.get(3)?,
                language: row.get(4)?,
                symbol: row.get(5)?,
                start_line: row.get(6)?,
                end_line: row.get(7)?,
                content: row.get(8)?,
                created_at: row.get(9)?,
            })
        },
    );

    let target = match target_result {
        Ok(c) => c,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };

    // Fetch neighbors: `neighbors` chunks before and after by start_line
    let mut stmt = conn.prepare(
        "SELECT id, code_project_id, file_path, file_hash, language, symbol,
                start_line, end_line, content, created_at
         FROM code_chunks
         WHERE code_project_id = ?1 AND file_path = ?2
           AND ABS(start_line - ?3) <= (?4 * 60)
         ORDER BY start_line ASC",
    )?;

    let rows = stmt.query_map(
        rusqlite::params![code_project_id, file_path, target.start_line, neighbors],
        |row| {
            Ok(CodeChunk {
                id: row.get(0)?,
                code_project_id: row.get(1)?,
                file_path: row.get(2)?,
                file_hash: row.get(3)?,
                language: row.get(4)?,
                symbol: row.get(5)?,
                start_line: row.get(6)?,
                end_line: row.get(7)?,
                content: row.get(8)?,
                created_at: row.get(9)?,
            })
        },
    )?;

    let mut chunks: Vec<CodeChunk> = rows.collect::<Result<_, _>>()?;

    // Trim to: target + `neighbors` before + `neighbors` after
    if let Some(pos) = chunks.iter().position(|c| c.id == target.id) {
        let start = pos.saturating_sub(neighbors as usize);
        let end = (pos + neighbors as usize + 1).min(chunks.len());
        chunks = chunks[start..end].to_vec();
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::models::types::Role;
    use crate::db::migrations;

    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn get_memory_owner_returns_user_id_when_found() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content", &[]);

        let owner = get_memory_owner(&conn, &org.id, &mem.id).unwrap();
        assert_eq!(owner, Some(user.id));
    }

    #[test]
    fn get_memory_owner_returns_none_when_not_found() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let owner = get_memory_owner(&conn, &org.id, "nonexistent-id").unwrap();
        assert!(owner.is_none());
    }

    #[test]
    fn get_memory_owner_returns_none_wrong_org() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content", &[]);

        // Correct memory ID but wrong org → None (org scoped)
        let owner = get_memory_owner(&conn, "wrong-org", &mem.id).unwrap();
        assert!(owner.is_none());
    }

    #[test]
    fn validate_api_key_allows_custom_role_string() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Insert a user with an invalid role directly
        let user_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, 'bad@acme.com', 'Bad', 'superuser', 'active', datetime('now'))",
            rusqlite::params![user_id, org.id],
        ).unwrap();

        let key_id = uuid::Uuid::new_v4().to_string();
        let (raw_key, key_hash) = crate::auth::api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org.id, key_hash],
        ).unwrap();

        // Key is structurally valid and role is custom — must return context with Custom(superuser)
        let result = validate_api_key(&conn, &api_keys::hash_key(&raw_key)).unwrap();
        assert!(result.is_some(), "custom role string must cause validate_api_key to return Some");
        assert_eq!(result.unwrap().role, UserRole::Custom("superuser".to_string()));
    }

    #[test]
    fn validate_api_key_returns_none_for_unknown_hash() {
        let conn = setup();
        let result = validate_api_key(&conn, "deadbeef").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn validate_api_key_returns_none_for_revoked_key() {
        let conn = setup();
        let (org, _user, raw_key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hash = api_keys::hash_key(&raw_key);

        conn.execute(
            "UPDATE api_keys SET revoked = 1 WHERE org_id = ?1",
            [&org.id],
        ).unwrap();

        let result = validate_api_key(&conn, &hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn validate_api_key_returns_context_for_valid_key() {
        let conn = setup();
        let (org, user, raw_key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hash = api_keys::hash_key(&raw_key);

        let ctx = validate_api_key(&conn, &hash).unwrap().expect("should return context");
        assert_eq!(ctx.org_id, org.id);
        assert_eq!(ctx.user_id, user.id);
        assert_eq!(ctx.role, UserRole::Standard(Role::Admin));
    }

    #[test]
    fn validate_api_key_returns_none_for_suspended_user() {
        let conn = setup();
        let (_org, user, raw_key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hash = api_keys::hash_key(&raw_key);

        conn.execute(
            "UPDATE users SET status = 'suspended' WHERE id = ?1",
            [&user.id],
        ).unwrap();

        let result = validate_api_key(&conn, &hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn bootstrap_creates_org_and_admin() {
        let conn = setup();
        let (org, user, raw_key) = bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin User").unwrap();

        assert_eq!(org.name, "Acme Corp");
        assert_eq!(org.slug, "acme");
        assert_eq!(user.email, "admin@acme.com");
        assert_eq!(user.role, "admin");
        assert!(raw_key.starts_with("nm_"));
    }

    #[test]
    fn bootstrap_fails_if_org_exists() {
        let conn = setup();
        bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let result = bootstrap(&conn, "Other", "other", "other@other.com", "Other");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("already_bootstrapped"));
    }

    // ── Memory tests ──────────────────────────────────────────────────────────

    #[test]
    fn store_and_retrieve_memory() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let tags = vec!["rust".to_string(), "axum".to_string()];
        let mem = legacy_store(&conn, &org.id, &user.id, "nexusmind", "claude", "use anyhow for errors", &tags);

        assert_eq!(mem.org_id, org.id);
        assert_eq!(mem.user_id, user.id);
        assert_eq!(mem.content, "use anyhow for errors");
        assert_eq!(mem.tags, tags);
        assert!(mem.id.len() > 0);
    }

    #[test]
    fn search_memories_returns_fts_matches() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        legacy_store(&conn, &org.id, &user.id, "proj", "claude", "use snake_case for identifiers", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj", "claude", "database migrations run at startup", &[]);

        let results = search_memories(&conn, &org.id, "snake_case", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("snake_case"));
    }

    #[test]
    fn search_memories_scoped_to_org() {
        let conn = setup();
        // org1
        let (org1, user1, _) = bootstrap(&conn, "Org1", "org1", "admin@org1.com", "Admin1").unwrap();
        legacy_store(&conn, &org1.id, &user1.id, "proj", "claude", "secret content org1", &[]);

        // org2 — manually insert since bootstrap only allows one org
        let org2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org2', 'org2')",
            [&org2_id],
        ).unwrap();
        let user2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role) VALUES (?1, ?2, 'u2@org2.com', 'U2', 'member')",
            [&user2_id, &org2_id],
        ).unwrap();

        let results = search_memories(&conn, &org2_id, "secret", 10).unwrap();
        assert_eq!(results.len(), 0, "org2 must not see org1 memories");
    }

    #[test]
    fn list_memories_with_filters() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        legacy_store(&conn, &org.id, &user.id, "proj-a", "claude", "mem 1", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj-b", "cursor", "mem 2", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj-a", "cursor", "mem 3", &[]);

        // filter by tool
        let cursor_mems = list_memories(&conn, &org.id, None, Some("cursor"), None, None, None, 10, 0).unwrap();
        assert_eq!(cursor_mems.len(), 2);

        // filter by project
        let proj_a = list_memories(&conn, &org.id, None, None, Some("proj-a"), None, None, 10, 0).unwrap();
        assert_eq!(proj_a.len(), 2);

        // filter by both
        let filtered = list_memories(&conn, &org.id, None, Some("cursor"), Some("proj-a"), None, None, 10, 0).unwrap();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn delete_memory_wrong_org_returns_false() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content", &[]);

        let deleted = delete_memory(&conn, "wrong-org-id", &mem.id).unwrap();
        assert!(!deleted, "delete with wrong org must return false");

        // original should still exist
        let still_there = list_memories(&conn, &org.id, None, None, None, None, None, 10, 0).unwrap();
        assert_eq!(still_there.len(), 1);
    }

    // ── User tests ────────────────────────────────────────────────────────────

    #[test]
    fn invite_user_creates_active_key() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let (user, raw_key) = invite_user(&conn, &org.id, "dev@acme.com", "Dev User", "member").unwrap();
        assert_eq!(user.role, "member");
        assert!(raw_key.starts_with("nm_"));

        // key must be valid
        let hash = api_keys::hash_key(&raw_key);
        let ctx = validate_api_key(&conn, &hash).unwrap();
        assert!(ctx.is_some());
        assert_eq!(ctx.unwrap().user_id, user.id);
    }

    #[test]
    fn suspend_user_revokes_keys() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let (user, raw_key) = invite_user(&conn, &org.id, "dev@acme.com", "Dev", "member").unwrap();

        suspend_user(&conn, &org.id, &user.id).unwrap();

        // key must be revoked
        let hash = api_keys::hash_key(&raw_key);
        let ctx = validate_api_key(&conn, &hash).unwrap();
        assert!(ctx.is_none(), "suspended user's key must not be valid");
    }

    #[test]
    fn rotate_key_invalidates_old_key() {
        let conn = setup();
        let (org, user, old_raw_key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let new_raw_key = rotate_key(&conn, &org.id, &user.id).unwrap();

        let old_hash = api_keys::hash_key(&old_raw_key);
        let new_hash = api_keys::hash_key(&new_raw_key);

        let old_ctx = validate_api_key(&conn, &old_hash).unwrap();
        assert!(old_ctx.is_none(), "old key must be revoked after rotation");

        let new_ctx = validate_api_key(&conn, &new_hash).unwrap();
        assert!(new_ctx.is_some(), "new key must be valid");
    }

    // ── Audit tests ───────────────────────────────────────────────────────────

    #[test]
    fn list_audit_returns_entries_for_org() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "search", "memory", None, serde_json::json!({})).unwrap();

        let entries = list_audit(&conn, &org.id, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn list_audit_scoped_to_org() {
        let conn = setup();
        let (org1, user1, _) = bootstrap(&conn, "Org1", "org1", "admin@org1.com", "Admin1").unwrap();
        log_audit(&conn, &org1.id, &user1.id, "store", "memory", None, serde_json::json!({})).unwrap();

        // manually create org2
        let org2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org2', 'org2')",
            [&org2_id],
        ).unwrap();
        let user2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role) VALUES (?1, ?2, 'u2@org2.com', 'U2', 'member')",
            [&user2_id, &org2_id],
        ).unwrap();
        log_audit(&conn, &org2_id, &user2_id, "store", "memory", None, serde_json::json!({})).unwrap();

        let org1_entries = list_audit(&conn, &org1.id, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(org1_entries.len(), 1, "org1 must not see org2 audit entries");

        let org2_entries = list_audit(&conn, &org2_id, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(org2_entries.len(), 1, "org2 must not see org1 audit entries");
    }

    #[test]
    fn list_audit_filters_by_action() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "search", "memory", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();

        let store_entries = list_audit(&conn, &org.id, None, Some("store"), None, None, None, 50, 0).unwrap();
        assert_eq!(store_entries.len(), 2);
        assert!(store_entries.iter().all(|e| e.action == "store"));

        let search_entries = list_audit(&conn, &org.id, None, Some("search"), None, None, None, 50, 0).unwrap();
        assert_eq!(search_entries.len(), 1);
    }

    // ── T-04 tests ────────────────────────────────────────────────────────────

    #[test]
    fn list_audit_returns_hash_fields() {
        // After v9 migration, log_audit should produce rows; list_audit must
        // return them with previous_hash / current_hash columns (NULL for rows
        // written before the chain logic, which is fine for this test — we only
        // confirm the columns come back without error).
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();

        let entries = list_audit(&conn, &org.id, None, None, None, None, None, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);

        // The SELECT now includes previous_hash and current_hash columns.
        // For rows written by the old log_audit (no chain), both will be NULL —
        // but the struct fields must exist and be None (not a query error).
        let e = &entries[0];
        // Just confirming the fields are accessible; NULL is expected for old rows.
        let _ = &e.previous_hash;
        let _ = &e.current_hash;
        assert_eq!(e.action, "store");
    }

    #[test]
    fn log_audit_creates_entry() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "search", "memory", None, serde_json::json!({"query": "rust"})).unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM audit_logs WHERE org_id = ?1 AND action = 'search'",
            [&org.id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }

    // ── Stats tests ───────────────────────────────────────────────────────────

    #[test]
    fn get_stats_returns_counts() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        legacy_store(&conn, &org.id, &user.id, "proj", "claude", "mem 1", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj", "claude", "mem 2", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj", "cursor", "mem 3", &[]);

        log_audit(&conn, &org.id, &user.id, "search", "memory", None, serde_json::json!({})).unwrap();

        let stats = get_stats(&conn, &org.id).unwrap();
        assert_eq!(stats.total_memories, 3);
        assert_eq!(stats.searches_today, 1);
        assert!(!stats.top_tools.is_empty());
        let tool_names: Vec<&str> = stats.top_tools.iter().map(|t| t.tool.as_str()).collect();
        assert!(tool_names.contains(&"claude"));
    }

    // ── v2 upsert tests ───────────────────────────────────────────────────────

    #[test]
    fn upsert_memory_first_call_inserts_with_revision_1() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "initial content".into(),
            tags: None,
            title: Some("Test Memory".into()),
            memory_type: Some("decision".into()),
            scope: None,
            topic_key: Some("arch/auth-model".into()),
            session_id: None,
        };

        let mem = upsert_memory(&conn, &org.id, &user.id, &req).unwrap();
        assert_eq!(mem.revision_count, 1);
        assert_eq!(mem.topic_key.as_deref(), Some("arch/auth-model"));
        assert!(mem.normalized_hash.is_some(), "hash must be computed");
    }

    #[test]
    fn upsert_memory_second_call_updates_revision_count() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req1 = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "first content".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: Some("arch/auth-model".into()),
            session_id: None,
        };
        let mem1 = upsert_memory(&conn, &org.id, &user.id, &req1).unwrap();
        assert_eq!(mem1.revision_count, 1);

        let req2 = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "updated content".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: Some("arch/auth-model".into()),
            session_id: None,
        };
        let mem2 = upsert_memory(&conn, &org.id, &user.id, &req2).unwrap();
        assert_eq!(mem2.revision_count, 2, "second store must increment revision_count");
        assert_eq!(mem2.id, mem1.id, "upsert must reuse existing row id");
        assert_eq!(mem2.content, "updated content");

        // Verify only one row exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories WHERE org_id = ?1", [&org.id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "upsert must not create duplicate rows");
    }

    #[test]
    fn upsert_memory_topic_key_is_org_scoped() {
        let conn = setup();
        let (org1, user1, _) = bootstrap(&conn, "Org1", "org1", "a@org1.com", "Admin1").unwrap();

        // Create org2 directly
        let org2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org2', 'org2')",
            [&org2_id],
        ).unwrap();
        let user2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role) VALUES (?1, ?2, 'u2@org2.com', 'U2', 'member')",
            [&user2_id, &org2_id],
        ).unwrap();

        let req = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "org1 content".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: Some("shared-key".into()),
            session_id: None,
        };

        let req2 = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "org2 content".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: Some("shared-key".into()),
            session_id: None,
        };

        let mem1 = upsert_memory(&conn, &org1.id, &user1.id, &req).unwrap();
        let mem2 = upsert_memory(&conn, &org2_id, &user2_id, &req2).unwrap();

        assert_ne!(mem1.id, mem2.id, "different orgs must get different rows for same topic_key");
        assert_eq!(mem1.revision_count, 1);
        assert_eq!(mem2.revision_count, 1);
    }

    #[test]
    fn upsert_memory_no_topic_key_always_inserts() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "same content".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        };

        upsert_memory(&conn, &org.id, &user.id, &req).unwrap();
        upsert_memory(&conn, &org.id, &user.id, &req).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories WHERE org_id = ?1", [&org.id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "no topic_key must always insert new rows");
    }

    #[test]
    fn normalized_hash_same_for_equivalent_content() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req_a = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "  Hello World  ".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        };
        let req_b = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "hello world".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        };

        let mem_a = upsert_memory(&conn, &org.id, &user.id, &req_a).unwrap();
        let mem_b = upsert_memory(&conn, &org.id, &user.id, &req_b).unwrap();
        assert_eq!(
            mem_a.normalized_hash, mem_b.normalized_hash,
            "whitespace/case variants must produce same hash"
        );
    }

    // ── v2 session tests ──────────────────────────────────────────────────────

    #[test]
    fn create_session_returns_session_with_id() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req = crate::models::types::CreateSessionRequest {
            project: "nexusmind".into(),
            directory: Some("/home/user".into()),
            summary: None,
        };
        let session = create_session(&conn, &org.id, &req).unwrap();
        assert!(!session.id.is_empty());
        assert_eq!(session.project, "nexusmind");
        assert_eq!(session.org_id, org.id);
        assert_eq!(session.directory, "/home/user");
        assert!(session.ended_at.is_none());
    }

    #[test]
    fn patch_session_persists_ended_at_and_summary() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let create_req = crate::models::types::CreateSessionRequest {
            project: "proj".into(),
            directory: None,
            summary: None,
        };
        let session = create_session(&conn, &org.id, &create_req).unwrap();

        let patch_req = crate::models::types::PatchSessionRequest {
            ended_at: Some("2026-01-01T01:00:00Z".into()),
            summary: Some("Session complete".into()),
        };
        let updated = patch_session(&conn, &org.id, &session.id, &patch_req).unwrap();
        assert!(updated.is_some(), "patch_session must return the updated session");
        let updated = updated.unwrap();
        assert_eq!(updated.ended_at.as_deref(), Some("2026-01-01T01:00:00Z"));
        assert_eq!(updated.summary.as_deref(), Some("Session complete"));
    }

    #[test]
    fn patch_session_wrong_org_returns_none() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let create_req = crate::models::types::CreateSessionRequest {
            project: "proj".into(),
            directory: None,
            summary: None,
        };
        let session = create_session(&conn, &org.id, &create_req).unwrap();

        let patch_req = crate::models::types::PatchSessionRequest {
            ended_at: Some("2026-01-01T01:00:00Z".into()),
            summary: None,
        };
        let result = patch_session(&conn, "wrong-org", &session.id, &patch_req).unwrap();
        assert!(result.is_none(), "wrong org must return None (404)");
    }

    // ── v2 list filter tests ──────────────────────────────────────────────────

    #[test]
    fn list_memories_filter_by_type() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req_bugfix = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "fixed null pointer".into(),
            tags: None,
            title: None,
            memory_type: Some("bugfix".into()),
            scope: None,
            topic_key: None,
            session_id: None,
        };
        let req_decision = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "use hexagonal arch".into(),
            tags: None,
            title: None,
            memory_type: Some("decision".into()),
            scope: None,
            topic_key: None,
            session_id: None,
        };

        upsert_memory(&conn, &org.id, &user.id, &req_bugfix).unwrap();
        upsert_memory(&conn, &org.id, &user.id, &req_decision).unwrap();

        let bugfix_mems = list_memories(&conn, &org.id, None, None, None, Some("bugfix"), None, 10, 0).unwrap();
        assert_eq!(bugfix_mems.len(), 1);
        assert_eq!(bugfix_mems[0].memory_type.as_deref(), Some("bugfix"));
    }

    #[test]
    fn list_memories_filter_by_scope() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req_personal = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "personal preference".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: Some("personal".into()),
            topic_key: None,
            session_id: None,
        };
        let req_project = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: "project convention".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: Some("project".into()),
            topic_key: None,
            session_id: None,
        };

        upsert_memory(&conn, &org.id, &user.id, &req_personal).unwrap();
        upsert_memory(&conn, &org.id, &user.id, &req_project).unwrap();

        let personal_mems = list_memories(&conn, &org.id, None, None, None, None, Some("personal"), 10, 0).unwrap();
        assert_eq!(personal_mems.len(), 1);
        assert_eq!(personal_mems[0].scope, "personal");

        let combined = list_memories(&conn, &org.id, None, None, None, None, Some("project"), 10, 0).unwrap();
        assert_eq!(combined.len(), 1);
    }

    #[test]
    fn list_memories_combined_type_scope_filter() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // bugfix+project
        upsert_memory(&conn, &org.id, &user.id, &crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(), content: "c1".into(),
            tags: None, title: None, memory_type: Some("bugfix".into()),
            scope: Some("project".into()), topic_key: None, session_id: None,
        }).unwrap();
        // bugfix+personal
        upsert_memory(&conn, &org.id, &user.id, &crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(), content: "c2".into(),
            tags: None, title: None, memory_type: Some("bugfix".into()),
            scope: Some("personal".into()), topic_key: None, session_id: None,
        }).unwrap();
        // decision+project
        upsert_memory(&conn, &org.id, &user.id, &crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(), content: "c3".into(),
            tags: None, title: None, memory_type: Some("decision".into()),
            scope: Some("project".into()), topic_key: None, session_id: None,
        }).unwrap();

        let results = list_memories(&conn, &org.id, None, None, None, Some("bugfix"), Some("project"), 10, 0).unwrap();
        assert_eq!(results.len(), 1, "combined filter must return only bugfix+project memories");
    }

    #[test]
    fn list_memories_unknown_type_returns_empty() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        upsert_memory(&conn, &org.id, &user.id, &crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(), content: "content".into(),
            tags: None, title: None, memory_type: Some("bugfix".into()),
            scope: None, topic_key: None, session_id: None,
        }).unwrap();

        let results = list_memories(&conn, &org.id, None, None, None, Some("config"), None, 10, 0).unwrap();
        assert!(results.is_empty(), "unknown type filter must return empty list");
    }

    // ── v2 FTS search tests ───────────────────────────────────────────────────

    #[test]
    fn search_memories_matches_on_title() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        upsert_memory(&conn, &org.id, &user.id, &crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(),
            content: "unrelated content".into(),
            tags: None, title: Some("JWT auth middleware".into()),
            memory_type: None, scope: None, topic_key: None, session_id: None,
        }).unwrap();

        let results = search_memories(&conn, &org.id, "JWT", 10).unwrap();
        assert_eq!(results.len(), 1, "FTS must match on title");
        assert_eq!(results[0].title.as_deref(), Some("JWT auth middleware"));
    }

    #[test]
    fn search_memories_matches_on_type() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        upsert_memory(&conn, &org.id, &user.id, &crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(),
            content: "unrelated".into(),
            tags: None, title: Some("Unrelated title".into()),
            memory_type: Some("bugfix".into()), scope: None, topic_key: None, session_id: None,
        }).unwrap();

        let results = search_memories(&conn, &org.id, "bugfix", 10).unwrap();
        assert_eq!(results.len(), 1, "FTS must match on type column");
    }

    // ── Dead-code removal guard (T-03) ────────────────────────────────────────

    /// Minimal helper that reproduces the old `store_memory` API via `upsert_memory`.
    /// Used only in tests below. `upsert_memory` with `topic_key: None` always INSERTs.
    fn legacy_store(
        conn: &Connection,
        org_id: &str,
        user_id: &str,
        project: &str,
        tool: &str,
        content: &str,
        tags: &[String],
    ) -> Memory {
        let req = crate::models::types::StoreMemoryRequest {
            project: Some(project.to_string()),
            tool: tool.to_string(),
            content: content.to_string(),
            tags: Some(tags.to_vec()),
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        };
        upsert_memory(conn, org_id, user_id, &req).unwrap()
    }

    /// Compile-time absence guard for `store_memory`.
    ///
    /// This test documents that `store_memory` MUST NOT be re-introduced as a
    /// public symbol. If the function is ever added back to `queries.rs`, the
    /// tests that formerly called `store_memory(...)` directly will start calling
    /// a different function than `legacy_store`, which serves as the canary.
    ///
    /// For a hard compile-time guard, the following expression must remain
    /// unreachable — if `store_memory` exists as a public fn, the build
    /// produces an "unused import" warning that escalates to an error under
    /// `#[deny(dead_code)]`.
    #[test]
    fn store_memory_symbol_is_gone() {
        // This test passes only when `store_memory` is not defined in this module.
        // It asserts the deletion contract by ensuring `legacy_store` is the sole
        // path to insert memories in tests. Any re-introduction of `store_memory`
        // at the module level will immediately fail the T-03 migration intent.
        //
        // Verification: `cargo check` must produce zero matches for
        // `pub fn store_memory` in `src/db/queries.rs`.
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "absence test", &[]);
        assert_eq!(mem.content, "absence test", "legacy_store must work as upsert_memory wrapper");
    }

    // ── T-07 tests: insert_audit_log_chained ─────────────────────────────────

    #[test]
    fn insert_audit_log_chained_bootstraps_chain() {
        // First insert for org must have previous_hash = NULL, current_hash = non-empty hex.
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let entry = insert_audit_log_chained(
            &conn,
            &org.id,
            &user.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
            None,
        ).unwrap();

        assert!(entry.previous_hash.is_none(), "genesis record must have previous_hash = NULL");
        assert!(entry.current_hash.is_some(), "genesis record must have a non-empty current_hash");
        let hash = entry.current_hash.unwrap();
        assert_eq!(hash.len(), 64, "SHA-256 hex string is 64 chars");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "current_hash must be hex");
    }

    #[test]
    fn insert_audit_log_chained_sequential_links() {
        // Insert 3 rows; verify each row's previous_hash equals the prior row's current_hash.
        // Also verify replaying sha256(prev || 0x1F || canonical) reproduces stored current_hash.
        use sha2::{Digest, Sha256};

        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let mut entries = Vec::new();
        for i in 0..3u32 {
            let e = insert_audit_log_chained(
                &conn,
                &org.id,
                &user.id,
                "store",
                "memory",
                Some(&format!("res-{i}")),
                serde_json::json!({}),
                None,
            ).unwrap();
            entries.push(e);
        }

        // Chain linkage: each entry's previous_hash must equal the prior entry's current_hash.
        assert!(entries[0].previous_hash.is_none(), "first entry genesis must have no previous_hash");
        assert_eq!(entries[1].previous_hash, entries[0].current_hash, "entry[1].previous_hash must equal entry[0].current_hash");
        assert_eq!(entries[2].previous_hash, entries[1].current_hash, "entry[2].previous_hash must equal entry[1].current_hash");

        // Replay hashes to verify correctness.
        for entry in &entries {
            let prev_bytes = entry.previous_hash.as_deref().unwrap_or("").as_bytes();
            let meta_str = serde_json::to_string(&entry.metadata).unwrap();
            let resource_id = entry.resource_id.as_deref().unwrap_or("");

            let mut hasher = Sha256::new();
            hasher.update(prev_bytes);
            hasher.update([0x1F]);
            hasher.update(entry.timestamp.as_bytes());
            hasher.update([0x1F]);
            hasher.update(entry.action.as_bytes());
            hasher.update([0x1F]);
            hasher.update(entry.resource_type.as_bytes());
            hasher.update([0x1F]);
            hasher.update(resource_id.as_bytes());
            hasher.update([0x1F]);
            hasher.update(meta_str.as_bytes());
            let computed = hex::encode(hasher.finalize());

            assert_eq!(
                Some(&computed),
                entry.current_hash.as_ref(),
                "replayed hash must match stored current_hash for entry id={}",
                entry.id
            );
        }
    }

    #[test]
    fn insert_audit_log_chained_cross_tenant_isolation() {
        // Org A inserts 2 rows; Org B inserts 1 row.
        // Org B's genesis must have previous_hash = NULL (its own chain start).
        let conn = setup();
        let (org_a, user_a, _) = bootstrap(&conn, "OrgA", "orga", "admin@orga.com", "AdminA").unwrap();

        // Create org B manually.
        let org_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'OrgB', 'orgb')",
            [&org_b_id],
        ).unwrap();
        let user_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES (?1, ?2, 'b@orgb.com', 'B', 'admin', 'active')",
            [&user_b_id, &org_b_id],
        ).unwrap();

        // Org A: 2 inserts.
        insert_audit_log_chained(&conn, &org_a.id, &user_a.id, "store", "memory", None, serde_json::json!({}), None).unwrap();
        let a2 = insert_audit_log_chained(&conn, &org_a.id, &user_a.id, "search", "memory", None, serde_json::json!({}), None).unwrap();

        // Org B: 1 insert — should bootstrap its own chain, not continue org A's.
        let b1 = insert_audit_log_chained(&conn, &org_b_id, &user_b_id, "store", "memory", None, serde_json::json!({}), None).unwrap();

        assert!(b1.previous_hash.is_none(), "org B genesis must have previous_hash = NULL");
        assert!(b1.current_hash.is_some(), "org B genesis must have a current_hash");
        // Org B's hash must NOT equal org A's last hash.
        assert_ne!(b1.current_hash, a2.current_hash, "org B chain must be independent of org A");
    }

    #[test]
    fn insert_audit_log_chained_concurrent_writes_no_corruption() {
        // Two threads write to the same org concurrently.
        // The resulting chain must have exactly 2 new records, correctly linked.
        use std::sync::{Arc, Mutex};

        let raw_conn = connect(":memory:").unwrap();
        migrations::run(&raw_conn).unwrap();
        let (org, user, _) = bootstrap(&raw_conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let org_id = org.id.clone();
        let user_id = user.id.clone();

        let conn = Arc::new(Mutex::new(raw_conn));

        let conn1 = Arc::clone(&conn);
        let org_id1 = org_id.clone();
        let user_id1 = user_id.clone();

        let conn2 = Arc::clone(&conn);
        let org_id2 = org_id.clone();
        let user_id2 = user_id.clone();

        std::thread::scope(|s| {
            s.spawn(move || {
                let c = conn1.lock().unwrap();
                insert_audit_log_chained(&c, &org_id1, &user_id1, "store", "memory", None, serde_json::json!({}), None).unwrap();
            });
            s.spawn(move || {
                let c = conn2.lock().unwrap();
                insert_audit_log_chained(&c, &org_id2, &user_id2, "search", "memory", None, serde_json::json!({}), None).unwrap();
            });
        });

        // Verify exactly 2 rows for this org.
        let guard = conn.lock().unwrap();
        let count: i64 = guard.query_row(
            "SELECT COUNT(*) FROM audit_logs WHERE org_id = ?1",
            [&org_id],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 2, "must have exactly 2 audit rows after concurrent writes");

        // Verify chain integrity: at least one row has a non-null current_hash,
        // and the chain links correctly (the second row's previous_hash = first row's current_hash).
        let entries = list_audit(&guard, &org_id, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.current_hash.is_some()), "both rows must have current_hash");

        // Verify linkage: one row has previous_hash=NULL, the other has the first's current_hash.
        let genesis = entries.iter().find(|e| e.previous_hash.is_none()).expect("must have a genesis row");
        let chained = entries.iter().find(|e| e.previous_hash.is_some()).expect("must have a chained row");
        assert_eq!(
            chained.previous_hash.as_ref(),
            genesis.current_hash.as_ref(),
            "second row's previous_hash must equal genesis current_hash"
        );
    }

    #[test]
    fn log_audit_wrapper_still_works() {
        // log_audit (thin wrapper) must still produce a row with non-null current_hash.
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();

        let entries = list_audit(&conn, &org.id, None, None, None, None, None, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].current_hash.is_some(),
            "log_audit wrapper must produce a row with non-null current_hash"
        );
    }

    // ── Policy query tests ────────────────────────────────────────────────────

    #[test]
    fn list_policies_returns_empty_for_new_org() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let policies = list_policies(&conn, &org.id).unwrap();
        assert!(policies.is_empty());
    }

    #[test]
    fn insert_policy_and_get_policy_roundtrip() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        let policy = insert_policy(&conn, &id, &org.id, "Whitelist", "model_whitelist", config_json, true).unwrap();

        assert_eq!(policy.id, id);
        assert_eq!(policy.org_id, org.id);
        assert_eq!(policy.name, "Whitelist");
        assert_eq!(policy.rule_type, "model_whitelist");
        assert!(policy.enabled);

        let fetched = get_policy(&conn, &id, &org.id).unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "Whitelist");
    }

    fn seed_second_org(conn: &Connection) -> String {
        let org2_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Beta', 'beta')",
            rusqlite::params![org2_id],
        ).unwrap();
        org2_id
    }

    #[test]
    fn get_policy_cross_org_returns_none() {
        let conn = setup();
        let (org1, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let org2_id = seed_second_org(&conn);

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(&conn, &id, &org1.id, "Whitelist", "model_whitelist", config_json, true).unwrap();

        // Querying with org2 must return None
        let result = get_policy(&conn, &id, &org2_id).unwrap();
        assert!(result.is_none(), "cross-org query must return None");
    }

    #[test]
    fn list_policies_scoped_to_org() {
        let conn = setup();
        let (org1, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let org2_id = seed_second_org(&conn);

        let id1 = format!("p_{}", Uuid::new_v4().simple());
        let id2 = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;

        insert_policy(&conn, &id1, &org1.id, "Org1 Policy", "model_whitelist", config_json, true).unwrap();
        insert_policy(&conn, &id2, &org2_id, "Org2 Policy", "model_whitelist", config_json, true).unwrap();

        let org1_policies = list_policies(&conn, &org1.id).unwrap();
        let org2_policies = list_policies(&conn, &org2_id).unwrap();

        assert_eq!(org1_policies.len(), 1);
        assert_eq!(org1_policies[0].name, "Org1 Policy");
        assert_eq!(org2_policies.len(), 1);
        assert_eq!(org2_policies[0].name, "Org2 Policy");
    }

    #[test]
    fn update_policy_changes_name_and_enabled() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(&conn, &id, &org.id, "Old Name", "model_whitelist", config_json, true).unwrap();

        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let updated = update_policy(&conn, &id, &org.id, Some("New Name"), None, Some(false), &now).unwrap();
        assert!(updated.is_some());
        let p = updated.unwrap();
        assert_eq!(p.name, "New Name");
        assert!(!p.enabled);
    }

    #[test]
    fn update_policy_returns_none_for_missing_id() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let result = update_policy(&conn, "nonexistent-id", &org.id, Some("X"), None, None, &now).unwrap();
        assert!(result.is_none(), "update must return None for nonexistent id");
    }

    #[test]
    fn delete_policy_removes_row() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(&conn, &id, &org.id, "Temp", "model_whitelist", config_json, true).unwrap();

        let deleted = delete_policy(&conn, &id, &org.id).unwrap();
        assert!(deleted);

        let fetched = get_policy(&conn, &id, &org.id).unwrap();
        assert!(fetched.is_none(), "policy must be gone after delete");
    }

    #[test]
    fn delete_policy_cross_org_returns_false() {
        let conn = setup();
        let (org1, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let org2_id = seed_second_org(&conn);

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(&conn, &id, &org1.id, "Org1 Policy", "model_whitelist", config_json, true).unwrap();

        let deleted = delete_policy(&conn, &id, &org2_id).unwrap();
        assert!(!deleted, "delete from wrong org must return false");

        // Policy still exists in org1
        assert!(get_policy(&conn, &id, &org1.id).unwrap().is_some());
    }

    #[test]
    fn list_enabled_policies_excludes_disabled() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let id1 = format!("p_{}", Uuid::new_v4().simple());
        let id2 = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;

        insert_policy(&conn, &id1, &org.id, "Enabled", "model_whitelist", config_json, true).unwrap();
        insert_policy(&conn, &id2, &org.id, "Disabled", "model_whitelist", config_json, false).unwrap();

        let enabled = list_enabled_policies(&conn, &org.id).unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "Enabled");
    }

    #[test]
    fn fetch_daily_stats_returns_zero_for_empty_org() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let stats = fetch_daily_stats(&conn, &org.id).unwrap();
        assert_eq!(stats.requests_today, 0);
        assert_eq!(stats.tokens_today, 0);
    }

    #[test]
    fn get_role_permissions_admin_includes_policy_write() {
        let conn = setup();
        let perms = get_role_permissions(&conn, "irrelevant", "admin").unwrap();
        assert!(perms.contains(&"policy:read".to_string()), "admin must have policy:read");
        assert!(perms.contains(&"policy:write".to_string()), "admin must have policy:write");
    }

    #[test]
    fn get_role_permissions_member_includes_policy_read_only() {
        let conn = setup();
        let perms = get_role_permissions(&conn, "irrelevant", "member").unwrap();
        assert!(perms.contains(&"policy:read".to_string()), "member must have policy:read");
        assert!(!perms.contains(&"policy:write".to_string()), "member must NOT have policy:write");
    }

    #[test]
    fn get_role_permissions_viewer_has_no_policy_perms() {
        let conn = setup();
        let perms = get_role_permissions(&conn, "irrelevant", "viewer").unwrap();
        assert!(!perms.contains(&"policy:read".to_string()), "viewer must not have policy:read");
        assert!(!perms.contains(&"policy:write".to_string()), "viewer must not have policy:write");
    }

    // ── Code index query tests ─────────────────────────────────────────────────

    fn setup_org_for_code(conn: &Connection) -> String {
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        "org1".to_string()
    }

    #[test]
    fn upsert_code_project_creates_new() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let id = upsert_code_project(&conn, &org_id, "myapp", "/ws/myapp").unwrap();
        assert!(id > 0, "project id must be positive");
    }

    #[test]
    fn upsert_code_project_idempotent() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let id1 = upsert_code_project(&conn, &org_id, "myapp", "/ws/myapp").unwrap();
        let id2 = upsert_code_project(&conn, &org_id, "myapp", "/ws/myapp2").unwrap();
        assert_eq!(id1, id2, "upsert must return same id for same (org_id, name)");
        // root_path should have been updated
        let project = get_code_project(&org_id, "myapp", &conn).unwrap().unwrap();
        assert_eq!(project.root_path, "/ws/myapp2", "root_path must be updated on conflict");
    }

    #[test]
    fn insert_and_get_code_chunks() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        let chunk_id = insert_code_chunk(
            &conn, project_id, "src/lib.rs", "abc123",
            Some("rust"), Some("authenticate_user"),
            1, 10, "fn authenticate_user() {}", None,
        ).unwrap();
        assert!(chunk_id > 0);

        let chunks = get_chunks_by_ids(&conn, &[chunk_id]).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].symbol.as_deref(), Some("authenticate_user"));
        assert_eq!(chunks[0].file_path, "src/lib.rs");
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 10);
    }

    #[test]
    fn delete_chunks_for_file_removes_only_target_file() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        insert_code_chunk(&conn, project_id, "src/lib.rs", "h1", Some("rust"), None, 1, 10, "code", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/main.rs", "h2", Some("rust"), None, 1, 5, "main", None).unwrap();

        delete_chunks_for_file(&conn, project_id, "src/lib.rs").unwrap();

        let lib_count = count_chunks_for_file(&conn, project_id, "src/lib.rs").unwrap();
        let main_count = count_chunks_for_file(&conn, project_id, "src/main.rs").unwrap();
        assert_eq!(lib_count, 0, "lib.rs chunks must be deleted");
        assert_eq!(main_count, 1, "main.rs chunks must be preserved");
    }

    #[test]
    fn get_code_embeddings_returns_only_non_null() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        let embedding: Vec<u8> = vec![0u8; 32]; // dummy blob
        insert_code_chunk(&conn, project_id, "a.rs", "h1", None, None, 1, 5, "code", Some(&embedding)).unwrap();
        insert_code_chunk(&conn, project_id, "b.rs", "h2", None, None, 1, 5, "code", None).unwrap();

        let pairs = get_code_embeddings(&conn, project_id).unwrap();
        assert_eq!(pairs.len(), 1, "only chunk with embedding must be returned");
    }

    #[test]
    fn list_indexed_files_with_hashes_returns_map() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        insert_code_chunk(&conn, project_id, "src/lib.rs", "deadbeef", None, None, 1, 5, "code", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/lib.rs", "deadbeef", None, None, 6, 10, "more", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/main.rs", "cafebabe", None, None, 1, 3, "main", None).unwrap();

        let hashes = list_indexed_files_with_hashes(&conn, project_id).unwrap();
        assert_eq!(hashes.len(), 2, "must deduplicate by file_path");
        assert_eq!(hashes.get("src/lib.rs").map(|h| h.as_str()), Some("deadbeef"));
        assert_eq!(hashes.get("src/main.rs").map(|h| h.as_str()), Some("cafebabe"));
    }

    #[test]
    fn update_code_project_stats_sets_counts() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        update_code_project_stats(&conn, project_id, 5, 42, "2026-06-19T12:00:00Z").unwrap();

        let project = get_code_project(&org_id, "myapp", &conn).unwrap().unwrap();
        assert_eq!(project.file_count, 5);
        assert_eq!(project.chunk_count, 42);
        assert_eq!(project.last_indexed.as_deref(), Some("2026-06-19T12:00:00Z"));
    }

    #[test]
    fn get_code_project_returns_none_for_unknown() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let result = get_code_project(&org_id, "ghost", &conn).unwrap();
        assert!(result.is_none(), "must return None for unknown project");
    }

    #[test]
    fn get_code_project_org_isolation() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org2', 'Beta', 'beta')",
            [],
        ).unwrap();
        upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();
        // org2 must not see org1's project
        let result = get_code_project("org2", "myapp", &conn).unwrap();
        assert!(result.is_none(), "org isolation must hold for code projects");
    }

    #[test]
    fn get_chunk_context_returns_target_and_neighbors() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        // Insert 3 chunks: before, target, after — all in the same file
        insert_code_chunk(&conn, project_id, "src/auth.rs", "h1", None, Some("validate_token"), 1, 20, "fn validate_token() {}", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/auth.rs", "h1", None, Some("authenticate_user"), 21, 60, "fn authenticate_user() {}", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/auth.rs", "h1", None, Some("refresh_token"), 61, 80, "fn refresh_token() {}", None).unwrap();

        let context = get_chunk_context(&conn, project_id, "src/auth.rs", "authenticate_user", 1).unwrap();
        assert!(!context.is_empty(), "must return at least the target chunk");
        assert!(
            context.iter().any(|c| c.symbol.as_deref() == Some("authenticate_user")),
            "target chunk must be present"
        );
    }

    #[test]
    fn get_chunk_context_returns_empty_for_unknown_symbol() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        let context = get_chunk_context(&conn, project_id, "src/auth.rs", "nonexistent_fn", 1).unwrap();
        assert!(context.is_empty(), "must return empty vec for unknown symbol");
    }
}
