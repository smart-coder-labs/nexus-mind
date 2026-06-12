# Spec — Policy Engine (MVP)

> **Change**: `policy-engine`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-06-11

This spec defines the contracts: data model, HTTP endpoints, permissions, evaluation semantics, and error envelope. The "how" lives in `design.md`.

---

## 1. Data Contract

### 1.1 Table `policies`

```sql
CREATE TABLE IF NOT EXISTS policies (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL REFERENCES organizations(id),
  name        TEXT NOT NULL,
  rule_type   TEXT NOT NULL CHECK(rule_type IN ('model_whitelist','budget_limit','pii_redact')),
  config      TEXT NOT NULL DEFAULT '{}',     -- JSON, shape varies by rule_type
  enabled     INTEGER NOT NULL DEFAULT 1,     -- 0 = disabled, 1 = enabled
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_policies_org ON policies(org_id, enabled);
```

**Constraints**:

- `id` is a UUIDv4 string generated server-side at create-time.
- `org_id` MUST reference an existing org (FK). A request body MUST NOT set `org_id`; the server derives it from `AuthContext.org_id`.
- `name` MUST be 1–128 chars, non-empty after trim. Names are not unique per org (deferred — clients can choose).
- `rule_type` is enforced by the `CHECK` constraint at the DB layer **and** validated at the handler layer (so the API returns 400, not 500).
- `config` MUST be valid JSON; the shape MUST match the variant for `rule_type` (see §1.2). Invalid JSON or wrong shape → 400.
- `enabled = 0` policies are persisted but never evaluated by `POST /v1/policy/check`.
- Timestamps use the same ISO-8601-with-ms format as the v9 audit hash columns (`strftime('%Y-%m-%dT%H:%M:%fZ','now')`).

### 1.2 Config shapes (per `rule_type`)

**`model_whitelist`**

```json
{ "allowed_models": ["claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022"] }
```

- `allowed_models` MUST be a non-empty array of strings.
- Matching is **exact** (case-sensitive). No glob, no prefix in MVP.

**`budget_limit`**

```json
{ "max_tokens_per_day": 50000, "max_requests_per_day": 100 }
```

- At least one of `max_tokens_per_day` or `max_requests_per_day` MUST be present.
- Both fields, when present, MUST be positive integers.
- "Per day" means UTC calendar day (`strftime('%Y-%m-%d','now')`).
- A request is denied when **either** cap is exceeded (logical OR).

**`pii_redact`**

```json
{ "patterns": ["\\d{3}-\\d{2}-\\d{4}", "[A-Z]{2}\\d{6}"] }
```

- `patterns` MUST be a non-empty array of strings.
- Each pattern MUST compile as a Rust `regex::Regex` and MUST be ≤256 chars.
- A request is denied if the optional `prompt_preview` matches **any** pattern.
- When `prompt_preview` is absent, this rule is a no-op (does not contribute a violation).

### 1.3 Rust models

