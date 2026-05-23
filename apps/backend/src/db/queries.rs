use anyhow::Result;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::api_keys;
use crate::models::types::{
    AuthContext, CreateSessionRequest, Memory, Org, OrgStats, PatchSessionRequest,
    Session, StoreMemoryRequest, ToolUsage, User,
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
        Ok((org_id, user_id, role, status)) => {
            if status != "active" {
                return Ok(None);
            }
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

/// Stores a new memory entry for a user within an org.
pub fn store_memory(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    project: &str,
    tool: &str,
    content: &str,
    tags: &[String],
) -> Result<Memory> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let tags_json = serde_json::to_string(tags)?;

    conn.execute(
        "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, org_id, user_id, project, tool, content, tags_json, now],
    )?;

    Ok(Memory {
        id,
        org_id: org_id.to_string(),
        user_id: user_id.to_string(),
        project: project.to_string(),
        tool: tool.to_string(),
        content: content.to_string(),
        tags: tags.to_vec(),
        created_at: now,
        title: None,
        memory_type: None,
        scope: "project".to_string(),
        topic_key: None,
        session_id: None,
        revision_count: 1,
        normalized_hash: None,
    })
}

/// Full-text search over memories, scoped to the org.
pub fn search_memories(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.org_id, m.user_id, m.project, m.tool, m.content, m.tags, m.created_at,
                m.title, m.type, m.scope, m.topic_key, m.session_id, m.revision_count, m.normalized_hash
         FROM memories m
         JOIN memories_fts fts ON fts.rowid = m.rowid
         WHERE memories_fts MATCH ?1 AND m.org_id = ?2
         ORDER BY rank
         LIMIT ?3",
    )?;

    let rows = stmt.query_map(rusqlite::params![query, org_id, limit], |row| {
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
        ))
    })?;

    let mut memories = Vec::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash) = row?;
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
                title, type, scope, topic_key, session_id, revision_count, normalized_hash
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
        ))
    })?;

    let mut memories = Vec::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash) = row?;
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
        });
    }
    Ok(memories)
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
        "SELECT id, org_id, user_id, timestamp, action, resource_type, resource_id, metadata
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
        ))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (id, org_id, user_id, timestamp, action, resource_type, resource_id, meta_str) = row?;
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
        });
    }
    Ok(entries)
}

/// Writes an audit log entry.
pub fn log_audit(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    metadata: serde_json::Value,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let meta_str = serde_json::to_string(&metadata)?;

    conn.execute(
        "INSERT INTO audit_logs (id, org_id, user_id, timestamp, action, resource_type, resource_id, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, org_id, user_id, now, action, resource_type, resource_id, meta_str],
    )?;

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
                     normalized_hash = ?5, revision_count = ?6, tags = ?7
                     WHERE id = ?8",
                    rusqlite::params![
                        req.content, req.title, req.memory_type, scope,
                        normalized_hash, new_revision, tags_json, existing_id
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
                               title, type, scope, topic_key, session_id, revision_count, normalized_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14)",
        rusqlite::params![
            id, org_id, user_id, project, req.tool, req.content, tags_json, now,
            req.title, req.memory_type, scope, req.topic_key, req.session_id, normalized_hash
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::db::migrations;

    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn
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
        assert_eq!(ctx.role, "admin");
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
        let mem = store_memory(&conn, &org.id, &user.id, "nexusmind", "claude", "use anyhow for errors", &tags).unwrap();

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

        store_memory(&conn, &org.id, &user.id, "proj", "claude", "use snake_case for identifiers", &[]).unwrap();
        store_memory(&conn, &org.id, &user.id, "proj", "claude", "database migrations run at startup", &[]).unwrap();

        let results = search_memories(&conn, &org.id, "snake_case", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("snake_case"));
    }

    #[test]
    fn search_memories_scoped_to_org() {
        let conn = setup();
        // org1
        let (org1, user1, _) = bootstrap(&conn, "Org1", "org1", "admin@org1.com", "Admin1").unwrap();
        store_memory(&conn, &org1.id, &user1.id, "proj", "claude", "secret content org1", &[]).unwrap();

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

        store_memory(&conn, &org.id, &user.id, "proj-a", "claude", "mem 1", &[]).unwrap();
        store_memory(&conn, &org.id, &user.id, "proj-b", "cursor", "mem 2", &[]).unwrap();
        store_memory(&conn, &org.id, &user.id, "proj-a", "cursor", "mem 3", &[]).unwrap();

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
        let mem = store_memory(&conn, &org.id, &user.id, "proj", "claude", "content", &[]).unwrap();

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

        store_memory(&conn, &org.id, &user.id, "proj", "claude", "mem 1", &[]).unwrap();
        store_memory(&conn, &org.id, &user.id, "proj", "claude", "mem 2", &[]).unwrap();
        store_memory(&conn, &org.id, &user.id, "proj", "cursor", "mem 3", &[]).unwrap();

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
}
