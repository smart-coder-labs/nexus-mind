use crate::{
    api::helpers::require_permission,
    context_fabric::{
        active_manifest, compile, generate_deterministic, publish_generation, rollback_generation,
        validate_generation_identity,
        verify_request_generation, AssembleRequest, AssembleResponse, ContextFabricManifest,
        GenerateRequest, GenerationRef, VerifyRequest, VerifyResponse, ClaimStatus,
        ClaimVerification, GenerationMetadata, ShadowRequest, ShadowResponse,
    },
    db::migrations,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::Connection;
use std::{collections::{HashMap, HashSet}, hash::{Hash, Hasher}};
use crate::context_fabric_runtime::{CacheIdentity, CacheStage, RolloutState, CANDIDATE_LANE, BASELINE_LANE};

const VERIFIED_MEMORY_PROVENANCE: &str = "memory-search";

fn verification_error(code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ApiError {
            error: "Evidence could not be verified".into(),
            code: code.into(),
        }),
    )
}

fn fabric_error(error: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let message = error.to_string();
    let code = if message.contains("not found") || message.contains("QueryReturnedNoRows") {
        "not_found"
    } else if message.contains("pending") {
        "migration_pending"
    } else if message.contains("UNIQUE") {
        "conflict"
    } else {
        "validation_error"
    };
    // Never return database/provider text here: it can contain SQL, locators, or
    // other tenant-owned details. The machine-readable code is the contract.
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ApiError {
            error: "Context Fabric request was rejected".into(),
            code: code.into(),
        }),
    )
}

#[derive(Debug, serde::Deserialize)]
pub struct PublishRequest {
    pub manifest: ContextFabricManifest,
}

pub async fn apply_migration(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "settings:write")?;
    migrations::apply_context_fabric(&conn).map_err(fabric_error)?;
    migrations::verify_context_fabric(&conn).map_err(fabric_error)?;
    Ok(Json(
        serde_json::json!({"schema_version": 58, "status": "applied"}),
    ))
}

pub async fn migration_status(
    State(store): State<SqliteStore>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    let pending = migrations::context_fabric_pending(&conn).map_err(fabric_error)?;
    Ok(Json(
        serde_json::json!({"schema_version": 58, "pending": pending}),
    ))
}

pub async fn publish(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(profile_id): Path<String>,
    Json(input): Json<PublishRequest>,
) -> Result<Json<ContextFabricManifest>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "settings:write")?;
    if input.manifest.profile_id != profile_id {
        return Err(verification_error("profile_path_mismatch"));
    }
    let result = publish_generation(&conn, &auth.org_id, &auth.user_id, &input.manifest)
        .map_err(fabric_error)?;
    drop(conn);
    store.context_runtime().invalidate_all(&auth.org_id, "profile_published");
    Ok(Json(result))
}

pub async fn active(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Option<ContextFabricManifest>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "memory:read")?;
    active_manifest(&conn, &auth.org_id)
        .map(Json)
        .map_err(fabric_error)
}

pub async fn rollback(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((generation_id, generation_version)): Path<(String, u64)>,
) -> Result<Json<ContextFabricManifest>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "settings:write")?;
    let result = rollback_generation(
        &conn,
        &auth.org_id,
        &GenerationRef {
            id: generation_id,
            version: generation_version,
        },
    )
    .map_err(fabric_error)?;
    drop(conn);
    store.context_runtime().invalidate_all(&auth.org_id, "generation_rollback");
    store.context_runtime().set_rollout(RolloutState::default());
    Ok(Json(result))
}

#[derive(Debug, serde::Deserialize)]
pub struct RolloutRequest {
    pub profile_id: String,
    pub generation: GenerationRef,
    pub manifest_evidence: String,
    pub run_evidence: String,
    pub approval_operator: String,
    #[serde(default)] pub fallback_baseline: bool,
    #[serde(default)] pub shadow_enabled: bool,
    #[serde(default)] pub canary_enabled: bool,
}

