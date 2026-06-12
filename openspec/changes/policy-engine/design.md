# Design — Policy Engine (MVP)

> **Change**: `policy-engine`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-06-11

This document is the implementation blueprint. It assumes the reader has read `proposal.md` (the "why") and `spec.md` (the "what"). It describes the "how": file layout, SQL, Rust signatures, the evaluation algorithm step-by-step, and the wiring touch points.

---

## 1. Architecture overview

```
                    ┌─────────────────────────────────┐
   Tool / Plugin → │  POST /v1/policy/check          │
                    │  (auth → rate_limit → handler)  │
                    └────────────┬────────────────────┘
                                 │
                                 ▼
                  ┌────────────────────────────────────┐
                  │  api::policy::check_handler        │
                  │  1. Load enabled policies (1 SQL)  │
                  │  2. Load DailyStats if budget rule │
                  │     is present (1 SQL, conditional)│
                  │  3. evaluate(...)  ← pure          │
                  │  4. Return PolicyCheckResponse     │
                  └────────────────────────────────────┘
                                 │
                                 ▼
                  ┌────────────────────────────────────┐
                  │  pure fn evaluate(policies, req,   │
                  │                   daily) -> Resp   │
                  │  no I/O, no panics                 │
                  └────────────────────────────────────┘

   Admin UI    → CRUD on /v1/policies (org-scoped, permission-gated)
```

Boundaries: the **handler** owns I/O and HTTP framing; the **evaluator** is a pure function so it can be unit-tested with synthetic inputs. The **store layer** (queries.rs) owns SQL.

---

## 2. File-by-file change list

| File | Action | Reason |
|---|---|---|
| `apps/backend/src/db/migrations.rs` | Edit — add `run_v10` + call from `run_all` + tests | Schema for `policies` table |
| `apps/backend/src/models/types.rs` | Edit — add `Policy`, `PolicyConfig`, `PolicyCheckRequest`, `PolicyCheckResponse`, `PolicyViolation` | Domain types |
| `apps/backend/src/db/queries.rs` | Edit — add `policies_*` query helpers (list, get, insert, update, delete, daily_stats) | Persistence |
| `apps/backend/src/api/policy.rs` | **New** — handlers + `evaluate()` + `DailyStats` | The actual feature |
| `apps/backend/src/api/mod.rs` | Edit — add `pub mod policy;` | Module registration |
| `apps/backend/src/api/router.rs` | Edit — add 5 routes inside `protected` Router; import `policy` | Wiring |
| `apps/backend/src/auth/permissions.rs` (or wherever `get_role_permissions` lives) | Edit — add `policy:read` to `member`+`admin`, `policy:write` to `admin` | RBAC |
| `apps/backend/Cargo.toml` | Edit — add `regex = "1"` (only if not already present) | PII matching |
| `openspec/changes/policy-engine/proposal.md` | exists | — |
| `openspec/changes/policy-engine/spec.md` | exists | — |
| `openspec/changes/policy-engine/design.md` | this file | — |

> The exact location of `get_role_permissions` should be confirmed during apply (likely `src/auth/permissions.rs` or `src/api/middleware.rs`). If it does not exist yet as a named helper, this change introduces it.

---

## 3. Migration `run_v10`

Add to `src/db/migrations.rs` directly after `run_v9`:

```rust
/// Migration v10: adds policies table + idx_policies_org index.
/// Idempotent — guarded by PRAGMA user_version < 10.
pub fn run_v10(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 10 {
        return Ok(());
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS policies (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id),
            name        TEXT NOT NULL,
            rule_type   TEXT NOT NULL CHECK(rule_type IN ('model_whitelist','budget_limit','pii_redact')),
            config      TEXT NOT NULL DEFAULT '{}',
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );

        CREATE INDEX IF NOT EXISTS idx_policies_org ON policies(org_id, enabled);

        PRAGMA user_version = 10;
        ",
    )?;
    Ok(())
}
```

And update `run_all`:

