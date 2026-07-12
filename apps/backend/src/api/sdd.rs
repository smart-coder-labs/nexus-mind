//! SDD artifacts — HTTP surface over the store layer in `db::queries`.
//!
//! Two rules run through every handler here:
//!
//! 1. **Not-found and not-visible are both 404.** An org-B caller holding
//!    `sdd:read` who fetches an org-A id gets a 404, never a 403 — a 403 would
//!    confirm the id exists. Same rule as `api/tasks.rs`.
//! 2. **List endpoints never carry content.** A change may hold a 36 KB design
//!    document; only the by-id artifact and revision reads return it.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use std::str::FromStr;

use crate::{
    api::helpers::{require_permission, AppJson},
    db::queries,
    models::types::{
        ApiError, AuthContext, LinkChangeMemoryRequest, Memory, PatchChangeRequest,
        SaveArtifactRequest, SddArtifact, SddArtifactDetail, SddArtifactKind, SddChange,
        SddChangeFilters, SddRevision, SddRevisionMeta, SddSearchHit, Task, UpsertChangeRequest,
    },
    store::sqlite::SqliteStore,
};

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let msg = e.to_string();
    let (status, code) = if msg.starts_with("artifact_too_large") {
        (StatusCode::UNPROCESSABLE_ENTITY, "artifact_too_large")
    } else if msg.starts_with("invalid_phase") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_phase")
    } else if msg.starts_with("invalid_status") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_status")
    } else if msg.starts_with("invalid_kind") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_kind")
    } else if msg.starts_with("invalid_source") {
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_source")
    } else if msg.starts_with("memory_not_found") || msg.starts_with("not_found") {
        // The memory does not exist *from this caller's view*. 404, not 403 and not 422 —
        // a 4xx that distinguished "exists but is another org's" would leak existence.
        (StatusCode::NOT_FOUND, "not_found")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
    };
    (status, Json(ApiError { error: msg, code: code.to_string() }))
}

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError { error: "Not found".to_string(), code: "not_found".to_string() }),
    )
}

// ── Changes ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct ListChangesParams {
    pub project: Option<String>,
    pub status: Option<String>,
    pub phase: Option<String>,
    pub sprint_id: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
}

pub async fn list_changes_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListChangesParams>,
) -> Result<Json<Vec<SddChange>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    let filters = SddChangeFilters {
        project: params.project,
        status: params.status,
        phase: params.phase,
        sprint_id: params.sprint_id,
        include_archived: params.include_archived,
    };
    let changes = queries::list_sdd_changes(&conn, &auth.org_id, &filters).map_err(db_err)?;
    Ok(Json(changes))
}

pub async fn create_change_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<UpsertChangeRequest>,
) -> Result<Json<SddChange>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, Some(&input.project), "sdd:write")?;

    let change = queries::upsert_sdd_change(&conn, &auth.org_id, &auth.user_id, &input)
        .map_err(db_err)?;
    Ok(Json(change))
}

/// Hydrated read: artifact inventory + linked tasks + linked memories.
/// An archived change still returns its full inventory — archiving hides a change
/// from the default list, it does not withdraw its artifacts.
pub async fn get_change_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<SddChange>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    let Some(mut change) = queries::get_sdd_change(&conn, &auth.org_id, &id).map_err(db_err)? else {
        return Err(not_found());
    };

    let viewer = if auth.role.is_privileged() { None } else { Some(auth.user_id.as_str()) };
    change.task_links =
        queries::list_tasks_for_sdd_change(&conn, &auth.org_id, &change.name, viewer)
            .map_err(db_err)?;
    change.memory_links =
        queries::list_sdd_change_memories(&conn, &auth.org_id, &change.id).map_err(db_err)?;
    Ok(Json(change))
}

pub async fn patch_change_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<PatchChangeRequest>,
) -> Result<Json<SddChange>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:write")?;

    if queries::get_sdd_change(&conn, &auth.org_id, &id).map_err(db_err)?.is_none() {
        return Err(not_found());
    }
    let change = queries::patch_sdd_change(&conn, &auth.org_id, &id, &input).map_err(db_err)?;
    Ok(Json(change))
}

pub async fn delete_change_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:delete")?;

    if queries::archive_sdd_change(&conn, &auth.org_id, &id).map_err(db_err)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

pub async fn list_change_artifacts_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SddArtifact>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    let Some(change) = queries::get_sdd_change(&conn, &auth.org_id, &id).map_err(db_err)? else {
        return Err(not_found());
    };
    Ok(Json(change.artifacts))
}

pub async fn list_change_tasks_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Task>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;
    require_permission(&conn, &auth, None, "task:read")?;

    let Some(change) = queries::get_sdd_change(&conn, &auth.org_id, &id).map_err(db_err)? else {
        return Err(not_found());
    };
    let viewer = if auth.role.is_privileged() { None } else { Some(auth.user_id.as_str()) };
    let tasks = queries::list_tasks_for_sdd_change(&conn, &auth.org_id, &change.name, viewer)
        .map_err(db_err)?;
    Ok(Json(tasks))
}

// ── Artifacts ───────────────────────────────────────────────────────────────

