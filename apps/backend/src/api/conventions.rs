use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::AppJson,
    db::queries,
    models::types::{ApiError, AuthContext, Convention, CreateConventionRequest, UpdateConventionRequest},
    store::sqlite::SqliteStore,
};

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn forbidden() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "Admin role required".to_string(),
            code: "forbidden".to_string(),
        }),
    )
}

fn not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Convention not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

#[derive(Deserialize)]
pub struct ListParams {
    pub category: Option<String>,
    pub include_archived: Option<bool>,
}

pub async fn list_conventions(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Convention>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let conventions = queries::list_conventions(
        &conn,
        &auth.org_id,
        params.category.as_deref(),
        params.include_archived,
    ).map_err(db_err)?;
    Ok(Json(conventions))
}

pub async fn get_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<Json<Convention>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let convention = queries::get_convention(&conn, &auth.org_id, id)
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(Json(convention))
}

pub async fn create_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(req): AppJson<CreateConventionRequest>,
) -> Result<(StatusCode, Json<Convention>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let convention = queries::create_convention(&conn, &auth.org_id, &req).map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(convention)))
}

pub async fn update_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
    AppJson(req): AppJson<UpdateConventionRequest>,
) -> Result<Json<Convention>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let convention = queries::update_convention(&conn, &auth.org_id, id, &req)
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(Json(convention))
}

pub async fn delete_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let deleted = queries::delete_convention(&conn, &auth.org_id, id).map_err(db_err)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

pub async fn archive_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let ok = queries::archive_convention(&conn, &auth.org_id, id).map_err(db_err)?;
    if ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

pub async fn restore_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let ok = queries::restore_convention(&conn, &auth.org_id, id).map_err(db_err)?;
    if ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}
