use axum::{
    extract::State,
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    api::helpers::require_permission,
    config::Config,
    db::queries as db_queries,
    models::types::{
        ApiError, AuthContext, GitHubAuthUrlResponse, GitHubCallbackRequest,
        GitHubConnectionStatus,
    },
    store::sqlite::SqliteStore,
};

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

fn not_configured() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError {
            error: "GitHub OAuth is not configured on this server".to_string(),
            code: "github_not_configured".to_string(),
        }),
    )
}

/// Private types used only for deserializing GitHub API responses.
#[derive(Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
    token_type: String,
    scope: String,
}

#[derive(Deserialize)]
struct GitHubUserResponse {
    login: String,
    id: i64,
}

/// `GET /v1/github/auth`
///
/// Returns the GitHub OAuth redirect URL. The client should redirect the user to this URL.
/// Returns 503 if GITHUB_CLIENT_ID is not configured.
pub async fn get_auth_url(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Extension(config): Extension<Arc<Config>>,
) -> Result<Json<GitHubAuthUrlResponse>, (StatusCode, Json<ApiError>)> {
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &auth, None, "memory:write")?;
    }

    let client_id = config.github_client_id.as_deref().ok_or_else(not_configured)?;
    let redirect_uri = config.github_redirect_uri.as_deref().unwrap_or("");

    let state = hex::encode(rand::random::<[u8; 16]>());

    let mut url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=repo,user:email&state={}",
        client_id, state
    );
    if !redirect_uri.is_empty() {
        url.push_str(&format!("&redirect_uri={}", redirect_uri));
    }

    Ok(Json(GitHubAuthUrlResponse { url }))
}

/// `POST /v1/github/callback`
///
/// Receives `{ code, state }`, exchanges the code for a GitHub access token,
/// fetches the authenticated user's login and id, then stores the connection in the DB.
pub async fn post_callback(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Extension(config): Extension<Arc<Config>>,
    Json(input): Json<GitHubCallbackRequest>,
) -> Result<Json<GitHubConnectionStatus>, (StatusCode, Json<ApiError>)> {
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &auth, None, "memory:write")?;
    }

    let client_id = config.github_client_id.as_deref().ok_or_else(not_configured)?;
    let client_secret = config.github_client_secret.as_deref().ok_or_else(not_configured)?;
    let redirect_uri = config.github_redirect_uri.as_deref().unwrap_or("").to_string();

    // Exchange code for access token
    let http = reqwest::Client::new();

    let token_body: serde_json::Value = http
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": client_id,
            "client_secret": client_secret,
            "code": input.code,
            "redirect_uri": redirect_uri,
        }))
        .send()
        .await
        .map_err(|_| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "Failed to reach GitHub token endpoint".to_string(),
                    code: "github_token_error".to_string(),
                }),
            )
        })?
        .json()
        .await
        .map_err(|_| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "Failed to communicate with GitHub".to_string(),
                    code: "github_token_error".to_string(),
                }),
            )
        })?;

    // GitHub returns an `error` field for invalid/expired codes rather than an HTTP error status.
    if token_body.get("error").is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "Invalid or expired authorization code".to_string(),
                code: "invalid_grant".to_string(),
            }),
        ));
    }

    let token_res: GitHubTokenResponse = serde_json::from_value(token_body).map_err(|_| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: "Failed to communicate with GitHub".to_string(),
                code: "github_token_error".to_string(),
            }),
        )
    })?;

    // Fetch authenticated user info
    let user_res: GitHubUserResponse = http
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token_res.access_token))
        .header("User-Agent", "nexusmind")
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: format!("Failed to reach GitHub user endpoint: {e}"),
                    code: "github_user_error".to_string(),
                }),
            )
        })?
        .json()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: format!("Invalid response from GitHub user endpoint: {e}"),
                    code: "github_user_error".to_string(),
                }),
            )
        })?;

    // Persist the connection
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::save_github_connection(
            &conn,
            &auth.org_id,
            &token_res.access_token,
            &token_res.token_type,
            &token_res.scope,
            &user_res.login,
            user_res.id,
        )
        .map_err(db_err)?;
    }

    Ok(Json(GitHubConnectionStatus {
        connected: true,
        github_login: Some(user_res.login),
        scopes: Some(token_res.scope),
    }))
}

/// `GET /v1/github/status`
///
/// Returns the GitHub OAuth connection status for the caller's org.
pub async fn get_status(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<GitHubConnectionStatus>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    match db_queries::get_github_connection(&conn, &auth.org_id).map_err(db_err)? {
        Some(gh) => Ok(Json(GitHubConnectionStatus {
            connected: true,
            github_login: Some(gh.github_login),
            scopes: Some(gh.scopes),
        })),
        None => Ok(Json(GitHubConnectionStatus {
            connected: false,
            github_login: None,
            scopes: None,
        })),
    }
}

/// `DELETE /v1/github/connection`
///
/// Removes the stored GitHub OAuth connection for the caller's org. Returns 204 No Content.
pub async fn delete_connection(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &auth, None, "memory:write")?;
    }

    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        db_queries::delete_github_connection(&conn, &auth.org_id).map_err(db_err)?;
    }

    Ok(StatusCode::NO_CONTENT)
}
