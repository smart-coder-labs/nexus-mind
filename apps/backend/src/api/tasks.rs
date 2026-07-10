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
    if auth.role.is_privileged() {
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
    require_permission(&conn, &auth, None, "task:read")?;
    let task = load_visible_task(&conn, &auth, &id)?;
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
    require_permission(&conn, &auth, None, "task:read")?;
    let parent = load_visible_task(&conn, &auth, &id)?;
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
    require_permission(&conn, &auth, None, "task:read")?;
    let task = load_visible_task(&conn, &auth, &id)?;
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

fn repo_root() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

pub async fn list_task_spec_links_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "task:read")?;
    let task = load_visible_task(&conn, &auth, &id)?;
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

    if !spec_change_exists(&repo_root(), &input.spec_change_name) {
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

/// Auto-resolves every task in the caller's org linked to `spec_change_name` to `done`.
/// Org-level `task:write` (not per-project), and NOT scoped by project membership — it may
/// transition tasks across every project in the org (spec §"Resolve-by-spec requires write
/// authority, not caller project membership").
pub async fn resolve_by_spec_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<ResolveBySpecRequest>,
) -> Result<Json<ResolveBySpecResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "task:write")?;
    let resolved = queries::resolve_tasks_by_spec(&conn, &auth.org_id, &input.spec_change_name)
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
    require_permission(&conn, &auth, None, "task:read")?;
    let sprint = load_visible_sprint(&conn, &auth, &id)?;
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
    require_permission(&conn, &auth, None, "task:read")?;
    let sprint = load_visible_sprint(&conn, &auth, &id)?;
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
        // NOTE: get_or_create_project seeds ALL active org users (including `user_id` if
        // already created) as members when it first creates the project row — so this must
        // be idempotent (INSERT OR IGNORE), not a plain INSERT.
        let project_id = q::get_or_create_project(&conn, org_id, project).unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO project_members (id, project_id, user_id, role, created_at)
             VALUES (?1, ?2, ?3, 'member', datetime('now'))",
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
    async fn resolve_by_spec_transitions_across_projects_ignoring_membership() {
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
        let resp = post_json(&store, &admin_key, "/v1/tasks/resolve-by-spec", serde_json::json!({ "spec_change_name": "team-tasks" })).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json["resolved"].as_array().unwrap().len(), 2);
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
}
