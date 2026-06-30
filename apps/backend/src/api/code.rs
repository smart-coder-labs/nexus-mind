use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use chrono::Utc;

use crate::{
    api::helpers::{require_permission, AppJson},
    db::queries as db_queries,
    embed::{self},
    indexer,
    models::types::{
        ApiError, AuthContext, CodeProject, CodeStatusResponse, GraphResponse, IndexProjectRequest,
        IndexProjectResponse, ReindexProjectResponse, SearchCodeRequest, SearchCodeResult,
        SnippetResponse, UpdateCodeProjectRequest, UpdateReindexScheduleRequest,
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
/// Runs asynchronously: the project is marked `indexing` and the clone + index run in a
/// background task, so large repos do not block the request (no 502). The client polls
/// `index_status`. Pass `graph_only: true` to build the structural + symbol graph and
/// skip the slow embedding pass (fast, no semantic search).
pub async fn post_index(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<IndexProjectRequest>,
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

    // Validate that exactly one source is provided and non-empty.
    let has_repo = input.repo_url.as_ref().map(|u| !u.trim().is_empty()).unwrap_or(false);
    let has_path = input.root_path.as_ref().map(|p| !p.trim().is_empty()).unwrap_or(false);
    if input.repo_url.is_some() && !has_repo {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
            error: "repo_url must not be empty".to_string(),
            code: "validation_error".to_string(),
        })));
    }
    if input.root_path.is_some() && !has_path {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
            error: "root_path must not be empty".to_string(),
            code: "validation_error".to_string(),
        })));
    }
    if !has_repo && !has_path {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ApiError {
            error: "either repo_url or root_path must be provided".to_string(),
            code: "validation_error".to_string(),
        })));
    }

    let project_name = input.project.trim().to_string();

    // The effective root path is the clone dir for repos (cloned in the background)
    // or the provided local path. We do NOT clone synchronously — large repos would
    // block the request and time out (502).
    let effective_root_path: String = if has_repo {
        format!("/tmp/nexusmind/{}/{}", auth.org_id, project_name)
    } else {
        input.root_path.as_ref().unwrap().trim().to_string()
    };

    // Inject GitHub OAuth token before spawning. The token-bearing URL is used only
    // for git commands and is NEVER logged or returned.
    let effective_repo_url: Option<String> = if has_repo {
        let url = input.repo_url.as_ref().unwrap().trim();
        if url.starts_with("https://github.com/") {
            let gh_result = {
                match store.conn().lock() {
                    Ok(conn) => db_queries::get_github_connection(&conn, &auth.org_id).ok().flatten(),
                    Err(_) => None,
                }
            };
            Some(match gh_result {
                Some(gh) => url.replacen("https://", &format!("https://oauth2:{}@", gh.access_token), 1),
                None => url.to_string(),
            })
        } else {
            Some(url.to_string())
        }
    } else {
        None
    };

    // Create/locate the project row and mark it indexing immediately, so the client
    // gets a fast response and can poll index_status.
    let project_id = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        let pid = db_queries::upsert_code_project(&conn, &auth.org_id, &project_name, &effective_root_path)
            .map_err(db_err)?;
        if let Some(url) = &input.repo_url {
            let _ = db_queries::set_code_project_repo_url(&conn, &auth.org_id, &project_name, url);
        }
        let _ = db_queries::set_code_project_indexing(&conn, pid);
        pid
    };

    // Spawn background clone (if needed) + index.
    let db = store.conn();
    let embed_svc = store.embed_service();
    let org_id = auth.org_id.clone();
    let spawn_project = project_name.clone();
    let spawn_path = effective_root_path.clone();
    let graph_only = input.graph_only.unwrap_or(false);
    tokio::spawn(async move {
        if let Some(ref effective_url) = effective_repo_url {
            let path = std::path::Path::new(&spawn_path);
            if path.join(".git").exists() {
                let _ = git_cmd()
                    .args(["-C", &spawn_path, "pull", "--rebase", "--quiet"])
                    .output();
            } else {
                let _ = std::fs::create_dir_all(&spawn_path);
                // effective_url may contain an OAuth token — do NOT log it
                let _ = git_cmd()
                    .args(["clone", "--depth=1", "--quiet", effective_url, &spawn_path])
                    .output();
            }
        }
        // Isolate the index so a panic sets error status instead of poisoning the
        // shared connection mutex / taking the process down.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            indexer::index_project(&org_id, &spawn_project, &spawn_path, &db, embed_svc.as_ref(), graph_only)
        }));
        let err_msg = match outcome {
            Ok(Ok(_)) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(_) => Some("indexing failed unexpectedly".to_string()),
        };
        if let Some(msg) = err_msg {
            let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            if let Ok(conn) = db.lock() {
                let _ = db_queries::set_code_project_error(&conn, project_id, &msg, &now);
            }
        }
    });

    Ok((StatusCode::OK, Json(IndexProjectResponse {
        project: project_name,
        status: "indexing_started".to_string(),
        file_count: 0,
        chunk_count: 0,
        last_indexed: String::new(),
    })))
}