fn generation_stamp(conn: &Connection, sql: &str, org_id: &str) -> u64 {
    let value: String = conn.query_row(sql, [org_id], |row| row.get(0)).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn project_scope(conn: &Connection, auth: &AuthContext, request: &AssembleRequest) -> String {
    let mut projects = request.candidates.iter().filter_map(|candidate| {
        conn.query_row(
            "SELECT COALESCE(project, '') FROM memories WHERE org_id = ?1 AND id = ?2",
            rusqlite::params![&auth.org_id, &candidate.locator.id],
            |row| row.get::<_, String>(0),
        ).ok()
    }).collect::<Vec<_>>();
    projects.sort();
    projects.dedup();
    if projects.is_empty() { "org".into() } else { projects.join(",") }
}

fn cache_identity(
    conn: &Connection,
    auth: &AuthContext,
    request: &AssembleRequest,
    stage: CacheStage,
    lane: &str,
    profile: &str,
) -> CacheIdentity {
    let mut ids = request.candidates.iter().map(|candidate| candidate.locator.id.clone()).collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let mut source_type = ids.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>().join(",");
    source_type.push(',');
    source_type.push_str(&request_fingerprint(request));
    CacheIdentity {
        tenant: auth.org_id.clone(), caller_scope: auth.role.to_string(), caller_user: auth.user_id.clone(),
        project: project_scope(conn, auth, request),
        acl_generation: generation_stamp(conn, "SELECT count(*) || ':' || COALESCE(group_concat(pm.role), '') FROM project_members pm JOIN projects p ON p.id = pm.project_id WHERE p.org_id = ?1", &auth.org_id),
        policy_generation: generation_stamp(conn, "SELECT count(*) || ':' || COALESCE(max(updated_at), '') FROM policies WHERE org_id = ?1", &auth.org_id),
        profile: profile.into(),
        captured_generation: request.generation.clone(), freshness: request.freshness_window_secs.map(|v| format!("bounded:{v}")).unwrap_or_else(|| "explicit".into()),
        source_type, contract_version: request.contract_version.clone(), lane: lane.into(),
        budget: Some(request.token_budget), tokenizer: Some(request.tokenizer.clone()), stage,
    }
}

fn request_fingerprint<T: serde::Serialize>(value: &T) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(value).unwrap_or_default().hash(&mut hasher);
    format!("request:{:016x}", hasher.finish())
}

fn validate_rollout(input: &RolloutRequest) -> Result<(), &'static str> {
    let profile = input.profile_id.to_ascii_lowercase();
    if profile.contains("bq") || profile.contains("mrl") || profile.contains("tool-search") {
        return Err("unsupported_rollout_capability");
    }
    if input.profile_id.trim().is_empty() || input.generation.id.trim().is_empty() { return Err("missing_rollout_identity"); }
    if input.manifest_evidence.trim().is_empty() { return Err("missing_manifest_evidence"); }
    if input.run_evidence.trim().is_empty() { return Err("missing_run_evidence"); }
    if input.approval_operator.trim().is_empty() { return Err("missing_approval_operator"); }
    if !input.manifest_evidence.starts_with("manifest:") { return Err("invalid_manifest_evidence"); }
    if !input.run_evidence.starts_with("run:") { return Err("invalid_run_evidence"); }
    Ok(())
}

fn rollout_error(code: &'static str) -> (StatusCode, Json<ApiError>) { verification_error(code) }

pub async fn rollout_shadow(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>, Json(input): Json<RolloutRequest>) -> Result<Json<RolloutState>, (StatusCode, Json<ApiError>)> {
    let db = store.conn(); let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "settings:write")?; validate_rollout(&input).map_err(rollout_error)?;
    let mut state = store.context_runtime().rollout(); state.shadow_enabled = input.shadow_enabled; state.active_profile = Some(input.profile_id); state.active_generation = Some(input.generation); state.approval_operator = Some(input.approval_operator); state.last_manifest_evidence = Some(input.manifest_evidence); state.last_run_evidence = Some(input.run_evidence); store.context_runtime().set_rollout(state.clone()); Ok(Json(state))
}

