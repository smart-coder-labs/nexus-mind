use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Extension, Router,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::api::{admin, audit, health, memory, middleware as auth_mw, sessions, users};
use crate::config::Config;

pub fn build(conn: Connection, config: Config) -> Router {
    let db = Arc::new(Mutex::new(conn));

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
        .layer(middleware::from_fn_with_state(db.clone(), auth_mw::auth));

    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/orgs", get(admin::list_orgs).post(admin::create_org))
        .merge(protected)
        .layer(Extension(config.superuser_key))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(db)
}
