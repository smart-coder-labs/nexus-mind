use crate::api::helpers::AppJson;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Deserialize;
use std::sync::Arc;
use tower_cookies::{Cookie, Cookies};

use crate::{
    auth::{api_keys, password::verify_password},
    config::Config,
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

/// Builds the session cookie. `secure` comes from [`Config::cookie_secure`]
/// rather than being hardcoded: a `Secure` cookie is silently dropped by the
/// browser on an insecure origin, which turns a successful login into an
/// immediate bounce back to /login. See the field docs for the security
/// trade-off — it must be true wherever TLS is available.
fn set_session_cookie(cookies: &Cookies, raw_key: String, secure: bool) {
    let mut cookie = Cookie::new("nexusmind_session", raw_key);
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_secure(secure);
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    cookies.add(cookie);
}

// ── POST /v1/admin/auth/login ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginInput {
    pub email: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>,
}

pub async fn login(
    cookies: Cookies,
    State(store): State<SqliteStore>,
    Extension(config): Extension<Arc<Config>>,
    AppJson(input): AppJson<LoginInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    // ── API key path ──────────────────────────────────────────────────────────
    if let Some(ref raw_api_key) = input.api_key {
        let hash = crate::auth::api_keys::hash_key(raw_api_key);
        let auth_ctx = queries::validate_api_key(&conn, &hash)
            .map_err(|_| internal_err("db error"))?
            .ok_or_else(|| unauth("Invalid API key"))?;

        let org = queries::get_org(&conn, &auth_ctx.org_id)
            .map_err(|_| internal_err("db error"))?
            .ok_or_else(|| internal_err("org not found"))?;

        let user = queries::get_user_by_id(&conn, &auth_ctx.user_id)
            .map_err(|_| internal_err("db error"))?
            .ok_or_else(|| internal_err("user not found"))?;

        let raw_key = queries::create_web_session_key(&conn, &auth_ctx.user_id, &auth_ctx.org_id)
            .map_err(|_| internal_err("failed to create session"))?;

        set_session_cookie(&cookies, raw_key, config.cookie_secure);

        return Ok(Json(serde_json::json!({
            "org": org,
            "user": user,
        })));
    }

    // ── Email + password path ─────────────────────────────────────────────────
    let email = input.email.as_deref().unwrap_or("");
    let password = input.password.as_deref().unwrap_or("");

    let (user, password_hash_opt) = queries::find_admin_by_email(&conn, email)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| unauth("Invalid email or password"))?;

    let password_hash = password_hash_opt
        .ok_or_else(|| unauth("Password not set. Check your email for a setup link."))?;

    let valid = verify_password(password, &password_hash)
        .map_err(|_| internal_err("password verification error"))?;

    if !valid {
        return Err(unauth("Invalid email or password"));
    }

    let org = queries::get_org(&conn, &user.org_id)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| internal_err("org not found"))?;

    let raw_key = queries::create_web_session_key(&conn, &user.id, &user.org_id)
        .map_err(|_| internal_err("failed to create session"))?;

    set_session_cookie(&cookies, raw_key, config.cookie_secure);

    Ok(Json(serde_json::json!({
        "org": org,
        "user": user,
    })))
}

// ── GET /v1/admin/auth/me ─────────────────────────────────────────────────────

pub async fn me(
    Extension(auth): Extension<AuthContext>,
    State(store): State<SqliteStore>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    let org = queries::get_org(&conn, &auth.org_id)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| internal_err("org not found"))?;

    let user = queries::get_user_by_id(&conn, &auth.user_id)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| internal_err("user not found"))?;

    let permissions =
        queries::get_role_permissions(&conn, &auth.org_id, auth.role.as_str()).unwrap_or_default();

    let mut user_json = serde_json::to_value(&user).unwrap_or_default();
    if let Some(obj) = user_json.as_object_mut() {
        obj.insert(
            "permissions".to_string(),
            serde_json::to_value(&permissions).unwrap_or_default(),
        );
    }

    Ok(Json(serde_json::json!({
        "org": org,
        "user": user_json,
    })))
}

// ── POST /v1/admin/auth/logout ────────────────────────────────────────────────