```rust
pub fn run_all(conn: &Connection) -> Result<()> {
    run_v1(conn)?;
    // ...
    run_v9(conn)?;
    run_v10(conn)?;
    Ok(())
}
```

Update the existing test `run_all_sets_user_version_to_9` → `run_all_sets_user_version_to_10` and bump the assertion to `10`. Add three new tests mirroring the v9 pattern:

- `run_v10_creates_policies_table`
- `run_v10_creates_org_index`
- `run_v10_is_idempotent`
- `run_v10_rejects_invalid_rule_type` — inserts with `rule_type='banana'` and asserts the CHECK fires.

Helper `in_memory_db_v9()` mirrors the existing `in_memory_db_v8()` pattern to test `run_v10` in isolation.

---

## 4. Rust models (in `models/types.rs`)

Append after the existing `Project`/`ProjectMember` block (around line 311):

```rust
// ── Policy types ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Policy {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub rule_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Internally-tagged enum used for create/update request bodies AND for the
/// pure evaluator. Tag lives in `rule_type`, payload in `config`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "rule_type", content = "config", rename_all = "snake_case")]
pub enum PolicyConfig {
    ModelWhitelist {
        allowed_models: Vec<String>,
    },
    BudgetLimit {
        #[serde(default)]
        max_tokens_per_day: Option<i64>,
        #[serde(default)]
        max_requests_per_day: Option<i64>,
    },
    PiiRedact {
        patterns: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreatePolicyRequest {
    pub name: String,
    #[serde(flatten)]
    pub config: PolicyConfig,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool { true }

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    pub config: Option<serde_json::Value>, // validated against existing rule_type
    pub enabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PolicyCheckRequest {
    pub model: String,
    #[serde(default)] pub prompt_tokens: Option<i64>,
    #[serde(default)] pub prompt_preview: Option<String>,
    #[serde(default)] pub user_id: Option<String>,
    #[serde(default)] pub project: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PolicyViolation {
    pub policy_id: String,
    pub policy_name: String,
    pub rule_type: String,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PolicyCheckResponse {
    pub allowed: bool,
    pub violations: Vec<PolicyViolation>,
}
```

Rationale for `CreatePolicyRequest` using `#[serde(flatten)]`: clients send a flat body `{ name, rule_type, config, enabled }` matching the API_SPEC shape. Flattening the `PolicyConfig` enum into the request hoists `rule_type`/`config` to the top level while reusing the same enum for validation.

---

## 5. SQL queries (add to `db/queries.rs`)

All queries are tenant-scoped: every WHERE clause filters by `org_id` from the caller's `AuthContext`. None of these queries should ever be called without `org_id`.

### 5.1 List policies for an org

```sql
SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at
FROM policies
WHERE org_id = ?1
ORDER BY created_at DESC
```

```rust
pub fn list_policies(conn: &Connection, org_id: &str) -> Result<Vec<Policy>>
```

### 5.2 List **enabled** policies (used by `/policy/check`)

```sql
SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at
FROM policies
WHERE org_id = ?1 AND enabled = 1
ORDER BY created_at ASC
```

```rust
pub fn list_enabled_policies(conn: &Connection, org_id: &str) -> Result<Vec<Policy>>
```

Uses the `idx_policies_org(org_id, enabled)` covering index.

### 5.3 Get one

```sql
SELECT id, org_id, name, rule_type, config, enabled, created_at, updated_at
FROM policies
WHERE id = ?1 AND org_id = ?2
```

```rust
pub fn get_policy(conn: &Connection, id: &str, org_id: &str) -> Result<Option<Policy>>
```

Returning `None` when the row exists but belongs to another org makes the handler return 404 (not 403) — defends against id-enumeration.

### 5.4 Insert

```sql
INSERT INTO policies (id, org_id, name, rule_type, config, enabled, created_at, updated_at)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
```

```rust
pub fn insert_policy(
    conn: &Connection,
    id: &str,
    org_id: &str,
    name: &str,
    rule_type: &str,
    config_json: &str,
    enabled: bool,
) -> Result<Policy>
```

