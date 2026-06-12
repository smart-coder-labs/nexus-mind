# Proposal: Backend Completeness

## Intent

The NexusMind backend has reached 98 passing tests but a recent audit against the PRD and API specification surfaced eight concrete completeness gaps that block external integrations and weaken tamper-evidence guarantees. The `memories` table is missing the `project_id` column its own `SELECT` statement already references (a latent runtime bug), the GET-by-id and project-context endpoints are not wired into the router, audit logs cannot be written by external tools, the audit hash chain promised by the PRD is absent, no per-API-key rate limiter exists, several frequently-filtered columns lack indexes, and a deprecated `store_memory()` helper still ships in `queries.rs`.

Closing these gaps now — before we expand the MCP surface or onboard the first external tool integrations — keeps the backend honest with its own spec, prevents a silent SQL failure when `project_id` filtering ships, and gives us the tamper-evident audit trail and rate-limit enforcement the multi-tenant story already advertises. It also clears the dead code path that has been confusing contributors who keep finding two ways to write a memory.

## Scope

### In Scope
- Migration v3: add `project_id TEXT REFERENCES projects(id)` to `memories`; add `previous_hash TEXT` and `current_hash TEXT` to `audit_logs`; create indexes `idx_memories_scope`, `idx_memories_type`, `idx_memories_project_id`.
- New HTTP handler `GET /v1/memory/:id` exposed via `api/memory.rs`, wired in `router.rs`.
- New module `api/context.rs` exposing `GET /v1/context/project/:project` — returns recent memories, distinct tools touched, last activity timestamp for the named project.
- New handler `POST /v1/audit/log` (in `api/audit.rs`) so external tools can ingest audit records; reuses the same hash-chain logic as internal writers.
- SHA-256 audit hash chain in `db/queries.rs`: every insert reads the latest `current_hash`, computes `sha256(previous_hash || canonical_record)`, and persists both fields atomically.
- New middleware module `api/rate_limit.rs` providing per-API-key token-bucket limiting at the documented tiers (100/min OSS, 1000/min team, 10000/min enterprise); wired in `main.rs` before route handlers.
- Removal of the deprecated `store_memory()` function in `queries.rs` and any references in `tests/integration_test.rs`; callers migrate to the canonical insert path.
- Tests covering: migration v3 idempotency, GET-by-id (happy + 404 + tenant isolation), project context shape, audit POST authentication, hash-chain continuity across N inserts, rate-limit enforcement and reset, index presence, removal of dead code path.

### Out of Scope
- Vector / semantic search.
- Distributed rate limiting (single-node in-memory bucket is sufficient for this change).
- Audit log signature/PKI — hash chain only; signatures are a future change.
- Admin UI surfaces for any of these endpoints (`apps/admin` untouched).
- Backfilling `project_id` for memories created before migration v3 (column allowed NULL; explicit backfill is a separate change).
- Changes to the MCP server surface beyond what naturally follows from the new HTTP routes.

## Capabilities

### New Capabilities
- `memory-fetch-by-id`: tenant-scoped GET-by-id endpoint with project_id projection now that the column exists.
- `project-context`: aggregated view of recent activity for a single project.
- `audit-ingest`: external write endpoint for audit records.
- `audit-hash-chain`: tamper-evident SHA-256 chain over the `audit_logs` table.
- `rate-limiting`: per-API-key token-bucket enforcement with tier-aware quotas.

### Modified Capabilities
- `memory-storage`: schema now includes `project_id`; existing `SELECT` in `SqliteStore::get()` becomes correct; insert path accepts optional `project_id`.
- `audit-read`: existing `GET /v1/audit` continues to work; responses now include `previous_hash` / `current_hash` for clients that want to verify the chain.
- `memory-list/search`: faster filtered queries thanks to new indexes; no API shape change.

## Approach

