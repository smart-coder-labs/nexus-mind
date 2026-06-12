# Tasks: Policy Engine (MVP)

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 550–700 |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | PR 1 (foundation: migration + types + queries + RBAC) → PR 2 (handlers + router + tests) |
| Delivery strategy | auto-chain |
| Chain strategy | stacked-to-main |

Decision needed before apply: No
Chained PRs recommended: Yes
Chain strategy: stacked-to-main
400-line budget risk: High

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Migration, types, queries, RBAC | PR 1 | Self-contained; no handler yet; `cargo test` must pass |
| 2 | `api/policy.rs`, router wiring, integration tests | PR 2 | Targets main after PR 1 merges |

---

## Phase 1: Foundation

- [x] T-01: Add `run_v10` + call from `run_all` in `apps/backend/src/db/migrations.rs`
  - Files: `src/db/migrations.rs`
  - Test (RED first): `run_v10_creates_policies_table`, `run_v10_creates_org_index`, `run_v10_is_idempotent`, `run_v10_rejects_invalid_rule_type`; update `run_all_sets_user_version_to_9` → `run_all_sets_user_version_to_10`
  - Notes: add `in_memory_db_v9()` helper mirroring `in_memory_db_v8()` pattern; migration must be idempotent (PRAGMA guard)

- [x] T-02: Add policy types to `apps/backend/src/models/types.rs`
  - Files: `src/models/types.rs`
  - Test (RED first): none (types are compile-time); confirm `cargo test` still passes after adding
  - Notes: append `Policy`, `PolicyConfig`, `CreatePolicyRequest`, `UpdatePolicyRequest`, `PolicyCheckRequest`, `PolicyViolation`, `PolicyCheckResponse` after the `Project`/`ProjectMember` block (~line 311); add `fn default_enabled() -> bool { true }`

- [x] T-03: Add policy query helpers to `apps/backend/src/db/queries.rs`
  - Files: `src/db/queries.rs`
  - Test (RED first): unit tests in `queries.rs` for `list_policies`, `insert_policy`, `get_policy` (own-org hit + cross-org miss), `update_policy` (own-org + 0-rows case), `delete_policy`, `list_enabled_policies`, `fetch_daily_stats`
  - Notes: add `DailyStats` struct in this file; all queries MUST filter by `org_id`; `fetch_daily_stats` uses `json_extract(metadata, '$.tokens_total')` against `audit_logs`

- [x] T-04: Add `policy:read` / `policy:write` to `get_role_permissions` in `apps/backend/src/db/queries.rs`
  - Files: `src/db/queries.rs`
  - Test (RED first): extend `api/helpers.rs` tests — assert `require_permission(&conn, admin_auth, None, "policy:write").is_ok()`, `require_permission(&conn, member_auth, None, "policy:read").is_ok()`, `require_permission(&conn, member_auth, None, "policy:write").is_err()`, `require_permission(&conn, viewer_auth, None, "policy:read").is_err()`
  - Notes: `get_role_permissions` is a hardcoded match in `queries.rs` (lines 1386–1410) — NOT a separate `permissions.rs` file; design.md is wrong on location; `admin` already gets all (early return), but must still add policy perms to member vec

---

## Phase 2: Core Implementation

- [x] T-05: Create `apps/backend/src/api/policy.rs` with pure `evaluate()` function and `DailyStats`
  - Files: `src/api/policy.rs` (new)
  - Test (RED first): 11 pure-function unit tests in `#[cfg(test)]` block — `evaluate_no_policies_allows_everything`, `model_whitelist_denies_unlisted_model`, `model_whitelist_allows_listed_model`, `budget_limit_request_cap_triggers`, `budget_limit_token_cap_triggers_when_no_request_cap`, `budget_limit_request_cap_takes_precedence`, `pii_redact_matches_pattern`, `pii_redact_skips_when_no_prompt`, `pii_redact_skips_malformed_pattern`, `disabled_policy_is_skipped`, `multiple_violations_all_returned`
  - Notes: `DailyStats` goes here (not in queries.rs); `evaluate()` is pure — no I/O; add `regex = "1"` to `apps/backend/Cargo.toml` only if not already present

- [x] T-06: Add handler scaffolding (`list`, `create`, `update`, `delete`, `check`) to `api/policy.rs`
  - Files: `src/api/policy.rs`
  - Test: covered by T-07 integration tests; ensure `cargo test` passes after this step
  - Notes: `require_permission` is in `crate::api::helpers`, NOT `crate::api::middleware` — design.md import is incorrect; signature is `require_permission(&conn, &ctx, None, "policy:read")`; acquire `conn` lock BEFORE calling it; use `iso8601_ms_now()`, `bad_request()`, `not_found()`, `internal_error()` helpers in same file

---

## Phase 3: Integration / Wiring

- [x] T-07: Register module + wire 5 routes in router
  - Files: `src/api/mod.rs`, `src/api/router.rs`
  - Test: compile check; integration tests in T-08 exercise the routes
  - Notes: add `pub mod policy;` to `mod.rs` alphabetically; add 3 `.route()` calls inside `protected` after the projects routes (line 77); add `policy` to the `use crate::api::{...}` import on line 11

---

## Phase 4: Integration Tests

- [x] T-08: Create `apps/backend/tests/policy_api.rs` with 10 integration tests
  - Files: `tests/policy_api.rs` (new)
  - Test: `create_policy_as_admin_returns_201`, `create_policy_as_member_returns_403`, `create_policy_with_invalid_rule_type_returns_400`, `create_policy_with_empty_allowed_models_returns_400`, `list_policies_returns_only_caller_org`, `update_policy_rejects_rule_type_change`, `delete_policy_in_other_org_returns_404`, `check_with_no_policies_allows`, `check_with_model_whitelist_denies_unknown_model`, `check_returns_http_200_even_when_denied`
  - Notes: follow existing test layout (look at `tests/memory_api.rs` or similar for helper patterns); all tests seed their own in-memory DB via `run_all`; `cargo test` must reach ≥184 passing tests after this task

---

*End of tasks.md*
