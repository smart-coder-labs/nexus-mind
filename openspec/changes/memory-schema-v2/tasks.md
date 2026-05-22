# Tasks: memory-schema-v2

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 480–530 |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | PR 1: migration + types + hash · PR 2: upsert + sessions handler · PR 3: filters + FTS + integration tests |
| Delivery strategy | single-pr |
| Chain strategy | size-exception |

Decision needed before apply: Yes
Chained PRs recommended: Yes
Chain strategy: size-exception
400-line budget risk: High

> **STOP**: Delivery strategy is `single-pr`. Estimated diff exceeds 400 lines.
> A maintainer-approved `size:exception` label is required before `sdd-apply` starts.

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Migration v2 + new type structs | PR 1 | Foundation; everything else depends on this |
| 2 | Upsert logic + SHA-256 hash + sessions handler | PR 2 | Depends on PR 1 |
| 3 | Taxonomy filters + FTS expand + integration tests | PR 3 | Depends on PR 2 |

---

## Phase 1: Foundation — Migration and Types

- [x] 1.1 **[RED]** Write failing test in `apps/backend/src/db/migrations.rs` (#[cfg(test)]) verifying `PRAGMA user_version` equals 2 after `run_migrations()`.
- [x] 1.2 **[GREEN]** In `apps/backend/src/db/migrations.rs`: add migration v2 guarded by `PRAGMA user_version < 2`. Create `sessions` table first (FK dependency), then `ALTER TABLE memories ADD COLUMN` for `type`, `title`, `scope`, `topic_key`, `revision_count`, `normalized_hash`, `session_id` (7 columns, idempotent via `IF NOT EXISTS` pattern or error-suppressed).
- [x] 1.3 **[GREEN]** In migration v2: drop `memories_fts`, recreate as FTS5 over `(content, tags, title, type)`, backfill from `memories`, recreate INSERT/UPDATE/DELETE triggers. Set `PRAGMA user_version = 2`.
- [x] 1.4 **[RED]** Write failing unit test in `apps/backend/src/models/types.rs` asserting `StoreMemoryRequest` deserializes new optional fields (`type`, `title`, `scope`, `topic_key`, `session_id`) and `MemoryRecord` serializes them.
- [x] 1.5 **[GREEN]** In `apps/backend/src/models/types.rs`: add optional fields to `StoreMemoryRequest` (`type_`, `title`, `topic_key`, `scope`, `session_id`) and extend `MemoryRecord` with matching fields. Add `Session`, `CreateSessionRequest`, `PatchSessionRequest` structs.
- [x] 1.6 **[REFACTOR]** Ensure `scope` defaults to `"project"` via `#[serde(default = ...)]` attribute; confirm no breaking changes to existing struct consumers in `memory.rs`.

## Phase 2: Core Logic — Upsert, Hash, Sessions

- [x] 2.1 **[RED]** In `apps/backend/src/db/queries.rs` (#[cfg(test)]): write failing test for `store_memory_upsert` — same `topic_key` second call updates `revision_count` to 2, different org gets new row.
- [x] 2.2 **[GREEN]** In `apps/backend/src/db/queries.rs`: implement `store_memory_upsert(conn, org_id, req)` — SELECT by `(org_id, topic_key)`, branch INSERT vs UPDATE. Compute `normalized_hash = sha256(content.trim().to_lowercase())` using `sha2` + `hex` crates. Returns `MemoryRecord`.
- [x] 2.3 **[REFACTOR]** Replace direct `INSERT INTO memories` call in `apps/backend/src/api/memory.rs` with `store_memory_upsert`; ensure legacy requests (no `topic_key`) still INSERT unconditionally.
- [x] 2.4 **[RED]** In `apps/backend/src/db/queries.rs`: write failing tests for `create_session` and `patch_session` — create returns id, patch persists `ended_at`/`summary`, wrong org returns `None`.
- [x] 2.5 **[GREEN]** In `apps/backend/src/db/queries.rs`: implement `create_session(conn, org_id, req) -> Session` and `patch_session(conn, org_id, session_id, req) -> Option<Session>` with dynamic SET clause.
- [x] 2.6 **[RED]** Write failing handler tests in `apps/backend/src/api/sessions.rs` (#[cfg(test)]): `POST /v1/sessions` returns 201 with id, missing `project` returns 422, `PATCH /v1/sessions/:id` wrong org returns 404.
- [x] 2.7 **[GREEN]** Create `apps/backend/src/api/sessions.rs` with `create_session_handler` and `patch_session_handler`. Validate `session_id` in `store_memory_upsert` against calling org (return 422 if invalid).
- [x] 2.8 In `apps/backend/src/api/mod.rs`: add `pub mod sessions;`.
- [x] 2.9 In `apps/backend/src/api/router.rs`: wire `POST /v1/sessions` and `PATCH /v1/sessions/:id` routes.

## Phase 3: Taxonomy Filters and FTS

- [x] 3.1 **[RED]** In `apps/backend/src/db/queries.rs`: write failing test for `list_memories` — `type=bugfix` returns only bugfix rows, `scope=personal` filters by scope, combined filter works, unknown type returns empty list.
- [x] 3.2 **[GREEN]** In `apps/backend/src/db/queries.rs`: extend `list_memories(conn, org_id, type_filter, scope_filter)` — append `AND type = ?` / `AND scope = ?` clauses dynamically when params are present.
- [x] 3.3 **[GREEN]** In `apps/backend/src/api/memory.rs`: extract `type` and `scope` from `GET /v1/memory` query params and forward to `list_memories`.
- [x] 3.4 **[RED]** In `apps/backend/src/db/queries.rs`: write failing test for FTS search — memory with `title = "JWT auth middleware"` appears when searching `"JWT auth"`, memory with `type = "bugfix"` appears when searching `"bugfix"`.
- [x] 3.5 **[GREEN]** Verify FTS query in `apps/backend/src/db/queries.rs` selects against the rebuilt `memories_fts` — no code change needed if migration already wired triggers correctly; otherwise fix the SELECT JOIN.

## Phase 4: Integration Tests and Cleanup

- [x] 4.1 In `apps/backend/tests/integration_test.rs`: add scenario "legacy request succeeds" — only `content` + `project` → 201, `scope = "project"`, `type = null`.
- [x] 4.2 In `apps/backend/tests/integration_test.rs`: add scenario "full v2 request succeeds" — all new fields present, all persisted and returned.
- [x] 4.3 In `apps/backend/tests/integration_test.rs`: add scenario "upsert on topic_key" — second store with same `topic_key` returns updated content, `revision_count = 2`.
- [x] 4.4 In `apps/backend/tests/integration_test.rs`: add scenario "migration idempotency" — run migrations twice on same DB, no error, schema unchanged.
- [x] 4.5 In `apps/backend/tests/integration_test.rs`: add scenario "FTS backfill" — pre-existing rows searchable after migration v2.
- [x] 4.6 Run `cargo test` — all tests green. Fix any compilation errors from new struct fields.
- [x] 4.7 Remove any `allow(dead_code)` stubs or TODO comments added during development.
