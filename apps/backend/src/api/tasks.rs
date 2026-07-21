use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::{require_permission, resolve_list_pagination, AppJson},
    db::queries::{self, TaskListFilters},
    models::types::{
        AddCommentRequest, AddLabelRequest, ApiError, AssignTaskRequest, AuthContext,
        CreateRetrospectiveRequest, CreateSprintRequest, CreateTaskRequest, LinkSpecRequest,
        PatchSprintRequest, PatchTaskRequest, ResolveBySpecRequest, Sprint, SprintRetrospective,
        Task, TaskAssignee, TaskComment,
    },
    store::sqlite::SqliteStore,
};

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let msg = e.to_string();
    let (status, code) = if msg.starts_with("invalid_status") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_status")
    } else if msg.starts_with("invalid_priority") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_priority")
    } else if msg.starts_with("invalid_transition") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_transition")
    } else if msg.starts_with("invalid_assignee") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_assignee")
    } else if msg.starts_with("empty_comment") {
        (StatusCode::UNPROCESSABLE_ENTITY, "empty_comment")
    } else if msg.contains("parent task not found")
        || msg.contains("cannot nest a subtask under a subtask")
        || msg.contains("parent task belongs to a different project")
        || msg.contains("sprint not found")
        || msg.contains("sprint belongs to a different project")
    {
        (StatusCode::UNPROCESSABLE_ENTITY, "validation_error")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
    };
    (status, Json(ApiError { error: msg, code: code.to_string() }))
}

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError { error: "Database lock error".to_string(), code: "internal_error".to_string() }),
    )
}

fn not_found(what: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError { error: format!("{what} not found"), code: "not_found".to_string() }),
    )
}

fn unknown_spec() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ApiError {
            error: "Unknown openspec change name".to_string(),
            code: "unknown_spec".to_string(),
        }),
    )
}

fn viewer_user_id(auth: &AuthContext) -> Option<&str> {
    if auth.role.is_super_user() {
        None
    } else {
        Some(auth.user_id.as_str())
    }
}

/// Whether the caller may see a task/sprint in `project` — mirrors sessions' project
/// visibility rule (org-shared/unregistered projects are visible to everyone; otherwise
/// membership is required). Used to enforce the 404-not-403 existence-leak rule on reads.
fn project_visible(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    project: &str,
) -> Result<bool, (StatusCode, Json<ApiError>)> {
    let viewer = viewer_user_id(auth);
    queries::user_can_view_project_name(conn, &auth.org_id, project, viewer).map_err(db_err)
}

/// Loads a task by id, applying the existence-leak rule: not-found and not-visible both
/// resolve to 404. Used by every child-resource handler before it acts on the parent task.
fn load_visible_task(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    task_id: &str,
) -> Result<Task, (StatusCode, Json<ApiError>)> {
    match queries::get_task(conn, &auth.org_id, task_id).map_err(db_err)? {
        Some(task) if project_visible(conn, auth, &task.project)? => Ok(task),
        _ => Err(not_found("Task")),
    }
}

fn load_visible_sprint(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    sprint_id: &str,
) -> Result<Sprint, (StatusCode, Json<ApiError>)> {
    match queries::get_sprint(conn, &auth.org_id, sprint_id).map_err(db_err)? {
        Some(sprint) if project_visible(conn, auth, &sprint.project)? => Ok(sprint),
        _ => Err(not_found("Sprint")),
    }
}

// ── Query param DTOs ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct TaskListQuery {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub sprint: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub include_archived: Option<bool>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SprintListQuery {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub include_archived: Option<bool>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(serde::Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<Task>,
    pub total: i64,
}

// ── PR1: core CRUD ───────────────────────────────────────────────────────────

pub async fn list_tasks_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(q): Query<TaskListQuery>,
) -> Result<Json<Vec<Task>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, q.project.as_deref(), "task:read")?;

    let assignee_user_id = match q.assignee.as_deref() {
        Some("me") => Some(auth.user_id.clone()),
        Some(other) => Some(other.to_string()),
        None => None,
    };
    let filters = TaskListFilters {
        project: q.project.clone(),
        status: q.status.clone(),
        priority: None,
        sprint_id: q.sprint.clone(),
        label: q.label.clone(),
        parent_id: q.parent_id.clone(),
        assignee_user_id,
        include_archived: q.include_archived.unwrap_or(false),
    };
    let (limit, offset) = resolve_list_pagination(q.limit, q.offset);
    let viewer = viewer_user_id(&auth);
    let tasks = queries::list_tasks(&conn, &auth.org_id, viewer, &filters, limit, offset)
        .map_err(db_err)?;
    Ok(Json(tasks))
}

pub async fn create_task_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, Some(&input.project), "task:write")?;
    let task = queries::create_task(&conn, &auth.org_id, &auth.user_id, &input).map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(task)))
}

pub async fn get_task_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Task>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:read")?;
    Ok(Json(task))
}

pub async fn patch_task_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<PatchTaskRequest>,
) -> Result<Json<Task>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let existing = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&existing.project), "task:write")?;
    let updated = queries::patch_task(&conn, &auth.org_id, &id, &input).map_err(db_err)?;
    match updated {
        Some(task) => Ok(Json(task)),
        None => Err(not_found("Task")),
    }
}

pub async fn delete_task_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let existing = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&existing.project), "task:delete")?;
    queries::soft_delete_task(&conn, &auth.org_id, &id).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_subtasks_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Task>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let parent = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&parent.project), "task:read")?;
    let children = queries::list_subtasks(&conn, &auth.org_id, &parent.id).map_err(db_err)?;
    Ok(Json(children))
}

// ── PR2: assignment ──────────────────────────────────────────────────────────

