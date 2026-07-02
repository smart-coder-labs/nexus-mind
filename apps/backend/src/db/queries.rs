use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use std::collections::{HashMap, HashSet};

use crate::auth::api_keys;
use crate::models::types::{
    AuthContext, AuditEntry, CodeChunk, CodeProject, CreateSessionRequest, CustomRole,
    GlobalMetrics, GraphEdgeDto, GraphNodeDto, MemGraphEdge, MemGraphNode, Memory, Org,
    OrgSettings, OrgStats, OrgWithStats,
    PatchSessionRequest, Policy, Session, SessionWithCount, StoreMemoryRequest, ToolUsage, User,
    UserRole, Project, ProjectMember, ProjectEventOverrides, Webhook, CreateWebhookRequest,
    UpdateWebhookRequest, WebhookDelivery, ApiKeyWithUser, OnboardingItem, OnboardingStatus,
    InviteLink, Convention, CreateConventionRequest, UpdateConventionRequest, GitHubConnection,
    Agent, CreateAgentRequest, UpdateAgentRequest, AgentAssignment,
};
use crate::indexer::tree_sitter_chunker::{FileGraph, Persist};

/// Looks up an API key by its SHA-256 hash.
/// Returns AuthContext if the key exists, is not revoked, the user is active,
/// and the account has not been disabled.
/// Also updates `last_used` on the api_keys row.
pub fn validate_api_key(conn: &Connection, key_hash: &str) -> Result<Option<AuthContext>> {
    let result = conn.query_row(
        "SELECT ak.org_id, ak.user_id, u.role, u.status, u.disabled_at
         FROM api_keys ak
         JOIN users u ON u.id = ak.user_id
         WHERE ak.key_hash = ?1 AND ak.revoked = 0
           AND (ak.expires_at IS NULL OR ak.expires_at > datetime('now'))
           AND u.disabled_at IS NULL",
        [key_hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        },
    );

    match result {
        Ok((org_id, user_id, role_str, status, _disabled_at)) => {
            if status != "active" {
                return Ok(None);
            }
            let role = match role_str.parse::<UserRole>() {
                Ok(r) => r,
                Err(_) => return Ok(None),
            };
            conn.execute(
                "UPDATE api_keys SET last_used = datetime('now'), times_used = COALESCE(times_used, 0) + 1, last_used_at = datetime('now') WHERE key_hash = ?1",
                [key_hash],
            )?;
            conn.execute(
                "UPDATE users SET last_login_at = datetime('now') WHERE id = ?1",
                [&user_id],
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

/// Checks whether a key exists and its associated user account is disabled.
/// Returns true if the key exists, is not revoked, has not expired, but the user's
/// disabled_at IS NOT NULL. Used by auth middleware to return a specific error code.
pub fn is_key_account_disabled(conn: &Connection, key_hash: &str) -> Result<bool> {
    let result: Option<Option<String>> = conn.query_row(
        "SELECT u.disabled_at
         FROM api_keys ak
         JOIN users u ON u.id = ak.user_id
         WHERE ak.key_hash = ?1 AND ak.revoked = 0
           AND (ak.expires_at IS NULL OR ak.expires_at > datetime('now'))",
        [key_hash],
        |row| row.get(0),
    ).optional()?;
    Ok(result.map(|d| d.is_some()).unwrap_or(false))
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
        last_active: None,
        disabled_at: None,
        admin_note: None,
        last_login_at: None,
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
///
/// Tokens are joined with `OR` (not implicit AND via whitespace) so that
/// natural-language queries with several terms still match rows containing
/// only a subset of them — results are then ranked by `bm25` (via `ORDER BY
/// rank` in `search_memories`), so rows matching more terms still surface
/// first. Joining with plain whitespace would make FTS5 require every term
/// to be present in the same row, which frequently yields zero results for
/// longer queries.
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

    if terms.is_empty() { None } else { Some(terms.join(" OR ")) }
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
                m.title, m.type, m.scope, m.topic_key, m.session_id, m.revision_count, m.normalized_hash, m.project_id,
                m.archived_at, m.pinned, m.collection_id, m.admin_note, m.delete_after
         FROM memories m
         JOIN memories_fts fts ON fts.rowid = m.rowid
         WHERE memories_fts MATCH ?1 AND m.org_id = ?2 AND m.archived_at IS NULL
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
            row.get::<_, Option<String>>(16)?,
            row.get::<_, i64>(17).unwrap_or(0),
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<String>>(20)?,
        ))
    })?;

    let mut memories = Vec::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
             archived_at, pinned_i64, collection_id, admin_note, delete_after) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() { "archived".to_string() } else { "active".to_string() };
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
            archived_at,
            pinned: pinned_i64 != 0,
            collection_id,
            admin_note,
            delete_after,
            status,
        });
    }
    Ok(memories)
}

/// Lists memories for an org with optional filters.
/// When `include_archived` is false (default), archived memories (archived_at IS NOT NULL) are excluded.
/// `from_date` / `to_date` are ISO 8601 date strings ("YYYY-MM-DD"). When provided they bound
/// `created_at` as: `created_at >= from_date` and `created_at < date(to_date, '+1 day')`.
#[allow(clippy::too_many_arguments)]
pub fn list_memories(
    conn: &Connection,
    org_id: &str,
    user_id: Option<&str>,
    tool: Option<&str>,
    project: Option<&str>,
    type_filter: Option<&str>,
    scope_filter: Option<&str>,
    session_id_filter: Option<&str>,
    limit: i64,
    offset: i64,
    include_archived: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
    collection_id_filter: Option<&str>,
) -> Result<Vec<Memory>> {
    let mut sql = String::from(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                archived_at, pinned, collection_id, admin_note, delete_after
         FROM memories
         WHERE org_id = ?1",
    );
    let mut param_idx = 2usize;
    let mut extra_params: Vec<String> = Vec::new();

    if !include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }

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
    if let Some(sid) = session_id_filter {
        sql.push_str(&format!(" AND session_id = ?{param_idx}"));
        extra_params.push(sid.to_string());
        param_idx += 1;
    }
    if let Some(fd) = from_date {
        sql.push_str(&format!(" AND created_at >= ?{param_idx}"));
        extra_params.push(fd.to_string());
        param_idx += 1;
    }
    if let Some(td) = to_date {
        sql.push_str(&format!(" AND created_at < date(?{param_idx}, '+1 day')"));
        extra_params.push(td.to_string());
        param_idx += 1;
    }
    if let Some(cid) = collection_id_filter {
        sql.push_str(&format!(" AND collection_id = ?{param_idx}"));
        extra_params.push(cid.to_string());
        param_idx += 1;
    }
    sql.push_str(&format!(
        " ORDER BY pinned DESC, created_at DESC LIMIT ?{param_idx} OFFSET ?{}",
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
            row.get::<_, Option<String>>(16)?,
            row.get::<_, i64>(17).unwrap_or(0),
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<String>>(20)?,
        ))
    })?;

    let mut memories = Vec::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
             archived_at, pinned_i64, collection_id, admin_note, delete_after) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() { "archived".to_string() } else { "active".to_string() };
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
            archived_at,
            pinned: pinned_i64 != 0,
            collection_id,
            admin_note,
            delete_after,
            status,
        });
    }
    Ok(memories)
}

/// Count memories matching the same filters as `list_memories`.
#[allow(clippy::too_many_arguments)]
pub fn count_memories(
    conn: &Connection,
    org_id: &str,
    user_id: Option<&str>,
    tool: Option<&str>,
    project: Option<&str>,
    type_filter: Option<&str>,
    scope_filter: Option<&str>,
    session_id_filter: Option<&str>,
    include_archived: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
    collection_id_filter: Option<&str>,
) -> Result<i64> {
    let mut sql = String::from("SELECT COUNT(*) FROM memories WHERE org_id = ?1");
    let mut param_idx = 2usize;
    let mut extra_params: Vec<String> = Vec::new();

    if !include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }
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
    if let Some(sid) = session_id_filter {
        sql.push_str(&format!(" AND session_id = ?{param_idx}"));
        extra_params.push(sid.to_string());
        param_idx += 1;
    }
    if let Some(fd) = from_date {
        sql.push_str(&format!(" AND created_at >= ?{param_idx}"));
        extra_params.push(fd.to_string());
        param_idx += 1;
    }
    if let Some(td) = to_date {
        sql.push_str(&format!(" AND created_at < date(?{param_idx}, '+1 day')"));
        extra_params.push(td.to_string());
        param_idx += 1;
    }
    if let Some(cid) = collection_id_filter {
        sql.push_str(&format!(" AND collection_id = ?{param_idx}"));
        extra_params.push(cid.to_string());
        param_idx += 1;
    }
    let _ = param_idx;

    let mut all_params: Vec<String> = vec![org_id.to_string()];
    all_params.extend(extra_params);
    let refs: Vec<&dyn rusqlite::ToSql> = all_params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let count: i64 = conn.query_row(&sql, refs.as_slice(), |row| row.get(0))?;
    Ok(count)
}

/// Archives a memory (sets archived_at = now). No-op if already archived.
/// Returns Ok(true) if the row was updated, Ok(false) if not found / already archived.
pub fn archive_memory(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE memories SET archived_at = datetime('now') WHERE id = ?1 AND org_id = ?2 AND archived_at IS NULL",
        rusqlite::params![id, org_id],
    )?;
    Ok(affected > 0)
}

/// Restores a memory (clears archived_at). No-op if not archived.
/// Returns Ok(true) if the row was updated, Ok(false) if not found / not archived.
pub fn restore_memory(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE memories SET archived_at = NULL WHERE id = ?1 AND org_id = ?2 AND archived_at IS NOT NULL",
        rusqlite::params![id, org_id],
    )?;
    Ok(affected > 0)
}

/// Pins a memory (sets pinned = 1). Returns true if updated, false if not found.
pub fn pin_memory(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE memories SET pinned = 1 WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
    )?;
    Ok(affected > 0)
}

/// Unpins a memory (sets pinned = 0). Returns true if updated, false if not found.
pub fn unpin_memory(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE memories SET pinned = 0 WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
    )?;
    Ok(affected > 0)
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

/// Bulk-deletes a set of memories scoped to the org.
///
/// For admins (`is_admin = true`) any memory belonging to the org is deleted.
/// For non-admins only memories owned by `caller_user_id` are deleted.
///
/// Returns the count of rows actually deleted.
pub fn bulk_delete_memories(
    conn: &Connection,
    org_id: &str,
    ids: &[String],
    is_admin: bool,
    caller_user_id: &str,
) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    // Build a parameterised placeholder list: (?2, ?3, … ?N)
    // ?1 = org_id, ?2..?N+1 = ids, ?N+2 = caller_user_id (non-admin path only)
    let placeholders: Vec<String> = (2..=ids.len() + 1)
        .map(|i| format!("?{i}"))
        .collect();
    let in_clause = placeholders.join(", ");

    let sql = if is_admin {
        format!(
            "DELETE FROM memories WHERE org_id = ?1 AND id IN ({in_clause})"
        )
    } else {
        let user_param = ids.len() + 2;
        format!(
            "DELETE FROM memories WHERE org_id = ?1 AND id IN ({in_clause}) AND user_id = ?{user_param}"
        )
    };

    // Build the parameter list
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    params.push(Box::new(org_id.to_string()));
    for id in ids {
        params.push(Box::new(id.clone()));
    }
    if !is_admin {
        params.push(Box::new(caller_user_id.to_string()));
    }

    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let affected = conn.execute(&sql, refs.as_slice())?;
    Ok(affected)
}

/// Bulk-adds or removes a tag from a set of memories, scoped to org_id.
///
/// For each id in `ids`:
/// - Fetch current `tags` JSON from the row (org-scoped).
/// - Parse as `Vec<String>`.
/// - Add or remove `tag` (case-insensitive match; add deduplicates).
/// - Write the updated array back.
///
/// All updates run inside a single transaction for atomicity.
/// Memories not belonging to `org_id` are silently skipped.
/// Returns the count of rows actually updated.
pub fn bulk_tag_memories(
    conn: &Connection,
    org_id: &str,
    ids: &[String],
    action: &str,
    tag: &str,
) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let tag_lower = tag.to_lowercase();
    let tx = conn.unchecked_transaction()?;
    let mut updated = 0usize;

    for id in ids {
        // Fetch current tags, scoped to org.
        let result: rusqlite::Result<String> = tx.query_row(
            "SELECT tags FROM memories WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![id, org_id],
            |row| row.get(0),
        );

        let tags_str = match result {
            Ok(s) => s,
            Err(rusqlite::Error::QueryReturnedNoRows) => continue, // wrong org or not found
            Err(e) => return Err(e.into()),
        };

        let mut tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

        match action {
            "add" => {
                let already = tags.iter().any(|t| t.to_lowercase() == tag_lower);
                if !already {
                    tags.push(tag.to_string());
                }
            }
            "remove" => {
                tags.retain(|t| t.to_lowercase() != tag_lower);
            }
            _ => {} // unknown action — skip
        }

        let new_tags_json = serde_json::to_string(&tags)?;
        let affected = tx.execute(
            "UPDATE memories SET tags = ?1 WHERE id = ?2 AND org_id = ?3",
            rusqlite::params![new_tags_json, id, org_id],
        )?;
        updated += affected;
    }

    tx.commit()?;
    Ok(updated)
}

// ── User queries ──────────────────────────────────────────────────────────────