1. **Migration v3** (`db/migrations.rs`): one new function `run_v3`, gated on `user_version < 3`. Performs additive `ALTER TABLE` for the four new columns and `CREATE INDEX IF NOT EXISTS` for the three indexes, all inside a transaction. Bumps `user_version` to 3. Idempotent on reruns.
2. **Hash chain** (`db/queries.rs`): introduce `insert_audit_log(...)` that, inside the same transaction, (a) SELECTs the most recent `current_hash` for the tenant scope, (b) computes `sha256(previous_hash_bytes || canonical_record_bytes)` using the `sha2` crate already present, (c) INSERTs the row with both hashes. Canonical record is a deterministic concatenation of `(timestamp, actor, action, resource, payload_json)` — documented in the spec.
3. **GET /v1/memory/:id** (`api/memory.rs`): thin handler delegating to existing `SqliteStore::get()`, returning 404 when the row does not belong to the authenticated tenant. Registered in `router.rs` alongside the existing memory routes.
4. **Project context** (`api/context.rs` new file): handler issues three small queries — recent memories (LIMIT 20 by `created_at DESC`), `SELECT DISTINCT tool`, and `SELECT MAX(created_at)` — all scoped by `project_id` and tenant. Returns a single JSON payload `{ memories, tools, last_activity }`.
5. **Audit POST** (`api/audit.rs`): new `post_audit` handler validates required fields, calls `insert_audit_log`, returns the persisted record including both hashes. Reuses the standard API-key auth extractor.
6. **Rate limiter** (`api/rate_limit.rs` new file + `main.rs` wiring): per-API-key token bucket stored in `Arc<DashMap<ApiKeyId, Bucket>>`. Tier resolved from the existing API key lookup; quotas come from a small const table. On exhaustion returns `429` with `Retry-After`. Wired as an Axum `from_fn_with_state` middleware layer applied to the protected route group, before handlers but after auth.
7. **Dead code removal** (`db/queries.rs`, `tests/integration_test.rs`): delete `store_memory()`, migrate any test that still calls it to the canonical insert path, run `cargo test` to confirm green.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `apps/backend/src/db/migrations.rs` | Modified | Add `run_v3` for new columns and indexes |
| `apps/backend/src/db/queries.rs` | Modified | Add `insert_audit_log` with hash chain; remove `store_memory()` |
| `apps/backend/src/api/memory.rs` | Modified | Add `get_memory_by_id` handler |
| `apps/backend/src/api/context.rs` | New | `GET /v1/context/project/:project` handler |
| `apps/backend/src/api/audit.rs` | New or Modified | Add `POST /v1/audit/log` handler |
| `apps/backend/src/api/rate_limit.rs` | New | Per-API-key token-bucket middleware |
| `apps/backend/src/api/router.rs` | Modified | Register GET-by-id, context, audit POST routes |
| `apps/backend/src/main.rs` | Modified | Wire rate-limit middleware into the router stack |
| `apps/backend/src/models/types.rs` | Modified | Add `project_id` to `Memory`, hash fields to `AuditLog` |
| `apps/backend/tests/integration_test.rs` | Modified | Cover new routes, migration, hash chain, rate limiting; drop `store_memory()` callers |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Hash chain becomes inconsistent under concurrent audit writes | Med | Wrap select-previous + insert in a single SQLite transaction; the existing mutex-wrapped connection serializes writers |
| Migration v3 leaves a partially-updated DB if a step fails | Low | Run all ALTER + CREATE INDEX in a single transaction; bump `user_version` last |
| In-memory rate limiter resets on process restart, briefly allowing burst | Low | Acceptable for single-node OSS deployment; document explicitly and mark distributed limiter as future work |
| Token-bucket map grows unboundedly with churned API keys | Low | Periodic cleanup task that evicts buckets idle > 1h; alternatively cap map size with LRU |
| External tools begin writing audit logs with malformed payloads | Med | Strict JSON schema validation in `POST /v1/audit/log`; reject with 400 before touching the chain |
| `project_id` added as NULLable column means old rows fail strict joins | Low | Keep column NULLable; downstream queries use LEFT JOIN or filter `WHERE project_id IS NOT NULL` when projection requires it |
| Removing `store_memory()` breaks an external caller we forgot about | Low | Grep the whole repo before deletion; `store_memory` was never exposed via HTTP/MCP, only internal |

## Rollback Plan

1. Revert the `run_v3` migration function. SQLite >= 3.35 supports `ALTER TABLE DROP COLUMN`; the bundled rusqlite ships 3.45+, so we can drop `project_id`, `previous_hash`, `current_hash` cleanly. Drop the three new indexes with `DROP INDEX IF EXISTS`.
2. Revert handler additions in `router.rs` — clients hitting the removed routes get 404 again, no data corruption.
3. Remove the rate-limit middleware layer; existing auth path is unaffected.
4. Restore `store_memory()` from git history if needed (this is the cheapest step; do it only if an unforeseen internal caller is found).
5. Audit rows written during the rolled-back window keep their hashes — they are still valid SHA-256 over their own canonical record; the chain just terminates there until v3 is re-applied.

## Dependencies

- `sha2` crate (already present from memory-schema-v2).
- `dashmap` crate for concurrent rate-limit bucket map (new dependency; minimal footprint, already widely used in the Rust ecosystem).
- No external services. No client-side coordination required.

## Success Criteria

- [ ] `cargo test` continues to pass with all 98 prior tests green plus the new tests added in this change.
- [ ] Migration v3 applies idempotently on a fresh DB and on an existing v2 DB; `user_version` ends at 3.
- [ ] `GET /v1/memory/:id` returns the row for the owning tenant and 404 for any other tenant or unknown id.
- [ ] `GET /v1/context/project/:project` returns `{ memories, tools, last_activity }` scoped to the caller's tenant and the named project.
- [ ] `POST /v1/audit/log` persists a record, returns both `previous_hash` and `current_hash`, and a follow-up `GET /v1/audit` shows the new row in chain order.
- [ ] Verifying the chain offline (replay SHA-256 over canonical records) reproduces every stored `current_hash`.
- [ ] Rate limiting returns 429 with `Retry-After` after the per-tier quota is exhausted and resumes serving after the bucket refills.
- [ ] `EXPLAIN QUERY PLAN` on filtered memory queries uses `idx_memories_scope`, `idx_memories_type`, and `idx_memories_project_id` where applicable.
- [ ] `rg "store_memory\("` returns zero hits in `apps/backend/`.
