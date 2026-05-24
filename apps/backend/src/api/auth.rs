use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    auth::password::verify_password,
    db::queries,
    email::{send_password_reset, EmailConfig},
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

fn internal_err(msg: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: msg.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn unauth(msg: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiError {
            error: msg.to_string(),
            code: "unauthorized".to_string(),
        }),
    )
}

fn bad_request(msg: &str, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: msg.to_string(),
            code: code.to_string(),
        }),
    )
}

// ── POST /v1/admin/auth/login ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(store): State<SqliteStore>,
    Json(input): Json<LoginInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    let (user, password_hash_opt) = queries::find_admin_by_email(&conn, &input.email)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| unauth("Invalid email or password"))?;

    let password_hash = password_hash_opt
        .ok_or_else(|| unauth("Password not set. Check your email for a setup link."))?;

    let valid = verify_password(&input.password, &password_hash)
        .map_err(|_| internal_err("password verification error"))?;

    if !valid {
        return Err(unauth("Invalid email or password"));
    }

    let org = queries::get_org(&conn, &user.org_id)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| internal_err("org not found"))?;

    let raw_key = queries::create_web_session_key(&conn, &user.id, &user.org_id)
        .map_err(|_| internal_err("failed to create session"))?;

    Ok(Json(serde_json::json!({
        "api_key": raw_key,
        "org": org,
        "user": user,
    })))
}

// ── POST /v1/admin/auth/set-password ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetPasswordInput {
    pub token: String,
    pub password: String,
}

pub async fn set_password(
    State(store): State<SqliteStore>,
    Json(input): Json<SetPasswordInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    if input.password.len() < 8 {
        return Err(bad_request("Password must be at least 8 characters", "password_too_short"));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    let user_id = queries::validate_and_consume_reset_token(&conn, &input.token)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| bad_request("Invalid or expired token", "invalid_token"))?;

    let hashed = crate::auth::password::hash_password(&input.password)
        .map_err(|_| internal_err("password hashing error"))?;

    queries::set_user_password(&conn, &user_id, &hashed)
        .map_err(|_| internal_err("db error"))?;

    Ok(Json(serde_json::json!({ "message": "Password set successfully" })))
}

// ── POST /v1/admin/auth/request-reset ────────────────────────────────────────

#[derive(Deserialize)]
pub struct RequestResetInput {
    pub email: String,
}

pub async fn request_reset(
    State(store): State<SqliteStore>,
    Extension(email_config): Extension<Option<Arc<EmailConfig>>>,
    Json(input): Json<RequestResetInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    let (user, _) = queries::find_admin_by_email(&conn, &input.email)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| bad_request("No account found with that email address.", "email_not_found"))?;

    let (raw_token, _) = queries::create_password_reset_token(&conn, &user.id)
        .map_err(|_| internal_err("db error"))?;

    drop(conn);

    if let Some(cfg) = email_config {
        let cfg = cfg.clone();
        let name = user.name.clone();
        let email = user.email.clone();
        tokio::spawn(async move {
            if let Err(e) = send_password_reset(&cfg, &email, &name, &raw_token).await {
                tracing::warn!("Failed to send password reset email: {e}");
            }
        });
    } else {
        tracing::warn!("SMTP not configured — reset token for {} (not sent): {}", user.email, raw_token);
    }

    Ok(Json(serde_json::json!({ "message": "Reset link sent. Check your email." })))
}

// ── POST /v1/admin/auth/change-password ───────────────────────────────────────

#[derive(Deserialize)]
pub struct ChangePasswordInput {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<ChangePasswordInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    if input.new_password.len() < 8 {
        return Err(bad_request("New password must be at least 8 characters", "password_too_short"));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    let current_hash = queries::get_user_password_hash(&conn, &auth.user_id)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| bad_request("No password set. Use the setup link sent to your email.", "no_password"))?;

    let valid = verify_password(&input.current_password, &current_hash)
        .map_err(|_| internal_err("password verification error"))?;

    if !valid {
        return Err(unauth("Current password is incorrect"));
    }

    let new_hash = crate::auth::password::hash_password(&input.new_password)
        .map_err(|_| internal_err("password hashing error"))?;

    queries::set_user_password(&conn, &auth.user_id, &new_hash)
        .map_err(|_| internal_err("db error"))?;

    Ok(Json(serde_json::json!({ "message": "Password updated successfully" })))
}