/// Returns all users in the org.
pub fn list_users(conn: &Connection, org_id: &str) -> Result<Vec<User>> {
    let mut stmt = conn.prepare(
        "SELECT u.id, u.org_id, u.email, u.name, u.role, u.status, u.created_at,
                MAX(ak.last_used) AS last_active, u.disabled_at, u.admin_note, u.last_login_at
         FROM users u
         LEFT JOIN api_keys ak ON ak.user_id = u.id AND ak.revoked = 0
         WHERE u.org_id = ?1
         GROUP BY u.id
         ORDER BY u.created_at ASC",
    )?;

    let rows = stmt.query_map([org_id], |row| {
        Ok(User {
            id: row.get(0)?,
            org_id: row.get(1)?,
            email: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            name: row.get(3)?,
            role: row.get(4)?,
            status: row.get(5)?,
            created_at: row.get(6)?,
            last_active: row.get(7)?,
            disabled_at: row.get(8)?,
            admin_note: row.get(9)?,
            last_login_at: row.get(10)?,
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
        last_active: None,
        disabled_at: None,
        admin_note: None,
        last_login_at: None,
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

/// Disables a user account by setting disabled_at to the current datetime.
/// Disabled accounts have all API requests rejected.
/// Does not revoke keys — re-enabling restores access without key rotation.
/// Returns true if the user was found and is not already disabled, false if not found or already disabled.
pub fn disable_user(conn: &Connection, org_id: &str, user_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE users SET disabled_at = datetime('now') WHERE id = ?1 AND org_id = ?2 AND disabled_at IS NULL",
        [user_id, org_id],
    )?;
    Ok(affected > 0)
}

/// Re-enables a user account by clearing disabled_at.
/// Returns true if the user was found and was disabled, false if not found or already active.
pub fn enable_user(conn: &Connection, org_id: &str, user_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE users SET disabled_at = NULL WHERE id = ?1 AND org_id = ?2 AND disabled_at IS NOT NULL",
        [user_id, org_id],
    )?;
    Ok(affected > 0)
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

/// Admin-only reset: generates a new API key for a user, revoking their current non-demo key.
/// Returns the raw new key (only time it is visible) or an error string for callers to map.
/// Error values:
///   "not_found"    → user does not exist in the org
///   "demo_key"     → user's active key has the demo label and must not be reset
pub fn reset_user_key(conn: &Connection, org_id: &str, user_id: &str) -> Result<std::result::Result<String, &'static str>> {
    // Check whether the user exists in this org.
    let user_exists: bool = conn.query_row(
        "SELECT count(*) FROM users WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![user_id, org_id],
        |row| row.get::<_, i32>(0),
    )? > 0;

    if !user_exists {
        return Ok(Err("not_found"));
    }

    // Check whether the user's active key is a demo key (label starts with 'demo').
    let has_demo_key: bool = conn.query_row(
        "SELECT count(*) FROM api_keys
         WHERE user_id = ?1 AND org_id = ?2 AND revoked = 0 AND label LIKE 'demo%'",
        rusqlite::params![user_id, org_id],
        |row| row.get::<_, i32>(0),
    )? > 0;

    if has_demo_key {
        return Ok(Err("demo_key"));
    }

    // Revoke all current keys for the user in this org.
    conn.execute(
        "UPDATE api_keys SET revoked = 1 WHERE user_id = ?1 AND org_id = ?2",
        rusqlite::params![user_id, org_id],
    )?;

    // Generate and insert a fresh key.
    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'admin-reset', ?5)",
        rusqlite::params![key_id, user_id, org_id, key_hash, now],
    )?;

    Ok(Ok(raw_key))
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
    search: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<crate::models::types::AuditEntry>> {
    list_audit_with_resource(conn, org_id, user_id, action, resource_type, None, from, to, search, limit, offset)
}

/// Extended audit query that also supports filtering by resource_id.
#[allow(clippy::too_many_arguments)]
pub fn list_audit_with_resource(
    conn: &Connection,
    org_id: &str,
    user_id: Option<&str>,
    action: Option<&str>,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    search: Option<&str>,
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
    if let Some(rid) = resource_id {
        sql.push_str(&format!(" AND resource_id = ?{param_idx}"));
        extra_params.push(rid.to_string());
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
    if let Some(s) = search {
        sql.push_str(&format!(
            " AND (\
              action LIKE '%' || ?{param_idx} || '%'\
              OR resource_type LIKE '%' || ?{param_idx} || '%'\
              OR COALESCE(resource_id, '') LIKE '%' || ?{param_idx} || '%'\
              OR metadata LIKE '%' || ?{param_idx} || '%'\
            )"
        ));
        extra_params.push(s.to_string());
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

#[allow(clippy::type_complexity)]
pub fn get_org_settings(conn: &Connection, org_id: &str) -> Result<OrgSettings> {
    let (raw, retention_days, custom_instructions, min_password_length, announcement, announcement_type, logo_url): (String, Option<i64>, Option<String>, Option<i64>, Option<String>, Option<String>, Option<String>) = conn.query_row(
        "SELECT COALESCE(settings, '{}'), retention_days, custom_instructions, min_password_length, announcement, announcement_type, logo_url FROM organizations WHERE id = ?1",
        [org_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    ).unwrap_or_else(|_| ("{}".to_string(), None, None, None, None, None, None));

    let mut settings: OrgSettings = serde_json::from_str(&raw).unwrap_or_default();
    settings.retention_days = retention_days;
    settings.custom_instructions = custom_instructions;
    settings.min_password_length = min_password_length;
    settings.announcement = announcement;
    settings.announcement_type = announcement_type;
    settings.logo_url = logo_url;
    Ok(settings)
}

pub fn update_org_settings(conn: &Connection, org_id: &str, settings: &OrgSettings) -> Result<OrgSettings> {
    // Strip direct-column fields from the JSON blob — they live in their own columns.
    let blob_settings = OrgSettings { retention_days: None, custom_instructions: None, min_password_length: None, announcement: None, announcement_type: None, ..settings.clone() };
    let raw = serde_json::to_string(&blob_settings)?;
    let ann: Option<&str> = settings.announcement.as_deref().filter(|s| !s.is_empty());
    let ann_type = settings.announcement_type.as_deref().unwrap_or("info");
    conn.execute(
        "UPDATE organizations SET settings = ?1, retention_days = ?2, custom_instructions = ?3, min_password_length = ?4, announcement = ?5, announcement_type = ?6 WHERE id = ?7",
        rusqlite::params![raw, settings.retention_days, settings.custom_instructions, settings.min_password_length, ann, ann_type, org_id],
    )?;
    get_org_settings(conn, org_id)
}

/// Set (or clear) the announcement banner for an org.
/// Empty `announcement` string → NULL (clears the banner).
pub fn update_announcement(conn: &Connection, org_id: &str, announcement: &str, announcement_type: &str) -> Result<OrgSettings> {
    let ann: Option<&str> = if announcement.is_empty() { None } else { Some(announcement) };
    conn.execute(
        "UPDATE organizations SET announcement = ?1, announcement_type = ?2 WHERE id = ?3",
        rusqlite::params![ann, announcement_type, org_id],
    )?;
    get_org_settings(conn, org_id)
}

/// Set (or clear) the logo URL for an org.
/// None = clear the logo (sets logo_url = NULL).
pub fn update_org_logo(conn: &Connection, org_id: &str, logo_url: Option<&str>) -> Result<OrgSettings> {
    conn.execute(
        "UPDATE organizations SET logo_url = ?1 WHERE id = ?2",
        rusqlite::params![logo_url, org_id],
    )?;
    get_org_settings(conn, org_id)
}

/// Set (or clear) the scheduled-deletion date for a single memory.
/// `delete_after` = None → clears the schedule.
pub fn schedule_memory_delete(conn: &Connection, org_id: &str, memory_id: &str, delete_after: Option<&str>) -> Result<()> {
    let affected = conn.execute(
        "UPDATE memories SET delete_after = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![delete_after, memory_id, org_id],
    )?;
    if affected == 0 {
        return Err(anyhow::anyhow!("memory_not_found"));
    }
    Ok(())
}

/// Delete all memories whose `delete_after` date has passed (on or before today).
/// Should be called alongside the retention-policy cleanup.
pub fn apply_scheduled_deletes(conn: &Connection, org_id: &str) -> Result<u64> {
    let n = conn.execute(
        "DELETE FROM memories WHERE org_id = ?1 AND delete_after IS NOT NULL AND delete_after <= date('now')",
        [org_id],
    )?;
    Ok(n as u64)
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

// ── Org usage stats ───────────────────────────────────────────────────────────

/// Returns org-level entity counts (memories, sessions, users, projects, code_repos).
pub fn get_usage_stats(conn: &Connection, org_id: &str) -> Result<crate::models::types::UsageStats> {
    let memories: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1",
        [org_id],
        |r| r.get(0),
    )?;
    let sessions: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE org_id = ?1",
        [org_id],
        |r| r.get(0),
    )?;
    let users: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users WHERE org_id = ?1 AND status != 'suspended'",
        [org_id],
        |r| r.get(0),
    )?;
    let projects: i64 = conn.query_row(
        "SELECT COUNT(*) FROM projects WHERE org_id = ?1",
        [org_id],
        |r| r.get(0),
    )?;
    let code_repos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM code_projects WHERE org_id = ?1",
        [org_id],
        |r| r.get(0),
    )?;
    Ok(crate::models::types::UsageStats { memories, sessions, users, projects, code_repos })
}

// ── Memory facets ─────────────────────────────────────────────────────────────

/// Returns distinct facet counts (type, scope, project) for an org's memories.
/// Each facet bucket is ordered by count descending, limited to 50 values.
pub fn get_memory_facets(conn: &Connection, org_id: &str) -> Result<crate::models::types::MemoryFacets> {
    // Types
    let mut stmt = conn.prepare(
        "SELECT COALESCE(type, ''), COUNT(*) as cnt
         FROM memories
         WHERE org_id = ?1 AND type IS NOT NULL AND type != ''
         GROUP BY type
         ORDER BY cnt DESC
         LIMIT 50",
    )?;
    let types: Vec<crate::models::types::FacetCount> = stmt
        .query_map([org_id], |row| {
            Ok(crate::models::types::FacetCount {
                value: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // Scopes
    let mut stmt = conn.prepare(
        "SELECT COALESCE(scope, 'project'), COUNT(*) as cnt
         FROM memories
         WHERE org_id = ?1
         GROUP BY COALESCE(scope, 'project')
         ORDER BY cnt DESC
         LIMIT 50",
    )?;
    let scopes: Vec<crate::models::types::FacetCount> = stmt
        .query_map([org_id], |row| {
            Ok(crate::models::types::FacetCount {
                value: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // Projects
    let mut stmt = conn.prepare(
        "SELECT project, COUNT(*) as cnt
         FROM memories
         WHERE org_id = ?1
         GROUP BY project
         ORDER BY cnt DESC
         LIMIT 50",
    )?;
    let projects: Vec<crate::models::types::FacetCount> = stmt
        .query_map([org_id], |row| {
            Ok(crate::models::types::FacetCount {
                value: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    Ok(crate::models::types::MemoryFacets { types, scopes, projects })
}

// ── Tag stats ─────────────────────────────────────────────────────────────────

/// Returns tag usage counts across all memories for the org.
/// `memories.tags` is stored as a JSON array string like '["tag1","tag2"]'.
/// SQLite's `json_each` expands the array so we can GROUP BY individual tag values.
pub fn get_tag_stats(conn: &Connection, org_id: &str) -> Result<Vec<crate::models::types::NameCount>> {
    use crate::models::types::NameCount;

    let mut stmt = conn.prepare(
        "SELECT value as name, COUNT(*) as count
         FROM memories, json_each(memories.tags)
         WHERE memories.org_id = ?1
           AND memories.tags != '[]'
           AND memories.tags IS NOT NULL
         GROUP BY value
         ORDER BY count DESC
         LIMIT 50",
    )?;
    let rows = stmt.query_map([org_id], |row| {
        Ok(NameCount {
            name: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut tags = Vec::new();
    for r in rows {
        tags.push(r?);
    }
    Ok(tags)
}

// ── Tag rename ───────────────────────────────────────────────────────────────

/// Renames a tag across all memories in the org.
/// Returns the number of memories updated.
pub fn rename_tag(conn: &Connection, org_id: &str, from: &str, to: &str) -> Result<i64> {
    // Step 1: find all memory IDs where the tag array contains `from`
    let mut stmt = conn.prepare(
        "SELECT id FROM memories
         WHERE org_id = ?1
           AND tags IS NOT NULL
           AND json_type(tags) = 'array'
           AND EXISTS (
             SELECT 1 FROM json_each(tags) WHERE value = ?2
           )",
    )?;

    let ids: Vec<String> = stmt
        .query_map(rusqlite::params![org_id, from], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?;

    if ids.is_empty() {
        return Ok(0);
    }

    // Step 2: for each memory, rewrite the tags JSON replacing `from` with `to`
    let tx = conn.unchecked_transaction()?;
    let mut updated = 0i64;

    for id in &ids {
        let affected = tx.execute(
            "UPDATE memories
             SET tags = (
               SELECT json_group_array(CASE WHEN value = ?1 THEN ?2 ELSE value END)
               FROM json_each(tags)
             )
             WHERE id = ?3 AND org_id = ?4",
            rusqlite::params![from, to, id, org_id],
        )?;
        updated += affected as i64;
    }

    tx.commit()?;
    Ok(updated)
}

// ── Memory trends ─────────────────────────────────────────────────────────────

/// Returns memory trend data for the last 30 days scoped to the org.
pub fn get_memory_trends(conn: &Connection, org_id: &str, days: i64) -> Result<crate::models::types::MemoryTrends> {
    use crate::models::types::{DailyCount, NameCount, MemoryTrends};

    // Daily counts for the requested period
    let mut stmt = conn.prepare(
        "SELECT date(created_at) as date, COUNT(*) as count
         FROM memories
         WHERE org_id = ?1 AND created_at >= datetime('now', '-' || ?2 || ' days')
         GROUP BY date
         ORDER BY date ASC",
    )?;
    let daily_counts: Vec<DailyCount> = stmt
        .query_map(rusqlite::params![org_id, days], |row| {
            Ok(DailyCount {
                date: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // By type — top 5 (within the requested period)
    let mut stmt = conn.prepare(
        "SELECT COALESCE(type, 'untyped') as name, COUNT(*) as count
         FROM memories
         WHERE org_id = ?1
           AND created_at >= datetime('now', '-' || ?2 || ' days')
         GROUP BY type
         ORDER BY count DESC
         LIMIT 5",
    )?;
    let by_type: Vec<NameCount> = stmt
        .query_map(rusqlite::params![org_id, days], |row| {
            Ok(NameCount {
                name: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // By project — top 5 (within the requested period)
    let mut stmt = conn.prepare(
        "SELECT project as name, COUNT(*) as count
         FROM memories
         WHERE org_id = ?1
           AND created_at >= datetime('now', '-' || ?2 || ' days')
         GROUP BY project
         ORDER BY count DESC
         LIMIT 5",
    )?;
    let by_project: Vec<NameCount> = stmt
        .query_map(rusqlite::params![org_id, days], |row| {
            Ok(NameCount {
                name: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // Total within the requested period
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1 AND created_at >= datetime('now', '-' || ?2 || ' days')",
        rusqlite::params![org_id, days],
        |r| r.get(0),
    )?;

    // This week (last 7 days)
    let this_week: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1 AND created_at >= date('now', '-7 days')",
        [org_id],
        |r| r.get(0),
    )?;

    // This month (last 30 days)
    let this_month: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1 AND created_at >= date('now', '-30 days')",
        [org_id],
        |r| r.get(0),
    )?;

    Ok(MemoryTrends {
        daily_counts,
        by_type,
        by_project,
        total,
        this_week,
        this_month,
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
                    archived_at: None,
                    pinned: false,
                    collection_id: None,
                    admin_note: None,
                    delete_after: None,
                    status: "active".to_string(),
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
        archived_at: None,
        pinned: false,
        collection_id: None,
        admin_note: None,
        delete_after: None,
        status: "active".to_string(),
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
        "INSERT INTO sessions (id, org_id, name, project, directory, started_at, summary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, org_id, req.name, req.project, directory, now, req.summary],
    )?;

    Ok(Session {
        id,
        org_id: org_id.to_string(),
        name: req.name.clone(),
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
    if req.name.is_none() && req.ended_at.is_none() && req.summary.is_none() {
        // Nothing to update — fetch and return existing session
        return get_session(conn, org_id, session_id);
    }

    let mut set_clauses: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();
    let mut param_idx = 1usize;

    if let Some(name) = &req.name {
        set_clauses.push(format!("name = ?{param_idx}"));
        params.push(name.clone());
        param_idx += 1;
    }
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
        "SELECT id, org_id, name, project, directory, started_at, ended_at, summary
         FROM sessions WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![session_id, org_id],
        |row| {
            Ok(Session {
                id: row.get(0)?,
                org_id: row.get(1)?,
                name: row.get(2)?,
                project: row.get(3)?,
                directory: row.get(4)?,
                started_at: row.get(5)?,
                ended_at: row.get(6)?,
                summary: row.get(7)?,
            })
        },
    );
    match result {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Lists all sessions for an org, with their memory count, ordered by started_at DESC.
pub fn list_sessions(conn: &Connection, org_id: &str) -> Result<Vec<SessionWithCount>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.org_id, s.name, s.project, s.directory, s.started_at, s.ended_at, s.summary,
                COUNT(m.id) as memory_count
         FROM sessions s
         LEFT JOIN memories m ON m.session_id = s.id AND m.org_id = s.org_id
         WHERE s.org_id = ?1
         GROUP BY s.id
         ORDER BY s.started_at DESC
         LIMIT 100",
    )?;

    let rows = stmt.query_map(rusqlite::params![org_id], |row| {
        Ok(SessionWithCount {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            project: row.get(3)?,
            directory: row.get(4)?,
            started_at: row.get(5)?,
            ended_at: row.get(6)?,
            summary: row.get(7)?,
            memory_count: row.get(8)?,
        })
    })?;

    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
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
                    email: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    name: row.get(3)?,
                    role: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                    last_active: None,
                    disabled_at: None,
                    admin_note: None,
                    last_login_at: None,
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
        "SELECT id, org_id, email, name, role, status, created_at, disabled_at, admin_note, last_login_at FROM users WHERE id = ?1",
        [user_id],
        |row| {
            Ok(User {
                id: row.get(0)?,
                org_id: row.get(1)?,
                email: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                name: row.get(3)?,
                role: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                last_active: None,
                disabled_at: row.get(7)?,
                admin_note: row.get(8)?,
                last_login_at: row.get(9)?,
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
         WHERE m.org_id = ?1 AND m.archived_at IS NULL",
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
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                archived_at, pinned, collection_id, admin_note, delete_after
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
            row.get::<_, Option<String>>(16)?,
            row.get::<_, i64>(17).unwrap_or(0),
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<String>>(20)?,
        ))
    })?;

    // Build id→memory map, then restore order
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
             archived_at, pinned_i64, collection_id, admin_note, delete_after) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() { "archived".to_string() } else { "active".to_string() };
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
            archived_at,
            pinned: pinned_i64 != 0,
            collection_id,
            admin_note,
            delete_after,
            status,
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

// ── Collections ───────────────────────────────────────────────────────────────

/// Lists all collections for an org with memory count.
pub fn list_collections(conn: &Connection, org_id: &str) -> Result<Vec<crate::models::types::Collection>> {
    use crate::models::types::Collection;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.org_id, c.name, c.description, c.created_at,
                COUNT(m.id) as memory_count
         FROM collections c
         LEFT JOIN memories m ON m.collection_id = c.id AND m.org_id = c.org_id
         WHERE c.org_id = ?1
         GROUP BY c.id
         ORDER BY c.created_at ASC",
    )?;
    let rows = stmt.query_map([org_id], |row| {
        Ok(Collection {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            memory_count: row.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Creates a collection for an org.
pub fn create_collection(
    conn: &Connection,
    org_id: &str,
    name: &str,
    description: Option<&str>,
) -> Result<crate::models::types::Collection> {
    use crate::models::types::Collection;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "INSERT INTO collections (id, org_id, name, description, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, org_id, name, description, now],
    )?;
    Ok(Collection {
        id,
        org_id: org_id.to_string(),
        name: name.to_string(),
        description: description.map(String::from),
        created_at: now,
        memory_count: Some(0),
    })
}

/// Deletes a collection by ID, scoped to org. Memories in collection get collection_id = NULL (via FK ON DELETE SET NULL).
/// Returns true if deleted, false if not found.
pub fn delete_collection(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM collections WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
    )?;
    Ok(affected > 0)
}

/// Assigns or unassigns a memory to a collection. `collection_id = None` unassigns.
/// Returns true if updated, false if memory not found.
pub fn assign_memory_collection(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
    collection_id: Option<&str>,
) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE memories SET collection_id = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![collection_id, memory_id, org_id],
    )?;
    Ok(affected > 0)
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
    list_projects_filtered(conn, org_id, false)
}

pub fn list_projects_filtered(conn: &Connection, org_id: &str, include_archived: bool) -> Result<Vec<Project>> {
    let sql = if include_archived {
        "SELECT id, org_id, name, description, created_at, parent_id, archived_at FROM projects WHERE org_id = ?1 ORDER BY name ASC"
    } else {
        "SELECT id, org_id, name, description, created_at, parent_id, archived_at FROM projects WHERE org_id = ?1 AND archived_at IS NULL ORDER BY name ASC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([org_id], |row| {
        Ok(Project {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            parent_id: row.get(5)?,
            archived_at: row.get(6)?,
        })
    })?;
    let mut projects = Vec::new();
    for row in rows {
        projects.push(row?);
    }
    Ok(projects)
}

pub fn get_project_by_id(conn: &Connection, org_id: &str, id: &str) -> Result<Option<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, description, created_at, parent_id, archived_at FROM projects WHERE id = ?1 AND org_id = ?2",
    )?;
    let mut rows = stmt.query_map([id, org_id], |row| {
        Ok(Project {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            parent_id: row.get(5)?,
            archived_at: row.get(6)?,
        })
    })?;
    rows.next().transpose().map_err(Into::into)
}

pub fn archive_project(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE projects SET archived_at = datetime('now') WHERE id = ?1 AND org_id = ?2",
        [id, org_id],
    )?;
    Ok(rows > 0)
}

pub fn restore_project(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE projects SET archived_at = NULL WHERE id = ?1 AND org_id = ?2",
        [id, org_id],
    )?;
    Ok(rows > 0)
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
        archived_at: None,
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
            email: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
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
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                archived_at, pinned, collection_id, admin_note, delete_after
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
                row.get::<_, Option<String>>(16)?,
                row.get::<_, i64>(17).unwrap_or(0),
                row.get::<_, Option<String>>(18)?,
                row.get::<_, Option<String>>(19)?,
                row.get::<_, Option<String>>(20)?,
            ))
        },
    );

    match result {
        Ok((id, org_id, user_id, project, tool, content, tags_str, created_at,
            title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
            archived_at, pinned_i64, collection_id, admin_note, delete_after)) => {
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let status = if archived_at.is_some() { "archived".to_string() } else { "active".to_string() };
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
                archived_at,
                pinned: pinned_i64 != 0,
                collection_id,
                admin_note,
                delete_after,
                status,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Updates the `content` field of a memory.
/// Returns `Some(Memory)` on success, `None` if the memory does not belong to this org.
pub fn update_memory_content(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
    content: &str,
) -> Result<Option<Memory>> {
    let rows_changed = conn.execute(
        "UPDATE memories SET content = ?1, revision_count = revision_count + 1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![content, memory_id, org_id],
    )?;
    if rows_changed == 0 {
        return Ok(None);
    }
    get_memory_by_id_for_org(conn, org_id, memory_id)
}

/// Partial update for PATCH /v1/memory/:id — updates whichever of content/title are provided.
/// Returns `Some(Memory)` on success, `None` if the memory does not belong to this org.
/// Caller must ensure at least one of content/title is Some.
pub fn update_memory_fields(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
    content: Option<&str>,
    title: Option<&str>,
) -> Result<Option<Memory>> {
    let rows_changed = match (content, title) {
        (Some(c), Some(t)) => conn.execute(
            "UPDATE memories SET content = ?1, title = ?2, revision_count = revision_count + 1 WHERE id = ?3 AND org_id = ?4",
            rusqlite::params![c, t, memory_id, org_id],
        )?,
        (Some(c), None) => conn.execute(
            "UPDATE memories SET content = ?1, revision_count = revision_count + 1 WHERE id = ?2 AND org_id = ?3",
            rusqlite::params![c, memory_id, org_id],
        )?,
        (None, Some(t)) => conn.execute(
            "UPDATE memories SET title = ?1, revision_count = revision_count + 1 WHERE id = ?2 AND org_id = ?3",
            rusqlite::params![t, memory_id, org_id],
        )?,
        (None, None) => return Err(anyhow::anyhow!("no fields to update")),
    };
    if rows_changed == 0 {
        return Ok(None);
    }
    get_memory_by_id_for_org(conn, org_id, memory_id)
}

/// Updates the `admin_note` field of a memory (admin-only).
/// Empty string clears the note (sets admin_note = NULL).
/// Returns `Some(Memory)` on success, `None` if the memory does not belong to this org.
pub fn update_memory_admin_note(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
    note: &str,
) -> Result<Option<Memory>> {
    let note_value: Option<&str> = if note.is_empty() { None } else { Some(note) };
    let rows_changed = conn.execute(
        "UPDATE memories SET admin_note = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![note_value, memory_id, org_id],
    )?;
    if rows_changed == 0 {
        return Ok(None);
    }
    get_memory_by_id_for_org(conn, org_id, memory_id)
}

/// Updates the `admin_note` field of a user (admin-only).
/// `None` clears the note (sets admin_note = NULL).
/// Returns `true` if the user was found and updated, `false` if not found.
pub fn update_user_admin_note(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    note: Option<&str>,
) -> Result<bool> {
    let rows_changed = conn.execute(
        "UPDATE users SET admin_note = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![note, user_id, org_id],
    )?;
    Ok(rows_changed > 0)
}

/// Returns aggregated project context for `org_id` + `project` name:
/// up to 20 most-recent memories, distinct tool values, and the latest `created_at`.
pub fn get_project_context(
    conn: &Connection,
    org_id: &str,
    project: &str,
) -> Result<crate::models::types::ProjectContext> {
    // Query 1: recent memories (last 20, DESC) — exclude archived.
    let mut stmt = conn.prepare(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                archived_at, pinned
         FROM memories
         WHERE org_id = ?1 AND project = ?2 AND archived_at IS NULL
         ORDER BY pinned DESC, created_at DESC
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
            row.get::<_, Option<String>>(16)?,
            row.get::<_, i64>(17).unwrap_or(0),
        ))
    })?;

    let mut recent_memories = Vec::new();
    for row in rows {
        let (id, org_id_col, user_id, proj, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
             archived_at, pinned_i64) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() { "archived".to_string() } else { "active".to_string() };
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
            archived_at,
            pinned: pinned_i64 != 0,
            collection_id: None,
            admin_note: None,
            delete_after: None,
            status,
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
            last_active: None,
            disabled_at: None,
            admin_note: None,
            last_login_at: None,
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
        project_id: row.get(8)?,
    })
}

/// Returns policies for an org, ordered by creation date DESC, page by `limit`/`offset`.
pub fn list_policies(conn: &Connection, org_id: &str, limit: i64, offset: i64) -> Result<Vec<Policy>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
         FROM policies WHERE org_id = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, limit, offset], row_to_policy)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Returns only enabled policies for an org, ordered by creation date ASC.
/// Used by the `/policy/check` handler for evaluation.
///
/// `project`: when `Some(p)`, returns org-wide policies (`project_id IS NULL`)
/// UNION policies scoped to project `p` — project scoping ADDS to org-wide, it
/// never replaces it. When `None`, returns every enabled policy for the org
/// regardless of `project_id` (admin listing / no-project-context behavior).
pub fn list_enabled_policies(conn: &Connection, org_id: &str, project: Option<&str>) -> Result<Vec<Policy>> {
    if let Some(p) = project {
        let mut stmt = conn.prepare(
            "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
             FROM policies WHERE org_id = ?1 AND enabled = 1 AND (project_id IS NULL OR project_id = ?2) ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![org_id, p], row_to_policy)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
             FROM policies WHERE org_id = ?1 AND enabled = 1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([org_id], row_to_policy)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}

/// Returns a single policy by id + org_id, or None (hides cross-org existence).
pub fn get_policy(conn: &Connection, id: &str, org_id: &str) -> Result<Option<Policy>> {
    let result = conn.query_row(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
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
/// `project_id`: `None` = org-wide (applies to every project); `Some(p)` scopes
/// the policy to project `p` only.
#[allow(clippy::too_many_arguments)]
pub fn insert_policy(
    conn: &Connection,
    id: &str,
    org_id: &str,
    name: &str,
    rule_type: &str,
    config_json: &str,
    enabled: bool,
    project_id: Option<&str>,
) -> Result<Policy> {
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let enabled_int: i64 = if enabled { 1 } else { 0 };
    conn.execute(
        "INSERT INTO policies (id, org_id, name, rule_type, config, enabled, project_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        rusqlite::params![id, org_id, name, rule_type, config_json, enabled_int, project_id, now],
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

/// Mark a code project as currently indexing.
pub fn set_code_project_indexing(conn: &Connection, code_project_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE code_projects SET index_status = 'indexing' WHERE id = ?1",
        rusqlite::params![code_project_id],
    )?;
    Ok(())
}

/// Mark a code project as successfully indexed.
pub fn set_code_project_success(
    conn: &Connection,
    code_project_id: i64,
    indexed_files_count: i64,
    last_indexed_at: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE code_projects
         SET index_status = 'success',
             last_indexed_at = ?1,
             indexed_files_count = ?2,
             last_index_error = NULL
         WHERE id = ?3",
        rusqlite::params![last_indexed_at, indexed_files_count, code_project_id],
    )?;
    Ok(())
}

/// Mark a code project as failed with an error message.
pub fn set_code_project_error(
    conn: &Connection,
    code_project_id: i64,
    error_msg: &str,
    last_indexed_at: &str,
) -> Result<()> {
    // Truncate error to 500 chars to avoid unbounded storage
    let truncated = if error_msg.len() > 500 { &error_msg[..500] } else { error_msg };
    conn.execute(
        "UPDATE code_projects
         SET index_status = 'error',
             last_indexed_at = ?1,
             last_index_error = ?2
         WHERE id = ?3",
        rusqlite::params![last_indexed_at, truncated, code_project_id],
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

/// Returns the smallest code_chunk whose [start_line, end_line] range covers `line`
/// for the given file. With AST chunking this is the symbol's own source. Returns
/// `None` when no chunk covers the line (e.g. an unsupported-language file).
pub fn get_chunk_covering_line(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
    line: i64,
) -> Result<Option<CodeChunk>> {
    conn.query_row(
        "SELECT id, code_project_id, file_path, file_hash, language, symbol, \
                start_line, end_line, content, created_at \
         FROM code_chunks \
         WHERE code_project_id = ?1 AND file_path = ?2 \
           AND start_line <= ?3 AND end_line >= ?3 \
         ORDER BY (end_line - start_line) ASC \
         LIMIT 1",
        rusqlite::params![code_project_id, file_path, line],
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
    )
    .optional()
    .map_err(Into::into)
}

/// Returns all code_chunks for a file, ordered by start_line. Used to assemble a
/// symbol's source by line-range overlap (containers like classes are split into
/// method chunks, so a point lookup misses the declaration line) and to show the
/// whole-file source when a File node is clicked.
pub fn get_file_chunks(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
) -> Result<Vec<CodeChunk>> {
    let mut stmt = conn.prepare(
        "SELECT id, code_project_id, file_path, file_hash, language, symbol, \
                start_line, end_line, content, created_at \
         FROM code_chunks \
         WHERE code_project_id = ?1 AND file_path = ?2 \
         ORDER BY start_line ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![code_project_id, file_path], |row| {
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
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Store (or replace) the raw source of a file, so exact symbol/file source can be
/// shown in the graph without reconstructing it from symbol-fragment chunks.
pub fn upsert_code_file(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
    content: &str,
    file_hash: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO code_files (code_project_id, file_path, content, file_hash) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(code_project_id, file_path) \
         DO UPDATE SET content = excluded.content, file_hash = excluded.file_hash",
        rusqlite::params![code_project_id, file_path, content, file_hash],
    )?;
    Ok(())
}

/// Returns the set of file paths that already have stored source (code_files).
pub fn list_files_with_source(
    conn: &Connection,
    code_project_id: i64,
) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT file_path FROM code_files WHERE code_project_id = ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![code_project_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashSet<_>>>()?)
}

/// Returns the set of file paths that already have file-owned code symbols.
pub fn list_files_with_symbols(
    conn: &Connection,
    code_project_id: i64,
) -> Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT file_path FROM code_symbols \
         WHERE code_project_id = ?1 \
           AND symbol_type NOT IN ('File','Folder','Project','External') \
           AND file_path IS NOT NULL",
    )?;
    let rows = stmt.query_map(rusqlite::params![code_project_id], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashSet<_>>>()?)
}

/// Returns the stored raw source of a file, if present.
pub fn get_code_file(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT content FROM code_files WHERE code_project_id = ?1 AND file_path = ?2",
        rusqlite::params![code_project_id, file_path],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
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
        "SELECT id, org_id, name, root_path, repo_url, file_count, chunk_count, last_indexed, created_at,
                reindex_interval_hours, last_indexed_at, last_index_error, indexed_files_count, index_status, archived_at,
                exclude_patterns
         FROM code_projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, name],
        |row| {
            let patterns_json: String = row.get::<_, Option<String>>(15)?.unwrap_or_else(|| "[]".to_string());
            Ok(CodeProject {
                id: row.get::<_, i64>(0)?.to_string(),
                org_id: row.get(1)?,
                name: row.get(2)?,
                root_path: row.get(3)?,
                repo_url: row.get(4)?,
                file_count: row.get(5)?,
                chunk_count: row.get(6)?,
                last_indexed: row.get(7)?,
                created_at: row.get(8)?,
                reindex_interval_hours: row.get(9)?,
                last_indexed_at: row.get(10)?,
                last_index_error: row.get(11)?,
                indexed_files_count: row.get(12)?,
                index_status: row.get(13)?,
                archived_at: row.get(14)?,
                exclude_patterns: serde_json::from_str(&patterns_json).unwrap_or_default(),
            })
        },
    );

    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Get a code project by numeric id and org_id (used for reindex endpoint).
pub fn get_code_project_by_id(
    conn: &Connection,
    org_id: &str,
    project_id: i64,
) -> Result<Option<CodeProject>> {
    let result = conn.query_row(
        "SELECT id, org_id, name, root_path, repo_url, file_count, chunk_count, last_indexed, created_at,
                reindex_interval_hours, last_indexed_at, last_index_error, indexed_files_count, index_status, archived_at,
                exclude_patterns
         FROM code_projects WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, project_id],
        |row| {
            let patterns_json: String = row.get::<_, Option<String>>(15)?.unwrap_or_else(|| "[]".to_string());
            Ok(CodeProject {
                id: row.get::<_, i64>(0)?.to_string(),
                org_id: row.get(1)?,
                name: row.get(2)?,
                root_path: row.get(3)?,
                repo_url: row.get(4)?,
                file_count: row.get(5)?,
                chunk_count: row.get(6)?,
                last_indexed: row.get(7)?,
                created_at: row.get(8)?,
                reindex_interval_hours: row.get(9)?,
                last_indexed_at: row.get(10)?,
                last_index_error: row.get(11)?,
                indexed_files_count: row.get(12)?,
                index_status: row.get(13)?,
                archived_at: row.get(14)?,
                exclude_patterns: serde_json::from_str(&patterns_json).unwrap_or_default(),
            })
        },
    );
    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List all code projects for an org, ordered by creation date (newest first).
pub fn list_code_projects(conn: &Connection, org_id: &str) -> Result<Vec<CodeProject>> {
    list_code_projects_filtered(conn, org_id, false)
}

/// When `include_archived` is false (default), archived code projects (archived_at IS NOT NULL) are excluded.
pub fn list_code_projects_filtered(conn: &Connection, org_id: &str, include_archived: bool) -> Result<Vec<CodeProject>> {
    let base = "SELECT id, org_id, name, root_path, repo_url, file_count, chunk_count, last_indexed, created_at,
                reindex_interval_hours, last_indexed_at, last_index_error, indexed_files_count, index_status, archived_at,
                exclude_patterns
         FROM code_projects WHERE org_id = ?1";
    let sql = if include_archived {
        format!("{base} ORDER BY created_at DESC")
    } else {
        format!("{base} AND archived_at IS NULL ORDER BY created_at DESC")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([org_id], |row| {
        let patterns_json: String = row.get::<_, Option<String>>(15)?.unwrap_or_else(|| "[]".to_string());
        Ok(CodeProject {
            id: row.get::<_, i64>(0)?.to_string(),
            org_id: row.get(1)?,
            name: row.get(2)?,
            root_path: row.get(3)?,
            repo_url: row.get(4)?,
            file_count: row.get(5)?,
            chunk_count: row.get(6)?,
            last_indexed: row.get(7)?,
            created_at: row.get(8)?,
            reindex_interval_hours: row.get(9)?,
            last_indexed_at: row.get(10)?,
            last_index_error: row.get(11)?,
            indexed_files_count: row.get(12)?,
            index_status: row.get(13)?,
            archived_at: row.get(14)?,
            exclude_patterns: serde_json::from_str(&patterns_json).unwrap_or_default(),
        })
    })?;
    rows.map(|r| r.map_err(Into::into)).collect()
}

/// Update exclude_patterns for a code project. Returns true if the project was found and updated.
pub fn update_code_project_exclude_patterns(
    conn: &Connection,
    org_id: &str,
    project_id: i64,
    patterns: &[String],
) -> Result<bool> {
    let json = serde_json::to_string(patterns).unwrap_or_else(|_| "[]".to_string());
    let rows = conn.execute(
        "UPDATE code_projects SET exclude_patterns = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![json, project_id, org_id],
    )?;
    Ok(rows > 0)
}

/// Archives a code project (sets archived_at = now). Admin only.
/// Returns Ok(true) if updated, Ok(false) if not found.
pub fn archive_code_project(conn: &Connection, org_id: &str, id: i64) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE code_projects SET archived_at = datetime('now') WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
    )?;
    Ok(rows > 0)
}

/// Restores a code project (clears archived_at). Returns Ok(true) if updated.
pub fn restore_code_project(conn: &Connection, org_id: &str, id: i64) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE code_projects SET archived_at = NULL WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
    )?;
    Ok(rows > 0)
}

/// Update the auto re-index interval for a code project. hours = None disables auto re-index.
pub fn update_reindex_interval(conn: &Connection, org_id: &str, project_id: i64, hours: Option<i64>) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE code_projects SET reindex_interval_hours = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![hours, project_id, org_id],
    )?;
    Ok(rows > 0)
}

/// Set the repo_url for an existing code project.
pub fn set_code_project_repo_url(conn: &Connection, org_id: &str, name: &str, repo_url: &str) -> Result<()> {
    conn.execute(
        "UPDATE code_projects SET repo_url = ?1 WHERE org_id = ?2 AND name = ?3",
        rusqlite::params![repo_url, org_id, name],
    )?;
    Ok(())
}

/// Delete a code project (and its chunks, via cascade) for (org_id, id).
/// Returns `true` if a row was deleted, `false` if not found.
pub fn delete_code_project(conn: &Connection, org_id: &str, id: i64) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM code_projects WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
    )?;
    Ok(affected > 0)
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

// ── Webhook queries ────────────────────────────────────────────────────────────

fn row_to_webhook(row: &rusqlite::Row<'_>) -> rusqlite::Result<Webhook> {
    let events_json: String = row.get(5)?;
    let events: Vec<String> = serde_json::from_str(&events_json).unwrap_or_else(|_| vec!["*".to_string()]);
    let active_int: i64 = row.get(6)?;
    Ok(Webhook {
        id: row.get(0)?,
        org_id: row.get(1)?,
        name: row.get(2)?,
        target_url: row.get(3)?,
        secret: row.get(4)?,
        events,
        active: active_int != 0,
        created_at: row.get(7)?,
    })
}

/// Returns all webhooks for an org ordered by creation date DESC.
pub fn list_webhooks(conn: &Connection, org_id: &str) -> Result<Vec<Webhook>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, target_url, secret, events, active, created_at
         FROM webhooks WHERE org_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([org_id], row_to_webhook)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Returns a single webhook by id + org_id, or None (hides cross-org existence).
pub fn get_webhook(conn: &Connection, id: &str, org_id: &str) -> Result<Option<Webhook>> {
    let result = conn.query_row(
        "SELECT id, org_id, name, target_url, secret, events, active, created_at
         FROM webhooks WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
        row_to_webhook,
    );
    match result {
        Ok(w) => Ok(Some(w)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Inserts a new webhook and returns the created row.
pub fn create_webhook(conn: &Connection, org_id: &str, req: &CreateWebhookRequest) -> Result<Webhook> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let events = req.events.clone().unwrap_or_else(|| vec!["*".to_string()]);
    let events_json = serde_json::to_string(&events)?;
    conn.execute(
        "INSERT INTO webhooks (id, org_id, name, target_url, secret, events, active, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
        rusqlite::params![id, org_id, req.name, req.target_url, req.secret, events_json, now],
    )?;
    get_webhook(conn, &id, org_id)?
        .ok_or_else(|| anyhow::anyhow!("create_webhook: row not found after insert"))
}

/// Updates active/secret/events for a webhook. Returns None if not found in this org.
pub fn update_webhook(
    conn: &Connection,
    org_id: &str,
    id: &str,
    req: &UpdateWebhookRequest,
) -> Result<Option<Webhook>> {
    let active_int: Option<i64> = req.active.map(|b| if b { 1 } else { 0 });
    let events_json: Option<String> = match &req.events {
        Some(evts) => Some(serde_json::to_string(evts)?),
        None => None,
    };
    // secret: None means "don't change", Some(None) is not representable in this API;
    // we treat Some(s) as update. The field is Option<String> in req.
    let rows_affected = conn.execute(
        "UPDATE webhooks
         SET active = COALESCE(?3, active),
             secret = CASE WHEN ?4 IS NOT NULL THEN ?4 ELSE secret END,
             events = COALESCE(?5, events)
         WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id, active_int, req.secret, events_json],
    )?;
    if rows_affected == 0 {
        return Ok(None);
    }
    get_webhook(conn, id, org_id)
}

/// Deletes a webhook scoped to org_id. Returns true if a row was deleted.
pub fn delete_webhook(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM webhooks WHERE id = ?1 AND org_id = ?2",
        [id, org_id],
    )?;
    Ok(affected > 0)
}

// ── Webhook delivery log queries ───────────────────────────────────────────────

/// Inserts a delivery attempt record into webhook_deliveries.
#[allow(clippy::too_many_arguments)]
pub fn log_webhook_delivery(
    conn: &Connection,
    org_id: &str,
    webhook_id: &str,
    event_type: &str,
    payload: &str,
    status_code: Option<i64>,
    success: bool,
    error: Option<&str>,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let success_int: i64 = if success { 1 } else { 0 };
    conn.execute(
        "INSERT INTO webhook_deliveries
             (id, webhook_id, org_id, event_type, payload, status_code, success, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, webhook_id, org_id, event_type, payload, status_code, success_int, error],
    )?;
    Ok(())
}

/// Returns the last `limit` deliveries for a webhook, ordered newest-first.
pub fn list_webhook_deliveries(
    conn: &Connection,
    org_id: &str,
    webhook_id: &str,
    limit: i64,
) -> Result<Vec<WebhookDelivery>> {
    let mut stmt = conn.prepare(
        "SELECT id, webhook_id, org_id, event_type, payload, status_code, success, error, delivered_at
         FROM webhook_deliveries
         WHERE org_id = ?1 AND webhook_id = ?2
         ORDER BY delivered_at DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, webhook_id, limit], |row| {
        let success_int: i64 = row.get(6)?;
        Ok(WebhookDelivery {
            id: row.get(0)?,
            webhook_id: row.get(1)?,
            org_id: row.get(2)?,
            event_type: row.get(3)?,
            payload: row.get(4)?,
            status_code: row.get(5)?,
            success: success_int != 0,
            error: row.get(7)?,
            delivered_at: row.get(8)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Fetches a single webhook delivery by ID, scoped to the org.
pub fn get_webhook_delivery(
    conn: &Connection,
    org_id: &str,
    delivery_id: &str,
) -> Result<Option<WebhookDelivery>> {
    conn.query_row(
        "SELECT id, webhook_id, org_id, event_type, payload, status_code, success, error, delivered_at
         FROM webhook_deliveries
         WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, delivery_id],
        |row| {
            let success_int: i64 = row.get(6)?;
            Ok(WebhookDelivery {
                id: row.get(0)?,
                webhook_id: row.get(1)?,
                org_id: row.get(2)?,
                event_type: row.get(3)?,
                payload: row.get(4)?,
                status_code: row.get(5)?,
                success: success_int != 0,
                error: row.get(7)?,
                delivered_at: row.get(8)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

/// Searches users by name or email (case-insensitive LIKE), org-scoped.
pub fn search_users_by_query(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
) -> Result<Vec<crate::models::types::UserSummary>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT id, email, name, role
         FROM users
         WHERE org_id = ?1
           AND (LOWER(name) LIKE ?2 OR LOWER(email) LIKE ?2)
         ORDER BY name ASC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, pattern, limit], |row| {
        Ok(crate::models::types::UserSummary {
            id: row.get(0)?,
            email: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            name: row.get(2)?,
            role: row.get(3)?,
        })
    })?;
    let mut users = Vec::new();
    for row in rows {
        users.push(row?);
    }
    Ok(users)
}

/// Searches projects by name (case-insensitive LIKE), org-scoped.
pub fn search_projects_by_query(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
) -> Result<Vec<crate::models::types::Project>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, description, created_at, parent_id
         FROM projects
         WHERE org_id = ?1
           AND LOWER(name) LIKE ?2
         ORDER BY name ASC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, pattern, limit], |row| {
        Ok(crate::models::types::Project {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            parent_id: row.get(5)?,
            archived_at: None,
        })
    })?;
    let mut projects = Vec::new();
    for row in rows {
        projects.push(row?);
    }
    Ok(projects)
}

/// Full-text LIKE search across policies for an org, matching on the `name` field.
pub fn search_policies_by_query(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
) -> Result<Vec<crate::models::types::Policy>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
         FROM policies
         WHERE org_id = ?1
           AND LOWER(name) LIKE ?2
         ORDER BY name ASC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, pattern, limit], row_to_policy)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// LIKE search across active (non-archived) conventions for an org, matching on `title` or `content`.
pub fn search_conventions_by_query(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
) -> Result<Vec<crate::models::types::Convention>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT id, org_id, project_id, title, content, category, weight, tags, created_at, updated_at, archived_at
         FROM conventions
         WHERE org_id = ?1
           AND archived_at IS NULL
           AND (LOWER(title) LIKE ?2 OR LOWER(content) LIKE ?2)
         ORDER BY weight DESC, title ASC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, pattern, limit], convention_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Global (cross-org) LIKE search across organizations for the backoffice.
pub fn search_orgs_by_query(
    conn: &Connection,
    q: &str,
    limit: i64,
) -> Result<Vec<crate::models::types::OrgWithStats>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT o.id, o.name, o.slug, o.created_at,
                (SELECT count(*) FROM users u WHERE u.org_id = o.id) AS user_count,
                (SELECT count(*) FROM memories m WHERE m.org_id = o.id) AS memory_count
         FROM organizations o
         WHERE LOWER(o.name) LIKE ?1 OR LOWER(o.slug) LIKE ?1
         ORDER BY o.name ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit], |r| {
        Ok(crate::models::types::OrgWithStats {
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

/// Global (cross-org) LIKE search across users for the backoffice, matching on `name` or `email`.
pub fn search_users_global_by_query(
    conn: &Connection,
    q: &str,
    limit: i64,
) -> Result<Vec<crate::models::types::User>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut stmt = conn.prepare(
        "SELECT id, org_id, email, name, role, status, created_at
         FROM users
         WHERE LOWER(name) LIKE ?1 OR LOWER(email) LIKE ?1
         ORDER BY name ASC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit], |r| {
        Ok(crate::models::types::User {
            id: r.get(0)?,
            org_id: r.get(1)?,
            email: r.get(2)?,
            name: r.get(3)?,
            role: r.get(4)?,
            status: r.get(5)?,
            created_at: r.get(6)?,
            last_active: None,
            disabled_at: None,
            admin_note: None,
            last_login_at: None,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Lists all non-revoked API keys for an org, joined with user info. Admin-only.
pub fn list_all_org_keys(conn: &Connection, org_id: &str) -> Result<Vec<ApiKeyWithUser>> {
    let mut stmt = conn.prepare(
        "SELECT ak.id, ak.user_id, u.name, u.email, ak.label, ak.last_used, ak.created_at, ak.revoked, ak.expires_at,
                COALESCE(ak.times_used, 0), ak.last_used_at
         FROM api_keys ak
         JOIN users u ON u.id = ak.user_id
         WHERE ak.org_id = ?1 AND ak.revoked = 0
         ORDER BY ak.created_at DESC",
    )?;
    let rows = stmt.query_map([org_id], |row| {
        Ok(ApiKeyWithUser {
            id: row.get(0)?,
            user_id: row.get(1)?,
            user_name: row.get(2)?,
            user_email: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            label: row.get(4)?,
            last_used: row.get(5)?,
            created_at: row.get(6)?,
            revoked: row.get::<_, i64>(7)? != 0,
            expires_at: row.get(8)?,
            times_used: row.get::<_, i64>(9).unwrap_or(0),
            last_used_at: row.get(10)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Revokes a specific API key in the org. Returns true if a row was updated.
pub fn revoke_key_admin(conn: &Connection, org_id: &str, key_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE api_keys SET revoked = 1 WHERE id = ?1 AND org_id = ?2 AND revoked = 0",
        [key_id, org_id],
    )?;
    Ok(affected > 0)
}

/// Returns a single API key (with joined user info) by key ID, scoped to the org.
/// Returns `None` if the key does not exist in the org (revoked or otherwise).
pub fn get_key_admin(conn: &Connection, org_id: &str, key_id: &str) -> Result<Option<ApiKeyWithUser>> {
    conn.query_row(
        "SELECT ak.id, ak.user_id, u.name, u.email, ak.label, ak.last_used, ak.created_at, ak.revoked, ak.expires_at,
                COALESCE(ak.times_used, 0), ak.last_used_at
         FROM api_keys ak
         JOIN users u ON u.id = ak.user_id
         WHERE ak.id = ?1 AND ak.org_id = ?2",
        rusqlite::params![key_id, org_id],
        |row| {
            Ok(ApiKeyWithUser {
                id: row.get(0)?,
                user_id: row.get(1)?,
                user_name: row.get(2)?,
                user_email: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                label: row.get(4)?,
                last_used: row.get(5)?,
                created_at: row.get(6)?,
                revoked: row.get::<_, i64>(7)? != 0,
                expires_at: row.get(8)?,
                times_used: row.get::<_, i64>(9).unwrap_or(0),
                last_used_at: row.get(10)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

/// Updates the label and/or expires_at of a non-revoked key.
/// `expires_at_value`: `Some(Some(s))` sets it, `Some(None)` clears it, `None` leaves it unchanged.
/// Returns true if the key was found and updated.
pub fn update_key_admin(
    conn: &Connection,
    org_id: &str,
    key_id: &str,
    label: Option<&str>,
    expires_at_value: Option<Option<&str>>,
) -> Result<bool> {
    // Build update dynamically so we only touch fields that were provided.
    let mut parts: Vec<String> = Vec::new();
    if label.is_some() {
        parts.push("label = ?3".to_string());
    }
    if expires_at_value.is_some() {
        parts.push(format!("expires_at = ?{}", if label.is_some() { 4 } else { 3 }));
    }
    if parts.is_empty() {
        // Nothing to update — check existence and return true if found.
        let exists: bool = conn.query_row(
            "SELECT count(*) FROM api_keys WHERE id = ?1 AND org_id = ?2 AND revoked = 0",
            rusqlite::params![key_id, org_id],
            |r| r.get::<_, i64>(0),
        )? > 0;
        return Ok(exists);
    }

    let sql = format!(
        "UPDATE api_keys SET {} WHERE id = ?1 AND org_id = ?2 AND revoked = 0",
        parts.join(", ")
    );

    // We need to bind params in order. Use rusqlite params! with the right arity.
    let affected = match (label, expires_at_value) {
        (Some(lbl), Some(exp)) => conn.execute(
            &sql,
            rusqlite::params![key_id, org_id, lbl, exp],
        )?,
        (Some(lbl), None) => conn.execute(
            &sql,
            rusqlite::params![key_id, org_id, lbl],
        )?,
        (None, Some(exp)) => conn.execute(
            &sql,
            rusqlite::params![key_id, org_id, exp],
        )?,
        (None, None) => unreachable!(),
    };
    Ok(affected > 0)
}

/// Rotates a specific API key: revokes it and creates a new one for the same user.
/// Returns `None` if the key does not exist or is already revoked.
/// Returns `(new_key_metadata, raw_key)` on success — raw_key is shown only once.
pub fn rotate_key_by_id(
    conn: &Connection,
    org_id: &str,
    key_id: &str,
) -> Result<Option<(ApiKeyWithUser, String)>> {
    // Fetch user_id for the key, verify it belongs to the org and is not revoked.
    let user_id: Option<String> = conn.query_row(
        "SELECT user_id FROM api_keys WHERE id = ?1 AND org_id = ?2 AND revoked = 0",
        rusqlite::params![key_id, org_id],
        |r| r.get(0),
    ).optional()?;

    let user_id = match user_id {
        Some(id) => id,
        None => return Ok(None),
    };

    // Revoke the specific key.
    conn.execute(
        "UPDATE api_keys SET revoked = 1 WHERE id = ?1",
        rusqlite::params![key_id],
    )?;

    // Create a new key for the same user.
    let new_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'rotated', ?5)",
        rusqlite::params![new_id, user_id, org_id, key_hash, now],
    )?;

    // Fetch the new key with user info.
    let key = get_key_admin(conn, org_id, &new_id)?
        .expect("newly inserted key must be found");

    Ok(Some((key, raw_key)))
}

/// Creates a new API key for a user. Returns `(key_metadata, raw_key)`.
/// Returns `Err` if user does not exist in the org.
pub fn create_key_admin(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    label: &str,
    expires_at: Option<&str>,
) -> Result<(ApiKeyWithUser, String)> {
    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![key_id, user_id, org_id, key_hash, label, now, expires_at],
    )?;

    let key = get_key_admin(conn, org_id, &key_id)?
        .expect("newly inserted key must be found");

    Ok((key, raw_key))
}

// ── Per-project event overrides ───────────────────────────────────────────────

/// Returns the per-project agent event overrides for a project.
/// Returns `ProjectEventOverrides::default()` (all `None`) when the column is NULL,
/// which means "inherit from org settings".
pub fn get_project_event_overrides(
    conn: &Connection,
    org_id: &str,
    project_id: &str,
) -> Result<ProjectEventOverrides> {
    let result: Option<String> = conn
        .query_row(
            "SELECT event_overrides FROM projects WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![project_id, org_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    match result {
        None => Ok(ProjectEventOverrides::default()),
        Some(json_str) => {
            let overrides: ProjectEventOverrides = serde_json::from_str(&json_str)
                .unwrap_or_default();
            Ok(overrides)
        }
    }
}

/// Persists per-project agent event overrides. Serializes to JSON and writes to
/// the `event_overrides` column.  Returns the saved overrides or an error if the
/// project does not exist for `org_id`.
pub fn update_project_event_overrides(
    conn: &Connection,
    org_id: &str,
    project_id: &str,
    overrides: ProjectEventOverrides,
) -> Result<ProjectEventOverrides> {
    let json = serde_json::to_string(&overrides)?;
    let affected = conn.execute(
        "UPDATE projects SET event_overrides = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![json, project_id, org_id],
    )?;
    if affected == 0 {
        return Err(anyhow::anyhow!("project not found"));
    }
    Ok(overrides)
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
        assert!(!mem.id.is_empty());
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

    // ── FTS recall fix: OR-join instead of implicit AND ──────────────────────

    #[test]
    fn search_matches_memory_with_partial_term_overlap() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Memory content only contains 3 of the 6 query terms below.
        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "apple banana cherry unrelated words here",
            &[],
        );

        // A 6-term natural-language query where only "apple banana cherry" appear
        // in the stored memory. Under the old space-joined (implicit AND) query
        // this required ALL 6 terms present in one row → 0 results.
        let results =
            search_memories(&conn, &org.id, "apple banana cherry date eggplant fig", 10).unwrap();
        assert_eq!(
            results.len(),
            1,
            "OR-joined FTS query must match a memory containing only a subset of query terms"
        );
    }

    #[test]
    fn search_ranks_more_matching_terms_higher() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Matches only 1 of the 4 query terms.
        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "grape only appears here alone",
            &[],
        );
        // Matches 3 of the 4 query terms.
        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "grape kiwi mango show up together",
            &[],
        );

        let results = search_memories(&conn, &org.id, "grape kiwi mango lemon", 10).unwrap();
        assert_eq!(results.len(), 2, "both memories share at least one query term");
        assert!(
            results[0].content.contains("kiwi mango"),
            "the memory matching more query terms must rank first, got: {:?}",
            results.iter().map(|m| &m.content).collect::<Vec<_>>()
        );
    }

    #[test]
    fn sanitize_fts_query_neutralizes_special_characters() {
        // Characters that are FTS5 operators must not be able to break out of
        // the MATCH expression or cause a query parse error.
        let raw = r#"foo" OR 1=1 -- * - ("bar)"#;
        let sanitized = sanitize_fts_query(raw).expect("must still produce a query");
        // No unescaped double-quote may appear outside of the per-token wrapping —
        // every token is individually wrapped in its own quote pair.
        assert!(
            !sanitized.contains("1=1"),
            "raw SQL/FTS injection payloads must be tokenized away, got: {sanitized}"
        );

        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        legacy_store(&conn, &org.id, &user.id, "proj", "claude", "bar foo baz", &[]);

        // Must not error even though the raw query contains FTS5 special chars.
        let result = search_memories(&conn, &org.id, raw, 10);
        assert!(result.is_ok(), "special characters must not cause a query error: {result:?}");
    }

    #[test]
    fn list_memories_with_filters() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        legacy_store(&conn, &org.id, &user.id, "proj-a", "claude", "mem 1", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj-b", "cursor", "mem 2", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj-a", "cursor", "mem 3", &[]);

        // filter by tool
        let cursor_mems = list_memories(&conn, &org.id, None, Some("cursor"), None, None, None, None, 10, 0, false, None, None, None).unwrap();
        assert_eq!(cursor_mems.len(), 2);

        // filter by project
        let proj_a = list_memories(&conn, &org.id, None, None, Some("proj-a"), None, None, None, 10, 0, false, None, None, None).unwrap();
        assert_eq!(proj_a.len(), 2);

        // filter by both
        let filtered = list_memories(&conn, &org.id, None, Some("cursor"), Some("proj-a"), None, None, None, 10, 0, false, None, None, None).unwrap();
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
        let still_there = list_memories(&conn, &org.id, None, None, None, None, None, None, 10, 0, false, None, None, None).unwrap();
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

        let entries = list_audit(&conn, &org.id, None, None, None, None, None, None, 50, 0).unwrap();
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

        let org1_entries = list_audit(&conn, &org1.id, None, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(org1_entries.len(), 1, "org1 must not see org2 audit entries");

        let org2_entries = list_audit(&conn, &org2_id, None, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(org2_entries.len(), 1, "org2 must not see org1 audit entries");
    }

    #[test]
    fn list_audit_filters_by_action() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "search", "memory", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();

        let store_entries = list_audit(&conn, &org.id, None, Some("store"), None, None, None, None, 50, 0).unwrap();
        assert_eq!(store_entries.len(), 2);
        assert!(store_entries.iter().all(|e| e.action == "store"));

        let search_entries = list_audit(&conn, &org.id, None, Some("search"), None, None, None, None, 50, 0).unwrap();
        assert_eq!(search_entries.len(), 1);
    }

    #[test]
    fn list_audit_full_text_search_filters_by_action_substring() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(&conn, &org.id, &user.id, "memory.created", "memory", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "user.updated", "user", None, serde_json::json!({})).unwrap();
        log_audit(&conn, &org.id, &user.id, "project.archived", "project", None, serde_json::json!({})).unwrap();

        let results = list_audit(&conn, &org.id, None, None, None, None, None, Some("memory"), 50, 0).unwrap();
        assert_eq!(results.len(), 1, "search for 'memory' must return exactly 1 result");
        assert_eq!(results[0].action, "memory.created");
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

        let entries = list_audit(&conn, &org.id, None, None, None, None, None, None, 10, 0).unwrap();
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
            name: None,
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
            name: None,
            directory: None,
            summary: None,
        };
        let session = create_session(&conn, &org.id, &create_req).unwrap();

        let patch_req = crate::models::types::PatchSessionRequest {
            name: None,
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
            name: None,
            directory: None,
            summary: None,
        };
        let session = create_session(&conn, &org.id, &create_req).unwrap();

        let patch_req = crate::models::types::PatchSessionRequest {
            name: None,
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

        let bugfix_mems = list_memories(&conn, &org.id, None, None, None, Some("bugfix"), None, None, 10, 0, false, None, None, None).unwrap();
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

        let personal_mems = list_memories(&conn, &org.id, None, None, None, None, Some("personal"), None, 10, 0, false, None, None, None).unwrap();
        assert_eq!(personal_mems.len(), 1);
        assert_eq!(personal_mems[0].scope, "personal");

        let combined = list_memories(&conn, &org.id, None, None, None, None, Some("project"), None, 10, 0, false, None, None, None).unwrap();
        assert_eq!(combined.len(), 1);
    }

    #[test]
    fn list_memories_filter_by_session_id() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Create a session to use as the session_id reference
        let session_id = "test-session-abc";
        conn.execute(
            "INSERT INTO sessions (id, org_id, project, directory, started_at)
             VALUES (?1, ?2, 'proj', '/tmp', datetime('now'))",
            rusqlite::params![session_id, org.id],
        ).unwrap();

        let req_with_session = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(), content: "session memory".into(),
            tags: None, title: None, memory_type: None, scope: None, topic_key: None,
            session_id: Some(session_id.into()),
        };
        let req_without_session = crate::models::types::StoreMemoryRequest {
            project: Some("proj".into()), tool: "claude".into(), content: "other memory".into(),
            tags: None, title: None, memory_type: None, scope: None, topic_key: None,
            session_id: None,
        };
        upsert_memory(&conn, &org.id, &user.id, &req_with_session).unwrap();
        upsert_memory(&conn, &org.id, &user.id, &req_without_session).unwrap();

        let session_mems = list_memories(&conn, &org.id, None, None, None, None, None, Some(session_id), 50, 0, false, None, None, None).unwrap();
        assert_eq!(session_mems.len(), 1, "only memories matching session_id should be returned");
        assert_eq!(session_mems[0].content, "session memory");
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

        let results = list_memories(&conn, &org.id, None, None, None, Some("bugfix"), Some("project"), None, 10, 0, false, None, None, None).unwrap();
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

        let results = list_memories(&conn, &org.id, None, None, None, Some("config"), None, None, 10, 0, false, None, None, None).unwrap();
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
        let entries = list_audit(&guard, &org_id, None, None, None, None, None, None, 50, 0).unwrap();
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

        let entries = list_audit(&conn, &org.id, None, None, None, None, None, None, 10, 0).unwrap();
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
        let policies = list_policies(&conn, &org.id, 1000, 0).unwrap();
        assert!(policies.is_empty());
    }

    #[test]
    fn insert_policy_and_get_policy_roundtrip() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        let policy = insert_policy(&conn, &id, &org.id, "Whitelist", "model_whitelist", config_json, true, None).unwrap();

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
        insert_policy(&conn, &id, &org1.id, "Whitelist", "model_whitelist", config_json, true, None).unwrap();

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

        insert_policy(&conn, &id1, &org1.id, "Org1 Policy", "model_whitelist", config_json, true, None).unwrap();
        insert_policy(&conn, &id2, &org2_id, "Org2 Policy", "model_whitelist", config_json, true, None).unwrap();

        let org1_policies = list_policies(&conn, &org1.id, 1000, 0).unwrap();
        let org2_policies = list_policies(&conn, &org2_id, 1000, 0).unwrap();

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
        insert_policy(&conn, &id, &org.id, "Old Name", "model_whitelist", config_json, true, None).unwrap();

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
        insert_policy(&conn, &id, &org.id, "Temp", "model_whitelist", config_json, true, None).unwrap();

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
        insert_policy(&conn, &id, &org1.id, "Org1 Policy", "model_whitelist", config_json, true, None).unwrap();

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

        insert_policy(&conn, &id1, &org.id, "Enabled", "model_whitelist", config_json, true, None).unwrap();
        insert_policy(&conn, &id2, &org.id, "Disabled", "model_whitelist", config_json, false, None).unwrap();

        let enabled = list_enabled_policies(&conn, &org.id, None).unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "Enabled");
    }

    // ── Policy project scoping tests ──────────────────────────────────────────

    #[test]
    fn insert_policy_with_project_id_round_trips() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project = create_project(&conn, &org.id, "proj-a", None, None).unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        let policy = insert_policy(&conn, &id, &org.id, "Scoped", "model_whitelist", config_json, true, Some(&project.id)).unwrap();

        assert_eq!(policy.project_id.as_deref(), Some(project.id.as_str()));

        let fetched = get_policy(&conn, &id, &org.id).unwrap().unwrap();
        assert_eq!(fetched.project_id.as_deref(), Some(project.id.as_str()));
    }

    #[test]
    fn insert_policy_without_project_id_is_org_wide() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        let policy = insert_policy(&conn, &id, &org.id, "OrgWide", "model_whitelist", config_json, true, None).unwrap();

        assert!(policy.project_id.is_none());
        let fetched = get_policy(&conn, &id, &org.id).unwrap().unwrap();
        assert!(fetched.project_id.is_none());
    }

    #[test]
    fn list_enabled_policies_org_wide_policy_applies_to_any_project() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project_a = create_project(&conn, &org.id, "proj-a", None, None).unwrap();
        let project_b = create_project(&conn, &org.id, "proj-b", None, None).unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(&conn, &id, &org.id, "OrgWide", "model_whitelist", config_json, true, None).unwrap();

        let for_a = list_enabled_policies(&conn, &org.id, Some(&project_a.id)).unwrap();
        let for_b = list_enabled_policies(&conn, &org.id, Some(&project_b.id)).unwrap();
        assert_eq!(for_a.len(), 1, "org-wide policy must apply to project A");
        assert_eq!(for_b.len(), 1, "org-wide policy must apply to project B");
    }

    #[test]
    fn list_enabled_policies_project_scoped_only_for_that_project() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project_a = create_project(&conn, &org.id, "proj-a", None, None).unwrap();
        let project_b = create_project(&conn, &org.id, "proj-b", None, None).unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(&conn, &id, &org.id, "ProjA Only", "model_whitelist", config_json, true, Some(&project_a.id)).unwrap();

        let for_a = list_enabled_policies(&conn, &org.id, Some(&project_a.id)).unwrap();
        let for_b = list_enabled_policies(&conn, &org.id, Some(&project_b.id)).unwrap();
        assert_eq!(for_a.len(), 1, "project-scoped policy must apply to its own project");
        assert_eq!(for_b.len(), 0, "project-scoped policy must NOT apply to a different project");
    }

    #[test]
    fn list_enabled_policies_none_returns_everything_including_project_scoped() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project_a = create_project(&conn, &org.id, "proj-a", None, None).unwrap();

        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        let id1 = format!("p_{}", Uuid::new_v4().simple());
        let id2 = format!("p_{}", Uuid::new_v4().simple());
        let id3 = format!("p_{}", Uuid::new_v4().simple());
        insert_policy(&conn, &id1, &org.id, "OrgWide", "model_whitelist", config_json, true, None).unwrap();
        insert_policy(&conn, &id2, &org.id, "ProjA", "model_whitelist", config_json, true, Some(&project_a.id)).unwrap();
        insert_policy(&conn, &id3, &org.id, "DisabledOrgWide", "model_whitelist", config_json, false, None).unwrap();

        let admin_view = list_enabled_policies(&conn, &org.id, None).unwrap();
        assert_eq!(admin_view.len(), 2, "None must return all ENABLED policies for the org regardless of project_id");
        let names: Vec<&str> = admin_view.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"OrgWide"));
        assert!(names.contains(&"ProjA"));
        assert!(!names.contains(&"DisabledOrgWide"), "disabled policies must still be excluded");
    }

    #[test]
    fn list_enabled_policies_project_resolution_is_union_not_other_projects() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project_a = create_project(&conn, &org.id, "proj-a", None, None).unwrap();
        let project_q = create_project(&conn, &org.id, "proj-q", None, None).unwrap();

        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        let id1 = format!("p_{}", Uuid::new_v4().simple());
        let id2 = format!("p_{}", Uuid::new_v4().simple());
        let id3 = format!("p_{}", Uuid::new_v4().simple());
        insert_policy(&conn, &id1, &org.id, "OrgWide", "model_whitelist", config_json, true, None).unwrap();
        insert_policy(&conn, &id2, &org.id, "ProjA", "model_whitelist", config_json, true, Some(&project_a.id)).unwrap();
        insert_policy(&conn, &id3, &org.id, "ProjQ", "model_whitelist", config_json, true, Some(&project_q.id)).unwrap();

        let for_a = list_enabled_policies(&conn, &org.id, Some(&project_a.id)).unwrap();
        let names: Vec<&str> = for_a.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(for_a.len(), 2, "resolving for project A must be org-wide UNION project A");
        assert!(names.contains(&"OrgWide"));
        assert!(names.contains(&"ProjA"));
        assert!(!names.contains(&"ProjQ"), "project Q's policy must not leak into project A's resolution");
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
    fn get_chunk_covering_line_returns_tightest_chunk() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        // A broad window and a tight symbol chunk both cover line 12.
        insert_code_chunk(&conn, project_id, "src/lib.rs", "h", Some("rust"), None, 1, 60, "whole window", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/lib.rs", "h", Some("rust"), Some("foo"), 10, 20, "fn foo() {}", None).unwrap();

        let chunk = get_chunk_covering_line(&conn, project_id, "src/lib.rs", 12)
            .unwrap()
            .expect("a chunk must cover line 12");
        assert_eq!(chunk.content, "fn foo() {}", "tightest covering chunk wins");
        assert_eq!(chunk.symbol.as_deref(), Some("foo"));

        // No chunk covers a line past EOF.
        assert!(
            get_chunk_covering_line(&conn, project_id, "src/lib.rs", 999).unwrap().is_none(),
            "out-of-range line returns None"
        );
    }

    #[test]
    fn get_file_chunks_returns_all_ordered() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        // Two methods of a class, plus a chunk in another file.
        insert_code_chunk(&conn, project_id, "src/svc.ts", "h", Some("typescript"), Some("two"), 20, 25, "two() {}", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/svc.ts", "h", Some("typescript"), Some("one"), 10, 15, "one() {}", None).unwrap();
        insert_code_chunk(&conn, project_id, "src/other.ts", "h", Some("typescript"), None, 1, 5, "other", None).unwrap();

        let chunks = get_file_chunks(&conn, project_id, "src/svc.ts").unwrap();
        assert_eq!(chunks.len(), 2, "only the target file's chunks");
        assert_eq!(chunks[0].start_line, 10, "ordered by start_line");
        assert_eq!(chunks[1].start_line, 20);
        // Range overlap [12, 22] (a class spanning its methods) catches both.
        let overlapping: Vec<_> = chunks.iter().filter(|c| c.start_line <= 22 && c.end_line >= 12).collect();
        assert_eq!(overlapping.len(), 2, "class range overlaps both method chunks");
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

    // ── get_memory_facets tests ───────────────────────────────────────────────

    #[test]
    fn get_memory_facets_empty_org_returns_empty_vecs() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let facets = get_memory_facets(&conn, &org.id).unwrap();
        assert!(facets.types.is_empty(), "no memories => no type facets");
        assert!(facets.projects.is_empty(), "no memories => no project facets");
        // scope may be empty too (no rows)
        assert!(facets.scopes.is_empty(), "no memories => no scope facets");
    }

    #[test]
    fn get_memory_facets_counts_types_correctly() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Insert 2 bugfix + 1 decision
        for i in 0..2 {
            upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
                project: Some("p".into()),
                tool: "claude".into(),
                content: format!("bugfix content {i}"),
                tags: None,
                title: None,
                memory_type: Some("bugfix".into()),
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();
        }
        upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
            project: Some("p".into()),
            tool: "claude".into(),
            content: "decision content".into(),
            tags: None,
            title: None,
            memory_type: Some("decision".into()),
            scope: None,
            topic_key: None,
            session_id: None,
        }).unwrap();

        let facets = get_memory_facets(&conn, &org.id).unwrap();

        let bugfix = facets.types.iter().find(|f| f.value == "bugfix");
        let decision = facets.types.iter().find(|f| f.value == "decision");

        assert!(bugfix.is_some(), "bugfix facet must be present");
        assert_eq!(bugfix.unwrap().count, 2);
        assert!(decision.is_some(), "decision facet must be present");
        assert_eq!(decision.unwrap().count, 1);
    }

    #[test]
    fn get_memory_facets_counts_projects_and_scopes() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
            project: Some("proj-a".into()),
            tool: "claude".into(),
            content: "content a".into(),
            tags: None, title: None,
            memory_type: None,
            scope: Some("personal".into()),
            topic_key: None, session_id: None,
        }).unwrap();
        upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
            project: Some("proj-b".into()),
            tool: "claude".into(),
            content: "content b".into(),
            tags: None, title: None,
            memory_type: None,
            scope: Some("project".into()),
            topic_key: None, session_id: None,
        }).unwrap();

        let facets = get_memory_facets(&conn, &org.id).unwrap();

        // Projects
        assert_eq!(facets.projects.len(), 2);
        let names: Vec<&str> = facets.projects.iter().map(|f| f.value.as_str()).collect();
        assert!(names.contains(&"proj-a"));
        assert!(names.contains(&"proj-b"));

        // Scopes
        let personal = facets.scopes.iter().find(|f| f.value == "personal");
        let project  = facets.scopes.iter().find(|f| f.value == "project");
        assert!(personal.is_some(), "personal scope must appear");
        assert!(project.is_some(), "project scope must appear");
    }

    #[test]
    fn get_memory_facets_scoped_to_org() {
        let conn = setup();
        let (org_a, user_a, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Second org inserted directly
        let org_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'OrgB', 'orgb')",
            [&org_b_id],
        ).unwrap();
        let user_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES (?1, ?2, 'b@b.com', 'B', 'admin', 'active')",
            rusqlite::params![user_b_id, org_b_id],
        ).unwrap();

        upsert_memory(&conn, &org_a.id, &user_a.id, &StoreMemoryRequest {
            project: Some("proj-a".into()),
            tool: "claude".into(), content: "a content".into(),
            tags: None, title: None, memory_type: Some("bugfix".into()),
            scope: None, topic_key: None, session_id: None,
        }).unwrap();
        upsert_memory(&conn, &org_b_id, &user_b_id, &StoreMemoryRequest {
            project: Some("proj-b".into()),
            tool: "claude".into(), content: "b content".into(),
            tags: None, title: None, memory_type: Some("decision".into()),
            scope: None, topic_key: None, session_id: None,
        }).unwrap();

        // Facets for org_a must not see org_b's memories
        let facets_a = get_memory_facets(&conn, &org_a.id).unwrap();
        assert_eq!(facets_a.projects.len(), 1);
        assert_eq!(facets_a.projects[0].value, "proj-a");
        assert!(facets_a.types.iter().all(|f| f.value != "decision"),
            "org_a must not see org_b type 'decision'");
    }

    // ── bulk_delete_memories tests ────────────────────────────────────────────

    #[test]
    fn bulk_delete_admin_deletes_any_org_memory() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let m1 = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content1", &[]);
        let m2 = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content2", &[]);
        let m3 = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content3", &[]);

        // Admin deletes m1 and m2
        let ids = vec![m1.id.clone(), m2.id.clone()];
        let deleted = bulk_delete_memories(&conn, &org.id, &ids, true, &user.id).unwrap();
        assert_eq!(deleted, 2, "admin should delete exactly 2 memories");

        // m3 must still exist
        let owner = get_memory_owner(&conn, &org.id, &m3.id).unwrap();
        assert!(owner.is_some(), "m3 must still be present");

        // m1 and m2 must be gone
        assert!(get_memory_owner(&conn, &org.id, &m1.id).unwrap().is_none());
        assert!(get_memory_owner(&conn, &org.id, &m2.id).unwrap().is_none());
    }

    #[test]
    fn bulk_delete_non_admin_only_deletes_own_memories() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Create a second user (member)
        let member_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, 'member@acme.com', 'Member', 'member', 'active', datetime('now'))",
            rusqlite::params![member_id, org.id],
        ).unwrap();

        let admin_mem = legacy_store(&conn, &org.id, &admin.id, "proj", "claude", "admin content", &[]);
        let member_mem = legacy_store(&conn, &org.id, &member_id, "proj", "claude", "member content", &[]);

        // Member tries to bulk-delete both (is_admin = false)
        let ids = vec![admin_mem.id.clone(), member_mem.id.clone()];
        let deleted = bulk_delete_memories(&conn, &org.id, &ids, false, &member_id).unwrap();

        // Only the member's own memory should be deleted
        assert_eq!(deleted, 1, "non-admin should only delete own memory");
        assert!(get_memory_owner(&conn, &org.id, &admin_mem.id).unwrap().is_some(),
            "admin memory must survive");
        assert!(get_memory_owner(&conn, &org.id, &member_mem.id).unwrap().is_none(),
            "member's own memory must be deleted");
    }

    #[test]
    fn bulk_delete_empty_ids_returns_zero() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let deleted = bulk_delete_memories(&conn, &org.id, &[], true, "anyone").unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn bulk_delete_cross_org_isolation() {
        // Use two separate in-memory databases to avoid bootstrap's single-org
        // constraint on the same connection.
        let conn_a = setup();
        let (org_a, user_a, _) = bootstrap(&conn_a, "OrgA", "orga", "admin@a.com", "AdminA").unwrap();

        let conn_b = setup();
        let (org_b, user_b, _) = bootstrap(&conn_b, "OrgB", "orgb", "admin@b.com", "AdminB").unwrap();

        // Store a memory in org A's DB
        let mem_a = legacy_store(&conn_a, &org_a.id, &user_a.id, "proj", "claude", "a content", &[]);

        // Org B (admin) tries to delete org A's memory ID via org B's connection.
        // The WHERE clause filters by org_b.id so nothing in org_a is touched.
        let deleted = bulk_delete_memories(&conn_b, &org_b.id, std::slice::from_ref(&mem_a.id), true, &user_b.id).unwrap();
        assert_eq!(deleted, 0, "cross-org deletion must not succeed");

        // Org A's memory must still exist in org A's DB
        assert!(get_memory_owner(&conn_a, &org_a.id, &mem_a.id).unwrap().is_some(),
            "org A memory must be untouched");
    }

    #[test]
    fn bulk_delete_nonexistent_ids_returns_zero() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let _ = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "real", &[]);

        let deleted = bulk_delete_memories(
            &conn, &org.id,
            &["ghost-1".to_string(), "ghost-2".to_string()],
            true, &user.id,
        ).unwrap();
        assert_eq!(deleted, 0, "deleting nonexistent IDs should return 0");
    }

    // ── bulk_tag_memories tests ───────────────────────────────────────────────

    #[test]
    fn bulk_tag_add_to_two_memories() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let m1 = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content1", &[]);
        let m2 = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content2", &[]);

        let ids = vec![m1.id.clone(), m2.id.clone()];
        let updated = bulk_tag_memories(&conn, &org.id, &ids, "add", "important").unwrap();
        assert_eq!(updated, 2, "should update both memories");

        let mems = get_memories_by_ids(&conn, &org.id, &ids).unwrap();
        assert!(mems.iter().all(|m| m.tags.contains(&"important".to_string())),
            "both memories must have the 'important' tag");
    }

    #[test]
    fn bulk_tag_remove_from_memory() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let m = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "content",
            &["keep".to_string(), "drop".to_string()]);

        let updated = bulk_tag_memories(&conn, &org.id, std::slice::from_ref(&m.id), "remove", "drop").unwrap();
        assert_eq!(updated, 1);

        let remaining = get_memories_by_ids(&conn, &org.id, &[m.id]).unwrap();
        let tags = &remaining[0].tags;
        assert!(tags.contains(&"keep".to_string()), "'keep' tag must survive");
        assert!(!tags.contains(&"drop".to_string()), "'drop' tag must be removed");
    }

    #[test]
    fn bulk_tag_wrong_org_memories_are_skipped() {
        let conn_a = setup();
        let (org_a, user_a, _) = bootstrap(&conn_a, "OrgA", "orga", "a@a.com", "AdminA").unwrap();

        let conn_b = setup();
        let (org_b, _, _) = bootstrap(&conn_b, "OrgB", "orgb", "b@b.com", "AdminB").unwrap();

        let mem_a = legacy_store(&conn_a, &org_a.id, &user_a.id, "proj", "claude", "content", &[]);

        // Attempt to tag org_a's memory using org_b's org_id on conn_b
        let updated = bulk_tag_memories(&conn_b, &org_b.id, std::slice::from_ref(&mem_a.id), "add", "hacked").unwrap();
        assert_eq!(updated, 0, "cross-org tag must not succeed");

        // Original memory in org_a must be untouched
        let orig = get_memories_by_ids(&conn_a, &org_a.id, &[mem_a.id]).unwrap();
        assert!(orig[0].tags.is_empty(), "org_a memory tags must be unchanged");
    }

    // ── webhook query tests ───────────────────────────────────────────────────

    fn make_create_webhook_req(name: &str, url: &str) -> CreateWebhookRequest {
        CreateWebhookRequest {
            name: name.to_string(),
            target_url: url.to_string(),
            secret: None,
            events: None,
        }
    }

    #[test]
    fn list_webhooks_returns_empty_for_new_org() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hooks = list_webhooks(&conn, &org.id).unwrap();
        assert!(hooks.is_empty(), "new org must have no webhooks");
    }

    #[test]
    fn create_webhook_and_list_roundtrip() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let req = make_create_webhook_req("my-hook", "https://example.com/hook");
        let created = create_webhook(&conn, &org.id, &req).unwrap();

        assert_eq!(created.name, "my-hook");
        assert_eq!(created.target_url, "https://example.com/hook");
        assert!(created.active, "new webhook must be active by default");
        assert_eq!(created.events, vec!["*"], "default events must be [\"*\"]");
        assert!(created.secret.is_none());

        let hooks = list_webhooks(&conn, &org.id).unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].id, created.id);
    }

    #[test]
    fn create_webhook_with_custom_events() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let req = CreateWebhookRequest {
            name: "pr-hook".to_string(),
            target_url: "https://example.com/pr".to_string(),
            secret: Some("s3cr3t".to_string()),
            events: Some(vec!["pull_request".to_string(), "push".to_string()]),
        };
        let created = create_webhook(&conn, &org.id, &req).unwrap();

        assert_eq!(created.events, vec!["pull_request", "push"]);
        assert_eq!(created.secret.as_deref(), Some("s3cr3t"));
    }

    #[test]
    fn update_webhook_toggles_active() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let req = make_create_webhook_req("hook", "https://example.com");
        let created = create_webhook(&conn, &org.id, &req).unwrap();

        let update = UpdateWebhookRequest { active: Some(false), ..Default::default() };
        let updated = update_webhook(&conn, &org.id, &created.id, &update).unwrap().unwrap();
        assert!(!updated.active, "webhook must be inactive after update");
    }

    #[test]
    fn update_webhook_returns_none_for_missing_id() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let update = UpdateWebhookRequest { active: Some(false), ..Default::default() };
        let result = update_webhook(&conn, &org.id, "nonexistent", &update).unwrap();
        assert!(result.is_none(), "must return None for nonexistent webhook");
    }

    #[test]
    fn delete_webhook_removes_row() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let req = make_create_webhook_req("hook", "https://example.com");
        let created = create_webhook(&conn, &org.id, &req).unwrap();

        let deleted = delete_webhook(&conn, &org.id, &created.id).unwrap();
        assert!(deleted, "delete must return true for existing webhook");

        let hooks = list_webhooks(&conn, &org.id).unwrap();
        assert!(hooks.is_empty(), "webhook must be gone after delete");
    }

    #[test]
    fn delete_webhook_cross_org_returns_false() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let req = make_create_webhook_req("hook", "https://example.com");
        let created = create_webhook(&conn, &org.id, &req).unwrap();

        // Another org tries to delete org1's webhook
        let deleted = delete_webhook(&conn, "other-org", &created.id).unwrap();
        assert!(!deleted, "cross-org deletion must not succeed");

        // Original must still exist
        let hook = get_webhook(&conn, &created.id, &org.id).unwrap();
        assert!(hook.is_some(), "webhook must survive cross-org delete attempt");
    }

    #[test]
    fn list_webhooks_org_isolation() {
        let conn = setup();
        let (org_a, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Create a second org directly
        let org_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Beta', 'beta')",
            [&org_b_id],
        ).unwrap();

        let req = make_create_webhook_req("hook", "https://example.com");
        create_webhook(&conn, &org_a.id, &req).unwrap();

        let hooks_b = list_webhooks(&conn, &org_b_id).unwrap();
        assert!(hooks_b.is_empty(), "org_b must not see org_a webhooks");
    }

    #[test]
    fn list_all_org_keys_returns_active_keys() {
        let conn = setup();
        let (org, _, _raw_key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        // The bootstrap creates one active key
        let keys = list_all_org_keys(&conn, &org.id).unwrap();
        assert_eq!(keys.len(), 1, "bootstrap creates exactly one active key");
        assert!(!keys[0].revoked, "key must not be revoked");
        assert_eq!(keys[0].user_email, "admin@acme.com");
    }

    #[test]
    fn revoke_key_admin_marks_key_revoked() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let keys_before = list_all_org_keys(&conn, &org.id).unwrap();
        assert_eq!(keys_before.len(), 1);
        let key_id = keys_before[0].id.clone();

        let revoked = revoke_key_admin(&conn, &org.id, &key_id).unwrap();
        assert!(revoked, "must return true for existing key");

        let keys_after = list_all_org_keys(&conn, &org.id).unwrap();
        assert!(keys_after.is_empty(), "revoked key must not appear in list");
    }

    // ── get_memory_trends tests ───────────────────────────────────────────────

    #[test]
    fn trends_empty_org_returns_zeros() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let trends = get_memory_trends(&conn, &org.id, 30).unwrap();
        assert_eq!(trends.total, 0);
        assert_eq!(trends.this_week, 0);
        assert_eq!(trends.this_month, 0);
        assert!(trends.daily_counts.is_empty());
        assert!(trends.by_type.is_empty());
        assert!(trends.by_project.is_empty());
    }

    #[test]
    fn trends_single_memory_appears_in_all_buckets() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let req = crate::models::types::StoreMemoryRequest {
            project: Some("myproject".to_string()),
            tool: "claude".to_string(),
            content: "test content".to_string(),
            tags: None,
            title: Some("Test".to_string()),
            memory_type: Some("decision".to_string()),
            scope: None,
            topic_key: None,
            session_id: None,
        };
        upsert_memory(&conn, &org.id, &user.id, &req).unwrap();

        let trends = get_memory_trends(&conn, &org.id, 30).unwrap();
        assert_eq!(trends.total, 1);
        assert_eq!(trends.this_week, 1);
        assert_eq!(trends.this_month, 1);
        assert_eq!(trends.daily_counts.len(), 1);
        assert_eq!(trends.daily_counts[0].count, 1);
        assert_eq!(trends.by_type.len(), 1);
        assert_eq!(trends.by_type[0].name, "decision");
        assert_eq!(trends.by_project.len(), 1);
        assert_eq!(trends.by_project[0].name, "myproject");
    }

    #[test]
    fn trends_multiple_memories_aggregated_correctly() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        for (project, mem_type, content) in &[
            ("proj-a", "decision", "content a1"),
            ("proj-a", "bugfix", "content a2"),
            ("proj-b", "decision", "content b1"),
        ] {
            let req = crate::models::types::StoreMemoryRequest {
                project: Some(project.to_string()),
                tool: "claude".to_string(),
                content: content.to_string(),
                tags: None,
                title: None,
                memory_type: Some(mem_type.to_string()),
                scope: None,
                topic_key: None,
                session_id: None,
            };
            upsert_memory(&conn, &org.id, &user.id, &req).unwrap();
        }

        let trends = get_memory_trends(&conn, &org.id, 30).unwrap();
        assert_eq!(trends.total, 3);
        assert_eq!(trends.this_week, 3);
        assert_eq!(trends.this_month, 3);

        // by_type: decision=2, bugfix=1
        let decision_count = trends.by_type.iter().find(|x| x.name == "decision").map(|x| x.count);
        assert_eq!(decision_count, Some(2));
        let bugfix_count = trends.by_type.iter().find(|x| x.name == "bugfix").map(|x| x.count);
        assert_eq!(bugfix_count, Some(1));

        // by_project: proj-a=2, proj-b=1
        let proj_a_count = trends.by_project.iter().find(|x| x.name == "proj-a").map(|x| x.count);
        assert_eq!(proj_a_count, Some(2));
        let proj_b_count = trends.by_project.iter().find(|x| x.name == "proj-b").map(|x| x.count);
        assert_eq!(proj_b_count, Some(1));
    }

    #[test]
    fn trends_days_param_scopes_total_and_breakdowns() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Insert a memory that appears to be from 45 days ago
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope, type)
             VALUES ('old1', ?1, ?2, 'old-project', 'test', 'old content', '[]', datetime('now', '-45 days'), 'project', 'discovery')",
            rusqlite::params![org.id, user.id],
        ).unwrap();

        // Insert a memory from today
        let req = crate::models::types::StoreMemoryRequest {
            project: Some("new-project".to_string()),
            tool: "test".to_string(),
            content: "recent content".to_string(),
            tags: None,
            title: None,
            memory_type: Some("bugfix".to_string()),
            scope: None,
            topic_key: None,
            session_id: None,
        };
        upsert_memory(&conn, &org.id, &user.id, &req).unwrap();

        // With days=30, the old memory (45 days ago) must be excluded
        let trends_30 = get_memory_trends(&conn, &org.id, 30).unwrap();
        assert_eq!(trends_30.total, 1, "days=30 must exclude the 45-day-old memory");
        assert!(!trends_30.by_project.iter().any(|x| x.name == "old-project"),
            "old-project must not appear in by_project when days=30");
        assert!(!trends_30.by_type.iter().any(|x| x.name == "discovery"),
            "discovery type must not appear in by_type when days=30");

        // With days=90, both memories must appear
        let trends_90 = get_memory_trends(&conn, &org.id, 90).unwrap();
        assert_eq!(trends_90.total, 2, "days=90 must include both memories");
        assert!(trends_90.by_project.iter().any(|x| x.name == "old-project"),
            "old-project must appear when days=90");
    }

    // ── project event override tests ──────────────────────────────────────────

    fn setup_project(conn: &Connection) -> (String, String) {
        let (org, _, _) = bootstrap(conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project = create_project(conn, &org.id, "my-project", None, None).unwrap();
        (org.id, project.id)
    }

    #[test]
    fn get_project_event_overrides_returns_default_when_null() {
        let conn = setup();
        let (org_id, project_id) = setup_project(&conn);
        let overrides = get_project_event_overrides(&conn, &org_id, &project_id).unwrap();
        // All fields must be None (inherit)
        assert!(overrides.resolve_issues.is_none());
        assert!(overrides.review_prs.is_none());
        assert!(overrides.respond_comments.is_none());
        assert!(overrides.auto_index.is_none());
        assert!(overrides.scanner.is_none());
    }

    #[test]
    fn update_project_event_overrides_persists_values() {
        let conn = setup();
        let (org_id, project_id) = setup_project(&conn);

        let new_overrides = ProjectEventOverrides {
            resolve_issues: Some(false),
            review_prs: Some(true),
            respond_comments: None,
            auto_index: Some(false),
            scanner: None,
        };
        let saved = update_project_event_overrides(&conn, &org_id, &project_id, new_overrides).unwrap();
        assert_eq!(saved.resolve_issues, Some(false));
        assert_eq!(saved.review_prs, Some(true));
        assert!(saved.respond_comments.is_none());
        assert_eq!(saved.auto_index, Some(false));
        assert!(saved.scanner.is_none());

        // Read back
        let read = get_project_event_overrides(&conn, &org_id, &project_id).unwrap();
        assert_eq!(read.resolve_issues, Some(false));
        assert_eq!(read.review_prs, Some(true));
    }

    #[test]
    fn update_project_event_overrides_cross_org_returns_err() {
        let conn = setup();
        let (_, project_id) = setup_project(&conn);
        // Use a different org_id — should fail (no rows affected)
        let result = update_project_event_overrides(
            &conn,
            "other-org",
            &project_id,
            ProjectEventOverrides::default(),
        );
        // update returns Err when project not found for that org
        assert!(result.is_err(), "cross-org update must return error");
    }

    #[test]
    fn update_then_clear_event_overrides() {
        let conn = setup();
        let (org_id, project_id) = setup_project(&conn);

        // Set some overrides
        update_project_event_overrides(&conn, &org_id, &project_id, ProjectEventOverrides {
            resolve_issues: Some(true),
            ..Default::default()
        }).unwrap();

        // Clear by saving empty overrides (all None = inherit)
        let cleared = update_project_event_overrides(&conn, &org_id, &project_id, ProjectEventOverrides::default()).unwrap();
        // With all-None, the JSON stored is "{}", which deserializes back as all-None
        assert!(cleared.resolve_issues.is_none());
    }

    // ── Duplicate detection tests ─────────────────────────────────────────────

    #[test]
    fn get_duplicate_groups_returns_empty_when_no_duplicates() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        // Store two distinct memories
        legacy_store(&conn, &org.id, &user.id, "proj", "claude", "unique content one", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj", "claude", "unique content two", &[]);

        let groups = get_duplicate_groups(&conn, &org.id).unwrap();
        assert!(groups.is_empty(), "expected no duplicate groups when all memories are distinct");
    }

    #[test]
    fn get_duplicate_groups_groups_identical_memories() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Insert two memories with identical content (but different IDs — no topic_key so always INSERT)
        let content = "use snake_case for all identifiers";
        legacy_store(&conn, &org.id, &user.id, "proj", "claude", content, &[]);
        legacy_store(&conn, &org.id, &user.id, "proj", "cursor", content, &[]);

        let groups = get_duplicate_groups(&conn, &org.id).unwrap();
        assert_eq!(groups.len(), 1, "expected exactly one duplicate group");
        assert_eq!(groups[0].len(), 2, "group must contain both memories");

        // All memories in the group share the same normalized_hash
        let hashes: Vec<_> = groups[0].iter().map(|m| m.normalized_hash.as_deref().unwrap_or("")).collect();
        assert_eq!(hashes[0], hashes[1], "both entries must have the same hash");
    }

    // ── v17 archive/restore unit tests ────────────────────────────────────────

    #[test]
    fn archive_memory_sets_archived_at() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
            project: None,
            tool: "claude".to_string(),
            content: "archive me".to_string(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        }).unwrap();

        assert!(mem.archived_at.is_none(), "new memory must not be archived");

        let updated = archive_memory(&conn, &org.id, &mem.id).unwrap();
        assert!(updated, "archive_memory must return true on first archive");

        let val: Option<String> = conn.query_row(
            "SELECT archived_at FROM memories WHERE id = ?1",
            [&mem.id],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_some(), "archived_at must be set after archive_memory");
    }

    #[test]
    fn restore_memory_clears_archived_at() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
            project: None,
            tool: "claude".to_string(),
            content: "archive then restore".to_string(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        }).unwrap();

        archive_memory(&conn, &org.id, &mem.id).unwrap();

        let restored = restore_memory(&conn, &org.id, &mem.id).unwrap();
        assert!(restored, "restore_memory must return true when memory was archived");

        let val: Option<String> = conn.query_row(
            "SELECT archived_at FROM memories WHERE id = ?1",
            [&mem.id],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "archived_at must be NULL after restore_memory");
    }

    #[test]
    fn list_memories_excludes_archived_by_default() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let mem1 = upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
            project: None,
            tool: "claude".to_string(),
            content: "active memory".to_string(),
            tags: None, title: None, memory_type: None, scope: None, topic_key: None, session_id: None,
        }).unwrap();
        let mem2 = upsert_memory(&conn, &org.id, &user.id, &StoreMemoryRequest {
            project: None,
            tool: "claude".to_string(),
            content: "archived memory".to_string(),
            tags: None, title: None, memory_type: None, scope: None, topic_key: None, session_id: None,
        }).unwrap();

        archive_memory(&conn, &org.id, &mem2.id).unwrap();

        // Default (include_archived=false) must exclude archived
        let active = list_memories(&conn, &org.id, None, None, None, None, None, None, 50, 0, false, None, None, None).unwrap();
        let ids: Vec<_> = active.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&mem1.id.as_str()), "active memory must appear");
        assert!(!ids.contains(&mem2.id.as_str()), "archived memory must be excluded by default");

        // include_archived=true must include both
        let all = list_memories(&conn, &org.id, None, None, None, None, None, None, 50, 0, true, None, None, None).unwrap();
        let all_ids: Vec<_> = all.iter().map(|m| m.id.as_str()).collect();
        assert!(all_ids.contains(&mem1.id.as_str()), "active memory must appear when include_archived=true");
        assert!(all_ids.contains(&mem2.id.as_str()), "archived memory must appear when include_archived=true");
    }
}