`now` is computed by the handler in the same ISO-8601-ms format SQLite would emit, and passed as `?7` for both timestamps — guarantees `created_at == updated_at` on create.

### 5.5 Update

```sql
UPDATE policies
SET name = COALESCE(?3, name),
    config = COALESCE(?4, config),
    enabled = COALESCE(?5, enabled),
    updated_at = ?6
WHERE id = ?1 AND org_id = ?2
```

```rust
pub fn update_policy(
    conn: &Connection,
    id: &str,
    org_id: &str,
    name: Option<&str>,
    config_json: Option<&str>,
    enabled: Option<bool>,
    now: &str,
) -> Result<Option<Policy>>
```

Returns `None` if 0 rows affected (id not found in this org). `rule_type` is never updated by this query — the handler rejects the request before reaching SQL.

### 5.6 Delete

```sql
DELETE FROM policies WHERE id = ?1 AND org_id = ?2
```

```rust
pub fn delete_policy(conn: &Connection, id: &str, org_id: &str) -> Result<bool>  // true if a row was deleted
```

### 5.7 Daily stats (for `budget_limit` evaluation)

```sql
SELECT
  COUNT(*) AS requests_today,
  COALESCE(SUM(CAST(json_extract(metadata, '$.tokens_total') AS INTEGER)), 0) AS tokens_today
FROM audit_logs
WHERE org_id = ?1
  AND timestamp >= strftime('%Y-%m-%dT00:00:00.000Z','now')
```

```rust
pub struct DailyStats {
    pub requests_today: i64,
    pub tokens_today: i64,
}

pub fn fetch_daily_stats(conn: &Connection, org_id: &str) -> Result<DailyStats>
```

Notes:

- Uses `idx_audit_logs_org_ts` from `run_v9` for the range scan.
- `json_extract(metadata, '$.tokens_total')` is the canonical key declared by this spec; pre-existing audit rows without that key contribute `NULL` (cast to 0 by `COALESCE`).
- The query is only invoked when at least one `budget_limit` policy exists in the org's enabled set. Otherwise the handler passes `DailyStats { 0, 0 }` and saves the round-trip.

---

## 6. New module `api/policy.rs`

### 6.1 Skeleton

