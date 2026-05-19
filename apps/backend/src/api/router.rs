use axum::{middleware, routing::{get, post}, Router};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::api::{admin, health, middleware as auth_mw};
use crate::config::Config;

pub fn build(conn: Connection, _config: Config) -> Router {
    let db = Arc::new(Mutex::new(conn));

    let protected = Router::new()
        // memory, users, admin, audit routes go here in Days 4-5
        .layer(middleware::from_fn_with_state(db.clone(), auth_mw::auth));

    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/bootstrap", post(admin::bootstrap))
        .merge(protected)
        .with_state(db)
}
