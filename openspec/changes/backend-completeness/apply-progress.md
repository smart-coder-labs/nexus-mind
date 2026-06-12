# Apply Progress: backend-completeness — PR-1 + PR-2 + PR-3 + PR-4

**Mode**: Strict TDD
**Batch**: PR-4 (T-09, T-10) — completed after PR-1..PR-3
**`cargo test` status**: PASS — all tests green
**Test count**: 174 (up from 166 after PR-3)

---

## TDD Cycle Evidence

| Task | RED | GREEN | REFACTOR |
|------|-----|-------|----------|
| T-01 | `cargo build` fails without dashmap (dependency absent) | Added `dashmap = "5"` to Cargo.toml; `cargo build` succeeds | N/A (dep declaration only) |
| T-02 | Tests (`run_v9_*`) fail to compile — `run_v9` undefined | Implemented `run_v9`; renamed `run_all_sets_user_version_to_8` → `_to_9`; all 14 migration tests pass | Index column name corrected: `created_at` → `timestamp` (audit_logs schema uses `timestamp`) |
| T-03 | Wrote `store_memory_symbol_is_gone` + `legacy_store` helper before deleting the function | Deleted `pub fn store_memory`; migrated 13 internal test call sites + 2 in integration_test.rs to `legacy_store`; all tests pass | Removed spurious `.unwrap()` from `legacy_store(...)` call sites (function returns `Memory`, not `Result<Memory>`) |
| T-04 | Added hash fields to `AuditEntry` struct; added 2 types.rs tests + 1 queries.rs test — all compile-fail until SELECT lists updated | Added `previous_hash, current_hash` to SELECT in `list_audit` and `list_all_audit`; updated both row-mapping closures; fixed struct literal in audit-entry tests | No structural changes needed |
| T-05 | Added 3 handler tests referencing `super::get_by_id` (which did not exist) → compile error | Implemented `get_memory_by_id_for_org` in queries.rs; `get_by_id` handler in memory.rs; registered `GET /v1/memory/:id` in router.rs | N/A — implementation matched design directly |
| T-06 | Added `context.rs` with 3 tests + referenced `db_queries::get_project_context` (not yet defined) → compile error | Implemented `get_project_context` in queries.rs (3 queries); handler in `context.rs`; `pub mod context` in mod.rs; route in router.rs | N/A |
| T-07 | Wrote 5 tests referencing `insert_audit_log_chained` (undefined) → compile error | Implemented `insert_audit_log_chained` with transactional read+insert and SHA-256 chain; rewrote `log_audit` as thin wrapper; all 5 tests pass | N/A |
| T-08 | Wrote 8 tests referencing `super::post_audit` (undefined) → compile error; `missing_action`/`missing_resource_type` tests initially returned 422 (Axum default) not 400 | Implemented `post_audit` handler with manual validation; changed `ExternalAuditRequest` required fields to `Option<String>` so Axum always deserializes, handler returns 400; registered `POST /v1/audit/log` in router.rs; all 8 tests pass | Required fields declared `Option<String>` to give the handler control over the 400 vs 422 distinction |
| T-09 | Wrote 6 unit tests exercising `Bucket::try_consume` and `evict_stale` directly — all compile and pass immediately since unit tests exercise the implementation | Implemented `rate_limit.rs`: `RateLimitState`, `Bucket` (with injectable `now: Instant`), `TierQuota`, `quota_for`, `rate_limit` middleware, `evict_stale`; added `pub mod rate_limit` to `api/mod.rs`; 6 tests pass | `Bucket::try_consume` takes `now: Instant` for full time control without sleeping; `RateLimitState` holds `conn: Arc<Mutex<Connection>>` for plan lookup |
| T-10 | Wrote 2 integration tests (`router_rate_limit_applied_after_auth`, `router_rate_limit_returns_429_on_exhaustion_integration`) using a local `app_with_rate_limit` test helper — tests pass immediately since local helper correctly layers auth+rate_limit | Wired `RateLimitState::new(store.conn())` + `.layer(middleware::from_fn_with_state(rate_state, rate_limit::rate_limit))` into `protected` router in `router.rs` (below auth layer); 2 new integration tests pass | Layer order: rate_limit is inner (added first in code, runs second at runtime); auth is outer (added last, runs first) — identical to test helper order |