```rust
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde_json::json;
use uuid::Uuid;

use crate::api::middleware::require_permission;
use crate::db::queries;
use crate::models::types::{
    ApiError, AuthContext, CreatePolicyRequest, Policy, PolicyCheckRequest,
    PolicyCheckResponse, PolicyConfig, PolicyViolation, UpdatePolicyRequest,
};
use crate::store::sqlite::SqliteStore;

pub async fn list(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    require_permission(&ctx, "policy:read")?;
    let conn = store.conn();
    let conn = conn.lock().unwrap();
    let policies = queries::list_policies(&conn, &ctx.org_id)
        .map_err(internal_error)?;
    Ok(Json(json!({ "policies": policies })))
}

pub async fn create(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<(StatusCode, Json<Policy>), (StatusCode, Json<ApiError>)> {
    require_permission(&ctx, "policy:write")?;
    validate_create(&req)?;

    let id = format!("p_{}", Uuid::new_v4().simple());
    let now = iso8601_ms_now();
    let (rule_type, config_value) = split_config(&req.config);
    let config_json = serde_json::to_string(&config_value).unwrap();

    let conn = store.conn();
    let conn = conn.lock().unwrap();
    let policy = queries::insert_policy(
        &conn, &id, &ctx.org_id, req.name.trim(),
        rule_type, &config_json, req.enabled,
    ).map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(policy)))
}

pub async fn update(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePolicyRequest>,
) -> Result<Json<Policy>, (StatusCode, Json<ApiError>)> {
    require_permission(&ctx, "policy:write")?;

    let conn = store.conn();
    let conn = conn.lock().unwrap();

    // 1. Load existing to know rule_type for config validation
    let existing = queries::get_policy(&conn, &id, &ctx.org_id)
        .map_err(internal_error)?
        .ok_or_else(|| not_found("policy_not_found"))?;

    // 2. Reject rule_type changes (immutable)
    if let Some(cfg) = &req.config {
        if let Some(rt) = cfg.get("rule_type") {
            if rt.as_str() != Some(&existing.rule_type) {
                return Err(bad_request("immutable_rule_type",
                    "rule_type cannot be changed after creation"));
            }
        }
        validate_config_shape(&existing.rule_type, cfg)?;
    }

    if let Some(name) = &req.name {
        validate_name(name)?;
    }

    let config_json = req.config.as_ref().map(|v| serde_json::to_string(v).unwrap());
    let updated = queries::update_policy(
        &conn, &id, &ctx.org_id,
        req.name.as_deref().map(str::trim),
        config_json.as_deref(),
        req.enabled,
        &iso8601_ms_now(),
    ).map_err(internal_error)?
     .ok_or_else(|| not_found("policy_not_found"))?;

    Ok(Json(updated))
}

pub async fn delete(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_permission(&ctx, "policy:write")?;
    let conn = store.conn();
    let conn = conn.lock().unwrap();
    let deleted = queries::delete_policy(&conn, &id, &ctx.org_id)
        .map_err(internal_error)?;
    if !deleted {
        return Err(not_found("policy_not_found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn check(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<PolicyCheckRequest>,
) -> Result<Json<PolicyCheckResponse>, (StatusCode, Json<ApiError>)> {
    if req.model.trim().is_empty() {
        return Err(bad_request("invalid_request", "model is required"));
    }

    let conn = store.conn();
    let conn = conn.lock().unwrap();

    let policies = queries::list_enabled_policies(&conn, &ctx.org_id)
        .map_err(internal_error)?;

    let needs_budget = policies.iter().any(|p| p.rule_type == "budget_limit");
    let daily = if needs_budget {
        queries::fetch_daily_stats(&conn, &ctx.org_id).map_err(internal_error)?
    } else {
        DailyStats::default()
    };

    drop(conn); // release lock before pure work

    Ok(Json(evaluate(&policies, &req, daily)))
}
```

### 6.2 Pure evaluator

```rust
#[derive(Default, Clone, Copy, Debug)]
pub struct DailyStats {
    pub requests_today: i64,
    pub tokens_today: i64,
}

pub fn evaluate(
    policies: &[Policy],
    req: &PolicyCheckRequest,
    daily: DailyStats,
) -> PolicyCheckResponse {
    let mut violations = Vec::new();

    for p in policies {
        if !p.enabled {
            continue;
        }

        match p.rule_type.as_str() {
            "model_whitelist" => {
                let allowed: Vec<String> = p.config
                    .get("allowed_models")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                if !allowed.iter().any(|m| m == &req.model) {
                    violations.push(PolicyViolation {
                        policy_id: p.id.clone(),
                        policy_name: p.name.clone(),
                        rule_type: p.rule_type.clone(),
                        reason: format!("Model '{}' is not in the allowed list", req.model),
                    });
                }
            }

            "budget_limit" => {
                let max_req = p.config.get("max_requests_per_day").and_then(|v| v.as_i64());
                let max_tok = p.config.get("max_tokens_per_day").and_then(|v| v.as_i64());

                if let Some(max) = max_req {
                    if daily.requests_today >= max {
                        violations.push(PolicyViolation {
                            policy_id: p.id.clone(),
                            policy_name: p.name.clone(),
                            rule_type: p.rule_type.clone(),
                            reason: format!("Daily request cap ({}) reached", max),
                        });
                        continue; // requests cap takes precedence
                    }
                }
                if let Some(max) = max_tok {
                    if daily.tokens_today >= max {
                        violations.push(PolicyViolation {
                            policy_id: p.id.clone(),
                            policy_name: p.name.clone(),
                            rule_type: p.rule_type.clone(),
                            reason: format!(
                                "Daily token cap ({}) reached (used: {})",
                                max, daily.tokens_today
                            ),
                        });
                    }
                }
            }

            "pii_redact" => {
                let Some(prompt) = req.prompt_preview.as_deref() else { continue };
                let patterns: Vec<String> = p.config
                    .get("patterns")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                for pat in &patterns {
                    match regex::Regex::new(pat) {
                        Ok(re) if re.is_match(prompt) => {
                            let trunc: String = pat.chars().take(40).collect();
                            violations.push(PolicyViolation {
                                policy_id: p.id.clone(),
                                policy_name: p.name.clone(),
                                rule_type: p.rule_type.clone(),
                                reason: format!("Prompt matches PII pattern: {}", trunc),
                            });
                            break; // one violation per policy
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(policy_id = %p.id, pattern = %pat, error = %e,
                                "skipping malformed PII pattern");
                        }
                    }
                }
            }

            other => {
                // Unknown rule_type (shouldn't happen given CHECK constraint).
                tracing::warn!(policy_id = %p.id, rule_type = %other,
                    "unknown rule_type — skipping");
            }
        }
    }

    PolicyCheckResponse {
        allowed: violations.is_empty(),
        violations,
    }
}
```

