use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Extension, Router,
};
use rusqlite::Connection;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::api::{admin, audit, auth, health, memory, middleware as auth_mw, sessions, users};
use crate::config::Config;
use crate::email::EmailConfig;
use crate::embed::EmbedService;
use crate::store::sqlite::SqliteStore;

pub fn build(conn: Connection, config: Config) -> Router {
    let embed = match EmbedService::init() {
        Ok(svc) => {
            tracing::info!("Embedding service initialized (nomic-embed-text-v1.5)");
            Some(svc)
        }
        Err(e) => {
            tracing::warn!("Embedding service unavailable — semantic search disabled: {e}");
            None
        }
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

    let protected = Router::new()
        .route("/v1/memory/store", post(memory::store))
        .route("/v1/memory/search", post(memory::search))
        .route("/v1/memory/:id", delete(memory::delete))
        .route("/v1/memory", get(memory::list))
        .route("/v1/sessions", post(sessions::create_session_handler))
        .route("/v1/sessions/:id", patch(sessions::patch_session_handler))
        .route("/v1/users", get(users::list))
        .route("/v1/users/invite", post(users::invite))
        .route("/v1/users/:id", delete(users::remove))
        .route("/v1/users/:id/rotate-key", post(users::rotate_key))
        .route("/v1/audit", get(audit::query))
        .route("/v1/admin/stats", get(admin::stats))
        .route("/v1/admin/org", get(admin::get_org).patch(admin::update_org))
        .route("/v1/admin/auth/change-password", post(auth::change_password))
        .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/orgs", get(admin::list_orgs).post(admin::create_org))
        .route("/v1/admin/auth/login", post(auth::login))
        .route("/v1/admin/auth/set-password", post(auth::set_password))
        .route("/v1/admin/auth/request-reset", post(auth::request_reset))
        .merge(protected)
        .layer(Extension(email_config))
        .layer(Extension(config.superuser_key))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(store)
}