pub async fn rollout_canary(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>, Json(input): Json<RolloutRequest>) -> Result<Json<RolloutState>, (StatusCode, Json<ApiError>)> {
    let db = store.conn(); let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "settings:write")?; validate_rollout(&input).map_err(rollout_error)?; if !input.shadow_enabled { return Err(rollout_error("shadow_gate_required")); }
    let mut state = store.context_runtime().rollout(); state.canary_enabled = input.canary_enabled; state.active_lane = CANDIDATE_LANE.into(); state.active_profile = Some(input.profile_id); state.active_generation = Some(input.generation); state.approval_operator = Some(input.approval_operator); state.last_manifest_evidence = Some(input.manifest_evidence); state.last_run_evidence = Some(input.run_evidence); store.context_runtime().set_rollout(state.clone()); Ok(Json(state))
}

pub async fn rollout_promote(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>, Json(input): Json<RolloutRequest>) -> Result<Json<RolloutState>, (StatusCode, Json<ApiError>)> {
    let db = store.conn(); let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "settings:write")?; validate_rollout(&input).map_err(rollout_error)?;
    if !input.fallback_baseline { return Err(verification_error("baseline_fallback_required")); }
    if !store.context_runtime().rollout().canary_enabled { return Err(verification_error("canary_gate_required")); }
    let mut state = store.context_runtime().rollout(); state.promotion_enabled = true; state.baseline_fallback = input.fallback_baseline; state.active_lane = CANDIDATE_LANE.into(); state.active_profile = Some(input.profile_id); state.active_generation = Some(input.generation); state.approval_operator = Some(input.approval_operator); state.last_manifest_evidence = Some(input.manifest_evidence); state.last_run_evidence = Some(input.run_evidence); store.context_runtime().set_rollout(state.clone()); Ok(Json(state))
}

pub async fn rollout_rollback(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>) -> Result<Json<RolloutState>, (StatusCode, Json<ApiError>)> {
    let db = store.conn(); let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "settings:write")?; drop(conn);
    let runtime = store.context_runtime(); runtime.set_rollout(RolloutState::default()); runtime.invalidate_all(&auth.org_id, "rollout_rollback"); Ok(Json(runtime.rollout()))
}

pub async fn diagnostics(State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn(); let conn = db.lock().map_err(|_| verification_error("db_lock"))?; require_permission(&conn, &auth, None, "memory:read")?;
    let active = active_manifest(&conn, &auth.org_id).map_err(fabric_error)?; let runtime = store.context_runtime();
    Ok(Json(serde_json::json!({"cache": runtime.stats(), "rollout": runtime.rollout(), "active_profile": active.as_ref().map(|m| &m.profile_id), "active_generation": active.map(|m| m.generation), "reason_codes": ["stale_evidence_excluded", "freshness_unknown_timestamp", "baseline_fallback_required"]})))
}

/// Laboratory-only BQ/MRL measurement. It never changes the active profile or result lane.
pub async fn lab_shadow(
    State(store): State<SqliteStore>, Extension(auth): Extension<AuthContext>, Json(request): Json<ShadowRequest>,
) -> Result<Json<ShadowResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn(); let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "memory:read")?;
    let capability = request.capability.to_ascii_lowercase();
    let flag_name = match capability.as_str() { "bq" => "CONTEXT_FABRIC_BQ_ENABLED", "mrl" => "CONTEXT_FABRIC_MRL_ENABLED", _ => return Err(verification_error("unsupported_shadow_capability")) };
    if std::env::var(flag_name).as_deref() != Ok("shadow") { return Err(verification_error("capability_flag_not_shadow")); }
    let active = active_manifest(&conn, &auth.org_id).map_err(fabric_error)?.ok_or_else(|| verification_error("no_active_generation"))?;
    if active.acl_generation != request.baseline_manifest.acl_generation || active.policy_generation != request.baseline_manifest.policy_generation
        || active.profile_id != request.baseline_manifest.profile_id || active.profile_version != request.baseline_manifest.profile_version
        || active.generation != request.baseline_manifest.generation { return Err(verification_error("active_profile_generation_mismatch")); }
    let _ = request;
    let _ = active;
    Err(verification_error("not_available"))
}