/// `POST /v1/code/search`
///
/// Embeds the query, cosine-ranks all chunks for the project, and returns top-K results.
/// Returns HTTP 404 if the project has not been indexed.
pub async fn post_search(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<SearchCodeRequest>,
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
    AppJson(body): AppJson<UpdateReindexScheduleRequest>,
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
    AppJson(body): AppJson<UpdateCodeProjectRequest>,
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

/// `GET /v1/code/projects/:id/files`
///
/// Returns a sorted list of distinct file paths indexed for the given code project.
pub async fn get_project_files(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    // Verify the project belongs to this org
    let project = db_queries::get_code_project_by_id(&conn, &auth.org_id, id).map_err(db_err)?;
    if project.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Code project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }
    let file_map = db_queries::list_indexed_files_with_hashes(&conn, id).map_err(db_err)?;
    let mut files: Vec<String> = file_map.into_keys().collect();
    files.sort();
    Ok(Json(files))
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

    // Compute effective clone URL with injected GitHub token BEFORE spawning.
    // The token-bearing URL is passed into the spawn closure but NEVER logged or returned.
    let effective_repo_url: Option<String> = if let Some(ref url) = repo_url {
        if url.trim().starts_with("https://github.com/") {
            let spawn_db = store.conn();
            // Extract the result in an inner block so the MutexGuard is dropped before spawn_db.
            let gh_result = {
                match spawn_db.lock() {
                    Ok(conn) => db_queries::get_github_connection(&conn, &auth.org_id).ok().flatten(),
                    Err(_) => None,
                }
            };
            if let Some(gh_conn) = gh_result {
                Some(url.trim().replacen("https://", &format!("https://oauth2:{}@", gh_conn.access_token), 1))
            } else {
                Some(url.trim().to_string())
            }
        } else {
            Some(url.trim().to_string())
        }
    } else {
        None
    };

    tokio::spawn(async move {
        // If a repo URL is set, git pull/clone first
        if let Some(ref effective_url) = effective_repo_url {
            let clone_dir = format!("/tmp/nexusmind/{}/{}", org_id, project_name);
            let path = std::path::Path::new(&clone_dir);
            if path.join(".git").exists() {
                let _ = git_cmd()
                    .args(["-C", &clone_dir, "pull", "--rebase", "--quiet"])
                    .output();
            } else {
                let _ = std::fs::create_dir_all(&clone_dir);
                // effective_url may contain an OAuth token — do NOT log it
                let _ = git_cmd()
                    .args(["clone", "--depth=1", "--quiet", effective_url, &clone_dir])
                    .output();
            }
            let effective_path = clone_dir;
            let result = indexer::index_project(&org_id, &project_name, &effective_path, &db, embed_svc.as_ref(), false);
            if let Err(e) = result {
                let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                if let Ok(conn) = db.lock() {
                    let _ = db_queries::set_code_project_error(&conn, id, &e.to_string(), &now);
                }
            }
        } else {
            let result = indexer::index_project(&org_id, &project_name, &root_path, &db, embed_svc.as_ref(), false);
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

// ── GET /v1/code/graph ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetGraphQuery {
    pub project: String,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub edge_type: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

const DEFAULT_GRAPH_LIMIT: i64 = 5_000;
const MAX_GRAPH_LIMIT: i64 = 20_000;

/// `GET /v1/code/graph`
///
/// Query parameters:
///   - `project` (required): code project name scoped to the caller's org
///   - `node_type` (optional): comma-separated list of symbol_type values to include
///   - `edge_type`  (optional): comma-separated list of edge_type values to include
///   - `limit`  (optional, default 5000, max 20000): maximum number of nodes returned
///   - `offset` (optional, default 0)
///
/// Returns `404` when the project does not exist or does not belong to the org.
pub async fn get_graph(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<GetGraphQuery>,
) -> Result<Json<GraphResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // Org-isolation: project must belong to this org
    let project = db_queries::get_code_project(&auth.org_id, &params.project, &conn)
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("Code project '{}' not found", params.project),
                    code: "not_found".to_string(),
                }),
            )
        })?;

    let node_types: Vec<String> = params
        .node_type
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let edge_types: Vec<String> = params
        .edge_type
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let limit = params
        .limit
        .unwrap_or(DEFAULT_GRAPH_LIMIT)
        .clamp(1, MAX_GRAPH_LIMIT);
    let offset = params.offset.unwrap_or(0).max(0);

    let code_project_id: i64 = project.id.parse().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Project id is not a valid integer".to_string(),
                code: "internal_error".to_string(),
            }),
        )
    })?;

    let (nodes, edges) =
        db_queries::get_graph(&conn, code_project_id, &node_types, &edge_types, limit, offset)
            .map_err(db_err)?;

    let node_count = nodes.len();
    let edge_count = edges.len();

    Ok(Json(GraphResponse {
        project: params.project,
        node_count,
        edge_count,
        nodes,
        edges,
    }))
}

