use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::api::{admin, health, memory, middleware as auth_mw, users};
use crate::config::Config;

pub fn build(conn: Connection, _config: Config) -> Router {
    let db = Arc::new(Mutex::new(conn));

    let protected = Router::new()
        .route("/v1/memory/store", post(memory::store))
        .route("/v1/memory/search", post(memory::search))
        .route("/v1/memory/:id", delete(memory::delete))
        .route("/v1/memory", get(memory::list))
        .route("/v1/users", get(users::list))
        .route("/v1/users/invite", post(users::invite))
        .route("/v1/users/:id", delete(users::remove))
        .route("/v1/users/:id/rotate-key", post(users::rotate_key))
        .route("/v1/admin/stats", get(admin::stats))
        .route("/v1/admin/org", get(admin::get_org).patch(admin::update_org))
        .layer(middleware::from_fn_with_state(db.clone(), auth_mw::auth));

    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/bootstrap", post(admin::bootstrap))
        .merge(protected)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(db)
}
