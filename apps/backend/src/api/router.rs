use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Extension, Router,
};
use rusqlite::Connection;
use std::sync::Arc;
use tower_cookies::CookieManagerLayer;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::api::{admin, audit, auth, code, context, health, internal, memory, middleware as auth_mw, policy, rate_limit, sessions, users};
use crate::config::Config;
use crate::email::EmailConfig;
use crate::embed::EmbedService;
use crate::store::sqlite::SqliteStore;

pub fn build(conn: Connection, config: Config) -> Router {
    let embed = if std::env::var("NEXUSMIND_EMBED_ENABLED").as_deref() == Ok("true") {
        match EmbedService::init() {
            Ok(svc) => {
                tracing::info!("Embedding service initialized (nomic-embed-text-v1.5)");
                Some(svc)
            }
            Err(e) => {
                tracing::warn!("Embedding service unavailable — semantic search disabled: {e}");
                None
            }
        }
    } else {
        tracing::info!("Embedding service disabled (set NEXUSMIND_EMBED_ENABLED=true to enable)");
        None
    };

    let store = match embed {
        Some(svc) => SqliteStore::new(conn).with_embed(svc),
        None      => SqliteStore::new(conn),
    };

    let email_config: Option<Arc<EmailConfig>> = match (
        config.smtp_username.clone(),
        config.smtp_password.clone(),
        config.smtp_from.clone(),
    ) {
        (Some(username), Some(password), Some(from)) => Some(Arc::new(EmailConfig {
            smtp_host: config.smtp_host.clone(),
            smtp_port: config.smtp_port,
            smtp_username: username,
            smtp_password: password,
            smtp_from: from,
            app_base_url: config.app_base_url.clone(),
        })),
        _ => {
            tracing::warn!("SMTP not configured (SMTP_USERNAME, SMTP_PASSWORD, SMTP_FROM required). Emails will not be sent.");
            None
        }
    };

    let rate_state = rate_limit::RateLimitState::new(store.conn());

    let protected = Router::new()
        .route("/v1/memory/store", post(memory::store))
        .route("/v1/memory/search", post(memory::search))
        .route("/v1/memory/export", get(memory::export))
        .route("/v1/memory/:id", get(memory::get_by_id).delete(memory::delete))
        .route("/v1/memory", get(memory::list))
        .route("/v1/sessions", post(sessions::create_session_handler))
        .route("/v1/sessions/:id", patch(sessions::patch_session_handler))
        .route("/v1/users", get(users::list))
        .route("/v1/users/invite", post(users::invite))
        .route("/v1/users/:id", delete(users::remove))
        .route("/v1/users/:id/rotate-key", post(users::rotate_key))
        .route("/v1/users/:id/role", patch(users::update_role))
        .route("/v1/roles", get(admin::list_roles_api).post(admin::create_role_api))
        .route("/v1/roles/:id", delete(admin::delete_role_api))
        .route("/v1/projects", get(admin::list_projects_api).post(admin::create_project_api))
        .route("/v1/projects/:id", delete(admin::delete_project_api).patch(admin::update_project_api))
        .route("/v1/projects/:project_id/members", get(admin::list_project_members_api).post(admin::upsert_project_member_api))
        .route("/v1/projects/:project_id/members/:user_id", delete(admin::delete_project_member_api))
        .route("/v1/policies", get(policy::list_policies).post(policy::create_policy))
        .route("/v1/policies/:id", patch(policy::update_policy).delete(policy::delete_policy))
        .route("/v1/policy/check", post(policy::check_policy))
        .route("/v1/context/project/:project", get(context::get_project_context))
        .route("/v1/code/index", post(code::post_index))
        .route("/v1/code/search", post(code::post_search))
        .route("/v1/code/status/:project", get(code::get_status))
        .route("/v1/code/context", get(code::get_context))
        .route("/v1/audit", get(audit::query))
        .route("/v1/audit/export", get(audit::export))
        .route("/v1/audit/log", post(audit::post_audit))
        .route("/v1/admin/stats", get(admin::stats))
        .route("/v1/admin/org", get(admin::get_org).patch(admin::update_org))
        .route("/v1/admin/org/settings", get(admin::get_org_settings_api).patch(admin::update_org_settings_api))
        .route("/v1/admin/auth/change-password", post(auth::change_password))
        .route("/v1/admin/auth/me", get(auth::me))
        // Rate limit runs after auth (inner layer = runs second at runtime).
        // Auth is outermost (last `.layer()`) so it runs first.
        .layer(middleware::from_fn_with_state(rate_state, rate_limit::rate_limit))
        .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

    let cors_origins = config.cors_origins.clone();
    let admin_origin = config.admin_origin.clone();

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            move |origin: &axum::http::HeaderValue, _parts: &axum::http::request::Parts| {
                if cors_origins == "*" {
                    return true;
                }
                let origin_str = match origin.to_str() {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                if origin_str == admin_origin {
                    return true;
                }
                cors_origins.split(',').any(|allowed| allowed.trim() == origin_str)
            },
        ))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
        .allow_credentials(true);

    let internal_routes = Router::new()
        .route("/internal/metrics", get(internal::get_metrics))
        .route("/internal/orgs", get(internal::list_orgs).post(internal::create_org))
        .route("/internal/orgs/:id", get(internal::get_org).patch(internal::update_org).delete(internal::delete_org))
        .route("/internal/orgs/:id/users", get(internal::list_org_users))
        .route("/internal/orgs/:id/impersonate", post(internal::impersonate_org))
        .route("/internal/users", get(internal::list_users))
        .route("/internal/users/:id/suspend", post(internal::suspend_user))
        .route("/internal/audit", get(internal::list_audit));

    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/orgs", get(admin::list_orgs).post(admin::create_org))
        .route("/v1/orgs/:id/users", get(admin::list_org_users))
        .route("/v1/admin/auth/login", post(auth::login))
        .route("/v1/admin/auth/set-password", post(auth::set_password))
        .route("/v1/admin/auth/request-reset", post(auth::request_reset))
        .route("/v1/admin/auth/logout", post(auth::logout))
        .merge(protected)
        .merge(internal_routes)
        .layer(Extension(email_config))
        .layer(Extension(config.superuser_key))
        .layer(cors)
        .layer(CookieManagerLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(store)
}
