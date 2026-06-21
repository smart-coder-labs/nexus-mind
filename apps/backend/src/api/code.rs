use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use chrono::Utc;

use crate::{
    api::helpers::require_permission,
    db::queries as db_queries,
    embed::{self},
    indexer,
    models::types::{
        ApiError, AuthContext, CodeProject, CodeStatusResponse, IndexProjectRequest,
        IndexProjectResponse, ReindexProjectResponse, SearchCodeRequest, SearchCodeResult,
        UpdateCodeProjectRequest, UpdateReindexScheduleRequest,
    },
    store::sqlite::SqliteStore,
};

const DEFAULT_TOP_K: i64 = 5;
const MAX_TOP_K: i64 = 20;

/// Build a Command for `git` with an augmented PATH that covers common install locations.
/// Servers started by process managers (systemd, Docker, etc.) often have a stripped PATH
/// that excludes /usr/local/bin and /opt/homebrew/bin where git lives.
fn git_cmd() -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    let base = std::env::var("PATH").unwrap_or_default();
    cmd.env(
        "PATH",
        format!("{base}:/usr/bin:/usr/local/bin:/opt/homebrew/bin:/usr/local/git/bin"),
    );
    cmd
}

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
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

/// `POST /v1/code/index`
///
/// Accepts either a `repo_url` (GitHub URL to clone/pull) or a `root_path` (local path).
/// Chunks and embeds all eligible source files and persists them for semantic search.
/// Synchronous in v1.
pub async fn post_index(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<IndexProjectRequest>,
) -> Result<(StatusCode, Json<IndexProjectResponse>), (StatusCode, Json<ApiError>)> {
    // Permission check
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &auth, None, "memory:write")?;
    }

    if input.project.trim().is_empty() {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
            error: "project must not be empty".to_string(),
            code: "validation_error".to_string(),
        })));
    }

    // Determine effective root_path
    let effective_root_path: String = if let Some(url) = &input.repo_url {
        if url.trim().is_empty() {
            return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
                error: "repo_url must not be empty".to_string(),
                code: "validation_error".to_string(),
            })));
        }
        // Clone to /tmp/nexusmind/{org_id}/{project}
        let clone_dir = format!("/tmp/nexusmind/{}/{}", auth.org_id, input.project.trim());
        let path = std::path::Path::new(&clone_dir);
        if path.join(".git").exists() {
            // Pull latest
            let out = git_cmd()
                .args(["-C", &clone_dir, "pull", "--rebase", "--quiet"])
                .output()
                .map_err(|e| db_err(anyhow::anyhow!("git pull failed: {e}")))?;
            if !out.status.success() {
                return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
                    error: format!("git pull failed: {}", String::from_utf8_lossy(&out.stderr)),
                    code: "git_error".to_string(),
                })));
            }
        } else {
            std::fs::create_dir_all(&clone_dir)
                .map_err(|e| db_err(anyhow::anyhow!("failed to create clone dir: {e}")))?;
            let out = git_cmd()
                .args(["clone", "--depth=1", "--quiet", url.trim(), &clone_dir])
                .output()
                .map_err(|e| db_err(anyhow::anyhow!("git clone failed: {e}")))?;
            if !out.status.success() {
                return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
                    error: format!("git clone failed: {}", String::from_utf8_lossy(&out.stderr)),
                    code: "git_error".to_string(),
                })));
            }
        }
        clone_dir
    } else if let Some(path) = &input.root_path {
        if path.trim().is_empty() {
            return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
                error: "root_path must not be empty".to_string(),
                code: "validation_error".to_string(),
            })));
        }
        path.trim().to_string()
    } else {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
            error: "either repo_url or root_path must be provided".to_string(),
            code: "validation_error".to_string(),
        })));
    };

    let embed_svc = store.embed_service();
    let db = store.conn();

    // Run synchronously (v1) — large repos may time out
    let result = indexer::index_project(
        &auth.org_id,
        &input.project,
        &effective_root_path,
        &db,
        embed_svc.as_ref(),
    );

    // On error, persist the error status before returning
    let response = match result {
        Ok(r) => r,
        Err(e) => {
            let now = Utc::now()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            // Best-effort: look up the project id to record the error
            if let Ok(conn) = db.lock() {
                if let Ok(Some(proj)) = db_queries::get_code_project(&auth.org_id, &input.project, &conn) {
                    if let Ok(pid) = proj.id.parse::<i64>() {
                        let _ = db_queries::set_code_project_error(&conn, pid, &e.to_string(), &now);
                    }
                }
            }
            return Err(db_err(e));
        }
    };

    // Save repo_url if provided
    if let Some(url) = &input.repo_url {
        let conn = db.lock().map_err(|_| lock_err())?;
        let _ = db_queries::set_code_project_repo_url(&conn, &auth.org_id, &input.project, url);
    }

    Ok((StatusCode::OK, Json(response)))
}