### 6.3 Helpers (same file)

```rust
fn validate_create(req: &CreatePolicyRequest) -> Result<(), (StatusCode, Json<ApiError>)> {
    validate_name(&req.name)?;
    validate_policy_config(&req.config)?;
    Ok(())
}

fn validate_name(name: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err(bad_request("invalid_name", "name must be 1..=128 chars after trim"));
    }
    Ok(())
}

fn validate_policy_config(cfg: &PolicyConfig) -> Result<(), (StatusCode, Json<ApiError>)> {
    match cfg {
        PolicyConfig::ModelWhitelist { allowed_models } => {
            if allowed_models.is_empty() {
                return Err(bad_request("invalid_config", "allowed_models must be non-empty"));
            }
        }
        PolicyConfig::BudgetLimit { max_tokens_per_day, max_requests_per_day } => {
            if max_tokens_per_day.is_none() && max_requests_per_day.is_none() {
                return Err(bad_request("invalid_config",
                    "budget_limit requires max_tokens_per_day or max_requests_per_day"));
            }
            if let Some(v) = max_tokens_per_day { if *v <= 0 {
                return Err(bad_request("invalid_config", "max_tokens_per_day must be > 0"));
            }}
            if let Some(v) = max_requests_per_day { if *v <= 0 {
                return Err(bad_request("invalid_config", "max_requests_per_day must be > 0"));
            }}
        }
        PolicyConfig::PiiRedact { patterns } => {
            if patterns.is_empty() {
                return Err(bad_request("invalid_config", "patterns must be non-empty"));
            }
            for p in patterns {
                if p.len() > 256 {
                    return Err(bad_request("invalid_config", "pattern exceeds 256 chars"));
                }
                if regex::Regex::new(p).is_err() {
                    return Err(bad_request("invalid_config",
                        &format!("invalid regex: {}", p)));
                }
            }
        }
    }
    Ok(())
}

fn validate_config_shape(rule_type: &str, raw: &serde_json::Value)
    -> Result<(), (StatusCode, Json<ApiError>)>
{
    // Re-build a PolicyConfig-shaped value and run it through the same validator.
    let v = serde_json::json!({ "rule_type": rule_type, "config": raw });
    let cfg: PolicyConfig = serde_json::from_value(v)
        .map_err(|e| bad_request("invalid_config", &e.to_string()))?;
    validate_policy_config(&cfg)
}

fn split_config(cfg: &PolicyConfig) -> (&'static str, serde_json::Value) {
    match cfg {
        PolicyConfig::ModelWhitelist { allowed_models } =>
            ("model_whitelist", serde_json::json!({ "allowed_models": allowed_models })),
        PolicyConfig::BudgetLimit { max_tokens_per_day, max_requests_per_day } => {
            let mut o = serde_json::Map::new();
            if let Some(v) = max_tokens_per_day { o.insert("max_tokens_per_day".into(), (*v).into()); }
            if let Some(v) = max_requests_per_day { o.insert("max_requests_per_day".into(), (*v).into()); }
            ("budget_limit", serde_json::Value::Object(o))
        }
        PolicyConfig::PiiRedact { patterns } =>
            ("pii_redact", serde_json::json!({ "patterns": patterns })),
    }
}

fn iso8601_ms_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn bad_request(code: &str, msg: &str) -> (StatusCode, Json<ApiError>) {
    (StatusCode::BAD_REQUEST, Json(ApiError { error: msg.into(), code: code.into() }))
}

fn not_found(code: &str) -> (StatusCode, Json<ApiError>) {
    (StatusCode::NOT_FOUND, Json(ApiError { error: "not found".into(), code: code.into() }))
}

fn internal_error<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ApiError>) {
    tracing::error!(error = %e, "policy handler error");
    (StatusCode::INTERNAL_SERVER_ERROR,
     Json(ApiError { error: "internal error".into(), code: "internal".into() }))
}
```