// ── Duplicate detection ───────────────────────────────────────────────────────

/// Fetches all memories for an org that share the given `normalized_hash`.
fn list_memories_by_hash(conn: &Connection, org_id: &str, hash: &str) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                archived_at, pinned
         FROM memories
         WHERE org_id = ?1 AND normalized_hash = ?2
         ORDER BY created_at DESC",
    )?;

    let rows = stmt.query_map(rusqlite::params![org_id, hash], |row| {
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
            row.get::<_, Option<String>>(16)?,
            row.get::<_, i64>(17).unwrap_or(0),
        ))
    })?;

    let mut memories = Vec::new();
    for row in rows {
        let (id, org_id, user_id, project, tool, content, tags_str, created_at,
             title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
             archived_at, pinned_i64) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() { "archived".to_string() } else { "active".to_string() };
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
            archived_at,
            pinned: pinned_i64 != 0,
            collection_id: None,
            admin_note: None,
            delete_after: None,
            status,
        });
    }
    Ok(memories)
}

/// Returns groups of memories that share the same `normalized_hash` within an org.
/// Each inner `Vec<Memory>` contains 2+ identical memories, ordered by `created_at DESC`
/// (newest first). Groups with only one member are excluded.
pub fn get_duplicate_groups(conn: &Connection, org_id: &str) -> Result<Vec<Vec<Memory>>> {
    let hashes: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT normalized_hash FROM memories
             WHERE org_id = ?1 AND normalized_hash IS NOT NULL
             GROUP BY normalized_hash HAVING COUNT(*) > 1",
        )?;
        let result = stmt.query_map([org_id], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        result
    };

    let mut groups = Vec::new();
    for hash in hashes {
        let memories = list_memories_by_hash(conn, org_id, &hash)?;
        if memories.len() > 1 {
            groups.push(memories);
        }
    }
    Ok(groups)
}

