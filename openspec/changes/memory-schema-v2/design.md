# Design: Memory Schema V2

## Technical Approach

Additive migration (v2) using the existing `migrations.rs` pattern: a new `run_v2(&Connection)` function guarded by `PRAGMA user_version`. Upsert logic lives in `queries.rs` as application-level SELECT-then-INSERT/UPDATE (not SQL `INSERT OR REPLACE`) to preserve the existing row's `id` and `created_at`. SHA-256 hash computed in Rust using the already-present `sha2 + hex` crates. Sessions get their own CRUD module mirroring the existing handler pattern.

## Architecture Decisions

| # | Decision | Choice | Rejected | Rationale |
|---|----------|--------|----------|-----------|
| 1 | Migration strategy | `ALTER TABLE ADD COLUMN` x6 + new `sessions` table, guarded by `PRAGMA user_version < 2` | Recreate table via temp copy | SQLite additive ALTERs are instant (no table rewrite). Recreate is needed only for column removal/reorder, which we don't have. |
| 2 | Idempotency guard | `PRAGMA user_version` (0 = fresh, 1 = v1, 2 = v2). Set to 2 after v2 completes. `run()` renamed to `run_v1()`, new `run_all()` calls both. | `IF NOT EXISTS` per column (not supported by SQLite ALTER) | `user_version` is the standard SQLite migration version pattern. Clean, atomic, no introspection queries. |
| 3 | FTS rebuild | Drop `memories_fts` + all 3 triggers, recreate with 4 columns `(content, tags, title, type)`, backfill via `INSERT INTO memories_fts SELECT ...` | ALTER FTS table (not possible in SQLite FTS5) | FTS5 virtual tables cannot be altered. Drop+recreate+backfill is the only path. Wrapped in the same transaction as the migration. |
| 4 | Upsert logic | Application-level: `SELECT id WHERE org_id=? AND topic_key=?`, then branch to UPDATE or INSERT | `INSERT OR REPLACE` / `ON CONFLICT` | `INSERT OR REPLACE` deletes then inserts, losing the original `id`, `rowid`, `created_at`, and triggering FTS delete+insert churn. A UNIQUE index on `(org_id, topic_key)` with `ON CONFLICT` would require the index to allow NULLs (SQLite treats each NULL as unique), making it useless for our case where topic_key is often NULL. Application-level check is explicit, testable, and the connection is already mutex-serialized. |
| 5 | normalized_hash | Compute in Rust: `hex(sha256(content.trim().to_lowercase()))` | SQLite trigger | `sha2` + `hex` crates already in `Cargo.toml`. Trigger-based hash would require loading a SQLite extension or custom function registration. Rust-side is simpler, testable, and consistent with existing patterns. |
| 6 | Type validation | Accept any string at API layer (no enum validation). Document known values but do not reject unknown ones. | Strict enum with 422 on unknown | Forward-compat: new Engram versions may add types. The DB column is TEXT. Validation can be added later via middleware if needed. |
| 7 | Session PATCH | Only `ended_at` and `summary` are updatable. Build SET clause dynamically (same pattern as `list_memories` filter building). | Full struct replacement | Spec says only these two fields. Dynamic SET avoids overwriting fields not sent. Follows existing dynamic-SQL pattern in `queries.rs`. |
| 8 | Session module | New `api/sessions.rs` handler + queries in `queries.rs` | Queries in separate `session_queries.rs` | Project has a single `queries.rs` file for all SQL. Follow existing convention. |

## Data Flow

```
POST /v1/memory/store (with topic_key)
    |
    v
memory::store handler
    |-- compute normalized_hash (sha2)
    |-- lock db mutex
    |-- queries::upsert_memory()
    |       |-- SELECT id FROM memories WHERE org_id=? AND topic_key=?
    |       |-- found? UPDATE content, title, type, scope, normalized_hash, revision_count+1
    |       |-- not found? INSERT with revision_count=1
    |-- log_audit
    |-- return 201 (insert) or 200 (update)

POST /v1/sessions
    |-- queries::create_session() -> INSERT into sessions
    |-- return 201 { id }

PATCH /v1/sessions/:id
    |-- queries::update_session() -> UPDATE sessions SET ... WHERE id=? AND org_id=?
    |-- 0 rows affected? 404
    |-- return 200
```

## File Changes