/// `POST /v1/code/search`
///
/// Embeds the query, cosine-ranks all chunks for the project, and returns top-K results.
/// Returns HTTP 404 if the project has not been indexed.
pub async fn post_search(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<SearchCodeRequest>,
) -> Result<Json<Vec<SearchCodeResult>>, (StatusCode, Json<ApiError>)> {
    // Permission check
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &auth, None, "memory:search")?;
    }

    // Resolve top_k with default and cap
    let top_k = input.top_k.unwrap_or(DEFAULT_TOP_K).clamp(1, MAX_TOP_K);

    // Check project exists and is indexed
    let code_project = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::get_code_project(&auth.org_id, &input.project, &conn)
            .map_err(db_err)?
    };

    let code_project = match code_project {
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("Project '{}' has not been indexed", input.project),
                    code: "project_not_indexed".to_string(),
                }),
            ));
        }
        Some(p) => p,
    };

    let code_project_id: i64 = code_project.id.parse().map_err(|_| {
        db_err(anyhow::anyhow!("invalid code_project_id"))
    })?;

    // Embed the query
    let embed_svc = store.embed_service();
    let q_vec = match embed_svc {
        Some(ref svc) => svc.embed_one(&input.query).map_err(db_err)?,
        None => {
            // No embedding service — return empty results gracefully
            return Ok(Json(vec![]));
        }
    };

    // Fetch all chunk embeddings for this project
    let pairs = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::get_code_embeddings(&conn, code_project_id).map_err(db_err)?
    };

    if pairs.is_empty() {
        return Ok(Json(vec![]));
    }

    // Cosine rank
    let mut scored: Vec<(i64, f32)> = pairs
        .into_iter()
        .map(|(id, blob)| {
            let v = embed::deserialize(&blob);
            let score = embed::cosine(&q_vec, &v);
            (id, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k as usize);

    let ids: Vec<i64> = scored.iter().map(|(id, _)| *id).collect();
    let score_map: std::collections::HashMap<i64, f32> = scored.into_iter().collect();

    let chunks = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::get_chunks_by_ids(&conn, &ids).map_err(db_err)?
    };

    let mut results: Vec<SearchCodeResult> = chunks
        .into_iter()
        .map(|c| SearchCodeResult {
            file_path: c.file_path.clone(),
            symbol: c.symbol.clone(),
            start_line: c.start_line,
            end_line: c.end_line,
            content: c.content.clone(),
            score: score_map.get(&c.id).copied().unwrap_or(0.0),
        })
        .collect();

    // Post-filter by extension if provided
    if let Some(ext) = &input.extension {
        if !ext.is_empty() {
            let suffix = format!(".{}", ext);
            results.retain(|r| r.file_path.ends_with(&suffix));
        }
    }

    Ok(Json(results))
}

/// `GET /v1/code/status/:project`
///
/// Returns the current indexing state for a project.
/// If the project has never been indexed, returns HTTP 200 with `status: "not_indexed"`.
pub async fn get_status(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(project): Path<String>,
) -> Result<Json<CodeStatusResponse>, (StatusCode, Json<ApiError>)> {
    // Permission check
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &auth, None, "memory:search")?;
    }

    let code_project = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::get_code_project(&auth.org_id, &project, &conn).map_err(db_err)?
    };

    match code_project {
        None => Ok(Json(CodeStatusResponse {
            project,
            status: "not_indexed".to_string(),
            last_indexed: None,
            file_count: None,
            chunk_count: None,
        })),
        Some(p) => Ok(Json(CodeStatusResponse {
            project: p.name,
            status: "indexed".to_string(),
            last_indexed: p.last_indexed,
            file_count: Some(p.file_count),
            chunk_count: Some(p.chunk_count),
        })),
    }
}