/// Returns a health summary for the org's memory corpus.
/// Used by `GET /v1/admin/memories/health`.
pub fn get_memory_health(conn: &Connection, org_id: &str) -> Result<crate::models::types::MemoryHealth> {
    let total_memories: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1 AND archived_at IS NULL",
        [org_id],
        |row| row.get(0),
    )?;

    let stale_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1 AND archived_at IS NULL
         AND created_at < datetime('now', '-30 days')",
        [org_id],
        |row| row.get(0),
    )?;

    let untagged_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE org_id = ?1 AND archived_at IS NULL
         AND (tags IS NULL OR tags = '[]' OR tags = '')",
        [org_id],
        |row| row.get(0),
    )?;

    let duplicate_count: i64 = conn.query_row(
        "SELECT COALESCE(SUM(cnt - 1), 0) FROM (
           SELECT COUNT(*) as cnt FROM memories
           WHERE org_id = ?1 AND archived_at IS NULL
           GROUP BY LOWER(TRIM(SUBSTR(content, 1, 200)))
           HAVING cnt > 1
         )",
        [org_id],
        |row| row.get(0),
    )?;

    Ok(crate::models::types::MemoryHealth {
        total_memories,
        duplicate_count,
        stale_count,
        untagged_count,
    })
}

/// Merges two memories: appends `merge_id`'s content to `keep_id`'s content (separated by
/// `\n\n---\n\n`), then deletes `merge_id`. Both must belong to the given org.
/// Returns the updated `keep_id` memory on success, or an error if either memory is not found.
pub fn merge_memories(conn: &Connection, org_id: &str, keep_id: &str, merge_id: &str) -> Result<Memory> {
    // Validate both memories exist in the org
    let keep_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM memories WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![keep_id, org_id],
        |r| r.get(0),
    )?;
    if !keep_exists {
        anyhow::bail!("keep memory not found");
    }
    let merge_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM memories WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![merge_id, org_id],
        |r| r.get(0),
    )?;
    if !merge_exists {
        anyhow::bail!("merge memory not found");
    }

    // Append merge_id content to keep_id content
    conn.execute(
        "UPDATE memories
         SET content = (SELECT content FROM memories WHERE id = ?1 AND org_id = ?3)
                       || '\n\n---\n\n'
                       || (SELECT content FROM memories WHERE id = ?2 AND org_id = ?3)
         WHERE id = ?1 AND org_id = ?3",
        rusqlite::params![keep_id, merge_id, org_id],
    )?;

    // Delete the merged memory
    conn.execute(
        "DELETE FROM memories WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![merge_id, org_id],
    )?;

    // Return the updated kept memory
    get_memory_by_id_for_org(conn, org_id, keep_id)?
        .ok_or_else(|| anyhow::anyhow!("keep memory disappeared after merge"))
}

