# Tasks: backend-completeness

> Delivery strategy: auto-chain (4 PRs at natural boundaries)
> TDD mode: STRICT — `cargo test` must pass after EVERY task
> Migration note: design confirmed current user_version = 8; new migration ships as `run_v9`

---

## PR-1: Data model — migration v9, indexes, dead-code removal

Safest slice to review first. Pure schema + deletion work; no new HTTP surface. Must land before PR-2 and PR-3 depend on new columns.

### Sequential block A — dependencies between tasks

- [x] T-01: Add `dashmap` dependency to Cargo.toml
  - Files: `apps/backend/Cargo.toml`
  - Test: `cargo build` compiles without error (dashmap resolves); no behavior test needed
  - PR: PR-1
  - Notes: `dashmap = "5"`. Must land first so later tasks can import it.

- [x] T-02: Implement `run_v9` migration (schema changes + indexes)
  - Files: `apps/backend/src/db/migrations.rs`
  - Test (write first):
    - `run_v9_adds_hash_columns_to_audit_logs` — run v9 on a v8 DB, confirm `PRAGMA table_info(audit_logs)` includes `previous_hash` and `current_hash`
    - `run_v9_adds_plan_to_organizations` — confirm `plan` column exists with DEFAULT 'free'
    - `run_v9_adds_four_indexes` — `PRAGMA index_list(memories)` includes `idx_memories_scope`, `idx_memories_type`, `idx_memories_project_id`; `PRAGMA index_list(audit_logs)` includes `idx_audit_logs_org_ts`
    - `run_v9_is_idempotent` — run v9 twice; no error; `user_version = 9`
    - `run_v9_preserves_existing_rows` — seed rows in v8 DB, run v9, confirm rows still readable with `previous_hash = NULL`
    - Update existing test `run_all_sets_user_version_to_8` → expect 9 (or rename to `run_all_sets_user_version_to_9`)
  - PR: PR-1
  - Notes: All DDL inside a single transaction. ALTER `audit_logs` + ALTER `organizations` + 4x `CREATE INDEX IF NOT EXISTS`. Set `user_version = 9` only after commit.

- [x] T-03: Remove `store_memory()` dead code from `queries.rs` and migrate its test callers
  - Files:
    - `apps/backend/src/db/queries.rs` (delete declaration at line 150; migrate 12 internal `#[cfg(test)]` call sites using a local `legacy_store` helper or inline `upsert_memory`)
    - `apps/backend/tests/integration_test.rs` (migrate 2 call sites at lines 100 and 126)
  - Test (write first):
    - `store_memory_symbol_is_gone` — `rg "store_memory\(" apps/backend/src apps/backend/tests` must return zero hits after this task; enforce via a doc-test comment or a dedicated grep-based shell test in a `#[test]` block that asserts the symbol does not compile if re-introduced
    - Existing tests that called `store_memory` must still pass after migrating to `upsert_memory`
  - PR: PR-1
  - Notes: `upsert_memory` without `topic_key` always INSERTs (per existing test `upsert_memory_no_topic_key_always_inserts`). Add private `#[cfg(test)] fn legacy_store(...)` helpers in both files to minimize call-site churn.

---

## PR-2: Memory & context endpoints

Depends on PR-1 (needs `project_id` parity confirmed by v9 running clean; needs `idx_memories_project_id` for context query performance). Tasks within this PR can run in parallel once T-02 is merged.

### Can run in parallel after PR-1 merges

- [x] T-04: Add `previous_hash` / `current_hash` fields to `AuditEntry` and update list query SELECTs
  - Files:
    - `apps/backend/src/models/types.rs`
    - `apps/backend/src/db/queries.rs` (update `list_audit` at line 537 and `list_all_audit` at line 1627 SELECT lists)
  - Test (write first):
    - `audit_entry_serializes_hash_fields_as_null_when_missing` — deserialize a pre-v9 row (both columns NULL); confirm JSON output includes `"previous_hash": null` and `"current_hash": null`
    - `list_audit_returns_hash_fields` — after v9 migration and one chained insert, `list_audit` response includes non-null hash fields
  - PR: PR-2 (placed here because `GET /v1/audit` is an existing endpoint; hash fields are required by the spec before PR-3 adds the write path)
  - Notes: Add `#[serde(default)]` to both fields so existing clients ignoring them are unaffected.

- [x] T-05: Add `get_memory_by_id_for_org` query and `get_by_id` handler
  - Files:
    - `apps/backend/src/db/queries.rs` (add `get_memory_by_id_for_org(conn, org_id, id) -> Result<Option<Memory>>`)
    - `apps/backend/src/api/memory.rs` (add `pub async fn get_by_id` handler)
    - `apps/backend/src/api/router.rs` (register `GET /v1/memory/:id`)
  - Test (write first):
    - `get_by_id_returns_200_for_own_memory` — org A stores a memory, calls `GET /v1/memory/{id}`, gets 200 with full record including `project_id`
    - `get_by_id_returns_404_for_unknown_id` — call with non-existent id, expect 404
    - `get_by_id_returns_404_for_other_org_memory` — org B calls with org A's memory id, expect 404 (not 403)
  - PR: PR-2

