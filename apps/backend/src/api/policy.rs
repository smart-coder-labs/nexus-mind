use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Serialize;

use serde::Deserialize;

use crate::{
    api::helpers::{require_permission, resolve_list_pagination, AppJson},
    db::queries,
    models::types::{
        ApiError, AuthContext, Policy, PolicyCheckRequest, PolicyCheckResponse, UpdatePolicyRequest,
    },
    store::sqlite::SqliteStore,
};

// ── Error helpers ─────────────────────────────────────────────────────────────

fn internal_error(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn bad_request(msg: &str, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: msg.to_string(),
            code: code.to_string(),
        }),
    )
}

fn not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Policy not found".to_string(),
            code: "policy_not_found".to_string(),
        }),
    )
}

// ── Valid rule types ───────────────────────────────────────────────────────────

const VALID_RULE_TYPES: &[&str] = &["model_whitelist", "budget_limit", "pii_redact"];

/// Validate a `CreatePolicyRequest` config against its rule_type. Returns an
/// error tuple on failure.
fn validate_config(
    rule_type: &str,
    config: &serde_json::Value,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    match rule_type {
        "model_whitelist" => {
            let models = config
                .get("allowed_models")
                .and_then(|v| v.as_array());
            match models {
                None => return Err(bad_request(
                    "config.allowed_models must be a non-empty array",
                    "invalid_config",
                )),
                Some(arr) if arr.is_empty() => return Err(bad_request(
                    "config.allowed_models must be a non-empty array",
                    "invalid_config",
                )),
                _ => {}
            }
        }
        "budget_limit" => {
            let has_tokens = config.get("max_tokens_per_day").and_then(|v| v.as_i64()).is_some();
            let has_requests = config.get("max_requests_per_day").and_then(|v| v.as_i64()).is_some();
            if !has_tokens && !has_requests {
                return Err(bad_request(
                    "config must have at least one of max_tokens_per_day or max_requests_per_day",
                    "invalid_config",
                ));
            }
            if let Some(v) = config.get("max_tokens_per_day").and_then(|v| v.as_i64()) {
                if v <= 0 {
                    return Err(bad_request("max_tokens_per_day must be positive", "invalid_config"));
                }
            }
            if let Some(v) = config.get("max_requests_per_day").and_then(|v| v.as_i64()) {
                if v <= 0 {
                    return Err(bad_request("max_requests_per_day must be positive", "invalid_config"));
                }
            }
        }
        "pii_redact" => {
            let patterns = config
                .get("patterns")
                .and_then(|v| v.as_array());
            match patterns {
                None => return Err(bad_request(
                    "config.patterns must be a non-empty array",
                    "invalid_config",
                )),
                Some(arr) if arr.is_empty() => return Err(bad_request(
                    "config.patterns must be a non-empty array",
                    "invalid_config",
                )),
                Some(arr) => {
                    for p in arr {
                        if let Some(s) = p.as_str() {
                            if s.len() > 256 {
                                return Err(bad_request(
                                    "each pattern must be ≤256 chars",
                                    "invalid_config",
                                ));
                            }
                            regex::Regex::new(s).map_err(|_| bad_request(
                                &format!("pattern '{}' is not a valid regex", s),
                                "invalid_config",
                            ))?;
                        }
                    }
                }
            }
        }
        _ => {
            return Err(bad_request(
                &format!("rule_type must be one of: {}", VALID_RULE_TYPES.join(", ")),
                "invalid_rule_type",
            ));
        }
    }
    Ok(())
}

// ── Request types ─────────────────────────────────────────────────────────────

/// Raw deserialization target for `POST /v1/policies`. Using a flat struct with
/// `rule_type: String` and `config: serde_json::Value` lets us return 400 (not
/// 422) when `rule_type` is unknown — the typed `PolicyConfig` enum would
/// reject the request at the serde layer before the handler runs.
#[derive(Deserialize)]
pub struct RawCreatePolicyRequest {
    pub name: String,
    pub rule_type: String,
    pub config: serde_json::Value,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Project to scope this policy to. None = org-wide (applies to every project).
    #[serde(default)]
    pub project_id: Option<String>,
}