fn verify_memory_evidence(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    request: AssembleRequest,
) -> Result<AssembleRequest, (StatusCode, Json<ApiError>)> {
    if request.source_cap == 0 {
        return Err(verification_error("invalid_source_cap"));
    }

    if request
        .candidates
        .iter()
        .any(|candidate| candidate.source != "memory")
    {
        return Err(verification_error("unsupported_unverified_source"));
    }

    let ids: Vec<String> = request
        .candidates
        .iter()
        .map(|candidate| candidate.locator.id.clone())
        .collect();
    let unique_ids: HashSet<&str> = ids.iter().map(String::as_str).collect();
    if unique_ids.len() != ids.len()
        || request.candidates.iter().any(|candidate| {
            candidate.unit_id.is_empty()
                || candidate.locator.id.is_empty()
                || candidate.unit_id != candidate.locator.id
                || candidate.locator.source != "memory"
        })
    {
        return Err(verification_error("invalid_memory_locator"));
    }

    // The visibility query is the authorization boundary. Missing rows are deliberately
    // indistinguishable from rows outside the caller's project/tenant scope.
    let viewer = if auth.role.is_super_user() {
        None
    } else {
        Some(auth.user_id.as_str())
    };
    let memories =
        crate::db::queries::get_memories_by_ids_visible(conn, &auth.org_id, &ids, viewer)
            .map_err(|_| verification_error("evidence_verification_failed"))?;
    if memories.len() != unique_ids.len() {
        return Err(verification_error("evidence_not_found"));
    }
    let memories: HashMap<String, crate::models::types::Memory> = memories
        .into_iter()
        .map(|memory| (memory.id.clone(), memory))
        .collect();

    for candidate in &request.candidates {
        let Some(memory) = memories.get(&candidate.locator.id) else {
            return Err(verification_error("evidence_not_found"));
        };
        if candidate.content != memory.content || candidate.provenance != VERIFIED_MEMORY_PROVENANCE
        {
            return Err(verification_error("evidence_integrity_mismatch"));
        }
    }

    Ok(request)
}

/// Read-only Compiler v0 boundary. Retrieval and authorization remain backend-owned.
pub async fn assemble(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<AssembleRequest>,
) -> Result<Json<AssembleResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".into(),
                code: "internal_error".into(),
            }),
        )
    })?;
    require_permission(&conn, &auth, None, "memory:read")?;
    let request = verify_memory_evidence(&conn, &auth, request)?;
    verify_request_generation(&conn, &auth.org_id, &request).map_err(fabric_error)?;
    let runtime = store.context_runtime();
    runtime.purge_expired(&auth.org_id);
    let identity = cache_identity(&conn, &auth, &request, CacheStage::Compile, BASELINE_LANE, request.profile_id.as_deref().unwrap_or("baseline"));
    if let Some(bytes) = runtime.get(&identity) {
        if let Ok(response) = serde_json::from_slice(&bytes) { return Ok(Json(response)); }
    }
    let response = compile(&request);
    if !response.abstained { let _ = runtime.put(identity, serde_json::to_vec(&response).unwrap_or_default(), runtime.ttl(request.freshness_window_secs), true); }
    Ok(Json(response))
}

fn compiled_as_request(
    contract_version: &str,
    profile_id: &str,
    profile_version: u32,
    generation: &GenerationRef,
    assembled: &AssembleResponse,
) -> AssembleRequest {
    AssembleRequest {
        contract_version: contract_version.to_string(),
        tokenizer: "whitespace-v0".into(),
        token_budget: usize::MAX,
        source_cap: usize::MAX,
        excluded_sources: Vec::new(),
        required_sources: Vec::new(),
        generation: generation.clone(),
        profile_id: Some(profile_id.to_string()),
        profile_version: Some(profile_version),
        candidates: assembled.units.clone(),
        freshness_window_secs: None,
    }
}

fn verify_compiled_for_caller(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    contract_version: &str,
    profile_id: &str,
    profile_version: u32,
    generation: &GenerationRef,
    assembled: &AssembleResponse,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let request = compiled_as_request(contract_version, profile_id, profile_version, generation, assembled);
    verify_memory_evidence(conn, auth, request).map(|_| ())
}

