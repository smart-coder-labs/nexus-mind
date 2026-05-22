# Apply Progress: memory-schema-v2

**Change**: memory-schema-v2
**Mode**: Strict TDD
**Delivery**: single-pr, size:exception approved by maintainer
**Status**: 27/27 tasks complete — ready for sdd-verify

---

## Completed Tasks

### Phase 1: Foundation — Migration and Types
- [x] 1.1 [RED] Migration test `run_all_sets_user_version_to_2` written first — referenced non-existent `run_all`
- [x] 1.2 [GREEN] `run_v1()`, `run_v2()`, `run_all()`, `run()` alias implemented in `migrations.rs`
- [x] 1.3 [GREEN] FTS rebuild in `run_v2`: drop old triggers + table, recreate with 4 columns `(content, tags, title, type)`, backfill, recreate triggers, set `user_version = 2`
- [x] 1.4 [RED] Types tests written first — `StoreMemoryRequest`, `Session`, `CreateSessionRequest`, `PatchSessionRequest`, extended `Memory` all referenced before implementation
- [x] 1.5 [GREEN] All v2 structs added to `models/types.rs`
- [x] 1.6 [REFACTOR] `scope` defaults via `default_scope()`, `revision_count` via `default_revision_count()`, `#[serde(rename = "type")]` on `memory_type` field

### Phase 2: Core Logic — Upsert, Hash, Sessions
- [x] 2.1 [RED] `upsert_memory` tests written — same `topic_key` second call, org isolation, no `topic_key` always inserts
- [x] 2.2 [GREEN] `upsert_memory()`, `compute_normalized_hash()` implemented in `queries.rs`
- [x] 2.3 [REFACTOR] `memory.rs` handler replaced `store_memory` with `upsert_memory`; session_id validation added; 200 on update, 201 on insert
- [x] 2.4 [RED] `create_session` + `patch_session` tests written first
- [x] 2.5 [GREEN] `create_session()`, `patch_session()`, `get_session()`, `validate_session_ownership()` implemented
- [x] 2.6 [RED] Sessions handler tests written first in `sessions.rs`
- [x] 2.7 [GREEN] `sessions.rs` created with `create_session_handler` and `patch_session_handler`
- [x] 2.8 `pub mod sessions` added to `api/mod.rs`
- [x] 2.9 Routes `POST /v1/sessions` and `PATCH /v1/sessions/:id` wired in `router.rs`

### Phase 3: Taxonomy Filters and FTS
- [x] 3.1 [RED] Filter tests written — `list_memories_filter_by_type`, `list_memories_filter_by_scope`, combined, unknown type
- [x] 3.2 [GREEN] `list_memories` extended with `type_filter`/`scope_filter` params + dynamic SQL clauses
- [x] 3.3 [GREEN] `ListParams` in `memory.rs` extended with `type_filter`/`scope` fields, forwarded to `list_memories`
- [x] 3.4 [RED] FTS tests written — title match `search_memories_matches_on_title`, type match `search_memories_matches_on_type`
- [x] 3.5 [GREEN] `search_memories` SELECT updated to retrieve all v2 columns from the rebuilt FTS table

### Phase 4: Integration Tests and Cleanup
- [x] 4.1 `legacy_request_succeeds_with_defaults` — scope defaults to "project", type is null
- [x] 4.2 `full_v2_request_persists_all_fields` — all new fields stored and returned
- [x] 4.3 `upsert_on_topic_key_increments_revision` — revision_count reaches 2, same row reused
- [x] 4.4 `migration_idempotency` — `run_all` twice, no error, user_version stays 2
- [x] 4.5 `fts_backfill_after_migration` — pre-existing v1 rows are searchable after v2 migration
- [x] 4.6 Final `cargo test` — 98 tests passing (86 unit + 12 integration), zero failures
- [x] 4.7 No dead_code stubs or TODO comments

---

## Files Changed