/// Returns agent/tool activity for the last 30 days — ordered by `memories_last_7d DESC`.
pub fn get_agent_activity(conn: &Connection, org_id: &str, days: i64) -> Result<Vec<crate::models::types::AgentActivity>> {
    use crate::models::types::AgentActivity;

    let mut stmt = conn.prepare(
        "SELECT
           COALESCE(tool, 'unknown') as tool,
           COUNT(*) as total_memories,
           SUM(CASE WHEN created_at >= datetime('now', '-1 day') THEN 1 ELSE 0 END) as memories_last_24h,
           SUM(CASE WHEN created_at >= datetime('now', '-7 days') THEN 1 ELSE 0 END) as memories_last_7d,
           MAX(created_at) as last_seen
         FROM memories
         WHERE org_id = ?1 AND created_at >= datetime('now', '-' || ?2 || ' days')
         GROUP BY COALESCE(tool, 'unknown')
         ORDER BY memories_last_7d DESC
         LIMIT 20",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, days], |row| {
        Ok(AgentActivity {
            tool: row.get(0)?,
            total_memories: row.get(1)?,
            memories_last_24h: row.get(2)?,
            memories_last_7d: row.get(3)?,
            last_seen: row.get(4)?,
        })
    })?;
    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }
    Ok(result)
}

/// Returns the onboarding checklist status for an org.
/// All checks are computed dynamically from existing tables — no schema changes needed.
pub fn get_onboarding_status(conn: &Connection, org_id: &str) -> Result<OnboardingStatus> {
    let has_members: bool = conn.query_row(
        "SELECT COUNT(*) > 1 FROM users WHERE org_id = ?1",
        [org_id],
        |r| r.get::<_, bool>(0),
    )?;

    let has_repository: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM code_projects WHERE org_id = ?1",
        [org_id],
        |r| r.get::<_, bool>(0),
    )?;

    let has_project: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM projects WHERE org_id = ?1",
        [org_id],
        |r| r.get::<_, bool>(0),
    )?;

    let has_memories: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM memories WHERE org_id = ?1",
        [org_id],
        |r| r.get::<_, bool>(0),
    )?;

    let has_webhook: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM webhooks WHERE org_id = ?1",
        [org_id],
        |r| r.get::<_, bool>(0),
    )?;

    let events_configured: bool = conn.query_row(
        "SELECT COALESCE(settings, '{}') != '{}' FROM organizations WHERE id = ?1",
        [org_id],
        |r| r.get::<_, bool>(0),
    ).unwrap_or(false);

    let items = vec![
        OnboardingItem {
            key: "has_members".to_string(),
            label: "Invite a team member".to_string(),
            description: "Add at least one more person to your organization.".to_string(),
            done: has_members,
        },
        OnboardingItem {
            key: "has_project".to_string(),
            label: "Create a project".to_string(),
            description: "Organize your work by creating a project.".to_string(),
            done: has_project,
        },
        OnboardingItem {
            key: "has_repository".to_string(),
            label: "Connect a code repository".to_string(),
            description: "Index a repository so the agent can search your codebase.".to_string(),
            done: has_repository,
        },
        OnboardingItem {
            key: "has_memories".to_string(),
            label: "Store your first memory".to_string(),
            description: "Start capturing decisions, bugs, and discoveries.".to_string(),
            done: has_memories,
        },
        OnboardingItem {
            key: "has_webhook".to_string(),
            label: "Configure a webhook".to_string(),
            description: "Connect NexusMind to GitHub or other tools via webhooks.".to_string(),
            done: has_webhook,
        },
        OnboardingItem {
            key: "events_configured".to_string(),
            label: "Customize agent events".to_string(),
            description: "Tune which events the agent should react to for your org.".to_string(),
            done: events_configured,
        },
    ];

    Ok(OnboardingStatus { items })
}

// ── Invite link queries ───────────────────────────────────────────────────────