pub async fn assign_task_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<AssignTaskRequest>,
) -> Result<Json<Vec<TaskAssignee>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:assign")?;
    let assignees = queries::set_task_assignees(&conn, &auth.org_id, &task.id, &auth.user_id, &input.user_ids)
        .map_err(db_err)?;
    Ok(Json(assignees))
}

pub async fn unassign_task_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:assign")?;
    queries::remove_task_assignee(&conn, &task.id, &user_id).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── PR3: labels ───────────────────────────────────────────────────────────────

pub async fn add_task_label_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<AddLabelRequest>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:write")?;
    let labels = queries::add_task_label(&conn, &task.id, &input.label).map_err(db_err)?;
    Ok(Json(labels))
}

pub async fn remove_task_label_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, label)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:write")?;
    queries::remove_task_label(&conn, &task.id, &label).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── PR4: comments ────────────────────────────────────────────────────────────

pub async fn list_task_comments_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<TaskComment>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:read")?;
    let comments = queries::list_task_comments(&conn, &task.id).map_err(db_err)?;
    Ok(Json(comments))
}

pub async fn add_task_comment_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<AddCommentRequest>,
) -> Result<(StatusCode, Json<TaskComment>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:write")?;
    let comment = queries::add_task_comment(&conn, &task.id, &auth.user_id, &input.body).map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(comment)))
}

pub async fn delete_task_comment_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, comment_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;

    let Some((comment_task_id, author_id)) = queries::get_task_comment(&conn, &comment_id).map_err(db_err)? else {
        return Err(not_found("Comment"));
    };
    if comment_task_id != task.id {
        return Err(not_found("Comment"));
    }

    let is_author = author_id == auth.user_id;
    if !is_author {
        require_permission(&conn, &auth, Some(&task.project), "task:manage")?;
    }

    queries::delete_task_comment(&conn, &comment_id).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ── PR5: spec links + auto-resolve ──────────────────────────────────────────

/// Checks whether `name` matches an active or archived openspec change folder. Advisory:
/// if the openspec root cannot be read, this returns `true` (cannot confirm, allow) rather
/// than blocking all linking — resolves design risk R5.
/// Validates a `spec_change_name` before a task links to it.
///
/// The filesystem check alone was decorative in production. The backend runs on
/// Fly.io, where no `openspec/` directory exists, so `resolve_openspec_root`
/// never finds a repo tree and `spec_change_exists` falls through to its
/// "root unreadable → allow" branch. Every string linked, typos included.
///
/// **The naive fix does not work.** Simply OR-ing a DB lookup in front of the
/// existing check changes nothing: the filesystem branch still returns `true`
/// unconditionally when there is no tree, so it swallows the DB's verdict every
/// time. A permissive fallback placed after an authoritative check makes the
/// authoritative check dead code.
///
/// So the two worlds are separated by whether an `openspec/` tree exists at all:
///
/// - **A tree exists** (a dev running the backend inside a checkout): keep the
///   filesystem as a fallback, so a change that lives on disk but was never
///   pushed to NexusMind still links. Nothing that works today stops working.
/// - **No tree** (production): the DB is the *only* referent, and if it says no,
///   the answer is no. There is nothing to be permissive about.
///
/// DEPLOYMENT ORDER: this makes `link_task_spec` genuinely reject unknown names in
/// production, so the `sdd_changes` table must be populated FIRST. Ship the
/// importer (`bin/import_sdd.rs`) before, or with, this change — never after.
fn spec_change_is_known(conn: &rusqlite::Connection, org_id: &str, name: &str) -> bool {
    if queries::sdd_change_exists(conn, org_id, name).unwrap_or(false) {
        return true;
    }

    let root = repo_root();
    if !root.join("openspec/changes").is_dir() {
        // No tree to consult. The DB already said no, and a check that cannot fail
        // is not a check.
        return false;
    }
    spec_change_exists(&root, name)
}

pub fn spec_change_exists(root: &std::path::Path, name: &str) -> bool {
    let active = root.join("openspec/changes").join(name);
    if active.is_dir() {
        return true;
    }
    let archive_dir = root.join("openspec/changes/archive");
    match std::fs::read_dir(&archive_dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).any(|entry| {
            entry.file_name().to_string_lossy().ends_with(&format!("-{name}"))
                || entry.file_name().to_string_lossy() == name
        }),
        Err(_) => {
            // Root unreadable (or archive dir absent): cannot confirm, allow (advisory).
            !root.join("openspec/changes").exists()
        }
    }
}

/// Resolves the monorepo root that contains `openspec/`, so `spec_change_exists`
/// can validate spec change names regardless of the server's working directory
/// (e.g. run from `apps/backend` in dev, or from the repo root in some deploys).
///
/// Priority order:
/// 1. `env_override` (in practice the `OPENSPEC_ROOT` env var) — explicit config.
/// 2. Walk up from `start_dir` looking for a directory containing `openspec/changes/`.
/// 3. `CARGO_MANIFEST_DIR/../../openspec`'s parent (compile-time apps/backend ->
///    monorepo root), if that `openspec` directory exists.
/// 4. Fallback: `start_dir` itself. Combined with `spec_change_exists`'s
///    "root unreadable -> advisory pass" behavior, this preserves the
///    intentional accept-by-default outcome (design R5) when no repo tree
///    can be found at all (e.g. a deployed prod binary with no repo checkout).
fn resolve_openspec_root(
    env_override: Option<String>,
    start_dir: &std::path::Path,
) -> std::path::PathBuf {
    if let Some(root) = env_override {
        if !root.is_empty() {
            return std::path::PathBuf::from(root);
        }
    }

    let mut current = Some(start_dir);
    while let Some(dir) = current {
        if dir.join("openspec/changes").is_dir() {
            return dir.to_path_buf();
        }
        current = dir.parent();
    }

    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        let candidate = std::path::PathBuf::from(manifest_dir).join("../../openspec");
        if candidate.is_dir() {
            // candidate is `<repo-root>/openspec`; return `<repo-root>`.
            if let Some(repo_root) = candidate.parent() {
                return repo_root.to_path_buf();
            }
        }
    }

    start_dir.to_path_buf()
}

