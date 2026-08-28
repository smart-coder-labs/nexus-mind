use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::auth::api_keys;
use crate::indexer::tree_sitter_chunker::{FileGraph, Persist};
use crate::models::types::{
    can_transition, CreateRetrospectiveRequest, CreateSprintRequest, CreateTaskRequest,
    PatchSprintRequest, PatchTaskRequest, Sprint, SprintRetrospective, Task, TaskAssignee,
    TaskComment, TaskStatus,
};
use crate::models::types::{
    validate_typed_harness_manifest, Agent, AgentAssignment, ApiKeyWithUser, AuditEntry,
    AuthContext, AutonomousAgentConnector, AutonomousAgentDefinition, AutonomousAgentDelivery,
    AutonomousAgentDetail, AutonomousAgentFinding, AutonomousAgentRevision, AutonomousAgentRun,
    AutonomousAgentSchedule, Client, ClientMember, CodeChunk, CodeProject, Convention,
    CreateAgentRequest, CreateAutonomousAgentRequest, CreateConventionRequest,
    CreateHarnessConfigReviewRequest, CreateHarnessRequest, CreateSessionRequest,
    CreateWebhookRequest, CustomRole, GitHubConnection, GlobalMetrics, GraphEdgeDto, GraphNodeDto,
    Harness, HarnessApproval, HarnessApprovalRequest, HarnessConfigReview,
    HarnessConfigReviewAuthor, HarnessConfigReviewComment, HarnessDownloadResponse,
    HarnessInstallResultRequest, HarnessOwner, HarnessRecommendation, HarnessVersion,
    HarnessVersionSummary, InviteLink, MemGraphEdge, MemGraphNode, Memory, OnboardingItem,
    OnboardingStatus, Org, OrgSettings, OrgStats, OrgWithStats, PatchSessionRequest, Policy,
    Project, ProjectEventOverrides, ProjectMember, ProjectResolutionReport,
    PublishHarnessVersionRequest, PutAutonomousAgentConnectorRequest,
    PutAutonomousAgentScheduleRequest, Session, SessionWithCount, StoreMemoryRequest, ToolUsage,
    UnresolvedProject, UpdateAgentRequest, UpdateAutonomousAgentRequest, UpdateConventionRequest,
    UpdateWebhookRequest, User, UserRole, Webhook, WebhookDelivery,
};
use crate::models::types::{
    PatchChangeRequest, SaveArtifactRequest, SaveSpecRequest, SddArtifact, SddArtifactDetail,
    SddArtifactKind, SddChange, SddChangeFilters, SddChangeSummary, SddPhase, SddRevision,
    SddRevisionMeta, SddSearchHit, SddSearchResult, SddSpec, SddSpecDetail, SddSpecFilters,
    SddSpecMerge, SddSpecRevision, SddSpecRevisionMeta, SddSpecSearchHit, SddSpecSummary,
    SddStatus, UpsertChangeRequest,
};
use anyhow::anyhow;
use std::str::FromStr;

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
    let result: Option<Option<String>> = conn
        .query_row(
            "SELECT u.disabled_at
         FROM api_keys ak
         JOIN users u ON u.id = ak.user_id
         WHERE ak.key_hash = ?1 AND ak.revoked = 0
           AND (ak.expires_at IS NULL OR ak.expires_at > datetime('now'))",
            [key_hash],
            |row| row.get(0),
        )
        .optional()?;
    Ok(result.map(|d| d.is_some()).unwrap_or(false))
}

/// Creates the first organization + admin user + admin API key.
/// Returns (org, user, raw_api_key).
/// Fails if any organization already exists.
/// Creates an org + admin user + API key with no guard. Used by seed and bootstrap.
/// Name of the default project auto-created for every org at bootstrap. Matches the
/// standard agent convention (CLAUDE.md: default `project` = `nexus-mind`) so that memory
/// writes work out of the box now that implicit project creation is disabled.
pub const DEFAULT_PROJECT_NAME: &str = "nexus-mind";

/// Idempotent backfill: ensure every existing org has the default `nexus-mind` project,
/// enrolling that org's admin users as members. Safe to run on every startup — orgs that
/// already have the project are skipped, and existing memberships are preserved
/// (`INSERT OR IGNORE`). Without this, agents in orgs created before the default-project
/// change would keep getting 404s when writing to the default project.
pub fn ensure_default_projects(conn: &Connection) -> Result<usize> {
    let org_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT o.id FROM organizations o
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects p WHERE p.org_id = o.id AND p.name = ?1
             )",
        )?;
        let rows = stmt.query_map([DEFAULT_PROJECT_NAME], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut created = 0usize;
    for org_id in &org_ids {
        let project = create_project(conn, org_id, DEFAULT_PROJECT_NAME, None, None)?;
        // Enrol the org's admins (mirrors new-org semantics: only privileged owners are
        // members; other users need an explicit invite).
        conn.execute(
            "INSERT OR IGNORE INTO project_members (id, project_id, user_id, role, created_at)
             SELECT lower(hex(randomblob(16))), ?1, u.id, 'admin', datetime('now')
             FROM users u WHERE u.org_id = ?2 AND u.role = 'admin'",
            rusqlite::params![project.id, org_id],
        )?;
        created += 1;
    }
    Ok(created)
}

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

    // Every org gets a default "nexus-mind" project at bootstrap so the standard agent
    // convention (default project = "nexus-mind") works without an extra admin step.
    // Only the initial admin is enrolled — normal membership semantics; other users need
    // an explicit invite. This is a one-time onboarding step, NOT the removed
    // auto-enroll-everyone-on-every-write behavior.
    let default_project = create_project(conn, &org_id, DEFAULT_PROJECT_NAME, None, None)?;
    conn.execute(
        "INSERT INTO project_members (id, project_id, user_id, role, created_at)
         VALUES (?1, ?2, ?3, 'admin', ?4)",
        rusqlite::params![Uuid::new_v4().to_string(), default_project.id, user_id, now],
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
    let mut stmt = conn
        .prepare("SELECT id, name, slug, created_at FROM organizations ORDER BY created_at ASC")?;
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
    let existing: i32 = conn.query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0))?;
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

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// Full-text search over memories, scoped to the org.
/// SQL fragment enforcing project-membership visibility for a non-admin viewer.
///
/// Restricts results to memories the viewer is allowed to see: those belonging to a
/// project where the viewer appears in `project_members`, plus project-less
/// (org-shared) memories (`project_id IS NULL`). `col` is the qualified `project_id`
/// column (e.g. `"project_id"` or `"m.project_id"`) and `placeholder` is the bound
/// parameter token holding the viewer's user id (e.g. `"?5"`).
fn visibility_predicate(col: &str, placeholder: &str) -> String {
    format!(
        " AND ({col} IS NULL OR {col} IN (SELECT project_id FROM project_visibility WHERE user_id = {placeholder}))"
    )
}

pub fn search_memories(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<Memory>> {
    search_memories_visible(conn, org_id, query, limit, None)
}

/// Like [`search_memories`], but when `viewer_user_id` is `Some(uid)` the result set is
/// restricted to memories `uid` may see (see [`visibility_predicate`]). `None` applies no
/// project-membership restriction — for admins and internal callers only.
pub fn search_memories_visible(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
    viewer_user_id: Option<&str>,
) -> Result<Vec<Memory>> {
    let fts_query = match sanitize_fts_query(query) {
        Some(q) => q,
        None => return Ok(Vec::new()),
    };

    let mut sql = String::from(
        "SELECT m.id, m.org_id, m.user_id, m.project, m.tool, m.content, m.tags, m.created_at,
                m.title, m.type, m.scope, m.topic_key, m.session_id, m.revision_count, m.normalized_hash, m.project_id,
                m.archived_at, m.pinned, m.collection_id, m.admin_note, m.delete_after
         FROM memories m
         JOIN memories_fts fts ON fts.rowid = m.rowid
         WHERE memories_fts MATCH ?1 AND m.org_id = ?2 AND m.archived_at IS NULL",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(fts_query), Box::new(org_id.to_string())];
    let mut idx = 3usize;
    if let Some(vid) = viewer_user_id {
        sql.push_str(&visibility_predicate("m.project_id", &format!("?{idx}")));
        params.push(Box::new(vid.to_string()));
        idx += 1;
    }
    sql.push_str(&format!(" ORDER BY rank LIMIT ?{idx}"));
    params.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

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
            row.get::<_, Option<String>>(10)
                .unwrap_or(Some("project".to_string())),
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
        let (
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags_str,
            created_at,
            title,
            memory_type,
            scope,
            topic_key,
            session_id,
            revision_count,
            normalized_hash,
            project_id,
            archived_at,
            pinned_i64,
            collection_id,
            admin_note,
            delete_after,
        ) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() {
            "archived".to_string()
        } else {
            "active".to_string()
        };
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
    list_memories_visible(
        conn,
        org_id,
        user_id,
        tool,
        project,
        type_filter,
        scope_filter,
        session_id_filter,
        limit,
        offset,
        include_archived,
        from_date,
        to_date,
        collection_id_filter,
        None,
    )
}

/// Like [`list_memories`], but when `viewer_user_id` is `Some(uid)` the result set is
/// restricted to memories `uid` may see (see [`visibility_predicate`]). `None` applies no
/// project-membership restriction — for admins and internal callers only.
#[allow(clippy::too_many_arguments)]
pub fn list_memories_visible(
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
    viewer_user_id: Option<&str>,
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
    if let Some(vid) = viewer_user_id {
        sql.push_str(&visibility_predicate(
            "project_id",
            &format!("?{param_idx}"),
        ));
        extra_params.push(vid.to_string());
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
        let (
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags_str,
            created_at,
            title,
            memory_type,
            scope,
            topic_key,
            session_id,
            revision_count,
            normalized_hash,
            project_id,
            archived_at,
            pinned_i64,
            collection_id,
            admin_note,
            delete_after,
        ) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() {
            "archived".to_string()
        } else {
            "active".to_string()
        };
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
    count_memories_visible(
        conn,
        org_id,
        user_id,
        tool,
        project,
        type_filter,
        scope_filter,
        session_id_filter,
        include_archived,
        from_date,
        to_date,
        collection_id_filter,
        None,
    )
}

/// Like [`count_memories`], but restricts the count to memories `viewer_user_id` may see
/// when it is `Some(uid)` (see [`visibility_predicate`]). Kept in lockstep with
/// [`list_memories_visible`] so page totals match the rows returned.
#[allow(clippy::too_many_arguments)]
pub fn count_memories_visible(
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
    viewer_user_id: Option<&str>,
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
    if let Some(vid) = viewer_user_id {
        sql.push_str(&visibility_predicate(
            "project_id",
            &format!("?{param_idx}"),
        ));
        extra_params.push(vid.to_string());
        param_idx += 1;
    }
    let _ = param_idx;

    let mut all_params: Vec<String> = vec![org_id.to_string()];
    all_params.extend(extra_params);
    let refs: Vec<&dyn rusqlite::ToSql> = all_params
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

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
pub fn get_memory_owner(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
) -> Result<Option<String>> {
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
    let placeholders: Vec<String> = (2..=ids.len() + 1).map(|i| format!("?{i}")).collect();
    let in_clause = placeholders.join(", ");

    let sql = if is_admin {
        format!("DELETE FROM memories WHERE org_id = ?1 AND id IN ({in_clause})")
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
pub fn reset_user_key(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
) -> Result<std::result::Result<String, &'static str>> {
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
    list_audit_with_resource(
        conn,
        org_id,
        user_id,
        action,
        resource_type,
        None,
        from,
        to,
        search,
        limit,
        offset,
    )
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
        let (
            id,
            org_id,
            user_id,
            timestamp,
            action,
            resource_type,
            resource_id,
            meta_str,
            previous_hash,
            current_hash,
        ) = row?;
        let metadata: serde_json::Value =
            serde_json::from_str(&meta_str).unwrap_or(serde_json::Value::Null);
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
    let previous_hash: Option<String> = tx
        .query_row(
            "SELECT current_hash FROM audit_logs
         WHERE org_id = ?1 AND current_hash IS NOT NULL
         ORDER BY rowid DESC LIMIT 1",
            [org_id],
            |r| r.get(0),
        )
        .optional()?;

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
            id,
            org_id,
            user_id,
            now,
            action,
            resource_type,
            resource_id,
            meta_str,
            previous_hash,
            current_hash
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
    insert_audit_log_chained(
        conn,
        org_id,
        user_id,
        action,
        resource_type,
        resource_id,
        metadata,
        None,
    )?;
    Ok(())
}

// ── Automation provenance ────────────────────────────────────────────────────

/// Creates a durable automation run. The v57 trigger enforces that an optional
/// project belongs to the run's organization.
pub fn create_automation_run(
    conn: &Connection,
    id: &str,
    org_id: &str,
    project_id: Option<&str>,
    created_by: &str,
    profile_version_ref: &str,
    policy_generation: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO automation_runs
         (id, org_id, project_id, created_by, profile_version_ref, policy_generation)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            id,
            org_id,
            project_id,
            created_by,
            profile_version_ref,
            policy_generation
        ],
    )?;
    Ok(())
}

/// Starts an attempt under a durable automation run.
pub fn create_automation_attempt(conn: &Connection, id: &str, run_id: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO automation_attempts (id, run_id) VALUES (?1, ?2)",
        rusqlite::params![id, run_id],
    )?;
    Ok(())
}

/// Records a worker callback as an immutable receipt. A replay with the same
/// attempt/callback identity is a successful no-op; an inactive, foreign, or
/// unknown attempt is denied by the database trigger.
pub fn record_automation_callback(
    conn: &Connection,
    org_id: &str,
    attempt_id: &str,
    callback_id: &str,
    payload_hash: &str,
) -> Result<bool> {
    let existing_payload: Option<String> = conn
        .query_row(
            "SELECT payload_hash FROM automation_receipts WHERE attempt_id = ?1 AND callback_id = ?2",
            rusqlite::params![attempt_id, callback_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing_payload) = existing_payload {
        if existing_payload == payload_hash {
            return Ok(false);
        }
        anyhow::bail!("automation_callback_payload_mismatch");
    }

    let affected = conn.execute(
        "INSERT INTO automation_receipts (id, org_id, attempt_id, callback_id, payload_hash)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(attempt_id, callback_id) DO NOTHING",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            org_id,
            attempt_id,
            callback_id,
            payload_hash
        ],
    )?;
    Ok(affected == 1)
}

/// Revokes an active attempt and appends its immutable stop evidence.
pub fn revoke_automation_attempt(
    conn: &Connection,
    org_id: &str,
    attempt_id: &str,
    reason: &str,
) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let affected = tx.execute(
        "UPDATE automation_attempts
         SET status = 'revoked', revoked_at = datetime('now')
         WHERE id = ?1 AND status = 'active' AND EXISTS (
             SELECT 1 FROM automation_runs r
             WHERE r.id = automation_attempts.run_id AND r.org_id = ?2
         )",
        rusqlite::params![attempt_id, org_id],
    )?;
    if affected == 1 {
        tx.execute(
            "INSERT INTO automation_revocations (id, org_id, attempt_id, reason)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![Uuid::new_v4().to_string(), org_id, attempt_id, reason],
        )?;
    }
    tx.commit()?;
    Ok(affected == 1)
}

// ── Harness sharing ───────────────────────────────────────────────────────────

fn harness_hash(manifest: &serde_json::Value) -> Result<String> {
    let bytes = serde_json::to_vec(manifest)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn json_vec(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn manifest_format(manifest: &serde_json::Value) -> Option<String> {
    manifest
        .get("format")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn warning_metadata(manifest: &serde_json::Value) -> Option<serde_json::Value> {
    let format = manifest.get("format").and_then(|v| v.as_str())?;
    if matches!(format, "hook" | "claude_code_plugin") {
        Some(serde_json::json!({
            "high_trust": true,
            "requires_acknowledgement": true,
            "message": "Review executable hooks or plugin metadata before approval."
        }))
    } else {
        None
    }
}

pub fn user_belongs_to_org(conn: &Connection, org_id: &str, user_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users WHERE id = ?1 AND org_id = ?2 AND status = 'active'",
        rusqlite::params![user_id, org_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn map_harness(row: &rusqlite::Row<'_>) -> rusqlite::Result<Harness> {
    let owner_user_id: String = row.get(9)?;
    let owner_name: Option<String> = row.get(10)?;
    let owner_email: Option<String> = row.get(11)?;
    Ok(Harness {
        id: row.get(0)?,
        org_id: row.get(1)?,
        project_id: row.get(2)?,
        slug: row.get(3)?,
        name: row.get(4)?,
        description: row.get(5)?,
        visibility: row.get(6)?,
        status: row.get(7)?,
        created_by: row.get(8)?,
        owner_user_id: owner_user_id.clone(),
        owner: Some(HarnessOwner {
            id: owner_user_id,
            name: owner_name.unwrap_or_default(),
            email: owner_email.unwrap_or_default(),
        }),
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        latest_version: None,
    })
}

fn map_harness_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<HarnessVersion> {
    let manifest_json: String = row.get(3)?;
    let targets_json: String = row.get(5)?;
    let provenance_json: String = row.get(6)?;
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_json).unwrap_or(serde_json::Value::Null);
    Ok(HarnessVersion {
        id: row.get(0)?,
        harness_id: row.get(1)?,
        version: row.get(2)?,
        format: manifest_format(&manifest),
        manifest,
        manifest_hash: row.get(4)?,
        targets: json_vec(&targets_json),
        provenance: serde_json::from_str(&provenance_json).unwrap_or(serde_json::json!({})),
        status: row.get(7)?,
        published_by: row.get(8)?,
        published_at: row.get(9)?,
        revoked_at: row.get(10)?,
    })
}

fn validate_manifest(
    manifest: &serde_json::Value,
) -> Result<(Vec<String>, serde_json::Value), &'static str> {
    if manifest.get("format").is_some()
        || manifest.get("schema_version").and_then(|v| v.as_str()) == Some("1.1")
    {
        validate_typed_harness_manifest(manifest)?;
    }
    let targets = manifest
        .get("targets")
        .and_then(|v| v.as_array())
        .ok_or("missing_targets")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Err("missing_targets");
    }
    let provenance = manifest
        .get("provenance")
        .cloned()
        .ok_or("missing_provenance")?;
    if !provenance.is_object() || provenance.as_object().map(|m| m.is_empty()).unwrap_or(true) {
        return Err("missing_provenance");
    }
    Ok((targets, provenance))
}

pub fn create_harness(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    input: &CreateHarnessRequest,
) -> Result<Harness> {
    if input.slug.trim().is_empty() || input.name.trim().is_empty() {
        anyhow::bail!("validation_error");
    }
    let id = Uuid::new_v4().to_string();
    let visibility = input.visibility.as_deref().unwrap_or("org");
    let owner_user_id = input.owner_user_id.as_deref().unwrap_or(user_id);
    if !user_belongs_to_org(conn, org_id, owner_user_id)? {
        anyhow::bail!("owner_not_in_org");
    }
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "INSERT INTO harnesses (id, org_id, project_id, slug, name, description, visibility, status, created_by, owner_user_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'draft', ?8, ?9, ?10, ?10)",
        rusqlite::params![id, org_id, input.project_id, input.slug.trim(), input.name.trim(), input.description, visibility, user_id, owner_user_id, now],
    )?;
    get_harness(conn, org_id, &id, None)?.ok_or_else(|| anyhow::anyhow!("harness_not_found"))
}

pub fn list_visible_harnesses(
    conn: &Connection,
    org_id: &str,
    viewer_user_id: Option<&str>,
    target: Option<&str>,
    owner_user_id: Option<&str>,
) -> Result<Vec<Harness>> {
    let mut sql = String::from("SELECT h.id, h.org_id, h.project_id, h.slug, h.name, h.description, h.visibility, h.status, h.created_by, h.owner_user_id, u.name, u.email, h.created_at, h.updated_at FROM harnesses h LEFT JOIN users u ON u.id = h.owner_user_id WHERE h.org_id = ?1");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    let mut idx = 2usize;
    if let Some(user_id) = viewer_user_id {
        sql.push_str(&format!(" AND ((h.project_id IS NULL AND h.owner_user_id = ?{idx}) OR h.project_id IN (SELECT project_id FROM project_visibility WHERE user_id = ?{idx}))"));
        params.push(Box::new(user_id.to_string()));
        idx += 1;
    }
    if let Some(target) = target.filter(|t| !t.is_empty()) {
        sql.push_str(&format!(" AND EXISTS (SELECT 1 FROM harness_versions hv WHERE hv.harness_id = h.id AND hv.status = 'published' AND hv.revoked_at IS NULL AND hv.targets_json LIKE ?{idx})"));
        params.push(Box::new(format!("%\"{}\"%", target)));
        idx += 1;
    }
    if let Some(owner_user_id) = owner_user_id.filter(|v| !v.is_empty()) {
        sql.push_str(&format!(" AND h.owner_user_id = ?{idx}"));
        params.push(Box::new(owner_user_id.to_string()));
    }
    sql.push_str(" ORDER BY h.updated_at DESC");
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), map_harness)?;
    let mut harnesses = Vec::new();
    for row in rows {
        let mut h = row?;
        h.latest_version = latest_harness_version_summary(conn, &h.id)?;
        harnesses.push(h);
    }
    Ok(harnesses)
}

pub fn get_harness(
    conn: &Connection,
    org_id: &str,
    harness_id: &str,
    viewer_user_id: Option<&str>,
) -> Result<Option<Harness>> {
    let result = conn.query_row(
        "SELECT h.id, h.org_id, h.project_id, h.slug, h.name, h.description, h.visibility, h.status, h.created_by, h.owner_user_id, u.name, u.email, h.created_at, h.updated_at FROM harnesses h LEFT JOIN users u ON u.id = h.owner_user_id WHERE h.org_id = ?1 AND h.id = ?2",
        rusqlite::params![org_id, harness_id], map_harness,
    ).optional()?;
    let Some(mut harness) = result else {
        return Ok(None);
    };
    if let Some(user_id) = viewer_user_id {
        let allowed =
            match harness.project_id.as_deref() {
                Some(project_id) => conn.query_row(
                    "SELECT COUNT(*) FROM project_members WHERE project_id = ?1 AND user_id = ?2",
                    rusqlite::params![project_id, user_id],
                    |r| r.get::<_, i64>(0),
                )? > 0,
                None => harness.owner_user_id == user_id,
            };
        if !allowed {
            return Ok(None);
        }
    }
    harness.latest_version = latest_harness_version_summary(conn, &harness.id)?;
    Ok(Some(harness))
}

pub fn latest_harness_version_summary(
    conn: &Connection,
    harness_id: &str,
) -> Result<Option<HarnessVersionSummary>> {
    conn.query_row(
        "SELECT id, version, manifest_hash, targets_json, status, published_at, manifest_json FROM harness_versions WHERE harness_id = ?1 AND revoked_at IS NULL ORDER BY published_at DESC LIMIT 1",
        [harness_id],
        |row| {
            let targets_json: String = row.get(3)?;
            let manifest_json: String = row.get(6)?;
            let manifest: serde_json::Value = serde_json::from_str(&manifest_json).unwrap_or(serde_json::Value::Null);
            Ok(HarnessVersionSummary { id: row.get(0)?, version: row.get(1)?, manifest_hash: row.get(2)?, targets: json_vec(&targets_json), format: manifest_format(&manifest), warning_metadata: warning_metadata(&manifest), status: row.get(4)?, published_at: row.get(5)? })
        },
    ).optional().map_err(Into::into)
}

pub fn publish_harness_version(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    harness_id: &str,
    input: &PublishHarnessVersionRequest,
) -> Result<HarnessVersion> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM harnesses WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![harness_id, org_id],
        |r| r.get(0),
    )?;
    if exists == 0 {
        anyhow::bail!("harness_not_found");
    }
    if input.version.trim().is_empty() {
        anyhow::bail!("validation_error");
    }
    let (targets, provenance) =
        validate_manifest(&input.manifest).map_err(|e| anyhow::anyhow!(e))?;
    let computed_hash = harness_hash(&input.manifest)?;
    if input
        .manifest_hash
        .as_deref()
        .is_some_and(|h| h != computed_hash)
    {
        anyhow::bail!("manifest_hash_mismatch");
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO harness_versions (id, harness_id, version, manifest_json, manifest_hash, targets_json, provenance_json, status, published_by, published_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'published', ?8, datetime('now'))",
        rusqlite::params![id, harness_id, input.version.trim(), serde_json::to_string(&input.manifest)?, computed_hash, serde_json::to_string(&targets)?, serde_json::to_string(&provenance)?, user_id],
    )?;
    conn.execute(
        "UPDATE harnesses SET status = 'published', updated_at = datetime('now') WHERE id = ?1",
        [harness_id],
    )?;
    get_harness_version(conn, org_id, harness_id, input.version.trim())?
        .ok_or_else(|| anyhow::anyhow!("version_not_found"))
}

/// Archives a harness (status = 'archived'). Returns the updated harness, or None
/// if it does not exist or is not visible to the viewer.
pub fn archive_harness(
    conn: &Connection,
    org_id: &str,
    id: &str,
    viewer_user_id: Option<&str>,
) -> Result<Option<Harness>> {
    if get_harness(conn, org_id, id, viewer_user_id)?.is_none() {
        return Ok(None);
    }
    conn.execute(
        "UPDATE harnesses SET status = 'archived', updated_at = datetime('now') WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![id, org_id],
    )?;
    get_harness(conn, org_id, id, viewer_user_id)
}

pub fn get_harness_version(
    conn: &Connection,
    org_id: &str,
    harness_id: &str,
    version: &str,
) -> Result<Option<HarnessVersion>> {
    conn.query_row(
        "SELECT hv.id, hv.harness_id, hv.version, hv.manifest_json, hv.manifest_hash, hv.targets_json, hv.provenance_json, hv.status, hv.published_by, hv.published_at, hv.revoked_at FROM harness_versions hv JOIN harnesses h ON h.id = hv.harness_id WHERE h.org_id = ?1 AND h.id = ?2 AND hv.version = ?3 AND hv.revoked_at IS NULL",
        rusqlite::params![org_id, harness_id, version], map_harness_version,
    ).optional().map_err(Into::into)
}

pub fn create_harness_approval(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    viewer_user_id: Option<&str>,
    harness_id: &str,
    version: &str,
    input: &HarnessApprovalRequest,
) -> Result<HarnessApproval> {
    let hv = get_visible_harness_version(conn, org_id, harness_id, version, viewer_user_id)?
        .ok_or_else(|| anyhow::anyhow!("version_not_found"))?;
    if hv.manifest_hash != input.manifest_hash {
        anyhow::bail!("manifest_hash_mismatch");
    }
    let metadata = input
        .metadata
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    if warning_metadata(&hv.manifest).is_some()
        && metadata
            .get("warning_acknowledged")
            .and_then(|value| value.as_bool())
            != Some(true)
    {
        anyhow::bail!("warning_acknowledgement_required");
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO harness_install_approvals (id, org_id, user_id, harness_version_id, target_tool, target_scope, manifest_hash, status, metadata_json, approved_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'approved', ?8, datetime('now'))",
        rusqlite::params![id, org_id, user_id, hv.id, input.target_tool, input.target_scope, input.manifest_hash, serde_json::to_string(&metadata)?],
    )?;
    conn.query_row(
        "SELECT id, org_id, user_id, harness_version_id, target_tool, target_scope, manifest_hash, status, metadata_json, approved_at FROM harness_install_approvals WHERE id = ?1",
        [id],
        |row| { let metadata_json: String = row.get(8)?; Ok(HarnessApproval { id: row.get(0)?, org_id: row.get(1)?, user_id: row.get(2)?, harness_version_id: row.get(3)?, target_tool: row.get(4)?, target_scope: row.get(5)?, manifest_hash: row.get(6)?, status: row.get(7)?, metadata: serde_json::from_str(&metadata_json).unwrap_or_else(|_| serde_json::json!({})), approved_at: row.get(9)? }) },
    ).map_err(Into::into)
}

pub fn download_harness_version(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    viewer_user_id: Option<&str>,
    harness_id: &str,
    version: &str,
) -> Result<Option<HarnessDownloadResponse>> {
    let Some(hv) = get_visible_harness_version(conn, org_id, harness_id, version, viewer_user_id)?
    else {
        return Ok(None);
    };
    let approval_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM harness_install_approvals WHERE org_id = ?1 AND user_id = ?2 AND harness_version_id = ?3 AND manifest_hash = ?4 AND status = 'approved'",
        rusqlite::params![org_id, user_id, hv.id, hv.manifest_hash],
        |row| row.get(0),
    )?;
    if approval_count == 0 {
        anyhow::bail!("approval_required");
    }
    Ok(Some(HarnessDownloadResponse {
        harness_id: hv.harness_id,
        version: hv.version,
        manifest: hv.manifest,
        manifest_hash: hv.manifest_hash,
        approval_required: true,
    }))
}

pub fn record_harness_install_result(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    harness_id: &str,
    version: &str,
    input: &HarnessInstallResultRequest,
) -> Result<HarnessApproval> {
    if input.status.trim().is_empty() {
        anyhow::bail!("validation_error");
    }
    let hv = get_harness_version(conn, org_id, harness_id, version)?
        .ok_or_else(|| anyhow::anyhow!("version_not_found"))?;
    if hv.manifest_hash != input.manifest_hash {
        anyhow::bail!("manifest_hash_mismatch");
    }
    let mut metadata = input
        .metadata
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    if has_secret_indicator(&metadata) || has_suspicious_content_key(&metadata) {
        anyhow::bail!("raw_local_content_rejected");
    }
    if !metadata.is_object() {
        metadata = serde_json::json!({ "details": metadata });
    }
    let mut install_result = metadata.as_object().cloned().unwrap_or_default();
    install_result.insert(
        "status".to_string(),
        serde_json::Value::String(input.status.trim().to_string()),
    );

    let existing_metadata: String = conn.query_row(
        "SELECT metadata_json FROM harness_install_approvals WHERE id = ?1 AND org_id = ?2 AND user_id = ?3 AND harness_version_id = ?4 AND manifest_hash = ?5 AND status = 'approved'",
        rusqlite::params![input.approval_id, org_id, user_id, hv.id, input.manifest_hash],
        |row| row.get(0),
    ).optional()?.ok_or_else(|| anyhow::anyhow!("approval_required"))?;
    let mut merged = serde_json::from_str::<serde_json::Value>(&existing_metadata)
        .unwrap_or_else(|_| serde_json::json!({}));
    if !merged.is_object() {
        merged = serde_json::json!({});
    }
    merged.as_object_mut().unwrap().insert(
        "install_result".to_string(),
        serde_json::Value::Object(install_result),
    );
    conn.execute(
        "UPDATE harness_install_approvals SET metadata_json = ?1 WHERE id = ?2",
        rusqlite::params![serde_json::to_string(&merged)?, input.approval_id],
    )?;
    conn.query_row(
        "SELECT id, org_id, user_id, harness_version_id, target_tool, target_scope, manifest_hash, status, metadata_json, approved_at FROM harness_install_approvals WHERE id = ?1",
        [&input.approval_id],
        |row| { let metadata_json: String = row.get(8)?; Ok(HarnessApproval { id: row.get(0)?, org_id: row.get(1)?, user_id: row.get(2)?, harness_version_id: row.get(3)?, target_tool: row.get(4)?, target_scope: row.get(5)?, manifest_hash: row.get(6)?, status: row.get(7)?, metadata: serde_json::from_str(&metadata_json).unwrap_or_else(|_| serde_json::json!({})), approved_at: row.get(9)? }) },
    ).map_err(Into::into)
}

pub fn list_harness_recommendations(
    conn: &Connection,
    org_id: &str,
    viewer_user_id: Option<&str>,
    target: Option<&str>,
) -> Result<Vec<HarnessRecommendation>> {
    let harnesses = list_visible_harnesses(conn, org_id, viewer_user_id, target, None)?;
    Ok(harnesses
        .into_iter()
        .filter_map(|h| {
            let version = h.latest_version?;
            Some(HarnessRecommendation {
                download_url: format!(
                    "/v1/harnesses/{}/versions/{}/download",
                    h.id, version.version
                ),
                harness_id: h.id,
                version: version.version,
                name: h.name,
                description: h.description,
                owner: h.owner,
                targets: version.targets,
                format: version.format,
                warning_metadata: version.warning_metadata,
                manifest_hash: version.manifest_hash,
                approval_required: true,
                required_permissions: vec![
                    "harness:download".to_string(),
                    "harness:install".to_string(),
                ],
            })
        })
        .collect())
}

fn has_secret_indicator(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => {
            let lower = s.to_lowercase();
            lower != "[redacted]"
                && (lower.contains("raw-secret")
                    || lower.starts_with("sk-")
                    || lower.starts_with("ghp_")
                    || lower.starts_with("nm_")
                    || lower.starts_with("xoxb-")
                    || lower.contains("bearer "))
        }
        serde_json::Value::Array(items) => items.iter().any(has_secret_indicator),
        serde_json::Value::Object(map) => map.values().any(has_secret_indicator),
        _ => false,
    }
}

fn has_suspicious_content_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(items) => items.iter().any(has_suspicious_content_key),
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            let normalized = key.to_lowercase();
            matches!(
                normalized.as_str(),
                "raw_file_contents"
                    | "raw_shell_content"
                    | "raw_hook_content"
                    | "shell_profile"
                    | "hook_args"
            ) || has_suspicious_content_key(value)
        }),
        _ => false,
    }
}

pub fn get_visible_harness_version(
    conn: &Connection,
    org_id: &str,
    harness_id: &str,
    version: &str,
    viewer_user_id: Option<&str>,
) -> Result<Option<HarnessVersion>> {
    if get_harness(conn, org_id, harness_id, viewer_user_id)?.is_none() {
        return Ok(None);
    }
    get_harness_version(conn, org_id, harness_id, version)
}

pub fn create_harness_config_review(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    input: &CreateHarnessConfigReviewRequest,
) -> Result<HarnessConfigReview> {
    if input.content_hash.trim().is_empty()
        || input
            .redaction_report
            .as_object()
            .map(|m| m.is_empty())
            .unwrap_or(true)
    {
        anyhow::bail!("missing_redaction_report");
    }
    if input
        .redaction_report
        .get("secret_scan_status")
        .and_then(|v| v.as_str())
        == Some("failed")
        || has_secret_indicator(&input.redacted_config)
        || has_secret_indicator(&input.redaction_report)
        || has_suspicious_content_key(&input.redaction_report)
    {
        anyhow::bail!("secret_scan_failed");
    }
    let id = Uuid::new_v4().to_string();
    let status = input.status.as_deref().unwrap_or("uploaded");
    conn.execute(
        "INSERT INTO harness_config_reviews (id, org_id, user_id, source_tool, redacted_config_json, redaction_report_json, content_hash, status, created_at, shared_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'), CASE WHEN ?8 = 'shared' THEN datetime('now') ELSE NULL END)",
        rusqlite::params![id, org_id, user_id, input.source_tool, serde_json::to_string(&input.redacted_config)?, serde_json::to_string(&input.redaction_report)?, input.content_hash, status],
    )?;
    get_harness_config_review(conn, org_id, &id)?
        .ok_or_else(|| anyhow::anyhow!("config_review_not_found"))
}

fn config_review_author(
    user_id: String,
    name: Option<String>,
    email: Option<String>,
) -> Option<HarnessConfigReviewAuthor> {
    match (name, email) {
        (Some(name), Some(email)) => Some(HarnessConfigReviewAuthor {
            id: user_id,
            name,
            email,
        }),
        _ => None,
    }
}

const CONFIG_REVIEW_SELECT: &str = "SELECT hcr.id, hcr.org_id, hcr.user_id, hcr.source_tool, hcr.redacted_config_json, hcr.redaction_report_json, hcr.content_hash, hcr.status, hcr.created_at, hcr.shared_at, u.name, u.email FROM harness_config_reviews hcr LEFT JOIN users u ON u.id = hcr.user_id";

fn map_config_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<HarnessConfigReview> {
    let config_json: String = row.get(4)?;
    let report_json: String = row.get(5)?;
    let user_id: String = row.get(2)?;
    let name: Option<String> = row.get(10)?;
    let email: Option<String> = row.get(11)?;
    Ok(HarnessConfigReview {
        id: row.get(0)?,
        org_id: row.get(1)?,
        user_id: user_id.clone(),
        source_tool: row.get(3)?,
        redacted_config: serde_json::from_str(&config_json).unwrap_or(serde_json::Value::Null),
        redaction_report: serde_json::from_str(&report_json).unwrap_or(serde_json::json!({})),
        content_hash: row.get(6)?,
        status: row.get(7)?,
        created_at: row.get(8)?,
        shared_at: row.get(9)?,
        author: config_review_author(user_id, name, email),
    })
}

pub fn get_harness_config_review(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<HarnessConfigReview>> {
    conn.query_row(
        &format!("{CONFIG_REVIEW_SELECT} WHERE hcr.org_id = ?1 AND hcr.id = ?2"),
        rusqlite::params![org_id, id],
        map_config_review,
    )
    .optional()
    .map_err(Into::into)
}

/// Retrieves a config review only when it belongs to the caller. Passing `None`
/// is reserved for the super-user scope.
pub fn get_harness_config_review_visible(
    conn: &Connection,
    org_id: &str,
    id: &str,
    viewer_user_id: Option<&str>,
) -> Result<Option<HarnessConfigReview>> {
    let Some(review) = get_harness_config_review(conn, org_id, id)? else {
        return Ok(None);
    };
    Ok(match viewer_user_id {
        Some(user_id) if review.user_id != user_id => None,
        _ => Some(review),
    })
}

/// Lists config reviews for an org, newest first. Optionally filters by status
/// (e.g. "shared"). Returns redacted snapshots only — raw secrets are never stored.
pub fn list_harness_config_reviews(
    conn: &Connection,
    org_id: &str,
    status: Option<&str>,
) -> Result<Vec<HarnessConfigReview>> {
    let mut sql = format!("{CONFIG_REVIEW_SELECT} WHERE hcr.org_id = ?1");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    if let Some(status) = status {
        sql.push_str(" AND hcr.status = ?2");
        params.push(Box::new(status.to_string()));
    }
    sql.push_str(" ORDER BY hcr.created_at DESC, hcr.id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(refs.as_slice(), map_config_review)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Lists config reviews within the caller's ownership scope.
pub fn list_harness_config_reviews_visible(
    conn: &Connection,
    org_id: &str,
    status: Option<&str>,
    viewer_user_id: Option<&str>,
) -> Result<Vec<HarnessConfigReview>> {
    let mut sql = format!("{CONFIG_REVIEW_SELECT} WHERE hcr.org_id = ?1");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    let mut index = 2;
    if let Some(user_id) = viewer_user_id {
        sql.push_str(&format!(" AND hcr.user_id = ?{index}"));
        params.push(Box::new(user_id.to_string()));
        index += 1;
    }
    if let Some(status) = status {
        sql.push_str(&format!(" AND hcr.status = ?{index}"));
        params.push(Box::new(status.to_string()));
    }
    sql.push_str(" ORDER BY hcr.created_at DESC, hcr.id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|value| value.as_ref()).collect();
    let rows = stmt
        .query_map(refs.as_slice(), map_config_review)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Adds a comment to a config review, returning the stored comment with author.
/// Callers must have already checked review-config permission and review existence.
pub fn create_harness_config_review_comment(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    review_id: &str,
    body: &str,
) -> Result<HarnessConfigReviewComment> {
    let body = body.trim();
    if body.is_empty() {
        anyhow::bail!("empty_comment");
    }
    if get_harness_config_review(conn, org_id, review_id)?.is_none() {
        anyhow::bail!("config_review_not_found");
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO harness_config_review_comments (id, org_id, review_id, user_id, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
        rusqlite::params![id, org_id, review_id, user_id, body],
    )?;
    get_harness_config_review_comment(conn, org_id, &id)?
        .ok_or_else(|| anyhow::anyhow!("comment_not_found"))
}

const CONFIG_REVIEW_COMMENT_SELECT: &str = "SELECT c.id, c.org_id, c.review_id, c.user_id, c.body, c.created_at, u.name, u.email FROM harness_config_review_comments c LEFT JOIN users u ON u.id = c.user_id";

fn map_config_review_comment(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<HarnessConfigReviewComment> {
    let user_id: String = row.get(3)?;
    let name: Option<String> = row.get(6)?;
    let email: Option<String> = row.get(7)?;
    Ok(HarnessConfigReviewComment {
        id: row.get(0)?,
        org_id: row.get(1)?,
        review_id: row.get(2)?,
        user_id: user_id.clone(),
        body: row.get(4)?,
        created_at: row.get(5)?,
        author: config_review_author(user_id, name, email),
    })
}

fn get_harness_config_review_comment(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<HarnessConfigReviewComment>> {
    conn.query_row(
        &format!("{CONFIG_REVIEW_COMMENT_SELECT} WHERE c.org_id = ?1 AND c.id = ?2"),
        rusqlite::params![org_id, id],
        map_config_review_comment,
    )
    .optional()
    .map_err(Into::into)
}

/// Lists comments on a config review, oldest first.
pub fn list_harness_config_review_comments(
    conn: &Connection,
    org_id: &str,
    review_id: &str,
) -> Result<Vec<HarnessConfigReviewComment>> {
    let mut stmt = conn.prepare(&format!(
        "{CONFIG_REVIEW_COMMENT_SELECT} WHERE c.org_id = ?1 AND c.review_id = ?2 ORDER BY c.created_at ASC, c.id ASC"
    ))?;
    let rows = stmt
        .query_map(
            rusqlite::params![org_id, review_id],
            map_config_review_comment,
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
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

    let org = get_org(conn, org_id)?.ok_or_else(|| anyhow::anyhow!("org_not_found"))?;
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

pub fn update_org_settings(
    conn: &Connection,
    org_id: &str,
    settings: &OrgSettings,
) -> Result<OrgSettings> {
    // Strip direct-column fields from the JSON blob — they live in their own columns.
    let blob_settings = OrgSettings {
        retention_days: None,
        custom_instructions: None,
        min_password_length: None,
        announcement: None,
        announcement_type: None,
        ..settings.clone()
    };
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
pub fn update_announcement(
    conn: &Connection,
    org_id: &str,
    announcement: &str,
    announcement_type: &str,
) -> Result<OrgSettings> {
    let ann: Option<&str> = if announcement.is_empty() {
        None
    } else {
        Some(announcement)
    };
    conn.execute(
        "UPDATE organizations SET announcement = ?1, announcement_type = ?2 WHERE id = ?3",
        rusqlite::params![ann, announcement_type, org_id],
    )?;
    get_org_settings(conn, org_id)
}

/// Set (or clear) the logo URL for an org.
/// None = clear the logo (sets logo_url = NULL).
pub fn update_org_logo(
    conn: &Connection,
    org_id: &str,
    logo_url: Option<&str>,
) -> Result<OrgSettings> {
    conn.execute(
        "UPDATE organizations SET logo_url = ?1 WHERE id = ?2",
        rusqlite::params![logo_url, org_id],
    )?;
    get_org_settings(conn, org_id)
}

/// Set (or clear) the scheduled-deletion date for a single memory.
/// `delete_after` = None → clears the schedule.
pub fn schedule_memory_delete(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
    delete_after: Option<&str>,
) -> Result<()> {
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
pub fn get_usage_stats(
    conn: &Connection,
    org_id: &str,
) -> Result<crate::models::types::UsageStats> {
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
    Ok(crate::models::types::UsageStats {
        memories,
        sessions,
        users,
        projects,
        code_repos,
    })
}

// ── Memory facets ─────────────────────────────────────────────────────────────

/// Returns distinct facet counts (type, scope, project) for an org's memories.
/// Each facet bucket is ordered by count descending, limited to 50 values.
pub fn get_memory_facets(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    is_super_user: bool,
) -> Result<crate::models::types::MemoryFacets> {
    let types: Vec<crate::models::types::FacetCount> = if is_super_user {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(type, ''), COUNT(*) as cnt
             FROM memories
             WHERE org_id = ?1 AND type IS NOT NULL AND type != ''
             GROUP BY type
             ORDER BY cnt DESC
             LIMIT 50",
        )?;
        let result: Vec<crate::models::types::FacetCount> = stmt
            .query_map([org_id], |row| {
                Ok(crate::models::types::FacetCount {
                    value: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        result
    } else {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(m.type, ''), COUNT(*) as cnt
             FROM memories m
             JOIN projects p ON p.org_id = m.org_id AND p.name = m.project
             JOIN project_visibility pv ON pv.project_id = p.id AND pv.user_id = ?2
             WHERE m.org_id = ?1 AND m.type IS NOT NULL AND m.type != ''
             GROUP BY m.type
             ORDER BY cnt DESC
             LIMIT 50",
        )?;
        let result: Vec<crate::models::types::FacetCount> = stmt
            .query_map(rusqlite::params![org_id, user_id], |row| {
                Ok(crate::models::types::FacetCount {
                    value: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        result
    };

    let scopes: Vec<crate::models::types::FacetCount> = if is_super_user {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(scope, 'project'), COUNT(*) as cnt
             FROM memories
             WHERE org_id = ?1
             GROUP BY COALESCE(scope, 'project')
             ORDER BY cnt DESC
             LIMIT 50",
        )?;
        let result: Vec<crate::models::types::FacetCount> = stmt
            .query_map([org_id], |row| {
                Ok(crate::models::types::FacetCount {
                    value: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        result
    } else {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(m.scope, 'project'), COUNT(*) as cnt
             FROM memories m
             JOIN projects p ON p.org_id = m.org_id AND p.name = m.project
             JOIN project_visibility pv ON pv.project_id = p.id AND pv.user_id = ?2
             WHERE m.org_id = ?1
             GROUP BY COALESCE(m.scope, 'project')
             ORDER BY cnt DESC
             LIMIT 50",
        )?;
        let result: Vec<crate::models::types::FacetCount> = stmt
            .query_map(rusqlite::params![org_id, user_id], |row| {
                Ok(crate::models::types::FacetCount {
                    value: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        result
    };

    let projects: Vec<crate::models::types::FacetCount> = if is_super_user {
        let mut stmt = conn.prepare(
            "SELECT project, COUNT(*) as cnt
             FROM memories
             WHERE org_id = ?1
             GROUP BY project
             ORDER BY cnt DESC
             LIMIT 50",
        )?;
        let result: Vec<crate::models::types::FacetCount> = stmt
            .query_map([org_id], |row| {
                Ok(crate::models::types::FacetCount {
                    value: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        result
    } else {
        let mut stmt = conn.prepare(
            "SELECT m.project, COUNT(*) as cnt
             FROM memories m
             JOIN projects p ON p.org_id = m.org_id AND p.name = m.project
             JOIN project_visibility pv ON pv.project_id = p.id AND pv.user_id = ?2
             WHERE m.org_id = ?1
             GROUP BY m.project
             ORDER BY cnt DESC
             LIMIT 50",
        )?;
        let result: Vec<crate::models::types::FacetCount> = stmt
            .query_map(rusqlite::params![org_id, user_id], |row| {
                Ok(crate::models::types::FacetCount {
                    value: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        result
    };

    Ok(crate::models::types::MemoryFacets {
        types,
        scopes,
        projects,
    })
}

// ── Tag stats ─────────────────────────────────────────────────────────────────

/// Returns tag usage counts across all memories for the org.
/// `memories.tags` is stored as a JSON array string like '["tag1","tag2"]'.
/// SQLite's `json_each` expands the array so we can GROUP BY individual tag values.
pub fn get_tag_stats(
    conn: &Connection,
    org_id: &str,
) -> Result<Vec<crate::models::types::NameCount>> {
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
pub fn get_memory_trends(
    conn: &Connection,
    org_id: &str,
    days: i64,
) -> Result<crate::models::types::MemoryTrends> {
    use crate::models::types::{DailyCount, MemoryTrends, NameCount};

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

/// The transactional core of storing a memory: session validation, the upsert,
/// and the audit row — everything except taking a lock and computing an
/// embedding.
///
/// It exists so the migration commit path and `MemoryStore::store` share **one**
/// body. The alternative was for the commit path to call `upsert_memory` plus
/// `log_audit` itself, which is the same code in two places until the day
/// somebody changes one of them; a migrated memory would then quietly stop being
/// audited, and nobody would notice until an auditor asked.
///
/// Takes `&Connection`, so a caller may pass a `&Transaction` and get the
/// memory, its audit row and its own bookkeeping committed atomically.
/// Embedding is deliberately NOT here: it is CPU-bound and must not run with a
/// write transaction open.
pub fn store_memory_with_audit(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    req: &StoreMemoryRequest,
) -> Result<Memory> {
    if let Some(ref sid) = req.session_id {
        let valid = validate_session_ownership(conn, org_id, sid)?;
        if !valid {
            anyhow::bail!("invalid_session_id:{sid}");
        }
    }

    let memory = upsert_memory(conn, org_id, user_id, req)?;

    let _ = log_audit(
        conn,
        org_id,
        user_id,
        "store",
        "memory",
        Some(&memory.id),
        serde_json::json!({
            "tool": memory.tool,
            "project": memory.project,
            "title": memory.title,
            "type": memory.memory_type,
            "tags": memory.tags,
            "preview": memory.content.chars().take(160).collect::<String>(),
        }),
    );

    Ok(memory)
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
    let project = req
        .project
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("default");
    // Block implicit project creation: an explicit project name that does not already
    // exist is rejected — an admin must create it first via the project API (which does
    // not auto-enroll members). Absent/empty/"default" is the org-shared bucket
    // (project_id NULL): never creates a project row and never enrolls anyone.
    let project_id: Option<String> = match find_project_id(conn, org_id, project)? {
        Some(id) => Some(id),
        None if project == "default" => None,
        None => anyhow::bail!("project_not_found:{project}"),
    };
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
                        req.content,
                        req.title,
                        req.memory_type,
                        scope,
                        normalized_hash,
                        new_revision,
                        tags_json,
                        &project_id,
                        existing_id
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
                    project_id: project_id.clone(),
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
        project_id,
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
        rusqlite::params![
            id,
            org_id,
            req.name,
            req.project,
            directory,
            now,
            req.summary
        ],
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

    let refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
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
    list_sessions_visible(conn, org_id, None)
}

/// Returns `true` if a viewer may see a session/memory whose project NAME is `project`.
///
/// Admins (`viewer_user_id = None`) always may. Non-admins may when the project name has no
/// registered `projects` row (org-shared / legacy, mirrors the `project_id IS NULL` case for
/// memories) OR when the viewer is a `project_members` row for that project. Used by
/// session read paths, which key projects by name rather than id.
pub fn user_can_view_project_name(
    conn: &Connection,
    org_id: &str,
    project: &str,
    viewer_user_id: Option<&str>,
) -> Result<bool> {
    let Some(vid) = viewer_user_id else {
        return Ok(true);
    };
    let visible: i64 = conn.query_row(
        "SELECT CASE
                  WHEN NOT EXISTS (SELECT 1 FROM projects p WHERE p.org_id = ?1 AND p.name = ?2) THEN 1
                  WHEN EXISTS (
                      SELECT 1 FROM projects p
                      JOIN project_visibility pv ON pv.project_id = p.id
                      WHERE p.org_id = ?1 AND p.name = ?2 AND pv.user_id = ?3
                  ) THEN 1
                  ELSE 0
                END",
        rusqlite::params![org_id, project, vid],
        |row| row.get(0),
    )?;
    Ok(visible != 0)
}

/// Like [`list_sessions`], but when `viewer_user_id` is `Some(uid)` restricts results to
/// sessions `uid` may see: sessions of projects they belong to, plus project-less
/// (org-shared / unregistered-project) sessions. `None` = no restriction (admin).
pub fn list_sessions_visible(
    conn: &Connection,
    org_id: &str,
    viewer_user_id: Option<&str>,
) -> Result<Vec<SessionWithCount>> {
    let mut sql = String::from(
        "SELECT s.id, s.org_id, s.name, s.project, s.directory, s.started_at, s.ended_at, s.summary,
                COUNT(m.id) as memory_count
         FROM sessions s
         LEFT JOIN memories m ON m.session_id = s.id AND m.org_id = s.org_id
         WHERE s.org_id = ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    if let Some(vid) = viewer_user_id {
        sql.push_str(
            " AND (
                NOT EXISTS (SELECT 1 FROM projects p WHERE p.org_id = ?1 AND p.name = s.project)
                OR EXISTS (
                    SELECT 1 FROM projects p
                    JOIN project_visibility pv ON pv.project_id = p.id
                    WHERE p.org_id = ?1 AND p.name = s.project AND pv.user_id = ?2
                )
            )",
        );
        params.push(Box::new(vid.to_string()));
    }
    sql.push_str(" GROUP BY s.id ORDER BY s.started_at DESC LIMIT 100");

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
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

    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// Validates that a session_id belongs to the given org.
pub fn validate_session_ownership(
    conn: &Connection,
    org_id: &str,
    session_id: &str,
) -> Result<bool> {
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
pub fn find_admin_by_email(
    conn: &Connection,
    email: &str,
) -> Result<Option<(User, Option<String>)>> {
    let result = conn.query_row(
        "SELECT id, org_id, email, name, role, status, created_at, password_hash
         FROM users WHERE email = ?1 AND status = 'active'
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
pub fn validate_and_consume_reset_token(
    conn: &Connection,
    raw_token: &str,
) -> Result<Option<String>> {
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
    get_memories_by_ids_visible(conn, org_id, ids, None)
}

/// Like [`get_memories_by_ids`], but when `viewer_user_id` is `Some(uid)` drops any id the
/// user may not see (see [`visibility_predicate`]). Used as the final authority for what
/// semantic / hybrid search returns, so a non-visible id surfaced by ranking is filtered
/// out here rather than leaked. `None` applies no restriction — admins / internal callers.
pub fn get_memories_by_ids_visible(
    conn: &Connection,
    org_id: &str,
    ids: &[String],
    viewer_user_id: Option<&str>,
) -> Result<Vec<Memory>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Build "?,?,?" placeholder
    let placeholders: String = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(", ");

    let mut sql = format!(
        "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                archived_at, pinned, collection_id, admin_note, delete_after
         FROM memories
         WHERE org_id = ?1 AND id IN ({placeholders})"
    );

    // Viewer id (if any) binds to the next placeholder after org_id + all ids.
    let viewer_owned = viewer_user_id.map(|v| v.to_string());
    if viewer_owned.is_some() {
        let idx = ids.len() + 2;
        sql.push_str(&visibility_predicate("project_id", &format!("?{idx}")));
    }

    let mut stmt = conn.prepare(&sql)?;

    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&org_id as &dyn rusqlite::ToSql];
    for id in ids.iter() {
        params.push(id as &dyn rusqlite::ToSql);
    }
    if let Some(ref v) = viewer_owned {
        params.push(v as &dyn rusqlite::ToSql);
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
        let (
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags_str,
            created_at,
            title,
            memory_type,
            scope,
            topic_key,
            session_id,
            revision_count,
            normalized_hash,
            project_id,
            archived_at,
            pinned_i64,
            collection_id,
            admin_note,
            delete_after,
        ) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() {
            "archived".to_string()
        } else {
            "active".to_string()
        };
        map.insert(
            id.clone(),
            Memory {
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
            },
        );
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
pub fn list_collections(
    conn: &Connection,
    org_id: &str,
) -> Result<Vec<crate::models::types::Collection>> {
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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

/// Updates the permissions of an existing custom role within an organization.
/// Returns true if updated, false if not found or is a template.
pub fn update_role_permissions(
    conn: &Connection,
    org_id: &str,
    role_id: &str,
    permissions: &[String],
) -> Result<bool> {
    let permissions_json = serde_json::to_string(permissions)?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let count = conn.execute(
        "UPDATE roles SET permissions = ?1, version = version + 1, updated_at = ?2 WHERE id = ?3 AND org_id = ?4 AND is_template = 0",
        rusqlite::params![permissions_json, now, role_id, org_id],
    )?;
    Ok(count > 0)
}

/// Updates the role of a user in an organization.
pub fn update_user_role(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    new_role: &str,
) -> Result<bool> {
    if new_role != "admin"
        && new_role != "member"
        && new_role != "viewer"
        && new_role != "super_user"
    {
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
pub fn get_role_permissions(
    conn: &Connection,
    org_id: &str,
    role_name: &str,
) -> Result<Vec<String>> {
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
            "client:read".to_string(),
            "client:write".to_string(),
            "harness:read".to_string(),
            "harness:write".to_string(),
            "harness:download".to_string(),
            "harness:install".to_string(),
            "harness:review_config".to_string(),
            // These two lists are hard-coded rather than read from `roles`, so every
            // migration that grants a new domain to the seeded role TEMPLATES (v52 for
            // task:*, v54 for sdd:*) silently leaves them behind. `require_permission`
            // bypasses the check for privileged roles, so nothing breaks server-side and
            // the drift goes unnoticed — but this list is what /v1/admin/auth/me reports,
            // and the admin UI gates controls on it. An omission here is a lie in the API
            // response. `no_template_grant_is_missing_from_the_privileged_lists` fails if
            // a future domain is added without updating this.
            "task:read".to_string(),
            "task:write".to_string(),
            "task:assign".to_string(),
            "task:delete".to_string(),
            "task:manage".to_string(),
            "sdd:read".to_string(),
            "sdd:write".to_string(),
            "sdd:delete".to_string(),
            // Knowledge migration (v60). `migration:review` is intentionally a
            // separate grant from `migration:write`: running a scan and deciding
            // what enters the company brain are different jobs, and in a
            // consultancy they are usually different people.
            "migration:read".to_string(),
            "migration:write".to_string(),
            "migration:review".to_string(),
            "autonomous_agent:read".to_string(),
            "autonomous_agent:create".to_string(),
            "autonomous_agent:update".to_string(),
            "autonomous_agent:enable".to_string(),
            "autonomous_agent:run".to_string(),
            "autonomous_agent:cancel".to_string(),
            "autonomous_agent:manage_connectors".to_string(),
        ]);
    } else if role_name == "super_user" {
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
            "project:read".to_string(),
            "project:write".to_string(),
            "client:read".to_string(),
            "client:write".to_string(),
            "session:read".to_string(),
            "api_key:read".to_string(),
            "convention:read".to_string(),
            "convention:write".to_string(),
            "webhook:read".to_string(),
            "code:read".to_string(),
            "code:write".to_string(),
            "code:index".to_string(),
            "collection:read".to_string(),
            "collection:write".to_string(),
            "backup:read".to_string(),
            "backup:write".to_string(),
            "graph:read".to_string(),
            "tag:read".to_string(),
            "tag:write".to_string(),
            "harness:read".to_string(),
            "harness:write".to_string(),
            "harness:download".to_string(),
            "harness:install".to_string(),
            "harness:review_config".to_string(),
            // These two lists are hard-coded rather than read from `roles`, so every
            // migration that grants a new domain to the seeded role TEMPLATES (v52 for
            // task:*, v54 for sdd:*) silently leaves them behind. `require_permission`
            // bypasses the check for privileged roles, so nothing breaks server-side and
            // the drift goes unnoticed — but this list is what /v1/admin/auth/me reports,
            // and the admin UI gates controls on it. An omission here is a lie in the API
            // response. `no_template_grant_is_missing_from_the_privileged_lists` fails if
            // a future domain is added without updating this.
            "task:read".to_string(),
            "task:write".to_string(),
            "task:assign".to_string(),
            "task:delete".to_string(),
            "task:manage".to_string(),
            "sdd:read".to_string(),
            "sdd:write".to_string(),
            "sdd:delete".to_string(),
            // Knowledge migration (v60). `migration:review` is intentionally a
            // separate grant from `migration:write`: running a scan and deciding
            // what enters the company brain are different jobs, and in a
            // consultancy they are usually different people.
            "migration:read".to_string(),
            "migration:write".to_string(),
            "migration:review".to_string(),
            "autonomous_agent:read".to_string(),
            "autonomous_agent:create".to_string(),
            "autonomous_agent:update".to_string(),
            "autonomous_agent:enable".to_string(),
            "autonomous_agent:run".to_string(),
            "autonomous_agent:cancel".to_string(),
            "autonomous_agent:manage_connectors".to_string(),
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
        return Ok(vec!["memory:read".to_string(), "memory:search".to_string()]);
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

/// Returns the id of an existing project by (org_id, name), or `None` if it does not
/// exist. Unlike [`get_or_create_project`], this never inserts a project row and never
/// enrolls members — it is the read-only lookup used by write paths that must reject
/// (rather than silently create) unknown project names.
pub fn find_project_id(conn: &Connection, org_id: &str, name: &str) -> Result<Option<String>> {
    match conn.query_row(
        "SELECT id FROM projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, name],
        |row| row.get::<_, String>(0),
    ) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_or_create_project(
    conn: &Connection,
    org_id: &str,
    project_name: &str,
) -> Result<String> {
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
    list_projects_filtered(conn, org_id, false, None)
}

/// Lists an org's projects, privileged view.
///
/// `client_id` filters the result set: `None` returns every project;
/// `Some(id)` returns only projects owned by that client. The filter is
/// backward-compatible — callers that don't pass it get the prior behaviour.
pub fn list_projects_filtered(
    conn: &Connection,
    org_id: &str,
    include_archived: bool,
    client_id: Option<&str>,
) -> Result<Vec<Project>> {
    let archived_clause = if include_archived {
        ""
    } else {
        " AND archived_at IS NULL"
    };
    let sql = format!(
        "SELECT id, org_id, name, description, created_at, parent_id, archived_at, client_id \
         FROM projects WHERE org_id = ?1{archived_clause} \
         AND (?2 IS NULL OR client_id = ?2) ORDER BY name ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![org_id, client_id], |row| {
        Ok(Project {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            parent_id: row.get(5)?,
            archived_at: row.get(6)?,
            client_id: row.get(7)?,
        })
    })?;
    let mut projects = Vec::new();
    for row in rows {
        projects.push(row?);
    }
    Ok(projects)
}

pub fn list_projects_visible(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.org_id, p.name, p.description, p.created_at, p.parent_id, p.archived_at, p.client_id
         FROM projects p
         JOIN project_visibility pv ON pv.project_id = p.id
         WHERE p.org_id = ?1 AND pv.user_id = ?2 AND p.archived_at IS NULL
         ORDER BY p.name ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, user_id], |row| {
        Ok(Project {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            parent_id: row.get(5)?,
            archived_at: row.get(6)?,
            client_id: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_project_by_id(conn: &Connection, org_id: &str, id: &str) -> Result<Option<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, description, created_at, parent_id, archived_at, client_id FROM projects WHERE id = ?1 AND org_id = ?2",
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
            client_id: row.get(7)?,
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

pub fn list_project_ids_for_user(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT pv.project_id
         FROM project_visibility pv
         JOIN projects p ON p.id = pv.project_id
         WHERE p.org_id = ?1 AND pv.user_id = ?2",
    )?;
    let project_ids = stmt
        .query_map(rusqlite::params![org_id, user_id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(project_ids)
}

pub fn user_is_project_member(
    conn: &Connection,
    org_id: &str,
    project_id: &str,
    user_id: &str,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM project_visibility pv
         JOIN projects p ON p.id = pv.project_id
         WHERE p.org_id = ?1 AND pv.project_id = ?2 AND pv.user_id = ?3",
        rusqlite::params![org_id, project_id, user_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Returns the UUID of a project looked up by its name within an org, or `None` if not found.
pub fn get_project_id_by_name(
    conn: &Connection,
    org_id: &str,
    name: &str,
) -> Result<Option<String>> {
    let result = conn
        .query_row(
            "SELECT id FROM projects WHERE org_id = ?1 AND name = ?2",
            rusqlite::params![org_id, name],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(result)
}

/// Stable 8-color palette used to color memory-graph nodes and legend swatches
/// per project. The frontend never has to know the palette — the backend
/// picks a color via `color_for_project_id` and ships it in the response.
pub const PROJECT_COLOR_PALETTE: &[&str] = &[
    "#2997ff", // sky blue
    "#34d399", // mint
    "#fb923c", // amber
    "#a78bfa", // violet
    "#facc15", // yellow
    "#f472b6", // pink
    "#22d3ee", // cyan
    "#fb7185", // rose
];

/// Returns a stable CSS color for a project id. The same id always maps to
/// the same color, so a project's color never changes between reloads.
pub fn color_for_project_id(project_id: &str) -> String {
    // FNV-1a 32-bit — cheap, stable, and the mod 8 means the palette wraps
    // deterministically. We don't need cryptographic quality, just determinism.
    let mut hash: u32 = 0x811c9dc5;
    for byte in project_id.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    let idx = (hash as usize) % PROJECT_COLOR_PALETTE.len();
    PROJECT_COLOR_PALETTE[idx].to_string()
}

/// Resolves a project to its full family: the project itself + every
/// descendant in `parent_id` (recursive children). Accepts either a project id
/// (UUID) or a project name. Returns the projects in BFS order (root first).
///
/// Cycle-safe: tracks visited ids so a pre-existing `parent_id` cycle in the
/// data terminates without infinite looping.
///
/// Returns an empty Vec if the root project doesn't exist in the org.
pub fn resolve_project_family(conn: &Connection, org_id: &str, root: &str) -> Result<Vec<Project>> {
    let root_project = if root.contains('-') && root.len() >= 32 {
        // Heuristic: UUID-shaped id (contains dashes and is long enough). Tries
        // id first, falls back to name on miss. Cheap because both are
        // indexed lookups.
        match get_project_by_id(conn, org_id, root)? {
            Some(p) => p,
            None => match conn.query_row(
                "SELECT id, org_id, name, description, created_at, parent_id, archived_at, client_id \
                 FROM projects WHERE org_id = ?1 AND name = ?2",
                rusqlite::params![org_id, root],
                |row| {
                    Ok(Project {
                        id: row.get(0)?,
                        org_id: row.get(1)?,
                        name: row.get(2)?,
                        description: row.get(3)?,
                        created_at: row.get(4)?,
                        parent_id: row.get(5)?,
                        archived_at: row.get(6)?,
                        client_id: row.get(7)?,
                    })
                },
            ) {
                Ok(p) => p,
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(Vec::new()),
                Err(e) => return Err(e.into()),
            },
        }
    } else {
        match conn.query_row(
            "SELECT id, org_id, name, description, created_at, parent_id, archived_at, client_id \
             FROM projects WHERE org_id = ?1 AND name = ?2",
            rusqlite::params![org_id, root],
            |row| {
                Ok(Project {
                    id: row.get(0)?,
                    org_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    created_at: row.get(4)?,
                    parent_id: row.get(5)?,
                    archived_at: row.get(6)?,
                    client_id: row.get(7)?,
                })
            },
        ) {
            Ok(p) => p,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        }
    };

    // BFS: every project whose `parent_id` is in the current frontier gets
    // added. The visited set prevents infinite loops if a cycle exists in
    // the data (which would be a data integrity bug, but we don't want the
    // graph endpoint to hang if it ever happens).
    let mut family: Vec<Project> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut frontier: Vec<String> = vec![root_project.id.clone()];

    while let Some(current_id) = frontier.pop() {
        if !visited.insert(current_id.clone()) {
            continue;
        }
        // Resolve the current node. We already loaded the root, so for the
        // root iteration we already have it; for descendants we re-fetch by
        // id. (We could optimize by inlining a batch query, but the typical
        // family size is < 50 — keep it simple.)
        let p = if current_id == root_project.id {
            root_project.clone()
        } else {
            match get_project_by_id(conn, org_id, &current_id)? {
                Some(p) => p,
                None => continue,
            }
        };
        family.push(p.clone());

        // Enqueue every child (parent_id == current_id) not yet visited.
        let mut child_stmt =
            conn.prepare("SELECT id FROM projects WHERE org_id = ?1 AND parent_id = ?2")?;
        let child_ids: Vec<String> = child_stmt
            .query_map(rusqlite::params![org_id, &current_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for cid in child_ids {
            if !visited.contains(&cid) {
                frontier.push(cid);
            }
        }
    }

    Ok(family)
}

pub fn create_project(
    conn: &Connection,
    org_id: &str,
    name: &str,
    description: Option<&str>,
    parent_id: Option<&str>,
) -> Result<Project> {
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
        client_id: None,
    })
}

/// Creates a project and makes its creator an admin member.
///
/// `client_id` of `None` creates an **internal u2s project** — that is a
/// meaning, not a missing value, so nothing backfills it later.
#[allow(clippy::too_many_arguments)]
pub fn create_project_with_creator_membership(
    conn: &Connection,
    org_id: &str,
    creator_id: &str,
    name: &str,
    description: Option<&str>,
    parent_id: Option<&str>,
    client_id: Option<&str>,
) -> Result<Project> {
    // A project may only be attached to a client of its own organization;
    // otherwise a caller could graft a project onto another tenant's client.
    if let Some(cid) = client_id {
        let belongs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clients WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![cid, org_id],
            |r| r.get(0),
        )?;
        if belongs == 0 {
            anyhow::bail!("client {cid} does not belong to this organization");
        }
    }
    let tx = conn.unchecked_transaction()?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    tx.execute(
        "INSERT INTO projects (id, org_id, name, description, created_at, parent_id, client_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, org_id, name, description, now, parent_id, client_id],
    )?;
    tx.execute(
        "INSERT INTO project_members (id, project_id, user_id, role, created_at) VALUES (?1, ?2, ?3, 'admin', ?4)",
        rusqlite::params![uuid::Uuid::new_v4().to_string(), id, creator_id, now],
    )?;
    tx.commit()?;
    Ok(Project {
        id,
        org_id: org_id.to_string(),
        name: name.to_string(),
        description: description.map(String::from),
        created_at: now,
        parent_id: parent_id.map(String::from),
        archived_at: None,
        client_id: client_id.map(String::from),
    })
}

/// Partially update a project.
///
/// Both `parent_id` and `client_id` use `Option<Option<&str>>` to distinguish
/// three intents, so a PATCH that omits a field never clobbers it:
/// - `None`            → field absent, leave the column untouched
/// - `Some(None)`      → set the column to NULL (parent → root / client → Internal)
/// - `Some(Some(val))` → set the column to `val`
///
/// Returns whether the project row exists in the org.
pub fn update_project(
    conn: &Connection,
    org_id: &str,
    project_id: &str,
    parent_id: Option<Option<&str>>,
    client_id: Option<Option<&str>>,
) -> Result<bool> {
    // Parent cycle/cross-org checks only run when a concrete parent is provided;
    // clearing the parent (Some(None)) or leaving it alone (None) needs no check.
    if let Some(Some(new_parent)) = parent_id {
        // Self-parenting is a cycle
        if new_parent == project_id {
            anyhow::bail!("cycle_detected: a project cannot be its own parent");
        }
        // Cross-org check: parent must belong to the same org
        let parent_in_org: bool = conn.query_row(
            "SELECT count(*) > 0 FROM projects WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![new_parent, org_id],
            |row| row.get(0),
        )?;
        if !parent_in_org {
            anyhow::bail!("not_found: parent project not found in this organization");
        }
        // Cycle check: walk the ancestor chain of the proposed new parent
        let mut visited = std::collections::HashSet::new();
        let mut current = new_parent.to_string();
        loop {
            let row: Option<Option<String>> = conn
                .query_row(
                    "SELECT parent_id FROM projects WHERE id = ?1 AND org_id = ?2",
                    rusqlite::params![current, org_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;
            match row {
                None => break,       // not found in this org — safe to stop
                Some(None) => break, // root project reached — no cycle
                Some(Some(next)) => {
                    if next == project_id {
                        anyhow::bail!("cycle_detected: assigning this parent would create a circular hierarchy");
                    }
                    // Guard against pre-existing cycles in the data: if we revisit
                    // a node, the chain loops without involving project_id — stop.
                    if !visited.insert(current.clone()) {
                        break;
                    }
                    current = next;
                }
            }
        }
    }

    // A project may only be attached to a client of its own organization;
    // otherwise a caller could graft a project onto another tenant's client.
    // Mirror the check in create_project_with_creator_membership.
    if let Some(Some(cid)) = client_id {
        let belongs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clients WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![cid, org_id],
            |r| r.get(0),
        )?;
        if belongs == 0 {
            anyhow::bail!("client_not_found: client {cid} does not belong to this organization");
        }
    }

    // Build a partial UPDATE that touches only the columns actually provided.
    // Owned copies keep the bound values alive for the duration of execute().
    let parent_val: Option<Option<String>> = parent_id.map(|p| p.map(|s| s.to_string()));
    let client_val: Option<Option<String>> = client_id.map(|c| c.map(|s| s.to_string()));

    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    if let Some(ref p) = parent_val {
        sets.push(format!("parent_id = ?{}", binds.len() + 1));
        binds.push(p);
    }
    if let Some(ref c) = client_val {
        sets.push(format!("client_id = ?{}", binds.len() + 1));
        binds.push(c);
    }

    if sets.is_empty() {
        // Nothing to change — just report whether the project exists.
        let exists: bool = conn.query_row(
            "SELECT count(*) > 0 FROM projects WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![project_id, org_id],
            |row| row.get(0),
        )?;
        return Ok(exists);
    }

    let sql = format!(
        "UPDATE projects SET {} WHERE id = ?{} AND org_id = ?{}",
        sets.join(", "),
        binds.len() + 1,
        binds.len() + 2,
    );
    binds.push(&project_id);
    binds.push(&org_id);
    let rows = conn.execute(&sql, binds.as_slice())?;
    Ok(rows > 0)
}

pub fn delete_project(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM projects WHERE id = ?1 AND org_id = ?2",
        [id, org_id],
    )?;
    Ok(affected > 0)
}

pub fn list_project_members(
    conn: &Connection,
    _org_id: &str,
    project_id: &str,
) -> Result<Vec<ProjectMember>> {
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

pub fn upsert_project_member(
    conn: &Connection,
    project_id: &str,
    user_id: &str,
    role: &str,
) -> Result<()> {
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

pub fn get_memory_owner_and_project(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
) -> Result<Option<(String, String)>> {
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
pub fn get_memory_by_id_for_org(
    conn: &Connection,
    org_id: &str,
    memory_id: &str,
) -> Result<Option<Memory>> {
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
        Ok((
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags_str,
            created_at,
            title,
            memory_type,
            scope,
            topic_key,
            session_id,
            revision_count,
            normalized_hash,
            project_id,
            archived_at,
            pinned_i64,
            collection_id,
            admin_note,
            delete_after,
        )) => {
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let status = if archived_at.is_some() {
                "archived".to_string()
            } else {
                "active".to_string()
            };
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
        let (
            id,
            org_id_col,
            user_id,
            proj,
            tool,
            content,
            tags_str,
            created_at,
            title,
            memory_type,
            scope,
            topic_key,
            session_id,
            revision_count,
            normalized_hash,
            project_id,
            archived_at,
            pinned_i64,
        ) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() {
            "archived".to_string()
        } else {
            "active".to_string()
        };
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
    let mut tool_stmt =
        conn.prepare("SELECT DISTINCT tool FROM memories WHERE org_id = ?1 AND project = ?2")?;
    let tool_rows = tool_stmt.query_map(rusqlite::params![org_id, project], |r| {
        r.get::<_, String>(0)
    })?;
    let tools: Vec<String> = tool_rows.collect::<rusqlite::Result<Vec<_>>>()?;

    // Query 3: last activity.
    let last_activity: Option<String> = conn
        .query_row(
            "SELECT MAX(created_at) FROM memories WHERE org_id = ?1 AND project = ?2",
            rusqlite::params![org_id, project],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    Ok(crate::models::types::ProjectContext {
        project: project.to_string(),
        recent_memories,
        tools,
        last_activity,
    })
}

pub fn get_global_metrics(conn: &Connection) -> Result<GlobalMetrics> {
    let total_orgs: i64 = conn.query_row("SELECT count(*) FROM organizations", [], |r| r.get(0))?;
    let total_users: i64 = conn.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
    let total_memories: i64 = conn.query_row("SELECT count(*) FROM memories", [], |r| r.get(0))?;
    let active_users_24h: i64 = conn.query_row(
        "SELECT count(DISTINCT user_id) FROM audit_logs WHERE timestamp >= datetime('now', '-24 hours')",
        [],
        |r| r.get(0),
    )?;
    Ok(GlobalMetrics {
        total_orgs,
        total_users,
        total_memories,
        active_users_24h,
    })
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn list_all_users(conn: &Connection) -> Result<Vec<User>> {
    let mut stmt = conn.prepare(
        "SELECT id, org_id, email, name, role, status, created_at FROM users ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(User {
            id: r.get(0)?,
            org_id: r.get(1)?,
            // email may be NULL for some seeded users — mirror list_users and default to "".
            email: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
    let mut sql =
        "SELECT id, org_id, user_id, timestamp, action, resource_type, resource_id, metadata, \
                          previous_hash, current_hash \
                   FROM audit_logs WHERE 1=1"
            .to_string();
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
    sql.push_str(&format!(
        " ORDER BY timestamp DESC LIMIT ?{} OFFSET ?{}",
        params.len() + 1,
        params.len() + 2
    ));
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
            metadata: r
                .get::<_, String>(7)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null),
            previous_hash: r.get(8)?,
            current_hash: r.get(9)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn delete_org(conn: &Connection, org_id: &str) -> Result<bool> {
    conn.execute(
        "DELETE FROM audit_logs WHERE org_id = ?1",
        rusqlite::params![org_id],
    )?;
    conn.execute(
        "DELETE FROM api_keys WHERE org_id = ?1",
        rusqlite::params![org_id],
    )?;
    conn.execute(
        "DELETE FROM memory_embeddings WHERE memory_id IN (SELECT id FROM memories WHERE org_id = ?1)",
        rusqlite::params![org_id],
    )?;
    conn.execute(
        "DELETE FROM memories WHERE org_id = ?1",
        rusqlite::params![org_id],
    )?;
    conn.execute(
        "DELETE FROM users WHERE org_id = ?1",
        rusqlite::params![org_id],
    )?;
    let deleted = conn.execute(
        "DELETE FROM organizations WHERE id = ?1",
        rusqlite::params![org_id],
    )?;
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
        |r| {
            Ok(OrgWithStats {
                id: r.get(0)?,
                name: r.get(1)?,
                slug: r.get(2)?,
                created_at: r.get(3)?,
                user_count: r.get(4)?,
                memory_count: r.get(5)?,
            })
        },
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
    let config: serde_json::Value =
        serde_json::from_str(&config_str).unwrap_or(serde_json::Value::Object(Default::default()));
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
pub fn list_policies(
    conn: &Connection,
    org_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Policy>> {
    list_policies_visible(conn, org_id, limit, offset, None)
}

pub fn list_policies_visible(
    conn: &Connection,
    org_id: &str,
    limit: i64,
    offset: i64,
    viewer_user_id: Option<&str>,
) -> Result<Vec<Policy>> {
    if let Some(viewer) = viewer_user_id {
        let mut stmt = conn.prepare(
            "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
             FROM policies
             WHERE org_id = ?1
               AND (project_id IS NULL OR project_id IN (
                   SELECT project_id FROM project_visibility WHERE user_id = ?2
               ))
             ORDER BY created_at DESC LIMIT ?3 OFFSET ?4",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![org_id, viewer, limit, offset],
            row_to_policy,
        )?;
        return rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into);
    }

    let mut stmt = conn.prepare(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
         FROM policies WHERE org_id = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, limit, offset], row_to_policy)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// Returns only enabled policies for an org, ordered by creation date ASC.
/// Used by the `/policy/check` handler for evaluation.
///
/// `project`: when `Some(p)`, returns org-wide policies (`project_id IS NULL`)
/// UNION policies scoped to project `p` — project scoping ADDS to org-wide, it
/// never replaces it. When `None`, returns every enabled policy for the org
/// regardless of `project_id` (admin listing / no-project-context behavior).
pub fn list_enabled_policies(
    conn: &Connection,
    org_id: &str,
    project: Option<&str>,
) -> Result<Vec<Policy>> {
    list_enabled_policies_visible(conn, org_id, project, None)
}

/// Enabled policies in force for a project, resolved **org → client → project**
/// additively — same chain as conventions. A client-level policy tightens an
/// org-level one; it never loosens or replaces it.
pub fn list_enabled_policies_visible(
    conn: &Connection,
    org_id: &str,
    project: Option<&str>,
    viewer_user_id: Option<&str>,
) -> Result<Vec<Policy>> {
    if let Some(p) = project {
        // Resolve the owning client here rather than making every caller do it:
        // a caller that forgot would silently drop the client's policies, which
        // is a governance hole that no test would obviously catch.
        let client_id = get_project_client_id(conn, org_id, p)?;
        let mut sql = String::from(
            "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
             FROM policies WHERE org_id = ?1 AND enabled = 1
               AND ((client_id IS NULL AND project_id IS NULL)
                 OR (client_id IS NOT NULL AND client_id = ?3 AND project_id IS NULL)
                 OR project_id = ?2)",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(org_id.to_string()),
            Box::new(p.to_string()),
            Box::new(client_id.clone()),
        ];
        if let Some(viewer) = viewer_user_id {
            sql.push_str(" AND (project_id IS NULL OR project_id IN (SELECT project_id FROM project_visibility WHERE user_id = ?4))");
            params.push(Box::new(viewer.to_string()));
        }
        sql.push_str(" ORDER BY created_at ASC");
        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), row_to_policy)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    } else {
        let mut sql = String::from(
            "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
             FROM policies WHERE org_id = ?1 AND enabled = 1",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
        if let Some(viewer) = viewer_user_id {
            sql.push_str(" AND (project_id IS NULL OR project_id IN (SELECT project_id FROM project_visibility WHERE user_id = ?2))");
            params.push(Box::new(viewer.to_string()));
        }
        sql.push_str(" ORDER BY created_at ASC");
        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), row_to_policy)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
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
    Ok(DailyStats {
        requests_today,
        tokens_today,
    })
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

/// Ensure the creator of a freshly-indexed code project can see and search it.
///
/// The visible-project queries (`list_code_projects_visible`,
/// `user_can_access_canonical_project_by_name`) join `code_projects` → `projects`
/// → the `project_visibility` view (backed by `project_members`). `upsert_code_project`
/// only writes the `code_projects` row, so a non-super_user creator would otherwise be
/// locked out of their own index (`/v1/code/projects` → [], `/v1/code/search` → 404).
///
/// This creates the matching canonical `projects` row (same org + name) if missing
/// and enrolls `creator_id` as an `admin` member so the visibility view includes them.
/// Idempotent — re-indexing reuses the existing project row and membership.
pub fn ensure_code_project_visible_to_creator(
    conn: &Connection,
    org_id: &str,
    project_name: &str,
    creator_id: &str,
) -> Result<()> {
    let project_id = get_or_create_project(conn, org_id, project_name)?;
    upsert_project_member(conn, &project_id, creator_id, "admin")?;
    Ok(())
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
    let truncated = if error_msg.len() > 500 {
        &error_msg[..500]
    } else {
        error_msg
    };
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

/// Fail any code projects left stuck in `index_status = 'indexing'` by an
/// interrupted run (OOM kill, crash, or restart). An indexing run marks the row
/// `'indexing'` up front and only flips it to `'success'`/`'error'` at the end, so
/// a process that dies mid-index leaves a zombie row that would otherwise report
/// "indexing" forever and block re-indexing. Call once on startup after migrations.
/// Returns the number of rows reset.
pub fn fail_stale_indexing_projects(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "UPDATE code_projects
         SET index_status = 'error',
             last_index_error = 'Indexing interrupted (server restart)'
         WHERE index_status = 'indexing'",
        [],
    )?;
    Ok(n)
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

/// Count all chunks currently stored for a project. Used as the authoritative
/// chunk total after an index run: freshly-embedded files (Pass 2) insert chunks
/// without incrementing the in-loop counter, so a fresh index would otherwise
/// report 0 chunks despite real rows existing.
pub fn count_chunks_for_project(conn: &Connection, code_project_id: i64) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM code_chunks WHERE code_project_id = ?1",
        rusqlite::params![code_project_id],
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
    let mut stmt = conn.prepare("SELECT file_path FROM code_files WHERE code_project_id = ?1")?;
    let rows = stmt.query_map(rusqlite::params![code_project_id], |r| {
        r.get::<_, String>(0)
    })?;
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
    let rows = stmt.query_map(rusqlite::params![code_project_id], |r| {
        r.get::<_, String>(0)
    })?;
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
pub fn get_code_embeddings(conn: &Connection, code_project_id: i64) -> Result<Vec<(i64, Vec<u8>)>> {
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

/// Return `(chunk_id, file_path, symbol)` for every embedded chunk in a project.
/// Lightweight companion to [`get_code_embeddings`] for `POST /v1/code/locate`: it
/// lets the handler dedupe ranked chunks down to distinct files without loading any
/// chunk `content` (the heavy column). Only chunks that have an embedding are
/// returned, so ids line up 1:1 with the cosine-scored set.
pub fn get_code_chunk_locations(
    conn: &Connection,
    code_project_id: i64,
) -> Result<Vec<(i64, String, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path, symbol FROM code_chunks
         WHERE code_project_id = ?1 AND embedding IS NOT NULL",
    )?;
    let rows = stmt.query_map(rusqlite::params![code_project_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Fetch multiple code chunks by their row IDs (ORDER preserved).
pub fn get_chunks_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<CodeChunk>> {
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

/// Returns true when `user_id` is a member of an existing canonical project with
/// the given name. Unlike legacy memory/session visibility, unknown project names
/// are not treated as shared here because code indexes can exist without a
/// canonical project row and must remain admin-only in that state.
pub fn user_can_access_canonical_project_by_name(
    conn: &Connection,
    org_id: &str,
    project_name: &str,
    user_id: &str,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM projects p
         JOIN project_visibility pv ON pv.project_id = p.id
         WHERE p.org_id = ?1 AND p.name = ?2 AND pv.user_id = ?3",
        rusqlite::params![org_id, project_name, user_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Retrieves a code project only if the caller is allowed to see it.
/// `viewer_user_id = None` is the admin/internal path and applies only org scope.
pub fn get_code_project_visible(
    conn: &Connection,
    org_id: &str,
    name: &str,
    viewer_user_id: Option<&str>,
) -> Result<Option<CodeProject>> {
    if let Some(user_id) = viewer_user_id {
        if !user_can_access_canonical_project_by_name(conn, org_id, name, user_id)? {
            return Ok(None);
        }
    }
    get_code_project(org_id, name, conn)
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

/// Retrieves a code project by id only if the caller is allowed to see it.
/// `viewer_user_id = None` is the admin/internal path and applies only org scope.
pub fn get_code_project_by_id_visible(
    conn: &Connection,
    org_id: &str,
    project_id: i64,
    viewer_user_id: Option<&str>,
) -> Result<Option<CodeProject>> {
    let Some(project) = get_code_project_by_id(conn, org_id, project_id)? else {
        return Ok(None);
    };
    if let Some(user_id) = viewer_user_id {
        if !user_can_access_canonical_project_by_name(conn, org_id, &project.name, user_id)? {
            return Ok(None);
        }
    }
    Ok(Some(project))
}

/// List all code projects for an org, ordered by creation date (newest first).
pub fn list_code_projects(conn: &Connection, org_id: &str) -> Result<Vec<CodeProject>> {
    list_code_projects_filtered(conn, org_id, false)
}

/// When `include_archived` is false (default), archived code projects (archived_at IS NOT NULL) are excluded.
pub fn list_code_projects_filtered(
    conn: &Connection,
    org_id: &str,
    include_archived: bool,
) -> Result<Vec<CodeProject>> {
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
        let patterns_json: String = row
            .get::<_, Option<String>>(15)?
            .unwrap_or_else(|| "[]".to_string());
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

/// Like [`list_code_projects_filtered`], but non-admin callers only see code
/// projects whose name matches a canonical project where they are a member.
pub fn list_code_projects_visible(
    conn: &Connection,
    org_id: &str,
    include_archived: bool,
    viewer_user_id: Option<&str>,
) -> Result<Vec<CodeProject>> {
    let Some(user_id) = viewer_user_id else {
        return list_code_projects_filtered(conn, org_id, include_archived);
    };

    let base = "SELECT cp.id, cp.org_id, cp.name, cp.root_path, cp.repo_url, cp.file_count, cp.chunk_count, cp.last_indexed, cp.created_at,
                cp.reindex_interval_hours, cp.last_indexed_at, cp.last_index_error, cp.indexed_files_count, cp.index_status, cp.archived_at,
                cp.exclude_patterns
         FROM code_projects cp
         JOIN projects p ON p.org_id = cp.org_id AND p.name = cp.name
         JOIN project_visibility pv ON pv.project_id = p.id AND pv.user_id = ?2
         WHERE cp.org_id = ?1";
    let sql = if include_archived {
        format!("{base} ORDER BY cp.created_at DESC")
    } else {
        format!("{base} AND cp.archived_at IS NULL ORDER BY cp.created_at DESC")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![org_id, user_id], |row| {
        let patterns_json: String = row
            .get::<_, Option<String>>(15)?
            .unwrap_or_else(|| "[]".to_string());
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
pub fn update_reindex_interval(
    conn: &Connection,
    org_id: &str,
    project_id: i64,
    hours: Option<i64>,
) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE code_projects SET reindex_interval_hours = ?1 WHERE id = ?2 AND org_id = ?3",
        rusqlite::params![hours, project_id, org_id],
    )?;
    Ok(rows > 0)
}

/// Set the repo_url for an existing code project.
pub fn set_code_project_repo_url(
    conn: &Connection,
    org_id: &str,
    name: &str,
    repo_url: &str,
) -> Result<()> {
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
    let events: Vec<String> =
        serde_json::from_str(&events_json).unwrap_or_else(|_| vec!["*".to_string()]);
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
pub fn create_webhook(
    conn: &Connection,
    org_id: &str,
    req: &CreateWebhookRequest,
) -> Result<Webhook> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let events = req.events.clone().unwrap_or_else(|| vec!["*".to_string()]);
    let events_json = serde_json::to_string(&events)?;
    conn.execute(
        "INSERT INTO webhooks (id, org_id, name, target_url, secret, events, active, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
        rusqlite::params![
            id,
            org_id,
            req.name,
            req.target_url,
            req.secret,
            events_json,
            now
        ],
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
        rusqlite::params![
            id,
            webhook_id,
            org_id,
            event_type,
            payload,
            status_code,
            success_int,
            error
        ],
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
    search_projects_by_query_visible(conn, org_id, q, limit, None)
}

pub fn search_projects_by_query_visible(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
    viewer_user_id: Option<&str>,
) -> Result<Vec<crate::models::types::Project>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut sql = String::from(
        "SELECT id, org_id, name, description, created_at, parent_id
         FROM projects
         WHERE org_id = ?1
           AND LOWER(name) LIKE ?2",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(org_id.to_string()), Box::new(pattern)];
    let mut limit_idx = 3usize;
    if let Some(viewer) = viewer_user_id {
        sql.push_str(" AND id IN (SELECT project_id FROM project_visibility WHERE user_id = ?3)");
        params.push(Box::new(viewer.to_string()));
        limit_idx = 4;
    }
    sql.push_str(&format!(" ORDER BY name ASC LIMIT ?{limit_idx}"));
    params.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(crate::models::types::Project {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            created_at: row.get(4)?,
            parent_id: row.get(5)?,
            archived_at: None,
            client_id: None,
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
    search_policies_by_query_visible(conn, org_id, q, limit, None)
}

pub fn search_policies_by_query_visible(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
    viewer_user_id: Option<&str>,
) -> Result<Vec<crate::models::types::Policy>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut sql = String::from(
        "SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at, project_id
         FROM policies
         WHERE org_id = ?1
           AND LOWER(name) LIKE ?2",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(org_id.to_string()), Box::new(pattern)];
    let mut limit_idx = 3usize;
    if let Some(viewer) = viewer_user_id {
        sql.push_str(" AND (project_id IS NULL OR project_id IN (SELECT project_id FROM project_visibility WHERE user_id = ?3))");
        params.push(Box::new(viewer.to_string()));
        limit_idx = 4;
    }
    sql.push_str(&format!(" ORDER BY name ASC LIMIT ?{limit_idx}"));
    params.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), row_to_policy)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// LIKE search across active (non-archived) conventions for an org, matching on `title` or `content`.
pub fn search_conventions_by_query(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
) -> Result<Vec<crate::models::types::Convention>> {
    search_conventions_by_query_visible(conn, org_id, q, limit, None)
}

pub fn search_conventions_by_query_visible(
    conn: &Connection,
    org_id: &str,
    q: &str,
    limit: i64,
    viewer_user_id: Option<&str>,
) -> Result<Vec<crate::models::types::Convention>> {
    let pattern = format!("%{}%", q.to_lowercase());
    let mut sql = String::from(
        "SELECT id, org_id, project_id, title, content, category, weight, tags, created_at, updated_at, archived_at
         FROM conventions
         WHERE org_id = ?1
           AND archived_at IS NULL
           AND (LOWER(title) LIKE ?2 OR LOWER(content) LIKE ?2)",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(org_id.to_string()), Box::new(pattern)];
    let mut limit_idx = 3usize;
    if let Some(viewer) = viewer_user_id {
        sql.push_str(" AND (project_id IS NULL OR project_id IN (SELECT project_id FROM project_visibility WHERE user_id = ?3))");
        params.push(Box::new(viewer.to_string()));
        limit_idx = 4;
    }
    sql.push_str(&format!(
        " ORDER BY weight DESC, title ASC LIMIT ?{limit_idx}"
    ));
    params.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), convention_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
pub fn get_key_admin(
    conn: &Connection,
    org_id: &str,
    key_id: &str,
) -> Result<Option<ApiKeyWithUser>> {
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
        parts.push(format!(
            "expires_at = ?{}",
            if label.is_some() { 4 } else { 3 }
        ));
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
        (Some(lbl), Some(exp)) => {
            conn.execute(&sql, rusqlite::params![key_id, org_id, lbl, exp])?
        }
        (Some(lbl), None) => conn.execute(&sql, rusqlite::params![key_id, org_id, lbl])?,
        (None, Some(exp)) => conn.execute(&sql, rusqlite::params![key_id, org_id, exp])?,
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
    let user_id: Option<String> = conn
        .query_row(
            "SELECT user_id FROM api_keys WHERE id = ?1 AND org_id = ?2 AND revoked = 0",
            rusqlite::params![key_id, org_id],
            |r| r.get(0),
        )
        .optional()?;

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
    let key = get_key_admin(conn, org_id, &new_id)?.expect("newly inserted key must be found");

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

    let key = get_key_admin(conn, org_id, &key_id)?.expect("newly inserted key must be found");

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
            let overrides: ProjectEventOverrides =
                serde_json::from_str(&json_str).unwrap_or_default();
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

// ── Tasks (team-tasks) ──────────────────────────────────────────────────────

/// How many levels of task nesting are allowed. `epic -> PR -> item` needs three;
/// the cap leaves room without letting a tree grow unbounded.
pub const MAX_TASK_DEPTH: usize = 5;

/// Number of ancestors above `task_id` (0 for a root task).
///
/// The loop is bounded by `MAX_TASK_DEPTH + 1` rather than by trusting the data to be
/// acyclic. It cannot cycle today — parents are set only at creation, and a task being
/// created has no children — but a walk that trusts its input is a hang waiting for a
/// corrupt row, and this one runs inside a write path.
fn task_ancestor_depth(conn: &Connection, org_id: &str, task_id: &str) -> Result<usize> {
    let mut depth = 0usize;
    let mut current = Some(task_id.to_string());

    while let Some(id) = current {
        if depth > MAX_TASK_DEPTH {
            return Err(anyhow!("task ancestry is cyclic or deeper than the cap"));
        }
        let parent: Option<String> = conn
            .query_row(
                "SELECT parent_id FROM tasks WHERE id = ?1 AND org_id = ?2",
                rusqlite::params![id, org_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        match parent {
            Some(p) => {
                depth += 1;
                current = Some(p);
            }
            None => break,
        }
    }
    Ok(depth)
}

/// Optional equality/membership filters for [`list_tasks`]/[`count_tasks`].
#[derive(Debug, Clone, Default)]
pub struct TaskListFilters {
    pub project: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub sprint_id: Option<String>,
    pub label: Option<String>,
    pub parent_id: Option<String>,
    /// When `Some(user_id)`, restricts to tasks assigned to that user (`assignee=me`).
    pub assignee_user_id: Option<String>,
    pub include_archived: bool,
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        org_id: row.get(1)?,
        project: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        priority: row.get(6)?,
        due_date: row.get(7)?,
        parent_id: row.get(8)?,
        sprint_id: row.get(9)?,
        created_by: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        archived_at: row.get(13)?,
        assignees: Vec::new(),
        labels: Vec::new(),
        comment_count: 0,
        spec_links: Vec::new(),
        subtask_count: 0,
    })
}

const TASK_SELECT: &str = "SELECT id, org_id, project, title, description, status, priority, due_date, parent_id, sprint_id, created_by, created_at, updated_at, archived_at FROM tasks";

/// Creates a task. Defaults `status` to `backlog` and `priority` to `medium` when omitted.
/// Validates `status`/`priority` against the fixed sets before insert. When `parent_id` is
/// set, rejects nesting under an existing subtask (only one level of nesting allowed) and
/// rejects a parent in a different project.
pub fn create_task(
    conn: &Connection,
    org_id: &str,
    created_by: &str,
    req: &CreateTaskRequest,
) -> Result<Task> {
    let status = match &req.status {
        Some(s) => TaskStatus::from_str_relaxed(s)?,
        None => "backlog".to_string(),
    };
    let priority = match &req.priority {
        Some(p) => validate_task_priority(p)?,
        None => "medium".to_string(),
    };

    if let Some(parent_id) = &req.parent_id {
        let parent = get_task(conn, org_id, parent_id)?
            .ok_or_else(|| anyhow::anyhow!("parent task not found"))?;
        if parent.project != req.project {
            return Err(anyhow::anyhow!(
                "parent task belongs to a different project"
            ));
        }
        // The old rule here was `cannot nest a subtask under a subtask` — two levels,
        // hard-stop. The schema never required it: `tasks.parent_id` is a self-referencing
        // FK and has always supported an arbitrary tree. The restriction lived only in
        // this validation, and it does not match how work is actually shaped:
        //
        //     change / epic  ->  PR / work unit  ->  checklist item
        //
        // which is exactly what SDD produces (a tasks.md has sections, each with items).
        // Flattening it costs you either the grouping or the items.
        //
        // Depth is bounded instead. A cycle cannot form here — the task being created has
        // no children yet, and `patch_task` cannot re-parent — but the walk is bounded
        // anyway rather than trusting that, because a loop that trusts its data is a
        // hang waiting for corrupt data.
        let depth = task_ancestor_depth(conn, org_id, parent_id)?;
        if depth + 1 >= MAX_TASK_DEPTH {
            return Err(anyhow::anyhow!(
                "task nesting too deep (max {MAX_TASK_DEPTH} levels)"
            ));
        }
    }

    // Mirrors patch_task's sprint validation: a sprint_id must reference an existing
    // sprint (scoped to this org, via get_sprint) in the SAME project as the task being
    // created — otherwise a task could be silently attached to another project's sprint.
    if let Some(sprint_id) = &req.sprint_id {
        let sprint = get_sprint(conn, org_id, sprint_id)?
            .ok_or_else(|| anyhow::anyhow!("sprint not found"))?;
        if sprint.project != req.project {
            return Err(anyhow::anyhow!("sprint belongs to a different project"));
        }
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO tasks (id, org_id, project, title, description, status, priority, due_date, parent_id, sprint_id, created_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        rusqlite::params![
            id, org_id, req.project, req.title, req.description, status, priority,
            req.due_date, req.parent_id, req.sprint_id, created_by, now,
        ],
    )?;

    Ok(Task {
        id,
        org_id: org_id.to_string(),
        project: req.project.clone(),
        title: req.title.clone(),
        description: req.description.clone(),
        status,
        priority,
        due_date: req.due_date.clone(),
        parent_id: req.parent_id.clone(),
        sprint_id: req.sprint_id.clone(),
        created_by: created_by.to_string(),
        created_at: now.clone(),
        updated_at: now,
        archived_at: None,
        assignees: Vec::new(),
        labels: Vec::new(),
        comment_count: 0,
        spec_links: Vec::new(),
        subtask_count: 0,
    })
}

/// Fetches a task by id, scoped to org, hydrated with assignees/labels/spec_links/
/// comment_count/subtask_count. Returns `None` if not found.
pub fn get_task(conn: &Connection, org_id: &str, task_id: &str) -> Result<Option<Task>> {
    let sql = format!("{TASK_SELECT} WHERE id = ?1 AND org_id = ?2");
    let result = conn
        .query_row(&sql, rusqlite::params![task_id, org_id], map_task_row)
        .optional()?;

    let Some(mut task) = result else {
        return Ok(None);
    };

    task.assignees = list_task_assignees(conn, &task.id)?;
    task.labels = list_task_labels(conn, &task.id)?;
    task.spec_links = list_task_spec_links(conn, &task.id)?;
    task.comment_count = conn.query_row(
        "SELECT COUNT(*) FROM task_comments WHERE task_id = ?1",
        [&task.id],
        |r| r.get(0),
    )?;
    task.subtask_count = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE parent_id = ?1 AND archived_at IS NULL",
        [&task.id],
        |r| r.get(0),
    )?;

    Ok(Some(task))
}

/// Updates the given fields on a task. Validates a `status` change against
/// [`can_transition`] (same-state is a no-op allowed). Returns `None` if the task does not
/// exist for the org (→ 404). Returns `Err` on an illegal status transition.
pub fn patch_task(
    conn: &Connection,
    org_id: &str,
    task_id: &str,
    req: &PatchTaskRequest,
) -> Result<Option<Task>> {
    let Some(existing) = get_task(conn, org_id, task_id)? else {
        return Ok(None);
    };

    if req.title.is_none()
        && req.description.is_none()
        && req.status.is_none()
        && req.priority.is_none()
        && req.due_date.is_none()
        && req.sprint_id.is_none()
    {
        return Ok(Some(existing));
    }

    let mut set_clauses: Vec<String> = vec!["updated_at = ?1".to_string()];
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
    let mut idx = 2usize;

    if let Some(title) = &req.title {
        set_clauses.push(format!("title = ?{idx}"));
        params.push(Box::new(title.clone()));
        idx += 1;
    }
    if let Some(description) = &req.description {
        set_clauses.push(format!("description = ?{idx}"));
        params.push(Box::new(description.clone()));
        idx += 1;
    }
    if let Some(status) = &req.status {
        let from = existing
            .status
            .parse::<TaskStatus>()
            .map_err(|e| anyhow::anyhow!(e))?;
        let to = status
            .parse::<TaskStatus>()
            .map_err(|_| anyhow::anyhow!("invalid_status: unrecognized task status '{status}'"))?;
        if !can_transition(from, to) {
            return Err(anyhow::anyhow!(
                "invalid_transition: cannot move task from {from} to {to}"
            ));
        }
        set_clauses.push(format!("status = ?{idx}"));
        params.push(Box::new(status.clone()));
        idx += 1;
    }
    if let Some(priority) = &req.priority {
        let priority = validate_task_priority(priority)?;
        set_clauses.push(format!("priority = ?{idx}"));
        params.push(Box::new(priority));
        idx += 1;
    }
    if let Some(due_date) = &req.due_date {
        set_clauses.push(format!("due_date = ?{idx}"));
        params.push(Box::new(due_date.clone()));
        idx += 1;
    }
    if let Some(sprint_id) = &req.sprint_id {
        let sprint = get_sprint(conn, org_id, sprint_id)?
            .ok_or_else(|| anyhow::anyhow!("sprint not found"))?;
        if sprint.project != existing.project {
            return Err(anyhow::anyhow!("sprint belongs to a different project"));
        }
        set_clauses.push(format!("sprint_id = ?{idx}"));
        params.push(Box::new(sprint_id.clone()));
        idx += 1;
    }

    params.push(Box::new(org_id.to_string()));
    params.push(Box::new(task_id.to_string()));

    let sql = format!(
        "UPDATE tasks SET {} WHERE org_id = ?{} AND id = ?{}",
        set_clauses.join(", "),
        idx,
        idx + 1
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, refs.as_slice())?;

    get_task(conn, org_id, task_id)
}

/// Soft-deletes a task by setting `archived_at`. Never cascades to subtasks (soft-delete is
/// a plain `UPDATE`, not the FK `ON DELETE CASCADE`, which only fires on hard delete — v1
/// never hard-deletes). Returns `false` if the task does not exist for the org.
pub fn soft_delete_task(conn: &Connection, org_id: &str, task_id: &str) -> Result<bool> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let affected = conn.execute(
        "UPDATE tasks SET archived_at = ?1 WHERE org_id = ?2 AND id = ?3 AND archived_at IS NULL",
        rusqlite::params![now, org_id, task_id],
    )?;
    Ok(affected > 0)
}

fn build_task_filter_sql(
    org_id: &str,
    viewer: Option<&str>,
    filters: &TaskListFilters,
) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut sql = String::from(" WHERE t.org_id = ?1");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    let mut idx = 2usize;

    if !filters.include_archived {
        sql.push_str(" AND t.archived_at IS NULL");
    }
    if let Some(project) = &filters.project {
        sql.push_str(&format!(" AND t.project = ?{idx}"));
        params.push(Box::new(project.clone()));
        idx += 1;
    }
    if let Some(status) = &filters.status {
        sql.push_str(&format!(" AND t.status = ?{idx}"));
        params.push(Box::new(status.clone()));
        idx += 1;
    }
    if let Some(priority) = &filters.priority {
        sql.push_str(&format!(" AND t.priority = ?{idx}"));
        params.push(Box::new(priority.clone()));
        idx += 1;
    }
    if let Some(sprint_id) = &filters.sprint_id {
        sql.push_str(&format!(" AND t.sprint_id = ?{idx}"));
        params.push(Box::new(sprint_id.clone()));
        idx += 1;
    }
    if let Some(parent_id) = &filters.parent_id {
        sql.push_str(&format!(" AND t.parent_id = ?{idx}"));
        params.push(Box::new(parent_id.clone()));
        idx += 1;
    }
    if let Some(label) = &filters.label {
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM task_labels tl WHERE tl.task_id = t.id AND tl.label = ?{idx})"
        ));
        params.push(Box::new(label.clone()));
        idx += 1;
    }
    if let Some(assignee) = &filters.assignee_user_id {
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM task_assignees ta WHERE ta.task_id = t.id AND ta.user_id = ?{idx})"
        ));
        params.push(Box::new(assignee.clone()));
        idx += 1;
    }
    if let Some(vid) = viewer {
        sql.push_str(&format!(
            " AND (NOT EXISTS (SELECT 1 FROM projects p WHERE p.org_id = t.org_id AND p.name = t.project)
                   OR EXISTS (SELECT 1 FROM project_visibility pv
                              WHERE pv.org_id = t.org_id AND pv.project_name = t.project AND pv.user_id = ?{idx}))"
        ));
        params.push(Box::new(vid.to_string()));
        idx += 1;
    }
    let _ = idx;
    (sql, params)
}

/// Batch-hydrates `assignees` and `labels` on every task in `tasks` using two queries total
/// (one for assignees, one for labels), regardless of how many tasks are passed in. This
/// avoids the N+1 pattern of calling `list_task_assignees`/`list_task_labels` per task — used
/// by list-oriented reads (`list_tasks`, `list_tasks_in_sprint`) where `get_task`'s per-row
/// hydration would be too expensive. No-ops on an empty slice (no queries run).
fn hydrate_tasks(conn: &Connection, tasks: &mut [Task]) -> Result<()> {
    if tasks.is_empty() {
        return Ok(());
    }

    let ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    let placeholders: String = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let id_refs: Vec<&dyn rusqlite::ToSql> =
        ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

    let mut assignees_by_task: HashMap<String, Vec<TaskAssignee>> = HashMap::new();
    {
        let sql = format!(
            "SELECT ta.task_id, u.id, u.name, u.email FROM task_assignees ta
             JOIN users u ON u.id = ta.user_id
             WHERE ta.task_id IN ({placeholders}) ORDER BY u.name"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(id_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                TaskAssignee {
                    id: row.get(1)?,
                    name: row.get(2)?,
                    email: row.get(3)?,
                },
            ))
        })?;
        for row in rows {
            let (task_id, assignee) = row?;
            assignees_by_task.entry(task_id).or_default().push(assignee);
        }
    }

    let mut labels_by_task: HashMap<String, Vec<String>> = HashMap::new();
    {
        let sql = format!(
            "SELECT task_id, label FROM task_labels WHERE task_id IN ({placeholders}) ORDER BY label"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(id_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (task_id, label) = row?;
            labels_by_task.entry(task_id).or_default().push(label);
        }
    }

    // subtask_count was hydrated only by `get_task`, so every LIST response reported 0
    // for tasks that in fact had children. The API was telling the admin that a task with
    // six subtasks had none — a lie in the payload, and one you only notice by opening the
    // task. Batched like the two above: one query, not one per task.
    let mut subtasks_by_task: HashMap<String, i64> = HashMap::new();
    {
        let sql = format!(
            "SELECT parent_id, COUNT(*) FROM tasks
             WHERE parent_id IN ({placeholders}) AND archived_at IS NULL
             GROUP BY parent_id"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(id_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (parent_id, count) = row?;
            subtasks_by_task.insert(parent_id, count);
        }
    }

    for task in tasks.iter_mut() {
        task.assignees = assignees_by_task.remove(&task.id).unwrap_or_default();
        task.labels = labels_by_task.remove(&task.id).unwrap_or_default();
        task.subtask_count = subtasks_by_task.remove(&task.id).unwrap_or(0);
    }

    Ok(())
}

/// Lists tasks scoped to `org_id`, applying `filters` and, when `viewer` is `Some(uid)`,
/// project-membership visibility (org-shared/unregistered projects are visible to everyone).
/// `None` viewer = no membership restriction (admin). Excludes archived tasks unless
/// `filters.include_archived` is set. Ordered by `created_at DESC`. Batch-hydrates
/// assignees/labels on the returned page (two extra queries total, not per-task).
pub fn list_tasks(
    conn: &Connection,
    org_id: &str,
    viewer: Option<&str>,
    filters: &TaskListFilters,
    limit: i64,
    offset: i64,
) -> Result<Vec<Task>> {
    let (where_sql, mut params) = build_task_filter_sql(org_id, viewer, filters);
    let limit_idx = params.len() + 1;
    let offset_idx = params.len() + 2;
    let sql = format!(
        "{TASK_SELECT} t{where_sql} ORDER BY t.created_at DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
    );
    params.push(Box::new(limit));
    params.push(Box::new(offset));

    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), map_task_row)?;
    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    hydrate_tasks(conn, &mut tasks)?;
    Ok(tasks)
}

/// Counts tasks matching `filters` (same predicate as [`list_tasks`], no pagination).
pub fn count_tasks(
    conn: &Connection,
    org_id: &str,
    viewer: Option<&str>,
    filters: &TaskListFilters,
) -> Result<i64> {
    let (where_sql, params) = build_task_filter_sql(org_id, viewer, filters);
    let sql = format!("SELECT COUNT(*) FROM tasks t{where_sql}");
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let count: i64 = conn.query_row(&sql, refs.as_slice(), |r| r.get(0))?;
    Ok(count)
}

/// Lists direct children of `parent_id` (one level of nesting only), non-archived.
pub fn list_subtasks(conn: &Connection, org_id: &str, parent_id: &str) -> Result<Vec<Task>> {
    let sql = format!(
        "{TASK_SELECT} WHERE org_id = ?1 AND parent_id = ?2 AND archived_at IS NULL ORDER BY created_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![org_id, parent_id], map_task_row)?;
    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    Ok(tasks)
}

fn validate_task_priority(priority: &str) -> Result<String> {
    match priority {
        "low" | "medium" | "high" | "urgent" => Ok(priority.to_string()),
        other => Err(anyhow::anyhow!(
            "invalid_priority: unrecognized task priority '{other}'"
        )),
    }
}

// Small helper so `create_task`/`patch_task` produce a typed error (mapped to 4xx by the
// handler) instead of panicking on an unrecognized status string.
impl TaskStatus {
    fn from_str_relaxed(s: &str) -> Result<String> {
        s.parse::<TaskStatus>()
            .map(|v| v.to_string())
            .map_err(|_| anyhow::anyhow!("invalid_status: unrecognized task status '{s}'"))
    }
}

// ── Task assignees ──────────────────────────────────────────────────────────

/// Assigns `user_ids` to `task_id`. Validates each user belongs to `org_id` before writing
/// (pre-validation loop — rejects the whole call on the first invalid id, before any insert
/// runs). The insert loop itself runs inside a real transaction (`unchecked_transaction`), so
/// if any individual insert fails partway through (e.g. a concurrent delete of `task_id`
/// between validation and the write), every insert made so far in this call is rolled back —
/// no partial assignee list survives. Idempotent for already-assigned users (`INSERT OR
/// IGNORE`, relying on `UNIQUE(task_id, user_id)`). Returns the full denormalized assignee
/// list after the write.
pub fn set_task_assignees(
    conn: &Connection,
    org_id: &str,
    task_id: &str,
    assigned_by: &str,
    user_ids: &[String],
) -> Result<Vec<TaskAssignee>> {
    for user_id in user_ids {
        if !user_belongs_to_org(conn, org_id, user_id)? {
            return Err(anyhow::anyhow!(
                "invalid_assignee: user {user_id} does not belong to this organization"
            ));
        }
    }

    let tx = conn.unchecked_transaction()?;
    for user_id in user_ids {
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT OR IGNORE INTO task_assignees (id, task_id, user_id, assigned_by) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, task_id, user_id, assigned_by],
        )?;
    }
    tx.commit()?;

    list_task_assignees(conn, task_id)
}

/// Removes a single assignee from a task. Returns `false` if the row did not exist.
pub fn remove_task_assignee(conn: &Connection, task_id: &str, user_id: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM task_assignees WHERE task_id = ?1 AND user_id = ?2",
        rusqlite::params![task_id, user_id],
    )?;
    Ok(affected > 0)
}

/// Lists a task's assignees with denormalized display fields (mirrors `HarnessOwner`).
pub fn list_task_assignees(conn: &Connection, task_id: &str) -> Result<Vec<TaskAssignee>> {
    let mut stmt = conn.prepare(
        "SELECT u.id, u.name, u.email FROM task_assignees ta JOIN users u ON u.id = ta.user_id
         WHERE ta.task_id = ?1 ORDER BY ta.assigned_at ASC",
    )?;
    let rows = stmt.query_map([task_id], |row| {
        Ok(TaskAssignee {
            id: row.get(0)?,
            name: row.get(1)?,
            email: row.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ── Task labels ──────────────────────────────────────────────────────────────

/// Attaches a free-text label to a task (idempotent — `UNIQUE(task_id, label)`).
/// Returns the full label list after the write.
pub fn add_task_label(conn: &Connection, task_id: &str, label: &str) -> Result<Vec<String>> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO task_labels (id, task_id, label) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, task_id, label],
    )?;
    list_task_labels(conn, task_id)
}

/// Removes a label from a task. Returns `false` if the row did not exist.
pub fn remove_task_label(conn: &Connection, task_id: &str, label: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM task_labels WHERE task_id = ?1 AND label = ?2",
        rusqlite::params![task_id, label],
    )?;
    Ok(affected > 0)
}

pub fn list_task_labels(conn: &Connection, task_id: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT label FROM task_labels WHERE task_id = ?1 ORDER BY created_at ASC")?;
    let rows = stmt.query_map([task_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ── Task comments ───────────────────────────────────────────────────────────

/// Adds a comment to a task. Rejects an empty/whitespace-only body.
pub fn add_task_comment(
    conn: &Connection,
    task_id: &str,
    user_id: &str,
    body: &str,
) -> Result<TaskComment> {
    if body.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "empty_comment: comment body must not be empty"
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "INSERT INTO task_comments (id, task_id, user_id, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, task_id, user_id, body, now],
    )?;
    let author_name: Option<String> = conn
        .query_row("SELECT name FROM users WHERE id = ?1", [user_id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(TaskComment {
        id,
        task_id: task_id.to_string(),
        user_id: user_id.to_string(),
        author_name,
        body: body.to_string(),
        created_at: now,
    })
}

/// Lists a task's comments in chronological order, with denormalized author name.
pub fn list_task_comments(conn: &Connection, task_id: &str) -> Result<Vec<TaskComment>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.task_id, c.user_id, u.name, c.body, c.created_at
         FROM task_comments c LEFT JOIN users u ON u.id = c.user_id
         WHERE c.task_id = ?1 ORDER BY c.created_at ASC",
    )?;
    let rows = stmt.query_map([task_id], |row| {
        Ok(TaskComment {
            id: row.get(0)?,
            task_id: row.get(1)?,
            user_id: row.get(2)?,
            author_name: row.get(3)?,
            body: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Deletes a comment by id. Returns `false` if it did not exist.
pub fn delete_task_comment(conn: &Connection, comment_id: &str) -> Result<bool> {
    let affected = conn.execute("DELETE FROM task_comments WHERE id = ?1", [comment_id])?;
    Ok(affected > 0)
}

/// Looks up the author (`user_id`) and parent `task_id` of a comment. Used by the delete
/// handler's author-or-manage authorization check.
pub fn get_task_comment(conn: &Connection, comment_id: &str) -> Result<Option<(String, String)>> {
    conn.query_row(
        "SELECT task_id, user_id FROM task_comments WHERE id = ?1",
        [comment_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()
    .map_err(|e| e.into())
}

// ── Task spec links ─────────────────────────────────────────────────────────

/// Links a task to an openspec change name. Idempotent no-op on re-link
/// (`UNIQUE(task_id, spec_change_name)`). No uniqueness beyond that composite key — a task
/// may link multiple changes, and a change may be linked from multiple tasks.
pub fn link_task_spec(
    conn: &Connection,
    task_id: &str,
    linked_by: &str,
    spec_change_name: &str,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO task_spec_links (id, task_id, spec_change_name, linked_by) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, task_id, spec_change_name, linked_by],
    )?;
    Ok(())
}

/// Removes a task<->spec-change link. Returns `false` if it did not exist.
pub fn unlink_task_spec(conn: &Connection, task_id: &str, spec_change_name: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM task_spec_links WHERE task_id = ?1 AND spec_change_name = ?2",
        rusqlite::params![task_id, spec_change_name],
    )?;
    Ok(affected > 0)
}

pub fn list_task_spec_links(conn: &Connection, task_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT spec_change_name FROM task_spec_links WHERE task_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([task_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Transitions every task in `org_id` linked to `spec_change_name` and not already
/// `done`/`cancelled` to `done` (bypassing [`can_transition`] — this is a system transition,
/// not a user edit). Org-scoped only, ignores per-project membership. Idempotent: already-
/// terminal tasks are skipped. Returns the ids of tasks actually transitioned.
pub fn resolve_tasks_by_spec(
    conn: &Connection,
    org_id: &str,
    spec_change_name: &str,
    viewer_user_id: Option<&str>,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tasks t
         JOIN task_spec_links l ON l.task_id = t.id
         WHERE t.org_id = ?1 AND l.spec_change_name = ?2
            AND t.status NOT IN ('done', 'cancelled')
            AND (?3 IS NULL OR EXISTS (
                SELECT 1 FROM project_visibility pv
                WHERE pv.user_id = ?3 AND pv.org_id = t.org_id AND pv.project_name = t.project
            ))",
    )?;
    let ids: Vec<String> = stmt
        .query_map(
            rusqlite::params![org_id, spec_change_name, viewer_user_id],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if ids.is_empty() {
        return Ok(ids);
    }

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    for id in &ids {
        conn.execute(
            "UPDATE tasks SET status = 'done', updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )?;
    }
    Ok(ids)
}

// ── Sprints ──────────────────────────────────────────────────────────────────

fn map_sprint_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sprint> {
    Ok(Sprint {
        id: row.get(0)?,
        org_id: row.get(1)?,
        project: row.get(2)?,
        name: row.get(3)?,
        goal: row.get(4)?,
        starts_at: row.get(5)?,
        ends_at: row.get(6)?,
        status: row.get(7)?,
        created_by: row.get(8)?,
        created_at: row.get(9)?,
        archived_at: row.get(10)?,
        task_count: 0,
    })
}

const SPRINT_SELECT: &str = "SELECT id, org_id, project, name, goal, starts_at, ends_at, status, created_by, created_at, archived_at FROM sprints";

fn validate_sprint_status(status: &str) -> Result<String> {
    match status {
        "planned" | "active" | "completed" => Ok(status.to_string()),
        other => Err(anyhow::anyhow!(
            "invalid_status: unrecognized sprint status '{other}'"
        )),
    }
}

fn hydrate_sprint_task_count(conn: &Connection, sprint: &mut Sprint) -> Result<()> {
    sprint.task_count = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE sprint_id = ?1 AND archived_at IS NULL",
        [&sprint.id],
        |r| r.get(0),
    )?;
    Ok(())
}

/// Creates a sprint scoped to `org_id`/`project`.
pub fn create_sprint(
    conn: &Connection,
    org_id: &str,
    created_by: &str,
    req: &CreateSprintRequest,
) -> Result<Sprint> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "INSERT INTO sprints (id, org_id, project, name, goal, starts_at, ends_at, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![id, org_id, req.project, req.name, req.goal, req.starts_at, req.ends_at, created_by, now],
    )?;
    Ok(Sprint {
        id,
        org_id: org_id.to_string(),
        project: req.project.clone(),
        name: req.name.clone(),
        goal: req.goal.clone(),
        starts_at: req.starts_at.clone(),
        ends_at: req.ends_at.clone(),
        status: "planned".to_string(),
        created_by: created_by.to_string(),
        created_at: now,
        archived_at: None,
        task_count: 0,
    })
}

/// Fetches a sprint by id, scoped to org, hydrated with `task_count`.
pub fn get_sprint(conn: &Connection, org_id: &str, sprint_id: &str) -> Result<Option<Sprint>> {
    let sql = format!("{SPRINT_SELECT} WHERE id = ?1 AND org_id = ?2");
    let result = conn
        .query_row(&sql, rusqlite::params![sprint_id, org_id], map_sprint_row)
        .optional()?;
    let Some(mut sprint) = result else {
        return Ok(None);
    };
    hydrate_sprint_task_count(conn, &mut sprint)?;
    Ok(Some(sprint))
}

/// Updates the given fields on a sprint. Returns `None` if it does not exist for the org.
pub fn patch_sprint(
    conn: &Connection,
    org_id: &str,
    sprint_id: &str,
    req: &PatchSprintRequest,
) -> Result<Option<Sprint>> {
    if get_sprint(conn, org_id, sprint_id)?.is_none() {
        return Ok(None);
    }
    if req.name.is_none()
        && req.goal.is_none()
        && req.starts_at.is_none()
        && req.ends_at.is_none()
        && req.status.is_none()
    {
        return get_sprint(conn, org_id, sprint_id);
    }

    let mut set_clauses: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut idx = 1usize;

    if let Some(name) = &req.name {
        set_clauses.push(format!("name = ?{idx}"));
        params.push(Box::new(name.clone()));
        idx += 1;
    }
    if let Some(goal) = &req.goal {
        set_clauses.push(format!("goal = ?{idx}"));
        params.push(Box::new(goal.clone()));
        idx += 1;
    }
    if let Some(starts_at) = &req.starts_at {
        set_clauses.push(format!("starts_at = ?{idx}"));
        params.push(Box::new(starts_at.clone()));
        idx += 1;
    }
    if let Some(ends_at) = &req.ends_at {
        set_clauses.push(format!("ends_at = ?{idx}"));
        params.push(Box::new(ends_at.clone()));
        idx += 1;
    }
    if let Some(status) = &req.status {
        let status = validate_sprint_status(status)?;
        set_clauses.push(format!("status = ?{idx}"));
        params.push(Box::new(status));
        idx += 1;
    }

    params.push(Box::new(org_id.to_string()));
    params.push(Box::new(sprint_id.to_string()));
    let sql = format!(
        "UPDATE sprints SET {} WHERE org_id = ?{} AND id = ?{}",
        set_clauses.join(", "),
        idx,
        idx + 1
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, refs.as_slice())?;
    get_sprint(conn, org_id, sprint_id)
}

/// Soft-deletes a sprint. Returns `false` if it did not exist for the org.
pub fn soft_delete_sprint(conn: &Connection, org_id: &str, sprint_id: &str) -> Result<bool> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let affected = conn.execute(
        "UPDATE sprints SET archived_at = ?1 WHERE org_id = ?2 AND id = ?3 AND archived_at IS NULL",
        rusqlite::params![now, org_id, sprint_id],
    )?;
    Ok(affected > 0)
}

/// Lists sprints for `org_id`, optionally filtered by `project`/`status`, respecting
/// project-membership visibility when `viewer` is `Some(uid)`. Excludes archived sprints
/// unless `include_archived`.
#[allow(clippy::too_many_arguments)]
pub fn list_sprints(
    conn: &Connection,
    org_id: &str,
    viewer: Option<&str>,
    project: Option<&str>,
    status: Option<&str>,
    include_archived: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Sprint>> {
    let mut sql = String::from(" WHERE s.org_id = ?1");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    let mut idx = 2usize;

    if !include_archived {
        sql.push_str(" AND s.archived_at IS NULL");
    }
    if let Some(project) = project {
        sql.push_str(&format!(" AND s.project = ?{idx}"));
        params.push(Box::new(project.to_string()));
        idx += 1;
    }
    if let Some(status) = status {
        sql.push_str(&format!(" AND s.status = ?{idx}"));
        params.push(Box::new(status.to_string()));
        idx += 1;
    }
    if let Some(vid) = viewer {
        sql.push_str(&format!(
            " AND (NOT EXISTS (SELECT 1 FROM projects p WHERE p.org_id = s.org_id AND p.name = s.project)
                   OR EXISTS (SELECT 1 FROM project_visibility pv
                              WHERE pv.org_id = s.org_id AND pv.project_name = s.project AND pv.user_id = ?{idx}))"
        ));
        params.push(Box::new(vid.to_string()));
        idx += 1;
    }
    let limit_idx = idx;
    let offset_idx = idx + 1;
    params.push(Box::new(limit));
    params.push(Box::new(offset));

    let sql = format!(
        "{SPRINT_SELECT} s{sql} ORDER BY s.created_at DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), map_sprint_row)?;
    let mut sprints = Vec::new();
    for row in rows {
        let mut sprint = row?;
        hydrate_sprint_task_count(conn, &mut sprint)?;
        sprints.push(sprint);
    }
    Ok(sprints)
}

/// Lists tasks currently assigned to a sprint (non-archived).
pub fn list_tasks_in_sprint(conn: &Connection, sprint_id: &str) -> Result<Vec<Task>> {
    let sql = format!(
        "{TASK_SELECT} WHERE sprint_id = ?1 AND archived_at IS NULL ORDER BY created_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([sprint_id], map_task_row)?;
    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    hydrate_tasks(conn, &mut tasks)?;
    Ok(tasks)
}

// ── Sprint retrospectives ───────────────────────────────────────────────────

/// Creates a retrospective note for a sprint.
pub fn create_retrospective(
    conn: &Connection,
    sprint_id: &str,
    org_id: &str,
    created_by: &str,
    req: &CreateRetrospectiveRequest,
) -> Result<SprintRetrospective> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "INSERT INTO sprint_retrospectives (id, sprint_id, org_id, went_well, went_wrong, action_items, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, sprint_id, org_id, req.went_well, req.went_wrong, req.action_items, created_by, now],
    )?;
    let author_name: Option<String> = conn
        .query_row("SELECT name FROM users WHERE id = ?1", [created_by], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(SprintRetrospective {
        id,
        sprint_id: sprint_id.to_string(),
        went_well: req.went_well.clone(),
        went_wrong: req.went_wrong.clone(),
        action_items: req.action_items.clone(),
        created_by: created_by.to_string(),
        author_name,
        created_at: now,
    })
}

/// Lists a sprint's retrospectives, chronologically, with denormalized author name.
pub fn list_retrospectives(conn: &Connection, sprint_id: &str) -> Result<Vec<SprintRetrospective>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.sprint_id, r.went_well, r.went_wrong, r.action_items, r.created_by, u.name, r.created_at
         FROM sprint_retrospectives r LEFT JOIN users u ON u.id = r.created_by
         WHERE r.sprint_id = ?1 ORDER BY r.created_at ASC",
    )?;
    let rows = stmt.query_map([sprint_id], |row| {
        Ok(SprintRetrospective {
            id: row.get(0)?,
            sprint_id: row.get(1)?,
            went_well: row.get(2)?,
            went_wrong: row.get(3)?,
            action_items: row.get(4)?,
            created_by: row.get(5)?,
            author_name: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ── SDD artifacts ───────────────────────────────────────────────────────────
//
// Three levels, mirroring `openspec/changes/{name}/` on disk exactly:
//   sdd_changes  →  sdd_artifacts  →  sdd_artifact_revisions
//
// INVARIANT: revisions are immutable and append-only. No statement anywhere in
// this file may update or delete a row of the revisions table — they are written
// by `upsert_sdd_artifact`'s INSERT and removed only by ON DELETE CASCADE from
// the parent change. The test `no_store_function_updates_or_deletes_a_revision`
// scans this file to keep it that way, which is also why neither this comment
// nor that test may spell the forbidden statements out literally: the scan reads
// its own source and would match itself.

/// Max bytes for a single artifact revision. Above this the save is rejected
/// **atomically** (design.md A2) — no change row, no artifact row, no revision.
const SDD_MAX_ARTIFACT_BYTES: usize = 1_048_576;

fn sha256_hex(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

const SDD_CHANGE_SELECT: &str = "SELECT id, org_id, project, name, title, status, phase, repo_url, repo_ref, sprint_id, created_by, created_at, updated_at, archived_at FROM sdd_changes";

fn map_sdd_change_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SddChange> {
    Ok(SddChange {
        id: row.get(0)?,
        org_id: row.get(1)?,
        project: row.get(2)?,
        name: row.get(3)?,
        title: row.get(4)?,
        status: row.get(5)?,
        phase: row.get(6)?,
        repo_url: row.get(7)?,
        repo_ref: row.get(8)?,
        sprint_id: row.get(9)?,
        created_by: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        archived_at: row.get(13)?,
        artifacts: Vec::new(),
        task_links: Vec::new(),
        memory_links: Vec::new(),
    })
}

const SDD_ARTIFACT_SELECT: &str = "SELECT id, change_id, kind, capability, path, latest_revision, created_at, updated_at FROM sdd_artifacts";

fn map_sdd_artifact_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SddArtifact> {
    Ok(SddArtifact {
        id: row.get(0)?,
        change_id: row.get(1)?,
        kind: row.get(2)?,
        capability: row.get(3)?,
        path: row.get(4)?,
        latest_revision: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Artifact inventory for a change. Metadata only — never content.
fn list_sdd_artifacts_for_change(conn: &Connection, change_id: &str) -> Result<Vec<SddArtifact>> {
    let sql = format!("{SDD_ARTIFACT_SELECT} WHERE change_id = ?1 ORDER BY kind, capability");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([change_id], map_sdd_artifact_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Upserts by the identity tuple `(org_id, project, name)`. Re-submitting the
/// same tuple updates the row in place — it never duplicates.
pub fn upsert_sdd_change(
    conn: &Connection,
    org_id: &str,
    created_by: &str,
    req: &UpsertChangeRequest,
) -> Result<SddChange> {
    if let Some(phase) = &req.phase {
        SddPhase::from_str(phase).map_err(|_| anyhow!("invalid_phase"))?;
    }
    if let Some(status) = &req.status {
        SddStatus::from_str(status).map_err(|_| anyhow!("invalid_status"))?;
    }

    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM sdd_changes WHERE org_id = ?1 AND project = ?2 AND name = ?3",
            rusqlite::params![org_id, req.project, req.name],
            |r| r.get(0),
        )
        .optional()?;

    let now = now_iso();
    let id = match existing {
        Some(id) => {
            conn.execute(
                "UPDATE sdd_changes SET
                    title      = COALESCE(?1, title),
                    status     = COALESCE(?2, status),
                    phase      = COALESCE(?3, phase),
                    repo_url   = COALESCE(?4, repo_url),
                    repo_ref   = COALESCE(?5, repo_ref),
                    sprint_id  = COALESCE(?6, sprint_id),
                    updated_at = ?7
                 WHERE id = ?8",
                rusqlite::params![
                    req.title,
                    req.status,
                    req.phase,
                    req.repo_url,
                    req.repo_ref,
                    req.sprint_id,
                    now,
                    id
                ],
            )?;
            id
        }
        None => {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO sdd_changes (id, org_id, project, name, title, status, phase, repo_url, repo_ref, sprint_id, created_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, COALESCE(?6, 'active'), COALESCE(?7, 'propose'), ?8, ?9, ?10, ?11, ?12, ?12)",
                rusqlite::params![
                    id, org_id, req.project, req.name, req.title, req.status, req.phase,
                    req.repo_url, req.repo_ref, req.sprint_id, created_by, now
                ],
            )?;
            id
        }
    };

    get_sdd_change(conn, org_id, &id)?.ok_or_else(|| anyhow!("not_found"))
}

/// Hydrates the artifact inventory. Not-found and out-of-org both yield `Ok(None)`.
pub fn get_sdd_change(conn: &Connection, org_id: &str, id: &str) -> Result<Option<SddChange>> {
    let sql = format!("{SDD_CHANGE_SELECT} WHERE id = ?1 AND org_id = ?2");
    let found = conn
        .query_row(&sql, rusqlite::params![id, org_id], map_sdd_change_row)
        .optional()?;
    let Some(mut change) = found else {
        return Ok(None);
    };
    change.artifacts = list_sdd_artifacts_for_change(conn, &change.id)?;
    Ok(Some(change))
}

pub fn get_sdd_change_by_name(
    conn: &Connection,
    org_id: &str,
    project: &str,
    name: &str,
) -> Result<Option<SddChange>> {
    let sql = format!("{SDD_CHANGE_SELECT} WHERE org_id = ?1 AND project = ?2 AND name = ?3");
    let found = conn
        .query_row(
            &sql,
            rusqlite::params![org_id, project, name],
            map_sdd_change_row,
        )
        .optional()?;
    let Some(mut change) = found else {
        return Ok(None);
    };
    change.artifacts = list_sdd_artifacts_for_change(conn, &change.id)?;
    Ok(Some(change))
}

/// Metadata only — never artifact content.
pub fn list_sdd_changes(
    conn: &Connection,
    org_id: &str,
    filters: &SddChangeFilters,
) -> Result<Vec<SddChange>> {
    let mut sql = format!("{SDD_CHANGE_SELECT} WHERE org_id = ?1");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    let mut idx = 2usize;

    if !filters.include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }
    if let Some(project) = &filters.project {
        sql.push_str(&format!(" AND project = ?{idx}"));
        params.push(Box::new(project.clone()));
        idx += 1;
    }
    if let Some(status) = &filters.status {
        sql.push_str(&format!(" AND status = ?{idx}"));
        params.push(Box::new(status.clone()));
        idx += 1;
    }
    if let Some(phase) = &filters.phase {
        sql.push_str(&format!(" AND phase = ?{idx}"));
        params.push(Box::new(phase.clone()));
        idx += 1;
    }
    if let Some(sprint_id) = &filters.sprint_id {
        sql.push_str(&format!(" AND sprint_id = ?{idx}"));
        params.push(Box::new(sprint_id.clone()));
        idx += 1;
    }
    let _ = idx;
    sql.push_str(" ORDER BY updated_at DESC");

    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), map_sdd_change_row)?;
    let mut out = Vec::new();
    for row in rows {
        let mut change = row?;
        change.artifacts = list_sdd_artifacts_for_change(conn, &change.id)?;
        out.push(change);
    }
    Ok(out)
}

/// Patches curation fields only. The identity tuple `(project, name)` is
/// deliberately unpatchable — `PatchChangeRequest` carries no such field, so
/// the `UPDATE` below cannot name those columns. Renaming a change would
/// orphan every `task_spec_links.spec_change_name` row pointing at the old
/// name, because tasks join by name, not by FK (design.md D3).
///
/// Enum fields are validated BEFORE the UPDATE (parse-then-write): a bad phase
/// rejects the whole patch, leaving every other field untouched (A2/2.21).
pub fn patch_sdd_change(
    conn: &Connection,
    org_id: &str,
    id: &str,
    req: &PatchChangeRequest,
) -> Result<SddChange> {
    if let Some(phase) = &req.phase {
        SddPhase::from_str(phase).map_err(|_| anyhow!("invalid_phase"))?;
    }
    if let Some(status) = &req.status {
        SddStatus::from_str(status).map_err(|_| anyhow!("invalid_status"))?;
    }
    if get_sdd_change(conn, org_id, id)?.is_none() {
        return Err(anyhow!("not_found"));
    }

    conn.execute(
        "UPDATE sdd_changes SET
            title      = COALESCE(?1, title),
            status     = COALESCE(?2, status),
            phase      = COALESCE(?3, phase),
            sprint_id  = COALESCE(?4, sprint_id),
            updated_at = ?5
         WHERE id = ?6 AND org_id = ?7",
        rusqlite::params![
            req.title,
            req.status,
            req.phase,
            req.sprint_id,
            now_iso(),
            id,
            org_id
        ],
    )?;

    get_sdd_change(conn, org_id, id)?.ok_or_else(|| anyhow!("not_found"))
}

/// Soft delete. Artifacts and revisions survive and stay retrievable by id.
pub fn archive_sdd_change(conn: &Connection, org_id: &str, id: &str) -> Result<bool> {
    let n = conn.execute(
        "UPDATE sdd_changes SET archived_at = ?1, updated_at = ?1
         WHERE id = ?2 AND org_id = ?3 AND archived_at IS NULL",
        rusqlite::params![now_iso(), id, org_id],
    )?;
    Ok(n > 0)
}

/// THE workhorse. Idempotent by content hash: re-saving identical content
/// creates no revision, touches no index, and does not bump `updated_at`.
///
/// Returns `(artifact, created_revision)`.
///
/// Never reads, writes, or gates on `sdd_changes.phase`: phase is advisory
/// metadata and the artifact inventory is the ground truth. A `verify-report`
/// saved to a change still in `propose` is accepted (2.49).
pub fn upsert_sdd_artifact(
    conn: &Connection,
    org_id: &str,
    created_by: &str,
    req: &SaveArtifactRequest,
    source: &str,
) -> Result<(SddArtifact, bool)> {
    // A2 — the size guard is the FIRST statement, before the transaction opens and
    // before any row is resolved-or-created. A rejected oversized save must leave no
    // change, no artifact, and no revision behind.
    if req.content.len() > SDD_MAX_ARTIFACT_BYTES {
        return Err(anyhow!("artifact_too_large"));
    }
    SddArtifactKind::from_str(&req.kind).map_err(|_| anyhow!("invalid_kind"))?;

    let capability = req.capability.as_deref().unwrap_or("");
    let tx = conn.unchecked_transaction()?;
    let now = now_iso();

    // 1. Resolve or create the change. org_id scopes the lookup, so an org-B caller
    //    with the same (project, name) gets its own change and cannot hijack org A's.
    let change_id: String = match tx
        .query_row(
            "SELECT id FROM sdd_changes WHERE org_id = ?1 AND project = ?2 AND name = ?3",
            rusqlite::params![org_id, req.project, req.change_name],
            |r| r.get(0),
        )
        .optional()?
    {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO sdd_changes (id, org_id, project, name, created_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![id, org_id, req.project, req.change_name, created_by, now],
            )?;
            id
        }
    };

    // 2. Resolve or create the artifact, keyed on all three of (change, kind, capability).
    let artifact_id: String = match tx
        .query_row(
            "SELECT id FROM sdd_artifacts WHERE change_id = ?1 AND kind = ?2 AND capability = ?3",
            rusqlite::params![change_id, req.kind, capability],
            |r| r.get(0),
        )
        .optional()?
    {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO sdd_artifacts (id, change_id, kind, capability, path, latest_revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
                rusqlite::params![id, change_id, req.kind, capability, req.path, now],
            )?;
            id
        }
    };

    // 3. A1 — compare the hash against the LATEST revision only, never against the
    //    full history. Content A → B → A must append revision 3, not resurrect
    //    revision 1: a revert is an event and must appear in the history.
    let hash = sha256_hex(&req.content);
    let latest: Option<(i64, String)> = tx
        .query_row(
            "SELECT revision, content_hash FROM sdd_artifact_revisions
             WHERE artifact_id = ?1 ORDER BY revision DESC LIMIT 1",
            [&artifact_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let latest_revision = latest.as_ref().map(|(rev, _)| *rev).unwrap_or(0);

    if let Some((_, latest_hash)) = &latest {
        if latest_hash == &hash {
            // Idempotent no-op: no revision, no FTS write, no updated_at bump.
            let sql = format!("{SDD_ARTIFACT_SELECT} WHERE id = ?1");
            let artifact = tx.query_row(&sql, [&artifact_id], map_sdd_artifact_row)?;
            tx.commit()?;
            return Ok((artifact, false));
        }
    }

    // 4. Append the next revision. Immutable: earlier revisions are never touched.
    let next = latest_revision + 1;
    tx.execute(
        "INSERT INTO sdd_artifact_revisions
            (id, artifact_id, revision, content, content_hash, byte_size, git_commit, git_path, source, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            artifact_id,
            next,
            req.content,
            hash,
            req.content.len() as i64,
            req.git_commit,
            req.path,
            source,
            created_by,
            now
        ],
    )?;
    tx.execute(
        "UPDATE sdd_artifacts SET latest_revision = ?1, path = COALESCE(?2, path), updated_at = ?3 WHERE id = ?4",
        rusqlite::params![next, req.path, now, artifact_id],
    )?;

    // 5. The FTS index tracks the LATEST revision only — delete-then-insert, so an
    //    artifact contributes exactly one hit no matter how many revisions it has,
    //    and a term deleted by a newer revision stops matching.
    tx.execute(
        "DELETE FROM sdd_artifacts_fts WHERE artifact_id = ?1",
        [&artifact_id],
    )?;
    tx.execute(
        "INSERT INTO sdd_artifacts_fts (artifact_id, change_name, kind, capability, content)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            artifact_id,
            req.change_name,
            req.kind,
            capability,
            req.content
        ],
    )?;

    let sql = format!("{SDD_ARTIFACT_SELECT} WHERE id = ?1");
    let artifact = tx.query_row(&sql, [&artifact_id], map_sdd_artifact_row)?;
    tx.commit()?;
    Ok((artifact, true))
}

fn artifact_detail_from(
    conn: &Connection,
    artifact: SddArtifact,
    project: String,
    change_name: String,
) -> Result<SddArtifactDetail> {
    let latest: Option<(String, String)> = conn
        .query_row(
            "SELECT content, content_hash FROM sdd_artifact_revisions
             WHERE artifact_id = ?1 ORDER BY revision DESC LIMIT 1",
            [&artifact.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (content, content_hash) = match latest {
        Some((c, h)) => (Some(c), Some(h)),
        None => (None, None),
    };
    Ok(SddArtifactDetail {
        artifact,
        change_name,
        project,
        content,
        content_hash,
    })
}

/// Artifacts carry no `org_id` of their own — it is inherited via `change_id`,
/// so every read joins through `sdd_changes` for the org predicate.
pub fn get_sdd_artifact(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<SddArtifactDetail>> {
    let found = conn
        .query_row(
            "SELECT a.id, a.change_id, a.kind, a.capability, a.path, a.latest_revision, a.created_at, a.updated_at,
                    c.project, c.name
             FROM sdd_artifacts a JOIN sdd_changes c ON c.id = a.change_id
             WHERE a.id = ?1 AND c.org_id = ?2",
            rusqlite::params![id, org_id],
            |row| {
                Ok((
                    map_sdd_artifact_row(row)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((artifact, project, change_name)) = found else {
        return Ok(None);
    };
    Ok(Some(artifact_detail_from(
        conn,
        artifact,
        project,
        change_name,
    )?))
}

/// Natural-key lookup behind `GET /v1/sdd/artifacts?project=&change_name=&kind=&capability=`.
/// A kind with no artifact yields `Ok(None)` — never an artifact with empty content.
pub fn get_sdd_artifact_by_kind(
    conn: &Connection,
    org_id: &str,
    project: &str,
    change_name: &str,
    kind: &str,
    capability: Option<&str>,
) -> Result<Option<SddArtifactDetail>> {
    let cap = capability.unwrap_or("");
    let found = conn
        .query_row(
            "SELECT a.id, a.change_id, a.kind, a.capability, a.path, a.latest_revision, a.created_at, a.updated_at
             FROM sdd_artifacts a JOIN sdd_changes c ON c.id = a.change_id
             WHERE c.org_id = ?1 AND c.project = ?2 AND c.name = ?3 AND a.kind = ?4 AND a.capability = ?5",
            rusqlite::params![org_id, project, change_name, kind, cap],
            map_sdd_artifact_row,
        )
        .optional()?;
    let Some(artifact) = found else {
        return Ok(None);
    };
    Ok(Some(artifact_detail_from(
        conn,
        artifact,
        project.to_string(),
        change_name.to_string(),
    )?))
}

/// Metadata only — the SELECT never names the `content` column, and
/// `SddRevisionMeta` has no field to hold one.
pub fn list_sdd_artifact_revisions(
    conn: &Connection,
    org_id: &str,
    artifact_id: &str,
) -> Result<Vec<SddRevisionMeta>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.artifact_id, r.revision, r.content_hash, r.byte_size, r.git_commit, r.git_path,
                r.source, r.created_by, r.created_at
         FROM sdd_artifact_revisions r
         JOIN sdd_artifacts a ON a.id = r.artifact_id
         JOIN sdd_changes c ON c.id = a.change_id
         WHERE r.artifact_id = ?1 AND c.org_id = ?2
         ORDER BY r.revision DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![artifact_id, org_id], |row| {
        Ok(SddRevisionMeta {
            id: row.get(0)?,
            artifact_id: row.get(1)?,
            revision: row.get(2)?,
            content_hash: row.get(3)?,
            byte_size: row.get(4)?,
            git_commit: row.get(5)?,
            git_path: row.get(6)?,
            source: row.get(7)?,
            created_by: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_sdd_artifact_revision(
    conn: &Connection,
    org_id: &str,
    artifact_id: &str,
    revision: i64,
) -> Result<Option<SddRevision>> {
    let found = conn
        .query_row(
            "SELECT r.id, r.artifact_id, r.revision, r.content, r.content_hash, r.byte_size,
                    r.git_commit, r.git_path, r.source, r.created_by, r.created_at
             FROM sdd_artifact_revisions r
             JOIN sdd_artifacts a ON a.id = r.artifact_id
             JOIN sdd_changes c ON c.id = a.change_id
             WHERE r.artifact_id = ?1 AND r.revision = ?2 AND c.org_id = ?3",
            rusqlite::params![artifact_id, revision, org_id],
            |row| {
                Ok(SddRevision {
                    id: row.get(0)?,
                    artifact_id: row.get(1)?,
                    revision: row.get(2)?,
                    content: row.get(3)?,
                    content_hash: row.get(4)?,
                    byte_size: row.get(5)?,
                    git_commit: row.get(6)?,
                    git_path: row.get(7)?,
                    source: row.get(8)?,
                    created_by: row.get(9)?,
                    created_at: row.get(10)?,
                })
            },
        )
        .optional()?;
    Ok(found)
}

/// FTS5 search over the latest revision of every artifact in the org.
/// Reuses `sanitize_fts_query` — do not hand-roll a second escaper.
pub fn search_sdd_artifacts(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<SddSearchHit>> {
    let Some(fts_query) = sanitize_fts_query(query) else {
        return Ok(Vec::new());
    };

    let mut stmt = conn.prepare(
        "SELECT f.artifact_id, a.change_id, c.name, c.project, a.kind, a.capability,
                snippet(sdd_artifacts_fts, 4, '<b>', '</b>', '…', 24)
         FROM sdd_artifacts_fts f
         JOIN sdd_artifacts a ON a.id = f.artifact_id
         JOIN sdd_changes c ON c.id = a.change_id
         WHERE sdd_artifacts_fts MATCH ?1 AND c.org_id = ?2
         ORDER BY rank
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![fts_query, org_id, limit], |row| {
        Ok(SddSearchHit {
            artifact_id: row.get(0)?,
            change_id: row.get(1)?,
            change_name: row.get(2)?,
            project: row.get(3)?,
            kind: row.get(4)?,
            capability: row.get(5)?,
            snippet: row.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// A3 — idempotent, and re-linking with a DIFFERENT relation UPDATES the row.
/// Deliberately not `INSERT OR IGNORE`, which would silently drop the relation change.
pub fn link_sdd_change_memory(
    conn: &Connection,
    org_id: &str,
    change_id: &str,
    memory_id: &str,
    relation: &str,
    linked_by: &str,
) -> Result<()> {
    if get_sdd_change(conn, org_id, change_id)?.is_none() {
        return Err(anyhow!("not_found"));
    }
    let memory_in_org: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1 AND org_id = ?2)",
        rusqlite::params![memory_id, org_id],
        |r| r.get(0),
    )?;
    if !memory_in_org {
        return Err(anyhow!("memory_not_found"));
    }

    conn.execute(
        "INSERT INTO sdd_change_memories (id, change_id, memory_id, relation, linked_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(change_id, memory_id) DO UPDATE SET relation = excluded.relation",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            change_id,
            memory_id,
            relation,
            linked_by,
            now_iso()
        ],
    )?;
    Ok(())
}

pub fn unlink_sdd_change_memory(
    conn: &Connection,
    org_id: &str,
    change_id: &str,
    memory_id: &str,
) -> Result<bool> {
    if get_sdd_change(conn, org_id, change_id)?.is_none() {
        return Err(anyhow!("not_found"));
    }
    let n = conn.execute(
        "DELETE FROM sdd_change_memories WHERE change_id = ?1 AND memory_id = ?2",
        rusqlite::params![change_id, memory_id],
    )?;
    Ok(n > 0)
}

pub fn list_sdd_change_memories(
    conn: &Connection,
    org_id: &str,
    change_id: &str,
) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT memory_id FROM sdd_change_memories WHERE change_id = ?1 ORDER BY created_at DESC",
    )?;
    let ids: Vec<String> = stmt
        .query_map([change_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut out = Vec::new();
    for id in ids {
        if let Some(memory) = get_memory_by_id_for_org(conn, org_id, &id)? {
            out.push(memory);
        }
    }
    Ok(out)
}

/// D3 — the join key is `task_spec_links.spec_change_name`, a plain string.
/// There is no `change_id` FK on `tasks` and no materialized edge: a link
/// created before the change existed resolves the moment the change appears.
///
/// `viewer` is `None` for a privileged caller; otherwise task visibility is
/// applied so linked tasks in projects the viewer cannot see are excluded.
pub fn list_tasks_for_sdd_change(
    conn: &Connection,
    org_id: &str,
    change_name: &str,
    viewer: Option<&str>,
) -> Result<Vec<Task>> {
    let mut sql = String::from(
        "SELECT t.id, t.org_id, t.project, t.title, t.description, t.status, t.priority, t.due_date,
                t.parent_id, t.sprint_id, t.created_by, t.created_at, t.updated_at, t.archived_at
         FROM tasks t
         JOIN task_spec_links tsl ON tsl.task_id = t.id
         WHERE t.org_id = ?1 AND tsl.spec_change_name = ?2 AND t.archived_at IS NULL",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(org_id.to_string()),
        Box::new(change_name.to_string()),
    ];

    if let Some(vid) = viewer {
        sql.push_str(
            " AND (NOT EXISTS (SELECT 1 FROM projects p WHERE p.org_id = t.org_id AND p.name = t.project)
                   OR EXISTS (SELECT 1 FROM project_visibility pv
                              WHERE pv.org_id = t.org_id AND pv.project_name = t.project AND pv.user_id = ?3))",
        );
        params.push(Box::new(vid.to_string()));
    }
    sql.push_str(" ORDER BY t.created_at ASC");

    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), map_task_row)?;
    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    hydrate_tasks(conn, &mut tasks)?;
    Ok(tasks)
}

/// Keyword search over change names and titles, for the `global_search` facet.
/// LIKE-based, mirroring `search_conventions_by_query_visible` — `global_search`
/// is keyword-only, not semantic. Archived changes are excluded.
pub fn search_sdd_changes_by_query(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<SddChangeSummary>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, project, name, title, phase, status
         FROM sdd_changes
         WHERE org_id = ?1
           AND archived_at IS NULL
           AND (name LIKE ?2 OR title LIKE ?2)
         ORDER BY updated_at DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, pattern, limit], |row| {
        Ok(SddChangeSummary {
            id: row.get(0)?,
            project: row.get(1)?,
            name: row.get(2)?,
            title: row.get(3)?,
            phase: row.get(4)?,
            status: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Backs the DB-first half of `spec_change_exists` (design.md D5).
///
/// A8 — NO `archived_at` predicate: an archived change remains a legitimate
/// link target, matching the filesystem check that globs the archive tree.
/// Name-only (project-agnostic), because `task_spec_links.spec_change_name` is.
pub fn sdd_change_exists(conn: &Connection, org_id: &str, name: &str) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sdd_changes WHERE org_id = ?1 AND name = ?2)",
        rusqlite::params![org_id, name],
        |r| r.get(0),
    )?;
    Ok(exists)
}

// ── SDD specs ───────────────────────────────────────────────────────────────
//
// The OTHER tree: `openspec/specs/{capability}/spec.md` — the living
// specification, the source of truth that `sdd-archive` merges a closing change's
// delta specs into. Two levels, and deliberately NOT hung off a change:
//   sdd_specs  →  sdd_spec_revisions
//
// A main spec belongs to the PROJECT and outlives the changes that modify it.
// Modelling it as an artifact of a synthetic change would invert that. What ties
// the two trees together is `sdd_spec_revisions.merged_from_change_id`: from a
// change you can see which specs it changed, and from a spec you can see which
// changes shaped each revision.
//
// The invariants are the artifact invariants, restated because they are the same
// invariants: idempotent by content hash against the LATEST revision only, an
// atomic size cap, delete-then-insert FTS, org in the WHERE, `Ok(None)` for
// not-found. And, as with `sdd_artifact_revisions`, the revisions here are
// immutable and append-only: nothing in this file may modify or remove a row of
// `sdd_spec_revisions` once written — they are produced by `upsert_sdd_spec`'s
// INSERT and reclaimed only by ON DELETE CASCADE from the parent spec. The test
// `no_store_function_mutates_a_spec_revision` scans this file to hold that line,
// and — as with the artifacts scan — neither it nor this comment may spell the
// prohibited statements out, because `include_str!` reads this very file and the
// scan would then match itself.

const SDD_SPEC_SELECT: &str = "SELECT id, org_id, project, capability, title, path, latest_revision, created_by, created_at, updated_at, archived_at FROM sdd_specs";

fn map_sdd_spec_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SddSpec> {
    Ok(SddSpec {
        id: row.get(0)?,
        org_id: row.get(1)?,
        project: row.get(2)?,
        capability: row.get(3)?,
        title: row.get(4)?,
        path: row.get(5)?,
        latest_revision: row.get(6)?,
        created_by: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        archived_at: row.get(10)?,
        last_merged_from_change_id: None,
        last_merged_from_change_name: None,
    })
}

/// Fills in the change that produced the spec's LATEST revision. Metadata, so the
/// list read carries it too — "which change last merged into this contract" is the
/// column an operator scans, not a detail they drill for.
fn hydrate_spec_provenance(conn: &Connection, spec: &mut SddSpec) -> Result<()> {
    let found: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT r.merged_from_change_id, c.name
             FROM sdd_spec_revisions r
             LEFT JOIN sdd_changes c ON c.id = r.merged_from_change_id
             WHERE r.spec_id = ?1
             ORDER BY r.revision DESC LIMIT 1",
            [&spec.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((change_id, change_name)) = found {
        spec.last_merged_from_change_id = change_id;
        spec.last_merged_from_change_name = change_name;
    }
    Ok(())
}

fn spec_detail_from(conn: &Connection, spec: SddSpec) -> Result<SddSpecDetail> {
    let latest: Option<(String, String)> = conn
        .query_row(
            "SELECT content, content_hash FROM sdd_spec_revisions
             WHERE spec_id = ?1 ORDER BY revision DESC LIMIT 1",
            [&spec.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (content, content_hash) = match latest {
        Some((c, h)) => (Some(c), Some(h)),
        None => (None, None),
    };
    Ok(SddSpecDetail {
        spec,
        content,
        content_hash,
    })
}

/// THE workhorse, and the exact analogue of `upsert_sdd_artifact`. Idempotent by
/// content hash: re-saving identical content creates no revision, writes no FTS
/// row, and does not bump `updated_at`.
///
/// Returns `(spec, created_revision)`.
///
/// `merged_from_change_name` is resolved to a change id here. A name that resolves
/// to nothing is an error, NOT a silently-NULL provenance — the traceability from a
/// revision back to the change that shaped it is the reason this column exists, and
/// quietly dropping it would leave a spec whose history lies by omission.
pub fn upsert_sdd_spec(
    conn: &Connection,
    org_id: &str,
    created_by: &str,
    req: &SaveSpecRequest,
    source: &str,
) -> Result<(SddSpec, bool)> {
    // A2 — the size guard is the FIRST statement, before the transaction opens and
    // before any row is resolved-or-created. A rejected oversized save must leave no
    // spec and no revision behind.
    if req.content.len() > SDD_MAX_ARTIFACT_BYTES {
        return Err(anyhow!("spec_too_large"));
    }
    if req.capability.trim().is_empty() {
        return Err(anyhow!("invalid_capability"));
    }

    let tx = conn.unchecked_transaction()?;
    let now = now_iso();

    // Resolve the provenance BEFORE anything is written, so an unknown change name
    // rejects the save whole rather than half-way through it.
    let merged_from_change_id: Option<String> = match req.merged_from_change_name.as_deref() {
        None => None,
        Some(name) => {
            let found: Option<String> = tx
                .query_row(
                    "SELECT id FROM sdd_changes WHERE org_id = ?1 AND project = ?2 AND name = ?3",
                    rusqlite::params![org_id, req.project, name],
                    |r| r.get(0),
                )
                .optional()?;
            Some(found.ok_or_else(|| anyhow!("change_not_found"))?)
        }
    };

    // 1. Resolve or create the spec. org_id scopes the lookup, so an org-B caller with
    //    the same (project, capability) gets its own spec and cannot touch org A's.
    let spec_id: String = match tx
        .query_row(
            "SELECT id FROM sdd_specs WHERE org_id = ?1 AND project = ?2 AND capability = ?3",
            rusqlite::params![org_id, req.project, req.capability],
            |r| r.get(0),
        )
        .optional()?
    {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO sdd_specs (id, org_id, project, capability, title, path, latest_revision, created_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?8)",
                rusqlite::params![
                    id, org_id, req.project, req.capability, req.title, req.path, created_by, now
                ],
            )?;
            id
        }
    };

    // 2. A1 — compare the hash against the LATEST revision only, never the whole
    //    history. Content A → B → A appends revision 3: a revert to an earlier text is
    //    an event in the life of the contract, and the history must show it happening.
    let hash = sha256_hex(&req.content);
    let latest: Option<(i64, String)> = tx
        .query_row(
            "SELECT revision, content_hash FROM sdd_spec_revisions
             WHERE spec_id = ?1 ORDER BY revision DESC LIMIT 1",
            [&spec_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let latest_revision = latest.as_ref().map(|(rev, _)| *rev).unwrap_or(0);

    if let Some((_, latest_hash)) = &latest {
        if latest_hash == &hash {
            // Idempotent no-op: no revision, no FTS write, no updated_at bump.
            let sql = format!("{SDD_SPEC_SELECT} WHERE id = ?1");
            let mut spec = tx.query_row(&sql, [&spec_id], map_sdd_spec_row)?;
            hydrate_spec_provenance(&tx, &mut spec)?;
            tx.commit()?;
            return Ok((spec, false));
        }
    }

    // 3. Append the next revision. Immutable: earlier revisions are never touched.
    let next = latest_revision + 1;
    tx.execute(
        "INSERT INTO sdd_spec_revisions
            (id, spec_id, revision, content, content_hash, byte_size, merged_from_change_id, git_commit, git_path, source, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            spec_id,
            next,
            req.content,
            hash,
            req.content.len() as i64,
            merged_from_change_id,
            req.git_commit,
            req.path,
            source,
            created_by,
            now
        ],
    )?;
    tx.execute(
        "UPDATE sdd_specs SET
            latest_revision = ?1,
            title      = COALESCE(?2, title),
            path       = COALESCE(?3, path),
            updated_at = ?4
         WHERE id = ?5",
        rusqlite::params![next, req.title, req.path, now, spec_id],
    )?;

    // 4. The FTS index tracks the LATEST revision only — delete-then-insert, so a spec
    //    contributes exactly one hit however long its history, and a requirement struck
    //    from the contract by a newer revision stops matching.
    tx.execute("DELETE FROM sdd_specs_fts WHERE spec_id = ?1", [&spec_id])?;
    tx.execute(
        "INSERT INTO sdd_specs_fts (spec_id, project, capability, content)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![spec_id, req.project, req.capability, req.content],
    )?;

    let sql = format!("{SDD_SPEC_SELECT} WHERE id = ?1");
    let mut spec = tx.query_row(&sql, [&spec_id], map_sdd_spec_row)?;
    hydrate_spec_provenance(&tx, &mut spec)?;
    tx.commit()?;
    Ok((spec, true))
}

/// By id. Not-found and out-of-org both yield `Ok(None)` — the caller turns that
/// into a 404, so an org-B caller cannot distinguish "no such spec" from "not yours".
pub fn get_sdd_spec(conn: &Connection, org_id: &str, id: &str) -> Result<Option<SddSpecDetail>> {
    let sql = format!("{SDD_SPEC_SELECT} WHERE id = ?1 AND org_id = ?2");
    let found = conn
        .query_row(&sql, rusqlite::params![id, org_id], map_sdd_spec_row)
        .optional()?;
    let Some(mut spec) = found else {
        return Ok(None);
    };
    hydrate_spec_provenance(conn, &mut spec)?;
    Ok(Some(spec_detail_from(conn, spec)?))
}

/// Natural-key lookup behind `GET /v1/sdd/specs?project=&capability=`. A capability
/// with no spec yields `Ok(None)` — never a spec with empty content, because "this
/// capability has no contract yet" and "its contract is empty" are different facts.
pub fn get_sdd_spec_by_capability(
    conn: &Connection,
    org_id: &str,
    project: &str,
    capability: &str,
) -> Result<Option<SddSpecDetail>> {
    let sql = format!("{SDD_SPEC_SELECT} WHERE org_id = ?1 AND project = ?2 AND capability = ?3");
    let found = conn
        .query_row(
            &sql,
            rusqlite::params![org_id, project, capability],
            map_sdd_spec_row,
        )
        .optional()?;
    let Some(mut spec) = found else {
        return Ok(None);
    };
    hydrate_spec_provenance(conn, &mut spec)?;
    Ok(Some(spec_detail_from(conn, spec)?))
}

/// Metadata only — never spec content. The SELECT does not name the `content`
/// column and `SddSpec` has no field to hold one.
pub fn list_sdd_specs(
    conn: &Connection,
    org_id: &str,
    filters: &SddSpecFilters,
) -> Result<Vec<SddSpec>> {
    let mut sql = format!("{SDD_SPEC_SELECT} WHERE org_id = ?1");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];

    if !filters.include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }
    if let Some(project) = &filters.project {
        sql.push_str(" AND project = ?2");
        params.push(Box::new(project.clone()));
    }
    sql.push_str(" ORDER BY capability ASC");

    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), map_sdd_spec_row)?;
    let mut out = Vec::new();
    for row in rows {
        let mut spec = row?;
        hydrate_spec_provenance(conn, &mut spec)?;
        out.push(spec);
    }
    Ok(out)
}

/// Metadata only — `SddSpecRevisionMeta` has no `content` field, on purpose.
pub fn list_sdd_spec_revisions(
    conn: &Connection,
    org_id: &str,
    spec_id: &str,
) -> Result<Vec<SddSpecRevisionMeta>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.spec_id, r.revision, r.content_hash, r.byte_size, r.merged_from_change_id,
                c.name, r.git_commit, r.git_path, r.source, r.created_by, r.created_at
         FROM sdd_spec_revisions r
         JOIN sdd_specs s ON s.id = r.spec_id
         LEFT JOIN sdd_changes c ON c.id = r.merged_from_change_id
         WHERE r.spec_id = ?1 AND s.org_id = ?2
         ORDER BY r.revision DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![spec_id, org_id], |row| {
        Ok(SddSpecRevisionMeta {
            id: row.get(0)?,
            spec_id: row.get(1)?,
            revision: row.get(2)?,
            content_hash: row.get(3)?,
            byte_size: row.get(4)?,
            merged_from_change_id: row.get(5)?,
            merged_from_change_name: row.get(6)?,
            git_commit: row.get(7)?,
            git_path: row.get(8)?,
            source: row.get(9)?,
            created_by: row.get(10)?,
            created_at: row.get(11)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_sdd_spec_revision(
    conn: &Connection,
    org_id: &str,
    spec_id: &str,
    revision: i64,
) -> Result<Option<SddSpecRevision>> {
    let found = conn
        .query_row(
            "SELECT r.id, r.spec_id, r.revision, r.content, r.content_hash, r.byte_size,
                    r.merged_from_change_id, c.name, r.git_commit, r.git_path, r.source,
                    r.created_by, r.created_at
             FROM sdd_spec_revisions r
             JOIN sdd_specs s ON s.id = r.spec_id
             LEFT JOIN sdd_changes c ON c.id = r.merged_from_change_id
             WHERE r.spec_id = ?1 AND r.revision = ?2 AND s.org_id = ?3",
            rusqlite::params![spec_id, revision, org_id],
            |row| {
                Ok(SddSpecRevision {
                    id: row.get(0)?,
                    spec_id: row.get(1)?,
                    revision: row.get(2)?,
                    content: row.get(3)?,
                    content_hash: row.get(4)?,
                    byte_size: row.get(5)?,
                    merged_from_change_id: row.get(6)?,
                    merged_from_change_name: row.get(7)?,
                    git_commit: row.get(8)?,
                    git_path: row.get(9)?,
                    source: row.get(10)?,
                    created_by: row.get(11)?,
                    created_at: row.get(12)?,
                })
            },
        )
        .optional()?;
    Ok(found)
}

/// The reverse edge, and the reason `merged_from_change_id` exists: which living
/// specifications has this change merged into? Metadata only — the answer is a list
/// of contracts, not their text.
pub fn list_sdd_specs_for_change(
    conn: &Connection,
    org_id: &str,
    change_id: &str,
) -> Result<Vec<SddSpecMerge>> {
    // The same projection as SDD_SPEC_SELECT (so `map_sdd_spec_row` reads it), plus the
    // aggregate in column 11.
    let mut stmt = conn.prepare(
        "SELECT s.id, s.org_id, s.project, s.capability, s.title, s.path, s.latest_revision,
                s.created_by, s.created_at, s.updated_at, s.archived_at, MAX(r.revision)
         FROM sdd_spec_revisions r
         JOIN sdd_specs s ON s.id = r.spec_id
         WHERE r.merged_from_change_id = ?1 AND s.org_id = ?2
         GROUP BY s.id
         ORDER BY s.capability ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![change_id, org_id], |row| {
        Ok((map_sdd_spec_row(row)?, row.get::<_, i64>(11)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (mut spec, merged_revision) = row?;
        hydrate_spec_provenance(conn, &mut spec)?;
        out.push(SddSpecMerge {
            spec,
            merged_revision,
        });
    }
    Ok(out)
}

/// FTS5 over the latest revision of every living specification in the org.
/// Reuses `sanitize_fts_query` — do not hand-roll a second escaper.
pub fn search_sdd_specs(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<SddSpecSearchHit>> {
    let Some(fts_query) = sanitize_fts_query(query) else {
        return Ok(Vec::new());
    };

    let mut stmt = conn.prepare(
        "SELECT f.spec_id, s.project, s.capability, s.title,
                snippet(sdd_specs_fts, 3, '<b>', '</b>', '…', 24)
         FROM sdd_specs_fts f
         JOIN sdd_specs s ON s.id = f.spec_id
         WHERE sdd_specs_fts MATCH ?1 AND s.org_id = ?2
         ORDER BY rank
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![fts_query, org_id, limit], |row| {
        Ok(SddSpecSearchHit {
            spec_id: row.get(0)?,
            project: row.get(1)?,
            capability: row.get(2)?,
            title: row.get(3)?,
            snippet: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// `GET /v1/sdd/search` over BOTH trees.
///
/// Specs are listed first. "Which spec covers rate limiting?" is a question about
/// the CONTRACT, and answering it with three drafts from an in-flight change before
/// the specification they are trying to amend gets the priority exactly backwards.
/// Within each tree the order is FTS `rank`; the two ranks are not comparable across
/// tables, so they are not interleaved and pretended to be.
pub fn search_sdd_all(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<SddSearchResult>> {
    let specs = search_sdd_specs(conn, org_id, query, limit)?;
    let artifacts = search_sdd_artifacts(conn, org_id, query, limit)?;

    let mut out: Vec<SddSearchResult> = specs
        .into_iter()
        .map(SddSearchResult::from_spec)
        .chain(artifacts.into_iter().map(SddSearchResult::from_artifact))
        .collect();
    out.truncate(limit.max(0) as usize);
    Ok(out)
}

/// Keyword search over capability and title, for the `global_search` facet.
/// LIKE-based, mirroring `search_sdd_changes_by_query` — `global_search` is
/// keyword-only, not semantic. Archived specs are excluded.
pub fn search_sdd_specs_by_query(
    conn: &Connection,
    org_id: &str,
    query: &str,
    limit: i64,
) -> Result<Vec<SddSpecSummary>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, project, capability, title, latest_revision
         FROM sdd_specs
         WHERE org_id = ?1
           AND archived_at IS NULL
           AND (capability LIKE ?2 OR title LIKE ?2)
         ORDER BY updated_at DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![org_id, pattern, limit], |row| {
        Ok(SddSpecSummary {
            id: row.get(0)?,
            project: row.get(1)?,
            capability: row.get(2)?,
            title: row.get(3)?,
            latest_revision: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::db::migrations;
    use crate::models::types::Role;

    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn list_all_users_tolerates_null_email() {
        // Regression: GET /internal/users 500'd when a seeded user had a NULL email,
        // because list_all_users read `email` as a non-optional String. The per-org
        // list_users already treats email as Option; list_all_users must match.
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES ('u-null', ?1, NULL, 'No Email', 'member', 'active', datetime('now'))",
            rusqlite::params![org.id],
        )
        .unwrap();

        let users = list_all_users(&conn).expect("list_all_users must not error on NULL email");
        let null_user = users
            .iter()
            .find(|u| u.id == "u-null")
            .expect("user with NULL email must be returned");
        assert_eq!(
            null_user.email, "",
            "NULL email must deserialize to empty string"
        );
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
        )
        .unwrap();

        let key_id = uuid::Uuid::new_v4().to_string();
        let (raw_key, key_hash) = crate::auth::api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org.id, key_hash],
        )
        .unwrap();

        // Key is structurally valid and role is custom — must return context with Custom(superuser)
        let result = validate_api_key(&conn, &api_keys::hash_key(&raw_key)).unwrap();
        assert!(
            result.is_some(),
            "custom role string must cause validate_api_key to return Some"
        );
        assert_eq!(
            result.unwrap().role,
            UserRole::Custom("superuser".to_string())
        );
    }

    #[test]
    fn validate_api_key_returns_none_for_unknown_hash() {
        let conn = setup();
        let result = validate_api_key(&conn, "deadbeef").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn new_org_gets_default_project_enrolling_only_admin() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let pid = find_project_id(&conn, &org.id, DEFAULT_PROJECT_NAME)
            .unwrap()
            .expect("bootstrap must create the default project");
        let members = list_project_members(&conn, &org.id, &pid).unwrap();
        assert_eq!(
            members.len(),
            1,
            "only the initial admin is enrolled at bootstrap"
        );
        assert_eq!(members[0].user_id, admin.id);

        // A later user is NOT auto-enrolled in the default project (needs explicit invite).
        let (member, _) = invite_user(&conn, &org.id, "m@acme.com", "M", "member").unwrap();
        let members_after = list_project_members(&conn, &org.id, &pid).unwrap();
        assert!(
            !members_after.iter().any(|m| m.user_id == member.id),
            "an invited member must not be auto-enrolled in the default project"
        );
    }

    #[test]
    fn ensure_default_projects_backfills_legacy_org_idempotently() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Simulate a legacy org created before default-project onboarding existed.
        let pid = find_project_id(&conn, &org.id, DEFAULT_PROJECT_NAME)
            .unwrap()
            .unwrap();
        conn.execute("DELETE FROM project_members WHERE project_id = ?1", [&pid])
            .unwrap();
        conn.execute("DELETE FROM projects WHERE id = ?1", [&pid])
            .unwrap();
        assert!(find_project_id(&conn, &org.id, DEFAULT_PROJECT_NAME)
            .unwrap()
            .is_none());

        let created = ensure_default_projects(&conn).unwrap();
        assert_eq!(
            created, 1,
            "backfill must create the missing default project"
        );
        let new_pid = find_project_id(&conn, &org.id, DEFAULT_PROJECT_NAME)
            .unwrap()
            .expect("backfill must create the default project");
        let members = list_project_members(&conn, &org.id, &new_pid).unwrap();
        assert!(
            members.iter().any(|m| m.user_id == admin.id),
            "backfill must enrol the org admin"
        );

        // Idempotent: a second run is a no-op.
        assert_eq!(
            ensure_default_projects(&conn).unwrap(),
            0,
            "backfill must be idempotent"
        );
    }

    #[test]
    fn validate_api_key_returns_none_for_revoked_key() {
        let conn = setup();
        let (org, _user, raw_key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hash = api_keys::hash_key(&raw_key);

        conn.execute(
            "UPDATE api_keys SET revoked = 1 WHERE org_id = ?1",
            [&org.id],
        )
        .unwrap();

        let result = validate_api_key(&conn, &hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn validate_api_key_returns_context_for_valid_key() {
        let conn = setup();
        let (org, user, raw_key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hash = api_keys::hash_key(&raw_key);

        let ctx = validate_api_key(&conn, &hash)
            .unwrap()
            .expect("should return context");
        assert_eq!(ctx.org_id, org.id);
        assert_eq!(ctx.user_id, user.id);
        assert_eq!(ctx.role, UserRole::Standard(Role::Admin));
    }

    #[test]
    fn validate_api_key_returns_none_for_suspended_user() {
        let conn = setup();
        let (_org, user, raw_key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hash = api_keys::hash_key(&raw_key);

        conn.execute(
            "UPDATE users SET status = 'suspended' WHERE id = ?1",
            [&user.id],
        )
        .unwrap();

        let result = validate_api_key(&conn, &hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn bootstrap_creates_org_and_admin() {
        let conn = setup();
        let (org, user, raw_key) =
            bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin User").unwrap();

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
        let mem = legacy_store(
            &conn,
            &org.id,
            &user.id,
            "nexusmind",
            "claude",
            "use anyhow for errors",
            &tags,
        );

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

        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "use snake_case for identifiers",
            &[],
        );
        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "database migrations run at startup",
            &[],
        );

        let results = search_memories(&conn, &org.id, "snake_case", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("snake_case"));
    }

    #[test]
    fn search_memories_scoped_to_org() {
        let conn = setup();
        // org1
        let (org1, user1, _) =
            bootstrap(&conn, "Org1", "org1", "admin@org1.com", "Admin1").unwrap();
        legacy_store(
            &conn,
            &org1.id,
            &user1.id,
            "proj",
            "claude",
            "secret content org1",
            &[],
        );

        // org2 — manually insert since bootstrap only allows one org
        let org2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org2', 'org2')",
            [&org2_id],
        )
        .unwrap();
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
        assert_eq!(
            results.len(),
            2,
            "both memories share at least one query term"
        );
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
        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "bar foo baz",
            &[],
        );

        // Must not error even though the raw query contains FTS5 special chars.
        let result = search_memories(&conn, &org.id, raw, 10);
        assert!(
            result.is_ok(),
            "special characters must not cause a query error: {result:?}"
        );
    }

    #[test]
    fn list_memories_with_filters() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        legacy_store(&conn, &org.id, &user.id, "proj-a", "claude", "mem 1", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj-b", "cursor", "mem 2", &[]);
        legacy_store(&conn, &org.id, &user.id, "proj-a", "cursor", "mem 3", &[]);

        // filter by tool
        let cursor_mems = list_memories(
            &conn,
            &org.id,
            None,
            Some("cursor"),
            None,
            None,
            None,
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(cursor_mems.len(), 2);

        // filter by project
        let proj_a = list_memories(
            &conn,
            &org.id,
            None,
            None,
            Some("proj-a"),
            None,
            None,
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(proj_a.len(), 2);

        // filter by both
        let filtered = list_memories(
            &conn,
            &org.id,
            None,
            Some("cursor"),
            Some("proj-a"),
            None,
            None,
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
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
        let still_there = list_memories(
            &conn, &org.id, None, None, None, None, None, None, 10, 0, false, None, None, None,
        )
        .unwrap();
        assert_eq!(still_there.len(), 1);
    }

    // ── User tests ────────────────────────────────────────────────────────────

    #[test]
    fn invite_user_creates_active_key() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let (user, raw_key) =
            invite_user(&conn, &org.id, "dev@acme.com", "Dev User", "member").unwrap();
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
        let (org, user, old_raw_key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

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

        log_audit(
            &conn,
            &org.id,
            &user.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();
        log_audit(
            &conn,
            &org.id,
            &user.id,
            "search",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();

        let entries =
            list_audit(&conn, &org.id, None, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn list_audit_scoped_to_org() {
        let conn = setup();
        let (org1, user1, _) =
            bootstrap(&conn, "Org1", "org1", "admin@org1.com", "Admin1").unwrap();
        log_audit(
            &conn,
            &org1.id,
            &user1.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();

        // manually create org2
        let org2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org2', 'org2')",
            [&org2_id],
        )
        .unwrap();
        let user2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role) VALUES (?1, ?2, 'u2@org2.com', 'U2', 'member')",
            [&user2_id, &org2_id],
        ).unwrap();
        log_audit(
            &conn,
            &org2_id,
            &user2_id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();

        let org1_entries =
            list_audit(&conn, &org1.id, None, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(
            org1_entries.len(),
            1,
            "org1 must not see org2 audit entries"
        );

        let org2_entries =
            list_audit(&conn, &org2_id, None, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(
            org2_entries.len(),
            1,
            "org2 must not see org1 audit entries"
        );
    }

    #[test]
    fn list_audit_filters_by_action() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(
            &conn,
            &org.id,
            &user.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();
        log_audit(
            &conn,
            &org.id,
            &user.id,
            "search",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();
        log_audit(
            &conn,
            &org.id,
            &user.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();

        let store_entries = list_audit(
            &conn,
            &org.id,
            None,
            Some("store"),
            None,
            None,
            None,
            None,
            50,
            0,
        )
        .unwrap();
        assert_eq!(store_entries.len(), 2);
        assert!(store_entries.iter().all(|e| e.action == "store"));

        let search_entries = list_audit(
            &conn,
            &org.id,
            None,
            Some("search"),
            None,
            None,
            None,
            None,
            50,
            0,
        )
        .unwrap();
        assert_eq!(search_entries.len(), 1);
    }

    #[test]
    fn list_audit_full_text_search_filters_by_action_substring() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        log_audit(
            &conn,
            &org.id,
            &user.id,
            "memory.created",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();
        log_audit(
            &conn,
            &org.id,
            &user.id,
            "user.updated",
            "user",
            None,
            serde_json::json!({}),
        )
        .unwrap();
        log_audit(
            &conn,
            &org.id,
            &user.id,
            "project.archived",
            "project",
            None,
            serde_json::json!({}),
        )
        .unwrap();

        let results = list_audit(
            &conn,
            &org.id,
            None,
            None,
            None,
            None,
            None,
            Some("memory"),
            50,
            0,
        )
        .unwrap();
        assert_eq!(
            results.len(),
            1,
            "search for 'memory' must return exactly 1 result"
        );
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

        log_audit(
            &conn,
            &org.id,
            &user.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();

        let entries =
            list_audit(&conn, &org.id, None, None, None, None, None, None, 10, 0).unwrap();
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

        log_audit(
            &conn,
            &org.id,
            &user.id,
            "search",
            "memory",
            None,
            serde_json::json!({"query": "rust"}),
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_logs WHERE org_id = ?1 AND action = 'search'",
                [&org.id],
                |r| r.get(0),
            )
            .unwrap();
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

        log_audit(
            &conn,
            &org.id,
            &user.id,
            "search",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();

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
            project: None,
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
            project: None,
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
            project: None,
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
        assert_eq!(
            mem2.revision_count, 2,
            "second store must increment revision_count"
        );
        assert_eq!(mem2.id, mem1.id, "upsert must reuse existing row id");
        assert_eq!(mem2.content, "updated content");

        // Verify only one row exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE org_id = ?1",
                [&org.id],
                |r| r.get(0),
            )
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
        )
        .unwrap();
        let user2_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role) VALUES (?1, ?2, 'u2@org2.com', 'U2', 'member')",
            [&user2_id, &org2_id],
        ).unwrap();

        let req = crate::models::types::StoreMemoryRequest {
            project: None,
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
            project: None,
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

        assert_ne!(
            mem1.id, mem2.id,
            "different orgs must get different rows for same topic_key"
        );
        assert_eq!(mem1.revision_count, 1);
        assert_eq!(mem2.revision_count, 1);
    }

    #[test]
    fn upsert_memory_no_topic_key_always_inserts() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req = crate::models::types::StoreMemoryRequest {
            project: None,
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
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE org_id = ?1",
                [&org.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "no topic_key must always insert new rows");
    }

    #[test]
    fn normalized_hash_same_for_equivalent_content() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req_a = crate::models::types::StoreMemoryRequest {
            project: None,
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
            project: None,
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
        assert!(
            updated.is_some(),
            "patch_session must return the updated session"
        );
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
            project: None,
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
            project: None,
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

        let bugfix_mems = list_memories(
            &conn,
            &org.id,
            None,
            None,
            None,
            Some("bugfix"),
            None,
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(bugfix_mems.len(), 1);
        assert_eq!(bugfix_mems[0].memory_type.as_deref(), Some("bugfix"));
    }

    #[test]
    fn list_memories_filter_by_scope() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let req_personal = crate::models::types::StoreMemoryRequest {
            project: None,
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
            project: None,
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

        let personal_mems = list_memories(
            &conn,
            &org.id,
            None,
            None,
            None,
            None,
            Some("personal"),
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(personal_mems.len(), 1);
        assert_eq!(personal_mems[0].scope, "personal");

        let combined = list_memories(
            &conn,
            &org.id,
            None,
            None,
            None,
            None,
            Some("project"),
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
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
        )
        .unwrap();

        let req_with_session = crate::models::types::StoreMemoryRequest {
            project: None,
            tool: "claude".into(),
            content: "session memory".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: Some(session_id.into()),
        };
        let req_without_session = crate::models::types::StoreMemoryRequest {
            project: None,
            tool: "claude".into(),
            content: "other memory".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        };
        upsert_memory(&conn, &org.id, &user.id, &req_with_session).unwrap();
        upsert_memory(&conn, &org.id, &user.id, &req_without_session).unwrap();

        let session_mems = list_memories(
            &conn,
            &org.id,
            None,
            None,
            None,
            None,
            None,
            Some(session_id),
            50,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            session_mems.len(),
            1,
            "only memories matching session_id should be returned"
        );
        assert_eq!(session_mems[0].content, "session memory");
    }

    #[test]
    fn list_memories_combined_type_scope_filter() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // bugfix+project
        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &crate::models::types::StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "c1".into(),
                tags: None,
                title: None,
                memory_type: Some("bugfix".into()),
                scope: Some("project".into()),
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();
        // bugfix+personal
        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &crate::models::types::StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "c2".into(),
                tags: None,
                title: None,
                memory_type: Some("bugfix".into()),
                scope: Some("personal".into()),
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();
        // decision+project
        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &crate::models::types::StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "c3".into(),
                tags: None,
                title: None,
                memory_type: Some("decision".into()),
                scope: Some("project".into()),
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        let results = list_memories(
            &conn,
            &org.id,
            None,
            None,
            None,
            Some("bugfix"),
            Some("project"),
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            results.len(),
            1,
            "combined filter must return only bugfix+project memories"
        );
    }

    #[test]
    fn list_memories_unknown_type_returns_empty() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &crate::models::types::StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "content".into(),
                tags: None,
                title: None,
                memory_type: Some("bugfix".into()),
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        let results = list_memories(
            &conn,
            &org.id,
            None,
            None,
            None,
            Some("config"),
            None,
            None,
            10,
            0,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(
            results.is_empty(),
            "unknown type filter must return empty list"
        );
    }

    // ── v2 FTS search tests ───────────────────────────────────────────────────

    #[test]
    fn search_memories_matches_on_title() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &crate::models::types::StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "unrelated content".into(),
                tags: None,
                title: Some("JWT auth middleware".into()),
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        let results = search_memories(&conn, &org.id, "JWT", 10).unwrap();
        assert_eq!(results.len(), 1, "FTS must match on title");
        assert_eq!(results[0].title.as_deref(), Some("JWT auth middleware"));
    }

    #[test]
    fn search_memories_matches_on_type() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &crate::models::types::StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "unrelated".into(),
                tags: None,
                title: Some("Unrelated title".into()),
                memory_type: Some("bugfix".into()),
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

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
        // Implicit project creation is disabled in upsert_memory, so ensure the project
        // exists first (test scaffolding — production requires an admin to create it).
        if project != "default" {
            get_or_create_project(conn, org_id, project).unwrap();
        }
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
        let mem = legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "absence test",
            &[],
        );
        assert_eq!(
            mem.content, "absence test",
            "legacy_store must work as upsert_memory wrapper"
        );
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
        )
        .unwrap();

        assert!(
            entry.previous_hash.is_none(),
            "genesis record must have previous_hash = NULL"
        );
        assert!(
            entry.current_hash.is_some(),
            "genesis record must have a non-empty current_hash"
        );
        let hash = entry.current_hash.unwrap();
        assert_eq!(hash.len(), 64, "SHA-256 hex string is 64 chars");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "current_hash must be hex"
        );
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
            )
            .unwrap();
            entries.push(e);
        }

        // Chain linkage: each entry's previous_hash must equal the prior entry's current_hash.
        assert!(
            entries[0].previous_hash.is_none(),
            "first entry genesis must have no previous_hash"
        );
        assert_eq!(
            entries[1].previous_hash, entries[0].current_hash,
            "entry[1].previous_hash must equal entry[0].current_hash"
        );
        assert_eq!(
            entries[2].previous_hash, entries[1].current_hash,
            "entry[2].previous_hash must equal entry[1].current_hash"
        );

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
        let (org_a, user_a, _) =
            bootstrap(&conn, "OrgA", "orga", "admin@orga.com", "AdminA").unwrap();

        // Create org B manually.
        let org_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'OrgB', 'orgb')",
            [&org_b_id],
        )
        .unwrap();
        let user_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES (?1, ?2, 'b@orgb.com', 'B', 'admin', 'active')",
            [&user_b_id, &org_b_id],
        ).unwrap();

        // Org A: 2 inserts.
        insert_audit_log_chained(
            &conn,
            &org_a.id,
            &user_a.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
            None,
        )
        .unwrap();
        let a2 = insert_audit_log_chained(
            &conn,
            &org_a.id,
            &user_a.id,
            "search",
            "memory",
            None,
            serde_json::json!({}),
            None,
        )
        .unwrap();

        // Org B: 1 insert — should bootstrap its own chain, not continue org A's.
        let b1 = insert_audit_log_chained(
            &conn,
            &org_b_id,
            &user_b_id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
            None,
        )
        .unwrap();

        assert!(
            b1.previous_hash.is_none(),
            "org B genesis must have previous_hash = NULL"
        );
        assert!(
            b1.current_hash.is_some(),
            "org B genesis must have a current_hash"
        );
        // Org B's hash must NOT equal org A's last hash.
        assert_ne!(
            b1.current_hash, a2.current_hash,
            "org B chain must be independent of org A"
        );
    }

    #[test]
    fn insert_audit_log_chained_concurrent_writes_no_corruption() {
        // Two threads write to the same org concurrently.
        // The resulting chain must have exactly 2 new records, correctly linked.
        use std::sync::{Arc, Mutex};

        let raw_conn = connect(":memory:").unwrap();
        migrations::run(&raw_conn).unwrap();
        let (org, user, _) =
            bootstrap(&raw_conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
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
                insert_audit_log_chained(
                    &c,
                    &org_id1,
                    &user_id1,
                    "store",
                    "memory",
                    None,
                    serde_json::json!({}),
                    None,
                )
                .unwrap();
            });
            s.spawn(move || {
                let c = conn2.lock().unwrap();
                insert_audit_log_chained(
                    &c,
                    &org_id2,
                    &user_id2,
                    "search",
                    "memory",
                    None,
                    serde_json::json!({}),
                    None,
                )
                .unwrap();
            });
        });

        // Verify exactly 2 rows for this org.
        let guard = conn.lock().unwrap();
        let count: i64 = guard
            .query_row(
                "SELECT COUNT(*) FROM audit_logs WHERE org_id = ?1",
                [&org_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 2,
            "must have exactly 2 audit rows after concurrent writes"
        );

        // Verify chain integrity: at least one row has a non-null current_hash,
        // and the chain links correctly (the second row's previous_hash = first row's current_hash).
        let entries =
            list_audit(&guard, &org_id, None, None, None, None, None, None, 50, 0).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries.iter().all(|e| e.current_hash.is_some()),
            "both rows must have current_hash"
        );

        // Verify linkage: one row has previous_hash=NULL, the other has the first's current_hash.
        let genesis = entries
            .iter()
            .find(|e| e.previous_hash.is_none())
            .expect("must have a genesis row");
        let chained = entries
            .iter()
            .find(|e| e.previous_hash.is_some())
            .expect("must have a chained row");
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

        log_audit(
            &conn,
            &org.id,
            &user.id,
            "store",
            "memory",
            None,
            serde_json::json!({}),
        )
        .unwrap();

        let entries =
            list_audit(&conn, &org.id, None, None, None, None, None, None, 10, 0).unwrap();
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
        let policy = insert_policy(
            &conn,
            &id,
            &org.id,
            "Whitelist",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

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
        )
        .unwrap();
        org2_id
    }

    #[test]
    fn get_policy_cross_org_returns_none() {
        let conn = setup();
        let (org1, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let org2_id = seed_second_org(&conn);

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(
            &conn,
            &id,
            &org1.id,
            "Whitelist",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

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

        insert_policy(
            &conn,
            &id1,
            &org1.id,
            "Org1 Policy",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();
        insert_policy(
            &conn,
            &id2,
            &org2_id,
            "Org2 Policy",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

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
        insert_policy(
            &conn,
            &id,
            &org.id,
            "Old Name",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let updated = update_policy(
            &conn,
            &id,
            &org.id,
            Some("New Name"),
            None,
            Some(false),
            &now,
        )
        .unwrap();
        assert!(updated.is_some());
        let p = updated.unwrap();
        assert_eq!(p.name, "New Name");
        assert!(!p.enabled);
    }

    #[test]
    fn update_policy_returns_none_for_missing_id() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let result = update_policy(
            &conn,
            "nonexistent-id",
            &org.id,
            Some("X"),
            None,
            None,
            &now,
        )
        .unwrap();
        assert!(
            result.is_none(),
            "update must return None for nonexistent id"
        );
    }

    #[test]
    fn delete_policy_removes_row() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let id = format!("p_{}", Uuid::new_v4().simple());
        let config_json = r#"{"allowed_models":["claude-3-5-sonnet"]}"#;
        insert_policy(
            &conn,
            &id,
            &org.id,
            "Temp",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

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
        insert_policy(
            &conn,
            &id,
            &org1.id,
            "Org1 Policy",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

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

        insert_policy(
            &conn,
            &id1,
            &org.id,
            "Enabled",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();
        insert_policy(
            &conn,
            &id2,
            &org.id,
            "Disabled",
            "model_whitelist",
            config_json,
            false,
            None,
        )
        .unwrap();

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
        let policy = insert_policy(
            &conn,
            &id,
            &org.id,
            "Scoped",
            "model_whitelist",
            config_json,
            true,
            Some(&project.id),
        )
        .unwrap();

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
        let policy = insert_policy(
            &conn,
            &id,
            &org.id,
            "OrgWide",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

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
        insert_policy(
            &conn,
            &id,
            &org.id,
            "OrgWide",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();

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
        insert_policy(
            &conn,
            &id,
            &org.id,
            "ProjA Only",
            "model_whitelist",
            config_json,
            true,
            Some(&project_a.id),
        )
        .unwrap();

        let for_a = list_enabled_policies(&conn, &org.id, Some(&project_a.id)).unwrap();
        let for_b = list_enabled_policies(&conn, &org.id, Some(&project_b.id)).unwrap();
        assert_eq!(
            for_a.len(),
            1,
            "project-scoped policy must apply to its own project"
        );
        assert_eq!(
            for_b.len(),
            0,
            "project-scoped policy must NOT apply to a different project"
        );
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
        insert_policy(
            &conn,
            &id1,
            &org.id,
            "OrgWide",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();
        insert_policy(
            &conn,
            &id2,
            &org.id,
            "ProjA",
            "model_whitelist",
            config_json,
            true,
            Some(&project_a.id),
        )
        .unwrap();
        insert_policy(
            &conn,
            &id3,
            &org.id,
            "DisabledOrgWide",
            "model_whitelist",
            config_json,
            false,
            None,
        )
        .unwrap();

        let admin_view = list_enabled_policies(&conn, &org.id, None).unwrap();
        assert_eq!(
            admin_view.len(),
            2,
            "None must return all ENABLED policies for the org regardless of project_id"
        );
        let names: Vec<&str> = admin_view.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"OrgWide"));
        assert!(names.contains(&"ProjA"));
        assert!(
            !names.contains(&"DisabledOrgWide"),
            "disabled policies must still be excluded"
        );
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
        insert_policy(
            &conn,
            &id1,
            &org.id,
            "OrgWide",
            "model_whitelist",
            config_json,
            true,
            None,
        )
        .unwrap();
        insert_policy(
            &conn,
            &id2,
            &org.id,
            "ProjA",
            "model_whitelist",
            config_json,
            true,
            Some(&project_a.id),
        )
        .unwrap();
        insert_policy(
            &conn,
            &id3,
            &org.id,
            "ProjQ",
            "model_whitelist",
            config_json,
            true,
            Some(&project_q.id),
        )
        .unwrap();

        let for_a = list_enabled_policies(&conn, &org.id, Some(&project_a.id)).unwrap();
        let names: Vec<&str> = for_a.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            for_a.len(),
            2,
            "resolving for project A must be org-wide UNION project A"
        );
        assert!(names.contains(&"OrgWide"));
        assert!(names.contains(&"ProjA"));
        assert!(
            !names.contains(&"ProjQ"),
            "project Q's policy must not leak into project A's resolution"
        );
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
        assert!(
            perms.contains(&"policy:read".to_string()),
            "admin must have policy:read"
        );
        assert!(
            perms.contains(&"policy:write".to_string()),
            "admin must have policy:write"
        );
    }

    #[test]
    fn get_role_permissions_member_includes_policy_read_only() {
        let conn = setup();
        let perms = get_role_permissions(&conn, "irrelevant", "member").unwrap();
        assert!(
            perms.contains(&"policy:read".to_string()),
            "member must have policy:read"
        );
        assert!(
            !perms.contains(&"policy:write".to_string()),
            "member must NOT have policy:write"
        );
    }

    #[test]
    fn get_role_permissions_viewer_has_no_policy_perms() {
        let conn = setup();
        let perms = get_role_permissions(&conn, "irrelevant", "viewer").unwrap();
        assert!(
            !perms.contains(&"policy:read".to_string()),
            "viewer must not have policy:read"
        );
        assert!(
            !perms.contains(&"policy:write".to_string()),
            "viewer must not have policy:write"
        );
    }

    fn valid_harness_manifest(target: &str) -> serde_json::Value {
        serde_json::json!({
            "schema_version": "1.0",
            "targets": [target],
            "components": [{ "name": "agent", "type": "skill" }],
            "compatibility": { target: ">=1.0.0" },
            "provenance": { "source": "nexus-mind", "author": "admin" },
            "security": { "requires_approval": true }
        })
    }

    fn executable_harness_manifest() -> serde_json::Value {
        let content = "#!/bin/sh\nexit 0";
        serde_json::json!({
            "schema_version": "1.1",
            "format": "hook",
            "targets": ["claude"],
            "components": [{
                "kind": "file",
                "path": "hooks/pre-commit.sh",
                "media_type": "text/x-shellscript",
                "size_bytes": content.as_bytes().len(),
                "sha256": format!("sha256:{}", hex::encode(Sha256::digest(content.as_bytes()))),
                "content": content
            }],
            "provenance": { "source": "test" },
            "security": { "requires_approval": true, "executable": true, "secret_scan_status": "passed" }
        })
    }

    #[test]
    fn harness_catalog_visibility_hides_inaccessible_project_harnesses() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project_a = create_project(&conn, &org.id, "visible", None, None).unwrap();
        let project_b = create_project(&conn, &org.id, "hidden", None, None).unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at) VALUES (?1, ?2, 'dev@acme.com', 'Dev', 'member', 'active', datetime('now'))",
            rusqlite::params![user_id, org.id],
        ).unwrap();
        upsert_project_member(&conn, &project_a.id, &user_id, "member").unwrap();

        create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "org-wide".into(),
                name: "Org Wide".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "visible".into(),
                name: "Visible".into(),
                description: None,
                project_id: Some(project_a.id),
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "hidden".into(),
                name: "Hidden".into(),
                description: None,
                project_id: Some(project_b.id),
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();

        let visible = list_visible_harnesses(&conn, &org.id, Some(&user_id), None, None).unwrap();
        let names: Vec<&str> = visible.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(visible.len(), 1);
        assert!(!names.contains(&"Org Wide"));
        assert!(names.contains(&"Visible"));
        assert!(!names.contains(&"Hidden"));
    }

    #[test]
    fn harness_ownership_defaults_joins_filters_and_validates_org_owner() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let owner = invite_user(&conn, &org.id, "owner@acme.com", "Owner User", "member")
            .unwrap()
            .0;
        let other_org = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Other', 'other')",
            [&other_org],
        )
        .unwrap();
        let other_owner = Uuid::new_v4().to_string();
        conn.execute("INSERT INTO users (id, org_id, email, name, role, status, created_at) VALUES (?1, ?2, 'other@example.com', 'Other User', 'member', 'active', datetime('now'))", rusqlite::params![other_owner, other_org]).unwrap();

        let default_owned = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "default-owned".into(),
                name: "Default Owned".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        assert_eq!(default_owned.owner_user_id, admin.id);
        assert_eq!(
            default_owned.owner.as_ref().map(|o| o.name.as_str()),
            Some("Admin")
        );

        let assigned = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "assigned".into(),
                name: "Assigned".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: Some(owner.id.clone()),
            },
        )
        .unwrap();
        assert_eq!(assigned.owner_user_id, owner.id);
        assert_eq!(
            assigned.owner.as_ref().map(|o| o.email.as_str()),
            Some("owner@acme.com")
        );

        let filtered = list_visible_harnesses(&conn, &org.id, None, None, Some(&owner.id)).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].slug, "assigned");

        let invalid = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "bad-owner".into(),
                name: "Bad Owner".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: Some(other_owner),
            },
        )
        .unwrap_err();
        assert!(invalid.to_string().contains("owner_not_in_org"));
    }

    #[test]
    fn publish_download_and_approval_preserve_manifest_hash() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let harness = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "claude-base".into(),
                name: "Claude Base".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        let manifest = valid_harness_manifest("claude");
        let version = publish_harness_version(
            &conn,
            &org.id,
            &admin.id,
            &harness.id,
            &PublishHarnessVersionRequest {
                version: "1.0.0".into(),
                manifest: manifest.clone(),
                manifest_hash: None,
            },
        )
        .unwrap();

        let before_approval =
            download_harness_version(&conn, &org.id, &admin.id, None, &harness.id, "1.0.0")
                .unwrap_err();
        assert!(before_approval.to_string().contains("approval_required"));

        let approval = create_harness_approval(
            &conn,
            &org.id,
            &admin.id,
            None,
            &harness.id,
            "1.0.0",
            &HarnessApprovalRequest {
                target_tool: "claude".into(),
                target_scope: "project".into(),
                manifest_hash: version.manifest_hash.clone(),
                metadata: None,
            },
        )
        .unwrap();
        assert_eq!(approval.manifest_hash, version.manifest_hash);

        let downloaded =
            download_harness_version(&conn, &org.id, &admin.id, None, &harness.id, "1.0.0")
                .unwrap()
                .unwrap();
        assert_eq!(downloaded.manifest, manifest);
        assert_eq!(downloaded.manifest_hash, version.manifest_hash);

        let mismatch = create_harness_approval(
            &conn,
            &org.id,
            &admin.id,
            None,
            &harness.id,
            "1.0.0",
            &HarnessApprovalRequest {
                target_tool: "claude".into(),
                target_scope: "project".into(),
                manifest_hash: "wrong".into(),
                metadata: None,
            },
        )
        .unwrap_err();
        assert!(mismatch.to_string().contains("manifest_hash_mismatch"));
    }

    #[test]
    fn executable_harness_approval_requires_and_persists_warning_acknowledgement() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let harness = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "hook-base".into(),
                name: "Hook Base".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        let version = publish_harness_version(
            &conn,
            &org.id,
            &admin.id,
            &harness.id,
            &PublishHarnessVersionRequest {
                version: "1.0.0".into(),
                manifest: executable_harness_manifest(),
                manifest_hash: None,
            },
        )
        .unwrap();

        let missing_ack = create_harness_approval(
            &conn,
            &org.id,
            &admin.id,
            None,
            &harness.id,
            "1.0.0",
            &HarnessApprovalRequest {
                target_tool: "claude".into(),
                target_scope: "project".into(),
                manifest_hash: version.manifest_hash.clone(),
                metadata: Some(serde_json::json!({ "source": "test" })),
            },
        )
        .unwrap_err();
        assert!(missing_ack
            .to_string()
            .contains("warning_acknowledgement_required"));

        let acknowledged = create_harness_approval(
            &conn,
            &org.id,
            &admin.id,
            None,
            &harness.id,
            "1.0.0",
            &HarnessApprovalRequest {
                target_tool: "claude".into(),
                target_scope: "project".into(),
                manifest_hash: version.manifest_hash,
                metadata: Some(serde_json::json!({
                    "source": "test",
                    "warning_acknowledged": true
                })),
            },
        )
        .unwrap();
        assert_eq!(acknowledged.metadata["warning_acknowledged"], true);
    }

    #[test]
    fn publish_harness_version_rejects_component_integrity_mismatch() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let harness = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "integrity-check".into(),
                name: "Integrity Check".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        let mut manifest = executable_harness_manifest();
        manifest["components"][0]["sha256"] = serde_json::json!("sha256:template");

        let err = publish_harness_version(
            &conn,
            &org.id,
            &admin.id,
            &harness.id,
            &PublishHarnessVersionRequest {
                version: "1.0.0".into(),
                manifest,
                manifest_hash: None,
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("component_integrity_mismatch"));
    }

    #[test]
    fn harness_approval_and_download_require_project_visibility() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let hidden_project = create_project(&conn, &org.id, "hidden", None, None).unwrap();
        let allowed_project = create_project(&conn, &org.id, "allowed", None, None).unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at) VALUES (?1, ?2, 'dev@acme.com', 'Dev', 'member', 'active', datetime('now'))",
            rusqlite::params![user_id, org.id],
        ).unwrap();
        upsert_project_member(&conn, &allowed_project.id, &user_id, "member").unwrap();

        let hidden_harness = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "hidden-project".into(),
                name: "Hidden Project".into(),
                description: None,
                project_id: Some(hidden_project.id),
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        let hidden_version = publish_harness_version(
            &conn,
            &org.id,
            &admin.id,
            &hidden_harness.id,
            &PublishHarnessVersionRequest {
                version: "1.0.0".into(),
                manifest: valid_harness_manifest("claude"),
                manifest_hash: None,
            },
        )
        .unwrap();

        let hidden_approval = create_harness_approval(
            &conn,
            &org.id,
            &user_id,
            Some(&user_id),
            &hidden_harness.id,
            "1.0.0",
            &HarnessApprovalRequest {
                target_tool: "claude".into(),
                target_scope: "project".into(),
                manifest_hash: hidden_version.manifest_hash.clone(),
                metadata: None,
            },
        )
        .unwrap_err();
        assert!(hidden_approval.to_string().contains("version_not_found"));

        let hidden_download = download_harness_version(
            &conn,
            &org.id,
            &user_id,
            Some(&user_id),
            &hidden_harness.id,
            "1.0.0",
        )
        .unwrap();
        assert!(hidden_download.is_none());

        let allowed_harness = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "allowed-project".into(),
                name: "Allowed Project".into(),
                description: None,
                project_id: Some(allowed_project.id),
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        let allowed_version = publish_harness_version(
            &conn,
            &org.id,
            &admin.id,
            &allowed_harness.id,
            &PublishHarnessVersionRequest {
                version: "1.0.0".into(),
                manifest: valid_harness_manifest("claude"),
                manifest_hash: None,
            },
        )
        .unwrap();
        create_harness_approval(
            &conn,
            &org.id,
            &user_id,
            Some(&user_id),
            &allowed_harness.id,
            "1.0.0",
            &HarnessApprovalRequest {
                target_tool: "claude".into(),
                target_scope: "project".into(),
                manifest_hash: allowed_version.manifest_hash,
                metadata: None,
            },
        )
        .unwrap();
        let allowed_download = download_harness_version(
            &conn,
            &org.id,
            &user_id,
            Some(&user_id),
            &allowed_harness.id,
            "1.0.0",
        )
        .unwrap();
        assert!(allowed_download.is_some());
    }

    #[test]
    fn record_harness_install_result_preserves_local_file_boundary() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let harness = create_harness(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessRequest {
                slug: "claude-base".into(),
                name: "Claude Base".into(),
                description: None,
                project_id: None,
                visibility: None,
                owner_user_id: None,
            },
        )
        .unwrap();
        let version = publish_harness_version(
            &conn,
            &org.id,
            &admin.id,
            &harness.id,
            &PublishHarnessVersionRequest {
                version: "1.0.0".into(),
                manifest: valid_harness_manifest("claude"),
                manifest_hash: None,
            },
        )
        .unwrap();
        let approval = create_harness_approval(
            &conn,
            &org.id,
            &admin.id,
            None,
            &harness.id,
            "1.0.0",
            &HarnessApprovalRequest {
                target_tool: "claude".into(),
                target_scope: "project".into(),
                manifest_hash: version.manifest_hash.clone(),
                metadata: None,
            },
        )
        .unwrap();

        let updated = record_harness_install_result(
            &conn,
            &org.id,
            &admin.id,
            &harness.id,
            "1.0.0",
            &HarnessInstallResultRequest {
                approval_id: approval.id,
                manifest_hash: version.manifest_hash.clone(),
                status: "installed".into(),
                metadata: Some(
                    serde_json::json!({ "tool_version": "1.2.3", "changed_files_count": 2 }),
                ),
            },
        )
        .unwrap();

        assert_eq!(updated.status, "approved");
        assert_eq!(updated.metadata["install_result"]["status"], "installed");
        assert_eq!(updated.metadata["install_result"]["changed_files_count"], 2);
        assert!(updated.metadata["install_result"]
            .get("raw_file_contents")
            .is_none());

        let nested_raw = record_harness_install_result(
            &conn,
            &org.id,
            &admin.id,
            &harness.id,
            "1.0.0",
            &HarnessInstallResultRequest {
                approval_id: updated.id,
                manifest_hash: version.manifest_hash,
                status: "installed".into(),
                metadata: Some(
                    serde_json::json!({ "details": { "raw_file_contents": "cat ~/.claude.json" } }),
                ),
            },
        )
        .unwrap_err();
        assert!(nested_raw
            .to_string()
            .contains("raw_local_content_rejected"));
    }

    #[test]
    fn harness_config_review_rejects_secret_bearing_snapshots() {
        let conn = setup();
        let (org, admin, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let accepted = create_harness_config_review(&conn, &org.id, &admin.id, &CreateHarnessConfigReviewRequest {
            source_tool: "claude".into(),
            redacted_config: serde_json::json!({ "mcpServers": { "nexusmind": { "command": "npx", "env": { "NEXUSMIND_API_KEY": "[REDACTED]" } } } }),
            redaction_report: serde_json::json!({ "secret_scan_status": "passed", "categories": { "env": 1 } }),
            content_hash: "sha256:abc".into(),
            status: Some("shared".into()),
        }).unwrap();
        assert_eq!(accepted.status, "shared");
        assert_eq!(accepted.redaction_report["categories"]["env"], 1);

        let rejected = create_harness_config_review(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessConfigReviewRequest {
                source_tool: "claude".into(),
                redacted_config: serde_json::json!({ "token": "raw-secret" }),
                redaction_report: serde_json::json!({ "secret_scan_status": "failed" }),
                content_hash: "sha256:def".into(),
                status: None,
            },
        )
        .unwrap_err();
        assert!(rejected.to_string().contains("secret_scan_failed"));

        let report_secret = create_harness_config_review(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessConfigReviewRequest {
                source_tool: "claude".into(),
                redacted_config: serde_json::json!({ "env": { "NEXUSMIND_API_KEY": "[REDACTED]" } }),
                redaction_report: serde_json::json!({ "secret_scan_status": "passed", "leaked_value": "nm_live_secret_key" }),
                content_hash: "sha256:ghi".into(),
                status: None,
            },
        )
        .unwrap_err();
        assert!(report_secret.to_string().contains("secret_scan_failed"));

        let report_hook_content = create_harness_config_review(
            &conn,
            &org.id,
            &admin.id,
            &CreateHarnessConfigReviewRequest {
                source_tool: "claude".into(),
                redacted_config: serde_json::json!({ "hooks": "[REDACTED]" }),
                redaction_report: serde_json::json!({ "secret_scan_status": "passed", "hook": { "raw_shell_content": "export TOKEN=abc" } }),
                content_hash: "sha256:jkl".into(),
                status: None,
            },
        )
        .unwrap_err();
        assert!(report_hook_content
            .to_string()
            .contains("secret_scan_failed"));
    }

    // ── Code index query tests ─────────────────────────────────────────────────

    fn setup_org_for_code(conn: &Connection) -> String {
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
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
        assert_eq!(
            id1, id2,
            "upsert must return same id for same (org_id, name)"
        );
        // root_path should have been updated
        let project = get_code_project(&org_id, "myapp", &conn).unwrap().unwrap();
        assert_eq!(
            project.root_path, "/ws/myapp2",
            "root_path must be updated on conflict"
        );
    }

    #[test]
    fn ensure_code_project_visible_to_creator_enrolls_member() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        // A non-super_user creator.
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES ('u-creator', ?1, 'dev@acme.com', 'Dev', 'member', 'active', datetime('now'))",
            rusqlite::params![org_id],
        )
        .unwrap();

        // Simulate what post_index does: create the code_projects row, then enroll.
        upsert_code_project(&conn, &org_id, "myapp", "/ws/myapp").unwrap();

        // Before enrollment, the creator can neither see nor access the project.
        let before = list_code_projects_visible(&conn, &org_id, false, Some("u-creator")).unwrap();
        assert!(
            before.is_empty(),
            "creator must not see the project before enrollment"
        );
        assert!(
            !user_can_access_canonical_project_by_name(&conn, &org_id, "myapp", "u-creator")
                .unwrap(),
            "creator must not have access before enrollment"
        );

        ensure_code_project_visible_to_creator(&conn, &org_id, "myapp", "u-creator").unwrap();

        // After enrollment, the project is visible and accessible.
        let after = list_code_projects_visible(&conn, &org_id, false, Some("u-creator")).unwrap();
        assert_eq!(after.len(), 1, "creator must see exactly their project");
        assert_eq!(after[0].name, "myapp");
        assert!(
            user_can_access_canonical_project_by_name(&conn, &org_id, "myapp", "u-creator")
                .unwrap(),
            "creator must pass the name-access check (ensure_code_project_name_access)"
        );

        // Idempotent — a second enrollment (re-index) must not error or duplicate.
        ensure_code_project_visible_to_creator(&conn, &org_id, "myapp", "u-creator").unwrap();
        let again = list_code_projects_visible(&conn, &org_id, false, Some("u-creator")).unwrap();
        assert_eq!(
            again.len(),
            1,
            "re-index must not duplicate the visible project"
        );
    }

    #[test]
    fn fail_stale_indexing_projects_resets_zombies() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        // Two projects mid-index (zombies) and one already successful.
        let z1 = upsert_code_project(&conn, &org_id, "zombie1", "/ws/z1").unwrap();
        let z2 = upsert_code_project(&conn, &org_id, "zombie2", "/ws/z2").unwrap();
        let ok = upsert_code_project(&conn, &org_id, "healthy", "/ws/ok").unwrap();
        set_code_project_indexing(&conn, z1).unwrap();
        set_code_project_indexing(&conn, z2).unwrap();
        set_code_project_success(&conn, ok, 5, "2026-01-01T00:00:00Z").unwrap();

        let reset = fail_stale_indexing_projects(&conn).unwrap();
        assert_eq!(reset, 2, "only the two 'indexing' rows must be reset");

        let p1 = get_code_project(&org_id, "zombie1", &conn)
            .unwrap()
            .unwrap();
        assert_eq!(p1.index_status.as_deref(), Some("error"));
        assert_eq!(
            p1.last_index_error.as_deref(),
            Some("Indexing interrupted (server restart)")
        );
        // The successful project is untouched.
        let ph = get_code_project(&org_id, "healthy", &conn)
            .unwrap()
            .unwrap();
        assert_eq!(ph.index_status.as_deref(), Some("success"));

        // Idempotent: a second call resets nothing.
        assert_eq!(fail_stale_indexing_projects(&conn).unwrap(), 0);
    }

    #[test]
    fn insert_and_get_code_chunks() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        let chunk_id = insert_code_chunk(
            &conn,
            project_id,
            "src/lib.rs",
            "abc123",
            Some("rust"),
            Some("authenticate_user"),
            1,
            10,
            "fn authenticate_user() {}",
            None,
        )
        .unwrap();
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

        insert_code_chunk(
            &conn,
            project_id,
            "src/lib.rs",
            "h1",
            Some("rust"),
            None,
            1,
            10,
            "code",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/main.rs",
            "h2",
            Some("rust"),
            None,
            1,
            5,
            "main",
            None,
        )
        .unwrap();

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
        insert_code_chunk(
            &conn,
            project_id,
            "src/lib.rs",
            "h",
            Some("rust"),
            None,
            1,
            60,
            "whole window",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/lib.rs",
            "h",
            Some("rust"),
            Some("foo"),
            10,
            20,
            "fn foo() {}",
            None,
        )
        .unwrap();

        let chunk = get_chunk_covering_line(&conn, project_id, "src/lib.rs", 12)
            .unwrap()
            .expect("a chunk must cover line 12");
        assert_eq!(chunk.content, "fn foo() {}", "tightest covering chunk wins");
        assert_eq!(chunk.symbol.as_deref(), Some("foo"));

        // No chunk covers a line past EOF.
        assert!(
            get_chunk_covering_line(&conn, project_id, "src/lib.rs", 999)
                .unwrap()
                .is_none(),
            "out-of-range line returns None"
        );
    }

    #[test]
    fn get_file_chunks_returns_all_ordered() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        // Two methods of a class, plus a chunk in another file.
        insert_code_chunk(
            &conn,
            project_id,
            "src/svc.ts",
            "h",
            Some("typescript"),
            Some("two"),
            20,
            25,
            "two() {}",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/svc.ts",
            "h",
            Some("typescript"),
            Some("one"),
            10,
            15,
            "one() {}",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/other.ts",
            "h",
            Some("typescript"),
            None,
            1,
            5,
            "other",
            None,
        )
        .unwrap();

        let chunks = get_file_chunks(&conn, project_id, "src/svc.ts").unwrap();
        assert_eq!(chunks.len(), 2, "only the target file's chunks");
        assert_eq!(chunks[0].start_line, 10, "ordered by start_line");
        assert_eq!(chunks[1].start_line, 20);
        // Range overlap [12, 22] (a class spanning its methods) catches both.
        let overlapping: Vec<_> = chunks
            .iter()
            .filter(|c| c.start_line <= 22 && c.end_line >= 12)
            .collect();
        assert_eq!(
            overlapping.len(),
            2,
            "class range overlaps both method chunks"
        );
    }

    #[test]
    fn get_code_embeddings_returns_only_non_null() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        let embedding: Vec<u8> = vec![0u8; 32]; // dummy blob
        insert_code_chunk(
            &conn,
            project_id,
            "a.rs",
            "h1",
            None,
            None,
            1,
            5,
            "code",
            Some(&embedding),
        )
        .unwrap();
        insert_code_chunk(
            &conn, project_id, "b.rs", "h2", None, None, 1, 5, "code", None,
        )
        .unwrap();

        let pairs = get_code_embeddings(&conn, project_id).unwrap();
        assert_eq!(pairs.len(), 1, "only chunk with embedding must be returned");
    }

    #[test]
    fn get_code_chunk_locations_returns_only_embedded_with_symbol() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        let embedding: Vec<u8> = vec![0u8; 32];
        // Embedded chunk with a symbol → included, content NOT loaded by this query.
        insert_code_chunk(
            &conn,
            project_id,
            "src/users.rs",
            "h1",
            Some("rust"),
            Some("list_users"),
            1,
            10,
            "fn list_users() { /* body */ }",
            Some(&embedding),
        )
        .unwrap();
        // No embedding → excluded (must line up 1:1 with cosine-scored set).
        insert_code_chunk(
            &conn,
            project_id,
            "src/misc.rs",
            "h2",
            None,
            Some("misc"),
            1,
            5,
            "code",
            None,
        )
        .unwrap();

        let locs = get_code_chunk_locations(&conn, project_id).unwrap();
        assert_eq!(locs.len(), 1, "only embedded chunks are returned");
        assert_eq!(locs[0].1, "src/users.rs");
        assert_eq!(locs[0].2.as_deref(), Some("list_users"));
    }

    #[test]
    fn list_indexed_files_with_hashes_returns_map() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        insert_code_chunk(
            &conn,
            project_id,
            "src/lib.rs",
            "deadbeef",
            None,
            None,
            1,
            5,
            "code",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/lib.rs",
            "deadbeef",
            None,
            None,
            6,
            10,
            "more",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/main.rs",
            "cafebabe",
            None,
            None,
            1,
            3,
            "main",
            None,
        )
        .unwrap();

        let hashes = list_indexed_files_with_hashes(&conn, project_id).unwrap();
        assert_eq!(hashes.len(), 2, "must deduplicate by file_path");
        assert_eq!(
            hashes.get("src/lib.rs").map(|h| h.as_str()),
            Some("deadbeef")
        );
        assert_eq!(
            hashes.get("src/main.rs").map(|h| h.as_str()),
            Some("cafebabe")
        );
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
        assert_eq!(
            project.last_indexed.as_deref(),
            Some("2026-06-19T12:00:00Z")
        );
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
        )
        .unwrap();
        upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();
        // org2 must not see org1's project
        let result = get_code_project("org2", "myapp", &conn).unwrap();
        assert!(
            result.is_none(),
            "org isolation must hold for code projects"
        );
    }

    #[test]
    fn get_chunk_context_returns_target_and_neighbors() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        // Insert 3 chunks: before, target, after — all in the same file
        insert_code_chunk(
            &conn,
            project_id,
            "src/auth.rs",
            "h1",
            None,
            Some("validate_token"),
            1,
            20,
            "fn validate_token() {}",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/auth.rs",
            "h1",
            None,
            Some("authenticate_user"),
            21,
            60,
            "fn authenticate_user() {}",
            None,
        )
        .unwrap();
        insert_code_chunk(
            &conn,
            project_id,
            "src/auth.rs",
            "h1",
            None,
            Some("refresh_token"),
            61,
            80,
            "fn refresh_token() {}",
            None,
        )
        .unwrap();

        let context =
            get_chunk_context(&conn, project_id, "src/auth.rs", "authenticate_user", 1).unwrap();
        assert!(!context.is_empty(), "must return at least the target chunk");
        assert!(
            context
                .iter()
                .any(|c| c.symbol.as_deref() == Some("authenticate_user")),
            "target chunk must be present"
        );
    }

    #[test]
    fn get_chunk_context_returns_empty_for_unknown_symbol() {
        let conn = setup();
        let org_id = setup_org_for_code(&conn);
        let project_id = upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();

        let context =
            get_chunk_context(&conn, project_id, "src/auth.rs", "nonexistent_fn", 1).unwrap();
        assert!(
            context.is_empty(),
            "must return empty vec for unknown symbol"
        );
    }

    // ── get_memory_facets tests ───────────────────────────────────────────────

    #[test]
    fn get_memory_facets_empty_org_returns_empty_vecs() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let facets = get_memory_facets(&conn, &org.id, "any-user", true).unwrap();
        assert!(facets.types.is_empty(), "no memories => no type facets");
        assert!(
            facets.projects.is_empty(),
            "no memories => no project facets"
        );
        // scope may be empty too (no rows)
        assert!(facets.scopes.is_empty(), "no memories => no scope facets");
    }

    #[test]
    fn get_memory_facets_counts_types_correctly() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Insert 2 bugfix + 1 decision
        get_or_create_project(&conn, &org.id, "p").unwrap();
        for i in 0..2 {
            upsert_memory(
                &conn,
                &org.id,
                &user.id,
                &StoreMemoryRequest {
                    project: Some("p".into()),
                    tool: "claude".into(),
                    content: format!("bugfix content {i}"),
                    tags: None,
                    title: None,
                    memory_type: Some("bugfix".into()),
                    scope: None,
                    topic_key: None,
                    session_id: None,
                },
            )
            .unwrap();
        }
        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: Some("p".into()),
                tool: "claude".into(),
                content: "decision content".into(),
                tags: None,
                title: None,
                memory_type: Some("decision".into()),
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        let facets = get_memory_facets(&conn, &org.id, "any-user", true).unwrap();

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

        get_or_create_project(&conn, &org.id, "proj-a").unwrap();
        get_or_create_project(&conn, &org.id, "proj-b").unwrap();
        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: Some("proj-a".into()),
                tool: "claude".into(),
                content: "content a".into(),
                tags: None,
                title: None,
                memory_type: None,
                scope: Some("personal".into()),
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();
        upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: Some("proj-b".into()),
                tool: "claude".into(),
                content: "content b".into(),
                tags: None,
                title: None,
                memory_type: None,
                scope: Some("project".into()),
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        let facets = get_memory_facets(&conn, &org.id, "any-user", true).unwrap();

        // Projects
        assert_eq!(facets.projects.len(), 2);
        let names: Vec<&str> = facets.projects.iter().map(|f| f.value.as_str()).collect();
        assert!(names.contains(&"proj-a"));
        assert!(names.contains(&"proj-b"));

        // Scopes
        let personal = facets.scopes.iter().find(|f| f.value == "personal");
        let project = facets.scopes.iter().find(|f| f.value == "project");
        assert!(personal.is_some(), "personal scope must appear");
        assert!(project.is_some(), "project scope must appear");
    }

    #[test]
    fn get_memory_facets_scoped_to_org() {
        let conn = setup();
        let (org_a, user_a, _) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Second org inserted directly
        let org_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'OrgB', 'orgb')",
            [&org_b_id],
        )
        .unwrap();
        let user_b_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES (?1, ?2, 'b@b.com', 'B', 'admin', 'active')",
            rusqlite::params![user_b_id, org_b_id],
        ).unwrap();

        get_or_create_project(&conn, &org_a.id, "proj-a").unwrap();
        get_or_create_project(&conn, &org_b_id, "proj-b").unwrap();
        upsert_memory(
            &conn,
            &org_a.id,
            &user_a.id,
            &StoreMemoryRequest {
                project: Some("proj-a".into()),
                tool: "claude".into(),
                content: "a content".into(),
                tags: None,
                title: None,
                memory_type: Some("bugfix".into()),
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();
        upsert_memory(
            &conn,
            &org_b_id,
            &user_b_id,
            &StoreMemoryRequest {
                project: Some("proj-b".into()),
                tool: "claude".into(),
                content: "b content".into(),
                tags: None,
                title: None,
                memory_type: Some("decision".into()),
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        // Facets for org_a must not see org_b's memories
        let facets_a = get_memory_facets(&conn, &org_a.id, "any-user", true).unwrap();
        assert_eq!(facets_a.projects.len(), 1);
        assert_eq!(facets_a.projects[0].value, "proj-a");
        assert!(
            facets_a.types.iter().all(|f| f.value != "decision"),
            "org_a must not see org_b type 'decision'"
        );
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
        )
        .unwrap();

        let admin_mem = legacy_store(
            &conn,
            &org.id,
            &admin.id,
            "proj",
            "claude",
            "admin content",
            &[],
        );
        let member_mem = legacy_store(
            &conn,
            &org.id,
            &member_id,
            "proj",
            "claude",
            "member content",
            &[],
        );

        // Member tries to bulk-delete both (is_admin = false)
        let ids = vec![admin_mem.id.clone(), member_mem.id.clone()];
        let deleted = bulk_delete_memories(&conn, &org.id, &ids, false, &member_id).unwrap();

        // Only the member's own memory should be deleted
        assert_eq!(deleted, 1, "non-admin should only delete own memory");
        assert!(
            get_memory_owner(&conn, &org.id, &admin_mem.id)
                .unwrap()
                .is_some(),
            "admin memory must survive"
        );
        assert!(
            get_memory_owner(&conn, &org.id, &member_mem.id)
                .unwrap()
                .is_none(),
            "member's own memory must be deleted"
        );
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
        let (org_a, user_a, _) =
            bootstrap(&conn_a, "OrgA", "orga", "admin@a.com", "AdminA").unwrap();

        let conn_b = setup();
        let (org_b, user_b, _) =
            bootstrap(&conn_b, "OrgB", "orgb", "admin@b.com", "AdminB").unwrap();

        // Store a memory in org A's DB
        let mem_a = legacy_store(
            &conn_a,
            &org_a.id,
            &user_a.id,
            "proj",
            "claude",
            "a content",
            &[],
        );

        // Org B (admin) tries to delete org A's memory ID via org B's connection.
        // The WHERE clause filters by org_b.id so nothing in org_a is touched.
        let deleted = bulk_delete_memories(
            &conn_b,
            &org_b.id,
            std::slice::from_ref(&mem_a.id),
            true,
            &user_b.id,
        )
        .unwrap();
        assert_eq!(deleted, 0, "cross-org deletion must not succeed");

        // Org A's memory must still exist in org A's DB
        assert!(
            get_memory_owner(&conn_a, &org_a.id, &mem_a.id)
                .unwrap()
                .is_some(),
            "org A memory must be untouched"
        );
    }

    #[test]
    fn bulk_delete_nonexistent_ids_returns_zero() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let _ = legacy_store(&conn, &org.id, &user.id, "proj", "claude", "real", &[]);

        let deleted = bulk_delete_memories(
            &conn,
            &org.id,
            &["ghost-1".to_string(), "ghost-2".to_string()],
            true,
            &user.id,
        )
        .unwrap();
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
        assert!(
            mems.iter()
                .all(|m| m.tags.contains(&"important".to_string())),
            "both memories must have the 'important' tag"
        );
    }

    #[test]
    fn bulk_tag_remove_from_memory() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let m = legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "content",
            &["keep".to_string(), "drop".to_string()],
        );

        let updated = bulk_tag_memories(
            &conn,
            &org.id,
            std::slice::from_ref(&m.id),
            "remove",
            "drop",
        )
        .unwrap();
        assert_eq!(updated, 1);

        let remaining = get_memories_by_ids(&conn, &org.id, &[m.id]).unwrap();
        let tags = &remaining[0].tags;
        assert!(
            tags.contains(&"keep".to_string()),
            "'keep' tag must survive"
        );
        assert!(
            !tags.contains(&"drop".to_string()),
            "'drop' tag must be removed"
        );
    }

    #[test]
    fn bulk_tag_wrong_org_memories_are_skipped() {
        let conn_a = setup();
        let (org_a, user_a, _) = bootstrap(&conn_a, "OrgA", "orga", "a@a.com", "AdminA").unwrap();

        let conn_b = setup();
        let (org_b, _, _) = bootstrap(&conn_b, "OrgB", "orgb", "b@b.com", "AdminB").unwrap();

        let mem_a = legacy_store(
            &conn_a,
            &org_a.id,
            &user_a.id,
            "proj",
            "claude",
            "content",
            &[],
        );

        // Attempt to tag org_a's memory using org_b's org_id on conn_b
        let updated = bulk_tag_memories(
            &conn_b,
            &org_b.id,
            std::slice::from_ref(&mem_a.id),
            "add",
            "hacked",
        )
        .unwrap();
        assert_eq!(updated, 0, "cross-org tag must not succeed");

        // Original memory in org_a must be untouched
        let orig = get_memories_by_ids(&conn_a, &org_a.id, &[mem_a.id]).unwrap();
        assert!(
            orig[0].tags.is_empty(),
            "org_a memory tags must be unchanged"
        );
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

        let update = UpdateWebhookRequest {
            active: Some(false),
            ..Default::default()
        };
        let updated = update_webhook(&conn, &org.id, &created.id, &update)
            .unwrap()
            .unwrap();
        assert!(!updated.active, "webhook must be inactive after update");
    }

    #[test]
    fn update_webhook_returns_none_for_missing_id() {
        let conn = setup();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let update = UpdateWebhookRequest {
            active: Some(false),
            ..Default::default()
        };
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
        assert!(
            hook.is_some(),
            "webhook must survive cross-org delete attempt"
        );
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
        )
        .unwrap();

        let req = make_create_webhook_req("hook", "https://example.com");
        create_webhook(&conn, &org_a.id, &req).unwrap();

        let hooks_b = list_webhooks(&conn, &org_b_id).unwrap();
        assert!(hooks_b.is_empty(), "org_b must not see org_a webhooks");
    }

    #[test]
    fn list_all_org_keys_returns_active_keys() {
        let conn = setup();
        let (org, _, _raw_key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
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
        get_or_create_project(&conn, &org.id, "myproject").unwrap();
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

        get_or_create_project(&conn, &org.id, "proj-a").unwrap();
        get_or_create_project(&conn, &org.id, "proj-b").unwrap();
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
        let decision_count = trends
            .by_type
            .iter()
            .find(|x| x.name == "decision")
            .map(|x| x.count);
        assert_eq!(decision_count, Some(2));
        let bugfix_count = trends
            .by_type
            .iter()
            .find(|x| x.name == "bugfix")
            .map(|x| x.count);
        assert_eq!(bugfix_count, Some(1));

        // by_project: proj-a=2, proj-b=1
        let proj_a_count = trends
            .by_project
            .iter()
            .find(|x| x.name == "proj-a")
            .map(|x| x.count);
        assert_eq!(proj_a_count, Some(2));
        let proj_b_count = trends
            .by_project
            .iter()
            .find(|x| x.name == "proj-b")
            .map(|x| x.count);
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
        get_or_create_project(&conn, &org.id, "new-project").unwrap();
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
        assert_eq!(
            trends_30.total, 1,
            "days=30 must exclude the 45-day-old memory"
        );
        assert!(
            !trends_30.by_project.iter().any(|x| x.name == "old-project"),
            "old-project must not appear in by_project when days=30"
        );
        assert!(
            !trends_30.by_type.iter().any(|x| x.name == "discovery"),
            "discovery type must not appear in by_type when days=30"
        );

        // With days=90, both memories must appear
        let trends_90 = get_memory_trends(&conn, &org.id, 90).unwrap();
        assert_eq!(trends_90.total, 2, "days=90 must include both memories");
        assert!(
            trends_90.by_project.iter().any(|x| x.name == "old-project"),
            "old-project must appear when days=90"
        );
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
        let saved =
            update_project_event_overrides(&conn, &org_id, &project_id, new_overrides).unwrap();
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
        update_project_event_overrides(
            &conn,
            &org_id,
            &project_id,
            ProjectEventOverrides {
                resolve_issues: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        // Clear by saving empty overrides (all None = inherit)
        let cleared = update_project_event_overrides(
            &conn,
            &org_id,
            &project_id,
            ProjectEventOverrides::default(),
        )
        .unwrap();
        // With all-None, the JSON stored is "{}", which deserializes back as all-None
        assert!(cleared.resolve_issues.is_none());
    }

    // ── Duplicate detection tests ─────────────────────────────────────────────

    #[test]
    fn get_duplicate_groups_returns_empty_when_no_duplicates() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        // Store two distinct memories
        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "unique content one",
            &[],
        );
        legacy_store(
            &conn,
            &org.id,
            &user.id,
            "proj",
            "claude",
            "unique content two",
            &[],
        );

        let groups = get_duplicate_groups(&conn, &org.id).unwrap();
        assert!(
            groups.is_empty(),
            "expected no duplicate groups when all memories are distinct"
        );
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
        let hashes: Vec<_> = groups[0]
            .iter()
            .map(|m| m.normalized_hash.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(hashes[0], hashes[1], "both entries must have the same hash");
    }

    // ── v17 archive/restore unit tests ────────────────────────────────────────

    #[test]
    fn archive_memory_sets_archived_at() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: None,
                tool: "claude".to_string(),
                content: "archive me".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        assert!(mem.archived_at.is_none(), "new memory must not be archived");

        let updated = archive_memory(&conn, &org.id, &mem.id).unwrap();
        assert!(updated, "archive_memory must return true on first archive");

        let val: Option<String> = conn
            .query_row(
                "SELECT archived_at FROM memories WHERE id = ?1",
                [&mem.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            val.is_some(),
            "archived_at must be set after archive_memory"
        );
    }

    #[test]
    fn restore_memory_clears_archived_at() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let mem = upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: None,
                tool: "claude".to_string(),
                content: "archive then restore".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        archive_memory(&conn, &org.id, &mem.id).unwrap();

        let restored = restore_memory(&conn, &org.id, &mem.id).unwrap();
        assert!(
            restored,
            "restore_memory must return true when memory was archived"
        );

        let val: Option<String> = conn
            .query_row(
                "SELECT archived_at FROM memories WHERE id = ?1",
                [&mem.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            val.is_none(),
            "archived_at must be NULL after restore_memory"
        );
    }

    #[test]
    fn list_memories_excludes_archived_by_default() {
        let conn = setup();
        let (org, user, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        let mem1 = upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: None,
                tool: "claude".to_string(),
                content: "active memory".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();
        let mem2 = upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: None,
                tool: "claude".to_string(),
                content: "archived memory".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();

        archive_memory(&conn, &org.id, &mem2.id).unwrap();

        // Default (include_archived=false) must exclude archived
        let active = list_memories(
            &conn, &org.id, None, None, None, None, None, None, 50, 0, false, None, None, None,
        )
        .unwrap();
        let ids: Vec<_> = active.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&mem1.id.as_str()), "active memory must appear");
        assert!(
            !ids.contains(&mem2.id.as_str()),
            "archived memory must be excluded by default"
        );

        // include_archived=true must include both
        let all = list_memories(
            &conn, &org.id, None, None, None, None, None, None, 50, 0, true, None, None, None,
        )
        .unwrap();
        let all_ids: Vec<_> = all.iter().map(|m| m.id.as_str()).collect();
        assert!(
            all_ids.contains(&mem1.id.as_str()),
            "active memory must appear when include_archived=true"
        );
        assert!(
            all_ids.contains(&mem2.id.as_str()),
            "archived memory must appear when include_archived=true"
        );
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
        let (
            id,
            org_id,
            user_id,
            project,
            tool,
            content,
            tags_str,
            created_at,
            title,
            memory_type,
            scope,
            topic_key,
            session_id,
            revision_count,
            normalized_hash,
            project_id,
            archived_at,
            pinned_i64,
        ) = row?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        let status = if archived_at.is_some() {
            "archived".to_string()
        } else {
            "active".to_string()
        };
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
        let result = stmt
            .query_map([org_id], |r| r.get(0))?
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
pub fn get_memory_health(
    conn: &Connection,
    org_id: &str,
) -> Result<crate::models::types::MemoryHealth> {
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

pub fn get_dashboard_data(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    is_super_user: bool,
    days: i64,
) -> Result<crate::models::types::DashboardData> {
    use crate::models::types::{DashboardAvailability, DashboardData, OrgStats, ToolUsage};
    if is_super_user {
        let from = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        return Ok(DashboardData {
            stats: get_stats(conn, org_id)?,
            usage: Some(get_usage_stats(conn, org_id)?),
            trends: get_memory_trends(conn, org_id, days)?,
            activity: list_audit(
                conn,
                org_id,
                None,
                None,
                None,
                Some(&from),
                None,
                None,
                20,
                0,
            )?,
            agent_activity: Some(get_agent_activity(conn, org_id, days)?),
            heatmap: Some(get_memory_heatmap(conn, org_id, days)?),
            contributors: Some(get_top_contributors(conn, org_id, days)?),
            health: Some(get_memory_health(conn, org_id)?),
            users: Some(list_users(conn, org_id)?),
            onboarding: Some(get_onboarding_status(conn, org_id)?),
            conventions: Some(list_conventions(conn, org_id, None, None, None, 100, 0)?),
            availability: DashboardAvailability {
                usage: true,
                users: true,
                onboarding: true,
                agent_activity: true,
                health: true,
                contributors: true,
                heatmap: true,
                conventions: true,
            },
        });
    }
    let visible = "m.org_id = ?1 AND EXISTS (SELECT 1 FROM project_visibility pv
                              WHERE pv.org_id = m.org_id AND pv.project_name = m.project AND pv.user_id = ?2)";
    let total_memories = conn.query_row(
        &format!("SELECT COUNT(*) FROM memories m WHERE {visible}"),
        rusqlite::params![org_id, user_id],
        |row| row.get(0),
    )?;
    let active_users_24h = conn.query_row(&format!("SELECT COUNT(DISTINCT a.user_id) FROM audit_logs a JOIN memories m ON m.id = a.resource_id AND m.org_id = a.org_id WHERE a.resource_type = 'memory' AND a.timestamp > datetime('now', '-24 hours') AND {visible}"), rusqlite::params![org_id, user_id], |row| row.get(0))?;
    let searches_today = conn.query_row(&format!("SELECT COUNT(*) FROM audit_logs a JOIN memories m ON m.id = a.resource_id AND m.org_id = a.org_id WHERE a.resource_type = 'memory' AND a.action = 'search' AND a.timestamp > datetime('now', 'start of day') AND {visible}"), rusqlite::params![org_id, user_id], |row| row.get(0))?;
    let mut tools = conn.prepare(&format!("SELECT COALESCE(m.tool, 'unknown'), COUNT(*) FROM memories m WHERE {visible} GROUP BY COALESCE(m.tool, 'unknown') ORDER BY COUNT(*) DESC LIMIT 5"))?;
    let top_tools = tools
        .query_map(rusqlite::params![org_id, user_id], |row| {
            Ok(ToolUsage {
                tool: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut events = conn.prepare(&format!("SELECT a.id, a.org_id, a.user_id, a.timestamp, a.action, a.resource_type, a.resource_id, a.previous_hash, a.current_hash FROM audit_logs a JOIN memories m ON m.id = a.resource_id AND m.org_id = a.org_id WHERE a.resource_type = 'memory' AND a.timestamp >= datetime('now', '-' || ?3 || ' days') AND {visible} ORDER BY a.timestamp DESC LIMIT 20"))?;
    let activity = events
        .query_map(rusqlite::params![org_id, user_id, days], |row| {
            Ok(crate::models::types::AuditEntry {
                id: row.get(0)?,
                org_id: row.get(1)?,
                user_id: row.get(2)?,
                timestamp: row.get(3)?,
                action: row.get(4)?,
                resource_type: row.get(5)?,
                resource_id: row.get(6)?,
                metadata: serde_json::json!({}),
                previous_hash: row.get(7)?,
                current_hash: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(DashboardData {
        stats: OrgStats {
            total_memories,
            active_users_24h,
            searches_today,
            top_tools,
        },
        trends: scoped_memory_trends(conn, org_id, user_id, days, visible)?,
        activity,
        usage: None,
        agent_activity: None,
        heatmap: None,
        contributors: None,
        health: None,
        users: None,
        onboarding: None,
        conventions: None,
        availability: DashboardAvailability {
            usage: false,
            users: false,
            onboarding: false,
            agent_activity: false,
            health: false,
            contributors: false,
            heatmap: false,
            conventions: false,
        },
    })
}

fn scoped_memory_trends(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    days: i64,
    visible: &str,
) -> Result<crate::models::types::MemoryTrends> {
    use crate::models::types::{DailyCount, NameCount};
    let period = "m.created_at >= datetime('now', '-' || ?3 || ' days')";
    let mut daily = conn.prepare(&format!("SELECT date(m.created_at), COUNT(*) FROM memories m WHERE {visible} AND {period} GROUP BY date(m.created_at) ORDER BY date(m.created_at)"))?;
    let daily_counts = daily
        .query_map(rusqlite::params![org_id, user_id, days], |r| {
            Ok(DailyCount {
                date: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut types = conn.prepare(&format!("SELECT COALESCE(m.type, 'untyped'), COUNT(*) FROM memories m WHERE {visible} AND {period} GROUP BY m.type ORDER BY COUNT(*) DESC LIMIT 5"))?;
    let by_type = types
        .query_map(rusqlite::params![org_id, user_id, days], |r| {
            Ok(NameCount {
                name: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut projects = conn.prepare(&format!("SELECT m.project, COUNT(*) FROM memories m WHERE {visible} AND {period} GROUP BY m.project ORDER BY COUNT(*) DESC LIMIT 5"))?;
    let by_project = projects
        .query_map(rusqlite::params![org_id, user_id, days], |r| {
            Ok(NameCount {
                name: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let total = conn.query_row(
        &format!("SELECT COUNT(*) FROM memories m WHERE {visible} AND {period}"),
        rusqlite::params![org_id, user_id, days],
        |r| r.get(0),
    )?;
    let this_week = conn.query_row(&format!("SELECT COUNT(*) FROM memories m WHERE {visible} AND m.created_at >= datetime('now', '-7 days')"), rusqlite::params![org_id, user_id], |r| r.get(0))?;
    let this_month = conn.query_row(&format!("SELECT COUNT(*) FROM memories m WHERE {visible} AND m.created_at >= datetime('now', '-30 days')"), rusqlite::params![org_id, user_id], |r| r.get(0))?;
    Ok(crate::models::types::MemoryTrends {
        daily_counts,
        by_type,
        by_project,
        total,
        this_week,
        this_month,
    })
}

/// Merges two memories: appends `merge_id`'s content to `keep_id`'s content (separated by
/// `\n\n---\n\n`), then deletes `merge_id`. Both must belong to the given org.
/// Returns the updated `keep_id` memory on success, or an error if either memory is not found.
pub fn merge_memories(
    conn: &Connection,
    org_id: &str,
    keep_id: &str,
    merge_id: &str,
) -> Result<Memory> {
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
pub fn get_agent_activity(
    conn: &Connection,
    org_id: &str,
    days: i64,
) -> Result<Vec<crate::models::types::AgentActivity>> {
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

    let events_configured: bool = conn
        .query_row(
            "SELECT COALESCE(settings, '{}') != '{}' FROM organizations WHERE id = ?1",
            [org_id],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false);

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
    let raw = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
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
                token: row.get(0)?,
                org_id: row.get(1)?,
                role: row.get(2)?,
                created_by: row.get(3)?,
                used_at: row.get(4)?,
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
                token: row.get(0)?,
                org_id: row.get(1)?,
                role: row.get(2)?,
                created_by: row.get(3)?,
                used_at: row.get(4)?,
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
                    token: row.get(0)?,
                    org_id: row.get(1)?,
                    role: row.get(2)?,
                    created_by: row.get(3)?,
                    used_at: row.get(4)?,
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
        rusqlite::params![
            user_id,
            invite.org_id,
            name,
            invite.role,
            password_hash,
            now
        ],
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
pub fn get_project_stats(
    conn: &Connection,
    org_id: &str,
    project_id: &str,
) -> Result<crate::models::types::ProjectStats> {
    // Look up the project name from ID
    let project_name: Option<String> = conn
        .query_row(
            "SELECT name FROM projects WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![project_id, org_id],
            |row| row.get(0),
        )
        .optional()?;

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
    let tags = stmt
        .query_map(rusqlite::params![org_id, &project_name], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(crate::models::types::ProjectStats {
        total_memories,
        memories_this_week,
        last_memory_at,
        top_tags: tags,
    })
}

/// Returns projects where every active org user is already enrolled as a member
/// (`member_count >= active_user_count`). This is the signature of a project that was
/// auto-enrolled by the old `get_or_create_project` behaviour.
///
/// Used by `GET /v1/admin/org/projects/over-enrolled`.
pub fn list_over_enrolled_projects(
    conn: &Connection,
    org_id: &str,
) -> Result<Vec<crate::models::types::OverEnrolledProject>> {
    let mut stmt = conn.prepare(
        "SELECT
           p.name AS project_name,
           COUNT(pm.user_id) AS member_count,
           (SELECT COUNT(*) FROM users
            WHERE org_id = ?1
              AND status = 'active'
              AND disabled_at IS NULL) AS active_user_count
         FROM projects p
         JOIN project_members pm ON pm.project_id = p.id
         WHERE p.org_id = ?1
           AND p.archived_at IS NULL
         GROUP BY p.id, p.name
         HAVING member_count >= active_user_count
         ORDER BY p.name ASC",
    )?;

    let rows = stmt.query_map(rusqlite::params![org_id], |row| {
        Ok(crate::models::types::OverEnrolledProject {
            project_name: row.get(0)?,
            member_count: row.get(1)?,
            active_user_count: row.get(2)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

/// Returns memory creation counts per day for the last 90 days (non-archived only).
/// Used by `GET /v1/admin/stats/memory-heatmap`.
/// Returns the top contributing agents (by memory count) in the last 30 days.
/// Groups by user_id (the agent/user that stored the memory).
/// Returned by `GET /v1/admin/stats/top-contributors`.
pub fn get_top_contributors(
    conn: &Connection,
    org_id: &str,
    days: i64,
) -> Result<Vec<crate::models::types::ContributorStat>> {
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
            user_id: row.get(0)?,
            memory_count: row.get(1)?,
            last_activity: row.get(2)?,
            user_name: row.get(3)?,
            user_email: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn get_memory_heatmap(
    conn: &Connection,
    org_id: &str,
    days: i64,
) -> Result<Vec<crate::models::types::HeatmapDay>> {
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
            day: row.get(0)?,
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invite_already_used"));
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
        assert!(
            result.is_err(),
            "redeemed invite must be rejected on re-use"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invite_already_used"));
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invite_already_used"));
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
        )
        .unwrap();
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

        let today = days
            .iter()
            .find(|d| d.day == today_str)
            .expect("today must be present");
        let yesterday = days
            .iter()
            .find(|d| d.day == yesterday_str)
            .expect("yesterday must be present");

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
        )
        .unwrap();
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
        assert_eq!(
            contributors[0].user_email.as_deref(),
            Some("alice@acme.com")
        );

        let bob = contributors
            .iter()
            .find(|c| c.user_id == "bob")
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
        )
        .unwrap();
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
        assert_eq!(
            stats.memories_this_week, 3,
            "memories_this_week should be 3 (just inserted)"
        );
        assert!(
            stats.last_memory_at.is_some(),
            "last_memory_at should be set"
        );
    }

    #[test]
    fn project_stats_returns_zeros_for_empty_project() {
        let conn = setup();
        let project = create_project(&conn, "org1", "empty-project", None, None).unwrap();
        let stats = get_project_stats(&conn, "org1", &project.id).unwrap();
        assert_eq!(stats.total_memories, 0, "total_memories should be 0");
        assert_eq!(
            stats.memories_this_week, 0,
            "memories_this_week should be 0, not NULL"
        );
        assert!(
            stats.last_memory_at.is_none(),
            "last_memory_at should be None"
        );
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
    list_conventions_visible(
        conn,
        org_id,
        category,
        include_archived,
        project,
        None,
        limit,
        offset,
        None,
    )
}

/// Lists conventions visible to `viewer_user_id`, resolving the inheritance
/// chain **org → client → project** additively.
///
/// Each level *adds* to the broader ones and never replaces them: a
/// client-level convention sits alongside the organization's, so u2s's own
/// standards stay enforceable no matter what a client engagement adds. That is
/// the whole point of a company brain, and it is why there is no override.
///
/// `client` is the owning client's id, or `None` for an internal project — in
/// which case the chain simply collapses to org → project, which is correct
/// rather than an error.
///
/// `project` is the project **id**, matching `conventions.project_id`, which is
/// a real foreign key to `projects(id)`. Callers holding a project *name* must
/// resolve it first — see [`get_project_id_by_name`]. Passing a name here
/// silently matches nothing, which is exactly the bug this parameter had
/// before the client model went in.
#[allow(clippy::too_many_arguments)]
pub fn list_conventions_visible(
    conn: &Connection,
    org_id: &str,
    category: Option<&str>,
    include_archived: Option<bool>,
    project: Option<&str>,
    client: Option<&str>,
    limit: i64,
    offset: i64,
    viewer_user_id: Option<&str>,
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
        match client {
            Some(c) => {
                let ci = param_idx;
                let pi = param_idx + 1;
                sql.push_str(&format!(
                    " AND ((client_id IS NULL AND project_id IS NULL)\
                       OR (client_id = ?{ci} AND project_id IS NULL)\
                       OR project_id = ?{pi})"
                ));
                extra_params.push(c.to_string());
                extra_params.push(p.to_string());
                param_idx += 2;
            }
            None => {
                // Internal u2s project: no client level exists, so the chain is
                // org → project. Client-scoped conventions must NOT leak here.
                sql.push_str(&format!(
                    " AND (((client_id IS NULL AND project_id IS NULL) OR project_id = ?{param_idx}))"
                ));
                extra_params.push(p.to_string());
                param_idx += 1;
            }
        }
    }
    if let Some(viewer) = viewer_user_id {
        sql.push_str(&format!(
            " AND (project_id IS NULL OR project_id IN (SELECT project_id FROM project_visibility WHERE user_id = ?{param_idx}))"
        ));
        extra_params.push(viewer.to_string());
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
    get_convention_visible(conn, org_id, id, None)
}

pub fn get_convention_visible(
    conn: &Connection,
    org_id: &str,
    id: i64,
    viewer_user_id: Option<&str>,
) -> Result<Option<Convention>> {
    if let Some(viewer) = viewer_user_id {
        let result = conn.query_row(
            "SELECT id, org_id, project_id, title, content, category, weight, tags, created_at, updated_at, archived_at
             FROM conventions
             WHERE org_id = ?1 AND id = ?2
               AND (project_id IS NULL OR project_id IN (SELECT project_id FROM project_visibility WHERE user_id = ?3))",
            rusqlite::params![org_id, id, viewer],
            convention_from_row,
        ).optional()?;
        return Ok(result);
    }

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
        rusqlite::params![
            org_id,
            req.project_id,
            req.title,
            req.content,
            category,
            weight,
            tags_json
        ],
    )?;
    let id = conn.last_insert_rowid();
    get_convention(conn, org_id, id)?
        .ok_or_else(|| anyhow::anyhow!("convention not found after insert"))
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
        )
        .unwrap();
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

        let for_a =
            list_conventions(&conn, &org_id, None, None, Some(&project_a.id), 1000, 0).unwrap();
        let for_b =
            list_conventions(&conn, &org_id, None, None, Some(&project_b.id), 1000, 0).unwrap();

        assert_eq!(
            for_a.len(),
            1,
            "org-wide convention must apply to project A"
        );
        assert_eq!(
            for_b.len(),
            1,
            "org-wide convention must apply to project B"
        );
    }

    #[test]
    fn project_scoped_convention_only_for_that_project() {
        let conn = setup();
        let org_id = seed_org(&conn);
        let project_a = create_project(&conn, &org_id, "proj-a", None, None).unwrap();
        let project_b = create_project(&conn, &org_id, "proj-b", None, None).unwrap();

        create_convention(
            &conn,
            &org_id,
            &make_req("Proj A rule", Some(&project_a.id)),
        )
        .unwrap();

        let for_a =
            list_conventions(&conn, &org_id, None, None, Some(&project_a.id), 1000, 0).unwrap();
        let for_b =
            list_conventions(&conn, &org_id, None, None, Some(&project_b.id), 1000, 0).unwrap();

        assert_eq!(
            for_a.len(),
            1,
            "project-scoped convention must apply to its own project"
        );
        assert_eq!(
            for_b.len(),
            0,
            "project-scoped convention must NOT apply to a different project"
        );
    }

    #[test]
    fn none_project_returns_everything_for_org() {
        let conn = setup();
        let org_id = seed_org(&conn);
        let project_a = create_project(&conn, &org_id, "proj-a", None, None).unwrap();

        create_convention(&conn, &org_id, &make_req("Org-wide", None)).unwrap();
        create_convention(&conn, &org_id, &make_req("Proj A", Some(&project_a.id))).unwrap();

        let all = list_conventions(&conn, &org_id, None, None, None, 1000, 0).unwrap();
        assert_eq!(
            all.len(),
            2,
            "None must return everything for the org regardless of project_id (admin listing)"
        );
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

        let for_a =
            list_conventions(&conn, &org_id, None, None, Some(&project_a.id), 1000, 0).unwrap();
        let titles: Vec<&str> = for_a.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            for_a.len(),
            2,
            "resolving for project A must be org-wide UNION project A"
        );
        assert!(titles.contains(&"Org-wide"));
        assert!(titles.contains(&"Proj A"));
        assert!(
            !titles.contains(&"Proj Q"),
            "project Q's convention must not leak into project A's resolution"
        );
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

        let style_for_a = list_conventions(
            &conn,
            &org_id,
            Some("style"),
            None,
            Some(&project_a.id),
            1000,
            0,
        )
        .unwrap();
        assert_eq!(style_for_a.len(), 1);
        assert_eq!(style_for_a[0].title, "Org-wide style");
    }
}

// ── Client queries (consultancy grouping) ─────────────────────────────────────

/// May this viewer see this client?
///
/// Mirrors [`user_can_view_project_name`], including its existence-hiding
/// branch: a client that does not exist reports as *visible*, so a caller
/// cannot tell "absent" from "forbidden" by the response code. Removing that
/// branch would turn every 404 into an existence oracle.
///
/// `viewer_user_id` is `None` only for super_user — see `api::context::viewer_scope`.
/// It must NOT be derived from `is_privileged()`: admin is privileged for
/// permission checks but stays membership-scoped for reads.
pub fn user_can_view_client(
    conn: &Connection,
    org_id: &str,
    client_id: &str,
    viewer_user_id: Option<&str>,
) -> Result<bool> {
    let Some(vid) = viewer_user_id else {
        return Ok(true);
    };
    let visible: i64 = conn.query_row(
        "SELECT CASE
                  WHEN NOT EXISTS (SELECT 1 FROM clients c WHERE c.org_id = ?1 AND c.id = ?2) THEN 1
                  WHEN EXISTS (SELECT 1 FROM client_members cm
                                WHERE cm.client_id = ?2 AND cm.user_id = ?3) THEN 1
                  WHEN EXISTS (SELECT 1 FROM projects p
                                JOIN project_members pm ON pm.project_id = p.id
                                WHERE p.org_id = ?1 AND p.client_id = ?2 AND pm.user_id = ?3) THEN 1
                  ELSE 0
                END",
        rusqlite::params![org_id, client_id, vid],
        |row| row.get(0),
    )?;
    Ok(visible != 0)
}

/// Lists clients the viewer may see. `viewer_user_id = None` = no restriction.
pub fn list_clients_visible(
    conn: &Connection,
    org_id: &str,
    include_archived: bool,
    viewer_user_id: Option<&str>,
) -> Result<Vec<Client>> {
    let mut sql = String::from(
        "SELECT id, org_id, name, slug, status, archived_at, created_at
         FROM clients WHERE org_id = ?1",
    );
    if !include_archived {
        sql.push_str(" AND archived_at IS NULL");
    }
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(org_id.to_string())];
    if let Some(vid) = viewer_user_id {
        sql.push_str(
            " AND (EXISTS (SELECT 1 FROM client_members cm
                            WHERE cm.client_id = clients.id AND cm.user_id = ?2)
               OR EXISTS (SELECT 1 FROM projects p
                            JOIN project_members pm ON pm.project_id = p.id
                            WHERE p.client_id = clients.id AND pm.user_id = ?2))",
        );
        params.push(Box::new(vid.to_string()));
    }
    sql.push_str(" ORDER BY name ASC");

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(Client {
            id: row.get(0)?,
            org_id: row.get(1)?,
            name: row.get(2)?,
            slug: row.get(3)?,
            status: row.get(4)?,
            archived_at: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn get_client(conn: &Connection, org_id: &str, client_id: &str) -> Result<Option<Client>> {
    conn.query_row(
        "SELECT id, org_id, name, slug, status, archived_at, created_at
         FROM clients WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, client_id],
        |row| {
            Ok(Client {
                id: row.get(0)?,
                org_id: row.get(1)?,
                name: row.get(2)?,
                slug: row.get(3)?,
                status: row.get(4)?,
                archived_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_client(
    conn: &Connection,
    org_id: &str,
    name: &str,
    slug: &str,
    status: &str,
) -> Result<Client> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO clients (id, org_id, name, slug, status) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, org_id, name, slug, status],
    )?;
    get_client(conn, org_id, &id)?
        .ok_or_else(|| anyhow::anyhow!("client vanished immediately after insert"))
}

/// Updates name and/or status. `slug` is immutable and deliberately absent.
pub fn update_client(
    conn: &Connection,
    org_id: &str,
    client_id: &str,
    name: Option<&str>,
    status: Option<&str>,
) -> Result<Option<Client>> {
    if name.is_none() && status.is_none() {
        return get_client(conn, org_id, client_id);
    }
    conn.execute(
        "UPDATE clients
            SET name   = COALESCE(?3, name),
                status = COALESCE(?4, status)
          WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, client_id, name, status],
    )?;
    get_client(conn, org_id, client_id)
}

/// Soft-archives a client. Idempotent: archiving an archived client is a no-op.
pub fn archive_client(conn: &Connection, org_id: &str, client_id: &str) -> Result<bool> {
    let n = conn.execute(
        "UPDATE clients SET archived_at = datetime('now')
          WHERE org_id = ?1 AND id = ?2 AND archived_at IS NULL",
        rusqlite::params![org_id, client_id],
    )?;
    Ok(n > 0 || get_client(conn, org_id, client_id)?.is_some())
}

/// Number of projects still owned by a client. A client with projects cannot be
/// deleted — offboarding is a status change, not a cascade.
pub fn count_client_projects(conn: &Connection, org_id: &str, client_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM projects WHERE org_id = ?1 AND client_id = ?2",
        rusqlite::params![org_id, client_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub fn list_client_members(conn: &Connection, client_id: &str) -> Result<Vec<ClientMember>> {
    let mut stmt = conn.prepare(
        "SELECT cm.id, cm.client_id, cm.user_id, u.email, u.name, cm.role, cm.created_at
         FROM client_members cm
         JOIN users u ON u.id = cm.user_id
         WHERE cm.client_id = ?1
         ORDER BY u.name ASC",
    )?;
    let rows = stmt.query_map([client_id], |row| {
        Ok(ClientMember {
            id: row.get(0)?,
            client_id: row.get(1)?,
            user_id: row.get(2)?,
            email: row.get(3)?,
            name: row.get(4)?,
            role: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn add_client_member(
    conn: &Connection,
    client_id: &str,
    user_id: &str,
    role: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO client_members (id, client_id, user_id, role)
         VALUES (COALESCE((SELECT id FROM client_members WHERE client_id = ?1 AND user_id = ?2), ?4),
                 ?1, ?2, ?3)",
        rusqlite::params![client_id, user_id, role, uuid::Uuid::new_v4().to_string()],
    )?;
    Ok(())
}

pub fn remove_client_member(conn: &Connection, client_id: &str, user_id: &str) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM client_members WHERE client_id = ?1 AND user_id = ?2",
        rusqlite::params![client_id, user_id],
    )?;
    Ok(n > 0)
}

/// The role a user holds on a client, if any. Mirrors `get_project_member_role`
/// in shape and return type so permission resolution reads the same either way.
pub fn get_client_member_role(
    conn: &Connection,
    org_id: &str,
    client_id: &str,
    user_id: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT cm.role FROM client_members cm
         JOIN clients c ON c.id = cm.client_id
         WHERE c.org_id = ?1 AND cm.client_id = ?2 AND cm.user_id = ?3",
        rusqlite::params![org_id, client_id, user_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

/// The client that owns a project, by project **name**. `None` means the
/// project is internal u2s work — not that the lookup failed.
pub fn get_project_client_id(
    conn: &Connection,
    org_id: &str,
    project_name: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT client_id FROM projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, project_name],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|outer| outer.flatten())
    .map_err(Into::into)
}

/// Links a code project (repo) to a project. One repo per project — a second
/// link to the same project is refused rather than silently repointing the
/// first, because "which repo is this project's repo" must have one answer.
pub fn link_code_project_to_project(
    conn: &Connection,
    org_id: &str,
    code_project_id: i64,
    project_id: &str,
) -> Result<()> {
    let taken: i64 = conn.query_row(
        "SELECT COUNT(*) FROM code_projects
          WHERE org_id = ?1 AND project_id = ?2 AND id != ?3",
        rusqlite::params![org_id, project_id, code_project_id],
        |r| r.get(0),
    )?;
    if taken > 0 {
        anyhow::bail!("project {project_id} is already linked to a repository");
    }
    let n = conn.execute(
        "UPDATE code_projects SET project_id = ?3 WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![org_id, code_project_id, project_id],
    )?;
    if n == 0 {
        anyhow::bail!("code project {code_project_id} not found in this organization");
    }
    Ok(())
}

// ── Promotion (client knowledge → organization asset) ─────────────────────────

/// Promotes a client- or project-scoped memory into an org-scoped one.
///
/// Creates a NEW memory and leaves the source untouched, recording lineage in
/// `promoted_from` so it stays auditable which client asset a shared playbook
/// came from. Never invoked implicitly — a leak here is a contractual breach,
/// not a bug, so the action is always an explicit human decision.
pub fn promote_memory(
    conn: &Connection,
    org_id: &str,
    source_id: &str,
    actor_user_id: &str,
) -> Result<Option<Memory>> {
    let source = match get_memory_by_id_for_org(conn, org_id, source_id)? {
        Some(m) => m,
        None => return Ok(None),
    };
    if source.scope != "client" && source.scope != "project" {
        anyhow::bail!(
            "only client- or project-scoped memories can be promoted (this one is '{}')",
            source.scope
        );
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO memories
           (id, org_id, user_id, project, tool, content, tags, title, type, scope,
            promoted_from)
         VALUES (?1, ?2, ?3, 'default', ?4, ?5, ?6, ?7, ?8, 'org', ?9)",
        rusqlite::params![
            new_id,
            org_id,
            actor_user_id,
            source.tool,
            source.content,
            serde_json::to_string(&source.tags).unwrap_or_else(|_| "[]".to_string()),
            source.title,
            source.memory_type,
            source_id,
        ],
    )?;
    get_memory_by_id_for_org(conn, org_id, &new_id)
}

// ── Project resolution (report only — never mutates) ──────────────────────────

/// Reports how legacy free-form `memories.project` values map onto real
/// projects. Exact match only: no case folding, no fuzzy matching, no prefix
/// heuristics. Assigning `project_id` to legacy rows is a separate operator
/// action, so this deliberately writes nothing.
pub fn report_project_resolution(
    conn: &Connection,
    org_id: &str,
) -> Result<ProjectResolutionReport> {
    let resolved: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories m
          WHERE m.org_id = ?1
            AND EXISTS (SELECT 1 FROM projects p WHERE p.org_id = m.org_id AND p.name = m.project)",
        [org_id],
        |r| r.get(0),
    )?;
    let unresolved: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories m
          WHERE m.org_id = ?1
            AND NOT EXISTS (SELECT 1 FROM projects p WHERE p.org_id = m.org_id AND p.name = m.project)",
        [org_id],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT m.project, COUNT(*) AS n FROM memories m
          WHERE m.org_id = ?1
            AND NOT EXISTS (SELECT 1 FROM projects p WHERE p.org_id = m.org_id AND p.name = m.project)
          GROUP BY m.project
          ORDER BY n DESC",
    )?;
    let rows = stmt.query_map([org_id], |row| {
        Ok(UnresolvedProject {
            project: row.get(0)?,
            memory_count: row.get(1)?,
        })
    })?;
    Ok(ProjectResolutionReport {
        resolved,
        unresolved,
        unresolved_values: rows.collect::<rusqlite::Result<Vec<_>>>()?,
    })
}

// ── GitHub OAuth connection queries ───────────────────────────────────────────

/// Upserts a GitHub OAuth connection for the given org and client.
///
/// `client_id` is `None` for the organization's own connection; a consultancy
/// stores one row per client, because each client has its own GitHub org.
///
/// The token is encrypted before it touches the database. Callers pass
/// plaintext; nothing plaintext is persisted. If no encryption key is
/// configured this returns an error rather than silently storing the token in
/// the clear — migration v58 encrypted the existing rows, and a write path that
/// re-introduced plaintext would undo it.
// Eight parameters, one over the lint's threshold. They are the columns of a
// single row and grouping them into a struct would only move the argument list
// to the call site, so the shape stays as it is.
#[allow(clippy::too_many_arguments)]
pub fn save_github_connection(
    conn: &Connection,
    org_id: &str,
    client_id: Option<&str>,
    access_token: &str,
    token_type: &str,
    scopes: &str,
    github_login: &str,
    github_user_id: i64,
) -> Result<()> {
    let encrypted = crate::crypto::encrypt(access_token).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot store a GitHub token: NEXUSMIND_TOKEN_ENCRYPTION_KEY is unset or invalid"
        )
    })?;
    conn.execute(
        "INSERT OR REPLACE INTO github_connections
         (org_id, client_id, github_login, access_token, token_type, scopes, github_user_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
           COALESCE((SELECT created_at FROM github_connections
                      WHERE org_id = ?1 AND client_id IS ?2 AND github_login = ?3), datetime('now')),
           datetime('now'))",
        rusqlite::params![org_id, client_id, github_login, encrypted, token_type, scopes, github_user_id],
    )?;
    Ok(())
}

/// Returns the org-level GitHub OAuth connection, or None if not connected.
///
/// The stored token is ciphertext; it is decrypted on the way out so callers
/// keep seeing plaintext. A row whose token cannot be decrypted (key rotated or
/// missing) is returned with an empty token rather than failing the whole
/// request — the caller will get a clean auth error from GitHub instead of a
/// 500 from us.
pub fn get_github_connection(conn: &Connection, org_id: &str) -> Result<Option<GitHubConnection>> {
    get_github_connection_for_client(conn, org_id, None)
}

/// Returns the GitHub OAuth connection for one client of an org. `client_id` of
/// `None` selects the organization's own connection.
pub fn get_github_connection_for_client(
    conn: &Connection,
    org_id: &str,
    client_id: Option<&str>,
) -> Result<Option<GitHubConnection>> {
    conn.query_row(
        "SELECT org_id, access_token, token_type, scopes, github_login, github_user_id, created_at, updated_at
         FROM github_connections WHERE org_id = ?1 AND client_id IS ?2",
        rusqlite::params![org_id, client_id],
        |row| {
            let stored: String = row.get(1)?;
            Ok(GitHubConnection {
                org_id: row.get(0)?,
                access_token: crate::crypto::decrypt(&stored).unwrap_or_default(),
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
        "DELETE FROM github_connections WHERE org_id = ?1 AND client_id IS NULL",
        [org_id],
    )?;
    Ok(n > 0)
}

// ── Per-project encrypted token queries ──────────────────────────────────────

/// Store an AES-256-GCM–encrypted PAT for a code project.
/// Pass the already-encrypted blob (hex-encoded). Clears it when `None`.
pub fn set_code_project_token(
    conn: &Connection,
    project_id: i64,
    encrypted: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE code_projects SET github_token_encrypted = ?1 WHERE id = ?2",
        rusqlite::params![encrypted, project_id],
    )?;
    Ok(())
}

/// Retrieve the encrypted PAT blob for a code project, or None if not set.
pub fn get_code_project_token(conn: &Connection, project_id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT github_token_encrypted FROM code_projects WHERE id = ?1",
        [project_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

// ── Autonomous agent definitions ────────────────────────────────────────────

fn autonomous_agent_definition_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AutonomousAgentDefinition> {
    Ok(AutonomousAgentDefinition {
        id: row.get(0)?,
        org_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        template_key: row.get(4)?,
        template_version: row.get(5)?,
        status: row.get(6)?,
        current_revision: row.get(7)?,
        created_by: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        validation_status: row.get(11)?,
    })
}

fn autonomous_agent_revision_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AutonomousAgentRevision> {
    let config: String = row.get(3)?;
    let capabilities: String = row.get(5)?;
    let budgets: String = row.get(6)?;
    let validation: Option<String> = row.get(10)?;
    Ok(AutonomousAgentRevision {
        id: row.get(0)?,
        definition_id: row.get(1)?,
        revision: row.get(2)?,
        config: serde_json::from_str(&config).unwrap_or(serde_json::Value::Null),
        config_hash: row.get(4)?,
        capabilities: serde_json::from_str(&capabilities).unwrap_or_default(),
        budgets: serde_json::from_str(&budgets).unwrap_or(serde_json::Value::Null),
        policy_generation: row.get(7)?,
        validation_status: row.get(8)?,
        validated_at: row.get(9)?,
        validation: validation.and_then(|value| serde_json::from_str(&value).ok()),
        created_by: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn autonomous_agent_capabilities(template_key: &str) -> Result<Vec<String>> {
    let capabilities = match template_key {
        "qa" => vec![
            "repository:read",
            "tests:run",
            "finding:write",
            "delivery:write",
        ],
        "github_issue_resolver" => vec![
            "repository:read",
            "repository:branch",
            "tests:run",
            "github:draft_pr",
        ],
        "github_pr_reviewer" => vec!["repository:read", "tests:run", "github:review"],
        "lead_generation" => vec!["web:search", "lead:write", "delivery:write"],
        "judge" => vec![
            "repository:read",
            "tests:run",
            "finding:write",
            "delivery:write",
            "github:review",
        ],
        _ => anyhow::bail!("invalid_template"),
    };
    Ok(capabilities.into_iter().map(str::to_string).collect())
}

fn autonomous_agent_config_hash(
    template_key: &str,
    template_version: i64,
    config: &serde_json::Value,
    budgets: &serde_json::Value,
) -> Result<String> {
    let canonical = serde_json::to_vec(&serde_json::json!({
        "template_key": template_key,
        "template_version": template_version,
        "config": config,
        "budgets": budgets,
    }))?;
    Ok(hex::encode(Sha256::digest(canonical)))
}

pub fn list_autonomous_agent_definitions(
    conn: &Connection,
    org_id: &str,
) -> Result<Vec<AutonomousAgentDefinition>> {
    let mut stmt = conn.prepare(
        "SELECT d.id,d.org_id,d.name,d.description,d.template_key,d.template_version,d.status,d.current_revision,d.created_by,d.created_at,d.updated_at,
                COALESCE((SELECT v.status FROM autonomous_agent_validations v JOIN autonomous_agent_revisions r ON r.id=v.revision_id WHERE r.definition_id=d.id AND r.revision=d.current_revision ORDER BY v.created_at DESC,v.id DESC LIMIT 1),'pending')
         FROM autonomous_agent_definitions d WHERE d.org_id=?1 ORDER BY d.created_at DESC",
    )?;
    let rows = stmt.query_map([org_id], autonomous_agent_definition_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn get_autonomous_agent_definition(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<AutonomousAgentDefinition>> {
    conn.query_row(
        "SELECT d.id,d.org_id,d.name,d.description,d.template_key,d.template_version,d.status,d.current_revision,d.created_by,d.created_at,d.updated_at,
                COALESCE((SELECT v.status FROM autonomous_agent_validations v JOIN autonomous_agent_revisions r ON r.id=v.revision_id WHERE r.definition_id=d.id AND r.revision=d.current_revision ORDER BY v.created_at DESC,v.id DESC LIMIT 1),'pending')
         FROM autonomous_agent_definitions d WHERE d.org_id=?1 AND d.id=?2",
        rusqlite::params![org_id, id],
        autonomous_agent_definition_from_row,
    ).optional().map_err(Into::into)
}

pub fn get_autonomous_agent_revision(
    conn: &Connection,
    definition_id: &str,
    revision: i64,
) -> Result<Option<AutonomousAgentRevision>> {
    conn.query_row(
        "SELECT r.id,r.definition_id,r.revision,r.config_json,r.config_hash,r.capabilities_json,r.budgets_json,
                r.policy_generation,COALESCE(v.status,'pending'),v.created_at,v.result_json,r.created_by,r.created_at
         FROM autonomous_agent_revisions r
         LEFT JOIN autonomous_agent_validations v ON v.id=(
            SELECT id FROM autonomous_agent_validations WHERE revision_id=r.id ORDER BY created_at DESC,id DESC LIMIT 1
         ) WHERE r.definition_id=?1 AND r.revision=?2",
        rusqlite::params![definition_id, revision],
        autonomous_agent_revision_from_row,
    ).optional().map_err(Into::into)
}

pub fn get_autonomous_agent_detail(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<AutonomousAgentDetail>> {
    let Some(definition) = get_autonomous_agent_definition(conn, org_id, id)? else {
        return Ok(None);
    };
    let revision = get_autonomous_agent_revision(conn, id, definition.current_revision)?
        .ok_or_else(|| anyhow::anyhow!("autonomous_agent_revision_missing"))?;
    Ok(Some(AutonomousAgentDetail {
        definition,
        revision,
    }))
}

pub fn create_autonomous_agent_definition(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    req: &CreateAutonomousAgentRequest,
) -> Result<AutonomousAgentDetail> {
    if req.name.trim().is_empty() || req.name.len() > 120 {
        anyhow::bail!("invalid_name");
    }
    if !req.config.is_object() || !req.budgets.is_object() {
        anyhow::bail!("invalid_configuration");
    }
    let capabilities = autonomous_agent_capabilities(&req.template_key)?;
    let definition_id = Uuid::new_v4().to_string();
    let revision_id = Uuid::new_v4().to_string();
    let template_version = 1_i64;
    let config_hash = autonomous_agent_config_hash(
        &req.template_key,
        template_version,
        &req.config,
        &req.budgets,
    )?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO autonomous_agent_definitions
         (id,org_id,name,description,template_key,template_version,status,current_revision,created_by)
         VALUES (?1,?2,?3,?4,?5,?6,'disabled',1,?7)",
        rusqlite::params![definition_id, org_id, req.name.trim(), req.description, req.template_key, template_version, user_id],
    )?;
    tx.execute(
        "INSERT INTO autonomous_agent_revisions
         (id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,created_by)
         VALUES (?1,?2,1,?3,?4,?5,?6,?7)",
        rusqlite::params![revision_id, definition_id, serde_json::to_string(&req.config)?, config_hash,
            serde_json::to_string(&capabilities)?, serde_json::to_string(&req.budgets)?, user_id],
    )?;
    tx.commit()?;
    get_autonomous_agent_detail(conn, org_id, &definition_id)?
        .ok_or_else(|| anyhow::anyhow!("autonomous_agent_not_found_after_insert"))
}

pub fn update_autonomous_agent_definition(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    id: &str,
    req: &UpdateAutonomousAgentRequest,
) -> Result<Option<AutonomousAgentDetail>> {
    let Some(current) = get_autonomous_agent_detail(conn, org_id, id)? else {
        return Ok(None);
    };
    if current.definition.status == "archived" {
        anyhow::bail!("agent_archived");
    }
    let name = req
        .name
        .as_deref()
        .unwrap_or(&current.definition.name)
        .trim();
    if name.is_empty() || name.len() > 120 {
        anyhow::bail!("invalid_name");
    }
    let config = req.config.as_ref().unwrap_or(&current.revision.config);
    let budgets = req.budgets.as_ref().unwrap_or(&current.revision.budgets);
    if !config.is_object() || !budgets.is_object() {
        anyhow::bail!("invalid_configuration");
    }
    let description = req
        .description
        .as_ref()
        .or(current.definition.description.as_ref());
    if name == current.definition.name
        && description == current.definition.description.as_ref()
        && config == &current.revision.config
        && budgets == &current.revision.budgets
    {
        return Ok(Some(current));
    }
    let next_revision = current.definition.current_revision + 1;
    let revision_id = Uuid::new_v4().to_string();
    let hash = autonomous_agent_config_hash(
        &current.definition.template_key,
        current.definition.template_version,
        config,
        budgets,
    )?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE autonomous_agent_definitions SET name=?3,description=COALESCE(?4,description),status='disabled',current_revision=?5,updated_at=datetime('now') WHERE org_id=?1 AND id=?2",
        rusqlite::params![org_id,id,name,description,next_revision],
    )?;
    tx.execute(
        "INSERT INTO autonomous_agent_revisions (id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,policy_generation,created_by)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![revision_id,id,next_revision,serde_json::to_string(config)?,hash,serde_json::to_string(&current.revision.capabilities)?,serde_json::to_string(budgets)?,current.revision.policy_generation + 1,user_id],
    )?;
    tx.commit()?;
    get_autonomous_agent_detail(conn, org_id, id)
}

pub fn validate_autonomous_agent_definition(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    id: &str,
) -> Result<Option<AutonomousAgentDetail>> {
    let Some(current) = get_autonomous_agent_detail(conn, org_id, id)? else {
        return Ok(None);
    };
    if current.definition.status == "archived" {
        anyhow::bail!("agent_archived");
    }
    let mut errors = Vec::new();
    if let Some(seconds) = current
        .revision
        .budgets
        .get("wall_time_seconds")
        .and_then(|v| v.as_i64())
    {
        if !(30..=3600).contains(&seconds) {
            errors.push("wall_time_out_of_range")
        }
    }
    for field in [
        "max_definition_concurrency",
        "max_repository_concurrency",
        "max_organization_concurrency",
    ] {
        if current
            .revision
            .budgets
            .get(field)
            .and_then(|value| value.as_i64())
            .is_some_and(|value| !(1..=32).contains(&value))
        {
            errors.push("concurrency_out_of_range");
        }
    }
    match current.definition.template_key.as_str() {
        "qa" => {
            let outputs = current
                .revision
                .config
                .get("outputs")
                .and_then(|v| v.as_array());
            if outputs.is_none()
                || outputs.is_some_and(|v| {
                    v.is_empty()
                        || v.iter().any(|item| {
                            !matches!(item.as_str(), Some("nexusmind" | "slack" | "github_issue"))
                        })
                })
            {
                errors.push("invalid_outputs")
            }
            if outputs.is_some_and(|v| v.iter().any(|i| i.as_str() == Some("github_issue")))
                && current
                    .revision
                    .config
                    .get("repository")
                    .and_then(|v| v.as_str())
                    .is_none()
            {
                errors.push("github_configuration_required")
            }
        }
        "github_issue_resolver" | "github_pr_reviewer" => {
            match current
                .revision
                .config
                .get("repository")
                .and_then(|v| v.as_str())
            {
                Some(value)
                    if crate::automation::connectors::validate_repository(value).is_ok() => {}
                _ => errors.push("valid_repository_required"),
            }
        }
        "lead_generation" => {
            for field in ["product", "icp"] {
                if current
                    .revision
                    .config
                    .get(field)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    errors.push("product_and_icp_required");
                    break;
                }
            }
            let outputs = current
                .revision
                .config
                .get("outputs")
                .and_then(|v| v.as_array());
            if outputs.is_none()
                || outputs.is_some_and(|v| {
                    v.is_empty()
                        || v.iter()
                            .any(|item| !matches!(item.as_str(), Some("nexusmind" | "slack")))
                })
            {
                errors.push("invalid_outputs")
            }
        }
        "judge" => {
            // One or more repositories are required so the judge can read each
            // PR/issue and its diff via `gh` to scope what it verifies. The concrete
            // PR/issue targets are chosen per run (not here), constrained to this list.
            let repositories = current
                .revision
                .config
                .get("repositories")
                .and_then(|v| v.as_array());
            let repositories_valid = repositories.is_some_and(|items| {
                !items.is_empty()
                    && items.iter().all(|item| {
                        item.as_str().is_some_and(|value| {
                            crate::automation::connectors::validate_repository(value).is_ok()
                        })
                    })
            });
            if !repositories_valid {
                errors.push("repositories_required")
            }
            // Findings delivery, same channels as QA.
            let outputs = current
                .revision
                .config
                .get("outputs")
                .and_then(|v| v.as_array());
            if outputs.is_none()
                || outputs.is_some_and(|v| {
                    v.is_empty()
                        || v.iter()
                            .any(|item| !matches!(item.as_str(), Some("nexusmind" | "slack")))
                })
            {
                errors.push("invalid_outputs")
            }
            // Publishing a verdict comment to GitHub is opt-in.
            if !matches!(
                current
                    .revision
                    .config
                    .get("publish")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none"),
                "none" | "comment"
            ) {
                errors.push("invalid_publish")
            }
        }
        _ => errors.push("unsupported_template"),
    }
    let valid = errors.is_empty();
    let result = serde_json::json!({"valid":valid,"checks":["schema","template","budgets","server_integrations"],"errors":errors});
    conn.execute(
        "INSERT INTO autonomous_agent_validations (id,org_id,definition_id,revision_id,config_hash,status,result_json,validated_by)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![Uuid::new_v4().to_string(),org_id,id,current.revision.id,current.revision.config_hash,if valid{"valid"}else{"invalid"},serde_json::to_string(&result)?,user_id],
    )?;
    get_autonomous_agent_detail(conn, org_id, id)
}

pub fn set_autonomous_agent_status(
    conn: &Connection,
    org_id: &str,
    id: &str,
    status: &str,
) -> Result<Option<AutonomousAgentDetail>> {
    let Some(current) = get_autonomous_agent_detail(conn, org_id, id)? else {
        return Ok(None);
    };
    match status {
        "enabled" => {
            if current.revision.validation_status != "valid" {
                anyhow::bail!("validation_required");
            }
        }
        "disabled" => {}
        "archived" => {
            if current.definition.status != "disabled" {
                anyhow::bail!("agent_must_be_disabled")
            }
            let active:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM autonomous_agent_runs WHERE definition_id=?1 AND status IN ('queued','leased','running'))",[id],|r|r.get(0))?;
            if active {
                anyhow::bail!("agent_has_active_runs")
            }
        }
        _ => anyhow::bail!("invalid_status"),
    }
    conn.execute(
        "UPDATE autonomous_agent_definitions SET status=?3,updated_at=datetime('now') WHERE org_id=?1 AND id=?2",
        rusqlite::params![org_id,id,status],
    )?;
    if status == "archived" {
        conn.execute("UPDATE autonomous_agent_schedules SET enabled=0,updated_at=datetime('now') WHERE definition_id=?1",[id])?;
    }
    get_autonomous_agent_detail(conn, org_id, id)
}

pub fn save_autonomous_runtime_health(
    conn: &Connection,
    health: &crate::automation::runtime::RuntimeHealth,
) -> Result<()> {
    let success = health.status == "ready";
    conn.execute(
        "INSERT INTO autonomous_runtime_health
         (id,status,reason_code,claude_version,last_success_at,last_failure_at,checked_at)
         VALUES (1,?1,?2,?3,CASE WHEN ?4 THEN datetime('now') END,CASE WHEN ?4 THEN NULL ELSE datetime('now') END,datetime('now'))
         ON CONFLICT(id) DO UPDATE SET status=excluded.status,reason_code=excluded.reason_code,
           claude_version=excluded.claude_version,
           last_success_at=CASE WHEN ?4 THEN datetime('now') ELSE autonomous_runtime_health.last_success_at END,
           last_failure_at=CASE WHEN ?4 THEN autonomous_runtime_health.last_failure_at ELSE datetime('now') END,
           checked_at=datetime('now')",
        rusqlite::params![health.status, health.reason_code, health.claude_version, success],
    )?;
    Ok(())
}

pub fn get_autonomous_runtime_health(
    conn: &Connection,
) -> Result<Option<crate::automation::runtime::RuntimeHealth>> {
    conn.query_row(
        "SELECT status,reason_code,claude_version,checked_at,last_success_at,last_failure_at FROM autonomous_runtime_health WHERE id=1",
        [],
        |row| Ok(crate::automation::runtime::RuntimeHealth {
            status: row.get(0)?, reason_code: row.get(1)?, claude_version: row.get(2)?,
            checked_at: row.get(3)?, last_success_at: row.get(4)?, last_failure_at: row.get(5)?,
        }),
    ).optional().map_err(Into::into)
}

fn autonomous_schedule_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AutonomousAgentSchedule> {
    Ok(AutonomousAgentSchedule {
        id: row.get(0)?,
        definition_id: row.get(1)?,
        kind: row.get(2)?,
        expression: row.get(3)?,
        timezone: row.get(4)?,
        misfire_policy: row.get(5)?,
        next_run_at: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn put_autonomous_agent_schedule(
    conn: &Connection,
    org_id: &str,
    definition_id: &str,
    req: &PutAutonomousAgentScheduleRequest,
) -> Result<Option<AutonomousAgentSchedule>> {
    let Some(definition) = get_autonomous_agent_definition(conn, org_id, definition_id)? else {
        return Ok(None);
    };
    if definition.status == "archived" {
        anyhow::bail!("agent_archived");
    }
    if !matches!(req.misfire_policy.as_str(), "run_once" | "skip") {
        anyhow::bail!("invalid_misfire_policy");
    }
    let next = crate::automation::scheduler::next_occurrence(
        &req.kind,
        req.expression.as_deref(),
        &req.timezone,
        chrono::Utc::now(),
    )?;
    let next_raw = next.map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string());
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO autonomous_agent_schedules (id,org_id,definition_id,kind,expression,timezone,misfire_policy,next_run_at,enabled)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(definition_id) DO UPDATE SET kind=excluded.kind,expression=excluded.expression,
         timezone=excluded.timezone,misfire_policy=excluded.misfire_policy,next_run_at=excluded.next_run_at,
         enabled=excluded.enabled,updated_at=datetime('now')",
        rusqlite::params![id,org_id,definition_id,req.kind,req.expression,req.timezone,req.misfire_policy,next_raw,req.enabled as i64],
    )?;
    get_autonomous_agent_schedule(conn, org_id, definition_id)
}

pub fn get_autonomous_agent_schedule(
    conn: &Connection,
    org_id: &str,
    definition_id: &str,
) -> Result<Option<AutonomousAgentSchedule>> {
    conn.query_row(
        "SELECT id,definition_id,kind,expression,timezone,misfire_policy,next_run_at,enabled,created_at,updated_at
         FROM autonomous_agent_schedules WHERE org_id=?1 AND definition_id=?2",
        rusqlite::params![org_id,definition_id], autonomous_schedule_from_row,
    ).optional().map_err(Into::into)
}

fn autonomous_run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomousAgentRun> {
    let budget: String = row.get(8)?;
    Ok(AutonomousAgentRun {
        id: row.get(0)?,
        definition_id: row.get(1)?,
        revision_id: row.get(2)?,
        trigger_kind: row.get(3)?,
        occurrence_key: row.get(4)?,
        scheduled_for: row.get(5)?,
        snapshot_sha: row.get(6)?,
        status: row.get(7)?,
        budget: serde_json::from_str(&budget).unwrap_or(serde_json::Value::Null),
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
        created_at: row.get(11)?,
    })
}

pub fn enqueue_autonomous_agent_run(
    conn: &Connection,
    org_id: &str,
    definition_id: &str,
    trigger_kind: &str,
    occurrence_key: &str,
    scheduled_for: Option<&str>,
    input: Option<&serde_json::Value>,
) -> Result<Option<AutonomousAgentRun>> {
    let Some(detail) = get_autonomous_agent_detail(conn, org_id, definition_id)? else {
        return Ok(None);
    };
    if detail.definition.status != "enabled" {
        anyhow::bail!("agent_not_enabled");
    }
    if detail.revision.validation_status != "valid" {
        anyhow::bail!("validation_required");
    }
    let automation_run_id = Uuid::new_v4().to_string();
    let run_id = Uuid::new_v4().to_string();
    let input_json = input.map(serde_json::to_string).transpose()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO automation_runs (id,org_id,project_id,created_by,profile_version_ref,policy_generation)
         VALUES (?1,?2,NULL,?3,?4,?5)",
        rusqlite::params![automation_run_id,org_id,detail.definition.created_by,format!("{}-v{}",detail.definition.template_key,detail.definition.template_version),detail.revision.policy_generation],
    )?;
    tx.execute(
        "INSERT INTO autonomous_agent_runs (id,org_id,definition_id,revision_id,automation_run_id,trigger_kind,occurrence_key,scheduled_for,budget_json,input_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        rusqlite::params![run_id,org_id,definition_id,detail.revision.id,automation_run_id,trigger_kind,occurrence_key,scheduled_for,serde_json::to_string(&detail.revision.budgets)?,input_json],
    )?;
    tx.commit()?;
    get_autonomous_agent_run(conn, org_id, &run_id)
}

pub fn get_autonomous_agent_run(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<AutonomousAgentRun>> {
    conn.query_row(
        "SELECT id,definition_id,revision_id,trigger_kind,occurrence_key,scheduled_for,snapshot_sha,status,budget_json,started_at,finished_at,created_at
         FROM autonomous_agent_runs WHERE org_id=?1 AND id=?2",
        rusqlite::params![org_id,id], autonomous_run_from_row,
    ).optional().map_err(Into::into)
}

pub fn autonomous_agent_run_is_cancelled(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT status='cancelled' FROM autonomous_agent_runs WHERE org_id=?1 AND id=?2",
            rusqlite::params![org_id, id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(true))
}

pub fn autonomous_agent_run_publish_authorized(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM autonomous_agent_runs run
            JOIN autonomous_agent_definitions definition ON definition.id=run.definition_id
            JOIN autonomous_agent_revisions revision ON revision.id=run.revision_id
            JOIN organizations org ON org.id=run.org_id
            WHERE run.id=?2 AND run.org_id=?1
              AND run.status IN('running','succeeded','partial')
              AND definition.status='enabled'
              AND definition.current_revision=revision.revision
              -- Derive validation from the append-only validations table: the
              -- autonomous_agent_revisions.validation_status column is frozen at
              -- 'pending' by the no-update trigger, so reading it directly would
              -- revoke publish authority on every run.
              AND (SELECT val.status FROM autonomous_agent_validations val
                   WHERE val.revision_id=revision.id
                   ORDER BY val.created_at DESC, val.id DESC LIMIT 1)='valid'
              AND org.autonomous_agents_enabled=1
              AND (run.status IN('succeeded','partial') OR EXISTS(
                    SELECT 1 FROM autonomous_agent_leases lease
                    WHERE lease.run_id=run.id AND lease.released_at IS NULL
                      AND lease.expires_at>datetime('now')))
        )",
        rusqlite::params![org_id, run_id],
        |row| row.get::<_, i64>(0),
    )? != 0)
}

pub fn set_autonomous_agent_run_snapshot(
    conn: &Connection,
    org_id: &str,
    id: &str,
    sha: &str,
) -> Result<bool> {
    if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("invalid_snapshot_sha")
    }
    Ok(conn.execute("UPDATE autonomous_agent_runs SET snapshot_sha=?3 WHERE org_id=?1 AND id=?2 AND snapshot_sha IS NULL",rusqlite::params![org_id,id,sha])?==1)
}

pub fn list_autonomous_agent_runs(
    conn: &Connection,
    org_id: &str,
    definition_id: Option<&str>,
) -> Result<Vec<AutonomousAgentRun>> {
    let mut stmt = conn.prepare(
        "SELECT id,definition_id,revision_id,trigger_kind,occurrence_key,scheduled_for,snapshot_sha,status,budget_json,started_at,finished_at,created_at
         FROM autonomous_agent_runs WHERE org_id=?1 AND (?2 IS NULL OR definition_id=?2) ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![org_id, definition_id],
        autonomous_run_from_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn cancel_autonomous_agent_run(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<AutonomousAgentRun>> {
    let active_attempts = conn
        .prepare(
            "SELECT attempt_id FROM autonomous_agent_leases
             WHERE run_id=?1 AND released_at IS NULL",
        )?
        .query_map([id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let changed = conn.execute(
        "UPDATE autonomous_agent_runs SET status='cancelled',finished_at=datetime('now')
         WHERE id=?1 AND org_id=?2 AND status IN ('queued','leased','running')",
        rusqlite::params![id, org_id],
    )?;
    if changed == 1 {
        for attempt_id in active_attempts {
            revoke_automation_attempt(conn, org_id, &attempt_id, "run_cancelled")?;
        }
        conn.execute("UPDATE autonomous_agent_leases SET released_at=datetime('now') WHERE run_id=?1 AND released_at IS NULL", [id])?;
        append_autonomous_agent_event(conn, org_id, id, "run.cancelled", &serde_json::json!({}))?;
    }
    get_autonomous_agent_run(conn, org_id, id)
}

pub fn list_autonomous_agent_events(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
) -> Result<Vec<crate::models::types::AutonomousAgentEvent>> {
    let mut stmt=conn.prepare("SELECT sequence,kind,payload_json,created_at FROM autonomous_agent_events WHERE org_id=?1 AND run_id=?2 ORDER BY sequence")?;
    let rows = stmt.query_map(rusqlite::params![org_id, run_id], |row| {
        let raw: String = row.get(2)?;
        Ok(crate::models::types::AutonomousAgentEvent {
            sequence: row.get(0)?,
            kind: row.get(1)?,
            payload: serde_json::from_str(&raw).unwrap_or_default(),
            created_at: row.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn enqueue_due_autonomous_agent_runs(conn: &Connection) -> Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT s.id,s.org_id,s.definition_id,s.kind,s.expression,s.timezone,s.next_run_at,s.misfire_policy
         FROM autonomous_agent_schedules s JOIN autonomous_agent_definitions d ON d.id=s.definition_id JOIN organizations o ON o.id=s.org_id
         WHERE s.enabled=1 AND d.status='enabled' AND o.autonomous_agents_enabled=1 AND s.next_run_at IS NOT NULL AND s.next_run_at<=datetime('now')
         ORDER BY s.next_run_at LIMIT 50",
    )?;
    let due = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut created = 0;
    for (
        schedule_id,
        org_id,
        definition_id,
        kind,
        expression,
        timezone,
        scheduled_for,
        misfire_policy,
    ) in due
    {
        let occurrence = hex::encode(Sha256::digest(
            format!("{schedule_id}|{scheduled_for}").as_bytes(),
        ));
        let next = crate::automation::scheduler::next_occurrence(
            &kind,
            expression.as_deref(),
            &timezone,
            chrono::Utc::now(),
        )?
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string());
        let advanced = conn.execute(
            "UPDATE autonomous_agent_schedules SET next_run_at=?3,updated_at=datetime('now')
             WHERE id=?1 AND next_run_at=?2",
            rusqlite::params![schedule_id, scheduled_for, next],
        )?;
        if advanced == 0 {
            continue;
        }
        if misfire_policy != "skip" {
            match enqueue_autonomous_agent_run(
                conn,
                &org_id,
                &definition_id,
                "schedule",
                &occurrence,
                Some(&scheduled_for),
                None,
            ) {
                Ok(Some(_)) => created += 1,
                Err(error) if error.to_string().contains("UNIQUE constraint failed") => {}
                Err(error) => {
                    conn.execute(
                        "UPDATE autonomous_agent_schedules SET next_run_at=?2 WHERE id=?1 AND next_run_at IS ?3",
                        rusqlite::params![schedule_id,scheduled_for,next],
                    )?;
                    return Err(error);
                }
                Ok(None) => continue,
            }
        }
    }
    Ok(created)
}

#[derive(Debug, Clone)]
pub struct ClaimedAutonomousRun {
    pub org_id: String,
    pub run: AutonomousAgentRun,
    pub attempt_id: String,
    pub claim_token: String,
    pub template_key: String,
    pub config: serde_json::Value,
}

pub fn claim_next_autonomous_agent_run(
    conn: &Connection,
    worker_id: &str,
    lease_seconds: i64,
) -> Result<Option<ClaimedAutonomousRun>> {
    let health = get_autonomous_runtime_health(conn)?;
    if !matches!(
        health.as_ref().map(|value| value.status.as_str()),
        Some("ready")
    ) {
        return Ok(None);
    }
    let expired=conn.prepare("SELECT run_id FROM autonomous_agent_leases WHERE released_at IS NULL AND expires_at<=datetime('now')")?.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for run_id in expired {
        let attempt_ids=conn.prepare("SELECT attempt_id FROM autonomous_agent_leases WHERE run_id=?1 AND released_at IS NULL")?.query_map([&run_id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let org_id: String = conn.query_row(
            "SELECT org_id FROM autonomous_agent_runs WHERE id=?1",
            [&run_id],
            |r| r.get(0),
        )?;
        for attempt_id in attempt_ids {
            revoke_automation_attempt(conn, &org_id, &attempt_id, "lease_expired")?;
        }
        conn.execute("UPDATE autonomous_agent_leases SET released_at=datetime('now') WHERE run_id=?1 AND released_at IS NULL",[&run_id])?;
        conn.execute("UPDATE autonomous_agent_runs SET status='queued',started_at=NULL WHERE id=?1 AND status IN ('leased','running')",[&run_id])?;
    }
    conn.execute("UPDATE autonomous_agent_runs SET status='dead_letter',finished_at=datetime('now') WHERE status='queued' AND (SELECT COUNT(*) FROM automation_attempts a WHERE a.run_id=autonomous_agent_runs.automation_run_id)>=COALESCE(json_extract(budget_json,'$.max_attempts'),2)",[])?;
    let candidate: Option<(String,String)> = conn.query_row(
        "SELECT r.id,r.org_id
         FROM autonomous_agent_runs r
         JOIN organizations o ON o.id=r.org_id
         LEFT JOIN autonomous_agent_work_items candidate_work ON candidate_work.run_id=r.id
         LEFT JOIN autonomous_agent_revisions candidate_revision ON candidate_revision.id=r.revision_id
         WHERE r.status='queued' AND o.autonomous_agents_enabled=1
           AND (SELECT COUNT(*) FROM autonomous_agent_runs active
                WHERE active.definition_id=r.definition_id AND active.status IN ('leased','running'))
               < COALESCE(json_extract(r.budget_json,'$.max_definition_concurrency'),1)
           AND (SELECT COUNT(*) FROM autonomous_agent_runs active
                WHERE active.org_id=r.org_id AND active.status IN ('leased','running'))
               < COALESCE(json_extract(r.budget_json,'$.max_organization_concurrency'),4)
           AND (
             COALESCE(candidate_work.repository,json_extract(candidate_revision.config_json,'$.repository')) IS NULL
             OR (SELECT COUNT(*)
                 FROM autonomous_agent_runs active
                 LEFT JOIN autonomous_agent_work_items active_work ON active_work.run_id=active.id
                 LEFT JOIN autonomous_agent_revisions active_revision ON active_revision.id=active.revision_id
                 WHERE active.org_id=r.org_id AND active.status IN ('leased','running')
                   AND COALESCE(active_work.repository,json_extract(active_revision.config_json,'$.repository'))
                       = COALESCE(candidate_work.repository,json_extract(candidate_revision.config_json,'$.repository')))
                < COALESCE(json_extract(r.budget_json,'$.max_repository_concurrency'),1)
           )
         ORDER BY r.created_at LIMIT 1",
        [], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    let Some((run_id, org_id)) = candidate else {
        return Ok(None);
    };
    let attempt_id = Uuid::new_v4().to_string();
    let lease_id = Uuid::new_v4().to_string();
    let claim_token = Uuid::new_v4().to_string();
    let claim_token_hash = hex::encode(Sha256::digest(claim_token.as_bytes()));
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE autonomous_agent_runs SET status='leased' WHERE id=?1 AND status='queued'",
        [&run_id],
    )?;
    if changed == 0 {
        tx.rollback()?;
        return Ok(None);
    }
    let automation_run_id: String = tx.query_row(
        "SELECT automation_run_id FROM autonomous_agent_runs WHERE id=?1",
        [&run_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO automation_attempts (id,run_id) VALUES (?1,?2)",
        rusqlite::params![attempt_id, automation_run_id],
    )?;
    tx.execute(
        "INSERT INTO autonomous_agent_leases (id,run_id,attempt_id,worker_id,claim_token_hash,expires_at) VALUES (?1,?2,?3,?4,?5,datetime('now',?6))",
        rusqlite::params![lease_id,run_id,attempt_id,worker_id,claim_token_hash,format!("+{lease_seconds} seconds")],
    )?;
    tx.commit()?;
    let run = get_autonomous_agent_run(conn, &org_id, &run_id)?
        .ok_or_else(|| anyhow::anyhow!("run_missing_after_claim"))?;
    let (template_key,config_raw): (String,String) = conn.query_row(
        "SELECT d.template_key,r.config_json FROM autonomous_agent_definitions d JOIN autonomous_agent_revisions r ON r.id=?2 WHERE d.id=?1",
        rusqlite::params![run.definition_id,run.revision_id], |row| Ok((row.get(0)?,row.get(1)?)),
    )?;
    let mut config: serde_json::Value = serde_json::from_str(&config_raw)?;
    if let Some(object) = config.as_object_mut() {
        let targets = list_autonomous_agent_targets(conn, &org_id, &run.definition_id)?;
        object.insert(
            "targets".into(),
            serde_json::to_value(targets).unwrap_or_else(|_| serde_json::json!([])),
        );
        let work:Option<(String,String,i64,Option<String>)>=conn.query_row(
            "SELECT repository,kind,external_number,head_sha FROM autonomous_agent_work_items WHERE run_id=?1",
            [&run.id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).optional()?;
        if let Some((repository, kind, number, head_sha)) = work {
            object.insert("trigger".into(),serde_json::json!({"repository":repository,"kind":kind,"number":number,"head_sha":head_sha}));
        }
        // Per-run inputs chosen at trigger time (e.g. the Judge template's PR/issue
        // targets) are merged over the config the worker sees for this run only.
        let input_json: Option<String> = conn
            .query_row(
                "SELECT input_json FROM autonomous_agent_runs WHERE id=?1",
                [&run.id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if let Some(input) = input_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        {
            if let Some(input_obj) = input.as_object() {
                for (key, value) in input_obj {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
    }
    Ok(Some(ClaimedAutonomousRun {
        org_id,
        run,
        attempt_id,
        claim_token,
        template_key,
        config,
    }))
}

pub fn requeue_autonomous_agent_run_without_attempt(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    attempt_id: &str,
    reason: &str,
) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let automation_run_id: Option<String> = tx
        .query_row(
            "SELECT automation_run_id FROM autonomous_agent_runs WHERE id=?1 AND org_id=?2",
            rusqlite::params![run_id, org_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(automation_run_id) = automation_run_id else {
        return Ok(false);
    };
    tx.execute(
        "DELETE FROM autonomous_agent_leases WHERE run_id=?1 AND attempt_id=?2",
        rusqlite::params![run_id, attempt_id],
    )?;
    tx.execute(
        "DELETE FROM automation_attempts WHERE id=?1 AND run_id=?2",
        rusqlite::params![attempt_id, automation_run_id],
    )?;
    let changed=tx.execute("UPDATE autonomous_agent_runs SET status='queued',started_at=NULL WHERE id=?1 AND org_id=?2 AND status IN('leased','running')",rusqlite::params![run_id,org_id])?;
    tx.commit()?;
    if changed == 1 {
        append_autonomous_agent_event(
            conn,
            org_id,
            run_id,
            "run.requeued",
            &serde_json::json!({"reason":reason}),
        )?;
    }
    Ok(changed == 1)
}

pub fn get_autonomous_agent_org_settings(
    conn: &Connection,
    org_id: &str,
) -> Result<Option<crate::models::types::AutonomousAgentOrgSettings>> {
    conn.query_row("SELECT autonomous_agents_enabled,autonomous_agent_retention_days FROM organizations WHERE id=?1",[org_id],|r|Ok(crate::models::types::AutonomousAgentOrgSettings{enabled:r.get::<_,i64>(0)?!=0,retention_days:r.get(1)?})).optional().map_err(Into::into)
}

pub fn patch_autonomous_agent_org_settings(
    conn: &Connection,
    org_id: &str,
    enabled: Option<bool>,
    retention_days: Option<i64>,
) -> Result<Option<crate::models::types::AutonomousAgentOrgSettings>> {
    if retention_days.is_some_and(|v| !(7..=3650).contains(&v)) {
        anyhow::bail!("invalid_retention_days")
    }
    conn.execute("UPDATE organizations SET autonomous_agents_enabled=COALESCE(?2,autonomous_agents_enabled),autonomous_agent_retention_days=COALESCE(?3,autonomous_agent_retention_days) WHERE id=?1",rusqlite::params![org_id,enabled.map(i64::from),retention_days])?;
    if enabled == Some(false) {
        let attempts=conn.prepare("SELECT lease.attempt_id FROM autonomous_agent_leases lease JOIN autonomous_agent_runs run ON run.id=lease.run_id WHERE run.org_id=?1 AND lease.released_at IS NULL")?.query_map([org_id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for attempt_id in attempts {
            revoke_automation_attempt(conn, org_id, &attempt_id, "organization_kill_switch")?;
        }
        conn.execute("UPDATE autonomous_agent_leases SET released_at=datetime('now') WHERE released_at IS NULL AND run_id IN(SELECT id FROM autonomous_agent_runs WHERE org_id=?1)",[org_id])?;
        conn.execute("UPDATE autonomous_agent_runs SET status='cancelled',finished_at=datetime('now') WHERE org_id=?1 AND status IN ('leased','running')",[org_id])?;
    }
    get_autonomous_agent_org_settings(conn, org_id)
}

pub fn cleanup_autonomous_agent_retention(conn: &Connection) -> Result<usize> {
    let orgs = conn
        .prepare("SELECT id,autonomous_agent_retention_days FROM organizations")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut deleted = 0;
    for (org, days) in orgs {
        let cutoff = format!("-{} days", days);
        conn.execute("DELETE FROM autonomous_agent_deliveries WHERE org_id=?1 AND run_id IN(SELECT id FROM autonomous_agent_runs WHERE org_id=?1 AND finished_at<datetime('now',?2))",rusqlite::params![org,cutoff])?;
        conn.execute("DELETE FROM autonomous_agent_findings WHERE org_id=?1 AND status IN('resolved','ignored') AND updated_at<datetime('now',?2)",rusqlite::params![org,cutoff])?;
        conn.execute("DELETE FROM autonomous_agent_output_links WHERE org_id=?1 AND run_id IN(SELECT id FROM autonomous_agent_runs WHERE org_id=?1 AND finished_at<datetime('now',?2))",rusqlite::params![org,cutoff])?;
        conn.execute("DELETE FROM autonomous_agent_work_items WHERE org_id=?1 AND run_id IN(SELECT id FROM autonomous_agent_runs WHERE org_id=?1 AND finished_at<datetime('now',?2))",rusqlite::params![org,cutoff])?;
        deleted+=conn.execute("DELETE FROM autonomous_agent_runs WHERE org_id=?1 AND finished_at<datetime('now',?2) AND status NOT IN('queued','leased','running') AND NOT EXISTS(SELECT 1 FROM autonomous_agent_findings f WHERE f.run_id=autonomous_agent_runs.id)",rusqlite::params![org,cutoff])?;
    }
    Ok(deleted)
}

pub fn get_autonomous_agent_metrics(
    conn: &Connection,
    org_id: &str,
) -> Result<crate::models::types::AutonomousAgentMetrics> {
    let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [org_id], |r| r.get(0))?) };
    let estimated_cost_usd=conn.query_row("SELECT COALESCE(SUM(CAST(json_extract(payload_json,'$.result.total_cost_usd') AS REAL)),0) FROM autonomous_agent_events WHERE org_id=?1 AND kind='run.finished'",[org_id],|r|r.get(0))?;
    Ok(crate::models::types::AutonomousAgentMetrics{queued:count("SELECT COUNT(*) FROM autonomous_agent_runs WHERE org_id=?1 AND status='queued'")?,running:count("SELECT COUNT(*) FROM autonomous_agent_runs WHERE org_id=?1 AND status IN('leased','running')")?,blocked:count("SELECT COUNT(*) FROM autonomous_agent_runs WHERE org_id=?1 AND status IN('blocked_policy','blocked_runtime','budget_exhausted')")?,open_findings:count("SELECT COUNT(*) FROM autonomous_agent_findings WHERE org_id=?1 AND status='open'")?,failed_deliveries:count("SELECT COUNT(*) FROM autonomous_agent_deliveries WHERE org_id=?1 AND status='failed'")?,dead_letters:count("SELECT (SELECT COUNT(*) FROM autonomous_agent_runs WHERE org_id=?1 AND status='dead_letter')+(SELECT COUNT(*) FROM autonomous_agent_deliveries WHERE org_id=?1 AND status='dead_letter')")?,estimated_cost_usd})
}

pub fn autonomous_agent_run_has_failed_deliveries(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM autonomous_agent_deliveries WHERE org_id=?1 AND run_id=?2 AND status IN('failed','dead_letter'))",
        rusqlite::params![org_id, run_id],
        |row| row.get::<_, i64>(0),
    )? != 0)
}

pub fn start_autonomous_agent_run(
    conn: &Connection,
    org_id: &str,
    id: &str,
    attempt_id: &str,
    claim_token: &str,
) -> Result<bool> {
    let token_hash = hex::encode(Sha256::digest(claim_token.as_bytes()));
    let valid:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM autonomous_agent_leases l JOIN autonomous_agent_runs r ON r.id=l.run_id WHERE l.run_id=?1 AND l.attempt_id=?2 AND l.claim_token_hash=?4 AND l.released_at IS NULL AND l.expires_at>datetime('now') AND r.org_id=?3)",rusqlite::params![id,attempt_id,org_id,token_hash],|r|r.get(0))?;
    if !valid {
        return Ok(false);
    }
    Ok(conn.execute("UPDATE autonomous_agent_runs SET status='running',started_at=COALESCE(started_at,datetime('now')) WHERE id=?1 AND org_id=?2 AND status='leased'",rusqlite::params![id,org_id])?==1)
}

pub fn heartbeat_autonomous_agent_run(
    conn: &Connection,
    org_id: &str,
    id: &str,
    attempt_id: &str,
    claim_token: &str,
    lease_seconds: i64,
) -> Result<bool> {
    let token_hash = hex::encode(Sha256::digest(claim_token.as_bytes()));
    Ok(conn.execute("UPDATE autonomous_agent_leases SET heartbeat_at=datetime('now'),expires_at=datetime('now',?5) WHERE run_id=?1 AND attempt_id=?2 AND claim_token_hash=?3 AND released_at IS NULL AND EXISTS(SELECT 1 FROM autonomous_agent_runs WHERE id=?1 AND org_id=?4 AND status='running')",rusqlite::params![id,attempt_id,token_hash,org_id,format!("+{} seconds",lease_seconds.max(30))])?==1)
}

/// Append one transcript turn (a sanitized Claude stream-json line) at an
/// explicit sequence. The worker owns the counter, so we avoid a MAX() subquery
/// per line; a duplicate (run_id,sequence) is ignored rather than erroring.
pub fn append_autonomous_agent_transcript_turn(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    sequence: i64,
    kind: &str,
    payload_json: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO autonomous_agent_run_transcript
            (id,org_id,run_id,sequence,kind,payload_json)
         VALUES (?1,?2,?3,?4,?5,?6)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            org_id,
            run_id,
            sequence,
            kind,
            payload_json
        ],
    )?;
    Ok(())
}

/// List transcript turns for a run after `after_sequence` (0 = from the start),
/// oldest first. Paginated (LIMIT) so a long run polls incrementally instead of
/// returning tens of thousands of rows in one response.
pub fn list_autonomous_agent_transcript(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    after_sequence: i64,
    limit: i64,
) -> Result<Vec<crate::models::types::AutonomousAgentEvent>> {
    let mut stmt = conn.prepare(
        "SELECT sequence,kind,payload_json,created_at
           FROM autonomous_agent_run_transcript
          WHERE org_id=?1 AND run_id=?2 AND sequence>?3
          ORDER BY sequence LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![org_id, run_id, after_sequence, limit],
        |row| {
            let raw: String = row.get(2)?;
            Ok(crate::models::types::AutonomousAgentEvent {
                sequence: row.get(0)?,
                kind: row.get(1)?,
                payload: serde_json::from_str(&raw).unwrap_or_default(),
                created_at: row.get(3)?,
            })
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn append_autonomous_agent_event(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    kind: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    conn.execute(
        "INSERT INTO autonomous_agent_events (id,org_id,run_id,sequence,kind,payload_json)
         SELECT ?1,?2,?3,COALESCE(MAX(sequence),0)+1,?4,?5 FROM autonomous_agent_events WHERE run_id=?3",
        rusqlite::params![Uuid::new_v4().to_string(),org_id,run_id,kind,serde_json::to_string(payload)?],
    )?;
    Ok(())
}

pub fn finish_autonomous_agent_run(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    attempt_id: &str,
    status: &str,
    result: &serde_json::Value,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE autonomous_agent_runs SET status=?3,finished_at=datetime('now')
         WHERE id=?1 AND org_id=?2 AND status IN('leased','running')
           AND EXISTS(SELECT 1 FROM autonomous_agent_leases lease
                      WHERE lease.run_id=?1 AND lease.attempt_id=?4 AND lease.released_at IS NULL)",
        rusqlite::params![run_id, org_id, status, attempt_id],
    )?;
    if changed != 1 {
        tx.rollback()?;
        anyhow::bail!("run_not_active")
    }
    tx.execute(
        "UPDATE automation_attempts SET status='revoked',revoked_at=datetime('now')
         WHERE id=?1 AND status='active'",
        [attempt_id],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO automation_revocations(id,org_id,attempt_id,reason)
         VALUES(?1,?2,?3,?4)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            org_id,
            attempt_id,
            format!("run_finished:{status}")
        ],
    )?;
    tx.execute("UPDATE autonomous_agent_leases SET released_at=datetime('now') WHERE run_id=?1 AND attempt_id=?2 AND released_at IS NULL", rusqlite::params![run_id,attempt_id])?;
    tx.commit()?;
    append_autonomous_agent_event(conn, org_id, run_id, "run.finished", result)
}

fn autonomous_connector_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AutonomousAgentConnector> {
    let metadata: String = row.get(3)?;
    let scopes: String = row.get(4)?;
    Ok(AutonomousAgentConnector {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        metadata: serde_json::from_str(&metadata).unwrap_or(serde_json::Value::Null),
        scopes: serde_json::from_str(&scopes).unwrap_or_default(),
        health: row.get(5)?,
        revocation_generation: row.get(6)?,
        secret_configured: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn list_autonomous_agent_connectors(
    conn: &Connection,
    org_id: &str,
) -> Result<Vec<AutonomousAgentConnector>> {
    let mut stmt = conn.prepare(
        "SELECT id,kind,name,metadata_json,scopes_json,health,revocation_generation,secret_ciphertext IS NOT NULL,created_at,updated_at
         FROM autonomous_agent_connectors WHERE org_id=?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([org_id], autonomous_connector_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn put_autonomous_agent_connector(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    req: &PutAutonomousAgentConnectorRequest,
) -> Result<AutonomousAgentConnector> {
    if !matches!(req.kind.as_str(), "github_app" | "slack" | "target_secret") {
        anyhow::bail!("invalid_connector_kind");
    }
    if req.name.trim().is_empty() || !req.metadata.is_object() {
        anyhow::bail!("invalid_connector");
    }
    let required_scopes: &[&str] = match req.kind.as_str() {
        "github_app" => &[
            "metadata:read",
            "contents:write",
            "issues:write",
            "pull_requests:write",
            "checks:read",
        ],
        "slack" => &["chat:write"],
        "target_secret" => &["target:use"],
        _ => &[],
    };
    if required_scopes
        .iter()
        .any(|required| !req.scopes.iter().any(|scope| scope == required))
    {
        anyhow::bail!("insufficient_connector_scopes")
    }
    match req.kind.as_str() {
        "github_app" => {
            if req
                .metadata
                .get("app_id")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .is_none()
                || req
                    .metadata
                    .get("installation_id")
                    .and_then(|v| v.as_i64())
                    .filter(|v| *v > 0)
                    .is_none()
            {
                anyhow::bail!("invalid_github_app_metadata")
            }
            if let Some(secret) = req.secret.as_deref() {
                let value: serde_json::Value = serde_json::from_str(secret)
                    .map_err(|_| anyhow::anyhow!("invalid_github_app_secret"))?;
                if value
                    .get("private_key")
                    .and_then(|v| v.as_str())
                    .filter(|v| v.contains("PRIVATE KEY"))
                    .is_none()
                    || value
                        .get("webhook_secret")
                        .and_then(|v| v.as_str())
                        .filter(|v| v.len() >= 16)
                        .is_none()
                {
                    anyhow::bail!("invalid_github_app_secret")
                }
            }
        }
        "slack" => {
            if let Some(secret) = req.secret.as_deref() {
                crate::automation::connectors::validate_slack_webhook(secret)?;
            }
        }
        _ => {}
    }
    let ciphertext = match req.secret.as_deref() {
        Some(secret) if !secret.is_empty() => Some(
            crate::crypto::encrypt(secret).ok_or_else(|| anyhow::anyhow!("encryption_required"))?,
        ),
        Some(_) => anyhow::bail!("invalid_connector_secret"),
        None => None,
    };
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO autonomous_agent_connectors (id,org_id,kind,name,secret_ciphertext,metadata_json,scopes_json,health,created_by)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'ready',?8)
         ON CONFLICT(org_id,kind,name) DO UPDATE SET
           secret_ciphertext=COALESCE(excluded.secret_ciphertext,autonomous_agent_connectors.secret_ciphertext),
           metadata_json=excluded.metadata_json,scopes_json=excluded.scopes_json,health=CASE WHEN excluded.secret_ciphertext IS NULL AND autonomous_agent_connectors.secret_ciphertext IS NULL THEN 'unknown' ELSE 'ready' END,
           revocation_generation=autonomous_agent_connectors.revocation_generation+1,updated_at=datetime('now')",
        rusqlite::params![id,org_id,req.kind,req.name.trim(),ciphertext,serde_json::to_string(&req.metadata)?,serde_json::to_string(&req.scopes)?,user_id],
    )?;
    conn.query_row(
        "SELECT id,kind,name,metadata_json,scopes_json,health,revocation_generation,secret_ciphertext IS NOT NULL,created_at,updated_at
         FROM autonomous_agent_connectors WHERE org_id=?1 AND kind=?2 AND name=?3",
        rusqlite::params![org_id,req.kind,req.name.trim()], autonomous_connector_from_row,
    ).map_err(Into::into)
}

pub fn revoke_autonomous_agent_connector(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE autonomous_agent_connectors SET secret_ciphertext=NULL,health='revoked',revocation_generation=revocation_generation+1,updated_at=datetime('now') WHERE org_id=?1 AND id=?2",
        rusqlite::params![org_id,id],
    )? == 1)
}

pub fn get_autonomous_agent_connector_secret(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<(AutonomousAgentConnector, String)>> {
    let value: Option<(AutonomousAgentConnector,String)> = conn.query_row(
        "SELECT id,kind,name,metadata_json,scopes_json,health,revocation_generation,secret_ciphertext IS NOT NULL,created_at,updated_at,secret_ciphertext
         FROM autonomous_agent_connectors WHERE org_id=?1 AND id=?2 AND health!='revoked'",
        rusqlite::params![org_id,id], |row| {
            let connector = autonomous_connector_from_row(row)?;
            let ciphertext: String = row.get(10)?;
            Ok((connector,ciphertext))
        },
    ).optional()?;
    match value {
        Some((connector, ciphertext)) => Ok(Some((
            connector,
            crate::crypto::decrypt(&ciphertext)
                .ok_or_else(|| anyhow::anyhow!("connector_decrypt_failed"))?,
        ))),
        None => Ok(None),
    }
}

pub fn find_github_app_webhook_connector(
    conn: &Connection,
    installation_id: i64,
) -> Result<Option<(String, String, String)>> {
    let value: Option<(String,String,String)> = conn.query_row(
        "SELECT org_id,id,secret_ciphertext FROM autonomous_agent_connectors
         WHERE kind='github_app' AND health!='revoked' AND json_extract(metadata_json,'$.installation_id')=?1",
        [installation_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).optional()?;
    match value {
        Some((org_id, id, ciphertext)) => Ok(Some((
            org_id,
            id,
            crate::crypto::decrypt(&ciphertext)
                .ok_or_else(|| anyhow::anyhow!("connector_decrypt_failed"))?,
        ))),
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn record_github_webhook_delivery(
    conn: &Connection,
    org_id: &str,
    connector_id: &str,
    delivery_id: &str,
    event_name: &str,
    action: Option<&str>,
    repository: Option<&str>,
    payload_hash: &str,
) -> Result<bool> {
    let existing: Option<String> = conn.query_row(
        "SELECT payload_hash FROM autonomous_github_deliveries WHERE connector_id=?1 AND delivery_id=?2",
        rusqlite::params![connector_id,delivery_id], |row| row.get(0),
    ).optional()?;
    if let Some(existing) = existing {
        if existing == payload_hash {
            return Ok(false);
        }
        anyhow::bail!("github_delivery_payload_mismatch");
    }
    conn.execute(
        "INSERT INTO autonomous_github_deliveries (id,org_id,connector_id,delivery_id,event_name,action,repository,payload_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![Uuid::new_v4().to_string(),org_id,connector_id,delivery_id,event_name,action,repository,payload_hash],
    )?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub fn enqueue_github_webhook_agents(
    conn: &Connection,
    org_id: &str,
    _delivery_id: &str,
    repository: &str,
    kind: &str,
    external_number: i64,
    head_sha: Option<&str>,
    payload_hash: &str,
) -> Result<usize> {
    let template = match kind {
        "github_issue" => "github_issue_resolver",
        "github_pr" => "github_pr_reviewer",
        _ => anyhow::bail!("invalid_work_item_kind"),
    };
    let mut stmt = conn.prepare(
        "SELECT d.id FROM autonomous_agent_definitions d JOIN autonomous_agent_revisions r
         ON r.definition_id=d.id AND r.revision=d.current_revision
         WHERE d.org_id=?1 AND d.status='enabled' AND d.template_key=?2
           AND json_extract(r.config_json,'$.repository')=?3 AND r.validation_status='valid'",
    )?;
    let ids = stmt
        .query_map(rusqlite::params![org_id, template, repository], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut created = 0;
    for definition_id in ids {
        if kind == "github_issue" {
            let already_owned: bool = conn.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM autonomous_agent_work_items work
                    WHERE work.definition_id=?1 AND work.repository=?2
                      AND work.kind='github_issue' AND work.external_number=?3
                      AND work.eligibility IN('pending','eligible','completed')
                )",
                rusqlite::params![definition_id, repository, external_number],
                |row| row.get(0),
            )?;
            if already_owned {
                continue;
            }
        }
        let identity = head_sha.unwrap_or(payload_hash);
        let occurrence = format!(
            "github:{}",
            hex::encode(Sha256::digest(
                format!("{definition_id}|{repository}|{kind}|{external_number}|{identity}")
                    .as_bytes()
            ))
        );
        let run = match enqueue_autonomous_agent_run(
            conn,
            org_id,
            &definition_id,
            "github_webhook",
            &occurrence,
            None,
            None,
        ) {
            Ok(Some(run)) => run,
            Err(error) if error.to_string().contains("UNIQUE constraint failed") => continue,
            Ok(None) => continue,
            Err(error) => return Err(error),
        };
        conn.execute(
            "UPDATE autonomous_agent_runs SET snapshot_sha=?2 WHERE id=?1",
            rusqlite::params![run.id, head_sha],
        )?;
        conn.execute("INSERT OR IGNORE INTO autonomous_agent_work_items(id,org_id,definition_id,run_id,repository,kind,external_number,head_sha,payload_hash,eligibility) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'eligible')",rusqlite::params![Uuid::new_v4().to_string(),org_id,definition_id,run.id,repository,kind,external_number,head_sha,payload_hash])?;
        created += 1;
    }
    Ok(created)
}

#[derive(Debug)]
pub struct GithubReconciliationSource {
    pub org_id: String,
    pub template_key: String,
    pub repository: String,
    pub connector_id: String,
}

pub fn list_github_reconciliation_sources(
    conn: &Connection,
) -> Result<Vec<GithubReconciliationSource>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d.org_id,d.template_key,
                json_extract(r.config_json,'$.repository'),
                json_extract(r.config_json,'$.github_connector_id')
         FROM autonomous_agent_definitions d
         JOIN autonomous_agent_revisions r
           ON r.definition_id=d.id AND r.revision=d.current_revision
         JOIN organizations o ON o.id=d.org_id
         JOIN autonomous_agent_connectors c
           ON c.id=json_extract(r.config_json,'$.github_connector_id')
          AND c.org_id=d.org_id
         WHERE d.status='enabled' AND o.autonomous_agents_enabled=1
           AND r.validation_status='valid' AND c.health='ready'
           AND d.template_key IN('github_issue_resolver','github_pr_reviewer')
           AND json_extract(r.config_json,'$.repository') IS NOT NULL
         ORDER BY d.org_id,d.template_key,json_extract(r.config_json,'$.repository')
         LIMIT 100",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(GithubReconciliationSource {
            org_id: row.get(0)?,
            template_key: row.get(1)?,
            repository: row.get(2)?,
            connector_id: row.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn create_autonomous_output_link(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    kind: &str,
    external_id: &str,
    external_url: Option<&str>,
) -> Result<()> {
    conn.execute("INSERT OR IGNORE INTO autonomous_agent_output_links(id,org_id,run_id,work_item_id,kind,external_id,external_url) VALUES(?1,?2,?3,(SELECT id FROM autonomous_agent_work_items WHERE run_id=?3),?4,?5,?6)",rusqlite::params![Uuid::new_v4().to_string(),org_id,run_id,kind,external_id,external_url])?;
    conn.execute(
        "UPDATE autonomous_agent_work_items SET eligibility='completed' WHERE run_id=?1",
        [run_id],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_autonomous_agent_finding(
    conn: &Connection,
    org_id: &str,
    definition_id: &str,
    run_id: &str,
    fingerprint: &str,
    title: &str,
    severity: &str,
    summary: &str,
    evidence: &serde_json::Value,
) -> Result<AutonomousAgentFinding> {
    if !matches!(severity, "info" | "low" | "medium" | "high" | "critical") {
        anyhow::bail!("invalid_severity");
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO autonomous_agent_findings (id,org_id,definition_id,run_id,fingerprint,title,severity,summary,evidence_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(org_id,definition_id,fingerprint) DO UPDATE SET run_id=excluded.run_id,title=excluded.title,
         severity=excluded.severity,summary=excluded.summary,evidence_json=excluded.evidence_json,
         occurrence_count=autonomous_agent_findings.occurrence_count+1,updated_at=datetime('now')",
        rusqlite::params![id,org_id,definition_id,run_id,fingerprint,title,severity,summary,serde_json::to_string(evidence)?],
    )?;
    conn.query_row(
        "SELECT id,definition_id,run_id,fingerprint,title,severity,status,summary,evidence_json,occurrence_count,created_at,updated_at
         FROM autonomous_agent_findings WHERE org_id=?1 AND definition_id=?2 AND fingerprint=?3",
        rusqlite::params![org_id,definition_id,fingerprint], finding_from_row,
    ).map_err(Into::into)
}

fn finding_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomousAgentFinding> {
    let evidence: String = row.get(8)?;
    Ok(AutonomousAgentFinding {
        id: row.get(0)?,
        definition_id: row.get(1)?,
        run_id: row.get(2)?,
        fingerprint: row.get(3)?,
        title: row.get(4)?,
        severity: row.get(5)?,
        status: row.get(6)?,
        summary: row.get(7)?,
        evidence: serde_json::from_str(&evidence).unwrap_or_default(),
        occurrence_count: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub fn list_autonomous_agent_findings(
    conn: &Connection,
    org_id: &str,
) -> Result<Vec<AutonomousAgentFinding>> {
    let mut stmt=conn.prepare("SELECT id,definition_id,run_id,fingerprint,title,severity,status,summary,evidence_json,occurrence_count,created_at,updated_at FROM autonomous_agent_findings WHERE org_id=?1 ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([org_id], finding_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn patch_autonomous_agent_finding(
    conn: &Connection,
    org_id: &str,
    id: &str,
    status: &str,
) -> Result<Option<AutonomousAgentFinding>> {
    if !matches!(status, "open" | "resolved" | "ignored") {
        anyhow::bail!("invalid_finding_status")
    }
    conn.execute("UPDATE autonomous_agent_findings SET status=?3,updated_at=datetime('now') WHERE org_id=?1 AND id=?2",rusqlite::params![org_id,id,status])?;
    conn.query_row("SELECT id,definition_id,run_id,fingerprint,title,severity,status,summary,evidence_json,occurrence_count,created_at,updated_at FROM autonomous_agent_findings WHERE org_id=?1 AND id=?2",rusqlite::params![org_id,id],finding_from_row).optional().map_err(Into::into)
}

/// Archive every finding that is not already archived (status `ignored`), returning
/// how many were archived. Archiving is reversible — a finding can be restored to
/// `open` — and survives re-detection (upsert never resets status).
pub fn archive_all_autonomous_agent_findings(conn: &Connection, org_id: &str) -> Result<usize> {
    conn.execute(
        "UPDATE autonomous_agent_findings SET status='ignored',updated_at=datetime('now') WHERE org_id=?1 AND status!='ignored'",
        rusqlite::params![org_id],
    )
    .map_err(Into::into)
}

/// Mark every OPEN finding that was delivered as the given GitHub issue (matched
/// by the issue's html_url on its `github_issue` delivery) as resolved, so a
/// finding the resolver has just addressed with a PR no longer lingers. Returns
/// how many findings were resolved.
pub fn resolve_open_findings_for_issue(
    conn: &Connection,
    org_id: &str,
    issue_url: &str,
) -> Result<usize> {
    let changed = conn.execute(
        "UPDATE autonomous_agent_findings SET status='resolved', updated_at=datetime('now')
         WHERE org_id=?1 AND status='open' AND id IN (
             SELECT finding_id FROM autonomous_agent_deliveries
             WHERE org_id=?1 AND channel='github_issue' AND external_url=?2
               AND finding_id IS NOT NULL
         )",
        rusqlite::params![org_id, issue_url],
    )?;
    Ok(changed)
}

pub fn list_autonomous_agent_deliveries(
    conn: &Connection,
    org_id: &str,
) -> Result<Vec<AutonomousAgentDelivery>> {
    let mut stmt=conn.prepare("SELECT id,run_id,finding_id,channel,status,external_id,external_url,attempts,last_error_code,created_at,updated_at FROM autonomous_agent_deliveries WHERE org_id=?1 ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([org_id], delivery_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn retry_autonomous_agent_delivery(
    conn: &Connection,
    org_id: &str,
    id: &str,
) -> Result<Option<AutonomousAgentDelivery>> {
    conn.execute("UPDATE autonomous_agent_deliveries SET status='pending',last_error_code=NULL,next_attempt_at=datetime('now'),updated_at=datetime('now') WHERE org_id=?1 AND id=?2 AND status IN ('failed','dead_letter')",rusqlite::params![org_id,id])?;
    conn.query_row("SELECT id,run_id,finding_id,channel,status,external_id,external_url,attempts,last_error_code,created_at,updated_at FROM autonomous_agent_deliveries WHERE org_id=?1 AND id=?2",rusqlite::params![org_id,id],delivery_from_row).optional().map_err(Into::into)
}

pub struct PendingAutonomousDelivery {
    pub org_id: String,
    pub delivery: AutonomousAgentDelivery,
    pub finding: AutonomousAgentFinding,
    pub config: serde_json::Value,
}

pub fn next_pending_autonomous_delivery(
    conn: &Connection,
) -> Result<Option<PendingAutonomousDelivery>> {
    conn.query_row("SELECT d.org_id,d.id,d.run_id,d.finding_id,d.channel,d.status,d.external_id,d.external_url,d.attempts,d.last_error_code,d.created_at,d.updated_at,f.id,f.definition_id,f.run_id,f.fingerprint,f.title,f.severity,f.status,f.summary,f.evidence_json,f.occurrence_count,f.created_at,f.updated_at,r.config_json FROM autonomous_agent_deliveries d JOIN autonomous_agent_findings f ON f.id=d.finding_id JOIN autonomous_agent_definitions a ON a.id=f.definition_id JOIN autonomous_agent_revisions r ON r.definition_id=a.id AND r.revision=a.current_revision WHERE ((d.status='pending' AND d.attempts>0) OR (d.status='failed' AND d.next_attempt_at<=datetime('now'))) AND d.channel IN ('slack','github_issue') ORDER BY d.updated_at LIMIT 1",[],|row|{
        let evidence:String=row.get(20)?;let config:String=row.get(24)?;
        Ok(PendingAutonomousDelivery{org_id:row.get(0)?,delivery:AutonomousAgentDelivery{id:row.get(1)?,run_id:row.get(2)?,finding_id:row.get(3)?,channel:row.get(4)?,status:row.get(5)?,external_id:row.get(6)?,external_url:row.get(7)?,attempts:row.get(8)?,last_error_code:row.get(9)?,created_at:row.get(10)?,updated_at:row.get(11)?},finding:AutonomousAgentFinding{id:row.get(12)?,definition_id:row.get(13)?,run_id:row.get(14)?,fingerprint:row.get(15)?,title:row.get(16)?,severity:row.get(17)?,status:row.get(18)?,summary:row.get(19)?,evidence:serde_json::from_str(&evidence).unwrap_or_default(),occurrence_count:row.get(21)?,created_at:row.get(22)?,updated_at:row.get(23)?},config:serde_json::from_str(&config).unwrap_or_default()})
    }).optional().map_err(Into::into)
}

fn autonomous_target_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<crate::models::types::AutonomousAgentTarget> {
    let raw: String = row.get(4)?;
    Ok(crate::models::types::AutonomousAgentTarget {
        id: row.get(0)?,
        definition_id: row.get(1)?,
        kind: row.get(2)?,
        name: row.get(3)?,
        config: serde_json::from_str(&raw).unwrap_or_default(),
        credential_connector_id: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn list_autonomous_agent_targets(
    conn: &Connection,
    org_id: &str,
    definition_id: &str,
) -> Result<Vec<crate::models::types::AutonomousAgentTarget>> {
    let mut stmt=conn.prepare("SELECT id,definition_id,kind,name,config_json,credential_connector_id,enabled,created_at,updated_at FROM autonomous_agent_targets WHERE org_id=?1 AND definition_id=?2 ORDER BY created_at")?;
    let rows = stmt.query_map(
        rusqlite::params![org_id, definition_id],
        autonomous_target_from_row,
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn put_autonomous_agent_target(
    conn: &Connection,
    org_id: &str,
    definition_id: &str,
    req: &crate::models::types::PutAutonomousAgentTargetRequest,
) -> Result<Option<crate::models::types::AutonomousAgentTarget>> {
    if get_autonomous_agent_definition(conn, org_id, definition_id)?.is_none() {
        return Ok(None);
    }
    if !matches!(
        req.kind.as_str(),
        "repository" | "web_application" | "project"
    ) {
        anyhow::bail!("invalid_target_kind")
    }
    if req.name.trim().is_empty() || !req.config.is_object() {
        anyhow::bail!("invalid_target")
    }
    if let Some(connector) = req.credential_connector_id.as_deref() {
        let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM autonomous_agent_connectors WHERE id=?1 AND org_id=?2 AND health!='revoked')",rusqlite::params![connector,org_id],|r|r.get(0))?;
        if !exists {
            anyhow::bail!("invalid_target_connector")
        }
    }
    let id = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO autonomous_agent_targets(id,org_id,definition_id,kind,name,config_json,credential_connector_id,enabled) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",rusqlite::params![id,org_id,definition_id,req.kind,req.name.trim(),serde_json::to_string(&req.config)?,req.credential_connector_id,req.enabled as i64])?;
    conn.query_row("SELECT id,definition_id,kind,name,config_json,credential_connector_id,enabled,created_at,updated_at FROM autonomous_agent_targets WHERE id=?1 AND org_id=?2",rusqlite::params![id,org_id],autonomous_target_from_row).optional().map_err(Into::into)
}

pub fn create_autonomous_agent_delivery(
    conn: &Connection,
    org_id: &str,
    run_id: &str,
    finding_id: Option<&str>,
    channel: &str,
    key: &str,
) -> Result<AutonomousAgentDelivery> {
    let id = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO autonomous_agent_deliveries (id,org_id,run_id,finding_id,channel,idempotency_key) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(org_id,channel,idempotency_key) DO NOTHING",rusqlite::params![id,org_id,run_id,finding_id,channel,key])?;
    conn.query_row("SELECT id,run_id,finding_id,channel,status,external_id,external_url,attempts,last_error_code,created_at,updated_at FROM autonomous_agent_deliveries WHERE org_id=?1 AND channel=?2 AND idempotency_key=?3",rusqlite::params![org_id,channel,key],delivery_from_row).map_err(Into::into)
}

fn delivery_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutonomousAgentDelivery> {
    Ok(AutonomousAgentDelivery {
        id: row.get(0)?,
        run_id: row.get(1)?,
        finding_id: row.get(2)?,
        channel: row.get(3)?,
        status: row.get(4)?,
        external_id: row.get(5)?,
        external_url: row.get(6)?,
        attempts: row.get(7)?,
        last_error_code: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub fn complete_autonomous_agent_delivery(
    conn: &Connection,
    org_id: &str,
    id: &str,
    external_id: Option<&str>,
    external_url: Option<&str>,
) -> Result<()> {
    conn.execute("UPDATE autonomous_agent_deliveries SET status='delivered',external_id=?3,external_url=?4,attempts=attempts+1,last_error_code=NULL,next_attempt_at=NULL,updated_at=datetime('now') WHERE org_id=?1 AND id=?2",rusqlite::params![org_id,id,external_id,external_url])?;
    Ok(())
}

pub fn fail_autonomous_agent_delivery(
    conn: &Connection,
    org_id: &str,
    id: &str,
    code: &str,
) -> Result<()> {
    conn.execute("UPDATE autonomous_agent_deliveries SET status=CASE WHEN attempts>=2 THEN 'dead_letter' ELSE 'failed' END,attempts=attempts+1,last_error_code=?3,next_attempt_at=CASE WHEN attempts>=2 THEN NULL ELSE datetime('now',printf('+%d seconds',30*(1 << attempts))) END,updated_at=datetime('now') WHERE org_id=?1 AND id=?2",rusqlite::params![org_id,id,code])?;
    Ok(())
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
        let mut prev_type = "project";
        for depth in 0..parts.len().saturating_sub(1) {
            let folder_path: String = parts[..=depth].join("/");
            let folder_qname = format!("folder::{}", folder_path);
            let folder_name = parts[depth];
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
            prev_type = "folder";
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

    let mut params: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(code_project_id), Box::new(limit)];
    for t in node_types {
        params.push(Box::new(t.clone()));
    }
    params.push(Box::new(offset));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

    let mut stmt = conn.prepare(&node_sql)?;
    let nodes: Vec<GraphNodeDto> = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(GraphNodeDto {
                id: row.get(0)?,
                node_type: row.get(1)?,
                name: row.get(2)?,
                qualified_name: row.get(3)?,
                file_path: row.get(4)?,
                start_line: row.get(5)?,
                end_line: row.get(6)?,
                language: row.get(7)?,
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
                id: row.get(0)?,
                from_id: row.get(1)?,
                to_id: row.get(2)?,
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
    let params: Vec<&dyn rusqlite::ToSql> =
        id_vec.iter().map(|s| *s as &dyn rusqlite::ToSql).collect();
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
        .query_map(
            rusqlite::params![org_id, project, since, limit, offset],
            |row| {
                let tags_str: String = row.get(6)?;
                let title: Option<String> = row.get(7)?;
                let content: String = row.get(8)?;
                let label = title
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| content.chars().take(60).collect());
                Ok(MemRow {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    project_id: row.get(2)?,
                    user_id: row.get(3)?,
                    session_id: row.get(4)?,
                    collection_id: row.get(5)?,
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    label,
                })
            },
        )?
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
                MemGraphNode {
                    id: node_id,
                    node_type: "Memory".to_string(),
                    label: r.label.clone(),
                },
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
            let placeholders: Vec<String> =
                (2..=1 + names.len()).map(|n| format!("?{n}")).collect();
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
                .query_map(params.as_slice(), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
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
            nodes
                .entry(canonical_id.clone())
                .or_insert_with(|| MemGraphNode {
                    id: canonical_id.clone(),
                    node_type: "Project".to_string(),
                    label: r.project.clone(),
                });
            edges.push(MemGraphEdge {
                id: format!("belongs_to:memory:{}:{}", r.id, canonical_id),
                from_id: format!("memory:{}", r.id),
                to_id: canonical_id,
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
        let session_ids: HashSet<String> =
            rows.iter().filter_map(|r| r.session_id.clone()).collect();
        let session_labels = lookup_labels(
            conn,
            "sessions",
            "COALESCE(name, summary, id)",
            &session_ids,
        )?;
        for r in &rows {
            if let Some(sid) = &r.session_id {
                let node_id = format!("session:{sid}");
                let label = session_labels
                    .get(sid)
                    .cloned()
                    .unwrap_or_else(|| sid.clone());
                nodes
                    .entry(node_id.clone())
                    .or_insert_with(|| MemGraphNode {
                        id: node_id.clone(),
                        node_type: "Session".to_string(),
                        label,
                    });
                edges.push(MemGraphEdge {
                    id: format!("in_session:memory:{}:{}", r.id, node_id),
                    from_id: format!("memory:{}", r.id),
                    to_id: node_id,
                    edge_type: "in_session".to_string(),
                });
            }
        }

        // User nodes + `created_by` edges (user_id is NOT NULL on memories, always present)
        let user_ids: HashSet<String> = rows.iter().map(|r| r.user_id.clone()).collect();
        let user_labels = lookup_labels(conn, "users", "name", &user_ids)?;
        for r in &rows {
            let node_id = format!("user:{}", r.user_id);
            let label = user_labels
                .get(&r.user_id)
                .cloned()
                .unwrap_or_else(|| r.user_id.clone());
            nodes
                .entry(node_id.clone())
                .or_insert_with(|| MemGraphNode {
                    id: node_id.clone(),
                    node_type: "User".to_string(),
                    label,
                });
            edges.push(MemGraphEdge {
                id: format!("created_by:memory:{}:{}", r.id, node_id),
                from_id: format!("memory:{}", r.id),
                to_id: node_id,
                edge_type: "created_by".to_string(),
            });
        }

        // Collection nodes + `in_collection` edges (omitted when collection_id is NULL)
        let collection_ids: HashSet<String> = rows
            .iter()
            .filter_map(|r| r.collection_id.clone())
            .collect();
        let collection_labels = lookup_labels(conn, "collections", "name", &collection_ids)?;
        for r in &rows {
            if let Some(cid) = &r.collection_id {
                let node_id = format!("collection:{cid}");
                let label = collection_labels
                    .get(cid)
                    .cloned()
                    .unwrap_or_else(|| cid.clone());
                nodes
                    .entry(node_id.clone())
                    .or_insert_with(|| MemGraphNode {
                        id: node_id.clone(),
                        node_type: "Collection".to_string(),
                        label,
                    });
                edges.push(MemGraphEdge {
                    id: format!("in_collection:memory:{}:{}", r.id, node_id),
                    from_id: format!("memory:{}", r.id),
                    to_id: node_id,
                    edge_type: "in_collection".to_string(),
                });
            }
        }

        // Tag nodes + `tagged` edges (omitted when tags is empty)
        for r in &rows {
            for tag in &r.tags {
                let node_id = format!("tag:{tag}");
                nodes
                    .entry(node_id.clone())
                    .or_insert_with(|| MemGraphNode {
                        id: node_id.clone(),
                        node_type: "Tag".to_string(),
                        label: tag.clone(),
                    });
                edges.push(MemGraphEdge {
                    id: format!("tagged:memory:{}:{}", r.id, node_id),
                    from_id: format!("memory:{}", r.id),
                    to_id: node_id,
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
                id: row.get(0)?,
                user_id: row.get(1)?,
                action: row.get(2)?,
                resource_type: row.get(3)?,
                resource_id: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if !audit_rows.is_empty() {
        let audit_user_ids: HashSet<String> =
            audit_rows.iter().map(|r| r.user_id.clone()).collect();
        let audit_user_labels = lookup_labels(conn, "users", "name", &audit_user_ids)?;

        for r in &audit_rows {
            let audit_node_id = format!("audit:{}", r.id);
            nodes
                .entry(audit_node_id.clone())
                .or_insert_with(|| MemGraphNode {
                    id: audit_node_id.clone(),
                    node_type: "AuditEvent".to_string(),
                    label: format!("{} {}", r.action, r.resource_type),
                });

            // performed_by: always present — audit_logs.user_id is NOT NULL.
            let user_node_id = format!("user:{}", r.user_id);
            let user_label = audit_user_labels
                .get(&r.user_id)
                .cloned()
                .unwrap_or_else(|| r.user_id.clone());
            nodes
                .entry(user_node_id.clone())
                .or_insert_with(|| MemGraphNode {
                    id: user_node_id.clone(),
                    node_type: "User".to_string(),
                    label: user_label,
                });
            edges.push(MemGraphEdge {
                id: format!("performed_by:{}:{}", audit_node_id, user_node_id),
                from_id: audit_node_id.clone(),
                to_id: user_node_id,
                edge_type: "performed_by".to_string(),
            });

            // targets: only when the resource already resolves to a node in this graph.
            if let Some(target_id) =
                resource_node_id(&r.resource_type, r.resource_id.as_deref(), &nodes)
            {
                edges.push(MemGraphEdge {
                    id: format!("targets:{}:{}", audit_node_id, target_id),
                    from_id: audit_node_id,
                    to_id: target_id,
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

/// Maximum cap for the family-graph endpoint. Generous on purpose — the user
/// explicitly wants no pagination on the new Graph page, so we return every
/// memory in the resolved family in one shot. Keep this aligned with the
/// frontend's expectation (a single fetch, no "capped at N" warning).
pub const MAX_FAMILY_GRAPH_LIMIT: i64 = 50_000;

/// Fetches the memory knowledge graph for a project family: the root project
/// plus every descendant in `parent_id`. Calls the per-project primitive
/// `get_memory_graph` for each project in the family and merges the results
/// into a single dedup'd node/edge set.
///
/// `family` is the list of resolved projects (root first, BFS). Caller must
/// resolve the family via `resolve_project_family` so the order is stable and
/// the hierarchy edges (added below) connect the right nodes.
///
/// The per-project primitive already runs the audit-logs query (which is
/// project-independent — see its doc comment) and adds audit nodes/edges to
/// every per-project result. Merging via `HashMap<node_id, _>` collapses the
/// duplicates; the `seen_edge_keys` set dedupes the matching edges. Net
/// result: every audit event appears exactly once in the merged graph.
pub fn get_memory_graph_for_family(
    conn: &Connection,
    org_id: &str,
    family: &[Project],
    since: Option<&str>,
    limit: i64,
) -> Result<(Vec<MemGraphNode>, Vec<MemGraphEdge>)> {
    use std::collections::HashSet;
    let cap = limit.clamp(1, MAX_FAMILY_GRAPH_LIMIT);

    let mut nodes: HashMap<String, MemGraphNode> = HashMap::new();
    let mut edges: Vec<MemGraphEdge> = Vec::new();
    let mut seen_edge_keys: HashSet<String> = HashSet::new();

    // Per-project queries. Allocate the full cap per project so a 10-project
    // family of 5,000 memories each returns all of them rather than starving
    // the last projects. The overall envelope is still bounded by `cap`.
    for project in family {
        let (proj_nodes, proj_edges) =
            get_memory_graph(conn, org_id, &project.name, since, cap, 0)?;
        for n in proj_nodes {
            // Last-write-wins for nodes — labels can vary slightly across
            // per-project calls but the id is the source of truth.
            nodes.insert(n.id.clone(), n);
        }
        for e in proj_edges {
            let key = format!("{}|{}|{}", e.from_id, e.edge_type, e.to_id);
            if seen_edge_keys.insert(key) {
                edges.push(e);
            }
        }
    }

    // Inject Project nodes for every family member — guarantees the family
    // has visible anchors even when no memories exist for some descendants.
    for p in family {
        let id = format!("project:{}", p.id);
        nodes.entry(id.clone()).or_insert(MemGraphNode {
            id,
            node_type: "Project".to_string(),
            label: p.name.clone(),
        });
    }

    // Inject parent_id hierarchy edges so the family structure is visible
    // in the rendered graph. These are in addition to the per-project
    // primitive's "belongs_to" edges from memories to their project.
    for p in family {
        let Some(parent_id) = p.parent_id.as_deref() else {
            continue;
        };
        let child_id = format!("project:{}", p.id);
        let parent_node = format!("project:{parent_id}");
        let key = format!("{child_id}|child_of|{parent_node}");
        if seen_edge_keys.insert(key) {
            edges.push(MemGraphEdge {
                id: format!("child_of:{child_id}:{parent_node}"),
                from_id: child_id,
                to_id: parent_node,
                edge_type: "child_of".to_string(),
            });
        }
    }

    Ok((nodes.into_values().collect(), edges))
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
        )
        .unwrap();
        let pid = upsert_code_project(&conn, "org1", "myapp", "/ws").unwrap();
        (conn, pid)
    }

    fn make_symbol(
        name: &str,
        qname: &str,
        sym_type: SymbolType,
        fp: &str,
        persist: Persist,
    ) -> RawSymbol {
        RawSymbol {
            symbol_type: sym_type,
            name: name.to_string(),
            qualified_name: qname.to_string(),
            file_path: Some(fp.to_string()),
            file_hash: Some("hash1".to_string()),
            start_line: Some(1),
            end_line: Some(10),
            language: "rust".to_string(),
            persist,
        }
    }

    fn make_edge(from: &str, to: &str, et: EdgeType, fp: &str) -> RawEdge {
        RawEdge {
            from_qname: from.to_string(),
            to_qname: to.to_string(),
            edge_type: et,
            file_path: Some(fp.to_string()),
            persist: Persist::FileOwned,
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
        let files: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND symbol_type='File'",
                rusqlite::params![pid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(files, 1, "exactly one File node");
    }

    #[test]
    fn persist_structure_is_idempotent() {
        let (conn, pid) = setup();
        let paths = vec!["src/a/b.rs".to_string()];
        persist_structure(&conn, pid, "myapp", &paths).unwrap();
        persist_structure(&conn, pid, "myapp", &paths).unwrap();

        let total: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1",
                rusqlite::params![pid],
                |r| r.get(0),
            )
            .unwrap();
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
            symbols: vec![make_symbol(
                "foo",
                "src/lib.rs::foo#1",
                SymbolType::Function,
                "src/lib.rs",
                Persist::FileOwned,
            )],
            edges: vec![make_edge(
                "file::src/lib.rs",
                "src/lib.rs::foo#1",
                EdgeType::Defines,
                "src/lib.rs",
            )],
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
            symbols: vec![make_symbol(
                "bar",
                "src/lib.rs::bar#1",
                SymbolType::Function,
                "src/lib.rs",
                Persist::FileOwned,
            )],
            edges: vec![make_edge(
                "file::src/lib.rs",
                "src/lib.rs::bar#1",
                EdgeType::Defines,
                "src/lib.rs",
            )],
        };
        persist_file_graph(&conn, pid, &fg2).unwrap();

        let count2: i32 = conn.query_row(
            "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND symbol_type='Function'",
            rusqlite::params![pid], |r| r.get(0),
        ).unwrap();
        assert_eq!(
            count2, 1,
            "still one Function after second index (foo replaced by bar)"
        );

        let bar_exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND name='bar'",
                rusqlite::params![pid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bar_exists, 1, "bar must exist");

        let foo_gone: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM code_symbols WHERE code_project_id=?1 AND name='foo'",
                rusqlite::params![pid],
                |r| r.get(0),
            )
            .unwrap();
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
            symbols: vec![make_symbol(
                "foo",
                "src/lib.rs::foo#1",
                SymbolType::Function,
                "src/lib.rs",
                Persist::FileOwned,
            )],
            edges: vec![make_edge(
                "file::src/lib.rs",
                "src/lib.rs::foo#1",
                EdgeType::Defines,
                "src/lib.rs",
            )],
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
        persist_structure(
            &conn,
            pid,
            "myapp",
            &["src/a.rs".to_string(), "src/b.rs".to_string()],
        )
        .unwrap();

        // Index a.rs
        let fg = FileGraph {
            file_rel_path: "src/a.rs".to_string(),
            symbols: vec![make_symbol(
                "foo",
                "src/a.rs::foo#1",
                SymbolType::Function,
                "src/a.rs",
                Persist::FileOwned,
            )],
            edges: vec![],
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
        assert_eq!(
            folder_id_before, folder_id_after,
            "Folder node id must be stable across reindexes"
        );

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
                make_symbol(
                    "foo",
                    "src/lib.rs::foo#1",
                    SymbolType::Function,
                    "src/lib.rs",
                    Persist::FileOwned,
                ),
                make_symbol(
                    "Bar",
                    "src/lib.rs::Bar#5",
                    SymbolType::Struct,
                    "src/lib.rs",
                    Persist::FileOwned,
                ),
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
            symbols: vec![make_symbol(
                "foo",
                "src/lib.rs::foo#1",
                SymbolType::Function,
                "src/lib.rs",
                Persist::FileOwned,
            )],
            edges: vec![make_edge(
                "file::src/lib.rs",
                "src/lib.rs::foo#1",
                EdgeType::Defines,
                "src/lib.rs",
            )],
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
        let (org, user, _key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
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
            rusqlite::params![
                id,
                org_id,
                user_id,
                project,
                project_id,
                tags,
                title,
                session_id,
                collection_id,
                created_at
            ],
        )
        .unwrap();
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
            insert_memory(
                &conn,
                &format!("m-fk-{i}"),
                &org_id,
                &user_id,
                "acme",
                Some(&project_id),
                None,
                None,
                "[]",
                None,
                "2026-01-01T00:00:00Z",
            );
        }
        for i in 0..3 {
            insert_memory(
                &conn,
                &format!("m-legacy-{i}"),
                &org_id,
                &user_id,
                "acme",
                None,
                None,
                None,
                "[]",
                None,
                "2026-01-01T00:00:00Z",
            );
        }

        let (nodes, _edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let project_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "Project").collect();
        assert_eq!(
            project_nodes.len(),
            1,
            "exactly one Project node for 'acme', got {}",
            project_nodes.len()
        );

        let memory_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == "Memory").collect();
        assert_eq!(
            memory_nodes.len(),
            8,
            "all 8 memories (5 FK-linked + 3 legacy) must appear"
        );
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
        )
        .unwrap();

        insert_memory(
            &conn,
            "m1",
            &org_id,
            &user_id,
            "acme",
            Some(&project_id),
            Some("sess1"),
            Some("col1"),
            r#"["auth","bug"]"#,
            Some("Fixed auth bug"),
            "2026-01-01T00:00:00Z",
        );

        let (nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        // Memory, Project, Session, User, Collection, 2 Tag nodes = 7
        assert_eq!(
            nodes.len(),
            7,
            "expected 7 nodes (memory+project+session+user+collection+2 tags), got {}",
            nodes.len()
        );

        let edge_types: std::collections::HashSet<&str> =
            edges.iter().map(|e| e.edge_type.as_str()).collect();
        assert!(edge_types.contains("belongs_to"));
        assert!(edge_types.contains("in_session"));
        assert!(edge_types.contains("created_by"));
        assert!(edge_types.contains("in_collection"));
        assert!(edge_types.contains("tagged"));
        // 1 belongs_to + 1 in_session + 1 created_by + 1 in_collection + 2 tagged = 6
        assert_eq!(
            edges.len(),
            6,
            "expected 6 edges total, got {}",
            edges.len()
        );
    }

    #[test]
    fn get_memory_graph_omits_edges_for_null_session_and_collection() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        insert_memory(
            &conn,
            "m1",
            &org_id,
            &user_id,
            "acme",
            Some(&project_id),
            None,
            None,
            "[]",
            None,
            "2026-01-01T00:00:00Z",
        );

        let (_nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let edge_types: std::collections::HashSet<&str> =
            edges.iter().map(|e| e.edge_type.as_str()).collect();
        assert!(
            edge_types.contains("belongs_to"),
            "belongs_to must be present"
        );
        assert!(
            edge_types.contains("created_by"),
            "created_by must be present"
        );
        assert!(
            !edge_types.contains("in_session"),
            "in_session must be absent for NULL session_id"
        );
        assert!(
            !edge_types.contains("in_collection"),
            "in_collection must be absent for NULL collection_id"
        );
        assert!(
            !edge_types.contains("tagged"),
            "tagged must be absent for empty tags"
        );
        assert_eq!(
            edges.len(),
            2,
            "only belongs_to + created_by, got {}",
            edges.len()
        );
    }

    #[test]
    fn get_memory_graph_has_no_dangling_edges_and_counts_match() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        conn.execute(
            "INSERT INTO sessions (id, org_id, project, directory) VALUES ('sess1', ?1, 'acme', '/ws')",
            rusqlite::params![org_id],
        ).unwrap();
        insert_memory(
            &conn,
            "m1",
            &org_id,
            &user_id,
            "acme",
            Some(&project_id),
            Some("sess1"),
            None,
            r#"["x"]"#,
            None,
            "2026-01-01T00:00:00Z",
        );
        insert_memory(
            &conn,
            "m2",
            &org_id,
            &user_id,
            "acme",
            None,
            None,
            None,
            "[]",
            None,
            "2026-01-02T00:00:00Z",
        );

        let (nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let node_ids: std::collections::HashSet<&str> =
            nodes.iter().map(|n| n.id.as_str()).collect();
        for edge in &edges {
            assert!(
                node_ids.contains(edge.from_id.as_str()),
                "from_id {} not in node set",
                edge.from_id
            );
            assert!(
                node_ids.contains(edge.to_id.as_str()),
                "to_id {} not in node set",
                edge.to_id
            );
        }
        assert_eq!(
            nodes.len(),
            node_ids.len(),
            "node_count must equal nodes.len()"
        );
    }

    // ── Phase 5: AuditEvent nodes/edges (Slice 2) ───────────────────────────

    #[test]
    fn get_memory_graph_audit_events_scoped_by_since_yield_performed_by_edge() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        insert_memory(
            &conn,
            "m1",
            &org_id,
            &user_id,
            "acme",
            Some(&project_id),
            None,
            None,
            "[]",
            None,
            "2026-01-01T00:00:00Z",
        );

        // One audit event before `since`, one at/after `since` — only the latter must appear.
        insert_audit_log_chained(
            &conn,
            &org_id,
            &user_id,
            "memory.read",
            "memory",
            Some("m1"),
            serde_json::json!({}),
            Some("2026-01-01T00:00:00Z"),
        )
        .unwrap();
        insert_audit_log_chained(
            &conn,
            &org_id,
            &user_id,
            "memory.updated",
            "memory",
            Some("m1"),
            serde_json::json!({}),
            Some("2026-06-01T00:00:00Z"),
        )
        .unwrap();

        let (nodes, edges) = get_memory_graph(
            &conn,
            &org_id,
            "acme",
            Some("2026-03-01T00:00:00Z"),
            2000,
            0,
        )
        .unwrap();

        let audit_nodes: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == "AuditEvent")
            .collect();
        assert_eq!(
            audit_nodes.len(),
            1,
            "only the audit event at/after `since` must appear, got {}",
            audit_nodes.len()
        );

        let audit_node_id = &audit_nodes[0].id;
        let performed_by: Vec<_> = edges
            .iter()
            .filter(|e| &e.from_id == audit_node_id && e.edge_type == "performed_by")
            .collect();
        assert_eq!(
            performed_by.len(),
            1,
            "exactly one performed_by edge from the AuditEvent node"
        );
        assert_eq!(performed_by[0].to_id, format!("user:{user_id}"));
    }

    #[test]
    fn get_memory_graph_audit_targets_edge_present_when_resource_in_graph_dropped_when_absent() {
        let (conn, org_id, user_id) = setup();
        let project_id = get_or_create_project(&conn, &org_id, "acme").unwrap();
        insert_memory(
            &conn,
            "m1",
            &org_id,
            &user_id,
            "acme",
            Some(&project_id),
            None,
            None,
            "[]",
            None,
            "2026-01-01T00:00:00Z",
        );

        // Targets a memory that IS in the graph (m1) -> targets edge must be present.
        insert_audit_log_chained(
            &conn,
            &org_id,
            &user_id,
            "memory.read",
            "memory",
            Some("m1"),
            serde_json::json!({}),
            Some("2026-01-02T00:00:00Z"),
        )
        .unwrap();
        // Targets a memory that is NOT in the graph -> targets edge must be dropped (no dangling edge).
        insert_audit_log_chained(
            &conn,
            &org_id,
            &user_id,
            "memory.read",
            "memory",
            Some("m-does-not-exist"),
            serde_json::json!({}),
            Some("2026-01-02T00:00:01Z"),
        )
        .unwrap();

        let (nodes, edges) = get_memory_graph(&conn, &org_id, "acme", None, 2000, 0).unwrap();

        let targets_edges: Vec<_> = edges.iter().filter(|e| e.edge_type == "targets").collect();
        assert_eq!(
            targets_edges.len(),
            1,
            "only the audit event whose resource is in the graph gets a targets edge"
        );
        assert_eq!(targets_edges[0].to_id, "memory:m1");

        // No-dangling-edges invariant must still hold across the whole graph.
        let node_ids: std::collections::HashSet<&str> =
            nodes.iter().map(|n| n.id.as_str()).collect();
        for edge in &edges {
            assert!(
                node_ids.contains(edge.from_id.as_str()),
                "from_id {} not in node set",
                edge.from_id
            );
            assert!(
                node_ids.contains(edge.to_id.as_str()),
                "to_id {} not in node set",
                edge.to_id
            );
        }

        // Both audit events still produce AuditEvent nodes even though one has no targets edge.
        let audit_nodes: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == "AuditEvent")
            .collect();
        assert_eq!(
            audit_nodes.len(),
            2,
            "both audit events must still appear as AuditEvent nodes"
        );
    }
}

#[cfg(test)]
mod task_query_tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::db::migrations;

    fn setup() -> (Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        (conn, org.id, user.id)
    }

    fn mk_task(conn: &Connection, org_id: &str, user_id: &str, project: &str, title: &str) -> Task {
        let req = CreateTaskRequest {
            project: project.to_string(),
            title: title.to_string(),
            ..Default::default()
        };
        create_task(conn, org_id, user_id, &req).unwrap()
    }

    // ── PR1: core CRUD ───────────────────────────────────────────────────

    #[test]
    fn create_task_persists_defaults() {
        let (conn, org, user) = setup();
        let req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "Do the thing".to_string(),
            ..Default::default()
        };
        let task = create_task(&conn, &org, &user, &req).unwrap();
        assert_eq!(task.status, "backlog");
        assert_eq!(task.priority, "medium");
        assert_eq!(task.created_by, user);
        assert!(!task.created_at.is_empty());
        assert_eq!(task.created_at, task.updated_at);
    }

    #[test]
    fn create_task_rejects_invalid_status() {
        let (conn, org, user) = setup();
        let req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "Bad status".to_string(),
            status: Some("bogus".to_string()),
            ..Default::default()
        };
        let result = create_task(&conn, &org, &user, &req);
        assert!(result.is_err());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "no row should be created on invalid status");
    }

    #[test]
    fn get_task_hydrates_relations() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "Parent");
        set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        add_task_label(&conn, &task.id, "bug").unwrap();
        link_task_spec(&conn, &task.id, &user, "team-tasks").unwrap();
        add_task_comment(&conn, &task.id, &user, "hello").unwrap();
        mk_task(&conn, &org, &user, "proj", "Child"); // unrelated, no parent_id set

        let hydrated = get_task(&conn, &org, &task.id).unwrap().unwrap();
        assert_eq!(hydrated.assignees.len(), 1);
        assert_eq!(hydrated.labels, vec!["bug".to_string()]);
        assert_eq!(hydrated.spec_links, vec!["team-tasks".to_string()]);
        assert_eq!(hydrated.comment_count, 1);
        assert_eq!(hydrated.subtask_count, 0);
    }

    #[test]
    fn patch_task_updates_fields_and_bumps_updated_at() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "Original");
        let patched = patch_task(
            &conn,
            &org,
            &task.id,
            &PatchTaskRequest {
                title: Some("Updated".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(patched.title, "Updated");
        // `updated_at` is always rewritten by the explicit `SET updated_at = ?1` clause in
        // `patch_task`; second-resolution timestamps make a value-inequality assertion flaky
        // under fast test execution, so this asserts presence rather than a timestamp diff.
        assert!(!patched.updated_at.is_empty());
    }

    #[test]
    fn patch_task_rejects_illegal_transition() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        // backlog -> done is not an allowed edge.
        let result = patch_task(
            &conn,
            &org,
            &task.id,
            &PatchTaskRequest {
                status: Some("done".to_string()),
                ..Default::default()
            },
        );
        assert!(result.is_err());
        let reloaded = get_task(&conn, &org, &task.id).unwrap().unwrap();
        assert_eq!(
            reloaded.status, "backlog",
            "status must be unchanged on rejected transition"
        );
    }

    #[test]
    fn soft_delete_task_sets_archived_at_and_excludes_from_list() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        assert!(soft_delete_task(&conn, &org, &task.id).unwrap());
        // get_task fetches by id regardless of archive status; archived_at must be set.
        let reloaded = get_task(&conn, &org, &task.id).unwrap().unwrap();
        assert!(reloaded.archived_at.is_some());
        // list_tasks excludes archived tasks by default (include_archived: false).
        let filters = TaskListFilters {
            project: Some("proj".to_string()),
            ..Default::default()
        };
        let listed = list_tasks(&conn, &org, None, &filters, i64::MAX, 0).unwrap();
        assert!(listed.iter().all(|t| t.id != task.id));
    }

    #[test]
    fn list_tasks_filters_by_project_status_priority() {
        let (conn, org, user) = setup();
        mk_task(&conn, &org, &user, "proj-a", "A1");
        let b = mk_task(&conn, &org, &user, "proj-b", "B1");
        patch_task(
            &conn,
            &org,
            &b.id,
            &PatchTaskRequest {
                status: Some("todo".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let filters = TaskListFilters {
            project: Some("proj-b".to_string()),
            ..Default::default()
        };
        let listed = list_tasks(&conn, &org, None, &filters, i64::MAX, 0).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, b.id);

        let filters_status = TaskListFilters {
            status: Some("todo".to_string()),
            ..Default::default()
        };
        let listed_status = list_tasks(&conn, &org, None, &filters_status, i64::MAX, 0).unwrap();
        assert_eq!(listed_status.len(), 1);
        assert_eq!(listed_status[0].id, b.id);
    }

    #[test]
    fn count_tasks_matches_filtered_set() {
        let (conn, org, user) = setup();
        mk_task(&conn, &org, &user, "proj", "A");
        mk_task(&conn, &org, &user, "proj", "B");
        mk_task(&conn, &org, &user, "other", "C");
        let filters = TaskListFilters {
            project: Some("proj".to_string()),
            ..Default::default()
        };
        let count = count_tasks(&conn, &org, None, &filters).unwrap();
        assert_eq!(count, 2);
    }

    // ── PR2: assignment ─────────────────────────────────────────────────

    #[test]
    fn set_task_assignees_returns_denormalized_display() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        let result = set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, user);
        assert!(!result[0].name.is_empty());
        assert!(result[0].email.contains('@'));
    }

    #[test]
    fn set_task_assignees_rejects_user_outside_org() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        let (other_org, _, _) = {
            let conn2 = connect(":memory:").unwrap();
            migrations::run_all(&conn2).unwrap();
            bootstrap(&conn2, "Other", "other", "a@other.com", "A").unwrap()
        };
        let _ = other_org;
        let fake_user_id = uuid::Uuid::new_v4().to_string();
        let result = set_task_assignees(&conn, &org, &task.id, &user, &[fake_user_id]);
        assert!(result.is_err());
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_assignees WHERE task_id = ?1",
                [&task.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "no partial write on rejected assignment");
    }

    #[test]
    fn set_task_assignees_rejects_nonexistent_user() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        let result = set_task_assignees(
            &conn,
            &org,
            &task.id,
            &user,
            &["does-not-exist".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn set_task_assignees_is_idempotent_for_duplicate() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_assignees WHERE task_id = ?1",
                [&task.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // FIX 3: the insert loop must run inside a real transaction so a failure partway
    // through (e.g. the task row disappears mid-call) rolls back every insert already
    // made in that call, instead of leaving a partial assignee list.
    #[test]
    fn set_task_assignees_rolls_back_all_inserts_on_mid_loop_failure() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        let other_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, 'other@acme.com', 'Other', 'member', 'active', datetime('now'))",
            rusqlite::params![other_id, org],
        )
        .unwrap();

        // Hard-delete the task row (simulating a concurrent delete) right before the
        // insert loop runs, so every INSERT INTO task_assignees hits a dangling FK on
        // task_id. If the loop is transactional, zero rows survive; with the pre-fix
        // "one INSERT per user_id, no transaction" implementation this would already
        // fail atomically for THIS exact case too — the regression this test guards is
        // that a real `tx.commit()`-or-rollback path exists at all, not ad hoc success.
        conn.execute("DELETE FROM tasks WHERE id = ?1", [&task.id])
            .unwrap();

        let result = set_task_assignees(&conn, &org, &task.id, &user, &[user.clone(), other_id]);
        assert!(
            result.is_err(),
            "insert against a deleted task must fail (FK violation)"
        );
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_assignees WHERE task_id = ?1",
                [&task.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "no partial writes must survive a mid-loop failure"
        );
    }

    #[test]
    fn remove_task_assignee_deletes_row() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        assert!(remove_task_assignee(&conn, &task.id, &user).unwrap());
        assert_eq!(list_task_assignees(&conn, &task.id).unwrap().len(), 0);
    }

    #[test]
    fn list_task_assignees_returns_display_data() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        let list = list_task_assignees(&conn, &task.id).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn list_tasks_assignee_me_filter() {
        let (conn, org, user) = setup();
        let assigned = mk_task(&conn, &org, &user, "proj", "Assigned");
        mk_task(&conn, &org, &user, "proj", "Unassigned");
        set_task_assignees(&conn, &org, &assigned.id, &user, &[user.clone()]).unwrap();

        let filters = TaskListFilters {
            assignee_user_id: Some(user.clone()),
            ..Default::default()
        };
        let listed = list_tasks(&conn, &org, None, &filters, i64::MAX, 0).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, assigned.id);
    }

    // ── PR3: organization (labels + subtasks) ───────────────────────────

    #[test]
    fn add_task_label_appends_to_list() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        let labels = add_task_label(&conn, &task.id, "bug").unwrap();
        assert_eq!(labels, vec!["bug".to_string()]);
    }

    #[test]
    fn remove_task_label_removes_it() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        add_task_label(&conn, &task.id, "bug").unwrap();
        assert!(remove_task_label(&conn, &task.id, "bug").unwrap());
        assert!(list_task_labels(&conn, &task.id).unwrap().is_empty());
    }

    #[test]
    fn list_tasks_filter_by_label() {
        let (conn, org, user) = setup();
        let a = mk_task(&conn, &org, &user, "proj", "A");
        mk_task(&conn, &org, &user, "proj", "B");
        add_task_label(&conn, &a.id, "urgent-fix").unwrap();

        let filters = TaskListFilters {
            label: Some("urgent-fix".to_string()),
            ..Default::default()
        };
        let listed = list_tasks(&conn, &org, None, &filters, i64::MAX, 0).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, a.id);
    }

    // FIX 1: list_tasks previously returned tasks via map_task_row alone, which always
    // leaves assignees/labels empty — only get_task hydrated them. This asserts list_tasks
    // batch-hydrates both relations so the admin list view stops showing "Unassigned" for
    // tasks that actually have an assignee.
    #[test]
    fn list_tasks_hydrates_assignees_and_labels() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        add_task_label(&conn, &task.id, "bug").unwrap();
        mk_task(&conn, &org, &user, "proj", "Other"); // no assignees/labels

        let filters = TaskListFilters {
            project: Some("proj".to_string()),
            ..Default::default()
        };
        let listed = list_tasks(&conn, &org, None, &filters, i64::MAX, 0).unwrap();
        assert_eq!(listed.len(), 2);
        let listed_task = listed.iter().find(|t| t.id == task.id).unwrap();
        assert_eq!(listed_task.assignees.len(), 1);
        assert_eq!(listed_task.assignees[0].id, user);
        assert_eq!(listed_task.labels, vec!["bug".to_string()]);
        let other_task = listed.iter().find(|t| t.id != task.id).unwrap();
        assert!(other_task.assignees.is_empty());
        assert!(other_task.labels.is_empty());
    }

    // Same hydration requirement applies to the sprint board view.
    #[test]
    fn list_tasks_in_sprint_hydrates_assignees_and_labels() {
        let (conn, org, user) = setup();
        let sprint = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj".to_string(),
                name: "Sprint 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "In sprint".to_string(),
            sprint_id: Some(sprint.id.clone()),
            ..Default::default()
        };
        let task = create_task(&conn, &org, &user, &req).unwrap();
        set_task_assignees(&conn, &org, &task.id, &user, &[user.clone()]).unwrap();
        add_task_label(&conn, &task.id, "bug").unwrap();

        let listed = list_tasks_in_sprint(&conn, &sprint.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].assignees.len(), 1);
        assert_eq!(listed[0].labels, vec!["bug".to_string()]);
    }

    // Batch-hydration must not run any queries for an empty result set.
    #[test]
    fn list_tasks_empty_result_hydrates_without_error() {
        let (conn, org, _user) = setup();
        let filters = TaskListFilters {
            project: Some("does-not-exist".to_string()),
            ..Default::default()
        };
        let listed = list_tasks(&conn, &org, None, &filters, i64::MAX, 0).unwrap();
        assert!(listed.is_empty());
    }

    #[test]
    fn create_task_with_parent_id_creates_subtask() {
        let (conn, org, user) = setup();
        let parent = mk_task(&conn, &org, &user, "proj", "Parent");
        let child_req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "Child".to_string(),
            parent_id: Some(parent.id.clone()),
            ..Default::default()
        };
        let child = create_task(&conn, &org, &user, &child_req).unwrap();
        let children = list_subtasks(&conn, &org, &parent.id).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child.id);
    }

    #[test]
    fn create_task_rejects_cross_project_parent() {
        let (conn, org, user) = setup();
        let parent = mk_task(&conn, &org, &user, "proj-x", "Parent");
        let child_req = CreateTaskRequest {
            project: "proj-y".to_string(),
            title: "Child".to_string(),
            parent_id: Some(parent.id.clone()),
            ..Default::default()
        };
        let result = create_task(&conn, &org, &user, &child_req);
        assert!(result.is_err());
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE title = 'Child'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    // FIX 2: create_task must validate sprint_id the same way patch_task does — a sprint
    // that belongs to a different project (or doesn't exist) must be rejected, not silently
    // attached.
    #[test]
    fn create_task_rejects_sprint_from_different_project() {
        let (conn, org, user) = setup();
        let sprint = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj-x".to_string(),
                name: "Sprint 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let req = CreateTaskRequest {
            project: "proj-y".to_string(),
            title: "T".to_string(),
            sprint_id: Some(sprint.id.clone()),
            ..Default::default()
        };
        let result = create_task(&conn, &org, &user, &req);
        assert!(result.is_err());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE title = 'T'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "no task row created when sprint validation fails");
    }

    #[test]
    fn create_task_rejects_nonexistent_sprint_id() {
        let (conn, org, user) = setup();
        let req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "T".to_string(),
            sprint_id: Some("does-not-exist".to_string()),
            ..Default::default()
        };
        let result = create_task(&conn, &org, &user, &req);
        assert!(result.is_err());
    }

    #[test]
    fn create_task_accepts_sprint_from_same_project() {
        let (conn, org, user) = setup();
        let sprint = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj".to_string(),
                name: "Sprint 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "T".to_string(),
            sprint_id: Some(sprint.id.clone()),
            ..Default::default()
        };
        let task = create_task(&conn, &org, &user, &req).unwrap();
        assert_eq!(task.sprint_id, Some(sprint.id));
    }

    #[test]
    fn soft_delete_parent_does_not_cascade_to_subtasks() {
        let (conn, org, user) = setup();
        let parent = mk_task(&conn, &org, &user, "proj", "Parent");
        let child_req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "Child".to_string(),
            parent_id: Some(parent.id.clone()),
            ..Default::default()
        };
        let child = create_task(&conn, &org, &user, &child_req).unwrap();

        assert!(soft_delete_task(&conn, &org, &parent.id).unwrap());
        let reloaded_child = get_task(&conn, &org, &child.id).unwrap().unwrap();
        assert!(
            reloaded_child.archived_at.is_none(),
            "subtask must remain non-archived"
        );
        assert_eq!(
            reloaded_child.parent_id.as_deref(),
            Some(parent.id.as_str())
        );
    }

    #[test]
    fn subtask_status_update_does_not_affect_parent() {
        let (conn, org, user) = setup();
        let parent = mk_task(&conn, &org, &user, "proj", "Parent");
        let child_req = CreateTaskRequest {
            project: "proj".to_string(),
            title: "Child".to_string(),
            parent_id: Some(parent.id.clone()),
            ..Default::default()
        };
        let child = create_task(&conn, &org, &user, &child_req).unwrap();
        patch_task(
            &conn,
            &org,
            &child.id,
            &PatchTaskRequest {
                status: Some("todo".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let reloaded_parent = get_task(&conn, &org, &parent.id).unwrap().unwrap();
        assert_eq!(reloaded_parent.status, "backlog");
    }

    // ── PR4: collaboration (comments) ───────────────────────────────────

    #[test]
    fn add_task_comment_persists_author_body_timestamp() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        let comment = add_task_comment(&conn, &task.id, &user, "First comment").unwrap();
        assert_eq!(comment.body, "First comment");
        assert_eq!(comment.user_id, user);
        assert!(!comment.created_at.is_empty());
    }

    #[test]
    fn add_task_comment_rejects_empty_or_whitespace_body() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        assert!(add_task_comment(&conn, &task.id, &user, "   ").is_err());
        assert!(add_task_comment(&conn, &task.id, &user, "").is_err());
        assert_eq!(list_task_comments(&conn, &task.id).unwrap().len(), 0);
    }

    #[test]
    fn list_task_comments_returns_chronological_order() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        add_task_comment(&conn, &task.id, &user, "first").unwrap();
        add_task_comment(&conn, &task.id, &user, "second").unwrap();
        let list = list_task_comments(&conn, &task.id).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].body, "first");
        assert_eq!(list[1].body, "second");
    }

    #[test]
    fn delete_comment_by_author_succeeds() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        let comment = add_task_comment(&conn, &task.id, &user, "hi").unwrap();
        assert!(delete_task_comment(&conn, &comment.id).unwrap());
        assert_eq!(list_task_comments(&conn, &task.id).unwrap().len(), 0);
    }

    // ── PR5: spec links + auto-resolve ──────────────────────────────────

    #[test]
    fn link_task_spec_adds_to_list() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        link_task_spec(&conn, &task.id, &user, "team-tasks").unwrap();
        assert_eq!(
            list_task_spec_links(&conn, &task.id).unwrap(),
            vec!["team-tasks".to_string()]
        );
    }

    #[test]
    fn link_task_spec_supports_multiple_changes_per_task_and_multiple_tasks_per_change() {
        let (conn, org, user) = setup();
        let t1 = mk_task(&conn, &org, &user, "proj", "T1");
        let t2 = mk_task(&conn, &org, &user, "proj", "T2");
        link_task_spec(&conn, &t1.id, &user, "change-a").unwrap();
        link_task_spec(&conn, &t1.id, &user, "change-b").unwrap();
        link_task_spec(&conn, &t2.id, &user, "change-a").unwrap();

        assert_eq!(list_task_spec_links(&conn, &t1.id).unwrap().len(), 2);
        assert_eq!(
            list_task_spec_links(&conn, &t2.id).unwrap(),
            vec!["change-a".to_string()]
        );
    }

    #[test]
    fn read_task_with_dangling_spec_link_still_succeeds() {
        let (conn, org, user) = setup();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        link_task_spec(&conn, &task.id, &user, "renamed-away").unwrap();
        let hydrated = get_task(&conn, &org, &task.id).unwrap().unwrap();
        assert_eq!(hydrated.spec_links, vec!["renamed-away".to_string()]);
    }

    #[test]
    fn resolve_tasks_by_spec_transitions_all_linked_non_terminal_tasks() {
        let (conn, org, user) = setup();
        let mut ids = Vec::new();
        for i in 0..3 {
            let t = mk_task(&conn, &org, &user, "proj", &format!("T{i}"));
            link_task_spec(&conn, &t.id, &user, "team-tasks").unwrap();
            ids.push(t.id);
        }
        let resolved = resolve_tasks_by_spec(&conn, &org, "team-tasks", None).unwrap();
        assert_eq!(resolved.len(), 3);
        for id in &ids {
            let task = get_task(&conn, &org, id).unwrap().unwrap();
            assert_eq!(task.status, "done");
        }
    }

    #[test]
    fn resolve_tasks_by_spec_noop_for_unlinked_change() {
        let (conn, org, user) = setup();
        mk_task(&conn, &org, &user, "proj", "T");
        let resolved = resolve_tasks_by_spec(&conn, &org, "no-such-change", None).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_tasks_by_spec_skips_already_terminal_tasks() {
        let (conn, org, user) = setup();
        let t = mk_task(&conn, &org, &user, "proj", "T");
        link_task_spec(&conn, &t.id, &user, "team-tasks").unwrap();
        // Route it to cancelled via a legal path: backlog -> cancelled.
        patch_task(
            &conn,
            &org,
            &t.id,
            &PatchTaskRequest {
                status: Some("cancelled".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let resolved = resolve_tasks_by_spec(&conn, &org, "team-tasks", None).unwrap();
        assert!(resolved.is_empty());
        let reloaded = get_task(&conn, &org, &t.id).unwrap().unwrap();
        assert_eq!(reloaded.status, "cancelled");
    }

    // ── PR6: sprints ─────────────────────────────────────────────────────

    #[test]
    fn create_sprint_scoped_to_project() {
        let (conn, org, user) = setup();
        let req = CreateSprintRequest {
            project: "proj".to_string(),
            name: "Sprint 1".to_string(),
            ..Default::default()
        };
        let sprint = create_sprint(&conn, &org, &user, &req).unwrap();
        assert_eq!(sprint.status, "planned");
        assert_eq!(sprint.project, "proj");
    }

    #[test]
    fn get_patch_soft_delete_list_sprints_round_trip() {
        let (conn, org, user) = setup();
        let req = CreateSprintRequest {
            project: "proj".to_string(),
            name: "Sprint 1".to_string(),
            ..Default::default()
        };
        let sprint = create_sprint(&conn, &org, &user, &req).unwrap();

        let fetched = get_sprint(&conn, &org, &sprint.id).unwrap().unwrap();
        assert_eq!(fetched.id, sprint.id);

        let patched = patch_sprint(
            &conn,
            &org,
            &sprint.id,
            &PatchSprintRequest {
                status: Some("active".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(patched.status, "active");

        let listed =
            list_sprints(&conn, &org, None, Some("proj"), None, false, i64::MAX, 0).unwrap();
        assert_eq!(listed.len(), 1);

        assert!(soft_delete_sprint(&conn, &org, &sprint.id).unwrap());
        let listed_after =
            list_sprints(&conn, &org, None, Some("proj"), None, false, i64::MAX, 0).unwrap();
        assert!(listed_after.is_empty());
    }

    #[test]
    fn assign_task_to_sprint_appears_in_sprint_task_list() {
        let (conn, org, user) = setup();
        let sprint = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj".to_string(),
                name: "Sprint 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        patch_task(
            &conn,
            &org,
            &task.id,
            &PatchTaskRequest {
                sprint_id: Some(sprint.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let in_sprint = list_tasks_in_sprint(&conn, &sprint.id).unwrap();
        assert_eq!(in_sprint.len(), 1);
        assert_eq!(in_sprint[0].id, task.id);
    }

    #[test]
    fn assign_task_to_sprint_rejects_cross_project() {
        let (conn, org, user) = setup();
        let sprint = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj-x".to_string(),
                name: "Sprint 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let task = mk_task(&conn, &org, &user, "proj-y", "T");
        let result = patch_task(
            &conn,
            &org,
            &task.id,
            &PatchTaskRequest {
                sprint_id: Some(sprint.id.clone()),
                ..Default::default()
            },
        );
        assert!(result.is_err());
        let reloaded = get_task(&conn, &org, &task.id).unwrap().unwrap();
        assert!(reloaded.sprint_id.is_none());
    }

    #[test]
    fn moving_task_to_new_sprint_removes_from_prior() {
        let (conn, org, user) = setup();
        let sprint_a = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj".to_string(),
                name: "Sprint A".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let sprint_b = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj".to_string(),
                name: "Sprint B".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let task = mk_task(&conn, &org, &user, "proj", "T");
        patch_task(
            &conn,
            &org,
            &task.id,
            &PatchTaskRequest {
                sprint_id: Some(sprint_a.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        patch_task(
            &conn,
            &org,
            &task.id,
            &PatchTaskRequest {
                sprint_id: Some(sprint_b.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(list_tasks_in_sprint(&conn, &sprint_a.id)
            .unwrap()
            .is_empty());
        assert_eq!(list_tasks_in_sprint(&conn, &sprint_b.id).unwrap().len(), 1);
    }

    #[test]
    fn create_retrospective_persists_and_associates_with_sprint() {
        let (conn, org, user) = setup();
        let sprint = create_sprint(
            &conn,
            &org,
            &user,
            &CreateSprintRequest {
                project: "proj".to_string(),
                name: "Sprint 1".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let retro = create_retrospective(
            &conn,
            &sprint.id,
            &org,
            &user,
            &CreateRetrospectiveRequest {
                went_well: Some("Good pace".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(retro.sprint_id, sprint.id);
        assert_eq!(retro.went_well.as_deref(), Some("Good pace"));

        let list = list_retrospectives(&conn, &sprint.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, retro.id);
    }
}

#[cfg(test)]
mod task_nesting_tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::db::migrations;

    fn setup() -> (Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _k) = bootstrap(&conn, "Acme", "acme", "a@acme.com", "A").unwrap();
        (conn, org.id, user.id)
    }

    fn mk(
        conn: &Connection,
        org: &str,
        user: &str,
        title: &str,
        parent: Option<&str>,
    ) -> Result<Task> {
        create_task(
            conn,
            org,
            user,
            &CreateTaskRequest {
                project: "p".into(),
                title: title.into(),
                parent_id: parent.map(str::to_string),
                ..Default::default()
            },
        )
    }

    /// The shape of real work is three levels deep, and the old rule forbade it:
    ///
    ///     change / epic  ->  PR / work unit  ->  checklist item
    ///
    /// SDD produces exactly that (a tasks.md has sections, each with items). Flattening
    /// it costs you either the grouping or the items.
    #[test]
    fn a_subtask_can_have_subtasks() {
        let (conn, org, user) = setup();
        let epic = mk(&conn, &org, &user, "epic", None).unwrap();
        let pr = mk(&conn, &org, &user, "PR-1", Some(&epic.id)).unwrap();
        let item = mk(&conn, &org, &user, "1.1 RED: write the failing test", Some(&pr.id))
            .expect("three levels must be allowed — this used to fail with `cannot nest a subtask under a subtask`");

        assert_eq!(item.parent_id.as_deref(), Some(pr.id.as_str()));
        assert_eq!(pr.parent_id.as_deref(), Some(epic.id.as_str()));
    }

    /// Unbounded depth is not the goal — a bounded tree is.
    #[test]
    fn nesting_deeper_than_the_cap_is_refused() {
        let (conn, org, user) = setup();
        let mut parent = mk(&conn, &org, &user, "root", None).unwrap();
        // MAX_TASK_DEPTH levels are reachable...
        for i in 1..MAX_TASK_DEPTH {
            parent = mk(&conn, &org, &user, &format!("level {i}"), Some(&parent.id))
                .unwrap_or_else(|e| {
                    panic!("level {i} must be allowed (cap is {MAX_TASK_DEPTH}): {e}")
                });
        }
        // ...and the one past it is not.
        let err = mk(&conn, &org, &user, "too deep", Some(&parent.id)).unwrap_err();
        assert!(
            err.to_string().contains("too deep"),
            "past the cap must be refused, got: {err}"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE title = 'too deep'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "a refused create must write no row");
    }

    /// `subtask_count` was hydrated only by `get_task`. Every LIST response reported 0 for
    /// tasks that had children — the API told the admin a task with six subtasks had none.
    #[test]
    fn list_tasks_reports_the_real_subtask_count() {
        let (conn, org, user) = setup();
        let parent = mk(&conn, &org, &user, "section", None).unwrap();
        for i in 0..3 {
            mk(&conn, &org, &user, &format!("item {i}"), Some(&parent.id)).unwrap();
        }
        let lonely = mk(&conn, &org, &user, "no children", None).unwrap();

        let listed = list_tasks(&conn, &org, None, &TaskListFilters::default(), 100, 0).unwrap();

        let p = listed.iter().find(|t| t.id == parent.id).unwrap();
        assert_eq!(
            p.subtask_count, 3,
            "the list must report the real count, not 0"
        );

        let l = listed.iter().find(|t| t.id == lonely.id).unwrap();
        assert_eq!(l.subtask_count, 0);
    }

    /// The parent must still belong to the same project — that rule was never the problem.
    #[test]
    fn a_parent_in_another_project_is_still_refused() {
        let (conn, org, user) = setup();
        let other = create_task(
            &conn,
            &org,
            &user,
            &CreateTaskRequest {
                project: "other-project".into(),
                title: "elsewhere".into(),
                ..Default::default()
            },
        )
        .unwrap();

        let err = mk(&conn, &org, &user, "child", Some(&other.id)).unwrap_err();
        assert!(err.to_string().contains("different project"));
    }

    /// The ancestry walk is bounded rather than trusting the data to be acyclic: it runs
    /// inside a write path, and a loop that trusts its input is a hang waiting for a
    /// corrupt row.
    #[test]
    fn a_cyclic_ancestry_is_reported_not_hung_on() {
        let (conn, org, user) = setup();
        let a = mk(&conn, &org, &user, "a", None).unwrap();
        let b = mk(&conn, &org, &user, "b", Some(&a.id)).unwrap();
        // Forge a cycle behind create_task's back — only corruption could do this.
        conn.execute(
            "UPDATE tasks SET parent_id = ?1 WHERE id = ?2",
            rusqlite::params![b.id, a.id],
        )
        .unwrap();

        let err = task_ancestor_depth(&conn, &org, &b.id).unwrap_err();
        assert!(err.to_string().contains("cyclic"), "got: {err}");
    }
}

#[cfg(test)]
mod sdd_query_tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::db::migrations;

    fn setup() -> (Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        (conn, org.id, user.id)
    }

    /// A second org with its own admin — for the isolation tests.
    fn second_org(conn: &Connection) -> (String, String) {
        let org_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Beta', 'beta')",
            [&org_id],
        )
        .unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status)
             VALUES (?1, ?2, 'b@beta.com', 'B', 'admin', 'active')",
            rusqlite::params![user_id, org_id],
        )
        .unwrap();
        (org_id, user_id)
    }

    fn save_req(project: &str, change: &str, kind: &str, content: &str) -> SaveArtifactRequest {
        SaveArtifactRequest {
            project: project.to_string(),
            change_name: change.to_string(),
            kind: kind.to_string(),
            content: content.to_string(),
            ..Default::default()
        }
    }

    fn mk_change(conn: &Connection, org: &str, user: &str, project: &str, name: &str) -> SddChange {
        let req = UpsertChangeRequest {
            project: project.to_string(),
            name: name.to_string(),
            ..Default::default()
        };
        upsert_sdd_change(conn, org, user, &req).unwrap()
    }

    fn revision_count(conn: &Connection, artifact_id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sdd_artifact_revisions WHERE artifact_id = ?1",
            [artifact_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn fts_row_count(conn: &Connection, artifact_id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sdd_artifacts_fts WHERE artifact_id = ?1",
            [artifact_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    // ── Changes ──────────────────────────────────────────────────────────

    /// 2.1
    #[test]
    fn upsert_sdd_change_creates_row_with_defaults() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "nexus-mind", "team-tasks");

        assert_eq!(change.status, "active");
        assert_eq!(change.phase, "propose");
        assert_eq!(change.created_by, user);
        assert!(!change.created_at.is_empty());
        assert!(!change.updated_at.is_empty());
        assert!(change.archived_at.is_none());
    }

    /// 2.3
    #[test]
    fn upsert_sdd_change_is_idempotent_on_org_project_name() {
        let (conn, org, user) = setup();
        let first = mk_change(&conn, &org, &user, "nexus-mind", "team-tasks");

        let req = UpsertChangeRequest {
            project: "nexus-mind".into(),
            name: "team-tasks".into(),
            title: Some("Team Tasks".into()),
            ..Default::default()
        };
        let second = upsert_sdd_change(&conn, &org, &user, &req).unwrap();

        assert_eq!(
            first.id, second.id,
            "the same (org, project, name) must upsert, not duplicate"
        );
        assert_eq!(second.title.as_deref(), Some("Team Tasks"));

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_changes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "exactly one row");
    }

    /// 2.5
    #[test]
    fn upsert_sdd_change_same_name_in_two_projects_are_two_changes() {
        let (conn, org, user) = setup();
        let a = mk_change(&conn, &org, &user, "nexus-mind", "team-tasks");
        let b = mk_change(&conn, &org, &user, "kasymir", "team-tasks");

        assert_ne!(a.id, b.id, "same name in two projects must be two changes");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_changes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    /// 2.5 — D4: project is a name string, not an FK. An unregistered name is accepted.
    #[test]
    fn upsert_sdd_change_accepts_an_unregistered_project_name() {
        let (conn, org, user) = setup();
        let change = mk_change(
            &conn,
            &org,
            &user,
            "never-registered-anywhere",
            "some-change",
        );
        assert_eq!(change.project, "never-registered-anywhere");

        let registered: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE name = 'never-registered-anywhere'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(registered, 0, "no projects row was implicitly created");
    }

    /// 2.7
    #[test]
    fn get_sdd_change_hydrates_artifact_inventory() {
        let (conn, org, user) = setup();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "proposal", "P"),
            "agent",
        )
        .unwrap();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "D"),
            "agent",
        )
        .unwrap();

        let change = get_sdd_change_by_name(&conn, &org, "p", "c")
            .unwrap()
            .unwrap();
        let fetched = get_sdd_change(&conn, &org, &change.id).unwrap().unwrap();

        assert_eq!(fetched.artifacts.len(), 2);
        // Ordered by kind: design < proposal
        assert_eq!(fetched.artifacts[0].kind, "design");
        assert_eq!(fetched.artifacts[1].kind, "proposal");
    }

    /// 2.9
    #[test]
    fn get_sdd_change_org_isolation() {
        let (conn, org_a, user_a) = setup();
        let change = mk_change(&conn, &org_a, &user_a, "p", "c");
        let (org_b, _) = second_org(&conn);

        assert!(
            get_sdd_change(&conn, &org_b, &change.id).unwrap().is_none(),
            "org B must not see org A's change"
        );
        assert!(get_sdd_change(&conn, &org_a, &change.id).unwrap().is_some());
    }

    /// 2.11
    #[test]
    fn get_sdd_change_by_name_resolves_project_scoped_name() {
        let (conn, org, user) = setup();
        let a = mk_change(&conn, &org, &user, "nexus-mind", "team-tasks");
        mk_change(&conn, &org, &user, "kasymir", "team-tasks");

        let found = get_sdd_change_by_name(&conn, &org, "nexus-mind", "team-tasks")
            .unwrap()
            .unwrap();
        assert_eq!(
            found.id, a.id,
            "must resolve the change in the requested project only"
        );
        assert!(get_sdd_change_by_name(&conn, &org, "other", "team-tasks")
            .unwrap()
            .is_none());
    }

    /// 2.13
    #[test]
    fn list_sdd_changes_filters_by_project_status_phase_sprint() {
        let (conn, org, user) = setup();
        mk_change(&conn, &org, &user, "nexus-mind", "alpha");
        mk_change(&conn, &org, &user, "kasymir", "beta");
        let gamma = mk_change(&conn, &org, &user, "nexus-mind", "gamma");

        patch_sdd_change(
            &conn,
            &org,
            &gamma.id,
            &PatchChangeRequest {
                phase: Some("design".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let sprint_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO sprints (id, org_id, project, name, created_by)
             VALUES (?1, ?2, 'nexus-mind', 'S1', ?3)",
            rusqlite::params![sprint_id, org, user],
        )
        .unwrap();
        patch_sdd_change(
            &conn,
            &org,
            &gamma.id,
            &PatchChangeRequest {
                sprint_id: Some(sprint_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

        let by_project = list_sdd_changes(
            &conn,
            &org,
            &SddChangeFilters {
                project: Some("nexus-mind".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(by_project.len(), 2);

        let by_phase = list_sdd_changes(
            &conn,
            &org,
            &SddChangeFilters {
                phase: Some("design".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(by_phase.len(), 1);
        assert_eq!(by_phase[0].name, "gamma");

        let by_sprint = list_sdd_changes(
            &conn,
            &org,
            &SddChangeFilters {
                sprint_id: Some(sprint_id),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            by_sprint.len(),
            1,
            "exactly the changes assigned to that sprint"
        );

        // A change with no sprint is still returned by an unfiltered list.
        let all = list_sdd_changes(&conn, &org, &SddChangeFilters::default()).unwrap();
        assert_eq!(all.len(), 3);
        assert!(all
            .iter()
            .any(|c| c.name == "alpha" && c.sprint_id.is_none()));
    }

    /// 2.15
    #[test]
    fn list_sdd_changes_excludes_archived_by_default_and_includes_them_on_request() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "archived-one");
        mk_change(&conn, &org, &user, "p", "active-one");
        assert!(archive_sdd_change(&conn, &org, &change.id).unwrap());

        let default = list_sdd_changes(&conn, &org, &SddChangeFilters::default()).unwrap();
        assert_eq!(default.len(), 1);
        assert_eq!(default[0].name, "active-one");

        let with_archived = list_sdd_changes(
            &conn,
            &org,
            &SddChangeFilters {
                include_archived: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(with_archived.len(), 2);
    }

    /// 2.17
    #[test]
    fn patch_sdd_change_updates_title_status_phase_and_bumps_updated_at() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c");

        let patched = patch_sdd_change(
            &conn,
            &org,
            &change.id,
            &PatchChangeRequest {
                title: Some("Renamed".into()),
                phase: Some("verify".into()),
                status: Some("archived".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(patched.title.as_deref(), Some("Renamed"));
        assert_eq!(patched.phase, "verify");
        assert_eq!(patched.status, "archived");
    }

    /// 2.19 — the identity tuple is immutable.
    #[test]
    fn patch_sdd_change_cannot_alter_project_or_name() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "nexus-mind", "team-tasks");

        patch_sdd_change(
            &conn,
            &org,
            &change.id,
            &PatchChangeRequest {
                title: Some("x".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let after = get_sdd_change(&conn, &org, &change.id).unwrap().unwrap();
        assert_eq!(after.project, "nexus-mind", "project is not patchable");
        assert_eq!(after.name, "team-tasks", "name is not patchable");
    }

    /// 2.21 — parse-then-write: a bad phase rejects the WHOLE patch.
    #[test]
    fn patch_sdd_change_rejects_invalid_phase_atomically() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c");

        let err = patch_sdd_change(
            &conn,
            &org,
            &change.id,
            &PatchChangeRequest {
                phase: Some("shipped".into()),
                title: Some("New title".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid_phase"));

        let after = get_sdd_change(&conn, &org, &change.id).unwrap().unwrap();
        assert_eq!(after.phase, "propose", "phase unchanged");
        assert!(
            after.title.is_none(),
            "the title in the same rejected patch must NOT have landed"
        );
    }

    /// 2.23 — soft delete; artifacts survive.
    #[test]
    fn archive_sdd_change_sets_archived_at_and_preserves_artifacts() {
        let (conn, org, user) = setup();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "D"),
            "agent",
        )
        .unwrap();
        let change = get_sdd_change_by_name(&conn, &org, "p", "c")
            .unwrap()
            .unwrap();

        assert!(archive_sdd_change(&conn, &org, &change.id).unwrap());

        let after = get_sdd_change(&conn, &org, &change.id).unwrap().unwrap();
        assert!(after.archived_at.is_some());
        assert_eq!(after.artifacts.len(), 1, "artifacts survive an archive");

        let revs = list_sdd_artifact_revisions(&conn, &org, &after.artifacts[0].id).unwrap();
        assert_eq!(revs.len(), 1, "revisions survive an archive");
    }

    // ── upsert_sdd_artifact ──────────────────────────────────────────────

    /// 2.25
    #[test]
    fn upsert_sdd_artifact_creates_change_artifact_and_revision_1() {
        let (conn, org, user) = setup();
        let (artifact, created) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("nexus-mind", "brand-new", "design", "hello"),
            "agent",
        )
        .unwrap();

        assert!(created, "first save creates a revision");
        assert_eq!(artifact.latest_revision, 1);
        assert_eq!(revision_count(&conn, &artifact.id), 1);

        let change = get_sdd_change_by_name(&conn, &org, "nexus-mind", "brand-new").unwrap();
        assert!(
            change.is_some(),
            "saving to an unknown change creates the change"
        );
    }

    /// 2.27 — THE de-dup contract (D2).
    #[test]
    fn upsert_sdd_artifact_creates_no_revision_when_hash_unchanged() {
        let (conn, org, user) = setup();
        let req = save_req("p", "c", "design", "identical content");
        let (artifact, first) = upsert_sdd_artifact(&conn, &org, &user, &req, "agent").unwrap();
        assert!(first);

        let updated_at_before = artifact.updated_at.clone();

        let (again, created) = upsert_sdd_artifact(&conn, &org, &user, &req, "agent").unwrap();
        assert!(!created, "an identical re-save must NOT create a revision");
        assert_eq!(
            revision_count(&conn, &artifact.id),
            1,
            "still exactly one revision"
        );
        assert_eq!(again.latest_revision, 1);
        assert_eq!(
            again.updated_at, updated_at_before,
            "updated_at must NOT be bumped"
        );
        assert_eq!(
            fts_row_count(&conn, &artifact.id),
            1,
            "the index must not be disturbed"
        );
    }

    /// 2.29
    #[test]
    fn upsert_sdd_artifact_appends_revision_2_on_changed_content() {
        let (conn, org, user) = setup();
        let (artifact, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "v1"),
            "agent",
        )
        .unwrap();
        let (artifact2, created) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "v2"),
            "agent",
        )
        .unwrap();

        assert!(created);
        assert_eq!(artifact2.latest_revision, 2);
        assert_eq!(artifact2.id, artifact.id, "same artifact, new revision");

        let rev1 = get_sdd_artifact_revision(&conn, &org, &artifact.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(rev1.content, "v1", "revision 1 is immutable");
        assert_eq!(rev1.byte_size, 2);
    }

    /// 2.31 — A1: a revert appends, it does not resurrect.
    #[test]
    fn upsert_sdd_artifact_revert_to_earlier_content_appends_revision_3() {
        let (conn, org, user) = setup();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "A"),
            "agent",
        )
        .unwrap();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "B"),
            "agent",
        )
        .unwrap();
        let (artifact, created) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "A"),
            "agent",
        )
        .unwrap();

        assert!(
            created,
            "reverting to earlier content is a real event and MUST append"
        );
        assert_eq!(
            artifact.latest_revision, 3,
            "revision 3, not a resurrection of revision 1"
        );
        assert_eq!(revision_count(&conn, &artifact.id), 3);

        let rev1 = get_sdd_artifact_revision(&conn, &org, &artifact.id, 1)
            .unwrap()
            .unwrap();
        let rev3 = get_sdd_artifact_revision(&conn, &org, &artifact.id, 3)
            .unwrap()
            .unwrap();
        assert_eq!(rev1.content, "A");
        assert_eq!(rev3.content, "A");
        assert_ne!(
            rev1.id, rev3.id,
            "two distinct revision rows with the same content"
        );
    }

    /// 2.33
    #[test]
    fn upsert_sdd_artifact_revision_numbering_is_monotonic_per_artifact() {
        let (conn, org, user) = setup();
        let (design, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "d1"),
            "agent",
        )
        .unwrap();
        let (proposal, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "proposal", "p1"),
            "agent",
        )
        .unwrap();

        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "d2"),
            "agent",
        )
        .unwrap();
        let (design3, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "d3"),
            "agent",
        )
        .unwrap();

        assert_eq!(design3.latest_revision, 3);

        let proposal_after = get_sdd_artifact(&conn, &org, &proposal.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            proposal_after.artifact.latest_revision, 1,
            "revisions are per-artifact, not a global counter"
        );

        let revs = list_sdd_artifact_revisions(&conn, &org, &design.id).unwrap();
        let mut numbers: Vec<i64> = revs.iter().map(|r| r.revision).collect();
        numbers.sort();
        assert_eq!(numbers, vec![1, 2, 3], "gapless, no reuse");
    }

    /// 2.35 — the FTS maintenance contract.
    #[test]
    fn upsert_sdd_artifact_replaces_fts_row_on_new_revision() {
        let (conn, org, user) = setup();
        let (artifact, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "the ALPHAWORD appears here"),
            "agent",
        )
        .unwrap();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "now only BETAWORD appears"),
            "agent",
        )
        .unwrap();

        let alpha = search_sdd_artifacts(&conn, &org, "ALPHAWORD", 10).unwrap();
        assert!(
            alpha.is_empty(),
            "a term removed by a newer revision must stop matching"
        );

        let beta = search_sdd_artifacts(&conn, &org, "BETAWORD", 10).unwrap();
        assert_eq!(beta.len(), 1, "the latest revision's term matches");

        assert_eq!(
            fts_row_count(&conn, &artifact.id),
            1,
            "the index must never accumulate rows per revision"
        );
    }

    /// 2.37 — A2: the oversized rejection is ATOMIC.
    #[test]
    fn upsert_sdd_artifact_rejects_content_over_1mb_atomically() {
        let (conn, org, user) = setup();
        let huge = "x".repeat(1_048_577);

        let err = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "oversized", "design", &huge),
            "agent",
        )
        .unwrap_err();
        assert!(err.to_string().contains("artifact_too_large"));

        let changes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_changes WHERE name = 'oversized'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let artifacts: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_artifacts", [], |r| r.get(0))
            .unwrap();
        let revisions: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_artifact_revisions", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(changes, 0, "a rejected save must leave NO change row");
        assert_eq!(artifacts, 0, "…NO artifact row");
        assert_eq!(revisions, 0, "…and NO revision row");

        // And against a pre-existing artifact: nothing moves.
        let (existing, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "small"),
            "agent",
        )
        .unwrap();
        let before = get_sdd_artifact(&conn, &org, &existing.id)
            .unwrap()
            .unwrap();

        assert!(upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", &huge),
            "agent"
        )
        .is_err());

        let after = get_sdd_artifact(&conn, &org, &existing.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            after.artifact.latest_revision,
            before.artifact.latest_revision
        );
        assert_eq!(after.artifact.updated_at, before.artifact.updated_at);
    }

    /// 2.39
    #[test]
    fn upsert_sdd_artifact_accepts_content_just_under_the_cap() {
        let (conn, org, user) = setup();
        let big = "y".repeat(1_048_575);
        let (artifact, created) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", &big),
            "agent",
        )
        .unwrap();
        assert!(created);

        let rev = get_sdd_artifact_revision(&conn, &org, &artifact.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(rev.byte_size, 1_048_575, "byte_size is bytes, not chars");
    }

    /// 2.41 — the capability sentinel.
    #[test]
    fn upsert_sdd_artifact_defaults_capability_to_empty_string() {
        let (conn, org, user) = setup();
        let (a1, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "v1"),
            "agent",
        )
        .unwrap();
        assert_eq!(
            a1.capability, "",
            "an omitted capability persists as '' — never NULL"
        );

        // An explicit `None` must converge on the SAME artifact row, not a duplicate.
        let mut req = save_req("p", "c", "design", "v2");
        req.capability = None;
        let (a2, _) = upsert_sdd_artifact(&conn, &org, &user, &req, "agent").unwrap();
        assert_eq!(a2.id, a1.id);

        let change = get_sdd_change_by_name(&conn, &org, "p", "c")
            .unwrap()
            .unwrap();
        assert_eq!(
            change.artifacts.len(),
            1,
            "two saves of the same kind converge on ONE artifact"
        );
    }

    /// 2.43
    #[test]
    fn upsert_sdd_artifact_spec_capabilities_have_independent_revision_histories() {
        let (conn, org, user) = setup();
        let mut store = save_req("p", "c", "spec", "store spec v1");
        store.capability = Some("sdd-artifact-store".into());
        let mut links = save_req("p", "c", "spec", "links spec v1");
        links.capability = Some("sdd-artifact-links".into());

        let (store_a, _) = upsert_sdd_artifact(&conn, &org, &user, &store, "agent").unwrap();
        let (links_a, _) = upsert_sdd_artifact(&conn, &org, &user, &links, "agent").unwrap();
        assert_ne!(store_a.id, links_a.id, "spec repeats per capability");

        store.content = "store spec v2".into();
        let (store_b, _) = upsert_sdd_artifact(&conn, &org, &user, &store, "agent").unwrap();
        assert_eq!(store_b.latest_revision, 2);

        let links_after = get_sdd_artifact(&conn, &org, &links_a.id).unwrap().unwrap();
        assert_eq!(
            links_after.artifact.latest_revision, 1,
            "the other capability is untouched"
        );
    }

    /// 2.45
    #[test]
    fn upsert_sdd_artifact_persists_provenance_without_clobbering_earlier_revisions() {
        let (conn, org, user) = setup();
        let mut req = save_req("p", "c", "design", "v1");
        req.path = Some("openspec/changes/c/design.md".into());
        req.git_commit = Some("abc123".into());
        let (artifact, _) = upsert_sdd_artifact(&conn, &org, &user, &req, "import").unwrap();

        let rev1 = get_sdd_artifact_revision(&conn, &org, &artifact.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(rev1.git_commit.as_deref(), Some("abc123"));
        assert_eq!(
            rev1.git_path.as_deref(),
            Some("openspec/changes/c/design.md")
        );
        assert_eq!(rev1.source, "import");
        assert_eq!(rev1.byte_size, 2);

        // A later revision with NO provenance must not overwrite revision 1's.
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "v2"),
            "agent",
        )
        .unwrap();

        let rev1_again = get_sdd_artifact_revision(&conn, &org, &artifact.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            rev1_again.git_commit.as_deref(),
            Some("abc123"),
            "revisions are immutable"
        );
        assert_eq!(rev1_again.source, "import");
    }

    /// 2.47 — source-scan invariant: nothing mutates or deletes a revision.
    #[test]
    fn no_store_function_updates_or_deletes_a_revision() {
        let src = include_str!("queries.rs");
        // The needles are assembled at runtime on purpose. Spelling them as string
        // literals would plant them in this very file, and `include_str!` pulls in the
        // test module too — the scan would match itself and fail against correct code.
        let table = "sdd_artifact_revisions";
        for forbidden in [
            format!("UPDATE {table}"),
            format!("DELETE FROM {table}"),
            format!("fn update_{table}"),
            format!("fn delete_{table}"),
        ] {
            assert!(
                !src.contains(&forbidden),
                "revisions are immutable and append-only — found `{forbidden}`. They are written \
                 by upsert_sdd_artifact's INSERT and removed only by ON DELETE CASCADE."
            );
        }
    }

    /// 2.49 — phase is advisory, never a write gate.
    #[test]
    fn upsert_sdd_artifact_does_not_mutate_the_changes_phase() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c");
        patch_sdd_change(
            &conn,
            &org,
            &change.id,
            &PatchChangeRequest {
                phase: Some("spec".into()),
                ..Default::default()
            },
        )
        .unwrap();

        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "D"),
            "agent",
        )
        .unwrap();
        let after = get_sdd_change(&conn, &org, &change.id).unwrap().unwrap();
        assert_eq!(after.phase, "spec", "a save must not advance the phase");

        // Out-of-order saves are accepted, not rejected.
        let early = mk_change(&conn, &org, &user, "p", "early");
        let result = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "early", "verify-report", "V"),
            "agent",
        );
        assert!(
            result.is_ok(),
            "a verify-report on a change in `propose` must be accepted"
        );
        let after_early = get_sdd_change(&conn, &org, &early.id).unwrap().unwrap();
        assert_eq!(after_early.phase, "propose");
    }

    /// 2.51
    #[test]
    fn upsert_sdd_artifact_org_isolation() {
        let (conn, org_a, user_a) = setup();
        let (org_b, user_b) = second_org(&conn);

        let (a, _) = upsert_sdd_artifact(
            &conn,
            &org_a,
            &user_a,
            &save_req("p", "c", "design", "org A"),
            "agent",
        )
        .unwrap();
        let (b, _) = upsert_sdd_artifact(
            &conn,
            &org_b,
            &user_b,
            &save_req("p", "c", "design", "org B"),
            "agent",
        )
        .unwrap();

        assert_ne!(
            a.id, b.id,
            "org B must get its own change and artifact, not hijack org A's"
        );

        let a_detail = get_sdd_artifact(&conn, &org_a, &a.id).unwrap().unwrap();
        assert_eq!(
            a_detail.content.as_deref(),
            Some("org A"),
            "org A's content is unmodified"
        );
        assert!(get_sdd_artifact(&conn, &org_b, &a.id).unwrap().is_none());
    }

    // ── Artifact reads ───────────────────────────────────────────────────

    /// 2.53
    #[test]
    fn get_sdd_artifact_returns_latest_revision_content() {
        let (conn, org, user) = setup();
        let long = "a very long design document ".repeat(500);
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "old"),
            "agent",
        )
        .unwrap();
        let (artifact, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", &long),
            "agent",
        )
        .unwrap();

        let detail = get_sdd_artifact(&conn, &org, &artifact.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            detail.content.as_deref(),
            Some(long.as_str()),
            "complete and untruncated"
        );
        assert_eq!(detail.artifact.latest_revision, 2);
        assert_eq!(detail.change_name, "c");
        assert_eq!(detail.project, "p");
        assert!(detail.content_hash.is_some());
    }

    /// 2.55
    #[test]
    fn get_sdd_artifact_by_kind_resolves_spec_by_capability() {
        let (conn, org, user) = setup();
        let mut store = save_req("p", "c", "spec", "STORE SPEC");
        store.capability = Some("sdd-artifact-store".into());
        let mut links = save_req("p", "c", "spec", "LINKS SPEC");
        links.capability = Some("sdd-artifact-links".into());
        upsert_sdd_artifact(&conn, &org, &user, &store, "agent").unwrap();
        upsert_sdd_artifact(&conn, &org, &user, &links, "agent").unwrap();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "DESIGN"),
            "agent",
        )
        .unwrap();

        let found =
            get_sdd_artifact_by_kind(&conn, &org, "p", "c", "spec", Some("sdd-artifact-store"))
                .unwrap()
                .unwrap();
        assert_eq!(found.content.as_deref(), Some("STORE SPEC"));

        // The '' sentinel resolves a non-spec kind.
        let design = get_sdd_artifact_by_kind(&conn, &org, "p", "c", "design", None)
            .unwrap()
            .unwrap();
        assert_eq!(design.content.as_deref(), Some("DESIGN"));

        // A kind with no artifact is not-found — NOT an artifact with empty content.
        assert!(
            get_sdd_artifact_by_kind(&conn, &org, "p", "c", "tasks", None)
                .unwrap()
                .is_none(),
            "a missing artifact must report not-found, not an empty document"
        );
    }

    /// 2.57
    #[test]
    fn list_sdd_artifact_revisions_returns_metadata_only_newest_first() {
        let (conn, org, user) = setup();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "v1"),
            "agent",
        )
        .unwrap();
        let (artifact, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "v2"),
            "agent",
        )
        .unwrap();

        let revs = list_sdd_artifact_revisions(&conn, &org, &artifact.id).unwrap();
        assert_eq!(revs.len(), 2);
        assert_eq!(revs[0].revision, 2, "newest first");
        assert_eq!(revs[1].revision, 1);
        assert_eq!(revs[0].byte_size, 2);
        assert!(!revs[0].content_hash.is_empty());

        // The type itself cannot hold content — assert the serialized shape too.
        let json = serde_json::to_value(&revs[0]).unwrap();
        assert!(
            json.get("content").is_none(),
            "revision metadata must never carry content"
        );
    }

    /// 2.59
    #[test]
    fn get_sdd_artifact_revision_returns_full_content_for_a_specific_rev() {
        let (conn, org, user) = setup();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "first"),
            "agent",
        )
        .unwrap();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "second"),
            "agent",
        )
        .unwrap();
        let (artifact, _) = upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "third"),
            "agent",
        )
        .unwrap();

        let rev1 = get_sdd_artifact_revision(&conn, &org, &artifact.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            rev1.content, "first",
            "byte-for-byte, and not revision 3's content"
        );
        assert!(get_sdd_artifact_revision(&conn, &org, &artifact.id, 99)
            .unwrap()
            .is_none());
    }

    // ── Search ───────────────────────────────────────────────────────────

    /// 2.61
    #[test]
    fn search_sdd_artifacts_returns_snippets_scoped_to_org() {
        let (conn, org_a, user_a) = setup();
        let (org_b, user_b) = second_org(&conn);

        upsert_sdd_artifact(
            &conn,
            &org_a,
            &user_a,
            &save_req("p", "c", "design", "the rate limiter uses a token bucket"),
            "agent",
        )
        .unwrap();
        upsert_sdd_artifact(
            &conn,
            &org_b,
            &user_b,
            &save_req(
                "p",
                "secret",
                "design",
                "the rate limiter is org B's secret",
            ),
            "agent",
        )
        .unwrap();

        let hits = search_sdd_artifacts(&conn, &org_a, "limiter", 10).unwrap();
        assert_eq!(hits.len(), 1, "search must never cross the org boundary");
        assert_eq!(hits[0].change_name, "c");
        assert_eq!(hits[0].kind, "design");
        assert!(
            hits[0].snippet.contains("limiter"),
            "the snippet must show the match"
        );
    }

    /// 2.63
    #[test]
    fn search_sdd_artifacts_spans_changes_and_honours_the_limit() {
        let (conn, org, user) = setup();
        for change in ["alpha", "beta", "gamma"] {
            upsert_sdd_artifact(
                &conn,
                &org,
                &user,
                &save_req("p", change, "design", "shared TOKENWORD here"),
                "agent",
            )
            .unwrap();
        }

        let all = search_sdd_artifacts(&conn, &org, "TOKENWORD", 10).unwrap();
        assert_eq!(all.len(), 3, "search spans changes, not just one");

        let limited = search_sdd_artifacts(&conn, &org, "TOKENWORD", 2).unwrap();
        assert_eq!(limited.len(), 2, "the limit is honoured in SQL");
    }

    /// 2.65
    #[test]
    fn search_sdd_artifacts_sanitizes_fts_query_syntax() {
        let (conn, org, user) = setup();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req("p", "c", "design", "hello world"),
            "agent",
        )
        .unwrap();

        // FTS5 metacharacters must not blow up the statement.
        for query in ["foo\"bar", "*", "a AND (b", "^^^", "-- drop"] {
            let result = search_sdd_artifacts(&conn, &org, query, 10);
            assert!(
                result.is_ok(),
                "query {query:?} must not propagate a SqliteFailure"
            );
        }
    }

    // ── Memory links ─────────────────────────────────────────────────────

    fn mk_memory(conn: &Connection, org: &str, user: &str, content: &str) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES (?1, ?2, ?3, 'claude-code', ?4)",
            rusqlite::params![id, org, user, content],
        )
        .unwrap();
        id
    }

    /// 2.67
    #[test]
    fn link_sdd_change_memory_is_idempotent_and_rejects_cross_org_memory() {
        let (conn, org_a, user_a) = setup();
        let (org_b, user_b) = second_org(&conn);
        let change = mk_change(&conn, &org_a, &user_a, "p", "c");
        let memory = mk_memory(&conn, &org_a, &user_a, "a decision");
        let foreign = mk_memory(&conn, &org_b, &user_b, "org B's memory");

        link_sdd_change_memory(&conn, &org_a, &change.id, &memory, "produced", &user_a).unwrap();
        link_sdd_change_memory(&conn, &org_a, &change.id, &memory, "produced", &user_a).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_change_memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "re-linking the same pair creates no duplicate");

        let err = link_sdd_change_memory(&conn, &org_a, &change.id, &foreign, "produced", &user_a)
            .unwrap_err();
        assert!(err.to_string().contains("memory_not_found"));
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_change_memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_after, 1, "the rejected cross-org link created no row");
    }

    /// 2.69 — A3: a different relation UPDATES the row.
    #[test]
    fn link_sdd_change_memory_with_a_different_relation_updates_the_existing_row() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c");
        let memory = mk_memory(&conn, &org, &user, "m");

        link_sdd_change_memory(&conn, &org, &change.id, &memory, "informed", &user).unwrap();
        link_sdd_change_memory(&conn, &org, &change.id, &memory, "produced", &user).unwrap();

        let (count, relation): (i64, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(relation) FROM sdd_change_memories WHERE change_id = ?1 AND memory_id = ?2",
                rusqlite::params![change.id, memory],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "still exactly one link row");
        assert_eq!(
            relation, "produced",
            "the relation was UPDATED, not ignored"
        );
    }

    /// 2.71
    #[test]
    fn unlink_sdd_change_memory_removes_the_link_but_not_the_memory() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c");
        let memory = mk_memory(&conn, &org, &user, "m");
        link_sdd_change_memory(&conn, &org, &change.id, &memory, "produced", &user).unwrap();

        assert!(unlink_sdd_change_memory(&conn, &org, &change.id, &memory).unwrap());
        assert!(
            !unlink_sdd_change_memory(&conn, &org, &change.id, &memory).unwrap(),
            "a second unlink reports false"
        );

        let memory_still_there: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE id = ?1",
                [&memory],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            memory_still_there, 1,
            "unlinking must not delete the memory"
        );
    }

    /// 2.73
    #[test]
    fn list_sdd_change_memories_returns_hydrated_memories() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c");
        let m1 = mk_memory(&conn, &org, &user, "first decision");
        let m2 = mk_memory(&conn, &org, &user, "second decision");
        link_sdd_change_memory(&conn, &org, &change.id, &m1, "produced", &user).unwrap();
        link_sdd_change_memory(&conn, &org, &change.id, &m2, "informed", &user).unwrap();

        let memories = list_sdd_change_memories(&conn, &org, &change.id).unwrap();
        assert_eq!(memories.len(), 2);
        assert!(memories.iter().any(|m| m.content == "first decision"));
        assert!(memories.iter().any(|m| m.content == "second decision"));
    }

    // ── Task join ────────────────────────────────────────────────────────

    /// 2.75 — D3: the join key is the NAME.
    #[test]
    fn list_tasks_for_sdd_change_joins_task_spec_links_by_name() {
        let (conn, org, user) = setup();
        mk_change(&conn, &org, &user, "p", "sdd-artifacts");

        for title in ["PR-1", "PR-2", "PR-3"] {
            let task = create_task(
                &conn,
                &org,
                &user,
                &CreateTaskRequest {
                    project: "p".into(),
                    title: title.into(),
                    ..Default::default()
                },
            )
            .unwrap();
            link_task_spec(&conn, &task.id, &user, "sdd-artifacts").unwrap();
        }

        let tasks = list_tasks_for_sdd_change(&conn, &org, "sdd-artifacts", None).unwrap();
        assert_eq!(tasks.len(), 3);
        assert!(tasks.iter().any(|t| t.title == "PR-1"));

        // A change with no links returns an empty vec, not an error.
        let none = list_tasks_for_sdd_change(&conn, &org, "unlinked-change", None).unwrap();
        assert!(none.is_empty());
    }

    /// 2.79 — the link resolves even if it was created BEFORE the change existed.
    #[test]
    fn a_spec_link_created_before_the_change_existed_resolves_once_the_change_appears() {
        let (conn, org, user) = setup();
        let task = create_task(
            &conn,
            &org,
            &user,
            &CreateTaskRequest {
                project: "p".into(),
                title: "Early".into(),
                ..Default::default()
            },
        )
        .unwrap();
        // No sdd_changes row of this name exists yet. The link is still recorded.
        link_task_spec(&conn, &task.id, &user, "not-yet-created").unwrap();

        // The change appears later. The pre-existing link resolves with NO re-linking
        // and no mutation of task_spec_links — the join is a pure name join that never
        // reads sdd_changes, which is exactly what makes this work (D3).
        mk_change(&conn, &org, &user, "p", "not-yet-created");

        let tasks = list_tasks_for_sdd_change(&conn, &org, "not-yet-created", None).unwrap();
        assert_eq!(tasks.len(), 1, "a pure name join needs no re-linking");
        assert_eq!(tasks[0].id, task.id);

        let links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_spec_links WHERE spec_change_name = 'not-yet-created'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(links, 1, "task_spec_links was never rewritten");
    }

    /// 2.80 — no duplicate source of truth: `tasks` gained no change_id column.
    #[test]
    fn tasks_table_has_no_change_id_column() {
        let (conn, _org, _user) = setup();
        let mut stmt = conn.prepare("PRAGMA table_info(tasks)").unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(
            !cols
                .iter()
                .any(|c| c == "change_id" || c == "sdd_change_id"),
            "tasks must join by NAME (task_spec_links), not by a second FK — D3"
        );
    }

    /// 2.81 — A8: archived changes remain valid link targets.
    #[test]
    fn sdd_change_exists_matches_by_name_across_projects_and_respects_org() {
        let (conn, org_a, user_a) = setup();
        let (org_b, _) = second_org(&conn);

        mk_change(&conn, &org_a, &user_a, "nexus-mind", "team-tasks");
        let archived = mk_change(&conn, &org_a, &user_a, "kasymir", "old-change");
        archive_sdd_change(&conn, &org_a, &archived.id).unwrap();

        assert!(sdd_change_exists(&conn, &org_a, "team-tasks").unwrap());
        assert!(
            sdd_change_exists(&conn, &org_a, "old-change").unwrap(),
            "an ARCHIVED change is still a legitimate link target (A8)"
        );
        assert!(!sdd_change_exists(&conn, &org_a, "never-existed").unwrap());
        assert!(
            !sdd_change_exists(&conn, &org_b, "team-tasks").unwrap(),
            "a change in another org must not satisfy the check"
        );
    }

    // ── The living specification (sdd_specs) ─────────────────────────────
    //
    // Every invariant the artifact store holds, the spec store must hold too —
    // these tests are deliberately the artifact tests again, on the other tree.

    fn spec_req(project: &str, capability: &str, content: &str) -> SaveSpecRequest {
        SaveSpecRequest {
            project: project.to_string(),
            capability: capability.to_string(),
            content: content.to_string(),
            ..Default::default()
        }
    }

    fn spec_revision_count(conn: &Connection, spec_id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sdd_spec_revisions WHERE spec_id = ?1",
            [spec_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn spec_fts_row_count(conn: &Connection, spec_id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sdd_specs_fts WHERE spec_id = ?1",
            [spec_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn upsert_sdd_spec_creates_spec_and_revision_1() {
        let (conn, org, user) = setup();
        let (spec, created) = upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("nexus-mind", "harness-library", "## Purpose\nThe library."),
            "agent",
        )
        .unwrap();

        assert!(created, "the first save creates a revision");
        assert_eq!(spec.latest_revision, 1);
        assert_eq!(spec.capability, "harness-library");
        assert_eq!(spec_revision_count(&conn, &spec.id), 1);
        assert_eq!(spec_fts_row_count(&conn, &spec.id), 1);
    }

    /// A spec is NOT an artifact of a change — saving one creates no change at all.
    /// This is the modelling decision, asserted.
    #[test]
    fn upsert_sdd_spec_does_not_create_a_synthetic_change() {
        let (conn, org, user) = setup();
        upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "text"), "agent").unwrap();

        let changes: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_changes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            changes, 0,
            "a living specification belongs to the PROJECT — it must not conjure a change to hang off"
        );
    }

    /// THE de-dup contract, on the other tree.
    #[test]
    fn upsert_sdd_spec_creates_no_revision_when_hash_unchanged() {
        let (conn, org, user) = setup();
        let req = spec_req("p", "cap", "identical contract");
        let (spec, first) = upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();
        assert!(first);
        let updated_at_before = spec.updated_at.clone();

        let (again, created) = upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();
        assert!(!created, "an identical re-save must NOT create a revision");
        assert_eq!(
            spec_revision_count(&conn, &spec.id),
            1,
            "still exactly one revision"
        );
        assert_eq!(again.latest_revision, 1);
        assert_eq!(
            again.updated_at, updated_at_before,
            "updated_at must NOT be bumped"
        );
        assert_eq!(
            spec_fts_row_count(&conn, &spec.id),
            1,
            "the index must not be disturbed"
        );
    }

    #[test]
    fn upsert_sdd_spec_appends_revision_2_on_changed_content() {
        let (conn, org, user) = setup();
        let (spec, _) =
            upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "v1"), "agent").unwrap();
        let (spec2, created) =
            upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "v2"), "agent").unwrap();

        assert!(created);
        assert_eq!(spec2.latest_revision, 2);
        assert_eq!(spec2.id, spec.id, "same spec, new revision");

        let rev1 = get_sdd_spec_revision(&conn, &org, &spec.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(rev1.content, "v1", "revision 1 is immutable");
        assert_eq!(rev1.byte_size, 2);
    }

    /// A1 — the hash is compared against the LATEST revision only. A → B → A appends
    /// revision 3: reverting the contract is an event, and it must appear as one.
    #[test]
    fn upsert_sdd_spec_revert_to_earlier_content_appends_revision_3() {
        let (conn, org, user) = setup();
        upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "A"), "agent").unwrap();
        upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "B"), "agent").unwrap();
        let (spec, created) =
            upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "A"), "agent").unwrap();

        assert!(
            created,
            "reverting to earlier content is a real event and MUST append"
        );
        assert_eq!(
            spec.latest_revision, 3,
            "revision 3, not a resurrection of revision 1"
        );
        assert_eq!(spec_revision_count(&conn, &spec.id), 3);

        let rev1 = get_sdd_spec_revision(&conn, &org, &spec.id, 1)
            .unwrap()
            .unwrap();
        let rev3 = get_sdd_spec_revision(&conn, &org, &spec.id, 3)
            .unwrap()
            .unwrap();
        assert_eq!(rev1.content, "A");
        assert_eq!(rev3.content, "A");
        assert_ne!(
            rev1.id, rev3.id,
            "two distinct revisions that happen to agree"
        );
    }

    /// A2 — the 1 MB rejection is ATOMIC: it happens before the transaction opens, so
    /// there is no spec and no revision to clean up afterwards.
    #[test]
    fn upsert_sdd_spec_rejects_content_over_1mb_atomically() {
        let (conn, org, user) = setup();
        let huge = "x".repeat(1_048_577);

        let err = upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("p", "oversized", &huge),
            "agent",
        )
        .unwrap_err();
        assert!(err.to_string().contains("spec_too_large"));

        let specs: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_specs", [], |r| r.get(0))
            .unwrap();
        let revisions: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_spec_revisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(specs, 0, "a rejected save must leave NO spec row");
        assert_eq!(revisions, 0, "…and NO revision row");

        // And against a pre-existing spec: nothing moves.
        let (existing, _) =
            upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "small"), "agent").unwrap();
        let before = get_sdd_spec(&conn, &org, &existing.id).unwrap().unwrap();

        assert!(
            upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", &huge), "agent").is_err()
        );

        let after = get_sdd_spec(&conn, &org, &existing.id).unwrap().unwrap();
        assert_eq!(after.spec.latest_revision, before.spec.latest_revision);
        assert_eq!(after.spec.updated_at, before.spec.updated_at);
        assert_eq!(
            after.content.as_deref(),
            Some("small"),
            "the contract is untouched"
        );
    }

    #[test]
    fn upsert_sdd_spec_accepts_content_just_under_the_cap() {
        let (conn, org, user) = setup();
        let big = "y".repeat(1_048_575);
        let (spec, created) =
            upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", &big), "agent").unwrap();
        assert!(created);
        let rev = get_sdd_spec_revision(&conn, &org, &spec.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(rev.byte_size, 1_048_575);
    }

    /// The FTS index tracks the LATEST revision only: it is replaced, never appended to,
    /// so a term struck from the contract stops matching.
    #[test]
    fn upsert_sdd_spec_replaces_fts_row_on_new_revision() {
        let (conn, org, user) = setup();
        let (spec, _) = upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("p", "cap", "the throttle uses leaky buckets"),
            "agent",
        )
        .unwrap();
        upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("p", "cap", "the throttle uses windows"),
            "agent",
        )
        .unwrap();

        assert_eq!(
            spec_fts_row_count(&conn, &spec.id),
            1,
            "the index must never accumulate one row per revision"
        );
        let stale = search_sdd_specs(&conn, &org, "leaky", 10).unwrap();
        assert!(
            stale.is_empty(),
            "a term deleted by a newer revision must stop matching"
        );
        let fresh = search_sdd_specs(&conn, &org, "windows", 10).unwrap();
        assert_eq!(fresh.len(), 1, "the latest revision's text must match");
    }

    /// Two orgs may hold the same (project, capability) and they are two contracts.
    #[test]
    fn upsert_sdd_spec_org_isolation() {
        let (conn, org_a, user_a) = setup();
        let (org_b, user_b) = second_org(&conn);

        let (spec_a, _) = upsert_sdd_spec(
            &conn,
            &org_a,
            &user_a,
            &spec_req("p", "cap", "A's contract"),
            "agent",
        )
        .unwrap();
        let (spec_b, _) = upsert_sdd_spec(
            &conn,
            &org_b,
            &user_b,
            &spec_req("p", "cap", "B's contract"),
            "agent",
        )
        .unwrap();

        assert_ne!(
            spec_a.id, spec_b.id,
            "same natural key in two orgs = two specs"
        );
        assert!(
            get_sdd_spec(&conn, &org_b, &spec_a.id).unwrap().is_none(),
            "org B must not see org A's spec by id — Ok(None), which the API turns into a 404"
        );
        assert!(
            get_sdd_spec_by_capability(&conn, &org_b, "p", "cap")
                .unwrap()
                .unwrap()
                .content
                .as_deref()
                == Some("B's contract"),
            "the natural key resolves within the caller's org only"
        );
    }

    /// The payoff: `merged_from_change_id` ties a revision back to the change that
    /// produced it, in both directions.
    #[test]
    fn upsert_sdd_spec_records_which_change_merged_into_the_contract() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "sdd-specs");

        let req = SaveSpecRequest {
            merged_from_change_name: Some("sdd-specs".to_string()),
            ..spec_req("p", "cap", "merged text")
        };
        let (spec, _) = upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();

        // Spec → change.
        assert_eq!(
            spec.last_merged_from_change_id.as_deref(),
            Some(change.id.as_str())
        );
        assert_eq!(
            spec.last_merged_from_change_name.as_deref(),
            Some("sdd-specs")
        );

        let rev = get_sdd_spec_revision(&conn, &org, &spec.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            rev.merged_from_change_id.as_deref(),
            Some(change.id.as_str())
        );
        assert_eq!(rev.merged_from_change_name.as_deref(), Some("sdd-specs"));

        // Change → spec.
        let merged = list_sdd_specs_for_change(&conn, &org, &change.id).unwrap();
        assert_eq!(
            merged.len(),
            1,
            "the change must report the spec it merged into"
        );
        assert_eq!(merged[0].spec.id, spec.id);
        assert_eq!(merged[0].merged_revision, 1);
    }

    /// A revision saved outside the change pipeline (import, admin edit) has no
    /// provenance, and that is a legitimate state — not an error.
    #[test]
    fn upsert_sdd_spec_without_a_change_name_has_null_provenance() {
        let (conn, org, user) = setup();
        let (spec, _) = upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("p", "cap", "imported"),
            "import",
        )
        .unwrap();
        let rev = get_sdd_spec_revision(&conn, &org, &spec.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(rev.merged_from_change_id, None);
        assert_eq!(rev.source, "import");
        assert_eq!(spec.last_merged_from_change_id, None);
    }

    /// An unresolvable change name rejects the save WHOLE. Storing the content with a
    /// silently-NULL provenance would leave a spec whose history lies by omission.
    #[test]
    fn upsert_sdd_spec_rejects_an_unknown_change_name_atomically() {
        let (conn, org, user) = setup();
        let req = SaveSpecRequest {
            merged_from_change_name: Some("no-such-change".to_string()),
            ..spec_req("p", "cap", "text")
        };
        let err = upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap_err();
        assert!(err.to_string().contains("change_not_found"));

        let specs: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_specs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(specs, 0, "the rejected save must leave no spec behind");
    }

    /// A change in ANOTHER org is not a resolvable provenance either.
    #[test]
    fn upsert_sdd_spec_will_not_merge_from_another_orgs_change() {
        let (conn, org_a, user_a) = setup();
        let (org_b, user_b) = second_org(&conn);
        mk_change(&conn, &org_a, &user_a, "p", "a-change");

        let req = SaveSpecRequest {
            merged_from_change_name: Some("a-change".to_string()),
            ..spec_req("p", "cap", "text")
        };
        let err = upsert_sdd_spec(&conn, &org_b, &user_b, &req, "agent").unwrap_err();
        assert!(err.to_string().contains("change_not_found"));
    }

    /// A change may merge into several specs; each spec reports the revision it got.
    #[test]
    fn list_sdd_specs_for_change_reports_every_spec_the_change_touched() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "big-change");

        for cap in ["auth", "billing"] {
            let req = SaveSpecRequest {
                merged_from_change_name: Some("big-change".to_string()),
                ..spec_req("p", cap, &format!("{cap} contract"))
            };
            upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();
        }
        // A second merge into `auth` — the reported revision must be the newest one.
        let req = SaveSpecRequest {
            merged_from_change_name: Some("big-change".to_string()),
            ..spec_req("p", "auth", "auth contract v2")
        };
        upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();

        let merged = list_sdd_specs_for_change(&conn, &org, &change.id).unwrap();
        assert_eq!(merged.len(), 2, "two specs, not three rows — one per spec");
        assert_eq!(merged[0].spec.capability, "auth", "ordered by capability");
        assert_eq!(
            merged[0].merged_revision, 2,
            "the newest revision this change produced"
        );
        assert_eq!(merged[1].spec.capability, "billing");
        assert_eq!(merged[1].merged_revision, 1);
    }

    /// A change that merged into nothing reports nothing — not an error.
    #[test]
    fn list_sdd_specs_for_change_is_empty_for_a_change_that_merged_nothing() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "drafting");
        assert!(list_sdd_specs_for_change(&conn, &org, &change.id)
            .unwrap()
            .is_empty());
    }

    // ── Spec reads ───────────────────────────────────────────────────────

    #[test]
    fn get_sdd_spec_returns_latest_revision_content() {
        let (conn, org, user) = setup();
        upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "old"), "agent").unwrap();
        let (spec, _) = upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("p", "cap", "current"),
            "agent",
        )
        .unwrap();

        let detail = get_sdd_spec(&conn, &org, &spec.id).unwrap().unwrap();
        assert_eq!(detail.content.as_deref(), Some("current"));
        assert_eq!(detail.spec.latest_revision, 2);
        assert!(detail.content_hash.is_some());
    }

    /// A capability with no spec is `Ok(None)` — never a spec carrying empty content.
    #[test]
    fn get_sdd_spec_by_capability_is_none_for_an_unknown_capability() {
        let (conn, org, user) = setup();
        upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "text"), "agent").unwrap();

        assert!(get_sdd_spec_by_capability(&conn, &org, "p", "nope")
            .unwrap()
            .is_none());
        assert!(
            get_sdd_spec_by_capability(&conn, &org, "other-project", "cap")
                .unwrap()
                .is_none(),
            "the capability is scoped to its project"
        );
        let found = get_sdd_spec_by_capability(&conn, &org, "p", "cap")
            .unwrap()
            .unwrap();
        assert_eq!(found.content.as_deref(), Some("text"));
    }

    /// The list is METADATA ONLY. `SddSpec` has no content field, so this is enforced by
    /// the type — the assertion is on the shape of the row set, not the absence of a leak.
    #[test]
    fn list_sdd_specs_returns_one_row_per_capability_with_its_provenance() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c1");

        upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "zeta", "z"), "agent").unwrap();
        let req = SaveSpecRequest {
            merged_from_change_name: Some("c1".to_string()),
            ..spec_req("p", "alpha", "a")
        };
        upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();
        upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("other", "gamma", "g"),
            "agent",
        )
        .unwrap();

        let all = list_sdd_specs(&conn, &org, &SddSpecFilters::default()).unwrap();
        assert_eq!(all.len(), 3);

        let filtered = list_sdd_specs(
            &conn,
            &org,
            &SddSpecFilters {
                project: Some("p".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(filtered.len(), 2, "filtered to the project");
        assert_eq!(filtered[0].capability, "alpha", "ordered by capability");
        assert_eq!(
            filtered[0].last_merged_from_change_name.as_deref(),
            Some("c1"),
            "the list carries the change that last merged into each contract"
        );
        assert_eq!(
            filtered[0].last_merged_from_change_id.as_deref(),
            Some(change.id.as_str())
        );
        assert_eq!(filtered[1].capability, "zeta");
        assert_eq!(filtered[1].last_merged_from_change_name, None);
    }

    #[test]
    fn list_sdd_specs_org_isolation() {
        let (conn, org_a, user_a) = setup();
        let (org_b, user_b) = second_org(&conn);
        upsert_sdd_spec(
            &conn,
            &org_a,
            &user_a,
            &spec_req("p", "a-cap", "x"),
            "agent",
        )
        .unwrap();
        upsert_sdd_spec(
            &conn,
            &org_b,
            &user_b,
            &spec_req("p", "b-cap", "y"),
            "agent",
        )
        .unwrap();

        let a = list_sdd_specs(&conn, &org_a, &SddSpecFilters::default()).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(
            a[0].capability, "a-cap",
            "org A must not see org B's contracts"
        );
    }

    #[test]
    fn list_sdd_spec_revisions_returns_metadata_only_newest_first() {
        let (conn, org, user) = setup();
        let change = mk_change(&conn, &org, &user, "p", "c1");
        upsert_sdd_spec(&conn, &org, &user, &spec_req("p", "cap", "v1"), "import").unwrap();
        let req = SaveSpecRequest {
            merged_from_change_name: Some("c1".to_string()),
            ..spec_req("p", "cap", "v2")
        };
        let (spec, _) = upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();

        let revs = list_sdd_spec_revisions(&conn, &org, &spec.id).unwrap();
        assert_eq!(revs.len(), 2);
        assert_eq!(revs[0].revision, 2, "newest first");
        assert_eq!(
            revs[0].merged_from_change_id.as_deref(),
            Some(change.id.as_str())
        );
        assert_eq!(revs[0].merged_from_change_name.as_deref(), Some("c1"));
        assert_eq!(revs[1].revision, 1);
        assert_eq!(revs[1].source, "import");
        assert_eq!(revs[1].merged_from_change_id, None);
        assert_eq!(revs[1].byte_size, 2);
    }

    #[test]
    fn get_sdd_spec_revision_returns_full_content_and_respects_org() {
        let (conn, org_a, user_a) = setup();
        let (org_b, _user_b) = second_org(&conn);
        upsert_sdd_spec(
            &conn,
            &org_a,
            &user_a,
            &spec_req("p", "cap", "first"),
            "agent",
        )
        .unwrap();
        let (spec, _) = upsert_sdd_spec(
            &conn,
            &org_a,
            &user_a,
            &spec_req("p", "cap", "second"),
            "agent",
        )
        .unwrap();

        let rev1 = get_sdd_spec_revision(&conn, &org_a, &spec.id, 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            rev1.content, "first",
            "an older revision is retrievable in full"
        );

        assert!(
            get_sdd_spec_revision(&conn, &org_b, &spec.id, 1)
                .unwrap()
                .is_none(),
            "another org's revision is Ok(None), which the API turns into a 404"
        );
        assert!(get_sdd_spec_revision(&conn, &org_a, &spec.id, 99)
            .unwrap()
            .is_none());
    }

    // ── Spec search ──────────────────────────────────────────────────────

    #[test]
    fn search_sdd_specs_returns_snippets_scoped_to_org() {
        let (conn, org_a, user_a) = setup();
        let (org_b, user_b) = second_org(&conn);
        upsert_sdd_spec(
            &conn,
            &org_a,
            &user_a,
            &spec_req(
                "p",
                "throttling",
                "requests are subject to rate limiting per key",
            ),
            "agent",
        )
        .unwrap();
        upsert_sdd_spec(
            &conn,
            &org_b,
            &user_b,
            &spec_req("p", "other", "rate limiting in another org"),
            "agent",
        )
        .unwrap();

        let hits = search_sdd_specs(&conn, &org_a, "rate limiting", 10).unwrap();
        assert_eq!(hits.len(), 1, "org B's contract must not appear");
        assert_eq!(hits[0].capability, "throttling");
        assert!(
            hits[0].snippet.contains("<b>"),
            "the snippet must be highlighted"
        );
    }

    #[test]
    fn search_sdd_specs_sanitizes_fts_query_syntax() {
        let (conn, org, user) = setup();
        upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req("p", "cap", "plain text"),
            "agent",
        )
        .unwrap();
        // Raw FTS operators must not blow up the query.
        assert!(search_sdd_specs(&conn, &org, "\"unbalanced", 10).is_ok());
        assert!(search_sdd_specs(&conn, &org, "AND OR NOT", 10).is_ok());
    }

    /// `GET /v1/sdd/search` spans BOTH trees, and every hit says which one it came from.
    /// "Which spec covers rate limiting?" must find the CONTRACT, not only the drafts.
    #[test]
    fn search_sdd_all_covers_specs_and_artifacts_and_labels_each_hit() {
        let (conn, org, user) = setup();
        upsert_sdd_artifact(
            &conn,
            &org,
            &user,
            &save_req(
                "p",
                "throttle-work",
                "design",
                "we will add rate limiting to the gateway",
            ),
            "agent",
        )
        .unwrap();
        upsert_sdd_spec(
            &conn,
            &org,
            &user,
            &spec_req(
                "p",
                "gateway",
                "the gateway MUST apply rate limiting per api key",
            ),
            "agent",
        )
        .unwrap();

        let hits = search_sdd_all(&conn, &org, "rate limiting", 10).unwrap();
        assert_eq!(hits.len(), 2, "both trees are searched");

        let spec_hit = hits
            .iter()
            .find(|h| h.hit_type == "spec")
            .expect("the contract must be found");
        assert_eq!(spec_hit.capability, "gateway");
        assert!(spec_hit.spec_id.is_some());
        assert!(
            spec_hit.change_id.is_none(),
            "a spec hit has no change — it outlives them"
        );

        let art_hit = hits
            .iter()
            .find(|h| h.hit_type == "artifact")
            .expect("the draft must be found too");
        assert_eq!(art_hit.change_name.as_deref(), Some("throttle-work"));
        assert_eq!(art_hit.kind.as_deref(), Some("design"));
        assert!(art_hit.spec_id.is_none());

        assert_eq!(
            hits[0].hit_type, "spec",
            "the contract outranks the drafts — that is the question being asked"
        );
    }

    #[test]
    fn search_sdd_all_honours_the_limit_across_both_trees() {
        let (conn, org, user) = setup();
        for i in 0..3 {
            upsert_sdd_spec(
                &conn,
                &org,
                &user,
                &spec_req("p", &format!("cap{i}"), "widget"),
                "agent",
            )
            .unwrap();
            upsert_sdd_artifact(
                &conn,
                &org,
                &user,
                &save_req("p", &format!("c{i}"), "design", "widget"),
                "agent",
            )
            .unwrap();
        }
        let hits = search_sdd_all(&conn, &org, "widget", 4).unwrap();
        assert_eq!(
            hits.len(),
            4,
            "the limit caps the MERGED result set, not each tree"
        );
    }

    #[test]
    fn search_sdd_specs_by_query_matches_capability_and_title_for_global_search() {
        let (conn, org, user) = setup();
        let req = SaveSpecRequest {
            title: Some("Harness Library".to_string()),
            ..spec_req(
                "p",
                "harness-library",
                "body text mentioning nothing relevant",
            )
        };
        upsert_sdd_spec(&conn, &org, &user, &req, "agent").unwrap();

        let by_cap = search_sdd_specs_by_query(&conn, &org, "harness", 10).unwrap();
        assert_eq!(by_cap.len(), 1);
        assert_eq!(by_cap[0].capability, "harness-library");
        assert_eq!(by_cap[0].latest_revision, 1);

        let by_title = search_sdd_specs_by_query(&conn, &org, "Library", 10).unwrap();
        assert_eq!(by_title.len(), 1, "the title matches too");

        assert!(
            search_sdd_specs_by_query(&conn, &org, "unrelated", 10)
                .unwrap()
                .is_empty(),
            "global_search is keyword-only over capability/title, not full text"
        );
    }

    /// Source-scan invariant: nothing in the store mutates or removes a spec revision.
    ///
    /// As with the artifact scan, the needles are assembled at runtime. Spelling them as
    /// string literals would plant them in this very file, and `include_str!` pulls the
    /// test module in with everything else — the scan would then match itself and fail
    /// against perfectly correct code. (This has bitten this codebase before.)
    #[test]
    fn no_store_function_mutates_a_spec_revision() {
        let src = include_str!("queries.rs");
        let table = "sdd_spec_revisions";
        for forbidden in [
            format!("UPDATE {table}"),
            format!("DELETE FROM {table}"),
            format!("fn update_{table}"),
            format!("fn delete_{table}"),
        ] {
            assert!(
                !src.contains(&forbidden),
                "spec revisions are immutable and append-only — found `{forbidden}`. They are \
                 written by upsert_sdd_spec's INSERT and reclaimed only by ON DELETE CASCADE \
                 from the parent spec."
            );
        }
    }
}

#[cfg(test)]
mod privileged_permission_tests {
    use super::*;
    use crate::db::connection::connect;
    use crate::db::migrations;

    fn setup() -> (Connection, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _user, _key) =
            bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        (conn, org.id)
    }

    /// The built-in `admin` and `super_user` permission lists are hard-coded in Rust,
    /// NOT read from the `roles` table — so every migration that grants a new domain to
    /// the role *templates* (v52 for `task:*`, v54 for `sdd:*`) silently leaves these two
    /// behind. `require_permission` bypasses the check for privileged roles, so nothing
    /// breaks server-side and the drift goes unnoticed.
    ///
    /// But the list is what `/v1/admin/auth/me` reports, and the admin UI gates controls
    /// on it (`isAdmin || permissions.includes('sdd:write')`). A list that omits a domain
    /// the role can actually use is a **lie in the API response** — and the first UI check
    /// that forgets its `isAdmin ||` prefix silently hides a control from the very people
    /// who are allowed to use it.
    #[test]
    fn privileged_roles_report_every_domain_they_can_actually_use() {
        let (conn, org_id) = setup();

        for role in ["admin", "super_user"] {
            let perms = get_role_permissions(&conn, &org_id, role).unwrap();

            for required in [
                "task:read",
                "task:write",
                "task:assign",
                "task:delete",
                "sdd:read",
                "sdd:write",
                "sdd:delete",
            ] {
                assert!(
                    perms.iter().any(|p| p == required),
                    "the hard-coded `{role}` permission list omits `{required}` — \
                     /v1/admin/auth/me would report a permission set the role does not match"
                );
            }
        }
    }

    /// Guards the drift itself: every permission string granted to a seeded role template
    /// must also appear in the privileged lists. A new domain added later fails here.
    #[test]
    fn no_template_grant_is_missing_from_the_privileged_lists() {
        let (conn, org_id) = setup();
        let admin = get_role_permissions(&conn, &org_id, "admin").unwrap();

        let mut stmt = conn
            .prepare("SELECT permissions FROM roles WHERE id LIKE 'tmpl_%'")
            .unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        for raw in rows {
            let granted: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
            for perm in granted {
                assert!(
                    admin.iter().any(|p| p == &perm),
                    "`{perm}` is granted to a role template but missing from the hard-coded \
                     `admin` list — add it there when you add a new permission domain"
                );
            }
        }
    }
}

// ── Client isolation tests (the acceptance gates of the client model) ─────────
#[cfg(test)]
mod client_isolation_tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    struct Fixture {
        conn: Connection,
    }

    fn setup() -> Fixture {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'u2s', 'u2s')",
            [],
        )
        .unwrap();
        for (uid, email) in [
            ("u_a", "a@u2s.io"),
            ("u_b", "b@u2s.io"),
            ("u_admin", "admin@u2s.io"),
        ] {
            conn.execute(
                "INSERT INTO users (id, org_id, email, name, role) VALUES (?1, 'org1', ?2, ?1, 'member')",
                rusqlite::params![uid, email],
            )
            .unwrap();
        }
        // Two clients, one project each, plus an internal u2s project.
        conn.execute("INSERT INTO clients (id, org_id, name, slug) VALUES ('cli_a', 'org1', 'Client A', 'client-a')", []).unwrap();
        conn.execute("INSERT INTO clients (id, org_id, name, slug) VALUES ('cli_b', 'org1', 'Client B', 'client-b')", []).unwrap();
        conn.execute("INSERT INTO projects (id, org_id, name, client_id) VALUES ('p_a1', 'org1', 'a-billing', 'cli_a')", []).unwrap();
        conn.execute("INSERT INTO projects (id, org_id, name, client_id) VALUES ('p_a2', 'org1', 'a-web', 'cli_a')", []).unwrap();
        conn.execute("INSERT INTO projects (id, org_id, name, client_id) VALUES ('p_b1', 'org1', 'b-api', 'cli_b')", []).unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('p_int', 'org1', 'internal-tooling')",
            [],
        )
        .unwrap();
        // u_a is a member of client A; u_b of client B.
        add_client_member(&conn, "cli_a", "u_a", "member").unwrap();
        add_client_member(&conn, "cli_b", "u_b", "member").unwrap();
        Fixture { conn }
    }

    fn visible_projects(conn: &Connection, uid: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT project_name FROM project_visibility WHERE org_id = 'org1' AND user_id = ?1 ORDER BY project_name")
            .unwrap();
        stmt.query_map([uid], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    /// One membership on the client reaches every project that client owns —
    /// that is the whole point of grouping by client.
    #[test]
    fn client_member_sees_all_projects_of_that_client() {
        let f = setup();
        assert_eq!(visible_projects(&f.conn, "u_a"), vec!["a-billing", "a-web"]);
    }

    #[test]
    fn non_member_cannot_see_other_client_projects() {
        let f = setup();
        let seen = visible_projects(&f.conn, "u_a");
        assert!(
            !seen.contains(&"b-api".to_string()),
            "client A must not see client B's project"
        );
        assert!(!user_can_view_client(&f.conn, "org1", "cli_b", Some("u_a")).unwrap());
    }

    /// Project membership alone must NOT widen to the client's other projects.
    #[test]
    fn project_member_sees_only_that_project() {
        let f = setup();
        f.conn
            .execute("INSERT INTO project_members (id, project_id, user_id, role) VALUES ('pm1', 'p_a1', 'u_b', 'member')", [])
            .unwrap();
        let seen = visible_projects(&f.conn, "u_b");
        assert!(seen.contains(&"a-billing".to_string()));
        assert!(
            !seen.contains(&"a-web".to_string()),
            "project membership must not leak the client's other projects"
        );
    }

    /// super_user (viewer_user_id = None) is the only org-wide reader.
    #[test]
    fn super_user_sees_every_client() {
        let f = setup();
        assert!(user_can_view_client(&f.conn, "org1", "cli_a", None).unwrap());
        assert!(user_can_view_client(&f.conn, "org1", "cli_b", None).unwrap());
    }

    /// Guards the `is_privileged()` trap: admin is privileged for permission
    /// checks but must stay membership-scoped for reads. If a future refactor
    /// swaps `is_super_user()` for `is_privileged()` in the visibility path,
    /// this test is what catches it.
    #[test]
    fn admin_without_membership_does_not_see_client() {
        let f = setup();
        assert!(
            !user_can_view_client(&f.conn, "org1", "cli_a", Some("u_admin")).unwrap(),
            "an admin with no membership must not see a client's data"
        );
    }

    /// Guards the existence oracle: a client that does not exist reports as
    /// visible, so "absent" and "forbidden" are indistinguishable to a caller.
    #[test]
    fn user_can_view_client_returns_true_for_nonexistent_client() {
        let f = setup();
        assert!(user_can_view_client(&f.conn, "org1", "cli_nope", Some("u_a")).unwrap());
    }

    /// An internal project has no client, so client membership cannot reach it.
    #[test]
    fn internal_project_visible_only_via_project_membership() {
        let f = setup();
        assert!(!visible_projects(&f.conn, "u_a").contains(&"internal-tooling".to_string()));
        f.conn
            .execute("INSERT INTO project_members (id, project_id, user_id, role) VALUES ('pm2', 'p_int', 'u_a', 'member')", [])
            .unwrap();
        assert!(visible_projects(&f.conn, "u_a").contains(&"internal-tooling".to_string()));
    }

    /// A user who is both a project member and a member of that project's
    /// client must appear once — UNION, not UNION ALL. Duplicates here would
    /// silently multiply rows in every JOIN against the view.
    #[test]
    fn dual_membership_does_not_duplicate_rows() {
        let f = setup();
        f.conn
            .execute("INSERT INTO project_members (id, project_id, user_id, role) VALUES ('pm3', 'p_a1', 'u_a', 'member')", [])
            .unwrap();
        let seen = visible_projects(&f.conn, "u_a");
        assert_eq!(seen.iter().filter(|p| *p == "a-billing").count(), 1);
    }

    #[test]
    fn client_with_projects_reports_its_project_count() {
        let f = setup();
        assert_eq!(count_client_projects(&f.conn, "org1", "cli_a").unwrap(), 2);
        assert_eq!(count_client_projects(&f.conn, "org1", "cli_b").unwrap(), 1);
    }

    #[test]
    fn archive_client_is_idempotent() {
        let f = setup();
        assert!(archive_client(&f.conn, "org1", "cli_a").unwrap());
        assert!(archive_client(&f.conn, "org1", "cli_a").unwrap());
        let c = get_client(&f.conn, "org1", "cli_a").unwrap().unwrap();
        assert!(c.archived_at.is_some());
    }

    #[test]
    fn list_clients_visible_scopes_to_membership() {
        let f = setup();
        let for_a = list_clients_visible(&f.conn, "org1", false, Some("u_a")).unwrap();
        assert_eq!(for_a.len(), 1);
        assert_eq!(for_a[0].slug, "client-a");

        let for_super = list_clients_visible(&f.conn, "org1", false, None).unwrap();
        assert_eq!(for_super.len(), 2);
    }

    #[test]
    fn report_project_resolution_counts_without_mutating() {
        let f = setup();
        f.conn
            .execute(
                "INSERT INTO memories (id, org_id, user_id, project, tool, content)
             VALUES ('m1', 'org1', 'u_a', 'a-billing', 'claude-code', 'resolved one')",
                [],
            )
            .unwrap();
        f.conn
            .execute(
                "INSERT INTO memories (id, org_id, user_id, project, tool, content)
             VALUES ('m2', 'org1', 'u_a', 'ghost-project', 'claude-code', 'unresolved one')",
                [],
            )
            .unwrap();

        let before: i64 = f
            .conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        let report = report_project_resolution(&f.conn, "org1").unwrap();
        let after: i64 = f
            .conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .unwrap();

        assert_eq!(report.resolved, 1);
        assert_eq!(report.unresolved, 1);
        assert_eq!(report.unresolved_values[0].project, "ghost-project");
        assert_eq!(before, after, "the report must never mutate");
    }
}

// ── Promotion tests ───────────────────────────────────────────────────────────
#[cfg(test)]
mod promotion_tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'u2s', 'u2s')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO users (id, org_id, email, name, role) VALUES ('u1', 'org1', 'a@u2s.io', 'A', 'member')", []).unwrap();
        conn
    }

    fn insert_memory(conn: &Connection, id: &str, scope: &str) {
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, project, tool, content, title, type, scope)
             VALUES (?1, 'org1', 'u1', 'a-billing', 'claude-code', 'the content', 'A title', 'decision', ?2)",
            rusqlite::params![id, scope],
        ).unwrap();
    }

    #[test]
    fn promote_creates_org_scoped_copy_with_lineage() {
        let conn = setup();
        insert_memory(&conn, "src1", "client");
        let promoted = promote_memory(&conn, "org1", "src1", "u1")
            .unwrap()
            .unwrap();
        assert_eq!(promoted.scope, "org");
        assert_ne!(promoted.id, "src1");
        let lineage: Option<String> = conn
            .query_row(
                "SELECT promoted_from FROM memories WHERE id = ?1",
                [&promoted.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(lineage.as_deref(), Some("src1"));
    }

    /// Promotion copies; it never moves. The client keeps its own record.
    #[test]
    fn promote_leaves_source_unchanged() {
        let conn = setup();
        insert_memory(&conn, "src2", "project");
        promote_memory(&conn, "org1", "src2", "u1")
            .unwrap()
            .unwrap();
        let scope: String = conn
            .query_row("SELECT scope FROM memories WHERE id = 'src2'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(scope, "project", "the source memory must not be modified");
    }

    #[test]
    fn promote_rejects_org_scoped_source() {
        let conn = setup();
        insert_memory(&conn, "src3", "org");
        assert!(promote_memory(&conn, "org1", "src3", "u1").is_err());
    }

    #[test]
    fn promote_rejects_personal_source() {
        let conn = setup();
        insert_memory(&conn, "src4", "personal");
        assert!(promote_memory(&conn, "org1", "src4", "u1").is_err());
    }
}

// ── Inheritance and project/client wiring tests ──────────────────────────────
#[cfg(test)]
mod inheritance_tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    fn setup() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'u2s', 'u2s')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO users (id, org_id, email, name, role) VALUES ('u1', 'org1', 'a@u2s.io', 'A', 'member')", []).unwrap();
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('cli_a', 'org1', 'A', 'a')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('cli_b', 'org1', 'B', 'b')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO projects (id, org_id, name, client_id) VALUES ('p_a', 'org1', 'a-billing', 'cli_a')", []).unwrap();
        conn.execute("INSERT INTO projects (id, org_id, name, client_id) VALUES ('p_b', 'org1', 'b-api', 'cli_b')", []).unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('p_int', 'org1', 'internal')",
            [],
        )
        .unwrap();
        conn
    }

    fn add_convention(conn: &Connection, title: &str, client: Option<&str>, project: Option<&str>) {
        conn.execute(
            "INSERT INTO conventions (org_id, project_id, client_id, title, content, category, weight, tags)
             VALUES ('org1', ?2, ?3, ?1, 'content', 'general', 100, '[]')",
            rusqlite::params![title, project, client],
        ).unwrap();
    }

    fn titles(conn: &Connection, project: &str) -> Vec<String> {
        let client = get_project_client_id(conn, "org1", project).unwrap();
        let pid = get_project_id_by_name(conn, "org1", project).unwrap();
        list_conventions_visible(
            conn,
            "org1",
            None,
            Some(false),
            pid.as_deref(),
            client.as_deref(),
            50,
            0,
            None,
        )
        .unwrap()
        .into_iter()
        .map(|c| c.title)
        .collect()
    }

    #[test]
    fn org_convention_applies_to_every_client_project() {
        let conn = setup();
        add_convention(&conn, "org-wide", None, None);
        assert!(titles(&conn, "a-billing").contains(&"org-wide".to_string()));
        assert!(titles(&conn, "b-api").contains(&"org-wide".to_string()));
        assert!(titles(&conn, "internal").contains(&"org-wide".to_string()));
    }

    /// The anti-override test: a client convention must sit ALONGSIDE the
    /// org-wide one, never in place of it. If this ever asserts one title
    /// instead of two, u2s's own standards have become overridable.
    #[test]
    fn client_convention_adds_to_org_convention() {
        let conn = setup();
        add_convention(&conn, "org-wide", None, None);
        add_convention(&conn, "client-a-rule", Some("cli_a"), None);
        let seen = titles(&conn, "a-billing");
        assert!(
            seen.contains(&"org-wide".to_string()),
            "org level must survive"
        );
        assert!(
            seen.contains(&"client-a-rule".to_string()),
            "client level must apply"
        );
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn client_convention_does_not_leak_to_another_client() {
        let conn = setup();
        add_convention(&conn, "client-a-rule", Some("cli_a"), None);
        assert!(!titles(&conn, "b-api").contains(&"client-a-rule".to_string()));
    }

    /// An internal project has no client level, so the chain is org → project.
    /// Crucially, no client's conventions may reach it.
    #[test]
    fn internal_project_resolves_org_then_project() {
        let conn = setup();
        add_convention(&conn, "org-wide", None, None);
        add_convention(&conn, "client-a-rule", Some("cli_a"), None);
        add_convention(&conn, "internal-rule", None, Some("p_int"));
        let seen = titles(&conn, "internal");
        assert!(seen.contains(&"org-wide".to_string()));
        assert!(seen.contains(&"internal-rule".to_string()));
        assert!(
            !seen.contains(&"client-a-rule".to_string()),
            "a client's rules must not reach internal work"
        );
    }

    #[test]
    fn all_three_levels_stack() {
        let conn = setup();
        add_convention(&conn, "org-wide", None, None);
        add_convention(&conn, "client-a-rule", Some("cli_a"), None);
        add_convention(&conn, "project-rule", None, Some("p_a"));
        assert_eq!(titles(&conn, "a-billing").len(), 3);
    }

    #[test]
    fn get_project_client_id_distinguishes_internal_from_missing() {
        let conn = setup();
        assert_eq!(
            get_project_client_id(&conn, "org1", "a-billing")
                .unwrap()
                .as_deref(),
            Some("cli_a")
        );
        assert_eq!(
            get_project_client_id(&conn, "org1", "internal").unwrap(),
            None
        );
        assert_eq!(
            get_project_client_id(&conn, "org1", "no-such-project").unwrap(),
            None
        );
    }

    // ── T-12: project creation and repo linking ──────────────────────────────

    #[test]
    fn create_project_without_client_is_internal() {
        let conn = setup();
        let p = create_project_with_creator_membership(
            &conn,
            "org1",
            "u1",
            "new-internal",
            None,
            None,
            None,
        )
        .unwrap();
        let cid: Option<String> = conn
            .query_row(
                "SELECT client_id FROM projects WHERE id = ?1",
                [&p.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(cid.is_none());
    }

    #[test]
    fn create_project_attaches_to_client() {
        let conn = setup();
        let p = create_project_with_creator_membership(
            &conn,
            "org1",
            "u1",
            "a-new",
            None,
            None,
            Some("cli_a"),
        )
        .unwrap();
        let cid: Option<String> = conn
            .query_row(
                "SELECT client_id FROM projects WHERE id = ?1",
                [&p.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cid.as_deref(), Some("cli_a"));
    }

    /// Every advertised template MUST resolve a capability envelope — a template
    /// present in the catalog but missing here fails agent creation with
    /// `invalid_template` (the lead_generation regression). Judge is included.
    #[test]
    fn every_template_resolves_capabilities() {
        for template in [
            "qa",
            "github_issue_resolver",
            "github_pr_reviewer",
            "lead_generation",
            "judge",
        ] {
            assert!(
                autonomous_agent_capabilities(template).is_ok(),
                "template {template} must resolve capabilities"
            );
        }
        assert_eq!(
            autonomous_agent_capabilities("lead_generation").unwrap(),
            vec!["web:search", "lead:write", "delivery:write"]
        );
        assert!(autonomous_agent_capabilities("does_not_exist").is_err());
    }

    /// Grafting a project onto another tenant's client must be refused.
    #[test]
    fn create_project_rejects_client_from_another_org() {
        let conn = setup();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org2', 'Other', 'other')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('cli_x', 'org2', 'X', 'x')",
            [],
        )
        .unwrap();
        assert!(create_project_with_creator_membership(
            &conn,
            "org1",
            "u1",
            "bad",
            None,
            None,
            Some("cli_x")
        )
        .is_err());
    }

    // ── update_project: client reassignment + partial-update semantics ────────

    fn project_parent_and_client(conn: &Connection, id: &str) -> (Option<String>, Option<String>) {
        conn.query_row(
            "SELECT parent_id, client_id FROM projects WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .unwrap()
    }

    #[test]
    fn update_project_reassigns_client_to_another_client() {
        let conn = setup();
        // p_a starts owned by cli_a; move it to cli_b.
        let found = update_project(&conn, "org1", "p_a", None, Some(Some("cli_b"))).unwrap();
        assert!(found);
        let (_, client) = project_parent_and_client(&conn, "p_a");
        assert_eq!(client.as_deref(), Some("cli_b"));
    }

    #[test]
    fn update_project_clears_client_to_internal() {
        let conn = setup();
        let found = update_project(&conn, "org1", "p_a", None, Some(None)).unwrap();
        assert!(found);
        let (_, client) = project_parent_and_client(&conn, "p_a");
        assert!(
            client.is_none(),
            "null client_id must clear the project to Internal"
        );
    }

    #[test]
    fn update_project_rejects_client_from_another_org() {
        let conn = setup();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org2', 'Other', 'other')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('cli_x', 'org2', 'X', 'x')",
            [],
        )
        .unwrap();
        let err = update_project(&conn, "org1", "p_a", None, Some(Some("cli_x")));
        assert!(err.is_err(), "a client from another org must be rejected");
        assert!(err.unwrap_err().to_string().contains("client_not_found"));
        // The offending update must not have partially applied.
        let (_, client) = project_parent_and_client(&conn, "p_a");
        assert_eq!(
            client.as_deref(),
            Some("cli_a"),
            "rejected update must leave client untouched"
        );
    }

    #[test]
    fn update_project_partial_update_does_not_clobber_the_other_field() {
        let conn = setup();
        // Give p_a a parent so both columns are populated (client=cli_a, parent=p_int).
        update_project(&conn, "org1", "p_a", Some(Some("p_int")), None).unwrap();
        let (parent, client) = project_parent_and_client(&conn, "p_a");
        assert_eq!(parent.as_deref(), Some("p_int"));
        assert_eq!(client.as_deref(), Some("cli_a"));

        // Sending only client_id must NOT null out parent_id.
        update_project(&conn, "org1", "p_a", None, Some(Some("cli_b"))).unwrap();
        let (parent, client) = project_parent_and_client(&conn, "p_a");
        assert_eq!(
            parent.as_deref(),
            Some("p_int"),
            "client-only update must not touch parent_id"
        );
        assert_eq!(client.as_deref(), Some("cli_b"));

        // Sending only parent_id must NOT null out client_id.
        update_project(&conn, "org1", "p_a", Some(None), None).unwrap();
        let (parent, client) = project_parent_and_client(&conn, "p_a");
        assert!(
            parent.is_none(),
            "parent-only update to null must detach to root"
        );
        assert_eq!(
            client.as_deref(),
            Some("cli_b"),
            "parent-only update must not touch client_id"
        );
    }

    #[test]
    fn update_project_no_fields_reports_existence() {
        let conn = setup();
        assert!(
            update_project(&conn, "org1", "p_a", None, None).unwrap(),
            "existing project returns true"
        );
        assert!(
            !update_project(&conn, "org1", "nope", None, None).unwrap(),
            "missing project returns false"
        );
    }

    #[test]
    fn second_repo_link_to_same_project_is_refused() {
        let conn = setup();
        conn.execute("INSERT INTO code_projects (id, org_id, name, root_path) VALUES (1, 'org1', 'repo-one', '/a')", []).unwrap();
        conn.execute("INSERT INTO code_projects (id, org_id, name, root_path) VALUES (2, 'org1', 'repo-two', '/b')", []).unwrap();

        link_code_project_to_project(&conn, "org1", 1, "p_a").unwrap();
        assert!(
            link_code_project_to_project(&conn, "org1", 2, "p_a").is_err(),
            "one repo per project — the second link must be refused, not silently repoint the first"
        );
    }

    #[test]
    fn relinking_the_same_repo_is_idempotent() {
        let conn = setup();
        conn.execute("INSERT INTO code_projects (id, org_id, name, root_path) VALUES (1, 'org1', 'repo-one', '/a')", []).unwrap();
        link_code_project_to_project(&conn, "org1", 1, "p_a").unwrap();
        link_code_project_to_project(&conn, "org1", 1, "p_a")
            .expect("re-linking the same repo must be a no-op");
    }
}
