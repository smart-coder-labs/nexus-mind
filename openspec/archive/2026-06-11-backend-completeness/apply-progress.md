# Apply Progress: backend-completeness — PR-1 + PR-2 + PR-3 + PR-4

**Mode**: Strict TDD
**Batch**: PR-4 (T-09, T-10) — completed after PR-1..PR-3
**`cargo test` status**: PASS — all tests green
**Test count**: 174 (up from 166 after PR-3)

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

## `cargo test` Output Summary

All test suites pass after PR-4 (final):
- `nexusmind` (lib tests): 155 tests pass
- `http_auth_test`: 5 tests pass
- `integration_test`: 14 tests pass
- Doc-tests: 0 tests (no doc-tests defined)
- **Total: 174 tests**

---

## Key Implementation Notes

- **Migration v9** uses `ALTER TABLE` statements individually (SQLite limitation), then runs indexes + version bump in one batch. Idempotent due to `IF NOT EXISTS` and error-ignore pattern.
- **Hash chain** implemented with transactional select-then-insert; per-tenant chains to avoid cross-org info leak; canonical record uses stored JSON hex string bytes.
- **Rate limiter** uses `auth.user_id` as bucket key; tier cached on bucket; lazy eviction every 1024 requests; wired after auth in Axum middleware stack.
- **Dead code removal** completed safely; no external API surfaces exposed `store_memory()`.
- All 14 files modified or created; all tests passing in strict TDD mode.