| File | Action | Description |
|------|--------|-------------|
| `apps/backend/src/db/migrations.rs` | Modify | Add `run_v2()` with ALTER TABLEs, sessions CREATE, FTS rebuild. Add `run_all()` entry point. Guard with `user_version`. |
| `apps/backend/src/db/queries.rs` | Modify | Add `upsert_memory()`, `create_session()`, `update_session()`, `validate_session_ownership()`. Extend `list_memories()` with `type`/`scope` filters. Extend `search_memories()` SELECT to include v2 columns. |
| `apps/backend/src/models/types.rs` | Modify | Add v2 fields to `Memory` struct. Add `Session` struct. |
| `apps/backend/src/api/memory.rs` | Modify | Extend `StoreInput` with optional v2 fields. Extend `ListParams` with `type`/`scope`. Add hash computation. Branch on topic_key for upsert. Return 200 on update, 201 on insert. |
| `apps/backend/src/api/sessions.rs` | Create | `create()` and `update()` handlers. |
| `apps/backend/src/api/router.rs` | Modify | Register `POST /v1/sessions` and `PATCH /v1/sessions/:id`. Add `mod sessions` to `api/mod.rs`. |
| `apps/backend/src/api/mod.rs` | Modify | Add `pub mod sessions;` |
| `apps/backend/src/main.rs` or startup | Modify | Call `migrations::run_all()` instead of `migrations::run()`. |

## Interfaces / Contracts

```rust
// models/types.rs — extended Memory
pub struct Memory {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub project: String,
    pub tool: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: String,
    // v2 fields
    pub title: Option<String>,
    pub r#type: Option<String>,
    pub scope: String,            // default "project"
    pub topic_key: Option<String>,
    pub session_id: Option<String>,
    pub revision_count: i64,      // default 1
    pub normalized_hash: Option<String>,
}

// models/types.rs — new Session
pub struct Session {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub directory: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub summary: Option<String>,
}
```

```sql
-- Migration v2 SQL (exact statements)
ALTER TABLE memories ADD COLUMN title TEXT;
ALTER TABLE memories ADD COLUMN type TEXT;
ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'project';
ALTER TABLE memories ADD COLUMN topic_key TEXT;
ALTER TABLE memories ADD COLUMN session_id TEXT REFERENCES sessions(id);
ALTER TABLE memories ADD COLUMN revision_count INTEGER NOT NULL DEFAULT 1;
ALTER TABLE memories ADD COLUMN normalized_hash TEXT;

CREATE TABLE IF NOT EXISTS sessions (
    id         TEXT PRIMARY KEY,
    org_id     TEXT NOT NULL REFERENCES organizations(id),
    project    TEXT NOT NULL,
    directory  TEXT NOT NULL DEFAULT '',
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at   TEXT,
    summary    TEXT
);

-- FTS rebuild (drop old, create new, backfill)
DROP TRIGGER IF EXISTS memories_ai;
DROP TRIGGER IF EXISTS memories_ad;
DROP TRIGGER IF EXISTS memories_au;
DROP TABLE IF EXISTS memories_fts;

CREATE VIRTUAL TABLE memories_fts USING fts5(
    content, tags, title, type,
    content='memories', content_rowid='rowid'
);

INSERT INTO memories_fts(rowid, content, tags, title, type)
    SELECT rowid, content, tags, title, type FROM memories;

-- Recreate triggers with 4 columns
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content, tags, title, type)
    VALUES (new.rowid, new.content, new.tags, new.title, new.type);
END;

CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, tags, title, type)
    VALUES ('delete', old.rowid, old.content, old.tags, old.title, old.type);
END;

CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, tags, title, type)
    VALUES ('delete', old.rowid, old.content, old.tags, old.title, old.type);
    INSERT INTO memories_fts(rowid, content, tags, title, type)
    VALUES (new.rowid, new.content, new.tags, new.title, new.type);
END;

PRAGMA user_version = 2;
```

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit | Migration v2 idempotency, v2 on fresh DB, v2 on already-migrated DB | `#[cfg(test)]` in `migrations.rs`, in-memory SQLite |
| Unit | Upsert: insert path, update path, org isolation, NULL topic_key always inserts | `#[cfg(test)]` in `queries.rs` |
| Unit | normalized_hash: same content = same hash, whitespace/case normalization | `#[cfg(test)]` in `queries.rs` |
| Unit | FTS includes title and type after migration | `#[cfg(test)]` in `queries.rs` |
| Unit | list_memories type/scope filters | `#[cfg(test)]` in `queries.rs` |
| Unit | Session create, update, wrong-org 404 | `#[cfg(test)]` in `queries.rs` |
| Integration | Full HTTP: store with v2 fields, upsert via API, session endpoints | `#[cfg(test)]` in `memory.rs` and `sessions.rs` |

## Migration / Rollout

1. `run_all()` calls `run_v1()` then `run_v2()`. Each checks `user_version`.
2. `run_v2()` creates `sessions` table BEFORE altering `memories` (because `session_id` FK references it).
3. Entire v2 migration wrapped in a single transaction. On failure, nothing changes.
4. Rollback: rusqlite bundles SQLite 3.45+ which supports `ALTER TABLE DROP COLUMN`. Revert code + drop columns + rebuild FTS to v1 schema.

## Open Questions

- None. All six design decisions are resolved.
