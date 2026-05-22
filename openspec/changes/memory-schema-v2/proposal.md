# Proposal: Memory Schema V2

## Intent

The `memories` table stores flat blobs with no taxonomy, no dedup, and no session tracking. This makes it impossible to filter by observation type, detect duplicate content, upsert evolving topics, or correlate memories with sessions. These are capabilities Engram's schema has proven essential over hundreds of real-world sessions. Enriching the schema now unblocks the MCP plugin from offering structured memory features (type filtering, topic upserts, session summaries) that clients already expect.

## Scope

### In Scope
- Add 6 columns to `memories`: `type`, `title`, `topic_key`, `scope`, `revision_count`, `normalized_hash`
- Add `session_id` FK column to `memories`
- Create `sessions` table
- Rebuild FTS index to include `title` and `type`
- Update `POST /v1/memory/store` to accept new fields + upsert on `topic_key`
- Add `type` and `scope` filters to `GET /v1/memory`
- Include `title`/`type` in `POST /v1/memory/search` FTS
- New endpoints: `POST /v1/sessions`, `PATCH /v1/sessions/:id`
- Update `Memory` struct in `models/types.rs`
- Migration v2 via `ALTER TABLE` (additive, backwards-compatible)
- Tests for migration, upsert logic, FTS rebuild, new filters, session CRUD

### Out of Scope
- Embedding / vector search
- Session auto-detection or lifecycle management
- Admin UI changes (apps/admin)
- Breaking changes to existing API contracts
- `updated_at` column (revision_count is sufficient for now)

## Capabilities

### New Capabilities
- `memory-upsert`: topic_key-based upsert with revision tracking and dedup via normalized_hash
- `session-tracking`: session CRUD + memory-session linkage
- `memory-taxonomy`: type/scope/title fields enabling structured filtering and FTS

### Modified Capabilities
- `memory-storage`: store endpoint accepts new optional fields; backwards-compatible
- `memory-search`: FTS index expanded to include title and type columns

## Approach

1. **Migration v2**: Separate function in `migrations.rs` — `ALTER TABLE ADD COLUMN` for each new field (SQLite requires one per statement). Drop + recreate FTS table and triggers to include `title` and `type`. Create `sessions` table. Idempotent guard via user_version pragma.
2. **Upsert logic**: In `queries.rs`, when `topic_key` is provided, query for existing memory with same `(org_id, topic_key)`. If found, UPDATE content + increment `revision_count` + recompute `normalized_hash`. Otherwise INSERT.
3. **Dedup**: Compute SHA-256 of `content.trim().to_lowercase()` before insert/update. Store in `normalized_hash`.
4. **API layer**: Extend `StoreInput`, `ListParams`, `SearchInput` structs. Add session handler module. Register new routes.
5. **FTS rebuild**: Drop and recreate `memories_fts` with columns `(content, tags, title, type)` + backfill trigger.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `apps/backend/src/db/migrations.rs` | Modified | Add migration v2 function + tests |
| `apps/backend/src/db/queries.rs` | Modified | Upsert logic, new filters, session CRUD queries |
| `apps/backend/src/api/memory.rs` | Modified | Extended structs, upsert flow, new filter params |
| `apps/backend/src/api/router.rs` | Modified | Register session routes |
| `apps/backend/src/models/types.rs` | Modified | Add fields to Memory, new Session struct |
| `apps/backend/src/api/sessions.rs` | New | Session create/update handlers |
| `apps/backend/tests/integration_test.rs` | Modified | Cover new endpoints |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| FTS rebuild on existing data loses index during migration | Med | Wrap in transaction; backfill from memories table immediately after recreate |
| ALTER TABLE on large SQLite DB blocks reads | Low | Additive ALTERs are fast in SQLite; no table rewrite needed |
| topic_key upsert race condition under concurrent writes | Low | Mutex-wrapped connection already serializes; document limitation |
| Backwards compat if client sends unknown fields | Low | All new fields are optional with defaults; old clients unaffected |

## Rollback Plan

1. Revert migration v2 code (the `run_v2` function).
2. SQLite does not support `DROP COLUMN` pre-3.35. If rollback is needed on older SQLite, restore from backup. For SQLite >= 3.35 (bundled rusqlite ships 3.45+), `ALTER TABLE DROP COLUMN` works for each added column.
3. Revert API handler changes — old structs ignore unknown columns.
4. FTS table can be rebuilt to original schema via the existing v1 migration triggers.

## Dependencies

- `sha2` crate for SHA-256 hashing (or use rusqlite's bundled SQLite `sha256` if available)

## Success Criteria

- [ ] Migration v2 runs idempotently on fresh and existing databases
- [ ] `POST /v1/memory/store` with `topic_key` performs upsert (insert first, update second)
- [ ] `normalized_hash` detects content-identical memories
- [ ] FTS search matches on `title` and `type`
- [ ] `GET /v1/memory?type=bugfix&scope=project` filters correctly
- [ ] Session create + update + memory linkage works end-to-end
- [ ] All existing tests continue to pass unchanged
- [ ] New tests cover: migration idempotency, upsert, dedup, FTS rebuild, session CRUD, filter params
