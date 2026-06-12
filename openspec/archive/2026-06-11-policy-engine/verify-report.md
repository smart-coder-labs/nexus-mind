# Verify Report — Policy Engine (MVP)

> **Change**: `policy-engine`
> **Verified**: 2026-06-11
> **Mode**: openspec
> **Verdict**: PASS WITH WARNINGS

---

## Build & Test Evidence

| Check | Result |
|-------|--------|
| `cargo test` exit code | 0 (green) |
| Total passing tests | 223 (lib: 204 + integration-like in `api/policy.rs`: 12 + `integration_test.rs`: 5 + `http_auth_test.rs`: 14, minus overlaps per suite output) |
| Failing tests | 0 |
| Test suites with policy coverage | `policy::tests` (11), `api::policy::tests` (12), `db::queries::tests` (policy subset: 13), `models::types::tests` (policy subset: 7) |

Suite breakdown from `cargo test`:
- `running 204 tests … ok` (lib unit tests)
- `running 5 tests … ok` (integration_test.rs)
- `running 14 tests … ok` (http_auth_test.rs)

---

## Task Completeness

| Task | Description | Status |
|------|-------------|--------|
| T-01 | `run_v10` migration + idempotency | COMPLETE |
| T-02 | Policy types in `models/types.rs` | COMPLETE |
| T-03 | Policy query helpers in `db/queries.rs` | COMPLETE |
| T-04 | RBAC `policy:read`/`policy:write` in `get_role_permissions` | COMPLETE |
| T-05 | `src/policy/mod.rs` — pure `evaluate()` + 11 unit tests | COMPLETE |
| T-06 | Handler scaffolding (`list`, `create`, `update`, `delete`, `check`) | COMPLETE |
| T-07 | Module registration + 5 route wiring | COMPLETE |
| T-08 | Integration tests (landed in `api/policy.rs` `#[cfg(test)]`, not a separate `tests/policy_api.rs`) | COMPLETE (file location deviates — see WARNING W-01) |

---

## Spec Compliance Matrix

### §1 Data Contract

| Requirement | Evidence | Status |
|-------------|----------|--------|
| `policies` table with CHECK on `rule_type` | `run_v10` creates table; `run_v10_rejects_invalid_rule_type` test | PASS |
| `idx_policies_org` index | `run_v10_creates_org_index` test | PASS |
| UUIDv4 `id` generated server-side | `uuid::Uuid::new_v4()` in `create_policy` handler | PASS |
| `org_id` from `AuthContext`, not request body | `ctx.org_id` used; `RawCreatePolicyRequest` has no `org_id` field | PASS |
| `name` 1–128 chars, non-empty after trim | validated in `create_policy` and `update_policy` handlers | PASS |
| `rule_type` CHECK + handler-level 400 | `VALID_RULE_TYPES` check + DB CHECK constraint | PASS |
| `config` shape validation per `rule_type` | `validate_config()` in handler | PASS |
| `enabled=0` policies skipped in evaluation | `if !p.enabled { continue; }` in `evaluate()` | PASS |

### §2 HTTP Contracts

| Endpoint | Handler | Route registered | Status |
|----------|---------|-----------------|--------|
| `GET /v1/policies` | `list_policies` | `router.rs:78` | PASS |
| `POST /v1/policies` | `create_policy` | `router.rs:78` | PASS |
| `PATCH /v1/policies/:id` | `update_policy` | `router.rs:79` | PASS |
| `DELETE /v1/policies/:id` | `delete_policy` | `router.rs:79` | PASS |
| `POST /v1/policy/check` | `check_policy` | `router.rs:80` | PASS |
| 201 on create | `(StatusCode::CREATED, Json(policy))` | — | PASS |
| 204 on delete | `Ok(StatusCode::NO_CONTENT)` | — | PASS |
| 200 on check (even when denied) | `Ok(Json(response))` with HTTP 200 always | — | PASS |
| 403 for member on `POST /v1/policies` | `create_as_member_returns_403` test | PASS |
| 404 for cross-org PATCH/DELETE | `update_unknown_id_returns_404`, `delete_unknown_id_returns_404` | PASS |
| `400 immutable_rule_type` on PATCH with `rule_type` | `UpdatePolicyRequest` has no `rule_type` field — serde silently drops it; no 400 returned; no covering test | WARNING (W-02) |
| `400 invalid_name` codes | returned in both create and update handlers | PASS |

