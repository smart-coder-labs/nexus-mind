# Archive Report — Policy Engine (MVP)

> **Change**: `policy-engine`
> **Archived**: 2026-06-11
> **Status**: VERIFIED — PASS WITH WARNINGS
> **Delivery**: auto-chain, stacked-to-main (2 PRs)

---

## What Was Implemented

8 tasks across 4 phases:

| Task | Deliverable |
|------|-------------|
| T-01 | `run_v10` migration — `policies` table + `idx_policies_org` index; idempotency guard via `PRAGMA user_version < 10` |
| T-02 | 7 new Rust types in `src/models/types.rs`: `Policy`, `PolicyConfig`, `CreatePolicyRequest`, `UpdatePolicyRequest`, `PolicyCheckRequest`, `PolicyViolation`, `PolicyCheckResponse` |
| T-03 | 8 DB query helpers in `src/db/queries.rs`: `DailyStats`, `list_policies`, `list_enabled_policies`, `get_policy`, `insert_policy`, `update_policy`, `delete_policy`, `fetch_daily_stats` |
| T-04 | RBAC: `policy:read` + `policy:write` added to `get_role_permissions` (admin gets both; member gets read-only; viewer gets none) |
| T-05 | Pure `evaluate()` function in `src/policy/mod.rs` — handles all 3 rule types (`model_whitelist`, `budget_limit`, `pii_redact`); 11 unit tests |
| T-06 | 5 HTTP handlers in `src/api/policy.rs`: `list_policies`, `create_policy`, `update_policy`, `delete_policy`, `check_policy` |
| T-07 | Module wiring: `pub mod policy;` in `src/api/mod.rs`; `pub mod policy;` in `src/lib.rs` |
| T-08 | Route registration in `src/api/router.rs`: 3 `.route()` calls under the `protected` router — `GET/POST /v1/policies`, `PATCH/DELETE /v1/policies/:id`, `POST /v1/policy/check` |

## Files Changed / Created

| File | Action |
|------|--------|
| `src/db/migrations.rs` | Modified — added `run_v10` |
| `src/models/types.rs` | Modified — 7 policy types + 7 round-trip tests |
| `src/db/queries.rs` | Modified — 8 query helpers + 13 unit tests; RBAC updated |
| `src/policy/mod.rs` | Created — pure `evaluate()` + 11 unit tests |
| `src/lib.rs` | Modified — `pub mod policy;` |
| `Cargo.toml` | Modified — `regex = "1"` added |
| `src/api/policy.rs` | Created — CRUD + check handlers + 14 in-module tests |
| `src/api/mod.rs` | Modified — `pub mod policy;` |
| `src/api/router.rs` | Modified — 5 routes wired |

## Test Count

| Baseline (pre-policy-engine) | Final |
|-----------------------------|-------|
| 174 | 223 |

All 223 tests passing at archive time (`cargo test` exit code 0).

## Verify Verdict

PASS WITH WARNINGS — 0 CRITICAL, 3 WARNINGS, 1 SUGGESTION.

Warnings on record (non-blocking):
- W-01: Integration tests placed as in-module `#[cfg(test)]` (not separate `tests/policy_api.rs`)
- W-02: `PATCH` with `rule_type` silently drops it instead of returning `400 immutable_rule_type`
- W-03: `evaluate()` accepts flat `u64` params instead of `DailyStats` struct per spec §4

## Known Deviations from Spec/Design

1. `evaluate()` signature: flat `(tokens_used: u64, requests_used: u64)` instead of `daily: DailyStats` — deliberate, keeps `policy/mod.rs` free of `db` module dependency.
2. `RawCreatePolicyRequest` (flat serde) introduced to ensure `invalid_rule_type` returns HTTP 400 rather than serde's default 422 — not in original spec but correct behavior.
3. `tests/policy_api.rs` not created; coverage landed in `src/api/policy.rs` as in-module tests.

