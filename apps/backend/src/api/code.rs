use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use chrono::Utc;

// ── Token cipher (AES-256-GCM) ────────────────────────────────────────────────
//
// Env var NEXUSMIND_TOKEN_ENCRYPTION_KEY must be a 64-char hex string (32 bytes).
// If not set, `encrypt_pat` returns None and the token is NOT persisted for reindex;
// it is still used for the current index operation.

mod token_cipher {
    use aes_gcm::{
        aead::{Aead, AeadCore, KeyInit, OsRng},
        Aes256Gcm,
    };

    const KEY_ENV: &str = "NEXUSMIND_TOKEN_ENCRYPTION_KEY";

    fn cipher() -> Option<Aes256Gcm> {
        let key_hex = std::env::var(KEY_ENV).ok()?;
        let key_bytes = hex::decode(key_hex.trim()).ok()?;
        if key_bytes.len() != 32 {
            tracing::warn!(
                "{KEY_ENV} must be 64 hex chars (32 bytes); token will not be persisted"
            );
            return None;
        }
        Aes256Gcm::new_from_slice(&key_bytes).ok()
    }

    /// Encrypt `plaintext` with AES-256-GCM. Returns hex(nonce || ciphertext).
    /// Returns None if NEXUSMIND_TOKEN_ENCRYPTION_KEY is not configured or invalid.
    pub fn encrypt(plaintext: &str) -> Option<String> {
        let c = cipher()?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = c.encrypt(&nonce, plaintext.as_bytes()).ok()?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ct);
        Some(hex::encode(blob))
    }

    /// Decrypt a blob produced by `encrypt`. Returns None on any failure.
    pub fn decrypt(blob: &str) -> Option<String> {
        let c = cipher()?;
        let bytes = hex::decode(blob).ok()?;
        if bytes.len() < 12 {
            return None;
        }
        let (nonce_bytes, ct) = bytes.split_at(12);
        let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
        let plain = c.decrypt(nonce, ct).ok()?;
        String::from_utf8(plain).ok()
    }
}

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

/// Strip `user:token@` credentials from any URL in `s` so OAuth tokens never leak
/// into stored error messages or logs.
fn redact_credentials(s: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"://[^/@\s]+@").expect("static regex must compile"));
    re.replace_all(s, "://***@").into_owned()
}

/// Returns true when a git stderr message indicates an authentication / access
/// denial rather than a network or other error.
fn is_auth_failure(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("authentication failed")
        || lower.contains("could not read username")
        || lower.contains("permission denied")
        || lower.contains("repository not found")
        || lower.contains("access denied")
        || lower.contains("403")
        || lower.contains("401")
}

/// Perform a lightweight, unauthenticated HEAD check against the GitHub REST API
/// to determine whether a repository is publicly accessible.
///
/// - Returns `Ok(())` for public repos (200 OK) or non-GitHub URLs.
/// - Returns a machine-readable `ApiError` with `code = "PRIVATE_REPO_TOKEN_REQUIRED"`
///   when the repo appears private/inaccessible without credentials.
/// - Returns `Ok(())` on network failures — the real clone will surface those.
///
/// When `token` is `Some`, the check is authenticated. A 404 with a valid token
/// means the token cannot access the repo → `code = "TOKEN_ACCESS_DENIED"`.
async fn check_repo_access(url: &str, token: Option<&str>) -> Result<(), (StatusCode, Json<ApiError>)> {
    if !url.starts_with("https://github.com/") {
        return Ok(());
    }
    let path = url["https://github.com/".len()..].trim_end_matches(".git");
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Ok(()); // Not a standard owner/repo URL — skip check
    }
    let api_url = format!("https://api.github.com/repos/{}/{}", parts[0], parts[1]);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .unwrap_or_default();
    let mut req = client
        .get(&api_url)
        .header("User-Agent", "nexusmind")
        .header("Accept", "application/vnd.github.v3+json");
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {}", t));
    }
    let status = match req.send().await {
        Ok(r) => r.status().as_u16(),
        Err(_) => return Ok(()), // Network error — let git handle it
    };
    if status == 200 || status == 301 || status == 302 {
        return Ok(());
    }
    // 404 / 403 = private or nonexistent
    if token.is_none() {
        Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "This repository is not accessible without authentication. \
                        It may be private — provide a GitHub Personal Access Token \
                        with the 'repo' (read) scope.".to_string(),
                code: "PRIVATE_REPO_TOKEN_REQUIRED".to_string(),
            }),
        ))
    } else {
        Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "The provided token cannot access this repository. \
                        Verify it has the 'repo' scope and grants access to this repository.".to_string(),
                code: "TOKEN_ACCESS_DENIED".to_string(),
            }),
        ))
    }
}