fn repo_root() -> std::path::PathBuf {
    let env_override = std::env::var("OPENSPEC_ROOT").ok();
    let start_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    resolve_openspec_root(env_override, &start_dir)
}

pub async fn list_task_spec_links_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:read")?;
    let links = queries::list_task_spec_links(&conn, &task.id).map_err(db_err)?;
    Ok(Json(links))
}

pub async fn link_task_spec_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<LinkSpecRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:write")?;

    if !spec_change_is_known(&conn, &auth.org_id, &input.spec_change_name) {
        return Err(unknown_spec());
    }

    queries::link_task_spec(&conn, &task.id, &auth.user_id, &input.spec_change_name).map_err(db_err)?;
    Ok(StatusCode::CREATED)
}

pub async fn unlink_task_spec_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let task = load_visible_task(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&task.project), "task:write")?;
    queries::unlink_task_spec(&conn, &task.id, &name).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Serialize)]
pub struct ResolveBySpecResponse {
    pub resolved: Vec<String>,
}

/// Auto-resolves visible tasks linked to `spec_change_name` to `done`.
pub async fn resolve_by_spec_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<ResolveBySpecRequest>,
) -> Result<Json<ResolveBySpecResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "task:write")?;
    let resolved = queries::resolve_tasks_by_spec(&conn, &auth.org_id, &input.spec_change_name, viewer_user_id(&auth))
        .map_err(db_err)?;
    Ok(Json(ResolveBySpecResponse { resolved }))
}

// ── PR6: sprints ──────────────────────────────────────────────────────────────

pub async fn list_sprints_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(q): Query<SprintListQuery>,
) -> Result<Json<Vec<Sprint>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, q.project.as_deref(), "task:read")?;
    let (limit, offset) = resolve_list_pagination(q.limit, q.offset);
    let viewer = viewer_user_id(&auth);
    let sprints = queries::list_sprints(
        &conn,
        &auth.org_id,
        viewer,
        q.project.as_deref(),
        q.status.as_deref(),
        q.include_archived.unwrap_or(false),
        limit,
        offset,
    )
    .map_err(db_err)?;
    Ok(Json(sprints))
}

pub async fn create_sprint_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateSprintRequest>,
) -> Result<(StatusCode, Json<Sprint>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, Some(&input.project), "task:manage")?;
    let sprint = queries::create_sprint(&conn, &auth.org_id, &auth.user_id, &input).map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(sprint)))
}

pub async fn get_sprint_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Sprint>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let sprint = load_visible_sprint(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&sprint.project), "task:read")?;
    Ok(Json(sprint))
}

pub async fn patch_sprint_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<PatchSprintRequest>,
) -> Result<Json<Sprint>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let existing = load_visible_sprint(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&existing.project), "task:manage")?;
    let updated = queries::patch_sprint(&conn, &auth.org_id, &id, &input).map_err(db_err)?;
    match updated {
        Some(sprint) => Ok(Json(sprint)),
        None => Err(not_found("Sprint")),
    }
}

pub async fn delete_sprint_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let existing = load_visible_sprint(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&existing.project), "task:manage")?;
    queries::soft_delete_sprint(&conn, &auth.org_id, &id).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_retrospectives_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SprintRetrospective>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let sprint = load_visible_sprint(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&sprint.project), "task:read")?;
    let retros = queries::list_retrospectives(&conn, &sprint.id).map_err(db_err)?;
    Ok(Json(retros))
}

