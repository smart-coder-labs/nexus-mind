use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use crate::{
    db::queries,
    models::types::{
        Agent, AgentAssignment, ApiError, AuthContext, CreateAgentRequest, UpdateAgentRequest,
    },
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
            error: "Agent not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

pub async fn list_agents(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<Agent>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db
        .lock()
        .map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let agents = queries::list_agents(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(agents))
}

pub async fn get_agent(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Agent>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db
        .lock()
        .map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let agent = queries::get_agent(&conn, &auth.org_id, &id)
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(Json(agent))
}

pub async fn create_agent(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<Agent>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_privileged() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db
        .lock()
        .map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let agent = queries::create_agent(&conn, &auth.org_id, &req).map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(agent)))
}

pub async fn update_agent(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<Agent>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_privileged() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db
        .lock()
        .map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let agent = queries::update_agent(&conn, &auth.org_id, &id, &req)
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(Json(agent))
}

pub async fn list_agent_assignments(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<AgentAssignment>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db
        .lock()
        .map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    // Verify agent exists and belongs to this org
    queries::get_agent(&conn, &auth.org_id, &id)
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    let assignments = queries::list_agent_assignments(&conn, &auth.org_id, &id).map_err(db_err)?;
    Ok(Json(assignments))
}
