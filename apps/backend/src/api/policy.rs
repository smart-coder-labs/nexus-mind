use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Serialize;

use serde::Deserialize;

use crate::{
    api::helpers::{require_permission, AppJson},
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
}

fn default_enabled() -> bool {
    true
}

// ── Query params ──────────────────────────────────────────────────────────────

/// Columns that `GET /v1/policies` may be sorted by.
const VALID_SORT_FIELDS: &[&str] = &["created_at", "updated_at", "name"];

/// Query parameters for `GET /v1/policies`. A non-numeric `limit`/`offset` fails
/// deserialization and is rejected by axum with `400 Bad Request`.
#[derive(Deserialize)]
pub struct ListPoliciesParams {
    pub rule_type: Option<String>,
    pub enabled: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort: Option<String>,
    pub order: Option<String>,
}

// ── Response wrappers ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PoliciesResponse {
    pub policies: Vec<Policy>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /v1/policies` — list policies for the caller's org with optional
/// filtering (`rule_type`, `enabled`), sorting (`sort`, `order`), and pagination
/// (`limit`, `offset`). Returns a `{policies, total, limit, offset}` envelope.
/// Requires `policy:read`.
pub async fn list_policies(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Query(params): Query<ListPoliciesParams>,
) -> Result<Json<PoliciesResponse>, (StatusCode, Json<ApiError>)> {
    // Validate pagination bounds.
    let limit = params.limit.unwrap_or(50);
    if limit < 0 {
        return Err(bad_request("limit must be non-negative", "validation_error"));
    }
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err(bad_request("offset must be non-negative", "validation_error"));
    }

    // Validate rule_type filter against the known set.
    if let Some(ref rt) = params.rule_type {
        if !VALID_RULE_TYPES.contains(&rt.as_str()) {
            return Err(bad_request(
                &format!("rule_type must be one of: {}", VALID_RULE_TYPES.join(", ")),
                "invalid_rule_type",
            ));
        }
    }

    // Validate sort/order against allowlists before inlining into SQL.
    let sort = params.sort.as_deref().unwrap_or("created_at");
    if !VALID_SORT_FIELDS.contains(&sort) {
        return Err(bad_request(
            &format!("sort must be one of: {}", VALID_SORT_FIELDS.join(", ")),
            "invalid_sort",
        ));
    }
    let order = match params.order.as_deref().unwrap_or("desc") {
        o @ ("asc" | "desc") => o,
        _ => return Err(bad_request("order must be 'asc' or 'desc'", "invalid_order")),
    };

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "policy:read")?;

    let (policies, total) = queries::list_policies_paginated(
        &conn,
        &ctx.org_id,
        params.rule_type.as_deref(),
        params.enabled,
        sort,
        order,
        limit,
        offset,
    )
    .map_err(internal_error)?;

    Ok(Json(PoliciesResponse {
        policies,
        total,
        limit,
        offset,
    }))
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

    let policies = queries::list_enabled_policies(&conn, &ctx.org_id).map_err(internal_error)?;
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
        assert_eq!(body["total"], 0);
        assert_eq!(body["limit"], 50);
        assert_eq!(body["offset"], 0);
    }

    // Helper: create a policy via the API and return its id.
    async fn create_policy_via_api(
        store: &SqliteStore,
        key: &str,
        payload: serde_json::Value,
    ) -> String {
        let resp = app(store.clone())
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
        body_json(resp).await["id"].as_str().unwrap().to_string()
    }

    async fn get_policies(store: &SqliteStore, key: &str, query: &str) -> axum::response::Response {
        app(store.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/policies{query}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn seed_four_policies(store: &SqliteStore, key: &str) {
        create_policy_via_api(
            store,
            key,
            serde_json::json!({
                "name": "Whitelist A",
                "rule_type": "model_whitelist",
                "config": { "allowed_models": ["claude"] }
            }),
        )
        .await;
        create_policy_via_api(
            store,
            key,
            serde_json::json!({
                "name": "Redact Email",
                "rule_type": "pii_redact",
                "config": { "patterns": ["\\d+"] }
            }),
        )
        .await;
        create_policy_via_api(
            store,
            key,
            serde_json::json!({
                "name": "Redact Phone",
                "rule_type": "pii_redact",
                "config": { "patterns": ["\\w+"] }
            }),
        )
        .await;
        create_policy_via_api(
            store,
            key,
            serde_json::json!({
                "name": "Budget 1M",
                "rule_type": "budget_limit",
                "config": { "max_tokens_per_day": 1000000 }
            }),
        )
        .await;
    }

    #[tokio::test]
    async fn list_filters_by_rule_type() {
        let store = make_store();
        let key = admin_key(&store);
        seed_four_policies(&store, &key).await;

        let resp = get_policies(&store, &key, "?rule_type=pii_redact").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["policies"].as_array().unwrap().len(), 2);
        assert_eq!(body["total"], 2);
    }

    #[tokio::test]
    async fn list_filters_by_enabled() {
        let store = make_store();
        let key = admin_key(&store);
        let id = create_policy_via_api(
            &store,
            &key,
            serde_json::json!({
                "name": "Disabled",
                "rule_type": "model_whitelist",
                "config": { "allowed_models": ["claude"] },
                "enabled": false
            }),
        )
        .await;
        create_policy_via_api(
            &store,
            &key,
            serde_json::json!({
                "name": "Enabled",
                "rule_type": "model_whitelist",
                "config": { "allowed_models": ["claude"] }
            }),
        )
        .await;

        let resp = get_policies(&store, &key, "?enabled=false").await;
        let body = body_json(resp).await;
        assert_eq!(body["total"], 1);
        assert_eq!(body["policies"][0]["id"], id);
    }

    #[tokio::test]
    async fn list_paginates_with_limit() {
        let store = make_store();
        let key = admin_key(&store);
        seed_four_policies(&store, &key).await;

        let resp = get_policies(&store, &key, "?limit=2").await;
        let body = body_json(resp).await;
        assert_eq!(body["policies"].as_array().unwrap().len(), 2);
        assert_eq!(body["total"], 4);
        assert_eq!(body["limit"], 2);
        assert_eq!(body["offset"], 0);
    }

    #[tokio::test]
    async fn list_non_numeric_limit_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        let resp = get_policies(&store, &key, "?limit=abc").await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_sort_and_order_applied() {
        let store = make_store();
        let key = admin_key(&store);
        seed_four_policies(&store, &key).await;

        // sort=name&order=asc → "Budget 1M" first (deterministic, no timestamp ties).
        let resp = get_policies(&store, &key, "?sort=name&order=asc").await;
        let body = body_json(resp).await;
        assert_eq!(body["policies"][0]["name"], "Budget 1M");

        // order=desc reverses it → "Whitelist A" first.
        let resp = get_policies(&store, &key, "?sort=name&order=desc").await;
        let body = body_json(resp).await;
        assert_eq!(body["policies"][0]["name"], "Whitelist A");
    }

    #[tokio::test]
    async fn list_invalid_sort_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        let resp = get_policies(&store, &key, "?sort=evil").await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["code"], "invalid_sort");
    }

    #[tokio::test]
    async fn list_invalid_order_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        let resp = get_policies(&store, &key, "?order=sideways").await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["code"], "invalid_order");
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
}