fn default_enabled() -> bool {
    true
}

// ── Response wrappers ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PoliciesResponse {
    pub policies: Vec<Policy>,
}

#[derive(Deserialize)]
pub struct ListPoliciesParams {
    /// Max rows to return. Pagination is opt-in — when neither `limit` nor
    /// `offset` is provided, the full list is returned unbounded. Once
    /// provided, `limit` is clamped to 500 (never errors).
    pub limit: Option<i64>,
    /// Rows to skip. Defaults to 0.
    pub offset: Option<i64>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /v1/policies` — list policies for the caller's org.
/// Requires `policy:read`.
pub async fn list_policies(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    axum::extract::Query(params): axum::extract::Query<ListPoliciesParams>,
) -> Result<Json<PoliciesResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "policy:read")?;

    let (limit, offset) = resolve_list_pagination(params.limit, params.offset);
    let viewer = if ctx.role.is_super_user() { None } else { Some(ctx.user_id.as_str()) };
    let policies = queries::list_policies_visible(&conn, &ctx.org_id, limit, offset, viewer)
        .map_err(internal_error)?;
    Ok(Json(PoliciesResponse { policies }))
}

/// `POST /v1/policies` — create a new policy.
/// Requires `policy:write`.
pub async fn create_policy(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    AppJson(req): AppJson<RawCreatePolicyRequest>,
) -> Result<(StatusCode, Json<Policy>), (StatusCode, Json<ApiError>)> {
    // Validate name.
    let name = req.name.trim();
    if name.is_empty() {
        return Err(bad_request("name must not be empty", "invalid_name"));
    }
    if name.len() > 128 {
        return Err(bad_request("name must be at most 128 characters", "invalid_name"));
    }

    // Validate rule_type is one of the known variants.
    if !VALID_RULE_TYPES.contains(&req.rule_type.as_str()) {
        return Err(bad_request(
            &format!("rule_type must be one of: {}", VALID_RULE_TYPES.join(", ")),
            "invalid_rule_type",
        ));
    }

    // Validate config shape for the rule_type.
    validate_config(&req.rule_type, &req.config)?;

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "policy:write")?;

    let id = uuid::Uuid::new_v4().to_string();
    let config_json = serde_json::to_string(&req.config).map_err(|e| internal_error(e.into()))?;

    let policy = queries::insert_policy(
        &conn,
        &id,
        &ctx.org_id,
        name,
        &req.rule_type,
        &config_json,
        req.enabled,
        req.project_id.as_deref(),
    )
    .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(policy)))
}

/// `PATCH /v1/policies/:id` — update an existing policy.
/// Requires `policy:write`. `rule_type` is immutable.
pub async fn update_policy(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(req): AppJson<UpdatePolicyRequest>,
) -> Result<Json<Policy>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "policy:write")?;

    // Fetch the existing policy to validate rule_type immutability and config shape.
    let existing = queries::get_policy(&conn, &id, &ctx.org_id)
        .map_err(internal_error)?
        .ok_or_else(not_found)?;

    // rule_type is immutable — reject any attempt to change it.
    if req.rule_type.is_some() {
        return Err(bad_request(
            "rule_type cannot be changed after creation",
            "immutable_rule_type",
        ));
    }

    // Validate name if provided.
    if let Some(ref n) = req.name {
        let trimmed = n.trim();
        if trimmed.is_empty() {
            return Err(bad_request("name must not be empty", "invalid_name"));
        }
        if trimmed.len() > 128 {
            return Err(bad_request("name must be at most 128 characters", "invalid_name"));
        }
    }

    // Validate config if provided — must still match existing rule_type.
    if let Some(ref cfg) = req.config {
        validate_config(&existing.rule_type, cfg)?;
    }

    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    let config_json: Option<String> = match &req.config {
        Some(cfg) => Some(serde_json::to_string(cfg).map_err(|e| internal_error(e.into()))?),
        None => None,
    };

    let updated = queries::update_policy(
        &conn,
        &id,
        &ctx.org_id,
        req.name.as_deref(),
        config_json.as_deref(),
        req.enabled,
        &now,
    )
    .map_err(internal_error)?
    .ok_or_else(not_found)?;

    Ok(Json(updated))
}

/// `DELETE /v1/policies/:id` — delete a policy.
/// Requires `policy:write`. Returns 404 if not found.
pub async fn delete_policy(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "policy:write")?;

    let deleted = queries::delete_policy(&conn, &id, &ctx.org_id).map_err(internal_error)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// `POST /v1/policy/check` — evaluate policies against an incoming request.
/// No special role required — any authenticated API key may call this.
pub async fn check_policy(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    AppJson(req): AppJson<PolicyCheckRequest>,
) -> Result<Json<PolicyCheckResponse>, (StatusCode, Json<ApiError>)> {
    if req.model.is_empty() {
        return Err(bad_request("model is required", "invalid_request"));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let viewer = if ctx.role.is_super_user() { None } else { Some(ctx.user_id.as_str()) };
    if let (Some(project_id), Some(user_id)) = (req.project.as_deref(), viewer) {
        let is_member = queries::user_is_project_member(&conn, &ctx.org_id, project_id, user_id)
            .map_err(internal_error)?;
        if !is_member {
            return Ok(Json(PolicyCheckResponse {
                allowed: false,
                violations: vec![crate::models::types::PolicyViolation {
                    policy_id: "project_access".to_string(),
                    policy_name: "Project access".to_string(),
                    rule_type: "authorization".to_string(),
                    reason: "Access denied to this project".to_string(),
                }],
            }));
        }
    }

    let policies = queries::list_enabled_policies_visible(&conn, &ctx.org_id, req.project.as_deref(), viewer)
        .map_err(internal_error)?;
    let stats = queries::fetch_daily_stats(&conn, &ctx.org_id).map_err(internal_error)?;

    // prompt_tokens counts as +1 to requests_used for budget check.
    let requests_used = stats.requests_today as u64
        + if req.prompt_tokens.is_some() { 1 } else { 0 };
    let tokens_used = stats.tokens_today as u64
        + req.prompt_tokens.unwrap_or(0).max(0) as u64;

    let response = crate::policy::evaluate(&policies, &req, tokens_used, requests_used);
    Ok(Json(response))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{get, patch, post},
        Router,
    };
    use tower::util::ServiceExt;

    use crate::api::middleware as auth_mw;
    use crate::db::{connection::connect, migrations};
    use crate::db::queries::bootstrap;
    use crate::store::sqlite::SqliteStore;

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/policies", get(super::list_policies).post(super::create_policy))
            .route(
                "/v1/policies/:id",
                patch(super::update_policy).delete(super::delete_policy),
            )
            .route("/v1/policy/check", post(super::check_policy))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn admin_key(store: &SqliteStore) -> String {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        key
    }

    fn create_user_with_role(store: &SqliteStore, org_id: &str, role: &str) -> String {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, ?3, 'Test', ?4, 'active', datetime('now'))",
            rusqlite::params![user_id, org_id, format!("{role}@test.com"), role],
        )
        .unwrap();
        let key_id = Uuid::new_v4().to_string();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org_id, key_hash],
        )
        .unwrap();
        raw_key
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── list ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_returns_empty_vec_initially() {
        let store = make_store();
        let key = admin_key(&store);

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["policies"], serde_json::json!([]));
    }

    // ── create ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_returns_201_with_policy() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({
            "name": "Whitelist Only Sonnet",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] }
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        assert_eq!(body["name"], "Whitelist Only Sonnet");
        assert_eq!(body["rule_type"], "model_whitelist");
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn create_with_invalid_rule_type_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({
            "name": "Bad Policy",
            "rule_type": "banana",
            "config": {}
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_with_empty_allowed_models_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({
            "name": "Empty Whitelist",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": [] }
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["code"], "invalid_config");
    }

    #[tokio::test]
    async fn create_as_member_returns_403() {
        let store = make_store();
        let member_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            drop(conn);
            create_user_with_role(&store, &org.id, "member")
        };

        let payload = serde_json::json!({
            "name": "Test",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude"] }
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn list_as_member_excludes_non_member_project_policies() {
        let store = make_store();
        let (org_id, member_key) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            drop(conn);
            let key = create_user_with_role(&store, &org.id, "member");
            (org.id, key)
        };

        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let member_id: String = conn.query_row("SELECT id FROM users WHERE email = 'member@test.com'", [], |r| r.get(0)).unwrap();
            let shared = crate::db::queries::create_project(&conn, &org_id, "shared", None, None).unwrap();
            let secret = crate::db::queries::create_project(&conn, &org_id, "secret", None, None).unwrap();
            conn.execute(
                "INSERT INTO project_members (id, project_id, user_id, role, created_at) VALUES ('pm_shared', ?1, ?2, 'member', datetime('now'))",
                rusqlite::params![shared.id, member_id],
            ).unwrap();
            crate::db::queries::insert_policy(&conn, "pol_global", &org_id, "Global Policy", "model_whitelist", r#"{"allowed_models":["claude"]}"#, true, None).unwrap();
            crate::db::queries::insert_policy(&conn, "pol_shared", &org_id, "Shared Policy", "model_whitelist", r#"{"allowed_models":["claude"]}"#, true, Some(&shared.id)).unwrap();
            crate::db::queries::insert_policy(&conn, "pol_secret", &org_id, "Secret Policy", "model_whitelist", r#"{"allowed_models":["claude"]}"#, true, Some(&secret.id)).unwrap();
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        let names: Vec<&str> = body["policies"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"Global Policy"));
        assert!(names.contains(&"Shared Policy"));
        assert!(!names.contains(&"Secret Policy"));
    }

    // ── update ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn update_returns_200() {
        let store = make_store();
        let key = admin_key(&store);

        // Create a policy first.
        let create_payload = serde_json::json!({
            "name": "Original Name",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude"] }
        });
        let create_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let created = body_json(create_resp).await;
        let policy_id = created["id"].as_str().unwrap().to_string();

        // Now update it.
        let update_payload = serde_json::json!({ "name": "Updated Name" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/policies/{policy_id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(update_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["name"], "Updated Name");
    }

    #[tokio::test]
    async fn update_unknown_id_returns_404() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({ "name": "New Name" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/v1/policies/nonexistent-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── delete ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn delete_returns_204() {
        let store = make_store();
        let key = admin_key(&store);

        // Create a policy to delete.
        let create_payload = serde_json::json!({
            "name": "To Delete",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude"] }
        });
        let create_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let created = body_json(create_resp).await;
        let policy_id = created["id"].as_str().unwrap().to_string();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/policies/{policy_id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_unknown_id_returns_404() {
        let store = make_store();
        let key = admin_key(&store);

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/policies/nonexistent-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── check ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn check_with_no_policies_returns_allowed_true() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({ "model": "gpt-4" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/check")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["allowed"], true);
        assert_eq!(body["violations"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn check_with_model_whitelist_blocking_returns_denied() {
        let store = make_store();
        let key = admin_key(&store);

        // Create a model_whitelist policy that only allows Sonnet.
        let create_payload = serde_json::json!({
            "name": "Sonnet Only",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] }
        });
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check with a model not in the whitelist.
        let check_payload = serde_json::json!({ "model": "gpt-4" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/check")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(check_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(!body["allowed"].as_bool().unwrap_or(true));
        assert!(!body["violations"].as_array().unwrap().is_empty());
        let violation = &body["violations"][0];
        assert_eq!(violation["rule_type"], "model_whitelist");
        assert!(violation["reason"].as_str().unwrap().contains("gpt-4"));
    }

    #[tokio::test]
    async fn update_policy_rule_type_returns_400_immutable() {
        let store = make_store();
        let key = admin_key(&store);

        // Create a policy first
        let create_payload = serde_json::json!({
            "name": "Whitelist",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude-3-5-sonnet"] }
        });
        let create_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let policy_id = body_json(create_resp).await["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Attempt to change rule_type — must return 400
        let patch_payload = serde_json::json!({ "rule_type": "budget_limit" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/policies/{policy_id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["code"], "immutable_rule_type");
    }

    #[tokio::test]
    async fn check_unauthenticated_returns_401() {
        let store = make_store();

        let payload = serde_json::json!({ "model": "gpt-4" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/check")
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── check: project scoping ───────────────────────────────────────────────

    #[tokio::test]
    async fn create_policy_with_project_id_round_trips() {
        let store = make_store();
        let key = admin_key(&store);

        let project_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            crate::db::queries::create_project(&conn, &org_id, "proj-a", None, None)
                .unwrap()
                .id
        };

        let payload = serde_json::json!({
            "name": "Scoped Policy",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] },
            "project_id": project_id
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        assert_eq!(body["project_id"], project_id);
    }

    #[tokio::test]
    async fn create_policy_without_project_id_is_org_wide() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({
            "name": "Org Wide Policy",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] }
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        assert_eq!(body["project_id"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn check_project_scoped_policy_only_applies_to_matching_project() {
        let store = make_store();
        let key = admin_key(&store);

        let (project_a, project_b) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let a = crate::db::queries::create_project(&conn, &org_id, "proj-a", None, None).unwrap().id;
            let b = crate::db::queries::create_project(&conn, &org_id, "proj-b", None, None).unwrap().id;
            (a, b)
        };

        // Create a model_whitelist policy scoped to project_a only.
        let create_payload = serde_json::json!({
            "name": "Proj A Whitelist",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] },
            "project_id": project_a
        });
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Checking against project_a with a disallowed model must be denied.
        let check_a = serde_json::json!({ "model": "gpt-4", "project": project_a });
        let resp_a = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/check")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(check_a.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body_a = body_json(resp_a).await;
        assert_eq!(body_a["allowed"], false, "project_a scoped policy must apply when checking project_a");

        // Checking against project_b (a different project) must be allowed — the
        // project-scoped policy must not leak into another project.
        let check_b = serde_json::json!({ "model": "gpt-4", "project": project_b });
        let resp_b = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/check")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(check_b.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body_b = body_json(resp_b).await;
        assert_eq!(body_b["allowed"], true, "project_a scoped policy must NOT apply when checking project_b");
    }

    #[tokio::test]
    async fn check_as_member_denies_non_member_project_before_policy_evaluation() {
        let store = make_store();
        let (org_id, member_key) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            drop(conn);
            let key = create_user_with_role(&store, &org.id, "member");
            (org.id, key)
        };
        let secret_project = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let secret = crate::db::queries::create_project(&conn, &org_id, "secret", None, None).unwrap();
            crate::db::queries::insert_policy(
                &conn,
                "pol_secret",
                &org_id,
                "Secret Allows GPT",
                "model_whitelist",
                r#"{"allowed_models":["gpt-4"]}"#,
                true,
                Some(&secret.id),
            ).unwrap();
            secret.id
        };

        let payload = serde_json::json!({ "model": "gpt-4", "project": secret_project });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/check")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["allowed"], false);
        assert_eq!(body["violations"][0]["rule_type"], "authorization");
    }

    #[tokio::test]
    async fn check_org_wide_policy_applies_regardless_of_project() {
        let store = make_store();
        let key = admin_key(&store);

        // Create an org-wide model_whitelist policy (no project_id).
        let create_payload = serde_json::json!({
            "name": "Org Wide Whitelist",
            "rule_type": "model_whitelist",
            "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] }
        });
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let check_payload = serde_json::json!({ "model": "gpt-4", "project": "any-project" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/check")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(check_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        assert_eq!(body["allowed"], false, "org-wide policy must apply regardless of project");
    }

    // ── pagination tests ──────────────────────────────────────────────────────

    /// Inserts a policy directly via the query layer and pins its `created_at` to a
    /// deterministic value so list ordering (created_at DESC) is stable in tests.
    fn insert_policy_at(store: &SqliteStore, org_id: &str, name: &str, created_at: &str) {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let id = format!("p_{}", uuid::Uuid::new_v4().simple());
        crate::db::queries::insert_policy(
            &conn, &id, org_id, name, "model_whitelist",
            r#"{"allowed_models":["claude"]}"#, true, None,
        ).unwrap();
        conn.execute(
            "UPDATE policies SET created_at = ?1 WHERE id = ?2",
            rusqlite::params![created_at, id],
        ).unwrap();
    }

    #[tokio::test]
    async fn list_default_returns_everything_under_the_default_limit() {
        let store = make_store();
        let key = admin_key(&store);
        let org_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get::<_, String>(0)).unwrap()
        };
        for i in 0..3 {
            insert_policy_at(&store, &org_id, &format!("P{i}"), &format!("2025-01-0{}T00:00:00.000Z", i + 1));
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["policies"].as_array().unwrap().len(), 3, "no limit/offset must still return everything under the default cap");
    }

    #[tokio::test]
    async fn list_without_params_returns_full_unbounded_list_beyond_100() {
        // Pagination is opt-in: when the caller sends neither `limit` nor
        // `offset`, the endpoint must behave exactly like before pagination
        // was introduced — i.e. return every policy for the org, even
        // beyond the old DEFAULT_LIST_LIMIT of 100.
        let store = make_store();
        let key = admin_key(&store);
        let org_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get::<_, String>(0)).unwrap()
        };
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            for i in 0..150 {
                let id = format!("p_{}", uuid::Uuid::new_v4().simple());
                crate::db::queries::insert_policy(
                    &conn, &id, &org_id, &format!("P{i}"), "model_whitelist",
                    r#"{"allowed_models":["claude"]}"#, true, None,
                ).unwrap();
            }
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(
            body["policies"].as_array().unwrap().len(), 150,
            "no limit/offset must return the full unbounded list, not truncate at 100"
        );
    }

    #[tokio::test]
    async fn list_respects_explicit_limit_and_offset() {
        let store = make_store();
        let key = admin_key(&store);
        let org_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get::<_, String>(0)).unwrap()
        };
        // created_at DESC ordering: newest first → P5, P4, P3, P2, P1
        for i in 1..=5 {
            insert_policy_at(&store, &org_id, &format!("P{i}"), &format!("2025-01-0{i}T00:00:00.000Z"));
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies?limit=2&offset=1")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        let names: Vec<&str> = body["policies"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["P4", "P3"], "limit=2&offset=1 must return the 2nd and 3rd most recent policies");
    }

    #[tokio::test]
    async fn list_limit_is_clamped_to_500_not_rejected() {
        let store = make_store();
        let key = admin_key(&store);
        let org_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get::<_, String>(0)).unwrap()
        };
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            for i in 0..505 {
                let id = format!("p_{}", uuid::Uuid::new_v4().simple());
                crate::db::queries::insert_policy(
                    &conn, &id, &org_id, &format!("P{i}"), "model_whitelist",
                    r#"{"allowed_models":["claude"]}"#, true, None,
                ).unwrap();
            }
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies?limit=10000")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "an over-max limit must be clamped, never rejected");
        let body = body_json(resp).await;
        assert_eq!(body["policies"].as_array().unwrap().len(), 500, "limit must be clamped to the 500 max, not the requested 10000 or the full 505 rows");
    }

    #[tokio::test]
    async fn list_limit_zero_returns_empty_not_error() {
        let store = make_store();
        let key = admin_key(&store);
        let org_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get::<_, String>(0)).unwrap()
        };
        insert_policy_at(&store, &org_id, "P0", "2025-01-01T00:00:00.000Z");

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies?limit=0")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "limit=0 must not error");
        let body = body_json(resp).await;
        assert_eq!(body["policies"].as_array().unwrap().len(), 0, "limit=0 must return an empty list");
    }

    #[tokio::test]
    async fn list_negative_limit_is_clamped_to_zero_not_error() {
        let store = make_store();
        let key = admin_key(&store);
        let org_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get::<_, String>(0)).unwrap()
        };
        insert_policy_at(&store, &org_id, "P0", "2025-01-01T00:00:00.000Z");

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/policies?limit=-5")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "negative limit must not error");
        let body = body_json(resp).await;
        assert_eq!(body["policies"].as_array().unwrap().len(), 0, "negative limit must be clamped to 0 rows, not treated as unbounded");
    }
}