- [x] T-06: Add `ProjectContext` type, `get_project_context` query, `context.rs` handler, wire route
  - Files:
    - `apps/backend/src/models/types.rs` (add `pub struct ProjectContext { project, recent_memories, tools, last_activity }`)
    - `apps/backend/src/db/queries.rs` (add `get_project_context(conn, org_id, project) -> Result<ProjectContext>` — three scoped queries)
    - `apps/backend/src/api/context.rs` (new file — `pub async fn get_project_context` handler)
    - `apps/backend/src/api/mod.rs` (add `pub mod context;`)
    - `apps/backend/src/api/router.rs` (register `GET /v1/context/project/:project`)
  - Test (write first):
    - `get_project_context_returns_correct_shape` — 5 memories for `project = "nexusmind"`, call endpoint, assert `memories` count <= 20 ordered DESC, `tools` deduplicated, `last_activity` equals newest `created_at`
    - `get_project_context_empty_project_returns_200_not_404` — no memories for `project = "empty-proj"`, call endpoint, expect 200 with empty arrays and `null` last_activity
    - `get_project_context_cross_tenant_isolation` — org A has memories for project "shared", org B calls same endpoint, org B receives empty (its own scope only)
  - PR: PR-2

---

## PR-3: Audit enhancements — hash chain + external ingest

Depends on PR-2 (T-04 must be merged; `AuditEntry` needs hash fields before the chained insert can return them). T-07 must complete before T-08 since the handler calls `insert_audit_log_chained`.

### Sequential within PR-3

- [x] T-07: Implement `insert_audit_log_chained` and rewrite `log_audit` as wrapper
  - Files:
    - `apps/backend/src/db/queries.rs` (add `pub fn insert_audit_log_chained(conn, org_id, user_id, action, resource_type, resource_id, metadata, timestamp_override) -> Result<AuditEntry>`; rewrite `log_audit` at line 631 as a thin wrapper calling `insert_audit_log_chained` with `timestamp_override = None`)
  - Test (write first):
    - `insert_audit_log_chained_bootstraps_chain` — first insert for org has `previous_hash = NULL`, `current_hash` is non-empty hex string
    - `insert_audit_log_chained_sequential_links` — insert N=3 rows for same org; verify each row's `previous_hash` equals preceding row's `current_hash`; verify replaying `sha256(prev || canonical)` reproduces stored `current_hash` for each row
    - `insert_audit_log_chained_cross_tenant_isolation` — org A inserts 2 rows, org B inserts 1 row; org B's `previous_hash` is NULL (its own genesis), not org A's last hash
    - `insert_audit_log_chained_concurrent_writes_no_corruption` — two writes for same org via `std::thread::scope`; resulting chain has exactly 2 new records correctly linked (tests `Arc<Mutex<Connection>>` safety)
    - `log_audit_wrapper_still_works` — existing `log_audit` call produces a row with non-null `current_hash` (proves wrapper path)
  - PR: PR-3
  - Notes: All existing `log_audit` call sites in `store/sqlite.rs` automatically join the chain with zero call-site changes. Canonical record format per design § Gap 5: `sha256(prev_hash_bytes || 0x1F || timestamp || 0x1F || action || 0x1F || resource_type || 0x1F || resource_id || 0x1F || metadata_json_compact)`.

- [x] T-08: Add `ExternalAuditRequest` type, `post_audit` handler, `audit:write` permission, wire route
  - Files:
    - `apps/backend/src/models/types.rs` (add `pub struct ExternalAuditRequest { action, resource_type, resource_id?, metadata?, timestamp? }`)
    - `apps/backend/src/db/queries.rs` (add `"audit:write"` to admin role in `get_role_permissions`)
    - `apps/backend/src/api/audit.rs` (add `pub async fn post_audit` handler — validation + call `insert_audit_log_chained` + return 201)
    - `apps/backend/src/api/router.rs` (register `POST /v1/audit/log`)
  - Test (write first):
    - `post_audit_log_returns_201_with_hash_fields` — valid API key, valid body `{ actor, action, resource_type }`, expect 201 with `previous_hash` and `current_hash` in response body
    - `post_audit_log_persisted_in_get_audit` — after POST, call `GET /v1/audit`, confirm new record appears
    - `post_audit_log_missing_action_returns_400` — omit `action` from body, expect 400; confirm no row written
    - `post_audit_log_missing_resource_type_returns_400` — same for `resource_type`
    - `post_audit_log_unauthenticated_returns_401` — no API key header, expect 401
    - `post_audit_log_member_role_returns_403` — API key with `member` role, expect 403 (`audit:write` is admin-only)
    - `post_audit_log_invalid_timestamp_returns_400` — unparseable RFC 3339 timestamp override, expect 400
    - `post_audit_log_oversized_metadata_returns_400` — metadata JSON > 16 KB, expect 400
  - PR: PR-3

