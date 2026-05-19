use axum::Router;
use rusqlite::Connection;
use crate::config::Config;

pub fn build(_conn: Connection, _config: Config) -> Router {
    Router::new()
}