Declared in `apps/backend/src/models/types.rs`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Policy {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub rule_type: String,       // "model_whitelist" | "budget_limit" | "pii_redact"
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "rule_type", content = "config", rename_all = "snake_case")]
pub enum PolicyConfig {
    ModelWhitelist { allowed_models: Vec<String> },
    BudgetLimit {
        #[serde(default)] max_tokens_per_day: Option<i64>,
        #[serde(default)] max_requests_per_day: Option<i64>,
    },
    PiiRedact { patterns: Vec<String> },
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

Notes:

- `Policy` carries `config` as raw `serde_json::Value` for storage / list responses. Handlers decode into `PolicyConfig` only when needed (create, update, evaluate). This keeps list endpoints zero-allocation in the happy path.
- `PolicyConfig` uses an internally-tagged variant for **create/update** request bodies — clients send `{ "rule_type": "model_whitelist", "config": { "allowed_models": [...] } }` and serde validates the variant + shape in one step.

---

## 2. HTTP Contracts

All endpoints live under the existing `protected` router → they require a valid `Authorization: Bearer <api_key>` header, run through `auth` then `rate_limit` middleware, and inject `Extension<AuthContext>` into handlers.

### 2.1 `GET /v1/policies`

**Permission**: `policy:read`.

**Response 200**:

```json
{
  "policies": [
    {
      "id": "p_5f3b...",
      "org_id": "org_acme",
      "name": "Allow only Claude Sonnet",
      "rule_type": "model_whitelist",
      "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] },
      "enabled": true,
      "created_at": "2026-06-11T18:00:00.000Z",
      "updated_at": "2026-06-11T18:00:00.000Z"
    }
  ]
}
```

- Lists **all** policies for the caller's org (enabled and disabled).
- Order: `created_at DESC`.
- No pagination in MVP; orgs are not expected to have >100 policies.

### 2.2 `POST /v1/policies`

**Permission**: `policy:write`.

**Request body**:

```json
{
  "name": "Allow only Claude Sonnet",
  "rule_type": "model_whitelist",
  "config": { "allowed_models": ["claude-3-5-sonnet-20241022"] },
  "enabled": true
}
```

- `name` required (1–128 chars after trim).
- `rule_type` required, must be one of the three enum values.
- `config` required and must match the variant shape (§1.2).
- `enabled` optional, defaults to `true`.

**Response 201**: full `Policy` object.

**Errors**:

- `400 invalid_rule_type` — `rule_type` not in the enum.
- `400 invalid_config` — JSON shape does not match the variant (e.g., empty `allowed_models`).
- `400 invalid_name` — empty or >128 chars.
- `403 forbidden` — caller lacks `policy:write`.

### 2.3 `PATCH /v1/policies/:id`

**Permission**: `policy:write`.

**Request body** (all fields optional):

```json
{
  "name": "Updated name",
  "config": { "allowed_models": ["..."] },
  "enabled": false
}
```

- `rule_type` is **immutable** post-create. Attempting to send it → 400.
- If `config` is sent, it must validate against the existing `rule_type`.
- `updated_at` is rewritten by the server on every PATCH.

**Response 200**: full updated `Policy`.

**Errors**: `400 invalid_config`, `400 immutable_rule_type`, `403 forbidden`, `404 not_found` (id not in caller's org).

### 2.4 `DELETE /v1/policies/:id`

**Permission**: `policy:write`.

**Response 204** on success.

**Errors**: `403 forbidden`, `404 not_found`.

### 2.5 `POST /v1/policy/check`

**Permission**: none beyond a valid API key (so any tool can call it).

**Request body**:

```json
{
  "model": "gpt-4",
  "prompt_tokens": 1200,
  "prompt_preview": "first 200 chars of prompt",
  "user_id": "u_abc",
  "project": "acme-webapp"
}
```

- `model` required.
- All other fields optional. `prompt_tokens` is informational in MVP (budget enforcement reads `audit_logs`, not the request).
- `prompt_preview`, when present, is matched against every enabled `pii_redact` policy.

**Response 200**:

```json
{
  "allowed": false,
  "violations": [
    {
      "policy_id": "p_5f3b...",
      "policy_name": "Allow only Claude Sonnet",
      "rule_type": "model_whitelist",
      "reason": "Model 'gpt-4' is not in the allowed list"
    }
  ]
}
```

- `allowed = (violations.is_empty())`.
- The endpoint always returns **HTTP 200**. A "denied" decision is a body field, not an HTTP status. This matches the API_SPEC oracle pattern.
- When no policies exist for the org, `allowed: true, violations: []`.

**Errors**: `401 unauthorized` (no/invalid key), `400 invalid_request` (missing `model`).

---

## 3. Permissions

Extend `get_role_permissions` (location: see `design.md` §4) so that:

| Role | Existing permissions | Added |
|---|---|---|
| `admin` | (all existing) | `policy:read`, `policy:write` |
| `member` | (existing memory perms) | `policy:read` |
| `viewer` | (existing) | — none — |

The check itself uses the same `require_permission(&ctx, "policy:read")` helper that the rest of the codebase already calls before each handler does real work. No middleware-level change.

`POST /v1/policy/check` does NOT call `require_permission` — any authenticated key may invoke it.

---

## 4. Evaluation Semantics

The pure function:

```rust
pub fn evaluate(
    policies: &[Policy],
    req: &PolicyCheckRequest,
    daily: DailyStats,         // { requests_today: i64, tokens_today: i64 }
) -> PolicyCheckResponse
```

Rules:

1. Iterate `policies` in `created_at ASC` order (stable, deterministic).
2. Skip any policy where `enabled == false`. (The SQL query in §1 of design.md already filters these out, but the pure function defends in depth.)
3. For each remaining policy, decode `policy.config` into `PolicyConfig` matching `policy.rule_type`. If decode fails (corrupt row), **skip silently** and emit a `tracing::warn!`. A corrupt policy MUST NOT block traffic.
4. Apply the rule:
   - **`model_whitelist`**: violation iff `req.model` not in `allowed_models`. Reason: `"Model '{model}' is not in the allowed list"`.
   - **`budget_limit`**:
     - If `max_requests_per_day` set and `daily.requests_today >= max_requests_per_day` → violation. Reason: `"Daily request cap ({max}) reached"`.
     - Else if `max_tokens_per_day` set and `daily.tokens_today >= max_tokens_per_day` → violation. Reason: `"Daily token cap ({max}) reached (used: {used})"`.
     - Only one violation per `budget_limit` policy (requests cap takes precedence).
   - **`pii_redact`**: if `req.prompt_preview.is_none()`, skip. Else compile each pattern (skip patterns that fail to compile, warn-log) and check `regex.is_match(prompt)`. First matching pattern → one violation. Reason: `"Prompt matches PII pattern: {pattern_truncated_to_40_chars}"`.
5. Collect every violation into `violations[]`.
6. `allowed = violations.is_empty()`.

The function is **pure** — no I/O, no logging at error level. The handler does the I/O (DB reads) and pre-computes `DailyStats` before calling `evaluate`.

---

## 5. Error Envelope

All non-2xx responses follow the existing `ApiError` shape (already defined in `models/types.rs`):

```json
{ "error": "human-readable message", "code": "machine_readable_code" }
```

Codes introduced by this change:

- `invalid_rule_type`
- `invalid_config`
- `invalid_name`
- `immutable_rule_type`
- `policy_not_found`
- `invalid_request` (generic 400 for missing required fields on `/v1/policy/check`)

`forbidden` and `unauthorized` reuse the existing codes.

---

## 6. Non-Functional Requirements (this change only)

- `POST /v1/policy/check` p95 latency **<50 ms** at 50 policies/org (PRD requirement).
- Migration `run_v10` is **idempotent**: running on a v10 DB MUST be a no-op (asserted in test).
- Org isolation: queries MUST filter by `org_id`. A user from `org_A` MUST NOT see, modify, delete, or be affected by policies of `org_B` (asserted in test).
- All new endpoints are covered by at least one integration-level test that exercises auth + permission gate + DB.

---

## 7. Acceptance Criteria

- [ ] `cargo test` passes including new migration, models, evaluation, and handler tests.
- [ ] `PRAGMA user_version` is `10` after running migrations on a fresh DB.
- [ ] `run_all` is idempotent (test already exists; new assertion confirms `policies` table is present).
- [ ] `policies` table rejects `rule_type='banana'` at the DB layer (CHECK constraint test).
- [ ] `POST /v1/policies` with an `admin` key creates a policy; with a `member` key returns 403.
- [ ] `POST /v1/policy/check` with `model='gpt-4'` and one `model_whitelist` policy listing only `claude-3-5-sonnet-20241022` returns `allowed: false` with exactly one violation.
- [ ] `POST /v1/policy/check` with no policies in the org returns `{ allowed: true, violations: [] }`.
- [ ] `PATCH /v1/policies/:id` rejects `rule_type` changes with `400 immutable_rule_type`.
- [ ] An `org_B` admin cannot `GET`, `PATCH`, or `DELETE` an `org_A` policy (all return 404).

---

*End of spec.md*