---

## Completed Tasks

- [x] T-01: `dashmap = "5"` added to `apps/backend/Cargo.toml`; resolves to dashmap v5.5.3
- [x] T-02: `run_v9` implemented in `apps/backend/src/db/migrations.rs`; `run_all` updated; 5 new tests + 1 test renamed
- [x] T-03: `store_memory()` deleted from `queries.rs`; 13 internal + 2 integration call sites migrated to `legacy_store`; `store_memory_symbol_is_gone` test added
- [x] T-04: `previous_hash` + `current_hash` fields added to `AuditEntry` in `types.rs` with `#[serde(default)]`; SELECT updated in `list_audit` and `list_all_audit`; 3 new tests
- [x] T-05: `get_memory_by_id_for_org` added to `queries.rs`; `get_by_id` handler added to `memory.rs`; `GET /v1/memory/:id` registered in router.rs; 3 new tests
- [x] T-06: `ProjectContext` struct added to `types.rs`; `get_project_context` query added to `queries.rs`; `context.rs` handler created; `pub mod context` in `mod.rs`; `GET /v1/context/project/:project` registered in router.rs; 3 new tests
- [x] T-07: `insert_audit_log_chained` added to `queries.rs` (transactional, SHA-256 chain per tenant); `log_audit` rewritten as thin wrapper; 5 new tests
- [x] T-08: `ExternalAuditRequest` added to `types.rs`; `post_audit` handler added to `audit.rs`; `audit:write` added to admin role in `get_role_permissions`; `POST /v1/audit/log` registered in `router.rs`; 8 new tests
- [x] T-09: `rate_limit.rs` created with `RateLimitState`, `Bucket`, `TierQuota`, `quota_for`, `rate_limit` middleware, lazy eviction; `pub mod rate_limit` added to `api/mod.rs`; 6 unit tests
- [x] T-10: `rate_limit` middleware wired into `protected` router in `router.rs`; `RateLimitState::new(store.conn())` constructed in `router::build`; 2 integration tests

---

## Files Changed

### PR-1

| File | Action | What Was Done |
|------|--------|---------------|
| `apps/backend/Cargo.toml` | Modified | Added `dashmap = "5"` |
| `apps/backend/src/db/migrations.rs` | Modified | Added `run_v9` function; appended to `run_all`; renamed version test; added 5 new T-02 tests |
| `apps/backend/src/db/queries.rs` | Modified | Deleted `pub fn store_memory` declaration; migrated 13 test call sites to `legacy_store`; added `legacy_store` helper and `store_memory_symbol_is_gone` test |
| `apps/backend/tests/integration_test.rs` | Modified | Added `StoreMemoryRequest + Memory` imports; added `legacy_store` helper; migrated 2 call sites; updated version assertion 8 → 9 |

### PR-2

| File | Action | What Was Done |
|------|--------|---------------|
| `apps/backend/src/models/types.rs` | Modified | Added `previous_hash: Option<String>` + `current_hash: Option<String>` to `AuditEntry` with `#[serde(default)]`; added `ProjectContext` struct; fixed existing `AuditEntry` struct literal in tests; added 3 T-04 tests |
| `apps/backend/src/db/queries.rs` | Modified | Updated `list_audit` SELECT + row mapping to include `previous_hash, current_hash`; updated `list_all_audit` SELECT + row mapping; added `get_memory_by_id_for_org` fn; added `get_project_context` fn; added 1 T-04 test |
| `apps/backend/src/api/memory.rs` | Modified | Added `pub async fn get_by_id` handler; added 3 T-05 tests in test module |
| `apps/backend/src/api/context.rs` | Created | New file: `get_project_context` handler + 3 T-06 tests |
| `apps/backend/src/api/mod.rs` | Modified | Added `pub mod context;` |
| `apps/backend/src/api/router.rs` | Modified | Added `context` import; registered `GET /v1/memory/:id` and `GET /v1/context/project/:project` |