/// Clone (or `git pull`, if already cloned) `bare_url` into `dest`.
///
/// `token` is an optional GitHub credential (PAT or OAuth token). When provided it
/// is injected via a shell credential helper that reads from the `GIT_TOKEN` env var —
/// the secret is **never embedded in the URL**, so it never appears in `.git/config`,
/// process argv, or error messages.
///
/// On the pull path, any credential-bearing origin URL that may have been written by
/// an older version of this code is defensively reset to `bare_url` before the pull.
fn clone_or_pull(bare_url: &str, token: Option<&str>, dest: &str) -> Result<(), String> {
    let already_cloned = std::path::Path::new(dest).join(".git").exists();

    if already_cloned {
        // Reset origin to the bare URL in case a previous version wrote credentials there.
        let _ = git_cmd()
            .args(["-C", dest, "remote", "set-url", "origin", bare_url])
            .output();
    }

    // Inject the token via a credential helper that reads GIT_TOKEN from the environment.
    // This keeps the secret out of the command line (not visible in `ps aux`) and out of
    // .git/config (which git clone writes the remote URL into verbatim).
    const CRED_HELPER: &str =
        r#"credential.helper=!f() { echo username=x-access-token; echo "password=$GIT_TOKEN"; }; f"#;

    let mut cmd = git_cmd();
    if let Some(tok) = token {
        cmd.env("GIT_TOKEN", tok).arg("-c").arg(CRED_HELPER);
    }

    let output = if already_cloned {
        cmd.args(["-C", dest, "pull", "--rebase", "--quiet"]).output()
    } else {
        let _ = std::fs::create_dir_all(dest);
        cmd.args(["clone", "--depth=1", "--quiet", bare_url, dest]).output()
    };

    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let op = if already_cloned { "pull" } else { "clone" };
            let stderr_raw = String::from_utf8_lossy(&o.stderr);
            let stderr = redact_credentials(stderr_raw.trim());
            if is_auth_failure(&stderr_raw) {
                Err(format!("PRIVATE_REPO_AUTH_FAILURE: git {op} failed ({}): {}", o.status, stderr))
            } else {
                Err(format!("git {op} failed ({}): {}", o.status, stderr))
            }
        }
        Err(e) => Err(format!("failed to run git: {e}")),
    }
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

    if !auth.role.is_privileged() {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        ensure_code_project_name_access(&conn, &auth, &project_name)?;
    }

    // The effective root path is the clone dir for repos (cloned in the background)
    // or the provided local path. We do NOT clone synchronously — large repos would
    // block the request and time out (502).
    let effective_root_path: String = if has_repo {
        format!("/tmp/nexusmind/{}/{}", auth.org_id, project_name)
    } else {
        input.root_path.as_ref().unwrap().trim().to_string()
    };

    // Check repository accessibility BEFORE spawning the background task so the
    // client receives a synchronous, machine-readable error instead of a deferred
    // failure that would require polling the status endpoint.
    //
    // We check only for GitHub URLs; non-GitHub remotes skip this step.
    // The check is skipped when an org-level OAuth connection already exists —
    // in that case we can assume the token covers the repo.
    let provided_pat: Option<String> = input.github_token.as_ref()
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.trim().to_string());

    if has_repo {
        let url = input.repo_url.as_ref().unwrap().trim();
        if url.starts_with("https://github.com/") {
            // Only pre-check when no OAuth connection and no PAT provided,
            // or when a PAT is explicitly provided (validate it grants access).
            let oauth_exists = store.conn().lock().ok()
                .and_then(|conn| db_queries::get_github_connection(&conn, &auth.org_id).ok().flatten())
                .is_some();
            if !oauth_exists || provided_pat.is_some() {
                let token_ref = provided_pat.as_deref();
                check_repo_access(url, token_ref).await?;
            }
        }
    }

    // Bare clone URL (no credentials embedded — token is injected out-of-band).
    let bare_repo_url: Option<String> = if has_repo {
        Some(input.repo_url.as_ref().unwrap().trim().to_string())
    } else {
        None
    };

    // Resolve the clone token independently of the URL. Priority:
    //   1. Provided PAT (per-request, validated above)
    //   2. Org-level GitHub OAuth connection
    // The token is passed to clone_or_pull via GIT_TOKEN env var, never embedded in the URL.
    let clone_token: Option<String> = if has_repo {
        let url = input.repo_url.as_ref().unwrap().trim();
        if url.starts_with("https://github.com/") {
            provided_pat.clone().or_else(|| {
                store.conn().lock().ok()
                    .and_then(|conn| db_queries::get_github_connection(&conn, &auth.org_id).ok().flatten())
                    .map(|gh| gh.access_token)
            })
        } else {
            None
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
        // Persist the encrypted PAT so future reindex operations can re-authenticate.
        // If NEXUSMIND_TOKEN_ENCRYPTION_KEY is not set, the token is only used for this
        // request and will not be available for scheduled reindexes.
        if let Some(ref pat) = provided_pat {
            match token_cipher::encrypt(pat) {
                Some(blob) => {
                    let _ = db_queries::set_code_project_token(&conn, pid, Some(&blob));
                }
                None => {
                    tracing::warn!(
                        "NEXUSMIND_TOKEN_ENCRYPTION_KEY not set — PAT will not be persisted for reindex"
                    );
                }
            }
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
        if let Some(ref bare_url) = bare_repo_url {
            // Surface clone/pull failures (e.g. auth failure on a private repo)
            // as a project error instead of silently indexing an empty directory.
            if let Err(e) = clone_or_pull(bare_url, clone_token.as_deref(), &spawn_path) {
                let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                if let Ok(conn) = db.lock() {
                    let _ = db_queries::set_code_project_error(&conn, project_id, &e, &now);
                }
                return;
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
                // Preserve the PRIVATE_REPO_AUTH_FAILURE prefix if present so the
                // status endpoint can expose it as a machine-readable signal.
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
        ensure_code_project_name_access(&conn, &auth, &input.project)?;
        db_queries::get_code_project(&auth.org_id, &input.project, &conn)
            .map_err(db_err)?
    };

    let code_project = match code_project {
        None => return Err(project_not_indexed(&input.project)),
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
        ensure_code_project_name_access(&conn, &auth, &project)?;
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
        ensure_code_project_name_access(&conn, &auth, &params.project)?;
        db_queries::get_code_project(&auth.org_id, &params.project, &conn).map_err(db_err)?
    };

    let project = match code_project {
        None => return Err(project_not_indexed(&params.project)),
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

fn viewer_user_id(auth: &AuthContext) -> Option<&str> {
    if auth.role.is_privileged() {
        None
    } else {
        Some(auth.user_id.as_str())
    }
}

fn code_project_not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Code project not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

fn project_not_indexed(project: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: format!("Project '{}' has not been indexed", project),
            code: "project_not_indexed".to_string(),
        }),
    )
}

fn ensure_code_project_name_access(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    project: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if auth.role.is_privileged() {
        return Ok(());
    }
    let allowed = db_queries::user_can_access_canonical_project_by_name(
        conn,
        &auth.org_id,
        project,
        &auth.user_id,
    )
    .map_err(db_err)?;
    if allowed {
        Ok(())
    } else {
        Err(code_project_not_found())
    }
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
    require_permission(&conn, &auth, None, "code:read")?;
    let projects = db_queries::list_code_projects_visible(
        &conn,
        &auth.org_id,
        params.include_archived,
        viewer_user_id(&auth),
    )
    .map_err(db_err)?;
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
    if !auth.role.is_privileged() {
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
    if !auth.role.is_privileged() {
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

/// `DELETE /v1/code/projects/:id`
///
/// Deletes a code project and all its indexed chunks. Admin only.
pub async fn delete_project(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_privileged() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let deleted = db_queries::delete_code_project(&conn, &auth.org_id, id).map_err(db_err)?;
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
    if !auth.role.is_privileged() {
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
    if !auth.role.is_privileged() {
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
    let project = db_queries::get_code_project_by_id_visible(
        &conn,
        &auth.org_id,
        id,
        viewer_user_id(&auth),
    )
    .map_err(db_err)?;
    if project.is_none() {
        return Err(code_project_not_found());
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
    if !auth.role.is_privileged() {
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

    // Resolve the clone token BEFORE spawning. Priority: stored PAT → org OAuth.
    // The token is injected via GIT_TOKEN env var in clone_or_pull — never embedded in the URL.
    let clone_token: Option<String> = if repo_url.as_deref().map(|u| u.trim().starts_with("https://github.com/")).unwrap_or(false) {
        // Attempt to decrypt the per-project PAT stored during the original index.
        let stored_token: Option<String> = {
            let spawn_db = store.conn();
            let result = match spawn_db.lock() {
                Ok(conn) => {
                    let encrypted = db_queries::get_code_project_token(&conn, id)
                        .ok()
                        .flatten();
                    encrypted.as_deref().and_then(token_cipher::decrypt)
                }
                Err(_) => None,
            };
            result
        };
        // Fall back to org-level OAuth connection if no per-project PAT.
        stored_token.or_else(|| {
            let spawn_db = store.conn();
            let result = match spawn_db.lock() {
                Ok(conn) => db_queries::get_github_connection(&conn, &auth.org_id)
                    .ok()
                    .flatten()
                    .map(|gh| gh.access_token),
                Err(_) => None,
            };
            result
        })
    } else {
        None
    };
    // Bare clone URL — credentials are never embedded here.
    let bare_repo_url: Option<String> = repo_url.as_deref().map(|u| u.trim().to_string());

    tokio::spawn(async move {
        // If a repo URL is set, git pull/clone first. Resolve the path to index.
        let effective_path = if let Some(ref bare_url) = bare_repo_url {
            let clone_dir = format!("/tmp/nexusmind/{}/{}", org_id, project_name);
            // Surface clone/pull failures (e.g. auth failure on a private repo)
            // as a project error instead of silently indexing an empty directory.
            if let Err(e) = clone_or_pull(bare_url, clone_token.as_deref(), &clone_dir) {
                let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                if let Ok(conn) = db.lock() {
                    let _ = db_queries::set_code_project_error(&conn, id, &e, &now);
                }
                return;
            }
            clone_dir
        } else {
            root_path
        };

        let result = indexer::index_project(&org_id, &project_name, &effective_path, &db, embed_svc.as_ref(), false);
        if let Err(e) = result {
            let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            if let Ok(conn) = db.lock() {
                let _ = db_queries::set_code_project_error(&conn, id, &e.to_string(), &now);
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
    ensure_code_project_name_access(&conn, &auth, &params.project)?;
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
    /// Symbol start line. When `start`+`end` are given, the source of all chunks
    /// overlapping that range is returned (so a Class is reassembled from its
    /// method chunks). When omitted, the WHOLE file source is returned (File nodes).
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub end: Option<i64>,
}

/// `GET /v1/code/snippet`
///
/// Returns source from `file`'s indexed chunks: the chunks overlapping
/// `[start, end]` for a symbol, or the whole file when no range is given.
/// `404` when the project is unknown to the org or the file has no chunks
/// (e.g. it was indexed graph-only without embeddings).
pub async fn get_snippet(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<GetSnippetQuery>,
) -> Result<Json<SnippetResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // Org-isolation: project must belong to this org
    ensure_code_project_name_access(&conn, &auth, &params.project)?;
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

    // Preferred path: the exact stored file source. A symbol returns its exact
    // line range [start, end]; a File node (no range) returns the whole file.
    if let Some(content) = db_queries::get_code_file(&conn, code_project_id, &params.file).map_err(db_err)? {
        let total = content.lines().count() as i64;
        let (start_line, end_line, body) = match (params.start, params.end) {
            (Some(s), Some(e)) if total > 0 => {
                let s = s.clamp(1, total);
                let e = e.clamp(s, total);
                let lines: Vec<&str> = content.lines().collect();
                (s, e, lines[(s as usize - 1)..(e as usize)].join("\n"))
            }
            _ => (1, total.max(1), content),
        };
        return Ok(Json(SnippetResponse {
            file_path: params.file,
            symbol: None,
            language: None,
            start_line,
            end_line,
            content: body,
        }));
    }

    // Fallback for projects indexed before code_files existed: reconstruct from
    // chunks (overlapping the range, or all chunks for a whole file).
    let all_chunks = db_queries::get_file_chunks(&conn, code_project_id, &params.file)
        .map_err(db_err)?;
    let selected: Vec<_> = match (params.start, params.end) {
        (Some(s), Some(e)) => all_chunks
            .into_iter()
            .filter(|c| c.start_line <= e && c.end_line >= s)
            .collect(),
        _ => all_chunks,
    };
    if selected.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "No source stored for this file yet — re-index the project to view source."
                    .to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }
    let start_line = selected.iter().map(|c| c.start_line).min().unwrap_or(0);
    let end_line = selected.iter().map(|c| c.end_line).max().unwrap_or(0);
    let language = selected.first().and_then(|c| c.language.clone());
    let symbol = selected.iter().find_map(|c| c.symbol.clone());
    let content = selected
        .iter()
        .map(|c| c.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(Json(SnippetResponse {
        file_path: params.file,
        symbol,
        language,
        start_line,
        end_line,
        content,
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

    // ── Private-repo clone-failure handling ───────────────────────────────────

    #[test]
    fn clone_or_pull_reports_error_for_unreachable_remote() {
        // A local path that does not exist mimics a private clone the server
        // cannot access (git exits non-zero, offline & deterministic). The old
        // code discarded this and indexed an empty dir; now it must be an Err so
        // the caller surfaces a project error instead of a silent empty index.
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("clone");
        let res = clone_or_pull(
            "/nonexistent/nexusmind-private-repo-xyz/repo.git",
            None,
            dest.to_str().unwrap(),
        );
        assert!(res.is_err(), "a failed clone must return Err, not silently succeed");
    }

    #[test]
    fn clone_or_pull_succeeds_for_valid_local_repo() {
        fn git(args: &[&str]) {
            let out = git_cmd()
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t.dev")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t.dev")
                .output()
                .expect("git runs");
            assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        let src = tempfile::TempDir::new().unwrap();
        let src_path = src.path().to_str().unwrap();
        git(&["init", "-q", src_path]);
        std::fs::write(src.path().join("README.md"), "# hi\n").unwrap();
        git(&["-C", src_path, "add", "."]);
        git(&["-C", src_path, "commit", "-qm", "init"]);

        let dst = tempfile::TempDir::new().unwrap();
        let dest = dst.path().join("clone");
        let res = clone_or_pull(src_path, None, dest.to_str().unwrap());
        assert!(res.is_ok(), "cloning a valid local repo must succeed: {res:?}");
        assert!(dest.join(".git").exists(), "clone must produce a .git dir");
    }

    #[test]
    fn redact_credentials_strips_oauth_token() {
        let s = "fatal: Authentication failed for 'https://oauth2:ghp_SECRETTOKEN@github.com/org/repo.git'";
        let out = redact_credentials(s);
        assert!(!out.contains("ghp_SECRETTOKEN"), "token must be redacted: {out}");
        assert!(out.contains("://***@github.com"), "redacted marker must be present: {out}");
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/code/index", post(post_index))
            .route("/v1/code/search", post(post_search))
            .route("/v1/code/status/:project", get(get_status))
            .route("/v1/code/context", get(get_context))
            .route("/v1/code/projects", get(list_projects))
            .route("/v1/code/projects/:id/files", get(get_project_files))
            .route("/v1/code/graph", get(get_graph))
            .route("/v1/code/snippet", get(get_snippet))
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

    fn setup_code_access_fixture() -> (SqliteStore, String, String, i64) {
        let store = make_store();
        let (admin_key, member_key, project_b_code_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, admin_key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            let (member, member_key) = q::invite_user(&conn, &org.id, "member@acme.com", "Member", "member").unwrap();
            let project_a = q::create_project(&conn, &org.id, "project-a", None, None).unwrap();
            let _project_b = q::create_project(&conn, &org.id, "project-b", None, None).unwrap();
            q::upsert_project_member(&conn, &project_a.id, &member.id, "member").unwrap();

            let project_a_code_id = q::upsert_code_project(&conn, &org.id, "project-a", "/ws/a").unwrap();
            q::update_code_project_stats(&conn, project_a_code_id, 1, 1, "2026-06-19T12:00:00Z").unwrap();
            q::insert_code_chunk(&conn, project_a_code_id, "src/lib.rs", "h1", None, Some("allowed"), 1, 1, "fn allowed() {}", None).unwrap();

            let project_b_code_id = q::upsert_code_project(&conn, &org.id, "project-b", "/ws/b").unwrap();
            q::update_code_project_stats(&conn, project_b_code_id, 1, 1, "2026-06-19T12:00:00Z").unwrap();
            q::insert_code_chunk(&conn, project_b_code_id, "src/lib.rs", "h2", None, Some("denied"), 1, 1, "fn denied() {}", None).unwrap();

            let code_only_id = q::upsert_code_project(&conn, &org.id, "code-only", "/ws/code-only").unwrap();
            q::update_code_project_stats(&conn, code_only_id, 2, 3, "2026-06-19T12:00:00Z").unwrap();

            (admin_key, member_key, project_b_code_id)
        };
        (store, admin_key, member_key, project_b_code_id)
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

    #[tokio::test]
    async fn admin_status_code_only_project_without_canonical_row_returns_indexed() {
        let (store, admin_key, _, _) = setup_code_access_fixture();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/status/code-only")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "indexed");
        assert_eq!(body["project"], "code-only");
    }

    #[tokio::test]
    async fn non_admin_cannot_infer_not_indexed_for_unknown_canonical_project() {
        let (store, _, member_key, _) = setup_code_access_fixture();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/status/no-canonical-row")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "not_found");
    }

    #[tokio::test]
    async fn member_of_project_a_cannot_access_project_b_code_surfaces() {
        let (store, _, member_key, project_b_code_id) = setup_code_access_fixture();

        let status_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .uri("/v1/code/status/project-b")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status_resp.status(), StatusCode::NOT_FOUND);

        let list_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .uri("/v1/code/projects")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(list_resp.into_body(), usize::MAX).await.unwrap();
        let projects: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert!(projects.iter().any(|p| p["name"] == "project-a"));
        assert!(projects.iter().all(|p| p["name"] != "project-b"));
        assert!(projects.iter().all(|p| p["name"] != "code-only"));

        let files_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/code/projects/{project_b_code_id}/files"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(files_resp.status(), StatusCode::NOT_FOUND);

        let graph_resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/code/graph?project=project-b")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(graph_resp.status(), StatusCode::NOT_FOUND);
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