> The exact mutex/`State` shape (`SqliteStore::conn()` returning `Arc<Mutex<Connection>>` vs `Arc<Connection>`) must mirror what other handlers (e.g. `memory.rs`, `audit.rs`) do. The apply phase confirms the pattern and reuses it verbatim. The pseudocode above uses `.lock().unwrap()` for illustration only.

---

## 7. Router wiring

In `apps/backend/src/api/router.rs`:

1. Add `policy` to the import line (line 11):

   ```rust
   use crate::api::{admin, audit, auth, context, health, internal, memory, middleware as auth_mw, policy, rate_limit, sessions, users};
   ```

2. Add five routes inside the `protected` Router, after the projects routes (line 77) and before the context route:

   ```rust
   .route("/v1/policies", get(policy::list).post(policy::create))
   .route("/v1/policies/:id", patch(policy::update).delete(policy::delete))
   .route("/v1/policy/check", post(policy::check))
   ```

All five sit inside `protected` so they automatically inherit:
- `auth_mw::auth` (validates API key, injects `AuthContext`)
- `rate_limit::rate_limit` (per-key throttle)

No router-level changes outside this block.

Also add `pub mod policy;` to `apps/backend/src/api/mod.rs` (alphabetical position).

---

## 8. RBAC additions

Edit `get_role_permissions` (likely `src/auth/permissions.rs`; confirm at apply time):

```rust
pub fn get_role_permissions(role: &UserRole) -> Vec<&'static str> {
    match role {
        UserRole::Standard(Role::Admin) => vec![
            // ...existing admin perms...
            "policy:read",
            "policy:write",
        ],
        UserRole::Standard(Role::Member) => vec![
            // ...existing member perms...
            "policy:read",
        ],
        UserRole::Standard(Role::Viewer) => vec![
            // ...existing viewer perms (no policy:* added)...
        ],
        UserRole::Custom(name) => {
            // custom roles already load permissions from the `roles` table;
            // admins can grant policy:* via the custom-role UI without code changes.
            load_custom_role_permissions(name)
        }
    }
}
```

If `get_role_permissions` doesn't exist as a named helper today, the apply phase introduces it (extracted from wherever the inlined permission check currently lives).

---

## 9. Tests

### 9.1 Migration (`db/migrations.rs`)

```rust
fn in_memory_db_v9() -> Connection { /* run v1..v9 */ }

#[test] fn run_v10_creates_policies_table() { /* assert table + columns */ }

#[test] fn run_v10_creates_org_index() {
    let conn = in_memory_db_v9();
    run_v10(&conn).unwrap();
    assert!(index_exists(&conn, "policies", "idx_policies_org"));
}

#[test] fn run_v10_is_idempotent() { /* run twice, no panic, user_version == 10 */ }

#[test] fn run_v10_rejects_invalid_rule_type() {
    let conn = in_memory_db_v9();
    run_v10(&conn).unwrap();
    seed_org(&conn, "org1");
    let bad = conn.execute(
        "INSERT INTO policies (id, org_id, name, rule_type, config) VALUES ('p1','org1','x','banana','{}')",
        [],
    );
    assert!(bad.is_err(), "CHECK constraint must reject unknown rule_type");
}
```