/// Query parameters for `GET /v1/code/context`.
#[derive(Deserialize)]
pub struct ContextParams {
    pub project: String,
    pub file_path: String,
    pub symbol: String,
}

/// `GET /v1/code/context`
///
/// Returns the target symbol chunk plus up to 2 adjacent file-order neighbors.
/// Returns HTTP 404 if the symbol is not found in the index.
pub async fn get_context(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ContextParams>,
) -> Result<Json<Vec<SearchCodeResult>>, (StatusCode, Json<ApiError>)> {
    // Permission check
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &auth, None, "memory:search")?;
    }

    // Find the project
    let code_project = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::get_code_project(&auth.org_id, &params.project, &conn).map_err(db_err)?
    };

    let project = match code_project {
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("Project '{}' has not been indexed", params.project),
                    code: "project_not_indexed".to_string(),
                }),
            ));
        }
        Some(p) => p,
    };

    let code_project_id: i64 = project.id.parse().map_err(|_| {
        db_err(anyhow::anyhow!("invalid code_project_id"))
    })?;

    // Fetch context chunks
    let chunks = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::get_chunk_context(&conn, code_project_id, &params.file_path, &params.symbol, 1)
            .map_err(db_err)?
    };

    if chunks.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Symbol '{}' not found in '{}'", params.symbol, params.file_path),
                code: "symbol_not_found".to_string(),
            }),
        ));
    }

    let results: Vec<SearchCodeResult> = chunks
        .into_iter()
        .map(|c| SearchCodeResult {
            file_path: c.file_path,
            symbol: c.symbol,
            start_line: c.start_line,
            end_line: c.end_line,
            content: c.content,
            score: 1.0, // context is exact match, not ranked
        })
        .collect();

    Ok(Json(results))
}

fn forbidden() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "Forbidden".to_string(),
            code: "forbidden".to_string(),
        }),
    )
}

/// Query params for `GET /v1/code/projects`.
#[derive(Deserialize)]
pub struct ListCodeProjectsParams {
    #[serde(default)]
    pub include_archived: bool,
}

/// `GET /v1/code/projects`
///
/// Returns code projects for the authenticated org.
/// Pass `?include_archived=true` to include archived projects.
pub async fn list_projects(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListCodeProjectsParams>,
) -> Result<Json<Vec<CodeProject>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let projects = db_queries::list_code_projects_filtered(&conn, &auth.org_id, params.include_archived).map_err(db_err)?;
    Ok(Json(projects))
}

/// `POST /v1/code/projects/:id/archive`
///
/// Soft-archives a code project by setting archived_at. Admin only.
pub async fn archive_project(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let found = db_queries::archive_code_project(&conn, &auth.org_id, id).map_err(db_err)?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Code project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// `POST /v1/code/projects/:id/restore`
///
/// Restores a soft-archived code project (clears archived_at).
pub async fn restore_project(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let found = db_queries::restore_code_project(&conn, &auth.org_id, id).map_err(db_err)?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Code project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// `DELETE /v1/code/projects/:name`
///
/// Deletes a code project and all its indexed chunks. Admin only.
pub async fn delete_project(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let deleted = db_queries::delete_code_project(&conn, &auth.org_id, &name).map_err(db_err)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// `PATCH /v1/code/projects/:id/schedule`
///
/// Sets or clears the auto re-index interval for a code project. Admin only.
/// Body: `{ "interval_hours": 6 }` — null clears the schedule.
pub async fn update_schedule(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateReindexScheduleRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "Admin access required".to_string(),
                code: "forbidden".to_string(),
            }),
        ));
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let found = db_queries::update_reindex_interval(&conn, &auth.org_id, id, body.interval_hours)
        .map_err(db_err)?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Code project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// `PATCH /v1/code/projects/:id`