pub async fn generate(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<GenerateRequest>,
) -> Result<Json<crate::context_fabric::GenerateResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "memory:read")?;
    let reasons = validate_generation_identity(
        &request.contract_version, &request.profile_id, request.profile_version,
        &request.generation, &request.model, &request.provider, &request.assembled,
    );
    if reasons.iter().any(|r| r == "context_not_compiled") {
        return Ok(Json(crate::context_fabric::GenerateResponse {
            output: None,
            metadata: GenerationMetadata {
                contract_version: request.contract_version.clone(), profile_id: request.profile_id.clone(),
                profile_version: request.profile_version, generation: request.generation.clone(),
                model: request.model.clone(), provider: request.provider.clone(),
                budgets: crate::context_fabric::BudgetReport { requested_tokens: request.output_token_budget, used_tokens: 0 },
                reason_codes: reasons,
            }, provenance: Vec::new(), claims: Vec::new(), abstained: true,
        }))
    }
    verify_compiled_for_caller(&conn, &auth, &request.contract_version, &request.profile_id,
        request.profile_version, &request.generation, &request.assembled)?;
    let runtime = store.context_runtime();
    runtime.purge_expired(&auth.org_id);
    let assembled_request = compiled_as_request(
        &request.contract_version, &request.profile_id, request.profile_version,
        &request.generation, &request.assembled,
    );
    let identity = cache_identity(
        &conn, &auth, &assembled_request, CacheStage::Generate, BASELINE_LANE,
        &request.profile_id,
    );
    if let Some(bytes) = runtime.get(&identity) {
        if let Ok(response) = serde_json::from_slice(&bytes) { return Ok(Json(response)); }
    }
    let response = generate_deterministic(&request);
    if !response.abstained {
        let _ = runtime.put(identity, serde_json::to_vec(&response).unwrap_or_default(), runtime.ttl(None), true);
    }
    Ok(Json(response))
}