pub async fn logout(
    cookies: Cookies,
    State(store): State<SqliteStore>,
    Extension(config): Extension<Arc<Config>>,
) -> StatusCode {
    if let Some(cookie) = cookies.get("nexusmind_session") {
        let token = cookie.value().to_string();
        drop(cookie);
        let hash = api_keys::hash_key(&token);
        let db = store.conn();
        let _ = db
            .lock()
            .map(|conn| queries::revoke_key_by_hash(&conn, &hash));
    }

    // Clear the cookie by setting Max-Age=0
    // Mirror the attributes used when the cookie was set. A removal cookie
    // carrying `Secure` is itself dropped on an insecure origin, which would
    // leave the session cookie in place and make logout a no-op.
    let mut removal = Cookie::new("nexusmind_session", "");
    removal.set_path("/");
    removal.set_max_age(tower_cookies::cookie::time::Duration::ZERO);
    removal.set_http_only(true);
    removal.set_secure(config.cookie_secure);
    removal.set_same_site(tower_cookies::cookie::SameSite::Lax);
    cookies.add(removal);

    StatusCode::NO_CONTENT
}

// ── POST /v1/admin/auth/set-password ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetPasswordInput {
    pub token: String,
    pub password: String,
}

pub async fn set_password(
    State(store): State<SqliteStore>,
    AppJson(input): AppJson<SetPasswordInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    if input.password.len() < 8 {
        return Err(bad_request(
            "Password must be at least 8 characters",
            "password_too_short",
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    let user_id = queries::validate_and_consume_reset_token(&conn, &input.token)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| bad_request("Invalid or expired token", "invalid_token"))?;

    let hashed = crate::auth::password::hash_password(&input.password)
        .map_err(|_| internal_err("password hashing error"))?;

    queries::set_user_password(&conn, &user_id, &hashed).map_err(|_| internal_err("db error"))?;

    Ok(Json(
        serde_json::json!({ "message": "Password set successfully" }),
    ))
}

// ── POST /v1/admin/auth/request-reset ────────────────────────────────────────

#[derive(Deserialize)]
pub struct RequestResetInput {
    pub email: String,
}

pub async fn request_reset(
    State(store): State<SqliteStore>,
    Extension(email_config): Extension<Option<Arc<EmailConfig>>>,
    AppJson(input): AppJson<RequestResetInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    let (user, _) = queries::find_admin_by_email(&conn, &input.email)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| {
            bad_request(
                "No account found with that email address.",
                "email_not_found",
            )
        })?;

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
        tracing::warn!(
            "SMTP not configured — reset token for {} (not sent): {}",
            user.email,
            raw_token
        );
    }

    Ok(Json(
        serde_json::json!({ "message": "Reset link sent. Check your email." }),
    ))
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
    AppJson(input): AppJson<ChangePasswordInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| internal_err("db lock error"))?;

    // Enforce org-level minimum password length (default 8 if not set)
    let org_settings = queries::get_org_settings(&conn, &auth.org_id).unwrap_or_default();
    let min_len = org_settings.min_password_length.unwrap_or(8) as usize;
    if input.new_password.len() < min_len {
        return Err(bad_request(
            &format!("Password must be at least {} characters", min_len),
            "password_too_short",
        ));
    }

    let current_hash = queries::get_user_password_hash(&conn, &auth.user_id)
        .map_err(|_| internal_err("db error"))?
        .ok_or_else(|| {
            bad_request(
                "No password set. Use the setup link sent to your email.",
                "no_password",
            )
        })?;

    let valid = verify_password(&input.current_password, &current_hash)
        .map_err(|_| internal_err("password verification error"))?;

    if !valid {
        return Err(unauth("Current password is incorrect"));
    }

    let new_hash = crate::auth::password::hash_password(&input.new_password)
        .map_err(|_| internal_err("password hashing error"))?;

    queries::set_user_password(&conn, &auth.user_id, &new_hash)
        .map_err(|_| internal_err("db error"))?;

    Ok(Json(
        serde_json::json!({ "message": "Password updated successfully" }),
    ))
}