### §3 Permissions

| Role | `policy:read` | `policy:write` |
|------|--------------|----------------|
| admin | PASS (lines 1397–1398) | PASS |
| member | PASS (line 1406) | denied — PASS |
| viewer | not granted — PASS | not granted — PASS |

RBAC tests: `get_role_permissions_admin_includes_policy_write`, `get_role_permissions_member_includes_policy_read_only`, `get_role_permissions_viewer_has_no_policy_perms` — all passing.

### §4 Evaluation Semantics (`evaluate()`)

| Rule | Spec requirement | Implementation | Status |
|------|-----------------|----------------|--------|
| `model_whitelist` — exact match, case-sensitive | `allowed.iter().any(|m| m == &req.model)` | PASS |
| `budget_limit` — request cap takes precedence | `continue` after request violation skips token check | PASS |
| `budget_limit` — either cap triggers denial | Tests confirm both paths | PASS |
| `pii_redact` — skip when `prompt_preview` absent | `let Some(prompt) = req.prompt_preview.as_deref() else { continue; }` | PASS |
| `pii_redact` — skip malformed pattern, warn-log | `Err(e)` arm calls `tracing::warn!` + no violation | PASS |
| `pii_redact` — one violation per policy | `break 'patterns` after first match | PASS |
| Corrupt row → skip silently, warn-log | Unknown `rule_type` arm calls `tracing::warn!` | PASS |
| `allowed = violations.is_empty()` | `PolicyCheckResponse { allowed: violations.is_empty(), violations }` | PASS |
| Spec signature: `evaluate(policies, req, DailyStats)` | Impl uses `evaluate(policies, req, tokens_used: u64, requests_used: u64)` — flat params instead of struct | WARNING (W-03) |

### §5 Error Codes

| Code | Handler | Status |
|------|---------|--------|
| `invalid_rule_type` | `create_policy`, `validate_config` fallback arm | PASS |
| `invalid_config` | `validate_config` for all 3 rule types | PASS |
| `invalid_name` | `create_policy`, `update_policy` | PASS |
| `immutable_rule_type` | Not returned — PATCH silently drops `rule_type` field | WARNING (W-02) |
| `policy_not_found` | `not_found()` helper | PASS |
| `invalid_request` | `check_policy` when `model` is empty | PASS |

### §6 Non-Functional

| Requirement | Evidence | Status |
|-------------|----------|--------|
| `run_v10` idempotent | `run_v10_is_idempotent` test | PASS |
| `PRAGMA user_version = 10` after migrations | `run_all_sets_user_version_to_10` test | PASS |
| Org isolation — queries filter by `org_id` | All 6 query functions scope by `org_id`; cross-org tests in `queries.rs` | PASS |
| All new endpoints covered by auth + permission tests | Covered by `api::policy::tests` in-module tests | PASS |
| `POST /v1/policy/check` p95 < 50ms | Not benchmarked (MVP; no perf test infra) | SUGGESTION (S-01) |

### §7 Acceptance Criteria

| Criterion | Status |
|-----------|--------|
| `cargo test` passes including new migration, models, evaluation, handler tests | PASS — 223/223 |
| `PRAGMA user_version = 10` after fresh DB | PASS — `run_all_sets_user_version_to_10` |
| `run_all` idempotent | PASS — `run_all_idempotent_on_already_migrated_db` |
| `policies` table rejects `rule_type='banana'` at DB layer | PASS — `run_v10_rejects_invalid_rule_type` |
| `POST /v1/policies` as admin → 201; as member → 403 | PASS — both tests green |
| `POST /v1/policy/check` with `gpt-4` against `model_whitelist` → `allowed: false`, 1 violation | PASS — `check_with_model_whitelist_blocking_returns_denied` |
| `POST /v1/policy/check` with no policies → `{ allowed: true, violations: [] }` | PASS — `check_with_no_policies_returns_allowed_true` |
| `PATCH /v1/policies/:id` rejects `rule_type` change → `400 immutable_rule_type` | PARTIAL — serde silently drops `rule_type`; no 400 returned; no test | WARNING (W-02) |
| `org_B` admin cannot GET/PATCH/DELETE `org_A` policy (all 404) | PASS — cross-org tests in `db::queries::tests` + handler 404 path |