pub async fn verify(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| verification_error("db_lock"))?;
    require_permission(&conn, &auth, None, "memory:read")?;
    let identity_reasons = validate_generation_identity(
        &request.contract_version, &request.profile_id, request.profile_version,
        &request.generation, &request.model, &request.provider, &request.assembled,
    );
    if request.assembled.abstained || request.output.as_deref().unwrap_or("").is_empty() {
        return Ok(Json(VerifyResponse {
            status: ClaimStatus::Abstained,
            metadata: GenerationMetadata {
                contract_version: request.contract_version.clone(), profile_id: request.profile_id.clone(),
                profile_version: request.profile_version, generation: request.generation.clone(),
                model: request.model.clone(), provider: request.provider.clone(),
                budgets: crate::context_fabric::BudgetReport { requested_tokens: 0, used_tokens: 0 },
                reason_codes: vec!["abstained".into()],
            }, claims: Vec::new(), reason_codes: vec!["abstained".into()],
        }));
    }
    if !identity_reasons.is_empty() {
        return Ok(Json(VerifyResponse {
            status: ClaimStatus::Contradicted,
            metadata: GenerationMetadata {
                contract_version: request.contract_version.clone(), profile_id: request.profile_id.clone(),
                profile_version: request.profile_version, generation: request.generation.clone(),
                model: request.model.clone(), provider: request.provider.clone(),
                budgets: crate::context_fabric::BudgetReport { requested_tokens: 0, used_tokens: 0 },
                reason_codes: identity_reasons.clone(),
            }, claims: Vec::new(), reason_codes: identity_reasons,
        }));
    }
    if request.provider != crate::context_fabric::DETERMINISTIC_PROVIDER {
        let reason = "provider_unavailable".to_string();
        return Ok(Json(VerifyResponse {
            status: ClaimStatus::Abstained,
            metadata: GenerationMetadata {
                contract_version: request.contract_version, profile_id: request.profile_id,
                profile_version: request.profile_version, generation: request.generation,
                model: request.model, provider: request.provider,
                budgets: crate::context_fabric::BudgetReport { requested_tokens: 0, used_tokens: 0 },
                reason_codes: vec![reason.clone()],
            }, claims: Vec::new(), reason_codes: vec![reason],
        }));
    }
    if let Err((_, error)) = verify_compiled_for_caller(&conn, &auth, &request.contract_version,
        &request.profile_id, request.profile_version, &request.generation, &request.assembled) {
        let code = error.0.code.clone();
        let status = if code == "evidence_not_found" { ClaimStatus::Unauthorized } else { ClaimStatus::Contradicted };
        return Ok(Json(VerifyResponse {
            status, metadata: GenerationMetadata {
                contract_version: request.contract_version, profile_id: request.profile_id,
                profile_version: request.profile_version, generation: request.generation,
                model: request.model, provider: request.provider,
                budgets: crate::context_fabric::BudgetReport { requested_tokens: 0, used_tokens: 0 },
                reason_codes: vec![code.clone()],
            }, claims: Vec::new(), reason_codes: vec![code],
        }));
    }
    let runtime = store.context_runtime();
    runtime.purge_expired(&auth.org_id);
    let assembled_request = compiled_as_request(
        &request.contract_version, &request.profile_id, request.profile_version,
        &request.generation, &request.assembled,
    );
    let mut identity = cache_identity(
        &conn, &auth, &assembled_request, CacheStage::Verify, BASELINE_LANE,
        &request.profile_id,
    );
    identity.source_type = format!("{},{}", identity.source_type, request_fingerprint(&request));
    if let Some(bytes) = runtime.get(&identity) {
        if let Ok(response) = serde_json::from_slice(&bytes) { return Ok(Json(response)); }
    }
    let units: HashMap<&str, &crate::context_fabric::CandidateEvidence> = request.assembled.units.iter()
        .map(|unit| (unit.unit_id.as_str(), unit)).collect();
    let output = request.output.as_deref().unwrap_or("");
    let mut results = Vec::new();
    for claim in &request.claims {
        let Some(unit) = units.get(claim.unit_id.as_str()) else {
            results.push(ClaimVerification { id: claim.id.clone(), status: ClaimStatus::Unauthorized, reason_codes: vec!["claim_unit_not_allowed".into()], locator: None });
            continue;
        };
        if claim.locator != unit.locator {
            results.push(ClaimVerification { id: claim.id.clone(), status: ClaimStatus::Unauthorized, reason_codes: vec!["locator_mismatch".into()], locator: None });
        } else if unit.generation != request.generation {
            results.push(ClaimVerification { id: claim.id.clone(), status: ClaimStatus::Stale, reason_codes: vec!["generation_mismatch".into()], locator: None });
        } else if !unit.fresh {
            results.push(ClaimVerification { id: claim.id.clone(), status: ClaimStatus::Stale, reason_codes: vec!["stale_evidence".into()], locator: None });
        } else if unit.content.contains(&claim.text) {
            results.push(ClaimVerification { id: claim.id.clone(), status: ClaimStatus::Verified, reason_codes: Vec::new(), locator: Some(unit.locator.clone()) });
        } else if output.contains(&claim.text) {
            results.push(ClaimVerification { id: claim.id.clone(), status: ClaimStatus::Contradicted, reason_codes: vec!["claim_not_supported_by_unit".into()], locator: None });
        } else {
            results.push(ClaimVerification { id: claim.id.clone(), status: ClaimStatus::Unsupported, reason_codes: vec!["claim_not_found".into()], locator: None });
        }
    }
    let status = if results.iter().all(|claim| claim.status == ClaimStatus::Verified) { ClaimStatus::Verified }
        else if results.iter().any(|claim| claim.status == ClaimStatus::Unauthorized) { ClaimStatus::Unauthorized }
        else if results.iter().any(|claim| claim.status == ClaimStatus::Stale) { ClaimStatus::Stale }
        else if results.iter().any(|claim| claim.status == ClaimStatus::Contradicted) { ClaimStatus::Contradicted }
        else { ClaimStatus::Unsupported };
    let response = VerifyResponse {
        status,
        metadata: GenerationMetadata { contract_version: request.contract_version, profile_id: request.profile_id,
            profile_version: request.profile_version, generation: request.generation, model: request.model,
            provider: request.provider, budgets: crate::context_fabric::BudgetReport { requested_tokens: 0, used_tokens: 0 }, reason_codes: Vec::new() },
        claims: results, reason_codes: Vec::new(),
    };
    let _ = runtime.put(identity, serde_json::to_vec(&response).unwrap_or_default(), runtime.ttl(None), true);
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::connect, migrations, queries},
        models::types::StoreMemoryRequest,
    };

    fn setup() -> (rusqlite::Connection, AuthContext, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "Admin").unwrap();
        let memory = queries::upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "verified memory".into(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();
        (
            conn,
            AuthContext {
                org_id: org.id,
                user_id: user.id,
                role: "member".parse().unwrap(),
            },
            memory.id,
        )
    }

    fn request(id: &str, content: &str, provenance: &str, source_cap: usize) -> AssembleRequest {
        AssembleRequest {
            contract_version: crate::context_fabric::CONTRACT_VERSION.into(),
            tokenizer: "whitespace-v0".into(),
            token_budget: 20,
            source_cap,
            excluded_sources: vec![],
            required_sources: vec![],
            generation: crate::context_fabric::GenerationRef {
                id: "g1".into(),
                version: 1,
            },
            profile_id: None,
            profile_version: None,
            candidates: vec![crate::context_fabric::CandidateEvidence {
                unit_id: id.into(),
                source: "memory".into(),
                content: content.into(),
                locator: crate::context_fabric::Locator {
                    source: "memory".into(),
                    id: id.into(),
                    reference: None,
                },
                provenance: provenance.into(),
                generation: crate::context_fabric::GenerationRef {
                    id: "g1".into(),
                    version: 1,
                },
                fresh: true,
                required: false,
                captured_at_unix: None,
            }],
            freshness_window_secs: None,
        }
    }

    #[test]
    fn authorized_memory_is_verifiable() {
        let (conn, auth, id) = setup();
        let verified = verify_memory_evidence(
            &conn,
            &auth,
            request(&id, "verified memory", VERIFIED_MEMORY_PROVENANCE, 1),
        )
        .unwrap();
        assert_eq!(verified.candidates[0].locator.id, id);
    }

    #[test]
    fn manipulated_content_and_unknown_source_are_rejected_without_details() {
        let (conn, auth, id) = setup();
        let error = verify_memory_evidence(
            &conn,
            &auth,
            request(&id, "tampered", VERIFIED_MEMORY_PROVENANCE, 1),
        )
        .unwrap_err();
        assert_eq!(error.1 .0.code, "evidence_integrity_mismatch");
        let error = verify_memory_evidence(&conn, &auth, {
            let mut req = request(&id, "verified memory", VERIFIED_MEMORY_PROVENANCE, 1);
            req.candidates[0].source = "code".into();
            req
        })
        .unwrap_err();
        assert_eq!(error.1 .0.code, "unsupported_unverified_source");
    }

    #[test]
    fn zero_source_cap_is_rejected_deterministically() {
        let (conn, auth, id) = setup();
        let error = verify_memory_evidence(
            &conn,
            &auth,
            request(&id, "verified memory", VERIFIED_MEMORY_PROVENANCE, 0),
        )
        .unwrap_err();
        assert_eq!(error.1 .0.code, "invalid_source_cap");
    }

    #[test]
    fn rollout_requires_gate_evidence_and_rejects_lab_capabilities() {
        let base = RolloutRequest {
            profile_id: "candidate".into(), generation: GenerationRef { id: "g2".into(), version: 2 },
            manifest_evidence: "manifest:sha256:ok".into(), run_evidence: "run:ok".into(),
            approval_operator: "operator".into(), fallback_baseline: false, shadow_enabled: false, canary_enabled: false,
        };
        assert_eq!(validate_rollout(&base), Ok(()));
        let mut missing = base;
        missing.manifest_evidence = "".into();
        assert_eq!(validate_rollout(&missing), Err("missing_manifest_evidence"));
        let mut lab = missing;
        lab.manifest_evidence = "manifest:ok".into();
        lab.profile_id = "bq-candidate".into();
        assert_eq!(validate_rollout(&lab), Err("unsupported_rollout_capability"));
    }
}