---

## PR-4: Rate limiting middleware

Depends on PR-1 (needs `dashmap` in Cargo.toml from T-01; needs `organizations.plan` column from T-02). Independent of PR-2 and PR-3; can be worked in parallel with PR-3 after PR-1 merges.

### Can run in parallel with PR-3 (after PR-1)

- [x] T-09: Implement `RateLimitState`, `Bucket`, `TierQuota`, token-bucket algorithm, and `rate_limit` middleware in `api/rate_limit.rs`
  - Files:
    - `apps/backend/src/api/rate_limit.rs` (new file — full module: state types, `quota_for(plan)`, middleware fn, lazy eviction)
    - `apps/backend/src/api/mod.rs` (add `pub mod rate_limit;`)
  - Test (write first):
    - `rate_limit_within_quota_passes_through` — OSS/free bucket with < 100 tokens consumed; next request gets 200
    - `rate_limit_exhausted_returns_429_with_retry_after` — exhaust 100-token free bucket; next request returns 429 with positive `Retry-After` integer header
    - `rate_limit_bucket_refills_after_window` — exhaust bucket, advance `Instant` mock past the refill window, confirm next request succeeds (use dependency injection for `Instant` or time-based helper in tests)
    - `rate_limit_team_key_higher_quota` — team-tier bucket (1000 capacity) not exhausted when free-tier would be
    - `rate_limit_different_users_independent_buckets` — user A exhausted, user B still succeeds (no cross-key interference)
    - `rate_limit_lazy_eviction_removes_stale_entries` — insert 1025 requests from different user IDs with stale `last_seen`; verify DashMap size is bounded after the 1024th-hit eviction sweep
  - PR: PR-4
  - Notes: Bucket key is `auth.user_id`. Tier is read from `organizations.plan` on new-bucket creation (one SELECT; cached on bucket). Eviction sweeps every 1024 requests via an `AtomicU64` counter on `RateLimitState`.

- [x] T-10: Wire `rate_limit` middleware into router, construct `RateLimitState` in `router.rs`
  - Files:
    - `apps/backend/src/api/router.rs` (construct `RateLimitState { buckets: Arc::new(DashMap::new()) }`; layer `middleware::from_fn_with_state(rate_state, rate_limit::rate_limit)` on the protected router BELOW the auth layer so it runs after auth at runtime)
  - Test (write first):
    - `router_rate_limit_applied_after_auth` — unauthenticated request to any protected route returns 401 (not 429); proves auth runs before the rate limiter and unauth floods don't allocate buckets
    - `router_rate_limit_returns_429_on_exhaustion_integration` — integration test: call a protected endpoint 101 times with an OSS API key; assert 101st returns 429
  - PR: PR-4
  - Notes: Layer order in Axum `from_fn_with_state` runs bottom-up; auth must be the outermost (last `.layer()` call) so it runs first. `main.rs` requires no changes — `router::build` already owns state construction.

---

## Dependency graph

```
T-01 (dashmap)
  └─ T-02 (run_v9 migration)
       └─ T-03 (dead-code removal)   ← PR-1 complete
            ├─ T-04 (AuditEntry hash fields)
            ├─ T-05 (GET /v1/memory/:id)       ← can be parallel
            ├─ T-06 (GET /v1/context/...)      ← can be parallel
            │                                  PR-2 complete
            ├─ T-07 (insert_audit_log_chained)   [needs T-04]
            │    └─ T-08 (POST /v1/audit/log)   PR-3 complete
            └─ T-09 (rate_limit.rs)              [needs T-01, T-02]
                 └─ T-10 (wire router)           PR-4 complete
```

T-05 and T-06 can be implemented in parallel (separate files, no shared state).
T-09 can be implemented in parallel with T-07/T-08 (different module, different concern).

---

## Review Workload Forecast

| PR | Tasks | Estimated changed lines | Notes |
|----|-------|------------------------|-------|
| PR-1 | T-01, T-02, T-03 | ~180 | Migration fn, index DDL, dead-code deletion + test migrations |
| PR-2 | T-04, T-05, T-06 | ~220 | Two new handlers, new module, query helpers, struct additions |
| PR-3 | T-07, T-08 | ~200 | Chained-insert core logic + handler + permission + 8 tests |
| PR-4 | T-09, T-10 | ~180 | New rate_limit module + router wiring + integration test |
| **Total** | **10** | **~780** | |

**Chained PRs recommended: Yes**

- Total estimated diff (~780 lines across 14 files) exceeds the 400-line single-PR budget.
- Each PR slice ships a coherent, independently reviewable and mergeable unit.
- PR ordering (1 → 2 → 3 → 4) satisfies all inter-PR dependencies.
- PR-3 and PR-4 can be reviewed concurrently (no shared files) once PR-2 is merged.

**Decision needed before apply: No** — delivery strategy is already set to `auto-chain`.
