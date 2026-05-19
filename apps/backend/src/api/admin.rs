use axum::{extract::State, http::StatusCode, Json};
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

use crate::{db::queries, models::types::ApiError};

#[derive(Deserialize)]
pub struct BootstrapInput {
    pub org_name: String,
    pub org_slug: String,
    pub admin_email: String,
    pub admin_name: String,
}

pub async fn bootstrap(
    State(db): State<Arc<Mutex<Connection>>>,
    Json(input): Json<BootstrapInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiError>)> {
    let conn = db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        )
    })?;

    match queries::bootstrap(
        &conn,
        &input.org_name,
        &input.org_slug,
        &input.admin_email,
        &input.admin_name,
    ) {
        Ok((org, user, api_key)) => {
            let body = serde_json::json!({
                "org": org,
                "user": user,
                "api_key": api_key,
            });
            Ok((StatusCode::CREATED, Json(body)))
        }
        Err(e) if e.to_string().contains("already_bootstrapped") => Err((
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "Organization already exists".to_string(),
                code: "already_bootstrapped".to_string(),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "internal_error".to_string(),
            }),
        )),
    }
}