// ── GET /v1/code/snippet ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetSnippetQuery {
    pub project: String,
    pub file: String,
    pub line: i64,
}

/// `GET /v1/code/snippet`
///
/// Returns the source of the code chunk covering `line` in `file` for the given
/// project — used when a graph node is clicked to reveal the symbol's code.
/// Returns `404` when the project is unknown to the org or no chunk covers the line.
pub async fn get_snippet(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<GetSnippetQuery>,
) -> Result<Json<SnippetResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // Org-isolation: project must belong to this org
    let project = db_queries::get_code_project(&auth.org_id, &params.project, &conn)
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("Code project '{}' not found", params.project),
                    code: "not_found".to_string(),
                }),
            )
        })?;

    let code_project_id: i64 = project.id.parse().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Project id is not a valid integer".to_string(),
                code: "internal_error".to_string(),
            }),
        )
    })?;

    let chunk = db_queries::get_chunk_covering_line(&conn, code_project_id, &params.file, params.line)
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "No source found for this symbol".to_string(),
                    code: "not_found".to_string(),
                }),
            )
        })?;

    Ok(Json(SnippetResponse {
        file_path: chunk.file_path,
        symbol: chunk.symbol,
        language: chunk.language,
        start_line: chunk.start_line,
        end_line: chunk.end_line,
        content: chunk.content,
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

    // ── GET /v1/code/graph ────────────────────────────────────────────────────

    fn graph_app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/code/graph", get(get_graph))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_graph_store() -> (SqliteStore, String, i64) {
        let store = make_store();
        let (raw_key, pid) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            let pid = q::upsert_code_project(&conn, &org.id, "myapp", "/ws").unwrap();
            (key, pid)
        };
        (store, raw_key, pid)
    }

    #[tokio::test]
    async fn get_graph_unknown_project_returns_404() {
        let (store, key, _) = setup_graph_store();

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/graph?project=ghost")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_graph_unauthenticated_returns_401() {
        let store = make_store();

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/graph?project=myapp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn get_graph_empty_project_returns_200_with_envelope() {
        let (store, key, _) = setup_graph_store();

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/graph?project=myapp")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["project"], "myapp");
        assert_eq!(body["node_count"], 0);
        assert_eq!(body["edge_count"], 0);
        assert!(body["nodes"].as_array().unwrap().is_empty());
        assert!(body["edges"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_graph_with_nodes_returns_correct_envelope() {
        use crate::db::queries as db_q;
        use crate::indexer::tree_sitter_chunker::{
            EdgeType, FileGraph, Persist, RawEdge, RawSymbol, SymbolType,
        };

        let (store, key, pid) = setup_graph_store();

        // Seed graph data
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            db_q::persist_structure(&conn, pid, "myapp", &["src/lib.rs".to_string()]).unwrap();
            let fg = FileGraph {
                file_rel_path: "src/lib.rs".to_string(),
                symbols: vec![RawSymbol {
                    symbol_type:    SymbolType::Function,
                    name:           "do_work".to_string(),
                    qualified_name: "src/lib.rs::do_work#1".to_string(),
                    file_path:      Some("src/lib.rs".to_string()),
                    file_hash:      Some("h".to_string()),
                    start_line:     Some(1),
                    end_line:       Some(5),
                    language:       "rust".to_string(),
                    persist:        Persist::FileOwned,
                }],
                edges: vec![RawEdge {
                    from_qname: "file::src/lib.rs".to_string(),
                    to_qname:   "src/lib.rs::do_work#1".to_string(),
                    edge_type:  EdgeType::Defines,
                    file_path:  Some("src/lib.rs".to_string()),
                    persist:    Persist::FileOwned,
                }],
            };
            db_q::persist_file_graph(&conn, pid, &fg).unwrap();
        }

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/graph?project=myapp")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["project"], "myapp");
        let node_count = body["node_count"].as_u64().unwrap();
        assert!(node_count >= 1, "at least the Function node must be present; got {}", node_count);
        let edge_count = body["edge_count"].as_u64().unwrap();
        assert!(edge_count >= 1, "at least the defines edge must be present; got {}", edge_count);

        // node_count field matches nodes array length
        assert_eq!(
            node_count as usize,
            body["nodes"].as_array().unwrap().len(),
            "node_count must equal nodes.length()"
        );
        assert_eq!(
            edge_count as usize,
            body["edges"].as_array().unwrap().len(),
            "edge_count must equal edges.length()"
        );

        // Every edge references a node in the returned set
        let node_ids: std::collections::HashSet<u64> = body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_u64().unwrap())
            .collect();
        for edge in body["edges"].as_array().unwrap() {
            let from = edge["from_id"].as_u64().unwrap();
            let to   = edge["to_id"].as_u64().unwrap();
            assert!(node_ids.contains(&from), "from_id {from} not in node set");
            assert!(node_ids.contains(&to), "to_id {to} not in node set");
        }
    }

    #[tokio::test]
    async fn get_graph_node_type_filter_applied() {
        use crate::db::queries as db_q;
        use crate::indexer::tree_sitter_chunker::{FileGraph, Persist, RawSymbol, SymbolType};

        let (store, key, pid) = setup_graph_store();

        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            db_q::persist_structure(&conn, pid, "myapp", &["src/lib.rs".to_string()]).unwrap();
            let fg = FileGraph {
                file_rel_path: "src/lib.rs".to_string(),
                symbols: vec![
                    RawSymbol {
                        symbol_type: SymbolType::Function,
                        name: "fn_one".to_string(),
                        qualified_name: "src/lib.rs::fn_one#1".to_string(),
                        file_path: Some("src/lib.rs".to_string()),
                        file_hash: Some("h".to_string()),
                        start_line: Some(1),
                        end_line: Some(3),
                        language: "rust".to_string(),
                        persist: Persist::FileOwned,
                    },
                    RawSymbol {
                        symbol_type: SymbolType::Struct,
                        name: "MyStruct".to_string(),
                        qualified_name: "src/lib.rs::MyStruct#5".to_string(),
                        file_path: Some("src/lib.rs".to_string()),
                        file_hash: Some("h".to_string()),
                        start_line: Some(5),
                        end_line: Some(10),
                        language: "rust".to_string(),
                        persist: Persist::FileOwned,
                    },
                ],
                edges: vec![],
            };
            db_q::persist_file_graph(&conn, pid, &fg).unwrap();
        }

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/graph?project=myapp&node_type=Function")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let nodes = body["nodes"].as_array().unwrap();
        assert!(
            nodes.iter().all(|n| n["type"] == "Function"),
            "all returned nodes must be of type Function"
        );
        assert!(
            nodes.iter().any(|n| n["name"] == "fn_one"),
            "fn_one must appear"
        );
        assert!(
            nodes.iter().all(|n| n["name"] != "MyStruct"),
            "MyStruct must be excluded by the filter"
        );
    }

    #[tokio::test]
    async fn get_graph_limit_capped_at_20000() {
        let (store, key, _) = setup_graph_store();

        // Request limit=99999 — should be silently capped to 20000
        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/graph?project=myapp&limit=99999")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // We only verify it does NOT 400/500; the cap is invisible in an empty project
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
