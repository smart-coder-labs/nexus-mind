//! Workspace/workflow control plane and separately authenticated runner protocol.
use crate::{
    api::helpers::{require_explicit_permission, AppJson},
    db::workflows as db,
    factory::workflows::{Definition, Evidence, Repository},
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

type Failure = (StatusCode, Json<ApiError>);
type Response = Result<Json<Value>, Failure>;
pub(super) fn fail(e: anyhow::Error) -> Failure {
    let message = e.to_string();
    let status = match message.as_str() {
        "not_found" => StatusCode::NOT_FOUND,
        "runner_unauthorized" => StatusCode::UNAUTHORIZED,
        "runner_version_unsupported" => StatusCode::UPGRADE_REQUIRED,
        "lease_conflict"
        | "inventory_conflict"
        | "run_conflict"
        | "retry_limit_or_step_conflict"
        | "factory_paused"
        | "workflow_agent_unavailable"
        | "workflow_agent_template_incompatible"
        | "workflow_agent_instructions_unsafe"
        | "workflow_agent_model_missing"
        | "runtime_revision_conflict"
        | "runtime_configuration_incomplete" => StatusCode::CONFLICT,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    // Do not disclose SQL statements or database internals.
    let public = if e.downcast_ref::<rusqlite::Error>().is_some() {
        "storage_error"
    } else {
        &message
    };
    (
        if public == "storage_error" {
            StatusCode::INTERNAL_SERVER_ERROR
        } else {
            status
        },
        Json(ApiError {
            code: public.into(),
            error: match public {
                "workflow_agent_instructions_unsafe"=>"Se detectó un posible secreto literal o una ruta personal en las instrucciones. Usa referencias o placeholders; las menciones a Bearer, tokens o cabeceras sin credenciales están permitidas.",
                "workflow_agent_instructions_too_large"=>"Las instrucciones superan 64 KiB. Reduce o divide el contenido; este error es de tamaño, no de credenciales.",
                "workflow_agent_unavailable"=>"El perfil de agente no existe en esta organización o ya no está habilitado y validado. Selecciónalo de nuevo.",
                "workflow_agent_template_incompatible"=>"La plantilla del agente no es compatible con una fase de código. Usa un resolver/reviewer o ejecuta su plantilla completa como agente individual.",
                "workflow_agent_model_missing"=>"El agente no tiene un modelo explícito. Desactiva la herencia y elige un modelo compatible con el harness.",
                "factory_paused"=>"Factory está pausada para esta organización. Reactívala desde Configuración.",
                "runtime_revision_conflict"=>"Otra persona guardó una revisión nueva. Recarga antes de guardar.",
                "runtime_configuration_incomplete"=>"Completa los perfiles y espera a que el runner aplique la configuración antes de iniciar el workflow completo.",
                "invalid_runtime_agent"=>"Completa imagen por digest, modelo, conexión de proxy y red interna de cada agente. No introduzcas credenciales.",
                "runtime_check_image_invalid"=>"Elige el entorno Node.js o una imagen inmutable válida. Cambiar el entorno por defecto también debe aplicarse a las comprobaciones existentes.",
                "runtime_check_repository_invalid"=>"Selecciona un repositorio del inventario autorizado en cada comprobación.",
                "runtime_check_command_invalid"=>"Revisa el comando de la comprobación: debe ser un argv válido, no vacío y sin credenciales.",
                "runtime_check_duplicate"=>"Hay dos comprobaciones con el mismo tipo, repositorio y comando. Elimina la duplicada o usa un comando diferente.",
                "invalid_runtime_check"=>"Selecciona un tipo de comprobación válido y revisa las referencias de entorno.",
                "invalid_runtime_service"=>"Cada servicio necesita nombre único, repositorio, imagen por digest, comando de inicio y healthcheck, sin credenciales incrustadas.",
                _=>public,
            }.into(),
        }),
    )
}
pub async fn publication_identity(State(store):State<SqliteStore>,Extension(auth):Extension<AuthContext>)->Response {
    {let h=store.conn();let c=h.lock().map_err(|_|locked())?;require_explicit_permission(&c,&auth,None,"factory_policy:read")?;}
    let enabled=crate::factory::workflow_publication::backend_account_enabled(&auth.org_id);
    if !enabled {return Ok(Json(json!({"enabled":false,"login":null,"error":"backend_github_publication_not_authorized"})))}
    match crate::automation::worker::server_github_identity().await {
        Ok(login)=>Ok(Json(json!({"enabled":true,"login":login,"error":null}))),
        Err(_)=>Ok(Json(json!({"enabled":true,"login":null,"error":"backend_github_authentication_required"}))),
    }
}
pub(super) fn locked() -> Failure {
    fail(anyhow::anyhow!("storage_unavailable"))
}
pub(super) fn runner_auth(
    conn: &rusqlite::Connection,
    headers: &HeaderMap,
) -> Result<(String, String), Failure> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(|| fail(anyhow::anyhow!("runner_unauthorized")))?;
    db::authenticate_runner(conn, token).map_err(fail)
}

pub async fn catalog(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    db::expire_leases(&conn, &auth.org_id, chrono::Utc::now().timestamp()).map_err(fail)?;
    Ok(Json(
        json!({"runners":db::list_runners(&conn,&auth.org_id).map_err(fail)?,"workspaces":db::list_workspaces(&conn,&auth.org_id,None).map_err(fail)?,"definitions":db::list_definitions(&conn,&auth.org_id).map_err(fail)?,"runs":db::list_runs(&conn,&auth.org_id).map_err(fail)?,"intakes":crate::automation::workflow_intake::list(&conn,&auth.org_id).map_err(fail)?,"pending_runs":db::list_pending_runs(&conn,&auth.org_id).map_err(fail)?,"findings":db::list_findings(&conn,&auth.org_id).map_err(fail)?,"factory_enabled":db::factory_enabled(&conn,&auth.org_id).map_err(fail)?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewRunner {
    name: String,
}
pub async fn register_runner(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<NewRunner>,
) -> Result<([(axum::http::HeaderName, &'static str); 1], Json<Value>), Failure> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(db::register_runner(&conn, &auth.org_id, &body.name).map_err(fail)?),
    ))
}
pub async fn revoke_runner(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    db::revoke_runner(&conn, &auth.org_id, &id).map_err(fail)?;
    Ok(Json(json!({"revoked":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewWorkspace {
    runner_id: String,
    name: String,
    root_path: String,
}
pub async fn create_workspace(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(b): AppJson<NewWorkspace>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    Ok(Json(
        db::create_workspace(&conn, &auth.org_id, &b.runner_id, &b.name, &b.root_path)
            .map_err(fail)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    revision: i64,
    repositories: Vec<Repository>,
}
pub async fn confirm_workspace(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(b): AppJson<Inventory>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    db::confirm_inventory(&conn, &auth.org_id, &id, b.revision, &b.repositories).map_err(fail)?;
    Ok(Json(
        db::workspace_by_id(&conn, &auth.org_id, &id).map_err(fail)?,
    ))
}
pub async fn create_definition(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(d): AppJson<Definition>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    if d.steps.iter().any(|s| s.options.get("agent_id").and_then(Value::as_str).is_some_and(|id| !id.is_empty())) {
        require_explicit_permission(&conn, &auth, None, "autonomous_agent:read")?;
    }
    Ok(Json(
        db::create_definition(&conn, &auth.org_id, &auth.user_id, &d).map_err(fail)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewRun {
    definition_id: String,
    source_key: String,
    task: Value,
    /// Explicit offline/exact-inventory execution. Ordinary runs always refresh.
    #[serde(default)]
    pinned_sources: bool,
}
pub async fn start_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(b): AppJson<NewRun>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    let start = if b.pinned_sources { db::start_pinned_run } else { db::start_run };
    let run = start(
        &conn,
        &auth.org_id,
        &b.definition_id,
        &b.source_key,
        &b.task,
    )
    .map_err(fail)?;
    let id = run["id"].as_str().unwrap_or_default();
    let policy =
        crate::factory::workflow_policy::decision(&conn, &auth.org_id, id, "fix", &[], false)
            .map_err(fail)?;
    if policy["never"] != true {
        crate::db::workflow_execution::approve(&conn, &auth.org_id, &auth.user_id, id, "fix", 0)
            .map_err(fail)?;
    }
    Ok(Json(run))
}
pub async fn get_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    db::expire_leases(&conn, &auth.org_id, chrono::Utc::now().timestamp()).map_err(fail)?;
    Ok(Json(
        db::run_detail(&conn, &auth.org_id, &id).map_err(fail)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    action: String,
    step_id: Option<String>,
}
pub async fn control_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(b): AppJson<Control>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    db::control_run(&conn, &auth.org_id, &id, &b.action, b.step_id.as_deref()).map_err(fail)?;
    Ok(Json(
        db::run_detail(&conn, &auth.org_id, &id).map_err(fail)?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearNext {
    connector_id: String,
    team_id: Option<String>,
    project_id: Option<String>,
}

/// Explicit intake action, not a background poller. Uses the identity associated
/// with the registered Linear API key and reserves the next unclaimed issue.
pub async fn linear_next(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(definition_id): Path<String>,
    AppJson(b): AppJson<LinearNext>,
) -> Response {
    let (token, workspace_id) = {
        let handle = store.conn();
        let conn = handle.lock().map_err(|_| locked())?;
        require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
        let definition = db::list_definitions(&conn, &auth.org_id)
            .map_err(fail)?
            .into_iter()
            .find(|d| d["id"] == definition_id)
            .ok_or_else(|| fail(anyhow::anyhow!("not_found")))?;
        let workspace = definition["definition"]["workspace_id"]
            .as_str()
            .ok_or_else(|| fail(anyhow::anyhow!("not_found")))?
            .to_string();
        let (connector, secret) = crate::db::queries::get_autonomous_agent_connector_secret(
            &conn,
            &auth.org_id,
            &b.connector_id,
        )
        .map_err(fail)?
        .ok_or_else(|| fail(anyhow::anyhow!("connector_unavailable")))?;
        if connector.kind != "target_secret"
            || connector.metadata["purpose"] != "factory_workflow_linear"
            || !connector.scopes.iter().any(|s| s == "linear:read")
        {
            return Err(fail(anyhow::anyhow!("connector_not_for_linear")));
        }
        (secret, workspace)
    };
    let issues = crate::factory::linear::assigned_issues(
        &token,
        b.team_id.as_deref(),
        b.project_id.as_deref(),
    )
    .await
    .map_err(fail)?;
    let handle = store.conn();
    let conn = handle.lock().map_err(|_| locked())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    // Revocation or rotation during the upstream call must take effect.
    let current = crate::db::queries::get_autonomous_agent_connector_secret(
        &conn,
        &auth.org_id,
        &b.connector_id,
    )
    .map_err(fail)?;
    if !current.is_some_and(|(_, secret)| secret == token) {
        return Err(fail(anyhow::anyhow!("connector_changed")));
    }
    for issue in issues {
        let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM factory_workflow_runs WHERE org_id=?1 AND workspace_id=?2 AND source_key=?3)",rusqlite::params![auth.org_id,workspace_id,issue.source_key()],|r|r.get(0)).map_err(|e|fail(e.into()))?;
        if !exists {
            let run = db::start_run(
                &conn,
                &auth.org_id,
                &definition_id,
                &issue.source_key(),
                &issue.task(),
            )
            .map_err(fail)?;
            let id = run["id"].as_str().unwrap_or_default();
            if crate::factory::workflow_policy::decision(&conn, &auth.org_id, id, "fix", &[], false)
                .map_err(fail)?["never"]
                != true
            {
                crate::db::workflow_execution::approve(
                    &conn,
                    &auth.org_id,
                    &auth.user_id,
                    id,
                    "fix",
                    0,
                )
                .map_err(fail)?;
            }
            return Ok(Json(json!({"run":run})));
        }
    }
    Ok(Json(
        json!({"run":null,"reason":"no_unclaimed_assigned_issue"}),
    ))
}

// These routes deliberately do NOT use human auth middleware. The narrowly scoped
// runner bearer credential cannot edit workflows, policies, approvals or secrets.
pub async fn runner_workspaces(State(store): State<SqliteStore>, headers: HeaderMap) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&conn, &headers)?;
    Ok(Json(
        json!({"workspaces":db::list_workspaces(&conn,&org,Some(&runner)).map_err(fail)?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discovered {
    repositories: Vec<Repository>,
}
pub async fn report_inventory(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path(id): Path<String>,
    AppJson(b): AppJson<Discovered>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&conn, &headers)?;
    db::report_inventory(&conn, &org, &runner, &id, &b.repositories).map_err(fail)?;
    Ok(Json(json!({"reported":true})))
}
/// Oldest runner protocol this backend still serves. Runners announce theirs as a
/// `runner-contract:<n>` capability; runners from before the announcement are
/// contract 1 and send none. Refusing at claim keeps an incompatible runner from
/// failing half way through a leased step.
pub(crate) const RUNNER_CONTRACT_MIN: u32 = 1;

pub(crate) fn runner_contract_supported(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .filter_map(|c| c.strip_prefix("runner-contract:"))
        .all(|v| v.parse::<u32>().is_ok_and(|n| n >= RUNNER_CONTRACT_MIN))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    capabilities: Vec<String>,
}
pub async fn claim(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    AppJson(b): AppJson<Claim>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&conn, &headers)?;
    if !runner_contract_supported(&b.capabilities) {
        return Err(fail(anyhow::anyhow!("runner_version_unsupported")));
    }
    Ok(Json(
        json!({"job":db::claim(&conn,&org,&runner,&b.capabilities,chrono::Utc::now().timestamp()).map_err(fail)?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    lease_token: String,
    success: bool,
    evidence: Vec<Evidence>,
    outcome: crate::factory::workflows::Outcome,
}
pub async fn complete(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    Path((run, step)): Path<(String, String)>,
    AppJson(b): AppJson<Completion>,
) -> Response {
    let db = store.conn();
    let conn = db.lock().map_err(|_| locked())?;
    let (runner, org) = runner_auth(&conn, &headers)?;
    db::complete_with_outcome(
        &conn,
        &org,
        &runner,
        &run,
        &step,
        &b.lease_token,
        b.success,
        &b.evidence,
        Some(&b.outcome),
        chrono::Utc::now().timestamp(),
    )
    .map_err(fail)?;
    Ok(Json(
        json!({"recorded":true,"status":db::run_detail(&conn,&org,&run).map_err(fail)?["status"]}),
    ))
}
