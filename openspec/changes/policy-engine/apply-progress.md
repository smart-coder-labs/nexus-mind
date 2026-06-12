# Apply Progress — Policy Engine

**Change**: policy-engine
**Mode**: Strict TDD (RED → GREEN → REFACTOR cycle)
**Batch**: 2 of 2 (PR 2 — Handlers + Router)

---

## TDD Cycle Evidence

| Task | RED (test written first) | GREEN (impl passes) | REFACTOR |
|------|--------------------------|---------------------|----------|
| T-01 | Tests existed from prior batch | Already passing (179 tests) | N/A — already done |
| T-02 | Tests added before types compiled | Types added → all pass | N/A |
| T-03 | Tests added before query fns existed | Query fns added → all pass | Fixed cross-org helper pattern |
| T-04 | Tests written inline before `evaluate` | Module created → all pass | N/A |
| T-05 | 11 handler tests written in `#[cfg(test)]` block before handlers | Handlers added → all pass | Fixed 422→400 serde issue with RawCreatePolicyRequest |
| T-06 | `check_policy` tests written before handler | Handler added → all pass | N/A |
| T-07 | Compile check | `pub mod policy;` added alphabetically to `mod.rs` | N/A |
| T-08 | All routes registered | 5 routes wired in `router.rs` | N/A |

---

## Completed Tasks

- [x] T-01: `run_v10` migration — policies table + idx_policies_org (done in prior batch, 179 tests)
- [x] T-02: Policy types added to `src/models/types.rs`
- [x] T-03: Policy query helpers added to `src/db/queries.rs`; RBAC updated in `get_role_permissions`
- [x] T-04: `src/policy/mod.rs` created with pure `evaluate()` function; `pub mod policy` added to `src/lib.rs`
- [x] T-05: `src/api/policy.rs` created with `list_policies`, `create_policy`, `update_policy`, `delete_policy` handlers + 11 handler tests
- [x] T-06: `check_policy` handler added to `src/api/policy.rs` + 3 tests (no_policies, whitelist_blocking, unauthenticated)
- [x] T-07: `pub mod policy;` added to `src/api/mod.rs` (alphabetical position after `audit`)
- [x] T-08: 5 routes registered in `src/api/router.rs` under the protected router

## Pending Tasks

None. All tasks complete.

---

## Files Changed

| File | Action | Notes |
|------|--------|-------|
| `src/models/types.rs` | Modified | Added `Policy`, `PolicyConfig`, `CreatePolicyRequest`, `UpdatePolicyRequest`, `PolicyCheckRequest`, `PolicyViolation`, `PolicyCheckResponse` + 7 round-trip tests |
| `src/db/queries.rs` | Modified | Added `DailyStats`, `list_policies`, `list_enabled_policies`, `get_policy`, `insert_policy`, `update_policy`, `delete_policy`, `fetch_daily_stats`; added `policy:read`/`policy:write` to `get_role_permissions`; 13 unit tests |
| `src/policy/mod.rs` | Created | Pure `evaluate()` function + 11 unit tests |
| `src/lib.rs` | Modified | Added `pub mod policy;` |
| `Cargo.toml` | Modified | Added `regex = "1"` |
| `src/api/policy.rs` | Created | CRUD + check handlers; `RawCreatePolicyRequest` (flat serde to enable 400 on bad rule_type); 14 handler tests |
| `src/api/mod.rs` | Modified | Added `pub mod policy;` after `audit` |
| `src/api/router.rs` | Modified | Added `policy` to imports; 5 routes in protected router |

---

## Deviations from Design / Notes

- T-05: `CreatePolicyRequest` from `models/types.rs` uses an internally-tagged serde enum (`PolicyConfig`) which causes serde to reject unknown `rule_type` with 422 before the handler runs. Fixed by introducing `RawCreatePolicyRequest` (flat `rule_type: String`, `config: serde_json::Value`) in `api/policy.rs` — this allows the handler to return 400 with `invalid_rule_type` code as the spec requires.
- T-04: `evaluate()` lives in `src/policy/mod.rs` (pure function, no HTTP); handler imports via `crate::policy::evaluate`. Deviates from design.md which placed it in `api/policy.rs`, but aligns with prompt instructions.
- `DailyStats` remains in `src/db/queries.rs` (next to the DB function that produces it).
- `check_policy` handler adds `prompt_tokens` to both `tokens_used` and `requests_used` (+1 to requests when present), matching the spec note.

---

## Test Count

| After task | Passing (lib) | Total (lib + integration) |
|-----------|---------------|--------------------------|
| Baseline (T-01 done) | 179 | — |
| After T-02 | 187 | — |
| After T-03 | 200 | — |
| After T-04 | 211 | — |
| After T-05 + T-06 + T-07 + T-08 | 204 | 223 |

All tests passing. Zero failures.

---

## PR Boundary (Batch 2)

- Delivery: `auto-chain`, `stacked-to-main`
- Scope: Handlers + Router — CRUD endpoints, check endpoint, route wiring, 14 new handler tests
- Rollback: safe — existing routes unchanged; new routes are purely additive
