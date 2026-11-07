//! Admin (write-only) and runner (lease-gated) access to per-repository secrets.
use crate::{
    api::{
        helpers::{require_explicit_permission, AppJson},
        workflows::{fail, locked, runner_auth},
    },
    db::workspace_secrets as db,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::Value;

type Response = Result<Json<Value>, (StatusCode, Json<ApiError>)>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    repository: String,
    kind: String,
    content: String,
}

#[derive(Deserialize)]
pub struct Target {
    repository: String,
    kind: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    lease_token: String,
}

pub async fn list(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>, Path(workspace): Path<String>) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:read")?;
    Ok(Json(db::list(&c, &auth.org_id, &workspace).map_err(fail)?))
}

pub async fn save(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(workspace): Path<String>,
    AppJson(b): AppJson<Save>,
) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
    Ok(Json(db::save(&c, &auth.org_id, &auth.user_id, &workspace, &b.repository, &b.kind, &b.content).map_err(fail)?))
}

pub async fn delete(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(workspace): Path<String>,
    Query(t): Query<Target>,
) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
    Ok(Json(db::delete(&c, &auth.org_id, &workspace, &t.repository, &t.kind).map_err(fail)?))
}

pub async fn runner(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(b): AppJson<Lease>,
) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&c, &headers)?;
    Ok(Json(
        db::for_runner(&c, &org, &runner, &run, &step, &b.lease_token, chrono::Utc::now().timestamp()).map_err(fail)?,
    ))
}