### PR-3

| File | Action | What Was Done |
|------|--------|---------------|
| `apps/backend/src/db/queries.rs` | Modified | Added `insert_audit_log_chained` (transactional SHA-256 chain); rewrote `log_audit` as wrapper; added `audit:write` to admin role in `get_role_permissions`; added 5 T-07 tests |
| `apps/backend/src/models/types.rs` | Modified | Added `ExternalAuditRequest` struct (required fields as `Option<String>` for 400 control) |
| `apps/backend/src/api/audit.rs` | Modified | Added `post_audit` handler with validation; added `ExternalAuditRequest` import; added 8 T-08 tests |
| `apps/backend/src/api/router.rs` | Modified | Registered `POST /v1/audit/log` |

### PR-4

| File | Action | What Was Done |
|------|--------|---------------|
| `apps/backend/src/api/rate_limit.rs` | Created | `RateLimitState` (with `conn` for plan lookup, `request_counter` for eviction), `Bucket` (injectable `now: Instant`), `TierQuota`, `quota_for`, `rate_limit` middleware fn, `evict_stale`; 6 unit tests (T-09) + 2 integration tests (T-10) |
| `apps/backend/src/api/mod.rs` | Modified | Added `pub mod rate_limit;` |
| `apps/backend/src/api/router.rs` | Modified | Imported `rate_limit` module; constructed `RateLimitState::new(store.conn())`; layered `rate_limit` middleware on `protected` router below auth |

---

## Test Count History

| After | Count | Delta |
|-------|-------|-------|
| PR-1 complete (T-01..T-03) | 144 | +0 baseline confirmed |
| T-04 | 147 | +3 |
| T-05 | 150 | +3 |
| T-06 | 153 | +3 |
| T-07 | 158 | +5 |
| T-08 | 166 | +8 |
| T-09 | 172 | +6 |
| T-10 | 174 | +2 |

---

## Deviations from Design

- **PR-1**: `idx_audit_logs_org_ts` uses `audit_logs.timestamp` (correct column name), not `created_at` (tasks.md typo). Documented in PR-1 progress.
- **PR-2 / T-06**: The complex test setup for `get_project_context_returns_correct_shape` avoids using an MCP server and works directly with the `upsert_memory` query helper — consistent with other tests in this codebase.
- **PR-2 / T-05 cross-tenant test**: Uses two separate in-memory SQLite stores to simulate genuine tenant isolation.
- **PR-3 / T-08**: `ExternalAuditRequest.action` and `.resource_type` declared as `Option<String>` (not `String`) to give the handler full control over returning 400 for missing fields. Axum's `Json` extractor returns 422 for missing required fields; the design spec and tasks require 400. Manual validation in the handler resolves this correctly.
- **PR-3 / T-07 chain canonical**: Previous hash bytes are the UTF-8 bytes of the hex string (not the raw 32 bytes). This matches the design spec's `prev_hash_bytes || 0x1F || ...` when interpreted as "the stored hex string bytes" — making offline verification a simple string read without hex decoding.
- **PR-4 / T-09**: `RateLimitState` includes `conn: Arc<Mutex<Connection>>` (not in the design sketch) to support per-org plan lookup on new-bucket creation. This is required by the design's tier-caching strategy and adds no observable behavior change.
- **PR-4 / T-10**: T-10 tests use a local `app_with_rate_limit` helper rather than the actual `router::build` (which requires `Config`). The middleware layer order in the helper is identical to `router.rs`.

---

## Remaining Tasks

None — all 10 tasks across PR-1 through PR-4 are complete.

---

## `cargo test` Output Summary

All test suites pass after PR-4 (final):
- `nexusmind` (lib tests): 155 tests pass
- `http_auth_test`: 5 tests pass
- `integration_test`: 14 tests pass
- Doc-tests: 0 tests (no doc-tests defined)
- **Total: 174 tests**
