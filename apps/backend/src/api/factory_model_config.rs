//! Factory provider/model selection control plane.
//! Credential values are owned by provider-specific secret stores and are never
//! accepted or returned by these endpoints.
use axum::{extract::State, http::StatusCode, Extension, Json};

use crate::{
    api::helpers::{require_explicit_permission, AppJson},
    db::factory_model_config,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: message.into(), code: code.into() }))
}

fn internal(error_value: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", error_value.to_string())
}

fn lock_error() -> (StatusCode, Json<ApiError>) {
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock error")
}

pub async fn get(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<factory_model_config::StoredModelConfig>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    Ok(Json(factory_model_config::get(&conn, &auth.org_id).map_err(internal)?))
}

pub async fn put(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<factory_model_config::ModelConfigWrite>,
) -> ApiResult<Json<factory_model_config::StoredModelConfig>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    if let Err(reason) = factory_model_config::validate(&input.roles) {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_model_config", reason));
    }
    match factory_model_config::put(&conn, &auth.org_id, &auth.user_id, &input.roles) {
        Ok(config) => Ok(Json(config)),
        Err(error_value) => Err(internal(error_value)),
    }
}