Update `run_all_sets_user_version_to_9` → `run_all_sets_user_version_to_10` (rename + bump assertion).

### 9.2 Evaluator (`api/policy.rs`)

Pure-function tests with synthetic inputs:

```rust
#[test] fn evaluate_no_policies_allows_everything() { /* empty slice, allowed: true */ }

#[test] fn model_whitelist_denies_unlisted_model() { /* gpt-4 vs ["claude-..."] → 1 violation */ }

#[test] fn model_whitelist_allows_listed_model() { /* exact match → allowed */ }

#[test] fn budget_limit_request_cap_triggers() { /* daily.requests >= max → violation */ }

#[test] fn budget_limit_token_cap_triggers_when_no_request_cap() { /* tokens >= max → violation */ }

#[test] fn budget_limit_request_cap_takes_precedence() { /* both caps exceeded → 1 violation, requests cap */ }

#[test] fn pii_redact_matches_pattern() { /* prompt="123-45-6789", pattern=SSN → violation */ }

#[test] fn pii_redact_skips_when_no_prompt() { /* prompt_preview=None → no violation */ }

#[test] fn pii_redact_skips_malformed_pattern() { /* invalid regex → no panic, no violation */ }

#[test] fn disabled_policy_is_skipped() { /* enabled=false, model mismatch → allowed */ }

#[test] fn multiple_violations_all_returned() { /* 2 active policies, both fail → 2 violations */ }
```

### 9.3 Handler-level (integration)

In `apps/backend/tests/policy_api.rs` (new file, following the existing tests layout):

- `create_policy_as_admin_returns_201`
- `create_policy_as_member_returns_403`
- `create_policy_with_invalid_rule_type_returns_400`
- `create_policy_with_empty_allowed_models_returns_400`
- `list_policies_returns_only_caller_org` (cross-org isolation)
- `update_policy_rejects_rule_type_change` (returns `400 immutable_rule_type`)
- `delete_policy_in_other_org_returns_404`
- `check_with_no_policies_allows`
- `check_with_model_whitelist_denies_unknown_model`
- `check_returns_http_200_even_when_denied` (oracle pattern)

---

## 10. Performance & operational notes

- **Hot path**: `POST /v1/policy/check`. Two queries max (list_enabled_policies + optional daily_stats), both index-backed. Target: <50 ms p95 with 50 policies.
- **Regex compilation per request** is acceptable in MVP. If a future profile shows it dominates, cache compiled regexes in a per-process `OnceCell<HashMap<policy_id, Vec<Regex>>>` invalidated on PATCH/DELETE.
- **No caching of policies list**: SQLite read is fast, and stale-cache bugs in a governance feature are worse than 1 ms of latency. Revisit only if scale demands.
- **Logging**: handler logs at `info` on create/update/delete (with `policy_id`, `org_id`, `rule_type`), and at `warn` on malformed regex / unknown rule_type during evaluation. `policy/check` does **not** log per-request (would blow audit-log volume); audit is the source of truth.
- **Audit integration**: this change does NOT modify `audit_logs` writes. A follow-up change ("policy decisions in audit") will append `policy_decisions` to the audit metadata. For MVP, callers log policy outcomes themselves via the existing `POST /v1/audit/log`.

---

## 11. Rollout

This is a single, contained PR:

1. Migration `run_v10` (additive, idempotent — safe to deploy with rolling restart).
2. New module + types + queries (no behavior change for existing endpoints).
3. Router wiring (5 new routes; no existing routes touched).
4. Permission additions (additive — no existing role loses access).

No feature flag needed. Rollback = revert PR + `PRAGMA user_version = 9` (only if no policies were created — otherwise drop the table manually since SQLite can't downgrade `user_version` automatically).

---

*End of design.md*