/// The workhorse. **Always 200, never 201** — the call is idempotent by content
/// hash, so "created" is not a property of the HTTP status but of the
/// `created_revision` flag in the body.
pub async fn put_artifact_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<SaveArtifactRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, Some(&input.project), "sdd:write")?;

    // Validate the kind before the store is touched, so a bad kind cannot create
    // a change or an artifact on its way to failing.
    SddArtifactKind::from_str(&input.kind)
        .map_err(|_| db_err(anyhow::anyhow!("invalid_kind: {}", input.kind)))?;

    // Provenance, honoured from the body. The importer runs over this endpoint when it
    // targets a remote backend, and hard-coding `agent` here would stamp every imported
    // revision as agent-authored — a lie about where the content came from, and one the
    // DB path does not tell.
    //
    // It is descriptive, not authoritative: `source` grants nothing, so there is nothing
    // to gain by lying about it. An unrecognized value is rejected rather than stored, so
    // the column stays a closed set.
    let source = input.source.as_deref().unwrap_or("agent");
    if !matches!(source, "agent" | "admin" | "import") {
        return Err(db_err(anyhow::anyhow!("invalid_source: {source}")));
    }

    let (artifact, created_revision) =
        queries::upsert_sdd_artifact(&conn, &auth.org_id, &auth.user_id, &input, source)
            .map_err(db_err)?;

    Ok(Json(serde_json::json!({
        "artifact": artifact,
        "created_revision": created_revision,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ArtifactKeyParams {
    pub project: String,
    pub change_name: String,
    pub kind: String,
    pub capability: Option<String>,
}

/// Natural-key read — how an agent fetches "the design of change X" without
/// knowing its id.
pub async fn get_artifact_by_key_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ArtifactKeyParams>,
) -> Result<Json<SddArtifactDetail>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    let found = queries::get_sdd_artifact_by_kind(
        &conn,
        &auth.org_id,
        &params.project,
        &params.change_name,
        &params.kind,
        params.capability.as_deref(),
    )
    .map_err(db_err)?;

    // A kind with no artifact is not-found, never a 200 carrying an empty document —
    // an agent must be able to tell "no design yet" from "an empty design".
    found.map(Json).ok_or_else(not_found)
}

pub async fn get_artifact_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<SddArtifactDetail>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    queries::get_sdd_artifact(&conn, &auth.org_id, &id)
        .map_err(db_err)?
        .map(Json)
        .ok_or_else(not_found)
}

pub async fn list_artifact_revisions_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SddRevisionMeta>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    if queries::get_sdd_artifact(&conn, &auth.org_id, &id).map_err(db_err)?.is_none() {
        return Err(not_found());
    }
    let revisions =
        queries::list_sdd_artifact_revisions(&conn, &auth.org_id, &id).map_err(db_err)?;
    Ok(Json(revisions))
}

pub async fn get_artifact_revision_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, revision)): Path<(String, i64)>,
) -> Result<Json<SddRevision>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    queries::get_sdd_artifact_revision(&conn, &auth.org_id, &id, revision)
        .map_err(db_err)?
        .map(Json)
        .ok_or_else(not_found)
}

// ── Search ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    pub limit: Option<i64>,
}

pub async fn search_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SddSearchHit>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:read")?;

    if params.q.trim().is_empty() {
        return Ok(Json(Vec::new()));
    }
    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    let hits = queries::search_sdd_artifacts(&conn, &auth.org_id, &params.q, limit)
        .map_err(db_err)?;
    Ok(Json(hits))
}

// ── Memory links ────────────────────────────────────────────────────────────

pub async fn link_change_memory_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<LinkChangeMemoryRequest>,
) -> Result<Json<Vec<Memory>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:write")?;

    let relation = input.relation.as_deref().unwrap_or("produced");
    queries::link_sdd_change_memory(
        &conn,
        &auth.org_id,
        &id,
        &input.memory_id,
        relation,
        &auth.user_id,
    )
    .map_err(db_err)?;

    let memories = queries::list_sdd_change_memories(&conn, &auth.org_id, &id).map_err(db_err)?;
    Ok(Json(memories))
}