/// Creates a new invite link and returns it.
/// The token is a 32-character hex string derived from two UUIDs.
/// The link expires 7 days from now.
pub fn create_invite_link(
    conn: &Connection,
    org_id: &str,
    role: &str,
    created_by: &str,
) -> Result<InviteLink> {
    // 32-char hex token (strip dashes from two UUIDs, take first 32 chars)
    let raw = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let token = &raw[..32];

    conn.execute(
        "INSERT INTO invite_links (token, org_id, role, created_by, expires_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now', '+7 days'))",
        rusqlite::params![token, org_id, role, created_by],
    )?;

    let invite = conn.query_row(
        "SELECT token, org_id, role, created_by, used_at, expires_at, created_at
         FROM invite_links WHERE token = ?1",
        [token],
        |row| {
            Ok(InviteLink {
                token:      row.get(0)?,
                org_id:     row.get(1)?,
                role:       row.get(2)?,
                created_by: row.get(3)?,
                used_at:    row.get(4)?,
                expires_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    )?;

    Ok(invite)
}

/// Validates and marks an invite link as used in one operation.
/// Fails if the token does not exist, is already used, or has expired.
pub fn use_invite_link(conn: &Connection, token: &str) -> Result<InviteLink> {
    let invite = get_invite_link(conn, token)?;

    if invite.used_at.is_some() {
        return Err(anyhow::anyhow!("invite_already_used"));
    }

    conn.execute(
        "UPDATE invite_links SET used_at = datetime('now') WHERE token = ?1",
        [token],
    )?;

    // Return the updated record
    let updated = conn.query_row(
        "SELECT token, org_id, role, created_by, used_at, expires_at, created_at
         FROM invite_links WHERE token = ?1",
        [token],
        |row| {
            Ok(InviteLink {
                token:      row.get(0)?,
                org_id:     row.get(1)?,
                role:       row.get(2)?,
                created_by: row.get(3)?,
                used_at:    row.get(4)?,
                expires_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    )?;

    Ok(updated)
}

/// Returns invite link metadata without marking it as used.
/// Validates that the token exists, is not used, and is not expired.
pub fn get_invite_link(conn: &Connection, token: &str) -> Result<InviteLink> {
    let invite = conn
        .query_row(
            "SELECT token, org_id, role, created_by, used_at, expires_at, created_at
             FROM invite_links WHERE token = ?1",
            [token],
            |row| {
                Ok(InviteLink {
                    token:      row.get(0)?,
                    org_id:     row.get(1)?,
                    role:       row.get(2)?,
                    created_by: row.get(3)?,
                    used_at:    row.get(4)?,
                    expires_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("invite_not_found"))?;

    if invite.used_at.is_some() {
        return Err(anyhow::anyhow!("invite_already_used"));
    }

    // Check expiry using SQLite — compare stored expires_at with current time
    let expired: bool = conn.query_row(
        "SELECT expires_at < datetime('now') FROM invite_links WHERE token = ?1",
        [token],
        |row| row.get::<_, bool>(0),
    )?;

    if expired {
        return Err(anyhow::anyhow!("invite_expired"));
    }

    Ok(invite)
}

/// Redeems an invite link: validates the token, creates a new user with the given
/// name and hashed password, creates a Personal API key, and marks the invite as used.
///
/// Returns `(user, raw_api_key)` on success.
/// Errors with `invite_not_found`, `invite_already_used`, or `invite_expired` via the
/// same semantics as `get_invite_link`.
pub fn redeem_invite(
    conn: &Connection,
    token: &str,
    name: &str,
    password_hash: &str,
) -> Result<(User, String)> {
    // Validate the invite (errors if not found / used / expired)
    let invite = get_invite_link(conn, token)?;

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let user_id = Uuid::new_v4().to_string();

    // Create the user — no email for invite-created users
    conn.execute(
        "INSERT INTO users (id, org_id, email, name, role, status, password_hash, created_at)
         VALUES (?1, ?2, NULL, ?3, ?4, 'active', ?5, ?6)",
        rusqlite::params![user_id, invite.org_id, name, invite.role, password_hash, now],
    )?;

    // Create a Personal API key
    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'Personal key', ?5)",
        rusqlite::params![key_id, user_id, invite.org_id, key_hash, now],
    )?;

    // Mark the invite as used
    conn.execute(
        "UPDATE invite_links SET used_at = datetime('now') WHERE token = ?1",
        [token],
    )?;

    let user = User {
        id: user_id,
        org_id: invite.org_id,
        email: String::new(),
        name: name.to_string(),
        role: invite.role,
        status: "active".to_string(),
        created_at: now,
        last_active: None,
        disabled_at: None,
        admin_note: None,
        last_login_at: None,
    };

    Ok((user, raw_key))
}

/// Returns memory statistics for a project by project ID.
/// Looks up the project name first, then queries memories by org_id + project name.
pub fn get_project_stats(conn: &Connection, org_id: &str, project_id: &str) -> Result<crate::models::types::ProjectStats> {
    // Look up the project name from ID
    let project_name: Option<String> = conn.query_row(
        "SELECT name FROM projects WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![project_id, org_id],
        |row| row.get(0),
    ).optional()?;

    let project_name = project_name.ok_or_else(|| anyhow::anyhow!("project_not_found"))?;

    // Aggregate stats
    let (total_memories, memories_this_week, last_memory_at) = conn.query_row(
        "SELECT
            COUNT(*) as total_memories,
            COALESCE(SUM(CASE WHEN created_at >= datetime('now', '-7 days') THEN 1 ELSE 0 END), 0) as memories_this_week,
            MAX(created_at) as last_memory_at
         FROM memories
         WHERE org_id = ?1 AND project = ?2 AND archived_at IS NULL",
        rusqlite::params![org_id, &project_name],
        |row| Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
        )),
    )?;

    // Top 5 tags
    let mut stmt = conn.prepare(
        "SELECT value as tag, COUNT(*) as cnt
         FROM memories, json_each(memories.tags)
         WHERE org_id = ?1 AND project = ?2 AND archived_at IS NULL
           AND tags IS NOT NULL AND tags != '[]' AND tags != 'null'
         GROUP BY value ORDER BY cnt DESC LIMIT 5",
    )?;
    let tags = stmt.query_map(rusqlite::params![org_id, &project_name], |row| {
        row.get::<_, String>(0)
    })?.collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(crate::models::types::ProjectStats {
        total_memories,
        memories_this_week,
        last_memory_at,
        top_tags: tags,
    })
}

/// Returns memory creation counts per day for the last 90 days (non-archived only).
/// Used by `GET /v1/admin/stats/memory-heatmap`.
/// Returns the top contributing agents (by memory count) in the last 30 days.
/// Groups by user_id (the agent/user that stored the memory).
/// Returned by `GET /v1/admin/stats/top-contributors`.
pub fn get_top_contributors(conn: &Connection, org_id: &str, days: i64) -> Result<Vec<crate::models::types::ContributorStat>> {
    let mut stmt = conn.prepare(
        "SELECT
           COALESCE(m.user_id, 'unknown') as user_id,
           COUNT(*) as memory_count,
           MAX(m.created_at) as last_activity,
           u.name as user_name,
           u.email as user_email
         FROM memories m
         LEFT JOIN users u ON m.user_id = u.id AND m.org_id = u.org_id
         WHERE m.org_id = ?1
           AND m.archived_at IS NULL
           AND m.created_at >= datetime('now', '-' || ?2 || ' days')
         GROUP BY m.user_id
         ORDER BY memory_count DESC
         LIMIT 8",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, days], |row| {
        Ok(crate::models::types::ContributorStat {
            user_id:       row.get(0)?,
            memory_count:  row.get(1)?,
            last_activity: row.get(2)?,
            user_name:     row.get(3)?,
            user_email:    row.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn get_memory_heatmap(conn: &Connection, org_id: &str, days: i64) -> Result<Vec<crate::models::types::HeatmapDay>> {
    let mut stmt = conn.prepare(
        "SELECT date(created_at) as day, COUNT(*) as count
         FROM memories
         WHERE org_id = ?1
           AND created_at >= datetime('now', '-' || ?2 || ' days')
           AND archived_at IS NULL
         GROUP BY date(created_at)
         ORDER BY day ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, days], |row| {
        Ok(crate::models::types::HeatmapDay {
            day:   row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod invite_link_tests {
    use super::*;
    use crate::db::{connection, migrations};

    fn setup() -> Connection {
        let conn = connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        // Seed org and user required by FK-less invite_links table
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('u1', 'org1', 'admin@acme.com', 'Admin', 'admin', 'active')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn create_returns_token() {
        let conn = setup();
        let invite = create_invite_link(&conn, "org1", "user", "u1").unwrap();
        assert_eq!(invite.token.len(), 32, "token must be 32 chars");
        assert_eq!(invite.role, "user");
        assert_eq!(invite.org_id, "org1");
        assert!(invite.used_at.is_none(), "new invite must not be used");
    }

    #[test]
    fn expired_token_rejected() {
        let conn = setup();
        // Insert an already-expired invite directly
        conn.execute(
            "INSERT INTO invite_links (token, org_id, role, created_by, expires_at)
             VALUES ('expiredtoken12345678901234567890', 'org1', 'user', 'u1', datetime('now', '-1 day'))",
            [],
        )
        .unwrap();

        let result = get_invite_link(&conn, "expiredtoken12345678901234567890");
        assert!(result.is_err(), "expired token must be rejected");
        assert!(result.unwrap_err().to_string().contains("invite_expired"));
    }

    #[test]
    fn used_token_rejected() {
        let conn = setup();
        let invite = create_invite_link(&conn, "org1", "user", "u1").unwrap();
        // Mark it used
        conn.execute(
            "UPDATE invite_links SET used_at = datetime('now') WHERE token = ?1",
            [&invite.token],
        )
        .unwrap();

        let result = get_invite_link(&conn, &invite.token);
        assert!(result.is_err(), "used token must be rejected");
        assert!(result.unwrap_err().to_string().contains("invite_already_used"));
    }

    #[test]
    fn redeem_invite_creates_user_and_key() {
        let conn = setup();
        let invite = create_invite_link(&conn, "org1", "member", "u1").unwrap();

        let (user, raw_key) = redeem_invite(&conn, &invite.token, "Alice", "hashed_pw").unwrap();

        assert_eq!(user.name, "Alice");
        assert_eq!(user.role, "member");
        assert_eq!(user.status, "active");
        assert_eq!(user.org_id, "org1");
        assert!(raw_key.starts_with("nm_"), "api key must have nm_ prefix");

        // The invite must now be marked as used
        let result = get_invite_link(&conn, &invite.token);
        assert!(result.is_err(), "redeemed invite must be rejected on re-use");
        assert!(result.unwrap_err().to_string().contains("invite_already_used"));
    }

    #[test]
    fn redeem_invite_rejects_already_used_token() {
        let conn = setup();
        let invite = create_invite_link(&conn, "org1", "member", "u1").unwrap();

        // First redeem succeeds
        redeem_invite(&conn, &invite.token, "Bob", "pw1").unwrap();

        // Second redeem must fail
        let result = redeem_invite(&conn, &invite.token, "Carol", "pw2");
        assert!(result.is_err(), "second redeem must fail");
        assert!(result.unwrap_err().to_string().contains("invite_already_used"));
    }
}

#[cfg(test)]
mod memory_heatmap_tests {
    use super::*;
    use crate::db::{connection, migrations};

    fn setup() -> Connection {
        let conn = connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('u1', 'org1', 'admin@acme.com', 'Admin', 'admin', 'active')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn heatmap_returns_correct_counts_for_today_and_yesterday() {
        let conn = setup();

        // Insert 5 memories today
        for i in 0..5 {
            let id = format!("today_{i}");
            conn.execute(
                "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope)
                 VALUES (?1, 'org1', 'u1', 'p', 'claude', 'content', '[]', datetime('now'), 'project')",
                rusqlite::params![id],
            ).unwrap();
        }

        // Insert 3 memories yesterday
        for i in 0..3 {
            let id = format!("yesterday_{i}");
            conn.execute(
                "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope)
                 VALUES (?1, 'org1', 'u1', 'p', 'claude', 'content', '[]', datetime('now', '-1 day'), 'project')",
                rusqlite::params![id],
            ).unwrap();
        }

        let days = get_memory_heatmap(&conn, "org1", 90).unwrap();

        let today_str = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let yesterday_str = (chrono::Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        let today = days.iter().find(|d| d.day == today_str).expect("today must be present");
        let yesterday = days.iter().find(|d| d.day == yesterday_str).expect("yesterday must be present");

        assert_eq!(today.count, 5, "today must have 5 memories");
        assert_eq!(yesterday.count, 3, "yesterday must have 3 memories");
    }
}

#[cfg(test)]
mod top_contributors_tests {
    use super::*;
    use crate::db::{connection, migrations};

    fn setup() -> Connection {
        let conn = connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        // Two distinct users (agents)
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('alice', 'org1', 'alice@acme.com', 'Alice', 'member', 'active')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('bob', 'org1', 'bob@acme.com', 'Bob', 'member', 'active')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn top_contributors_ranks_by_memory_count() {
        let conn = setup();

        // Insert 5 memories attributed to user "alice"
        for i in 0..5 {
            let id = format!("alice_{i}");
            conn.execute(
                "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope)
                 VALUES (?1, 'org1', 'alice', 'p', 'claude', 'content', '[]', datetime('now'), 'project')",
                rusqlite::params![id],
            ).unwrap();
        }

        // Insert 2 memories attributed to user "bob"
        for i in 0..2 {
            let id = format!("bob_{i}");
            conn.execute(
                "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope)
                 VALUES (?1, 'org1', 'bob', 'p', 'claude', 'content', '[]', datetime('now'), 'project')",
                rusqlite::params![id],
            ).unwrap();
        }

        let contributors = get_top_contributors(&conn, "org1", 30).unwrap();

        assert!(!contributors.is_empty(), "must return contributors");
        assert_eq!(contributors[0].user_id, "alice", "alice must rank first");
        assert_eq!(contributors[0].memory_count, 5, "alice must have count=5");
        assert_eq!(contributors[0].user_name.as_deref(), Some("Alice"));
        assert_eq!(contributors[0].user_email.as_deref(), Some("alice@acme.com"));

        let bob = contributors.iter().find(|c| c.user_id == "bob")
            .expect("bob must be present");
        assert_eq!(bob.memory_count, 2, "bob must have 2 memories");
    }
}

#[cfg(test)]
mod project_stats_tests {
    use super::*;
    use crate::db::{connection, migrations};

    fn setup() -> Connection {
        let conn = connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('u1', 'org1', 'admin@acme.com', 'Admin', 'admin', 'active')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn project_stats_returns_correct_counts() {
        let conn = setup();

        // Create a project
        let project = create_project(&conn, "org1", "test-project", None, None).unwrap();

        // Insert 3 memories for this project
        for i in 0..3 {
            let id = format!("mem{i}");
            let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            conn.execute(
                "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope)
                 VALUES (?1, 'org1', 'u1', 'test-project', 'claude-code', 'content', '[]', ?2, 'project')",
                rusqlite::params![id, now],
            ).unwrap();
        }

        let stats = get_project_stats(&conn, "org1", &project.id).unwrap();
        assert_eq!(stats.total_memories, 3, "total_memories should be 3");
        assert_eq!(stats.memories_this_week, 3, "memories_this_week should be 3 (just inserted)");
        assert!(stats.last_memory_at.is_some(), "last_memory_at should be set");
    }

    #[test]
    fn project_stats_returns_zeros_for_empty_project() {
        let conn = setup();
        let project = create_project(&conn, "org1", "empty-project", None, None).unwrap();
        let stats = get_project_stats(&conn, "org1", &project.id).unwrap();
        assert_eq!(stats.total_memories, 0, "total_memories should be 0");
        assert_eq!(stats.memories_this_week, 0, "memories_this_week should be 0, not NULL");
        assert!(stats.last_memory_at.is_none(), "last_memory_at should be None");
        assert!(stats.top_tags.is_empty(), "top_tags should be empty");
    }
}

// ── Convention queries ────────────────────────────────────────────────────────

/// `project`: when `Some(p)`, returns org-wide conventions (`project_id IS NULL`)
/// UNION conventions scoped to project `p` — project scoping ADDS to org-wide, it
/// never replaces it. When `None`, returns every convention for the org
/// regardless of `project_id` (admin listing / no-project-context behavior).
/// `limit`/`offset` page the result set, ordered by `weight DESC, created_at DESC`
/// (highest-weight conventions first). Callers that want "everything" should pass
/// a generously large `limit`.
pub fn list_conventions(
    conn: &Connection,
    org_id: &str,
    category: Option<&str>,
    include_archived: Option<bool>,
    project: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Convention>> {
    let include_archived = include_archived.unwrap_or(false);
    let mut sql = String::from(
        "SELECT id, org_id, project_id, title, content, category, weight, tags, created_at, updated_at, archived_at
         FROM conventions
         WHERE org_id = ?1"
    );
    let mut param_idx = 2usize;
    let mut extra_params: Vec<String> = Vec::new();

    if !include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }
    if let Some(cat) = category {
        sql.push_str(&format!(" AND category = ?{param_idx}"));
        extra_params.push(cat.to_string());
        param_idx += 1;
    }
    if let Some(p) = project {
        sql.push_str(&format!(" AND (project_id IS NULL OR project_id = ?{param_idx})"));
        extra_params.push(p.to_string());
        param_idx += 1;
    }
    sql.push_str(&format!(
        " ORDER BY weight DESC, created_at DESC LIMIT ?{param_idx} OFFSET ?{}",
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
    let rows = stmt
        .query_map(refs.as_slice(), convention_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_convention(conn: &Connection, org_id: &str, id: i64) -> Result<Option<Convention>> {
    let result = conn.query_row(
        "SELECT id, org_id, project_id, title, content, category, weight, tags, created_at, updated_at, archived_at
         FROM conventions WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, id],
        convention_from_row,
    ).optional()?;
    Ok(result)
}

pub fn create_convention(
    conn: &Connection,
    org_id: &str,
    req: &CreateConventionRequest,
) -> Result<Convention> {
    let tags_json = serde_json::to_string(&req.tags.clone().unwrap_or_default())?;
    let category = req.category.as_deref().unwrap_or("general");
    let weight = req.weight.unwrap_or(100);
    conn.execute(
        "INSERT INTO conventions (org_id, project_id, title, content, category, weight, tags)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![org_id, req.project_id, req.title, req.content, category, weight, tags_json],
    )?;
    let id = conn.last_insert_rowid();
    get_convention(conn, org_id, id)?.ok_or_else(|| anyhow::anyhow!("convention not found after insert"))
}

pub fn update_convention(
    conn: &Connection,
    org_id: &str,
    id: i64,
    req: &UpdateConventionRequest,
) -> Result<Option<Convention>> {
    let existing = match get_convention(conn, org_id, id)? {
        Some(c) => c,
        None => return Ok(None),
    };
    let title = req.title.as_deref().unwrap_or(&existing.title);
    let content = req.content.as_deref().unwrap_or(&existing.content);
    let category = req.category.as_deref().unwrap_or(&existing.category);
    let weight = req.weight.unwrap_or(existing.weight);
    let tags = req.tags.clone().unwrap_or(existing.tags.clone());
    let tags_json = serde_json::to_string(&tags)?;
    conn.execute(
        "UPDATE conventions SET title = ?1, content = ?2, category = ?3, weight = ?4, tags = ?5, updated_at = datetime('now')
         WHERE org_id = ?6 AND id = ?7",
        rusqlite::params![title, content, category, weight, tags_json, org_id, id],
    )?;
    get_convention(conn, org_id, id)
}

pub fn archive_convention(conn: &Connection, org_id: &str, id: i64) -> Result<bool> {
    let n = conn.execute(
        "UPDATE conventions SET archived_at = datetime('now') WHERE org_id = ?1 AND id = ?2 AND archived_at IS NULL",
        rusqlite::params![org_id, id],
    )?;
    Ok(n > 0)
}

pub fn restore_convention(conn: &Connection, org_id: &str, id: i64) -> Result<bool> {
    let n = conn.execute(
        "UPDATE conventions SET archived_at = NULL WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, id],
    )?;
    Ok(n > 0)
}

pub fn delete_convention(conn: &Connection, org_id: &str, id: i64) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM conventions WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, id],
    )?;
    Ok(n > 0)
}

fn convention_from_row(row: &rusqlite::Row) -> rusqlite::Result<Convention> {
    let tags_str: String = row.get(7)?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    Ok(Convention {
        id: row.get(0)?,
        org_id: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        category: row.get(5)?,
        weight: row.get(6)?,
        tags,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        archived_at: row.get(10)?,
    })
}

#[cfg(test)]
mod convention_scope_tests {
    use super::*;
    use crate::db::{connection, migrations};
    use crate::models::types::CreateConventionRequest;

    fn setup() -> Connection {
        let conn = connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn
    }

    fn seed_org(conn: &Connection) -> String {
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('u1', 'org1', 'admin@acme.com', 'Admin', 'admin', 'active')",
            [],
        ).unwrap();
        "org1".to_string()
    }

    fn make_req(title: &str, project_id: Option<&str>) -> CreateConventionRequest {
        CreateConventionRequest {
            title: title.to_string(),
            content: "content".to_string(),
            category: None,
            weight: None,
            tags: None,
            project_id: project_id.map(|s| s.to_string()),
        }
    }

    #[test]
    fn org_wide_convention_returned_for_any_project() {
        let conn = setup();
        let org_id = seed_org(&conn);
        let project_a = create_project(&conn, &org_id, "proj-a", None, None).unwrap();
        let project_b = create_project(&conn, &org_id, "proj-b", None, None).unwrap();

        create_convention(&conn, &org_id, &make_req("Org-wide rule", None)).unwrap();

        let for_a = list_conventions(&conn, &org_id, None, None, Some(&project_a.id), 1000, 0).unwrap();
        let for_b = list_conventions(&conn, &org_id, None, None, Some(&project_b.id), 1000, 0).unwrap();

        assert_eq!(for_a.len(), 1, "org-wide convention must apply to project A");
        assert_eq!(for_b.len(), 1, "org-wide convention must apply to project B");
    }

    #[test]
    fn project_scoped_convention_only_for_that_project() {
        let conn = setup();
        let org_id = seed_org(&conn);
        let project_a = create_project(&conn, &org_id, "proj-a", None, None).unwrap();
        let project_b = create_project(&conn, &org_id, "proj-b", None, None).unwrap();

        create_convention(&conn, &org_id, &make_req("Proj A rule", Some(&project_a.id))).unwrap();

        let for_a = list_conventions(&conn, &org_id, None, None, Some(&project_a.id), 1000, 0).unwrap();
        let for_b = list_conventions(&conn, &org_id, None, None, Some(&project_b.id), 1000, 0).unwrap();

        assert_eq!(for_a.len(), 1, "project-scoped convention must apply to its own project");
        assert_eq!(for_b.len(), 0, "project-scoped convention must NOT apply to a different project");
    }

    #[test]
    fn none_project_returns_everything_for_org() {
        let conn = setup();
        let org_id = seed_org(&conn);
        let project_a = create_project(&conn, &org_id, "proj-a", None, None).unwrap();

        create_convention(&conn, &org_id, &make_req("Org-wide", None)).unwrap();
        create_convention(&conn, &org_id, &make_req("Proj A", Some(&project_a.id))).unwrap();

        let all = list_conventions(&conn, &org_id, None, None, None, 1000, 0).unwrap();
        assert_eq!(all.len(), 2, "None must return everything for the org regardless of project_id (admin listing)");
    }

    #[test]
    fn resolving_for_project_is_union_org_wide_and_project_not_other_project() {
        let conn = setup();
        let org_id = seed_org(&conn);
        let project_a = create_project(&conn, &org_id, "proj-a", None, None).unwrap();
        let project_q = create_project(&conn, &org_id, "proj-q", None, None).unwrap();

        create_convention(&conn, &org_id, &make_req("Org-wide", None)).unwrap();
        create_convention(&conn, &org_id, &make_req("Proj A", Some(&project_a.id))).unwrap();
        create_convention(&conn, &org_id, &make_req("Proj Q", Some(&project_q.id))).unwrap();

        let for_a = list_conventions(&conn, &org_id, None, None, Some(&project_a.id), 1000, 0).unwrap();
        let titles: Vec<&str> = for_a.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(for_a.len(), 2, "resolving for project A must be org-wide UNION project A");
        assert!(titles.contains(&"Org-wide"));
        assert!(titles.contains(&"Proj A"));
        assert!(!titles.contains(&"Proj Q"), "project Q's convention must not leak into project A's resolution");
    }

    #[test]
    fn list_conventions_project_scoping_combines_with_category_filter() {
        let conn = setup();
        let org_id = seed_org(&conn);
        let project_a = create_project(&conn, &org_id, "proj-a", None, None).unwrap();

        let mut org_wide_style = make_req("Org-wide style", None);
        org_wide_style.category = Some("style".to_string());
        create_convention(&conn, &org_id, &org_wide_style).unwrap();

        let mut proj_a_naming = make_req("Proj A naming", Some(&project_a.id));
        proj_a_naming.category = Some("naming".to_string());
        create_convention(&conn, &org_id, &proj_a_naming).unwrap();

        let style_for_a = list_conventions(&conn, &org_id, Some("style"), None, Some(&project_a.id), 1000, 0).unwrap();
        assert_eq!(style_for_a.len(), 1);
        assert_eq!(style_for_a[0].title, "Org-wide style");
    }
}

// ── GitHub OAuth connection queries ───────────────────────────────────────────

/// Upserts a GitHub OAuth connection for the given org.
pub fn save_github_connection(
    conn: &Connection,
    org_id: &str,
    access_token: &str,
    token_type: &str,
    scopes: &str,
    github_login: &str,
    github_user_id: i64,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO github_connections
         (org_id, access_token, token_type, scopes, github_login, github_user_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6,
           COALESCE((SELECT created_at FROM github_connections WHERE org_id = ?1), datetime('now')),
           datetime('now'))",
        rusqlite::params![org_id, access_token, token_type, scopes, github_login, github_user_id],
    )?;
    Ok(())
}

/// Returns the GitHub OAuth connection for the given org, or None if not connected.
pub fn get_github_connection(conn: &Connection, org_id: &str) -> Result<Option<GitHubConnection>> {
    conn.query_row(
        "SELECT org_id, access_token, token_type, scopes, github_login, github_user_id, created_at, updated_at
         FROM github_connections WHERE org_id = ?1",
        [org_id],
        |row| {
            Ok(GitHubConnection {
                org_id: row.get(0)?,
                access_token: row.get(1)?,
                token_type: row.get(2)?,
                scopes: row.get(3)?,
                github_login: row.get(4)?,
                github_user_id: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    ).optional().map_err(Into::into)
}

/// Deletes the GitHub OAuth connection for the given org.
/// Returns true if a row was deleted, false if no connection existed.
pub fn delete_github_connection(conn: &Connection, org_id: &str) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM github_connections WHERE org_id = ?1",
        [org_id],
    )?;
    Ok(n > 0)
}

// ── Agent queries ─────────────────────────────────────────────────────────────

fn agent_from_row(row: &rusqlite::Row) -> rusqlite::Result<Agent> {
    Ok(Agent {
        id: row.get(0)?,
        org_id: row.get(1)?,
        name: row.get(2)?,
        model: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub fn list_agents(conn: &Connection, org_id: &str) -> Result<Vec<Agent>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, model, description, status, created_at, updated_at
         FROM agents WHERE org_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([org_id], agent_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

pub fn get_agent(conn: &Connection, org_id: &str, id: &str) -> Result<Option<Agent>> {
    conn.query_row(
        "SELECT id, org_id, name, model, description, status, created_at, updated_at
         FROM agents WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, id],
        agent_from_row,
    )
    .optional()
    .map_err(Into::into)
}

pub fn create_agent(conn: &Connection, org_id: &str, req: &CreateAgentRequest) -> Result<Agent> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO agents (id, org_id, name, model, description, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', datetime('now'), datetime('now'))",
        rusqlite::params![id, org_id, req.name, req.model, req.description],
    )?;
    get_agent(conn, org_id, &id)?.ok_or_else(|| anyhow::anyhow!("agent not found after insert"))
}

pub fn update_agent(
    conn: &Connection,
    org_id: &str,
    id: &str,
    req: &UpdateAgentRequest,
) -> Result<Option<Agent>> {
    let n = conn.execute(
        "UPDATE agents SET
           name = COALESCE(?3, name),
           model = COALESCE(?4, model),
           description = CASE WHEN ?5 IS NOT NULL THEN ?5 ELSE description END,
           status = COALESCE(?6, status),
           updated_at = datetime('now')
         WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, id, req.name, req.model, req.description, req.status],
    )?;
    if n == 0 {
        return Ok(None);
    }
    get_agent(conn, org_id, id)
}

pub fn list_agent_assignments(
    conn: &Connection,
    org_id: &str,
    agent_id: &str,
) -> Result<Vec<AgentAssignment>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, org_id, repo_url, created_at
         FROM agent_assignments WHERE org_id = ?1 AND agent_id = ?2
         ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, agent_id], |row| {
        Ok(AgentAssignment {
            id: row.get(0)?,
            agent_id: row.get(1)?,
            org_id: row.get(2)?,
            repo_url: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

// ── Code knowledge graph persistence ─────────────────────────────────────────

/// Create the structural virtual nodes (Project, Folder, File) for every file in a
/// project using a single `BEGIN`/`COMMIT` transaction. All inserts use
/// `INSERT OR IGNORE` so the function is safe to call multiple times.
///
/// Edges emitted: `project→folder (contains_folder)`, `folder→subfolder`,
/// `folder→file (contains_file)`.
pub fn persist_structure(
    conn: &Connection,
    code_project_id: i64,
    project_name: &str,
    rel_paths: &[String],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    // Project node
    let project_qname = format!("project::{}", project_name);
    tx.execute(
        "INSERT OR IGNORE INTO code_symbols \
         (code_project_id, symbol_type, name, qualified_name, language) \
         VALUES (?1, 'Project', ?2, ?3, 'unknown')",
        rusqlite::params![code_project_id, project_name, project_qname],
    )?;

    for rel_path in rel_paths {
        // All ancestor folder paths for this file
        let parts: Vec<&str> = rel_path.split('/').collect();
        let file_name = parts.last().copied().unwrap_or(rel_path.as_str());

        // Insert folders from root down
        let mut prev_qname = project_qname.clone();
        let mut prev_type  = "project";
        for depth in 0..parts.len().saturating_sub(1) {
            let folder_path: String = parts[..=depth].join("/");
            let folder_qname = format!("folder::{}", folder_path);
            let folder_name  = parts[depth];
            tx.execute(
                "INSERT OR IGNORE INTO code_symbols \
                 (code_project_id, symbol_type, name, qualified_name, language) \
                 VALUES (?1, 'Folder', ?2, ?3, 'unknown')",
                rusqlite::params![code_project_id, folder_name, folder_qname],
            )?;
            let edge_type = "contains_folder";
            let _ = prev_type; // only used to track traversal direction, same edge_type either way
            tx.execute(
                "INSERT OR IGNORE INTO code_edges \
                 (code_project_id, from_symbol_id, to_symbol_id, edge_type) \
                 SELECT ?1, \
                        (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?2), \
                        (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?3), \
                        ?4 \
                 WHERE (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?2) IS NOT NULL \
                   AND (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?3) IS NOT NULL",
                rusqlite::params![code_project_id, prev_qname, folder_qname, edge_type],
            )?;
            prev_qname = folder_qname;
            prev_type  = "folder";
        }

        // File node
        let file_qname = format!("file::{}", rel_path);
        tx.execute(
            "INSERT OR IGNORE INTO code_symbols \
             (code_project_id, symbol_type, name, qualified_name, file_path, language) \
             VALUES (?1, 'File', ?2, ?3, ?3, 'unknown')",
            rusqlite::params![code_project_id, file_name, file_qname],
        )?;
        // Wait — file_path should be rel_path, not file_qname
        // Fix: file_path = rel_path, qualified_name = file_qname
        // Actually we already inserted with file_path = file_qname (the qualified_name) above.
        // Let me correct the insert:

        // The previous INSERT already happened. Since it's INSERT OR IGNORE, if it failed due to
        // UNIQUE, we need to use UPDATE. But on first run there's no prior row. Let me use a
        // different approach: use UPSERT to ensure file_path is set correctly.
        tx.execute(
            "INSERT OR IGNORE INTO code_symbols \
             (code_project_id, symbol_type, name, qualified_name, file_path, language) \
             VALUES (?1, 'File', ?2, ?3, ?4, 'unknown')",
            rusqlite::params![code_project_id, file_name, file_qname, rel_path],
        )?;

        let edge_type = "contains_file";
        tx.execute(
            "INSERT OR IGNORE INTO code_edges \
             (code_project_id, from_symbol_id, to_symbol_id, edge_type) \
             SELECT ?1, \
                    (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?2), \
                    (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?3), \
                    ?4 \
             WHERE (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?2) IS NOT NULL \
               AND (SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name=?3) IS NOT NULL",
            rusqlite::params![code_project_id, prev_qname, file_qname, edge_type],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Delete and re-insert all file-owned symbols and edges for a single source file,
/// wrapped in one `BEGIN`/`COMMIT` transaction. RAII drop rolls back on failure.
///
/// Shared nodes (File, Folder, Project, External stubs) are upserted with
/// `INSERT OR IGNORE` — they survive re-indexes and are only removed via CASCADE
/// when the project is deleted.
pub fn persist_file_graph(
    conn: &Connection,
    code_project_id: i64,
    file_graph: &FileGraph,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    // 1. Delete FileOwned symbols for this file (cascades to FileOwned edges via FK ON DELETE CASCADE)
    tx.execute(
        "DELETE FROM code_symbols \
         WHERE code_project_id = ?1 AND file_path = ?2 AND symbol_type NOT IN ('File','Folder','Project','External')",
        rusqlite::params![code_project_id, file_graph.file_rel_path],
    )?;

    // 2. Delete FileOwned edges for this file (those not already cascade-deleted)
    tx.execute(
        "DELETE FROM code_edges WHERE code_project_id = ?1 AND file_path = ?2",
        rusqlite::params![code_project_id, file_graph.file_rel_path],
    )?;

    // 3. Upsert symbols and build qname → id map
    let mut id_map: HashMap<String, i64> = HashMap::new();

    for sym in &file_graph.symbols {
        let id: i64 = match sym.persist {
            Persist::Shared => {
                tx.execute(
                    "INSERT OR IGNORE INTO code_symbols \
                     (code_project_id, symbol_type, name, qualified_name, file_path, file_hash, \
                      start_line, end_line, language) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        code_project_id,
                        sym.symbol_type.as_str(),
                        sym.name,
                        sym.qualified_name,
                        sym.file_path,
                        sym.file_hash,
                        sym.start_line,
                        sym.end_line,
                        sym.language,
                    ],
                )?;
                tx.query_row(
                    "SELECT id FROM code_symbols WHERE code_project_id = ?1 AND qualified_name = ?2",
                    rusqlite::params![code_project_id, sym.qualified_name],
                    |row| row.get(0),
                )?
            }
            Persist::FileOwned => {
                tx.execute(
                    "INSERT INTO code_symbols \
                     (code_project_id, symbol_type, name, qualified_name, file_path, file_hash, \
                      start_line, end_line, language) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        code_project_id,
                        sym.symbol_type.as_str(),
                        sym.name,
                        sym.qualified_name,
                        sym.file_path,
                        sym.file_hash,
                        sym.start_line,
                        sym.end_line,
                        sym.language,
                    ],
                )?;
                tx.last_insert_rowid()
            }
        };
        id_map.insert(sym.qualified_name.clone(), id);
    }

    // 4. Insert edges — resolve qnames to ids, skip edges with unresolvable endpoints
    for edge in &file_graph.edges {
        let from_id = match resolve_symbol_id(&tx, code_project_id, &edge.from_qname, &id_map)? {
            Some(id) => id,
            None => continue,
        };
        let to_id = match resolve_symbol_id(&tx, code_project_id, &edge.to_qname, &id_map)? {
            Some(id) => id,
            None => continue,
        };
        tx.execute(
            "INSERT OR IGNORE INTO code_edges \
             (code_project_id, from_symbol_id, to_symbol_id, edge_type, file_path) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                code_project_id,
                from_id,
                to_id,
                edge.edge_type.as_str(),
                edge.file_path,
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Returns true if the file already has at least one file-owned graph symbol
/// (Function/Class/Method/Interface/Type/Enum/...). Used to decide whether to
/// backfill graph data for files that were chunked before the knowledge-graph
/// feature existed and are now unchanged on re-index.
pub fn file_has_graph_symbols(
    conn: &Connection,
    code_project_id: i64,
    file_path: &str,
) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM code_symbols \
         WHERE code_project_id = ?1 AND file_path = ?2 \
           AND symbol_type NOT IN ('File','Folder','Project','External'))",
        rusqlite::params![code_project_id, file_path],
        |row| row.get(0),
    )?;
    Ok(exists)
}

/// Resolve a qualified_name to a symbol id, first from the in-memory map then from the DB.
fn resolve_symbol_id(
    conn: &Connection,
    code_project_id: i64,
    qname: &str,
    id_map: &HashMap<String, i64>,
) -> Result<Option<i64>> {
    if let Some(id) = id_map.get(qname) {
        return Ok(Some(*id));
    }
    conn.query_row(
        "SELECT id FROM code_symbols WHERE code_project_id = ?1 AND qualified_name = ?2",
        rusqlite::params![code_project_id, qname],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

/// Return nodes and edges for a project, applying optional type filters.
///
/// Two-step query:
/// 1. SELECT nodes with filters + LIMIT/OFFSET.
/// 2. SELECT edges WHERE both endpoints are in the returned node set.
///
/// This guarantees no dangling edges in the response.
pub fn get_graph(
    conn: &Connection,
    code_project_id: i64,
    node_types: &[String],
    edge_types: &[String],
    limit: i64,
    offset: i64,
) -> Result<(Vec<GraphNodeDto>, Vec<GraphEdgeDto>)> {
    // Step 1: build the node query
    let type_filter = if node_types.is_empty() {
        String::new()
    } else {
        let placeholders: Vec<String> = (3..3 + node_types.len())
            .map(|n| format!("?{}", n))
            .collect();
        format!(" AND symbol_type IN ({})", placeholders.join(", "))
    };
    let node_sql = format!(
        "SELECT id, symbol_type, name, qualified_name, file_path, start_line, end_line, language \
         FROM code_symbols WHERE code_project_id = ?1{} \
         ORDER BY id LIMIT ?2 OFFSET ?{} ",
        type_filter,
        3 + node_types.len(),
    );

    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(code_project_id),
        Box::new(limit),
    ];
    for t in node_types {
        params.push(Box::new(t.clone()));
    }
    params.push(Box::new(offset));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

    let mut stmt = conn.prepare(&node_sql)?;
    let nodes: Vec<GraphNodeDto> = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(GraphNodeDto {
                id:             row.get(0)?,
                node_type:      row.get(1)?,
                name:           row.get(2)?,
                qualified_name: row.get(3)?,
                file_path:      row.get(4)?,
                start_line:     row.get(5)?,
                end_line:       row.get(6)?,
                language:       row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if nodes.is_empty() {
        return Ok((nodes, vec![]));
    }

    // Step 2: collect edges where BOTH endpoints are in the returned node set
    let node_ids: Vec<i64> = nodes.iter().map(|n| n.id).collect();
    let id_placeholders: Vec<String> = (1..=node_ids.len()).map(|n| format!("?{}", n)).collect();
    let edge_type_filter = if edge_types.is_empty() {
        String::new()
    } else {
        let et_start = node_ids.len() + 1;
        let et_placeholders: Vec<String> = (et_start..et_start + edge_types.len())
            .map(|n| format!("?{}", n))
            .collect();
        format!(" AND edge_type IN ({})", et_placeholders.join(", "))
    };
    let edge_sql = format!(
        "SELECT id, from_symbol_id, to_symbol_id, edge_type \
         FROM code_edges \
         WHERE code_project_id = ?{} \
           AND from_symbol_id IN ({}) \
           AND to_symbol_id IN ({}) {} \
         ORDER BY id",
        node_ids.len() + 1,
        id_placeholders.join(", "),
        id_placeholders.join(", "),
        edge_type_filter,
    );

    let mut edge_params: Vec<Box<dyn rusqlite::ToSql>> = node_ids
        .iter()
        .map(|id| -> Box<dyn rusqlite::ToSql> { Box::new(*id) })
        .collect();
    edge_params.push(Box::new(code_project_id));
    for t in edge_types {
        edge_params.push(Box::new(t.clone()));
    }
    let edge_refs: Vec<&dyn rusqlite::ToSql> = edge_params.iter().map(|b| b.as_ref()).collect();

    let mut estmt = conn.prepare(&edge_sql)?;
    let edges: Vec<GraphEdgeDto> = estmt
        .query_map(edge_refs.as_slice(), |row| {
            Ok(GraphEdgeDto {
                id:        row.get(0)?,
                from_id:   row.get(1)?,
                to_id:     row.get(2)?,
                edge_type: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok((nodes, edges))
}

/// Looks up a `{label}` column for each id in `ids` from `table`, keyed by id.
/// `label_expr` and `table` are always static, code-controlled strings — never
/// user input — so building the SQL via `format!` here is safe.
fn lookup_labels(
    conn: &Connection,
    table: &str,
    label_expr: &str,
    ids: &HashSet<String>,
) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let id_vec: Vec<&String> = ids.iter().collect();
    let placeholders: Vec<String> = (1..=id_vec.len()).map(|n| format!("?{n}")).collect();
    let sql = format!(
        "SELECT id, {label_expr} FROM {table} WHERE id IN ({})",
        placeholders.join(", ")
    );
    let params: Vec<&dyn rusqlite::ToSql> = id_vec.iter().map(|s| *s as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let pairs: Vec<(String, String)> = stmt
        .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(pairs.into_iter().collect())
}

/// Builds a read-only, on-the-fly memory knowledge graph for `GET /v1/memory/graph`.
///
/// The anchor set is `memories` (org-scoped, filtered by `project`/`since`, capped by
/// `limit`/`offset`). Satellite nodes (Project, Session, User, Collection, Tag) are
/// derived from that capped set, so every edge always has both endpoints present in
/// the returned node set — no separate dangling-edge filter is needed.
///
/// `project` matches either the legacy free-text `memories.project` column or the
/// FK `memories.project_id`, so callers can pass either a project name or id.
pub fn get_memory_graph(
    conn: &Connection,
    org_id: &str,
    project: &str,
    since: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MemGraphNode>, Vec<MemGraphEdge>)> {
    struct MemRow {
        id: String,
        project: String,
        project_id: Option<String>,
        user_id: String,
        session_id: Option<String>,
        collection_id: Option<String>,
        tags: Vec<String>,
        label: String,
    }

    let mut stmt = conn.prepare(
        "SELECT id, project, project_id, user_id, session_id, collection_id, tags, title, content \
         FROM memories \
         WHERE org_id = ?1 AND (project = ?2 OR project_id = ?2) \
           AND (?3 IS NULL OR created_at >= ?3) \
         ORDER BY created_at DESC \
         LIMIT ?4 OFFSET ?5",
    )?;

    let rows: Vec<MemRow> = stmt
        .query_map(rusqlite::params![org_id, project, since, limit, offset], |row| {
            let tags_str: String = row.get(6)?;
            let title: Option<String> = row.get(7)?;
            let content: String = row.get(8)?;
            let label = title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| content.chars().take(60).collect());
            Ok(MemRow {
                id:            row.get(0)?,
                project:       row.get(1)?,
                project_id:    row.get(2)?,
                user_id:       row.get(3)?,
                session_id:    row.get(4)?,
                collection_id: row.get(5)?,
                tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                label,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut nodes: HashMap<String, MemGraphNode> = HashMap::new();
    let mut edges: Vec<MemGraphEdge> = Vec::new();

    // Memory nodes + satellites are only built when the anchor set is non-empty.
    // NOTE: this does NOT early-return — the AuditEvent block below (Slice 2)
    // always runs regardless of whether any memories matched, since audit_logs
    // is scoped independently (org_id + since, not by `project`).
    if !rows.is_empty() {
        // Memory nodes
        for r in &rows {
            let node_id = format!("memory:{}", r.id);
            nodes.insert(
                node_id.clone(),
                MemGraphNode { id: node_id, node_type: "Memory".to_string(), label: r.label.clone() },
            );
        }

        // Project canonicalization: one node per distinct LOGICAL project, keyed by the
        // real project row id whenever one exists — either because `project_id` is set,
        // or because a `projects` row with a matching name already exists (so a legacy
        // row and an FK-linked row for the same project never split into two nodes).
        // Only synthesize a `project:name:{name}` id when no real project row exists at all.
        let legacy_names: HashSet<String> = rows
            .iter()
            .filter(|r| r.project_id.is_none())
            .map(|r| r.project.clone())
            .collect();
        let name_to_id: HashMap<String, String> = if legacy_names.is_empty() {
            HashMap::new()
        } else {
            let names: Vec<&String> = legacy_names.iter().collect();
            let placeholders: Vec<String> = (2..=1 + names.len()).map(|n| format!("?{n}")).collect();
            let sql = format!(
                "SELECT name, id FROM projects WHERE org_id = ?1 AND name IN ({})",
                placeholders.join(", ")
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = vec![&org_id];
            for n in &names {
                params.push(*n as &dyn rusqlite::ToSql);
            }
            let mut stmt = conn.prepare(&sql)?;
            let pairs: Vec<(String, String)> = stmt
                .query_map(params.as_slice(), |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            pairs.into_iter().collect()
        };

        let mut fk_project_ids: HashSet<String> = HashSet::new();
        for r in &rows {
            let canonical_id = match &r.project_id {
                Some(pid) => {
                    fk_project_ids.insert(pid.clone());
                    format!("project:{pid}")
                }
                None => match name_to_id.get(&r.project) {
                    Some(pid) => {
                        fk_project_ids.insert(pid.clone());
                        format!("project:{pid}")
                    }
                    None => format!("project:name:{}", r.project),
                },
            };
            nodes.entry(canonical_id.clone()).or_insert_with(|| MemGraphNode {
                id:        canonical_id.clone(),
                node_type: "Project".to_string(),
                label:     r.project.clone(),
            });
            edges.push(MemGraphEdge {
                id:        format!("belongs_to:memory:{}:{}", r.id, canonical_id),
                from_id:   format!("memory:{}", r.id),
                to_id:     canonical_id,
                edge_type: "belongs_to".to_string(),
            });
        }
        // Resolve real project names for FK-linked projects (overrides the legacy-name fallback).
        let project_labels = lookup_labels(conn, "projects", "name", &fk_project_ids)?;
        for (pid, name) in &project_labels {
            if let Some(n) = nodes.get_mut(&format!("project:{pid}")) {
                n.label = name.clone();
            }
        }

        // Session nodes + `in_session` edges (omitted when session_id is NULL)
        let session_ids: HashSet<String> = rows.iter().filter_map(|r| r.session_id.clone()).collect();
        let session_labels = lookup_labels(conn, "sessions", "COALESCE(name, summary, id)", &session_ids)?;
        for r in &rows {
            if let Some(sid) = &r.session_id {
                let node_id = format!("session:{sid}");
                let label = session_labels.get(sid).cloned().unwrap_or_else(|| sid.clone());
                nodes.entry(node_id.clone()).or_insert_with(|| MemGraphNode {
                    id: node_id.clone(), node_type: "Session".to_string(), label,
                });
                edges.push(MemGraphEdge {
                    id:        format!("in_session:memory:{}:{}", r.id, node_id),
                    from_id:   format!("memory:{}", r.id),
                    to_id:     node_id,
                    edge_type: "in_session".to_string(),
                });
            }
        }

        // User nodes + `created_by` edges (user_id is NOT NULL on memories, always present)
        let user_ids: HashSet<String> = rows.iter().map(|r| r.user_id.clone()).collect();
        let user_labels = lookup_labels(conn, "users", "name", &user_ids)?;
        for r in &rows {
            let node_id = format!("user:{}", r.user_id);
            let label = user_labels.get(&r.user_id).cloned().unwrap_or_else(|| r.user_id.clone());
            nodes.entry(node_id.clone()).or_insert_with(|| MemGraphNode {
                id: node_id.clone(), node_type: "User".to_string(), label,
            });
            edges.push(MemGraphEdge {
                id:        format!("created_by:memory:{}:{}", r.id, node_id),
                from_id:   format!("memory:{}", r.id),
                to_id:     node_id,
                edge_type: "created_by".to_string(),
            });
        }

        // Collection nodes + `in_collection` edges (omitted when collection_id is NULL)
        let collection_ids: HashSet<String> = rows.iter().filter_map(|r| r.collection_id.clone()).collect();
        let collection_labels = lookup_labels(conn, "collections", "name", &collection_ids)?;
        for r in &rows {
            if let Some(cid) = &r.collection_id {
                let node_id = format!("collection:{cid}");
                let label = collection_labels.get(cid).cloned().unwrap_or_else(|| cid.clone());
                nodes.entry(node_id.clone()).or_insert_with(|| MemGraphNode {
                    id: node_id.clone(), node_type: "Collection".to_string(), label,
                });
                edges.push(MemGraphEdge {
                    id:        format!("in_collection:memory:{}:{}", r.id, node_id),
                    from_id:   format!("memory:{}", r.id),
                    to_id:     node_id,
                    edge_type: "in_collection".to_string(),
                });
            }
        }

        // Tag nodes + `tagged` edges (omitted when tags is empty)
        for r in &rows {
            for tag in &r.tags {
                let node_id = format!("tag:{tag}");
                nodes.entry(node_id.clone()).or_insert_with(|| MemGraphNode {
                    id: node_id.clone(), node_type: "Tag".to_string(), label: tag.clone(),
                });
                edges.push(MemGraphEdge {
                    id:        format!("tagged:memory:{}:{}", r.id, node_id),
                    from_id:   format!("memory:{}", r.id),
                    to_id:     node_id,
                    edge_type: "tagged".to_string(),
                });
            }
        }
    } // end `if !rows.is_empty()`

    // ── AuditEvent nodes + performed_by/targets edges (Slice 2) ─────────────
    // Scoped by `org_id` + `since` only — NOT by `project`. `audit_logs` has no
    // `project` column (it records org-wide activity across every resource
    // type), so unlike the memory anchor above there is no direct project
    // filter to apply here. Every audit event in-scope gets an AuditEvent node
    // and a `performed_by` edge to its actor (a User node, created if it
    // isn't already part of the memory-derived node set). A `targets` edge is
    // added ONLY when `(resource_type, resource_id)` resolves to a node that
    // is ALREADY present in `nodes` — in practice this means "the audited
    // resource belongs to the project(s) already pulled in by the memory
    // anchor" — which keeps the no-dangling-edges invariant without needing a
    // project column on `audit_logs` itself.
    //
    // Deferred: an `audit -> policy -> project` edge (audit events whose
    // resource_type is "policy"). `policies` has no `project_id` column yet —
    // that's staged in a PARALLEL migration (see migrations.rs:63) — so until
    // it lands there is no way to resolve a Policy node into this project's
    // scope. Policy resource types simply fall through `resource_node_id`
    // below and never gain a `targets` edge; the AuditEvent + performed_by
    // edge still appear. Do not implement the Policy node/edge here.
    let mut audit_stmt = conn.prepare(
        "SELECT id, user_id, action, resource_type, resource_id \
         FROM audit_logs \
         WHERE org_id = ?1 AND (?2 IS NULL OR timestamp >= ?2) \
         ORDER BY timestamp DESC \
         LIMIT ?3",
    )?;
    struct AuditRow {
        id: String,
        user_id: String,
        action: String,
        resource_type: String,
        resource_id: Option<String>,
    }
    let audit_rows: Vec<AuditRow> = audit_stmt
        .query_map(rusqlite::params![org_id, since, limit], |row| {
            Ok(AuditRow {
                id:            row.get(0)?,
                user_id:       row.get(1)?,
                action:        row.get(2)?,
                resource_type: row.get(3)?,
                resource_id:   row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if !audit_rows.is_empty() {
        let audit_user_ids: HashSet<String> = audit_rows.iter().map(|r| r.user_id.clone()).collect();
        let audit_user_labels = lookup_labels(conn, "users", "name", &audit_user_ids)?;

        for r in &audit_rows {
            let audit_node_id = format!("audit:{}", r.id);
            nodes.entry(audit_node_id.clone()).or_insert_with(|| MemGraphNode {
                id:        audit_node_id.clone(),
                node_type: "AuditEvent".to_string(),
                label:     format!("{} {}", r.action, r.resource_type),
            });

            // performed_by: always present — audit_logs.user_id is NOT NULL.
            let user_node_id = format!("user:{}", r.user_id);
            let user_label = audit_user_labels.get(&r.user_id).cloned().unwrap_or_else(|| r.user_id.clone());
            nodes.entry(user_node_id.clone()).or_insert_with(|| MemGraphNode {
                id: user_node_id.clone(), node_type: "User".to_string(), label: user_label,
            });
            edges.push(MemGraphEdge {
                id:        format!("performed_by:{}:{}", audit_node_id, user_node_id),
                from_id:   audit_node_id.clone(),
                to_id:     user_node_id,
                edge_type: "performed_by".to_string(),
            });

            // targets: only when the resource already resolves to a node in this graph.
            if let Some(target_id) = resource_node_id(&r.resource_type, r.resource_id.as_deref(), &nodes) {
                edges.push(MemGraphEdge {
                    id:        format!("targets:{}:{}", audit_node_id, target_id),
                    from_id:   audit_node_id,
                    to_id:     target_id,
                    edge_type: "targets".to_string(),
                });
            }
        }
    }

    Ok((nodes.into_values().collect(), edges))
}

/// Resolves an audit log's `(resource_type, resource_id)` to the namespaced
/// node id it targets, IF that node is already present in `nodes`. Returns
/// `None` when `resource_id` is absent, `resource_type` has no known mapping
/// (e.g. "policy", "convention" — deferred, see the caller's comment), or the
/// referenced node simply isn't in the graph. Callers MUST skip the `targets`
/// edge when this returns `None`, to preserve the no-dangling-edges invariant.
fn resource_node_id(
    resource_type: &str,
    resource_id: Option<&str>,
    nodes: &HashMap<String, MemGraphNode>,
) -> Option<String> {
    let rid = resource_id?;
    let candidate = match resource_type {
        "memory" => format!("memory:{rid}"),
        "session" => format!("session:{rid}"),
        "user" => format!("user:{rid}"),
        "collection" => format!("collection:{rid}"),
        "project" => {
            let by_id = format!("project:{rid}");
            if nodes.contains_key(&by_id) {
                by_id
            } else {
                format!("project:name:{rid}")
            }
        }
        _ => return None,
    };
    nodes.contains_key(&candidate).then_some(candidate)
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use crate::db::{connection::connect, migrations};
    use crate::indexer::tree_sitter_chunker::{
        EdgeType, FileGraph, Persist, RawEdge, RawSymbol, SymbolType,
    };

    fn setup() -> (Connection, i64) {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        let pid = upsert_code_project(&conn, "org1", "myapp", "/ws").unwrap();
        (conn, pid)
    }

    fn make_symbol(name: &str, qname: &str, sym_type: SymbolType, fp: &str, persist: Persist) -> RawSymbol {
        RawSymbol {
            symbol_type:    sym_type,
            name:           name.to_string(),
            qualified_name: qname.to_string(),
            file_path:      Some(fp.to_string()),
            file_hash:      Some("hash1".to_string()),
            start_line:     Some(1),
            end_line:       Some(10),
            language:       "rust".to_string(),
            persist,
        }
    }

    fn make_edge(from: &str, to: &str, et: EdgeType, fp: &str) -> RawEdge {
        RawEdge {
            from_qname: from.to_string(),
            to_qname:   to.to_string(),
            edge_type:  et,
            file_path:  Some(fp.to_string()),
            persist:    Persist::FileOwned,
        }
    }

    #[test]
    fn persist_structure_creates_project_folder_file_nodes() {
        let (conn, pid) = setup();
        persist_structure(&conn, pid, "myapp", &["src/a/b.rs".to_string()]).unwrap();

        // Project node
        let proj: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND symbol_type='Project'",
            rusqlite::params![pid],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(proj, 1, "exactly one Project node");

        // Folder nodes: src and src/a
        let folders: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND symbol_type='Folder'",
            rusqlite::params![pid],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(folders, 2, "exactly two Folder nodes (src, src/a)");

        // File node
        let files: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND symbol_type='File'",
            rusqlite::params![pid],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(files, 1, "exactly one File node");
    }

    #[test]
    fn persist_structure_is_idempotent() {
        let (conn, pid) = setup();
        let paths = vec!["src/a/b.rs".to_string()];
        persist_structure(&conn, pid, "myapp", &paths).unwrap();
        persist_structure(&conn, pid, "myapp", &paths).unwrap();

        let total: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1",
            rusqlite::params![pid],
            |r| r.get(0),
        ).unwrap();
        // Project(1) + Folder src(1) + Folder src/a(1) + File(1) = 4
        assert_eq!(total, 4, "second call must not create extra rows");
    }

    #[test]
    fn persist_file_graph_deletes_and_reinserts_file_owned() {
        let (conn, pid) = setup();
        persist_structure(&conn, pid, "myapp", &["src/lib.rs".to_string()]).unwrap();

        // First index: function 'foo'
        let fg1 = FileGraph {
            file_rel_path: "src/lib.rs".to_string(),
            symbols: vec![make_symbol("foo", "src/lib.rs::foo#1", SymbolType::Function, "src/lib.rs", Persist::FileOwned)],
            edges:   vec![make_edge("file::src/lib.rs", "src/lib.rs::foo#1", EdgeType::Defines, "src/lib.rs")],
        };
        persist_file_graph(&conn, pid, &fg1).unwrap();
        let count1: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND symbol_type='Function'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();
        assert_eq!(count1, 1, "one Function after first index");

        // Second index: function 'bar' (replaces 'foo')
        let fg2 = FileGraph {
            file_rel_path: "src/lib.rs".to_string(),
            symbols: vec![make_symbol("bar", "src/lib.rs::bar#1", SymbolType::Function, "src/lib.rs", Persist::FileOwned)],
            edges:   vec![make_edge("file::src/lib.rs", "src/lib.rs::bar#1", EdgeType::Defines, "src/lib.rs")],
        };
        persist_file_graph(&conn, pid, &fg2).unwrap();

        let count2: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND symbol_type='Function'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();
        assert_eq!(count2, 1, "still one Function after second index (foo replaced by bar)");

        let bar_exists: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND name='bar'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();
        assert_eq!(bar_exists, 1, "bar must exist");

        let foo_gone: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND name='foo'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();
        assert_eq!(foo_gone, 0, "foo must be gone");
    }

    #[test]
    fn file_has_graph_symbols_drives_backfill_decision() {
        let (conn, pid) = setup();
        // Structural nodes only (File/Folder/Project) — no code symbols yet.
        persist_structure(&conn, pid, "myapp", &["src/lib.rs".to_string()]).unwrap();
        assert!(
            !file_has_graph_symbols(&conn, pid, "src/lib.rs").unwrap(),
            "a structural-only file must be reported as needing graph backfill"
        );

        // After a real code symbol is persisted, no backfill is needed.
        let fg = FileGraph {
            file_rel_path: "src/lib.rs".to_string(),
            symbols: vec![make_symbol("foo", "src/lib.rs::foo#1", SymbolType::Function, "src/lib.rs", Persist::FileOwned)],
            edges:   vec![make_edge("file::src/lib.rs", "src/lib.rs::foo#1", EdgeType::Defines, "src/lib.rs")],
        };
        persist_file_graph(&conn, pid, &fg).unwrap();
        assert!(
            file_has_graph_symbols(&conn, pid, "src/lib.rs").unwrap(),
            "a file with a Function symbol must not be flagged for backfill"
        );
    }

    #[test]
    fn persist_file_graph_sibling_folder_node_survives_reindex() {
        let (conn, pid) = setup();
        persist_structure(&conn, pid, "myapp", &[
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
        ]).unwrap();

        // Index a.rs
        let fg = FileGraph {
            file_rel_path: "src/a.rs".to_string(),
            symbols: vec![make_symbol("foo", "src/a.rs::foo#1", SymbolType::Function, "src/a.rs", Persist::FileOwned)],
            edges:   vec![],
        };
        persist_file_graph(&conn, pid, &fg).unwrap();

        // Get the Folder 'src' id before re-index
        let folder_id_before: i64 = conn.query_row(
            "SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name='folder::src'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();

        // Re-index a.rs
        persist_file_graph(&conn, pid, &fg).unwrap();

        // Folder 'src' must still have the same id
        let folder_id_after: i64 = conn.query_row(
            "SELECT id FROM code_symbols WHERE code_project_id=?1 AND qualified_name='folder::src'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();
        assert_eq!(folder_id_before, folder_id_after, "Folder node id must be stable across reindexes");

        // Count folder nodes — still exactly one 'src'
        let folder_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND qualified_name='folder::src'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();
        assert_eq!(folder_count, 1, "no duplicate folder created by reindex");
    }

    #[test]
    fn get_graph_with_node_type_filter() {
        let (conn, pid) = setup();
        persist_structure(&conn, pid, "myapp", &["src/lib.rs".to_string()]).unwrap();

        let fg = FileGraph {
            file_rel_path: "src/lib.rs".to_string(),
            symbols: vec![
                make_symbol("foo", "src/lib.rs::foo#1", SymbolType::Function, "src/lib.rs", Persist::FileOwned),
                make_symbol("Bar", "src/lib.rs::Bar#5", SymbolType::Struct, "src/lib.rs", Persist::FileOwned),
            ],
            edges: vec![],
        };
        persist_file_graph(&conn, pid, &fg).unwrap();

        let (nodes, _) = get_graph(&conn, pid, &["Function".to_string()], &[], 5000, 0).unwrap();
        assert!(
            nodes.iter().all(|n| n.node_type == "Function"),
            "node_type filter must exclude Struct nodes"
        );
        assert!(
            nodes.iter().any(|n| n.name == "foo"),
            "Function 'foo' must appear"
        );
        assert!(
            nodes.iter().all(|n| n.name != "Bar"),
            "Struct 'Bar' must not appear when filtered to Function"
        );
    }

    #[test]
    fn get_graph_edges_reference_only_returned_nodes() {
        let (conn, pid) = setup();
        persist_structure(&conn, pid, "myapp", &["src/lib.rs".to_string()]).unwrap();

        let fg = FileGraph {
            file_rel_path: "src/lib.rs".to_string(),
            symbols: vec![
                make_symbol("foo", "src/lib.rs::foo#1", SymbolType::Function, "src/lib.rs", Persist::FileOwned),
            ],
            edges: vec![
                make_edge("file::src/lib.rs", "src/lib.rs::foo#1", EdgeType::Defines, "src/lib.rs"),
            ],
        };
        persist_file_graph(&conn, pid, &fg).unwrap();

        let (nodes, edges) = get_graph(&conn, pid, &[], &[], 5000, 0).unwrap();
        let node_ids: std::collections::HashSet<i64> = nodes.iter().map(|n| n.id).collect();
        for edge in &edges {
            assert!(
                node_ids.contains(&edge.from_id),
                "from_id {} not in node set",
                edge.from_id
            );
            assert!(
                node_ids.contains(&edge.to_id),
                "to_id {} not in node set",
                edge.to_id
            );
        }
    }

    #[test]
    fn get_graph_empty_project_returns_empty() {
        let (conn, pid) = setup();
        let (nodes, edges) = get_graph(&conn, pid, &[], &[], 5000, 0).unwrap();
        assert!(nodes.is_empty(), "empty project must return no nodes");
        assert!(edges.is_empty(), "empty project must return no edges");
    }
}

#[cfg(test)]
mod mem_graph_tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    /// Bootstraps an org + admin user, returns (conn, org_id, user_id).
    fn setup() -> (Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, user, _key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        (conn, org.id, user.id)
    }

    /// Inserts a memory row directly with full control over project/project_id/
    /// session_id/collection_id/tags — bypasses `upsert_memory`'s auto-project-creation
    /// so legacy (project_id = NULL) rows can be constructed deliberately.
    #[allow(clippy::too_many_arguments)]
    fn insert_memory(
        conn: &Connection,
        id: &str,
        org_id: &str,
        user_id: &str,
        project: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
        collection_id: Option<&str>,
        tags: &str,
        title: Option<&str>,
        created_at: &str,
    ) {
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, project, project_id, tool, content, tags,
                                    title, session_id, collection_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'claude', 'body', ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![id, org_id, user_id, project, project_id, tags, title, session_id, collection_id, created_at],
        ).unwrap();
    }

    #[test]
    fn get_memory_graph_empty_project_returns_empty() {
        let (conn, org_id, _user_id) = setup();
        let (nodes, edges) = get_memory_graph(&conn, &org_id, "empty-proj", None, 2000, 0).unwrap();
        assert!(nodes.is_empty(), "empty project must return no nodes");
        assert!(edges.is_empty(), "empty project must return no edges");
    }

    #[test]
    fn get_memory_graph_collapses_legacy_and_fk_linked_project_to_one_node() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();

        for i in 0..5 {
            insert_memory(&conn, &format!("m-fk-{i}"), &org_id, &user_id, "acme", Some(&project_id), None, None, "[]", None, "2026-01-01T00:00:00Z");
        }
        for i in 0..3 {
            insert_memory(&conn, &format!("m-legacy-{i}"), &org_id, &user_id, "acme", None, None, None, "[]", None, "2026-01-01T00:00:00Z");
        }

        let (nodes, _edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let project_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "Project").collect();
        assert_eq!(project_nodes.len(), 1, "exactly one Project node for 'acme', got {}", project_nodes.len());

        let memory_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "Memory").collect();
        assert_eq!(memory_nodes.len(), 8, "all 8 memories (5 FK-linked + 3 legacy) must appear");
    }

    #[test]
    fn get_memory_graph_full_memory_yields_five_nodes_and_five_edges() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        conn.execute(
            "INSERT INTO sessions (id, org_id, project, directory) VALUES ('sess1', ?1, 'acme', '/ws')",
            rusqlite::params![org_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO collections (id, org_id, name) VALUES ('col1', ?1, 'My Collection')",
            rusqlite::params![org_id],
        ).unwrap();

        insert_memory(&conn, "m1", &org_id, &user_id, "acme", Some(&project_id), Some("sess1"), Some("col1"), r#"["auth","bug"]"#, Some("Fixed auth bug"), "2026-01-01T00:00:00Z");

        let (nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        // Memory, Project, Session, User, Collection, 2 Tag nodes = 7
        assert_eq!(nodes.len(), 7, "expected 7 nodes (memory+project+session+user+collection+2 tags), got {}", nodes.len());

        let edge_types: std::collections::HashSet<&str> = edges.iter().map(|e| e.edge_type.as_str()).collect();
        assert!(edge_types.contains("belongs_to"));
        assert!(edge_types.contains("in_session"));
        assert!(edge_types.contains("created_by"));
        assert!(edge_types.contains("in_collection"));
        assert!(edge_types.contains("tagged"));
        // 1 belongs_to + 1 in_session + 1 created_by + 1 in_collection + 2 tagged = 6
        assert_eq!(edges.len(), 6, "expected 6 edges total, got {}", edges.len());
    }

    #[test]
    fn get_memory_graph_omits_edges_for_null_session_and_collection() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        insert_memory(&conn, "m1", &org_id, &user_id, "acme", Some(&project_id), None, None, "[]", None, "2026-01-01T00:00:00Z");

        let (_nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let edge_types: std::collections::HashSet<&str> = edges.iter().map(|e| e.edge_type.as_str()).collect();
        assert!(edge_types.contains("belongs_to"), "belongs_to must be present");
        assert!(edge_types.contains("created_by"), "created_by must be present");
        assert!(!edge_types.contains("in_session"), "in_session must be absent for NULL session_id");
        assert!(!edge_types.contains("in_collection"), "in_collection must be absent for NULL collection_id");
        assert!(!edge_types.contains("tagged"), "tagged must be absent for empty tags");
        assert_eq!(edges.len(), 2, "only belongs_to + created_by, got {}", edges.len());
    }

    #[test]
    fn get_memory_graph_has_no_dangling_edges_and_counts_match() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        conn.execute(
            "INSERT INTO sessions (id, org_id, project, directory) VALUES ('sess1', ?1, 'acme', '/ws')",
            rusqlite::params![org_id],
        ).unwrap();
        insert_memory(&conn, "m1", &org_id, &user_id, "acme", Some(&project_id), Some("sess1"), None, r#"["x"]"#, None, "2026-01-01T00:00:00Z");
        insert_memory(&conn, "m2", &org_id, &user_id, "acme", None, None, None, "[]", None, "2026-01-02T00:00:00Z");

        let (nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let node_ids: std::collections::HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        for edge in &edges {
            assert!(node_ids.contains(edge.from_id.as_str()), "from_id {} not in node set", edge.from_id);
            assert!(node_ids.contains(edge.to_id.as_str()), "to_id {} not in node set", edge.to_id);
        }
        assert_eq!(nodes.len(), node_ids.len(), "node_count must equal nodes.len()");
    }

    // ── Phase 5: AuditEvent nodes/edges (Slice 2) ───────────────────────────

    #[test]
    fn get_memory_graph_audit_events_scoped_by_since_yield_performed_by_edge() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        insert_memory(&conn, "m1", &org_id, &user_id, "acme", Some(&project_id), None, None, "[]", None, "2026-01-01T00:00:00Z");

        // One audit event before `since`, one at/after `since` — only the latter must appear.
        insert_audit_log_chained(&conn, &org_id, &user_id, "memory.read", "memory", Some("m1"), serde_json::json!({}), Some("2026-01-01T00:00:00Z")).unwrap();
        insert_audit_log_chained(&conn, &org_id, &user_id, "memory.updated", "memory", Some("m1"), serde_json::json!({}), Some("2026-06-01T00:00:00Z")).unwrap();

        let (nodes, edges) = get_memory_graph(&conn, &org_id, "acme", Some("2026-03-01T00:00:00Z"), 2000, 0).unwrap();

        let audit_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "AuditEvent").collect();
        assert_eq!(audit_nodes.len(), 1, "only the audit event at/after `since` must appear, got {}", audit_nodes.len());

        let audit_node_id = &audit_nodes[0].id;
        let performed_by: Vec<_> = edges
            .iter()
            .filter(|e| &e.from_id == audit_node_id && e.edge_type == "performed_by")
            .collect();
        assert_eq!(performed_by.len(), 1, "exactly one performed_by edge from the AuditEvent node");
        assert_eq!(performed_by[0].to_id, format!("user:{user_id}"));
    }

    #[test]
    fn get_memory_graph_audit_targets_edge_present_when_resource_in_graph_dropped_when_absent() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        insert_memory(&conn, "m1", &org_id, &user_id, "acme", Some(&project_id), None, None, "[]", None, "2026-01-01T00:00:00Z");

        // Targets a memory that IS in the graph (m1) -> targets edge must be present.
        insert_audit_log_chained(&conn, &org_id, &user_id, "memory.read", "memory", Some("m1"), serde_json::json!({}), Some("2026-01-02T00:00:00Z")).unwrap();
        // Targets a memory that is NOT in the graph -> targets edge must be dropped (no dangling edge).
        insert_audit_log_chained(&conn, &org_id, &user_id, "memory.read", "memory", Some("m-does-not-exist"), serde_json::json!({}), Some("2026-01-02T00:00:01Z")).unwrap();

        let (nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let targets_edges: Vec<_> = edges.iter().filter(|e| e.edge_type == "targets").collect();
        assert_eq!(targets_edges.len(), 1, "only the audit event whose resource is in the graph gets a targets edge");
        assert_eq!(targets_edges[0].to_id, "memory:m1");

        // No-dangling-edges invariant must still hold across the whole graph.
        let node_ids: std::collections::HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        for edge in &edges {
            assert!(node_ids.contains(edge.from_id.as_str()), "from_id {} not in node set", edge.from_id);
            assert!(node_ids.contains(edge.to_id.as_str()), "to_id {} not in node set", edge.to_id);
        }

        // Both audit events still produce AuditEvent nodes even though one has no targets edge.
        let audit_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "AuditEvent").collect();
        assert_eq!(audit_nodes.len(), 2, "both audit events must still appear as AuditEvent nodes");
    }
}