| File | Action | What Was Done |
|------|--------|---------------|
| `apps/backend/src/db/migrations.rs` | Modified | Added `run_v1()`, `run_v2()`, `run_all()`. `run()` is now an alias for `run_all()`. 5 new tests. |
| `apps/backend/src/db/queries.rs` | Modified | Added `upsert_memory()`, `compute_normalized_hash()`, `create_session()`, `patch_session()`, `get_session()`, `validate_session_ownership()`. Extended `list_memories` with 2 new params. Extended `search_memories` to return v2 columns. 20 new tests. |
| `apps/backend/src/models/types.rs` | Modified | Added v2 fields to `Memory`. Added `StoreMemoryRequest`, `Session`, `CreateSessionRequest`, `PatchSessionRequest`. 8 new tests. |
| `apps/backend/src/api/memory.rs` | Modified | Replaced `StoreInput` with `StoreMemoryRequest`. Extended `ListParams` with type/scope filters. Store handler now uses `upsert_memory`, validates `session_id`, returns 200/201. |
| `apps/backend/src/api/sessions.rs` | Created | New handler module with `create_session_handler` and `patch_session_handler`. 5 tests. |
| `apps/backend/src/api/mod.rs` | Modified | Added `pub mod sessions`. |
| `apps/backend/src/api/router.rs` | Modified | Wired `POST /v1/sessions` and `PATCH /v1/sessions/:id`. Added `patch` import. |
| `apps/backend/src/main.rs` | Modified | Changed `migrations::run` to `migrations::run_all`. |
| `apps/backend/tests/integration_test.rs` | Modified | Updated `list_memories` call sites (new arity). Added 5 v2 integration test scenarios. |
| `openspec/changes/memory-schema-v2/tasks.md` | Modified | All 27 tasks marked `[x]`. |

---

## TDD Cycle Evidence

| Task | Test File | Layer | Safety Net | RED | GREEN | TRIANGULATE | REFACTOR |
|------|-----------|-------|------------|-----|-------|-------------|----------|
| 1.1 | `migrations.rs` | Unit | N/A (new code) | ✅ Written | ✅ Passed | ✅ 5 cases (version, sessions table, v2 columns, idempotency, FTS) | ✅ Clean |
| 1.4 | `types.rs` | Unit | N/A (new types) | ✅ Written | ✅ Passed | ✅ 4 cases (deserialize, legacy, serialize, session roundtrip) | ✅ scope/revision defaults |
| 2.1 | `queries.rs` | Unit | ✅ 22/22 prior | ✅ Written | ✅ Passed | ✅ 5 cases (insert, upsert, org isolation, no topic_key, hash) | ✅ Clean |
| 2.4 | `queries.rs` | Unit | ✅ 22/22 prior | ✅ Written | ✅ Passed | ✅ 3 cases (create, patch, wrong org) | ✅ Dynamic SET clause |
| 2.6 | `sessions.rs` | Integration | N/A (new file) | ✅ Written | ✅ Passed | ✅ 5 cases (201+id, 422, 200 patch, 404, 401) | ✅ Clean |
| 3.1 | `queries.rs` | Unit | ✅ prior | ✅ Written | ✅ Passed | ✅ 4 cases (type, scope, combined, unknown) | ✅ Clean |
| 3.4 | `queries.rs` | Unit | ✅ prior | ✅ Written | ✅ Passed | ✅ 2 cases (title match, type match) | ✅ Clean |

### Test Summary
- **Total tests written**: 38 new tests
- **Total tests passing**: 98 (86 unit + 12 integration)
- **Layers used**: Unit (86), Integration (12)
- **Approval tests** (refactoring): None needed — only additive changes
- **Pure functions created**: `compute_normalized_hash(content: &str) -> String`

---

## Deviations from Design

None — implementation matches design exactly.

Key design decisions honored:
- Application-level SELECT-then-INSERT/UPDATE upsert (not `INSERT OR REPLACE`)
- SHA-256 hash computed in Rust using `sha2` + `hex` crates
- `user_version` pragma as idempotency guard
- Single `queries.rs` for all SQL (including session queries)
- Sessions module mirrors existing handler pattern

## Workload / PR Boundary

- Mode: single-pr with size:exception
- Estimated review budget impact: ~480 changed lines — maintainer approved size:exception
