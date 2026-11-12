use crate::{
    api::{
        helpers::{require_explicit_permission, AppJson},
        workflows::{fail, locked, runner_auth},
    },
    db::workflow_runtime as db,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::Value;
type Response = Result<Json<Value>, (StatusCode, Json<ApiError>)>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    revision: u32,
    config: db::RuntimeConfig,
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
    if b.config.agents.iter().any(|a| a.binding.is_some()) {
        require_explicit_permission(&c, &auth, None, "autonomous_agent:read")?;
    }
    Ok(Json(
        db::save(
            &c,
            &auth.org_id,
            &auth.user_id,
            &workspace,
            b.revision,
            &b.config,
        )
        .map_err(fail)?,
    ))
}
pub async fn agent_profile(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    require_explicit_permission(&c, &auth, None, "factory_policy:read")?;
    require_explicit_permission(&c, &auth, None, "autonomous_agent:read")?;
    Ok(Json(
        db::agent_profile(&c, &auth.org_id, &id).map_err(fail)?,
    ))
}
pub async fn report(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path(workspace): Path<String>,
    AppJson(b): AppJson<db::Report>,
) -> Response {
    let h = store.conn();
    let c = h.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&c, &headers)?;
    Ok(Json(
        db::report(&c, &org, &runner, &workspace, &b).map_err(fail)?,
    ))
}