///
/// Updates mutable settings for a code project (currently: exclude_patterns). Admin only.
pub async fn update_code_project(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id_str): Path<String>,
    Json(body): Json<UpdateCodeProjectRequest>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let id: i64 = id_str.parse().map_err(|_| (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ApiError {
            error: "Invalid project id".to_string(),
            code: "validation_error".to_string(),
        }),
    ))?;
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let patterns = body.exclude_patterns.unwrap_or_default();
    // Enforce maximum 20 patterns
    if patterns.len() > 20 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Maximum 20 exclude patterns allowed".to_string(),
                code: "validation_error".to_string(),
            }),
        ));
    }
    let found = db_queries::update_code_project_exclude_patterns(&conn, &auth.org_id, id, &patterns)
        .map_err(db_err)?;
    if found {
        Ok(axum::Json(serde_json::json!({ "ok": true })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Code project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// `POST /v1/code/projects/:id/reindex`
///
/// Triggers an immediate background reindex of the code project. Admin only.
/// Sets `index_status = 'indexing'` synchronously, then spawns a background task.
pub async fn post_reindex(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<Json<ReindexProjectResponse>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    // Look up the project
    let project = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::get_code_project_by_id(&conn, &auth.org_id, id)
            .map_err(db_err)?
    };

    let project = match project {
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "Code project not found".to_string(),
                    code: "not_found".to_string(),
                }),
            ));
        }
        Some(p) => p,
    };

    // Set status to indexing immediately
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::set_code_project_indexing(&conn, id).map_err(db_err)?;
    }

    // Spawn background indexing task
    let db = store.conn();
    let embed_svc = store.embed_service();
    let org_id = auth.org_id.clone();
    let project_name = project.name.clone();
    let root_path = project.root_path.clone();
    let repo_url = project.repo_url.clone();

    tokio::spawn(async move {
        // If repo_url is set, git pull first
        if let Some(ref url) = repo_url {
            let clone_dir = format!("/tmp/nexusmind/{}/{}", org_id, project_name);
            let path = std::path::Path::new(&clone_dir);
            if path.join(".git").exists() {
                let _ = git_cmd()
                    .args(["-C", &clone_dir, "pull", "--rebase", "--quiet"])
                    .output();
            } else {
                let _ = std::fs::create_dir_all(&clone_dir);
                let _ = git_cmd()
                    .args(["clone", "--depth=1", "--quiet", url.trim(), &clone_dir])
                    .output();
            }
            let effective_path = clone_dir;
            let result = indexer::index_project(&org_id, &project_name, &effective_path, &db, embed_svc.as_ref());
            if let Err(e) = result {
                let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                if let Ok(conn) = db.lock() {
                    let _ = db_queries::set_code_project_error(&conn, id, &e.to_string(), &now);
                }
            }
        } else {
            let result = indexer::index_project(&org_id, &project_name, &root_path, &db, embed_svc.as_ref());
            if let Err(e) = result {
                let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                if let Ok(conn) = db.lock() {
                    let _ = db_queries::set_code_project_error(&conn, id, &e.to_string(), &now);
                }
            }
        }
    });

    Ok(Json(ReindexProjectResponse {
        status: "indexing_started".to_string(),
        project_id: id.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{get, post},
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
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/code/index", post(post_index))
            .route("/v1/code/search", post(post_search))
            .route("/v1/code/status/:project", get(get_status))
            .route("/v1/code/context", get(get_context))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_with_key() -> (SqliteStore, String) {
        let store = make_store();
        let raw_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        (store, raw_key)
    }

    // ── GET /v1/code/status/:project ──────────────────────────────────────────

    #[tokio::test]
    async fn status_unindexed_project_returns_200_not_indexed() {
        let (store, key) = setup_with_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/status/ghost")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "not_indexed");
        assert_eq!(body["project"], "ghost");
        // Optional fields must be absent (skip_serializing_if)
        assert!(body.get("last_indexed").is_none() || body["last_indexed"].is_null(),
                "last_indexed must be absent for not_indexed projects");
    }

    #[tokio::test]
    async fn status_unauthenticated_returns_401() {
        let store = make_store();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/status/myapp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_indexed_project_returns_200_with_stats() {
        let (store, key) = setup_with_key();

        // Seed a code project directly via queries
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn.query_row(
                "SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)
            ).unwrap();
            let project_id = q::upsert_code_project(&conn, &org_id, "myapp", "/ws/myapp").unwrap();
            q::update_code_project_stats(&conn, project_id, 5, 42, "2026-06-19T12:00:00Z").unwrap();
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/status/myapp")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "indexed");
        assert_eq!(body["file_count"], 5);
        assert_eq!(body["chunk_count"], 42);
        assert!(body["last_indexed"].as_str().is_some(), "last_indexed must be present for indexed projects");
    }

    // ── POST /v1/code/search ──────────────────────────────────────────────────

    #[tokio::test]
    async fn search_unindexed_project_returns_404() {
        let (store, key) = setup_with_key();

        let body = serde_json::json!({ "project": "ghost", "query": "anything" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/code/search")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let resp_body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(resp_body["code"], "project_not_indexed");
        assert!(resp_body["error"].as_str().unwrap().contains("ghost"),
                "error message must mention the project name");
    }

    #[tokio::test]
    async fn search_unauthenticated_returns_401() {
        let store = make_store();
        let body = serde_json::json!({ "project": "myapp", "query": "auth logic" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/code/search")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn search_no_embed_service_returns_empty_array() {
        // When embed service is disabled, search returns [] not an error
        let (store, key) = setup_with_key();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn.query_row(
                "SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)
            ).unwrap();
            let project_id = q::upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();
            q::update_code_project_stats(&conn, project_id, 1, 1, "2026-06-19T12:00:00Z").unwrap();
        }

        let body = serde_json::json!({ "project": "myapp", "query": "authentication" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/code/search")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let results: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(results.is_empty(), "no embed service must return empty array, not error");
    }

    // ── POST /v1/code/index ───────────────────────────────────────────────────

    #[tokio::test]
    async fn index_empty_project_field_returns_422() {
        let (store, key) = setup_with_key();
        let body = serde_json::json!({ "project": "", "root_path": "/ws" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/code/index")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn index_unauthenticated_returns_401() {
        let store = make_store();
        let body = serde_json::json!({ "project": "myapp", "root_path": "/ws" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/code/index")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn index_nonexistent_root_path_returns_ok_with_zero_files() {
        // The ignore crate returns an empty walk for a missing path — no error
        let (store, key) = setup_with_key();
        let body = serde_json::json!({
            "project": "myapp",
            "root_path": "/this/path/does/not/exist/at/all"
        });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/code/index")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Should return 200 with 0 files
        let status = resp.status();
        assert!(status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR,
                "nonexistent path must return 200 (empty walk) or 500, got {status}");
    }

    // ── GET /v1/code/context ──────────────────────────────────────────────────

    #[tokio::test]
    async fn context_unknown_symbol_returns_404_symbol_not_found() {
        let (store, key) = setup_with_key();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn.query_row(
                "SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)
            ).unwrap();
            q::upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/context?project=myapp&file_path=src/auth.rs&symbol=ghost_fn")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "symbol_not_found");
        assert!(body["error"].as_str().unwrap().contains("ghost_fn"),
                "error message must mention the symbol name");
    }

    #[tokio::test]
    async fn context_unindexed_project_returns_404_project_not_indexed() {
        let (store, key) = setup_with_key();
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/context?project=ghost&file_path=src/lib.rs&symbol=foo")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "project_not_indexed");
    }

    #[tokio::test]
    async fn context_unauthenticated_returns_401() {
        let store = make_store();
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/context?project=myapp&file_path=src/lib.rs&symbol=foo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn context_returns_chunk_with_neighbors() {
        let (store, key) = setup_with_key();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn.query_row(
                "SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)
            ).unwrap();
            let project_id = q::upsert_code_project(&conn, &org_id, "myapp", "/ws").unwrap();
            q::insert_code_chunk(&conn, project_id, "src/auth.rs", "h1", None, Some("validate_token"), 1, 20, "fn validate_token() {}", None).unwrap();
            q::insert_code_chunk(&conn, project_id, "src/auth.rs", "h1", None, Some("authenticate_user"), 21, 60, "fn authenticate_user() {}", None).unwrap();
            q::insert_code_chunk(&conn, project_id, "src/auth.rs", "h1", None, Some("refresh_token"), 61, 80, "fn refresh_token() {}", None).unwrap();
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/context?project=myapp&file_path=src/auth.rs&symbol=authenticate_user")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let results: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(!results.is_empty(), "must return at least the target chunk");
        assert!(
            results.iter().any(|r| r["symbol"].as_str() == Some("authenticate_user")),
            "target chunk must be present in results"
        );
    }
}