pub async fn create_retrospective_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<CreateRetrospectiveRequest>,
) -> Result<(StatusCode, Json<SprintRetrospective>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let sprint = load_visible_sprint(&conn, &auth, &id)?;
    require_permission(&conn, &auth, Some(&sprint.project), "task:manage")?;
    let retro = queries::create_retrospective(&conn, &sprint.id, &auth.org_id, &auth.user_id, &input)
        .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(retro)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, middleware, routing::{delete, get, post}, Router};
    use tower::util::ServiceExt;

    use crate::{
        api::middleware as auth_mw,
        db::{connection::connect, migrations, queries as q},
        store::sqlite::SqliteStore,
    };

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        SqliteStore::new(conn)
    }

    /// `OPENSPEC_ROOT` is a process-global env var. `cargo test` runs tests in
    /// parallel threads within the same process, so any test that mutates it
    /// via `std::env::set_var`/`remove_var` MUST hold this lock for the
    /// duration of the mutation + assertions, or it races with other tests
    /// reading/mutating the same var.
    fn openspec_root_env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/tasks", get(list_tasks_handler).post(create_task_handler))
            .route("/v1/tasks/resolve-by-spec", post(resolve_by_spec_handler))
            .route(
                "/v1/tasks/:id",
                get(get_task_handler).patch(patch_task_handler).delete(delete_task_handler),
            )
            .route("/v1/tasks/:id/subtasks", get(list_subtasks_handler))
            .route("/v1/tasks/:id/assignees", post(assign_task_handler))
            .route("/v1/tasks/:id/assignees/:user_id", delete(unassign_task_handler))
            .route("/v1/tasks/:id/labels", post(add_task_label_handler))
            .route("/v1/tasks/:id/labels/:label", delete(remove_task_label_handler))
            .route(
                "/v1/tasks/:id/comments",
                get(list_task_comments_handler).post(add_task_comment_handler),
            )
            .route("/v1/tasks/:id/comments/:comment_id", delete(delete_task_comment_handler))
            .route(
                "/v1/tasks/:id/spec-links",
                get(list_task_spec_links_handler).post(link_task_spec_handler),
            )
            .route("/v1/tasks/:id/spec-links/:name", delete(unlink_task_spec_handler))
            .route("/v1/sprints", get(list_sprints_handler).post(create_sprint_handler))
            .route(
                "/v1/sprints/:id",
                get(get_sprint_handler).patch(patch_sprint_handler).delete(delete_sprint_handler),
            )
            .route(
                "/v1/sprints/:id/retrospectives",
                get(list_retrospectives_handler).post(create_retrospective_handler),
            )
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_with_key() -> (SqliteStore, String, String) {
        let store = make_store();
        let (org_id, raw_key) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _user, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            (org.id, key)
        };
        (store, raw_key, org_id)
    }

    fn create_member_with_id(store: &SqliteStore, org_id: &str, role: &str) -> (String, String) {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, ?3, 'Test', ?4, 'active', datetime('now'))",
            rusqlite::params![user_id, org_id, format!("{}-{role}@test.com", &user_id[..8]), role],
        ).unwrap();
        let key_id = Uuid::new_v4().to_string();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org_id, key_hash],
        ).unwrap();
        (raw_key, user_id)
    }

    fn admin_user_id(store: &SqliteStore, org_id: &str) -> String {
        let db = store.conn();
        let conn = db.lock().unwrap();
        q::list_users(&conn, org_id).unwrap().into_iter().next().unwrap().id
    }

    fn add_member_to_project(store: &SqliteStore, org_id: &str, project: &str, user_id: &str) {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let project_id = q::get_or_create_project(&conn, org_id, project).unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO project_members (id, project_id, user_id, role, created_at)
              VALUES (?1, ?2, ?3, 'dev-senior', datetime('now'))",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), project_id, user_id],
        ).unwrap();
    }

    async fn post_json(store: &SqliteStore, key: &str, uri: &str, body: serde_json::Value) -> axum::response::Response {
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST").uri(uri)
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string())).unwrap(),
            )
            .await
            .unwrap()
    }

    async fn get_req(store: &SqliteStore, key: &str, uri: &str) -> axum::response::Response {
        app(store.clone())
            .oneshot(
                Request::builder().uri(uri)
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty()).unwrap(),
            )
            .await
            .unwrap()
    }

    async fn json_body(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── PR1: CRUD + visibility ───────────────────────────────────────────

    #[tokio::test]
    async fn create_task_creates_and_returns_201() {
        let (store, admin_key, org_id) = setup_with_key();
        let _ = org_id;
        let resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "First task" })).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = json_body(resp).await;
        assert_eq!(json["title"], "First task");
        assert_eq!(json["status"], "backlog");
    }

    #[tokio::test]
    async fn create_task_denied_without_task_write() {
        let (store, _admin_key, org_id) = setup_with_key();
        // security_officer template has task:read only, not task:write.
        let (member_key, _uid) = create_member_with_id(&store, &org_id, "security-officer");
        let resp = post_json(&store, &member_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "X" })).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let db = store.conn();
        let conn = db.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "no row created on 403");
    }

    #[tokio::test]
    async fn get_task_returns_404_for_non_member() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "secret-proj", "title": "T" })).await;
        let created = json_body(create_resp).await;
        let task_id = created["id"].as_str().unwrap().to_string();

        // Register the project so membership is enforced, then create a member NOT in it.
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::get_or_create_project(&conn, &org_id, "secret-proj").unwrap();
        }
        let (member_key, _uid) = create_member_with_id(&store, &org_id, "dev-senior");

        let resp = get_req(&store, &member_key, &format!("/v1/tasks/{task_id}")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_tasks_scoped_to_membership() {
        let (store, admin_key, org_id) = setup_with_key();
        post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "shared-proj", "title": "Shared" })).await;

        let (member_key, member_id) = create_member_with_id(&store, &org_id, "dev-senior");
        add_member_to_project(&store, &org_id, "shared-proj", &member_id);

        let resp = get_req(&store, &member_key, "/v1/tasks?project=shared-proj").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unassigned_admin_cannot_enumerate_tasks_in_another_project() {
        let (store, admin_key, org_id) = setup_with_key();
        let admin_id = admin_user_id(&store, &org_id);
        add_member_to_project(&store, &org_id, "private-proj", &admin_id);
        post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({
            "project": "private-proj",
            "title": "Private task"
        })).await;

        let (other_admin_key, _) = create_member_with_id(&store, &org_id, "admin");
        let resp = get_req(&store, &other_admin_key, "/v1/tasks").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(json_body(resp).await.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn patch_task_denied_without_write_permission() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let created = json_body(create_resp).await;
        let task_id = created["id"].as_str().unwrap().to_string();

        let (member_key, _uid) = create_member_with_id(&store, &org_id, "security-officer");
        let resp = app(store.clone())
            .oneshot(
                Request::builder().method("PATCH").uri(format!("/v1/tasks/{task_id}"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::json!({ "title": "Hijacked" }).to_string())).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let reloaded = get_req(&store, &admin_key, &format!("/v1/tasks/{task_id}")).await;
        let json = json_body(reloaded).await;
        assert_eq!(json["title"], "T");
    }

    #[tokio::test]
    async fn delete_task_requires_delete_permission_not_just_write() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let created = json_body(create_resp).await;
        let task_id = created["id"].as_str().unwrap().to_string();

        // dev_junior has task:write but not task:delete.
        let (member_key, _uid) = create_member_with_id(&store, &org_id, "dev-junior");
        let resp = app(store.clone())
            .oneshot(
                Request::builder().method("DELETE").uri(format!("/v1/tasks/{task_id}"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty()).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let reloaded = get_req(&store, &admin_key, &format!("/v1/tasks/{task_id}")).await;
        let json = json_body(reloaded).await;
        assert!(json["archived_at"].is_null());
    }

    #[tokio::test]
    async fn delete_nonexistent_or_invisible_task_returns_404() {
        let (store, admin_key, _org_id) = setup_with_key();
        let resp = app(store.clone())
            .oneshot(
                Request::builder().method("DELETE").uri("/v1/tasks/does-not-exist")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty()).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_tasks_pagination_reports_accurate_total() {
        let (store, admin_key, _org_id) = setup_with_key();
        for i in 0..5 {
            post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": format!("T{i}") })).await;
        }
        let resp = get_req(&store, &admin_key, "/v1/tasks?project=proj&limit=2&offset=0").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json.as_array().unwrap().len(), 2);
    }

    // ── PR2: assignment ──────────────────────────────────────────────────

    #[tokio::test]
    async fn assign_denied_without_task_assign() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();

        // dev_junior has task:write but not task:assign.
        let (member_key, member_id) = create_member_with_id(&store, &org_id, "dev-junior");
        let resp = post_json(&store, &member_key, &format!("/v1/tasks/{task_id}/assignees"), serde_json::json!({ "user_ids": [member_id] })).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let db = store.conn();
        let conn = db.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM task_assignees WHERE task_id = ?1", [&task_id], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn assign_succeeds_with_task_assign_permission() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();

        let (senior_key, senior_id) = create_member_with_id(&store, &org_id, "dev-senior");
        let resp = post_json(&store, &senior_key, &format!("/v1/tasks/{task_id}/assignees"), serde_json::json!({ "user_ids": [senior_id] })).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unassign_denied_without_task_assign() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let (senior_key, senior_id) = create_member_with_id(&store, &org_id, "dev-senior");
        post_json(&store, &senior_key, &format!("/v1/tasks/{task_id}/assignees"), serde_json::json!({ "user_ids": [senior_id] })).await;

        let (junior_key, _jid) = create_member_with_id(&store, &org_id, "dev-junior");
        let resp = app(store.clone())
            .oneshot(
                Request::builder().method("DELETE").uri(format!("/v1/tasks/{task_id}/assignees/{senior_id}"))
                    .header("Authorization", format!("Bearer {junior_key}"))
                    .body(Body::empty()).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn read_assignees_requires_only_task_read() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let (senior_key, senior_id) = create_member_with_id(&store, &org_id, "dev-senior");
        post_json(&store, &senior_key, &format!("/v1/tasks/{task_id}/assignees"), serde_json::json!({ "user_ids": [senior_id] })).await;

        let (readonly_key, _uid) = create_member_with_id(&store, &org_id, "security-officer");
        let resp = get_req(&store, &readonly_key, &format!("/v1/tasks/{task_id}")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json["assignees"].as_array().unwrap().len(), 1);
    }

    // ── PR3: labels + subtasks ────────────────────────────────────────────

    #[tokio::test]
    async fn label_write_denied_without_permission() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let (readonly_key, _uid) = create_member_with_id(&store, &org_id, "security-officer");
        let resp = post_json(&store, &readonly_key, &format!("/v1/tasks/{task_id}/labels"), serde_json::json!({ "label": "bug" })).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_subtasks_endpoint_returns_children() {
        let (store, admin_key, _org_id) = setup_with_key();
        let parent_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "Parent" })).await;
        let parent_id = json_body(parent_resp).await["id"].as_str().unwrap().to_string();
        post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "Child", "parent_id": parent_id })).await;

        let resp = get_req(&store, &admin_key, &format!("/v1/tasks/{parent_id}/subtasks")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json.as_array().unwrap().len(), 1);
    }

    // ── PR4: comments ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn add_comment_denied_without_write_permission() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let (readonly_key, _uid) = create_member_with_id(&store, &org_id, "security-officer");
        let resp = post_json(&store, &readonly_key, &format!("/v1/tasks/{task_id}/comments"), serde_json::json!({ "body": "hi" })).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let db = store.conn();
        let conn = db.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM task_comments WHERE task_id = ?1", [&task_id], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn list_comments_non_member_returns_404() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "secret-proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::get_or_create_project(&conn, &org_id, "secret-proj").unwrap();
        }
        let (member_key, _uid) = create_member_with_id(&store, &org_id, "dev-senior");
        let resp = get_req(&store, &member_key, &format!("/v1/tasks/{task_id}/comments")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_comment_by_manager_succeeds_for_others_comment() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let (author_key, _aid) = create_member_with_id(&store, &org_id, "dev-junior");
        let comment_resp = post_json(&store, &author_key, &format!("/v1/tasks/{task_id}/comments"), serde_json::json!({ "body": "hello" })).await;
        let comment_id = json_body(comment_resp).await["id"].as_str().unwrap().to_string();

        // Admin is privileged and bypasses require_permission (task:manage).
        let resp = app(store.clone())
            .oneshot(
                Request::builder().method("DELETE").uri(format!("/v1/tasks/{task_id}/comments/{comment_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty()).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_comment_denied_for_non_author_non_manager() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let (author_key, _aid) = create_member_with_id(&store, &org_id, "dev-junior");
        let comment_resp = post_json(&store, &author_key, &format!("/v1/tasks/{task_id}/comments"), serde_json::json!({ "body": "hello" })).await;
        let comment_id = json_body(comment_resp).await["id"].as_str().unwrap().to_string();

        let (other_key, _oid) = create_member_with_id(&store, &org_id, "dev-junior");
        let resp = app(store.clone())
            .oneshot(
                Request::builder().method("DELETE").uri(format!("/v1/tasks/{task_id}/comments/{comment_id}"))
                    .header("Authorization", format!("Bearer {other_key}"))
                    .body(Body::empty()).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── PR5: spec links + auto-resolve ───────────────────────────────────

    #[tokio::test]
    async fn spec_change_exists_matches_active_tree() {
        let tmp = std::env::temp_dir().join(format!("nm-spec-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("openspec/changes/my-change")).unwrap();
        assert!(spec_change_exists(&tmp, "my-change"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn spec_change_exists_matches_archived_tree() {
        let tmp = std::env::temp_dir().join(format!("nm-spec-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("openspec/changes/archive/2026-01-01-my-change")).unwrap();
        assert!(spec_change_exists(&tmp, "my-change"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn spec_change_exists_returns_false_for_unknown_name() {
        let tmp = std::env::temp_dir().join(format!("nm-spec-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("openspec/changes")).unwrap();
        assert!(!spec_change_exists(&tmp, "unknown-change"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn spec_change_exists_treats_unreadable_root_as_advisory_pass() {
        let tmp = std::env::temp_dir().join(format!("nm-spec-test-nonexistent-{}", uuid::Uuid::new_v4()));
        // Root does not exist at all — cannot confirm, advisory pass.
        assert!(spec_change_exists(&tmp, "anything"));
    }

    // ── openspec root resolution (server run from apps/backend vs repo root) ──

    #[tokio::test]
    async fn resolve_openspec_root_prefers_env_override() {
        let tmp = std::env::temp_dir().join(format!("nm-root-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("openspec/changes/known-change")).unwrap();

        let resolved = resolve_openspec_root(Some(tmp.to_string_lossy().to_string()), &tmp);
        assert_eq!(resolved, tmp);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn resolve_openspec_root_walks_up_from_nested_cwd() {
        // Simulates the server process running from `apps/backend` while
        // `openspec/` lives at the monorepo root.
        let tmp = std::env::temp_dir().join(format!("nm-root-walkup-{}", uuid::Uuid::new_v4()));
        let nested_cwd = tmp.join("apps/backend");
        std::fs::create_dir_all(tmp.join("openspec/changes/known-change")).unwrap();
        std::fs::create_dir_all(&nested_cwd).unwrap();

        let resolved = resolve_openspec_root(None, &nested_cwd);
        assert_eq!(resolved, tmp);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn resolve_openspec_root_uses_manifest_relative_fallback_when_walkup_fails() {
        // Nothing above the isolated temp start dir has an `openspec/changes`
        // tree, so priority 2 (walk-up) fails. Priority 3 (CARGO_MANIFEST_DIR
        // relative to the compiled test binary) resolves to this real repo's
        // root, which DOES have an `openspec/changes` tree — so it must win
        // over falling straight back to `start_dir`.
        let tmp = std::env::temp_dir().join(format!("nm-root-none-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let resolved = resolve_openspec_root(None, &tmp);
        assert_ne!(resolved, tmp);
        assert!(resolved.join("openspec/changes").is_dir());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn resolve_openspec_root_falls_back_to_start_dir_when_truly_unresolvable() {
        // Point OPENSPEC_ROOT explicitly at an isolated dir with no
        // `openspec/changes` tree anywhere above it. Since priority 1 (env
        // override) is set, it wins outright and no further fallback runs —
        // this covers the deployed-binary-with-no-repo-checkout case, where
        // an explicit misconfigured/empty OPENSPEC_ROOT should not silently
        // discover the CI/dev repo tree.
        let tmp = std::env::temp_dir().join(format!("nm-root-forced-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let resolved = resolve_openspec_root(Some(tmp.to_string_lossy().to_string()), &tmp);
        assert_eq!(resolved, tmp);
        assert!(spec_change_exists(&resolved, "anything"), "advisory pass when tree absent under explicit root");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn link_task_spec_handler_rejects_unknown_via_openspec_root_env() {
        let _guard = openspec_root_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("nm-link-handler-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("openspec/changes/known-change")).unwrap();

        std::env::set_var("OPENSPEC_ROOT", &tmp);

        let (store, admin_key, _org_id) = setup_with_key();
        let create_resp =
            post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();

        let ok_resp = post_json(
            &store,
            &admin_key,
            &format!("/v1/tasks/{task_id}/spec-links"),
            serde_json::json!({ "spec_change_name": "known-change" }),
        )
        .await;
        assert_eq!(ok_resp.status(), StatusCode::CREATED);

        let bad_resp = post_json(
            &store,
            &admin_key,
            &format!("/v1/tasks/{task_id}/spec-links"),
            serde_json::json!({ "spec_change_name": "unknown-change" }),
        )
        .await;
        assert_eq!(bad_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

        std::env::remove_var("OPENSPEC_ROOT");
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// BEHAVIOR CHANGE (sdd-artifacts PR-4). This test used to be named
    /// `link_task_spec_handler_advisory_passes_when_root_unresolvable` and asserted a
    /// 201 here — it encoded the bug as intended behavior.
    ///
    /// With no `openspec/` tree, the old check fell through to its "root unreadable ->
    /// allow" branch and accepted ANY string. That is the state of production: the
    /// backend runs on Fly.io, where no tree exists, so the validation never rejected
    /// anything. Confirmed live on 2026-07-11 by linking 11 tasks to a change folder
    /// that existed only on one laptop.
    ///
    /// Now `sdd_changes` is the referent. With no tree AND no DB row, the answer is no.
    #[tokio::test]
    async fn link_task_spec_handler_rejects_unknown_change_when_no_openspec_tree() {
        let _guard = openspec_root_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("nm-link-handler-noroot-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("OPENSPEC_ROOT", &tmp);

        let (store, admin_key, _org_id) = setup_with_key();
        let create_resp =
            post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();

        let resp = post_json(
            &store,
            &admin_key,
            &format!("/v1/tasks/{task_id}/spec-links"),
            serde_json::json!({ "spec_change_name": "anything-goes" }),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "with no openspec tree and no sdd_changes row, an unknown name must be rejected"
        );

        std::env::remove_var("OPENSPEC_ROOT");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test]
    async fn get_spec_links_returns_list() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let admin_id = admin_user_id(&store, &org_id);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            queries::link_task_spec(&conn, &task_id, &admin_id, "team-tasks").unwrap();
        }
        let resp = get_req(&store, &admin_key, &format!("/v1/tasks/{task_id}/spec-links")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn read_task_with_dangling_spec_link_still_succeeds() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let admin_id = admin_user_id(&store, &org_id);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            queries::link_task_spec(&conn, &task_id, &admin_id, "renamed-away-change").unwrap();
        }
        let resp = get_req(&store, &admin_key, &format!("/v1/tasks/{task_id}")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json["spec_links"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn remove_spec_link_denied_without_write_permission() {
        let (store, admin_key, org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let admin_id = admin_user_id(&store, &org_id);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            queries::link_task_spec(&conn, &task_id, &admin_id, "team-tasks").unwrap();
        }
        let (readonly_key, _uid) = create_member_with_id(&store, &org_id, "security-officer");
        let resp = app(store.clone())
            .oneshot(
                Request::builder().method("DELETE").uri(format!("/v1/tasks/{task_id}/spec-links/team-tasks"))
                    .header("Authorization", format!("Bearer {readonly_key}"))
                    .body(Body::empty()).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn resolve_by_spec_transitions_member_projects_with_task_write() {
        let (store, admin_key, org_id) = setup_with_key();
        let t1_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj-a", "title": "T1" })).await;
        let t1 = json_body(t1_resp).await["id"].as_str().unwrap().to_string();
        let t2_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj-b", "title": "T2" })).await;
        let t2 = json_body(t2_resp).await["id"].as_str().unwrap().to_string();
        let admin_id = admin_user_id(&store, &org_id);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            queries::link_task_spec(&conn, &t1, &admin_id, "team-tasks").unwrap();
            queries::link_task_spec(&conn, &t2, &admin_id, "team-tasks").unwrap();
        }
        // dev-senior explicitly supplies task:write; membership supplies visibility.
        let (member_key, member_id) = create_member_with_id(&store, &org_id, "dev-senior");
        add_member_to_project(&store, &org_id, "proj-a", &member_id);
        add_member_to_project(&store, &org_id, "proj-b", &member_id);

        let resp = post_json(&store, &member_key, "/v1/tasks/resolve-by-spec", serde_json::json!({ "spec_change_name": "team-tasks" })).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json["resolved"], serde_json::json!([t1, t2]));
    }

    // ── PR6: sprints ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_sprint_denied_without_manage_permission() {
        let (store, _admin_key, org_id) = setup_with_key();
        // dev_senior has task:write/assign/delete but not task:manage.
        let (senior_key, _uid) = create_member_with_id(&store, &org_id, "dev-senior");
        let resp = post_json(&store, &senior_key, "/v1/sprints", serde_json::json!({ "project": "proj", "name": "Sprint 1" })).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let db = store.conn();
        let conn = db.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM sprints", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn create_retrospective_denied_without_manage_permission() {
        let (store, admin_key, org_id) = setup_with_key();
        let sprint_resp = post_json(&store, &admin_key, "/v1/sprints", serde_json::json!({ "project": "proj", "name": "Sprint 1" })).await;
        let sprint_id = json_body(sprint_resp).await["id"].as_str().unwrap().to_string();
        let (senior_key, _uid) = create_member_with_id(&store, &org_id, "dev-senior");
        let resp = post_json(&store, &senior_key, &format!("/v1/sprints/{sprint_id}/retrospectives"), serde_json::json!({ "went_well": "ok" })).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn retrospective_retrievable_via_read_path() {
        let (store, admin_key, _org_id) = setup_with_key();
        let sprint_resp = post_json(&store, &admin_key, "/v1/sprints", serde_json::json!({ "project": "proj", "name": "Sprint 1" })).await;
        let sprint_id = json_body(sprint_resp).await["id"].as_str().unwrap().to_string();
        post_json(&store, &admin_key, &format!("/v1/sprints/{sprint_id}/retrospectives"), serde_json::json!({ "went_well": "Great pace" })).await;

        let resp = get_req(&store, &admin_key, &format!("/v1/sprints/{sprint_id}/retrospectives")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert_eq!(json[0]["went_well"], "Great pace");
    }

    #[tokio::test]
    async fn sprint_and_retrospective_reads_scoped_to_membership() {
        let (store, admin_key, org_id) = setup_with_key();
        let sprint_resp = post_json(&store, &admin_key, "/v1/sprints", serde_json::json!({ "project": "secret-proj", "name": "Sprint 1" })).await;
        let sprint_id = json_body(sprint_resp).await["id"].as_str().unwrap().to_string();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::get_or_create_project(&conn, &org_id, "secret-proj").unwrap();
        }
        let (member_key, _uid) = create_member_with_id(&store, &org_id, "dev-senior");
        let resp = get_req(&store, &member_key, &format!("/v1/sprints/{sprint_id}")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let retro_resp = get_req(&store, &member_key, &format!("/v1/sprints/{sprint_id}/retrospectives")).await;
        assert_eq!(retro_resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_task_unauthenticated_returns_401() {
        let (store, _admin_key, _org_id) = setup_with_key();
        let resp = app(store)
            .oneshot(
                Request::builder().method("POST").uri("/v1/tasks")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::json!({ "project": "proj", "title": "T" }).to_string())).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── FIX 1: project-scoped role override must apply on read handlers ────
    //
    // A `viewer` global role has NO task:read at all (see get_role_permissions).
    // A `project_members.role = 'dev-junior'` override grants task:read (+ write) but
    // ONLY when the handler threads `Some(&project)` into require_permission. Passing
    // `None` (the pre-fix behavior) ignores the override and always falls back to the
    // global role, producing a false 403 for a project-scoped grant.
    fn add_member_with_project_role(store: &SqliteStore, org_id: &str, project: &str, user_id: &str, role: &str) {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let project_id = q::get_or_create_project(&conn, org_id, project).unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO project_members (id, project_id, user_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), project_id, user_id, role],
        ).unwrap();
        // Overwrite in case get_or_create_project auto-seeded a 'member' row for this user.
        conn.execute(
            "UPDATE project_members SET role = ?1 WHERE project_id = ?2 AND user_id = ?3",
            rusqlite::params![role, project_id, user_id],
        ).unwrap();
    }

    #[tokio::test]
    async fn project_scoped_role_override_grants_read_on_all_read_handlers() {
        let (store, admin_key, org_id) = setup_with_key();

        // Global role 'viewer' has no task:read. Only the project-level 'dev-junior'
        // override (task:read + task:write) should grant access.
        let (viewer_key, viewer_id) = create_member_with_id(&store, &org_id, "viewer");
        add_member_with_project_role(&store, &org_id, "scoped-proj", &viewer_id, "dev-junior");

        // Task + subtask.
        let task_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "scoped-proj", "title": "Parent" })).await;
        let task_id = json_body(task_resp).await["id"].as_str().unwrap().to_string();
        post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "scoped-proj", "title": "Child", "parent_id": task_id })).await;

        // Comment.
        let admin_id = admin_user_id(&store, &org_id);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            queries::add_task_comment(&conn, &task_id, &admin_id, "hi").unwrap();
            queries::link_task_spec(&conn, &task_id, &admin_id, "team-tasks").unwrap();
        }

        // Sprint + retrospective.
        let sprint_resp = post_json(&store, &admin_key, "/v1/sprints", serde_json::json!({ "project": "scoped-proj", "name": "Sprint 1" })).await;
        let sprint_id = json_body(sprint_resp).await["id"].as_str().unwrap().to_string();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            queries::create_retrospective(
                &conn, &sprint_id, &org_id, &admin_id,
                &CreateRetrospectiveRequest { went_well: Some("ok".to_string()), went_wrong: None, action_items: None },
            ).unwrap();
        }

        assert_eq!(get_req(&store, &viewer_key, &format!("/v1/tasks/{task_id}")).await.status(), StatusCode::OK, "get_task_handler must honor project-scoped role override");
        assert_eq!(get_req(&store, &viewer_key, &format!("/v1/tasks/{task_id}/subtasks")).await.status(), StatusCode::OK, "list_subtasks_handler must honor project-scoped role override");
        assert_eq!(get_req(&store, &viewer_key, &format!("/v1/tasks/{task_id}/comments")).await.status(), StatusCode::OK, "list_task_comments_handler must honor project-scoped role override");
        assert_eq!(get_req(&store, &viewer_key, &format!("/v1/tasks/{task_id}/spec-links")).await.status(), StatusCode::OK, "list_task_spec_links_handler must honor project-scoped role override");
        assert_eq!(get_req(&store, &viewer_key, &format!("/v1/sprints/{sprint_id}")).await.status(), StatusCode::OK, "get_sprint_handler must honor project-scoped role override");
        assert_eq!(get_req(&store, &viewer_key, &format!("/v1/sprints/{sprint_id}/retrospectives")).await.status(), StatusCode::OK, "list_retrospectives_handler must honor project-scoped role override");
    }

    // ── FIX 2: create_task must validate sprint_id like patch_task does ────

    #[tokio::test]
    async fn create_task_rejects_sprint_from_different_project() {
        let (store, admin_key, _org_id) = setup_with_key();
        let sprint_resp = post_json(&store, &admin_key, "/v1/sprints", serde_json::json!({ "project": "proj-a", "name": "Sprint A" })).await;
        let sprint_id = json_body(sprint_resp).await["id"].as_str().unwrap().to_string();

        let resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({
            "project": "proj-b", "title": "T", "sprint_id": sprint_id
        })).await;
        assert!(resp.status().is_client_error(), "expected 4xx, got {}", resp.status());

        let db = store.conn();
        let conn = db.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE project = 'proj-b'", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "no task row created when sprint validation fails");
    }

    #[tokio::test]
    async fn create_task_rejects_nonexistent_sprint_id() {
        let (store, admin_key, _org_id) = setup_with_key();
        let resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({
            "project": "proj-a", "title": "T", "sprint_id": "does-not-exist"
        })).await;
        assert!(resp.status().is_client_error(), "expected 4xx, got {}", resp.status());
    }

    // ── FIX 4: 401 (unauthenticated) coverage beyond create_task ───────────

    #[tokio::test]
    async fn list_tasks_unauthenticated_returns_401() {
        let (store, _admin_key, _org_id) = setup_with_key();
        let resp = app(store)
            .oneshot(Request::builder().uri("/v1/tasks").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn assign_task_unauthenticated_returns_401() {
        let (store, admin_key, _org_id) = setup_with_key();
        let create_resp = post_json(&store, &admin_key, "/v1/tasks", serde_json::json!({ "project": "proj", "title": "T" })).await;
        let task_id = json_body(create_resp).await["id"].as_str().unwrap().to_string();
        let resp = app(store)
            .oneshot(
                Request::builder().method("POST").uri(format!("/v1/tasks/{task_id}/assignees"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::json!({ "user_ids": [] }).to_string())).unwrap(),
            )
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn get_sprint_unauthenticated_returns_401() {
        let (store, admin_key, _org_id) = setup_with_key();
        let sprint_resp = post_json(&store, &admin_key, "/v1/sprints", serde_json::json!({ "project": "proj", "name": "Sprint 1" })).await;
        let sprint_id = json_body(sprint_resp).await["id"].as_str().unwrap().to_string();
        let resp = app(store)
            .oneshot(Request::builder().uri(format!("/v1/sprints/{sprint_id}")).body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