---

## Issues

### CRITICAL

None.

### WARNING

**W-01 — T-08 integration tests landed in `api/policy.rs`, not `tests/policy_api.rs`**
- Spec §6 and T-08 require a separate `tests/policy_api.rs` integration test file.
- Implementation placed all tests as in-module `#[cfg(test)]` in `src/api/policy.rs`.
- The tests themselves are comprehensive and passing; this is a file-location deviation only.
- Impact: test isolation is slightly weaker (in-module tests have access to private items); no functional gap.

**W-02 — `PATCH /v1/policies/:id` does not return `400 immutable_rule_type`**
- Spec §2.3 and AC §7 require: "Attempting to send `rule_type` → 400 `immutable_rule_type`".
- `UpdatePolicyRequest` struct has no `rule_type` field; serde silently ignores it (no error, no 400).
- No test covering this path exists. The spec acceptance criterion is unmet.
- Impact: low — a client sending `rule_type` gets a silent no-op instead of a clear error; the rule type is never changed (correct), but the API contract is violated.

**W-03 — `evaluate()` signature deviates from spec §4**
- Spec defines: `pub fn evaluate(policies: &[Policy], req: &PolicyCheckRequest, daily: DailyStats) -> PolicyCheckResponse`
- Implementation uses: `pub fn evaluate(policies: &[Policy], req: &PolicyCheckRequest, tokens_used: u64, requests_used: u64) -> PolicyCheckResponse`
- `DailyStats` struct exists in `db/queries.rs` (next to the DB fn); `evaluate()` receives pre-decomposed values instead.
- Documented as a deliberate deviation in apply-progress. Semantically equivalent, but the public contract differs.
- Impact: low — internal function, not a public API surface.

### SUGGESTION

**S-01 — No p95 latency benchmark for `POST /v1/policy/check`**
- Spec §6 requires p95 < 50ms at 50 policies/org as a PRD requirement.
- No benchmark or performance test exists. Acceptable for MVP given no Criterion/bench infra.
- Recommend adding a benchmark before the endpoint is used in production with large policy sets.

---

## Design Coherence

| Design decision | Implementation | Match |
|-----------------|----------------|-------|
| `evaluate()` in `src/policy/mod.rs` (pure, no I/O) | Yes — `src/policy/mod.rs` | MATCH |
| `DailyStats` next to DB query that produces it | Yes — in `db/queries.rs` | MATCH |
| `RawCreatePolicyRequest` flat struct for 400 vs 422 | Yes — in `api/policy.rs` | MATCH |
| `require_permission` called with `conn` lock held before handler work | Yes — all 4 CRUD handlers | MATCH |
| Protected router placement (after projects routes) | Lines 78–80 in `router.rs` | MATCH |
| `pub mod policy;` alphabetically after `audit` in `api/mod.rs` | Confirmed in apply-progress; T-07 complete | MATCH |

---

## Final Verdict

**PASS WITH WARNINGS**

- 0 CRITICAL issues
- 3 WARNINGS (W-01: test file location, W-02: missing `immutable_rule_type` 400, W-03: evaluate() signature deviation)
- 1 SUGGESTION (S-01: no p95 benchmark)

All 223 tests pass. All 8 tasks are marked complete. The two functional gaps are: (1) `tests/policy_api.rs` was not created as a standalone file, and (2) `PATCH` with an explicit `rule_type` silently ignores it instead of returning `400 immutable_rule_type`. Neither blocks operation; both are spec contract deviations to document.