pub async fn unlink_change_memory_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, memory_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "sdd:write")?;

    if queries::unlink_sdd_change_memory(&conn, &auth.org_id, &id, &memory_id).map_err(db_err)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::Request,
        middleware,
        routing::{delete, get, post},
        Router,
    };
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

    /// Mirrors router.rs exactly, including the static-paths-first ordering.
    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/sdd/search", get(search_handler))
            .route("/v1/sdd/artifacts", get(get_artifact_by_key_handler).put(put_artifact_handler))
            .route("/v1/sdd/artifacts/:id", get(get_artifact_handler))
            .route("/v1/sdd/artifacts/:id/revisions", get(list_artifact_revisions_handler))
            .route("/v1/sdd/artifacts/:id/revisions/:rev", get(get_artifact_revision_handler))
            .route("/v1/sdd/changes", get(list_changes_handler).post(create_change_handler))
            .route(
                "/v1/sdd/changes/:id",
                get(get_change_handler).patch(patch_change_handler).delete(delete_change_handler),
            )
            .route("/v1/sdd/changes/:id/artifacts", get(list_change_artifacts_handler))
            .route("/v1/sdd/changes/:id/tasks", get(list_change_tasks_handler))
            .route("/v1/sdd/changes/:id/memories", post(link_change_memory_handler))
            .route("/v1/sdd/changes/:id/memories/:memory_id", delete(unlink_change_memory_handler))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_with_key() -> (SqliteStore, String, String) {
        let store = make_store();
        let (org_id, raw_key) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _user, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            (org.id, key)
        };
        (store, raw_key, org_id)
    }

    /// Creates a user whose role is a custom role holding EXACTLY `perms` — so a test can
    /// isolate one permission string at a time (e.g. sdd:write without sdd:read).
    fn member_with_perms(store: &SqliteStore, org_id: &str, perms: &[&str]) -> (String, String) {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();

        let role_name = format!("custom-{}", &Uuid::new_v4().to_string()[..8]);
        conn.execute(
            "INSERT INTO roles (id, org_id, name, display_name, permissions)
             VALUES (?1, ?2, ?3, 'Custom', ?4)",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                org_id,
                role_name,
                serde_json::to_string(perms).unwrap()
            ],
        )
        .unwrap();

        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, ?3, 'Test', ?4, 'active', datetime('now'))",
            rusqlite::params![
                user_id,
                org_id,
                format!("{}@test.com", &user_id[..8]),
                role_name
            ],
        )
        .unwrap();

        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![Uuid::new_v4().to_string(), user_id, org_id, key_hash],
        )
        .unwrap();
        (raw_key, user_id)
    }

    /// A second org with its own admin key — for the isolation tests.
    fn second_org(store: &SqliteStore) -> (String, String) {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();
        let org_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Beta', 'beta')",
            [&org_id],
        )
        .unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, 'b@beta.com', 'B', 'admin', 'active', datetime('now'))",
            rusqlite::params![user_id, org_id],
        )
        .unwrap();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![Uuid::new_v4().to_string(), user_id, org_id, key_hash],
        )
        .unwrap();
        (raw_key, org_id)
    }

    async fn req(
        store: &SqliteStore,
        key: &str,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> axum::response::Response {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json");
        let body = match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        };
        app(store.clone()).oneshot(builder.body(body).unwrap()).await.unwrap()
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Recursively asserts no `content` key appears anywhere in the payload.
    fn assert_no_content_key(v: &serde_json::Value, path: &str) {
        match v {
            serde_json::Value::Object(map) => {
                assert!(
                    !map.contains_key("content"),
                    "a list endpoint leaked a `content` key at {path}"
                );
                for (k, val) in map {
                    assert_no_content_key(val, &format!("{path}.{k}"));
                }
            }
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    assert_no_content_key(item, &format!("{path}[{i}]"));
                }
            }
            _ => {}
        }
    }

    async fn save_artifact(store: &SqliteStore, key: &str, change: &str, kind: &str, content: &str) -> serde_json::Value {
        let resp = req(
            store,
            key,
            "PUT",
            "/v1/sdd/artifacts",
            Some(serde_json::json!({
                "project": "nexus-mind",
                "change_name": change,
                "kind": kind,
                "content": content,
            })),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        body_json(resp).await
    }

    fn count(store: &SqliteStore, table: &str) -> i64 {
        let db = store.conn();
        let conn = db.lock().unwrap();
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    // ── Permissions ──────────────────────────────────────────────────────

    /// 3.3
    #[tokio::test]
    async fn list_sdd_changes_denied_without_sdd_read() {
        let (store, _admin, org_id) = setup_with_key();
        let (key, _) = member_with_perms(&store, &org_id, &["sdd:write"]);
        let resp = req(&store, &key, "GET", "/v1/sdd/changes", None).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "sdd:write alone must not grant reads");
    }

    /// 3.9 — privileged roles bypass, with no explicit sdd:* grant anywhere.
    #[tokio::test]
    async fn privileged_role_bypasses_sdd_permission_checks() {
        let (store, admin_key, _org) = setup_with_key();

        let listed = req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await;
        assert_eq!(listed.status(), StatusCode::OK);

        save_artifact(&store, &admin_key, "c", "design", "D").await;

        let created =
            req(&store, &admin_key, "POST", "/v1/sdd/changes", Some(serde_json::json!({
                "project": "nexus-mind", "name": "another"
            })))
            .await;
        assert_eq!(created.status(), StatusCode::OK);
        let id = body_json(created).await["id"].as_str().unwrap().to_string();

        let patched = req(&store, &admin_key, "PATCH", &format!("/v1/sdd/changes/{id}"), Some(serde_json::json!({"phase": "design"}))).await;
        assert_eq!(patched.status(), StatusCode::OK);

        let deleted = req(&store, &admin_key, "DELETE", &format!("/v1/sdd/changes/{id}"), None).await;
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    }

    /// 3.11
    #[tokio::test]
    async fn create_sdd_change_denied_without_sdd_write() {
        let (store, _admin, org_id) = setup_with_key();
        let (key, _) = member_with_perms(&store, &org_id, &["sdd:read"]);
        let resp = req(&store, &key, "POST", "/v1/sdd/changes", Some(serde_json::json!({
            "project": "nexus-mind", "name": "c"
        })))
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(count(&store, "sdd_changes"), 0, "no row created on 403");
    }

    /// 3.25 — sdd:delete is a distinct grant; write does not imply it.
    #[tokio::test]
    async fn delete_sdd_change_requires_sdd_delete_not_just_write() {
        let (store, admin_key, org_id) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let (key, _) = member_with_perms(&store, &org_id, &["sdd:read", "sdd:write"]);
        let resp = req(&store, &key, "DELETE", &format!("/v1/sdd/changes/{id}"), None).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let db = store.conn();
        let conn = db.lock().unwrap();
        let archived: Option<String> = conn
            .query_row("SELECT archived_at FROM sdd_changes WHERE id = ?1", [&id], |r| r.get(0))
            .unwrap();
        assert!(archived.is_none(), "archived_at must stay NULL on a 403");
    }

    /// 3.29
    #[tokio::test]
    async fn put_sdd_artifact_denied_without_sdd_write() {
        let (store, _admin, org_id) = setup_with_key();
        let (key, _) = member_with_perms(&store, &org_id, &["sdd:read"]);
        let resp = req(&store, &key, "PUT", "/v1/sdd/artifacts", Some(serde_json::json!({
            "project": "nexus-mind", "change_name": "c", "kind": "design", "content": "D"
        })))
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(count(&store, "sdd_changes"), 0);
        assert_eq!(count(&store, "sdd_artifacts"), 0);
        assert_eq!(count(&store, "sdd_artifact_revisions"), 0);
    }

    /// 3.51 (first half)
    #[tokio::test]
    async fn search_sdd_artifacts_denied_without_sdd_read() {
        let (store, _admin, org_id) = setup_with_key();
        let (key, _) = member_with_perms(&store, &org_id, &["sdd:write"]);
        let resp = req(&store, &key, "GET", "/v1/sdd/search?q=anything", None).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// 3.57 (first half)
    #[tokio::test]
    async fn link_change_memory_denied_without_sdd_write() {
        let (store, admin_key, org_id) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let (key, _) = member_with_perms(&store, &org_id, &["sdd:read"]);
        let resp = req(&store, &key, "POST", &format!("/v1/sdd/changes/{id}/memories"), Some(serde_json::json!({
            "memory_id": "whatever"
        })))
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(count(&store, "sdd_change_memories"), 0);
    }

    /// 3.66 — every /v1/sdd/* path is behind auth.
    #[tokio::test]
    async fn sdd_routes_require_authentication() {
        let (store, _admin, _org) = setup_with_key();
        for (method, uri) in [
            ("GET", "/v1/sdd/changes"),
            ("POST", "/v1/sdd/changes"),
            ("GET", "/v1/sdd/artifacts?project=p&change_name=c&kind=design"),
            ("PUT", "/v1/sdd/artifacts"),
            ("GET", "/v1/sdd/search?q=x"),
        ] {
            let resp = app(store.clone())
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("Content-Type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {uri} must require authentication"
            );
        }
    }

    // ── Changes ──────────────────────────────────────────────────────────

    /// 3.5
    #[tokio::test]
    async fn list_sdd_changes_returns_metadata_only_never_content() {
        let (store, admin_key, _org) = setup_with_key();
        let big = "a very long design document ".repeat(1_300); // ~36 KB
        save_artifact(&store, &admin_key, "c", "design", &big).await;

        let json = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert_no_content_key(&json, "changes");
    }

    /// 3.7
    #[tokio::test]
    async fn list_sdd_changes_org_isolation() {
        let (store, admin_key, _org_a) = setup_with_key();
        save_artifact(&store, &admin_key, "org-a-change", "design", "secret").await;
        let (key_b, _org_b) = second_org(&store);

        let json = body_json(req(&store, &key_b, "GET", "/v1/sdd/changes", None).await).await;
        assert_eq!(json.as_array().unwrap().len(), 0, "org B must not see org A's changes");
    }

    /// 3.13
    #[tokio::test]
    async fn create_sdd_change_upserts_by_project_and_name() {
        let (store, admin_key, _org) = setup_with_key();
        let body = serde_json::json!({ "project": "nexus-mind", "name": "team-tasks" });

        let first = body_json(req(&store, &admin_key, "POST", "/v1/sdd/changes", Some(body.clone())).await).await;
        let second = body_json(req(&store, &admin_key, "POST", "/v1/sdd/changes", Some(body)).await).await;

        assert_eq!(first["id"], second["id"], "the same (project, name) upserts — no duplicate, no 409");
        assert_eq!(count(&store, "sdd_changes"), 1);
    }

    /// 3.15 — 404, never 403: a 403 would confirm the id exists.
    #[tokio::test]
    async fn get_sdd_change_returns_404_for_other_org_not_403() {
        let (store, admin_key, _org_a) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let (key_b, _org_b) = second_org(&store);
        let resp = req(&store, &key_b, "GET", &format!("/v1/sdd/changes/{id}"), None).await;
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "an org-B admin (who HAS sdd:read) must get 404, not 403 — a 403 would confirm the id exists"
        );

        let unknown = req(&store, &admin_key, "GET", "/v1/sdd/changes/does-not-exist", None).await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    }

    /// 3.17
    #[tokio::test]
    async fn get_sdd_change_hydrates_artifacts_tasks_and_memories() {
        let (store, admin_key, org_id) = setup_with_key();
        save_artifact(&store, &admin_key, "sdd-artifacts", "design", "D").await;
        save_artifact(&store, &admin_key, "sdd-artifacts", "proposal", "P").await;

        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        // A task linked by name, and a memory linked through the API.
        let (task_id, memory_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let user_id = q::list_users(&conn, &org_id).unwrap()[0].id.clone();
            let task = q::create_task(
                &conn,
                &org_id,
                &user_id,
                &crate::models::types::CreateTaskRequest {
                    project: "nexus-mind".into(),
                    title: "PR-1".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            q::link_task_spec(&conn, &task.id, &user_id, "sdd-artifacts").unwrap();

            let memory_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO memories (id, org_id, user_id, tool, content)
                 VALUES (?1, ?2, ?3, 'claude-code', 'a decision')",
                rusqlite::params![memory_id, org_id, user_id],
            )
            .unwrap();
            (task.id, memory_id)
        };

        req(&store, &admin_key, "POST", &format!("/v1/sdd/changes/{id}/memories"), Some(serde_json::json!({
            "memory_id": memory_id
        })))
        .await;

        let json = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/changes/{id}"), None).await).await;
        assert_eq!(json["artifacts"].as_array().unwrap().len(), 2);
        assert_eq!(json["task_links"].as_array().unwrap().len(), 1);
        assert_eq!(json["task_links"][0]["id"], task_id);
        assert_eq!(json["memory_links"].as_array().unwrap().len(), 1);

        // An ARCHIVED change still returns its full inventory.
        req(&store, &admin_key, "DELETE", &format!("/v1/sdd/changes/{id}"), None).await;
        let after = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/changes/{id}"), None).await).await;
        assert_eq!(after["artifacts"].as_array().unwrap().len(), 2, "archiving does not withdraw artifacts");
        assert!(!after["archived_at"].is_null());
    }

    /// 3.19
    #[tokio::test]
    async fn patch_sdd_change_denied_without_sdd_write() {
        let (store, admin_key, org_id) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let (key, _) = member_with_perms(&store, &org_id, &["sdd:read"]);
        let resp = req(&store, &key, "PATCH", &format!("/v1/sdd/changes/{id}"), Some(serde_json::json!({"phase": "design"}))).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let after = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/changes/{id}"), None).await).await;
        assert_eq!(after["phase"], "propose", "unmodified on 403");
    }

    /// 3.21 — the whole patch is rejected, not partially applied.
    #[tokio::test]
    async fn patch_sdd_change_rejects_invalid_phase_with_422_atomically() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let resp = req(&store, &admin_key, "PATCH", &format!("/v1/sdd/changes/{id}"), Some(serde_json::json!({
            "phase": "shipped", "title": "New"
        })))
        .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body_json(resp).await["code"], "invalid_phase");

        let after = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/changes/{id}"), None).await).await;
        assert_eq!(after["phase"], "propose");
        assert!(after["title"].is_null(), "the title in the same rejected patch must NOT have landed");
    }

    /// 3.23 — the identity tuple is not patchable, and saying so is a 4xx, not a shrug.
    #[tokio::test]
    async fn patch_sdd_change_rejects_project_or_name_with_4xx() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        for field in ["project", "name"] {
            let resp = req(&store, &admin_key, "PATCH", &format!("/v1/sdd/changes/{id}"), Some(serde_json::json!({
                field: "hijacked"
            })))
            .await;
            assert!(
                resp.status().is_client_error(),
                "a PATCH carrying `{field}` must be a 4xx, not a silent no-op"
            );
        }

        let after = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/changes/{id}"), None).await).await;
        assert_eq!(after["project"], "nexus-mind");
        assert_eq!(after["name"], "c");
    }

    /// 3.27
    #[tokio::test]
    async fn delete_unknown_or_cross_org_sdd_change_returns_404() {
        let (store, admin_key, _org_a) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let unknown = req(&store, &admin_key, "DELETE", "/v1/sdd/changes/nope", None).await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

        let (key_b, _org_b) = second_org(&store);
        let cross = req(&store, &key_b, "DELETE", &format!("/v1/sdd/changes/{id}"), None).await;
        assert_eq!(cross.status(), StatusCode::NOT_FOUND);
    }

    // ── Artifacts ────────────────────────────────────────────────────────

    /// 3.31 — 200, never 201.
    #[tokio::test]
    async fn put_sdd_artifact_returns_200_not_201_on_first_save() {
        let (store, admin_key, _org) = setup_with_key();
        let json = save_artifact(&store, &admin_key, "c", "design", "D").await;

        assert_eq!(json["created_revision"], true);
        assert_eq!(json["artifact"]["latest_revision"], 1);
    }

    /// 3.33 — the idempotency contract at the HTTP boundary.
    #[tokio::test]
    async fn put_sdd_artifact_second_identical_save_returns_created_revision_false() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "same").await;
        let second = save_artifact(&store, &admin_key, "c", "design", "same").await;

        assert_eq!(second["created_revision"], false);
        let artifact_id = second["artifact"]["id"].as_str().unwrap();

        let revs = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/artifacts/{artifact_id}/revisions"), None).await).await;
        assert_eq!(revs.as_array().unwrap().len(), 1, "still exactly one revision");
    }

    /// 3.35 — A2 at the HTTP boundary. Must be a 422, not a 413.
    #[tokio::test]
    async fn put_sdd_artifact_over_1mb_returns_422_and_creates_nothing() {
        let (store, admin_key, _org) = setup_with_key();
        let huge = "x".repeat(1_048_577);

        let resp = req(&store, &admin_key, "PUT", "/v1/sdd/artifacts", Some(serde_json::json!({
            "project": "nexus-mind", "change_name": "oversized", "kind": "design", "content": huge
        })))
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "must be a 422 from our guard, not a 413 from Axum's body limit"
        );
        assert_eq!(body_json(resp).await["code"], "artifact_too_large");

        assert_eq!(count(&store, "sdd_changes"), 0, "a rejected save leaves NO change");
        assert_eq!(count(&store, "sdd_artifacts"), 0);
        assert_eq!(count(&store, "sdd_artifact_revisions"), 0);
    }

    /// Provenance is honoured from the body, and is a closed set.
    ///
    /// The importer runs over this endpoint when it targets a remote backend. Hard-coding
    /// `agent` here stamped every imported revision as agent-authored — a lie about where
    /// the content came from, and one the DB path does not tell.
    #[tokio::test]
    async fn put_sdd_artifact_honours_the_source_field_and_rejects_unknown_values() {
        let (store, admin_key, _org) = setup_with_key();

        let saved = req(&store, &admin_key, "PUT", "/v1/sdd/artifacts", Some(serde_json::json!({
            "project": "nexus-mind", "change_name": "c", "kind": "design",
            "content": "D", "source": "import"
        })))
        .await;
        assert_eq!(saved.status(), StatusCode::OK);
        let id = body_json(saved).await["artifact"]["id"].as_str().unwrap().to_string();

        let revs = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/artifacts/{id}/revisions"), None).await).await;
        assert_eq!(revs[0]["source"], "import", "the revision must record where it actually came from");

        // Omitted → the default.
        save_artifact(&store, &admin_key, "c2", "design", "D2").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let c2 = list.as_array().unwrap().iter().find(|c| c["name"] == "c2").unwrap();
        let artifact_id = c2["artifacts"][0]["id"].as_str().unwrap();
        let revs2 = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/artifacts/{artifact_id}/revisions"), None).await).await;
        assert_eq!(revs2[0]["source"], "agent", "an omitted source defaults to agent");

        // A value outside the set is rejected, not stored — the column stays closed.
        let bad = req(&store, &admin_key, "PUT", "/v1/sdd/artifacts", Some(serde_json::json!({
            "project": "nexus-mind", "change_name": "c3", "kind": "design",
            "content": "X", "source": "definitely-not-a-source"
        })))
        .await;
        assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body_json(bad).await["code"], "invalid_source");
    }

    /// 3.37
    #[tokio::test]
    async fn put_sdd_artifact_rejects_unknown_kind_with_422_and_creates_nothing() {
        let (store, admin_key, _org) = setup_with_key();
        let resp = req(&store, &admin_key, "PUT", "/v1/sdd/artifacts", Some(serde_json::json!({
            "project": "nexus-mind", "change_name": "c", "kind": "not-a-kind", "content": "X"
        })))
        .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body_json(resp).await["code"], "invalid_kind");

        assert_eq!(count(&store, "sdd_changes"), 0, "a bad kind must not create a change on its way to failing");
        assert_eq!(count(&store, "sdd_artifacts"), 0);
    }

    /// 3.39
    #[tokio::test]
    async fn get_sdd_artifact_by_id_returns_latest_content_and_denies_without_sdd_read() {
        let (store, admin_key, org_id) = setup_with_key();
        let long = "the full design document ".repeat(500);
        save_artifact(&store, &admin_key, "c", "design", "old").await;
        let saved = save_artifact(&store, &admin_key, "c", "design", &long).await;
        let id = saved["artifact"]["id"].as_str().unwrap().to_string();

        let json = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/artifacts/{id}"), None).await).await;
        assert_eq!(json["content"].as_str().unwrap(), long, "complete and untruncated");
        assert_eq!(json["latest_revision"], 2);

        let (key, _) = member_with_perms(&store, &org_id, &["sdd:write"]);
        let denied = req(&store, &key, "GET", &format!("/v1/sdd/artifacts/{id}"), None).await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    }

    /// 3.41 — the natural-key read.
    #[tokio::test]
    async fn get_sdd_artifact_by_natural_key_resolves_change_kind_and_capability() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "sdd-artifacts", "design", "DESIGN BODY").await;

        for (capability, content) in [("sdd-artifact-store", "STORE SPEC"), ("sdd-artifact-links", "LINKS SPEC")] {
            let resp = req(&store, &admin_key, "PUT", "/v1/sdd/artifacts", Some(serde_json::json!({
                "project": "nexus-mind", "change_name": "sdd-artifacts", "kind": "spec",
                "capability": capability, "content": content
            })))
            .await;
            assert_eq!(resp.status(), StatusCode::OK);
        }

        let spec = body_json(req(&store, &admin_key, "GET",
            "/v1/sdd/artifacts?project=nexus-mind&change_name=sdd-artifacts&kind=spec&capability=sdd-artifact-store", None).await).await;
        assert_eq!(spec["content"], "STORE SPEC");

        // Omitting capability resolves the '' sentinel.
        let design = body_json(req(&store, &admin_key, "GET",
            "/v1/sdd/artifacts?project=nexus-mind&change_name=sdd-artifacts&kind=design", None).await).await;
        assert_eq!(design["content"], "DESIGN BODY");

        // A kind with no artifact is a 404, NOT a 200 with empty content.
        let missing = req(&store, &admin_key, "GET",
            "/v1/sdd/artifacts?project=nexus-mind&change_name=sdd-artifacts&kind=tasks", None).await;
        assert_eq!(
            missing.status(),
            StatusCode::NOT_FOUND,
            "an agent must be able to tell 'no tasks.md yet' from 'an empty tasks.md'"
        );
    }

    /// 3.43
    #[tokio::test]
    async fn get_sdd_artifact_from_other_org_returns_404() {
        let (store, admin_key, _org_a) = setup_with_key();
        let saved = save_artifact(&store, &admin_key, "c", "design", "org A only").await;
        let id = saved["artifact"]["id"].as_str().unwrap().to_string();

        let (key_b, _org_b) = second_org(&store);

        let by_id = req(&store, &key_b, "GET", &format!("/v1/sdd/artifacts/{id}"), None).await;
        assert_eq!(by_id.status(), StatusCode::NOT_FOUND);

        let by_key = req(&store, &key_b, "GET",
            "/v1/sdd/artifacts?project=nexus-mind&change_name=c&kind=design", None).await;
        assert_eq!(by_key.status(), StatusCode::NOT_FOUND, "artifacts have no org_id — this proves the join through sdd_changes is there");
    }

    /// 3.45
    #[tokio::test]
    async fn list_artifact_revisions_returns_metadata_without_content() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "v1").await;
        save_artifact(&store, &admin_key, "c", "design", "v2").await;
        let saved = save_artifact(&store, &admin_key, "c", "design", "v3").await;
        let id = saved["artifact"]["id"].as_str().unwrap().to_string();

        let json = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/artifacts/{id}/revisions"), None).await).await;
        let revs = json.as_array().unwrap();
        assert_eq!(revs.len(), 3);
        assert_eq!(revs[0]["revision"], 3, "newest first");
        assert!(revs[0]["content_hash"].is_string());
        assert!(revs[0]["byte_size"].is_number());
        assert_no_content_key(&json, "revisions");
    }

    /// 3.47
    #[tokio::test]
    async fn get_artifact_revision_returns_full_content_for_older_rev() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "first").await;
        save_artifact(&store, &admin_key, "c", "design", "second").await;
        let saved = save_artifact(&store, &admin_key, "c", "design", "third").await;
        let id = saved["artifact"]["id"].as_str().unwrap().to_string();

        let rev1 = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/artifacts/{id}/revisions/1"), None).await).await;
        assert_eq!(rev1["content"], "first");
        assert_eq!(rev1["revision"], 1);

        let missing = req(&store, &admin_key, "GET", &format!("/v1/sdd/artifacts/{id}/revisions/99"), None).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    /// 3.49 — revisions are immutable: no mutating route exists.
    #[tokio::test]
    async fn no_endpoint_mutates_or_deletes_a_revision() {
        let (store, admin_key, _org) = setup_with_key();
        let saved = save_artifact(&store, &admin_key, "c", "design", "original").await;
        let id = saved["artifact"]["id"].as_str().unwrap().to_string();
        let uri = format!("/v1/sdd/artifacts/{id}/revisions/1");

        for method in ["PUT", "PATCH", "DELETE"] {
            let resp = req(&store, &admin_key, method, &uri, Some(serde_json::json!({"content": "tampered"}))).await;
            assert_eq!(
                resp.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} on a revision must be 405 — revisions are immutable"
            );
        }

        let rev1 = body_json(req(&store, &admin_key, "GET", &uri, None).await).await;
        assert_eq!(rev1["content"], "original", "the revision is intact");
    }

    // ── Search ───────────────────────────────────────────────────────────

    /// 3.51 (second half)
    #[tokio::test]
    async fn search_sdd_artifacts_returns_snippets_and_honours_limit() {
        let (store, admin_key, _org) = setup_with_key();
        for i in 0..20 {
            save_artifact(&store, &admin_key, &format!("change-{i}"), "design", "shared TOKENWORD body").await;
        }

        let json = body_json(req(&store, &admin_key, "GET", "/v1/sdd/search?q=TOKENWORD&limit=5", None).await).await;
        let hits = json.as_array().unwrap();
        assert_eq!(hits.len(), 5, "the limit is honoured");
        assert!(hits[0]["snippet"].is_string());
        assert!(hits[0]["change_name"].is_string());
        assert_eq!(hits[0]["kind"], "design");
    }

    /// 3.53
    #[tokio::test]
    async fn search_sdd_artifacts_with_empty_q_returns_empty_list_not_500() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "content").await;

        for uri in ["/v1/sdd/search?q=", "/v1/sdd/search?q=%20%20", "/v1/sdd/search"] {
            let resp = req(&store, &admin_key, "GET", uri, None).await;
            assert_eq!(resp.status(), StatusCode::OK, "{uri} must not 500");
            assert_eq!(body_json(resp).await.as_array().unwrap().len(), 0);
        }
    }

    /// 3.55
    #[tokio::test]
    async fn list_change_artifacts_returns_inventory_without_content() {
        let (store, admin_key, _org) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        save_artifact(&store, &admin_key, "c", "tasks", "T").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let json = body_json(req(&store, &admin_key, "GET", &format!("/v1/sdd/changes/{id}/artifacts"), None).await).await;
        let artifacts = json.as_array().unwrap();
        assert_eq!(artifacts.len(), 2);
        assert!(artifacts[0]["latest_revision"].is_number());
        assert_no_content_key(&json, "artifacts");
    }

    // ── Memory links ─────────────────────────────────────────────────────

    async fn mk_memory(store: &SqliteStore, org_id: &str) -> String {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = q::list_users(&conn, org_id).unwrap()[0].id.clone();
        let memory_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content)
             VALUES (?1, ?2, ?3, 'claude-code', 'a decision')",
            rusqlite::params![memory_id, org_id, user_id],
        )
        .unwrap();
        memory_id
    }

    /// 3.57 (second half) + 3.59 — A3 at the HTTP boundary.
    #[tokio::test]
    async fn relinking_a_memory_with_a_different_relation_updates_it() {
        let (store, admin_key, org_id) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();
        let memory_id = mk_memory(&store, &org_id).await;

        let uri = format!("/v1/sdd/changes/{id}/memories");
        let first = req(&store, &admin_key, "POST", &uri, Some(serde_json::json!({
            "memory_id": memory_id, "relation": "informed"
        })))
        .await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = req(&store, &admin_key, "POST", &uri, Some(serde_json::json!({
            "memory_id": memory_id, "relation": "produced"
        })))
        .await;
        assert_eq!(second.status(), StatusCode::OK, "re-linking is idempotent, not a 409");

        assert_eq!(count(&store, "sdd_change_memories"), 1, "still exactly one link row");

        let db = store.conn();
        let conn = db.lock().unwrap();
        let relation: String = conn
            .query_row("SELECT relation FROM sdd_change_memories WHERE memory_id = ?1", [&memory_id], |r| r.get(0))
            .unwrap();
        assert_eq!(relation, "produced", "a different relation UPDATES the row, it is not ignored");
    }

    /// 3.61 — a cross-org memory id is a 404, not a 403 and not a 422.
    #[tokio::test]
    async fn link_change_memory_with_other_org_memory_returns_404() {
        let (store, admin_key, _org_a) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();

        let (_key_b, org_b) = second_org(&store);
        let foreign_memory = mk_memory(&store, &org_b).await;

        let resp = req(&store, &admin_key, "POST", &format!("/v1/sdd/changes/{id}/memories"), Some(serde_json::json!({
            "memory_id": foreign_memory
        })))
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "the memory does not exist from this caller's view — 404, never 403 or 422"
        );
        assert_eq!(count(&store, "sdd_change_memories"), 0);
    }

    /// 3.63
    #[tokio::test]
    async fn unlink_change_memory_removes_link_but_not_the_memory() {
        let (store, admin_key, org_id) = setup_with_key();
        save_artifact(&store, &admin_key, "c", "design", "D").await;
        let list = body_json(req(&store, &admin_key, "GET", "/v1/sdd/changes", None).await).await;
        let id = list[0]["id"].as_str().unwrap().to_string();
        let memory_id = mk_memory(&store, &org_id).await;

        req(&store, &admin_key, "POST", &format!("/v1/sdd/changes/{id}/memories"), Some(serde_json::json!({
            "memory_id": memory_id
        })))
        .await;

        let uri = format!("/v1/sdd/changes/{id}/memories/{memory_id}");

        let (key, _) = member_with_perms(&store, &org_id, &["sdd:read"]);
        let denied = req(&store, &key, "DELETE", &uri, None).await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);

        let ok = req(&store, &admin_key, "DELETE", &uri, None).await;
        assert_eq!(ok.status(), StatusCode::NO_CONTENT);

        let again = req(&store, &admin_key, "DELETE", &uri, None).await;
        assert_eq!(again.status(), StatusCode::NOT_FOUND, "the link is already gone");

        assert_eq!(count(&store, "memories"), 1, "unlinking must not delete the memory");
    }
}
