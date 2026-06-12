# Design: Backend Completeness

## 0. Context corrections to the proposal

The proposal was drafted against an older snapshot of the codebase. Before designing, two facts must be acknowledged:

1. **`memories.project_id` already exists.** It was added by `run_v6` in `db/migrations.rs` (line 327), and `run_v6` even backfills it from `memories.project`. There is no latent runtime bug — `SqliteStore::get()`'s SELECT already returns the column. What's actually missing is the **fresh-DB v1 schema parity** (the v1 `CREATE TABLE memories` doesn't list `project_id`, only the v6 ALTER does), and the **indexes** the proposal calls out. Gap #1 is therefore re-scoped: no schema change for `project_id`, but documentation and an idempotency guard to confirm it.
2. **Current `PRAGMA user_version` is 8**, not 2. The proposal calls the new migration "v3"; we ship it as **`run_v9`** to preserve the existing chain. All references to "v3" in the proposal map to v9 here.

These corrections are surgical — the eight gaps stand, the approach stands, only the migration number and the `project_id` framing change.

## 1. Architecture approach

This change does NOT introduce a new architectural pattern. It extends the existing layered backend:

```
HTTP (Axum handlers)  →  Store trait (SqliteStore)  →  Queries (raw rusqlite)  →  SQLite
                           ↑
                       Auth middleware
                       Rate limit middleware (NEW)
```

Decisions:

- **Keep the `MemoryStore` trait boundary.** New memory-side endpoints (`GET /v1/memory/:id`, `GET /v1/context/project/:project`) flow through `SqliteStore`, never reach into `queries::*` from handlers (except where existing patterns already do — `delete_memory` already calls `db_queries::get_memory_owner_and_project` for permission checks; new code follows the same convention).
- **Hash chain lives in `db/queries.rs`**, not in a new "audit service" module. Reason: the chain is a SQL-level invariant — read previous hash + insert new row in the same transaction. Adding a service layer would force two locks of the `Arc<Mutex<Connection>>` for one logical operation.
- **Rate limiter is an Axum `from_fn_with_state` middleware**, layered on the `protected` router AFTER `auth_mw::auth`. It reads the resolved `AuthContext` from request extensions so it knows which org/api-key/tier to throttle. No new trait, no new store; bucket state is a `Arc<DashMap<String, Bucket>>` held by the layer closure.
- **No new long-running tasks.** Bucket cleanup runs lazily on each request hit (check-and-prune-stale-if-needed inside `get_or_insert`) instead of a background tokio task, to keep `main.rs` small.

## 2. Component map

```
apps/backend/src/
├── api/
│   ├── audit.rs          [modify]  add post_audit handler
│   ├── context.rs        [new]     project-context aggregation handler
│   ├── memory.rs         [modify]  add get_by_id handler
│   ├── rate_limit.rs     [new]     token bucket + axum middleware
│   ├── router.rs         [modify]  wire new routes + layer rate_limit
│   └── mod.rs            [modify]  pub mod context; pub mod rate_limit;
├── db/
│   ├── migrations.rs     [modify]  add run_v9 to chain
│   └── queries.rs        [modify]  add insert_audit_log_chained,
│                                    get_memory_by_id, get_project_context;
│                                    delete store_memory()
├── models/
│   └── types.rs          [modify]  add previous_hash/current_hash to AuditEntry;
│                                    add ProjectContext + ExternalAuditRequest types
└── main.rs               [modify]  construct rate-limit state, pass to router::build

apps/backend/tests/
└── integration_test.rs   [modify]  migrate two store_memory() callers to upsert_memory()
```

## 3. Data flow per gap

### Gap 1 — project_id parity (re-scoped)

**No DDL change** beyond what v6 already shipped. `run_v9` includes a defensive `pragma_table_info` check identical to v6's so that a DB initialized via the v1+v6 path is identical to one initialized via a future "v1-with-project_id-baked-in" path. We do NOT modify the v1 `CREATE TABLE` — that would corrupt the migration history. The fresh-DB story is "run v1..v9 in sequence" and that's already correct.

### Gap 2 — GET /v1/memory/:id

```
client → Axum router → auth middleware → rate_limit middleware
       → memory::get_by_id handler
         → require_permission(conn, &auth, project_of(id), "memory:read")
         → SqliteStore::get(&auth.org_id, &id)  // already implemented
         → match Some(m) | None → 200 Json(m) | 404 ApiError
```

Tenant isolation reuses the `WHERE id = ?1 AND org_id = ?2` pattern from `SqliteStore::get`. Cross-tenant lookups return **404**, not 403 — the row doesn't exist *for this tenant*. This matches the convention already in `delete_memory_wrong_org_returns_false` and avoids leaking existence of rows owned by other orgs (timing-safe by accident, because both branches go through the same query).

Permission check order:
1. Fetch row via store (org-scoped). If `None` → 404 immediately.
2. Run `require_permission(conn, &auth, Some(&memory.project), "memory:read")` — this honours project-level role overrides exactly like `delete` does today.
3. Return 200 with the memory.

### Gap 3 — GET /v1/context/project/:project

Response schema (new struct `ProjectContext` in `models/types.rs`):

```rust
pub struct ProjectContext {
    pub project: String,                  // echoed param
    pub recent_memories: Vec<Memory>,     // last 20 by created_at DESC
    pub tools: Vec<String>,               // distinct tool values
    pub last_activity: Option<String>,    // MAX(created_at), ISO 8601
}
```

Three small queries, all scoped by `org_id` AND `project` name (NOT `project_id` — the request comes in with a human-readable project name, matching the `/v1/projects/:project_id/...` pattern used elsewhere; we lookup by `project` text column to keep the URL ergonomic and aligned with the proposal):

```sql
-- recent_memories
SELECT id, org_id, user_id, project, tool, content, tags, created_at,
       title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id
FROM memories
WHERE org_id = ?1 AND project = ?2
ORDER BY created_at DESC
LIMIT 20;

-- tools
SELECT DISTINCT tool FROM memories
WHERE org_id = ?1 AND project = ?2;

-- last_activity
SELECT MAX(created_at) FROM memories
WHERE org_id = ?1 AND project = ?2;
```

Permission: `require_permission(conn, &auth, Some(project), "memory:read")` — same gate the list endpoint uses. Empty project (no memories yet) returns `{ project, recent_memories: [], tools: [], last_activity: null }` with status 200, not 404 — the project may exist but have no activity yet.

### Gap 4 — POST /v1/audit/log (external ingest)

Request shape (new `ExternalAuditRequest` in `models/types.rs`):

```rust
pub struct ExternalAuditRequest {
    pub action: String,                          // required, e.g. "store" | "search" | custom
    pub resource_type: String,                   // required, e.g. "memory" | "tool_call"
    pub resource_id: Option<String>,
    pub metadata: Option<serde_json::Value>,     // arbitrary JSON, default {}
    pub timestamp: Option<String>,               // optional override; server fills if absent
}
```

Auth & assignment:
- `org_id` ← `auth.org_id` from the API-key auth middleware. Clients **cannot** specify it.
- `user_id` ← `auth.user_id` from the API-key auth middleware. The audit row represents "this API key took this action," not "this human did" — matching how internal `log_audit` calls work today.
- `timestamp` — if client provides ISO 8601, validate format with `chrono::DateTime::parse_from_rfc3339`; if absent, server stamps `datetime('now')`. Allowing client override is intentional for late-arriving batched events from external tools; tampering with timestamps is detected later via the hash chain (an out-of-order timestamp doesn't break the chain — `previous_hash` still binds the row to its predecessor by insertion order).

Permission: requires `audit:write` (a new permission string). Existing roles get `audit:write` added to admin only; member/viewer cannot ingest audit. Custom roles can be configured. Update `get_role_permissions` in `queries.rs` to include `audit:write` in the admin set.

Response: 201 Created with the persisted `AuditEntry` (now including `previous_hash` and `current_hash`).

Validation rejection (400) cases:
- `action` empty or > 64 chars
- `resource_type` empty or > 64 chars
- `metadata` larger than 16 KB serialized
- `timestamp` provided but unparseable RFC 3339

### Gap 5 — Audit hash chain

Schema additions in `audit_logs` (added by `run_v9`):

```sql
ALTER TABLE audit_logs ADD COLUMN previous_hash TEXT;
ALTER TABLE audit_logs ADD COLUMN current_hash  TEXT;
```

Both nullable initially. Rows written before v9 will have `previous_hash = NULL, current_hash = NULL` — they are the "genesis pre-chain" segment. The first row written after v9 begins the chain with `previous_hash = NULL` and a `current_hash` computed against an empty previous-hash byte string. This matches the rollback story in the proposal: existing rows remain valid SHA-256 over their own canonical record, and the chain simply resumes after re-applying.

Canonical record (deterministic, documented in spec):

```
canonical = utf8_bytes(timestamp)
         || 0x1F  // ASCII unit separator
         || utf8_bytes(action)
         || 0x1F
         || utf8_bytes(resource_type)
         || 0x1F
         || utf8_bytes(resource_id.unwrap_or(""))
         || 0x1F
         || utf8_bytes(metadata_json_compact)
```

`metadata_json_compact` is `serde_json::to_string(&metadata)` of the **stored** JSON value — the same string that goes into the `metadata` column. Using the stored form makes offline verification a direct string read; no re-canonicalization needed.

New transactional insert:

```rust
pub fn insert_audit_log_chained(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    metadata: serde_json::Value,
    timestamp_override: Option<&str>,
) -> Result<AuditEntry> {
    let tx = conn.unchecked_transaction()?;

    // 1. Read latest current_hash for this org (chain is per-tenant)
    let previous_hash: Option<String> = tx.query_row(
        "SELECT current_hash FROM audit_logs
         WHERE org_id = ?1 AND current_hash IS NOT NULL
         ORDER BY timestamp DESC, id DESC LIMIT 1",
        [org_id],
        |r| r.get(0),
    ).optional()?;

    // 2. Build canonical record + compute current_hash
    let id = Uuid::new_v4().to_string();
    let now = timestamp_override
        .map(String::from)
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let meta_str = serde_json::to_string(&metadata)?;

    let mut hasher = Sha256::new();
    if let Some(ref prev) = previous_hash {
        hasher.update(prev.as_bytes());
    }
    hasher.update([0x1F]);
    hasher.update(now.as_bytes());
    hasher.update([0x1F]);
    hasher.update(action.as_bytes());
    hasher.update([0x1F]);
    hasher.update(resource_type.as_bytes());
    hasher.update([0x1F]);
    hasher.update(resource_id.unwrap_or("").as_bytes());
    hasher.update([0x1F]);
    hasher.update(meta_str.as_bytes());
    let current_hash = hex::encode(hasher.finalize());

    // 3. INSERT inside the same transaction
    tx.execute(
        "INSERT INTO audit_logs
         (id, org_id, user_id, timestamp, action, resource_type, resource_id, metadata,
          previous_hash, current_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![id, org_id, user_id, now, action, resource_type, resource_id,
                          meta_str, previous_hash, current_hash],
    )?;
    tx.commit()?;

    Ok(AuditEntry { /* ... including previous_hash, current_hash */ })
}
```

**Why per-tenant chains, not one global chain.** Each org has its own chain because (a) cross-tenant info-leak — a global chain lets one tenant's hash be revealed in another tenant's verification, (b) the existing `list_audit` is already org-scoped, so verification flows naturally, (c) it avoids one global write-contention point.

**Why SELECT-then-INSERT is safe.** The connection is wrapped in `Arc<Mutex<Connection>>` (see `store/sqlite.rs:21`); only one write can hold the mutex at a time. The explicit transaction is belt-and-suspenders: it guarantees atomicity even if a future refactor moves to a multi-conn pool.

**Migrating `log_audit`**. The existing `log_audit` function (line 631 of queries.rs) keeps its signature and becomes a thin wrapper around `insert_audit_log_chained` with `timestamp_override = None`. Every existing call site (search, store, delete, etc. in `store/sqlite.rs`) keeps working unchanged. This is the smallest-blast-radius migration path.

**Update `AuditEntry`** (models/types.rs):

```rust
pub struct AuditEntry {
    // ... existing fields ...
    #[serde(default)]
    pub previous_hash: Option<String>,
    #[serde(default)]
    pub current_hash: Option<String>,
}
```

`#[serde(default)]` makes responses backward-compatible: pre-v9 rows serialize as `null` for both fields, post-v9 rows have real values.

Update `list_audit` and `list_all_audit` SELECTs to include both new columns.

### Gap 6 — Rate limiting

Module: `api/rate_limit.rs`.

State:

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;

#[derive(Clone)]
pub struct RateLimitState {
    pub buckets: Arc<DashMap<String, Bucket>>,
}

pub struct Bucket {
    pub tokens: f64,
    pub last_refill: Instant,
    pub last_seen: Instant,   // for idle eviction
}

#[derive(Clone, Copy)]
pub struct TierQuota {
    pub capacity: f64,        // max tokens
    pub refill_per_sec: f64,  // tokens added per second
}
```

**Bucket key.** Use `auth.user_id` rather than the api-key hash. Reasons:
1. The middleware sees `AuthContext` (org_id, user_id, role), not the raw api-key id; api-key id is not on `AuthContext`.
2. A user with rotated keys should keep the same bucket — rotation is operational, not a quota reset.
3. Simpler test path: tests can drive the limiter via the user_id from `bootstrap`.

If we want to throttle per-key in the future, we add `api_key_id` to `AuthContext` and switch the key — additive change, no design refactor.

**Tier resolution.** The proposal mentions tier per org "free / team / enterprise" but there is no `plan` column on `organizations`. Two options:

| Option | Pros | Cons |
|---|---|---|
| (A) Add `plan TEXT NOT NULL DEFAULT 'free'` to `organizations` in `run_v9` | Honest, complete | Expands the migration; admin UI not updated this change |
| (B) Hard-code a single tier in `run_v9`, read tier from env var `NEXUSMIND_RATE_TIER` (per-deployment) | Smallest blast radius, ships rate limiting immediately | Doesn't fulfill the multi-tenant tier story; one-tier-per-process |

**Decision: (A).** The proposal's success criterion mentions "per-tier quotas". A single env-var tier is half the feature. The plan column is one ALTER, one DEFAULT, zero data-loss risk. Admin UI editing of the plan is out of scope (proposal § Out of Scope says `apps/admin` untouched) — for now plans are set via internal `/internal/orgs/:id` PATCH (which already exists) or SQL.

Quotas as constants in `rate_limit.rs`:

```rust
const QUOTAS: &[(&str, TierQuota)] = &[
    ("free",       TierQuota { capacity: 100.0,   refill_per_sec: 100.0  / 60.0 }),
    ("team",       TierQuota { capacity: 1000.0,  refill_per_sec: 1000.0 / 60.0 }),
    ("enterprise", TierQuota { capacity: 10000.0, refill_per_sec: 10000.0/ 60.0 }),
];

fn quota_for(plan: &str) -> TierQuota {
    QUOTAS.iter()
        .find(|(name, _)| *name == plan)
        .map(|(_, q)| *q)
        .unwrap_or(QUOTAS[0].1) // default: free
}
```

Token-bucket algorithm (per request):

1. Get/insert bucket for `auth.user_id` in DashMap. On insert, `tokens = capacity`.
2. `elapsed = now - last_refill`; refill: `tokens = min(capacity, tokens + elapsed.as_secs_f64() * refill_per_sec)`; set `last_refill = now`.
3. If `tokens >= 1.0` → decrement by 1.0, set `last_seen = now`, call `next.run(req).await`.
4. Else → reject with 429, header `Retry-After: ceil((1.0 - tokens) / refill_per_sec)` seconds, body `ApiError { code: "rate_limited", error: "Rate limit exceeded" }`.

Middleware signature:

```rust
pub async fn rate_limit(
    State(state): State<RateLimitState>,
    Extension(auth): Extension<AuthContext>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, HeaderMap, Json<ApiError>)> { /* ... */ }
```

**Eviction.** Every 1024th hit (atomic counter on the state), sweep `buckets` and drop entries with `last_seen < now - 1h`. Keeps memory bounded without a background task. The sweep is `O(n)` but only runs once per ~1000 requests, and DashMap supports concurrent retain.

**Tier lookup cost.** Fetching `org.plan` from SQLite on every request is wasteful. Cache the tier on the bucket: when we insert a new bucket, do one SELECT `plan FROM organizations WHERE id = ?`; store `TierQuota` on the bucket. If the admin changes a plan, the new quota takes effect after the bucket is evicted (max 1h) — acceptable for OSS single-node.

Wiring in `router.rs`:

```rust
let rate_state = RateLimitState { buckets: Arc::new(DashMap::new()) };

let protected = Router::new()
    .route(/* ... existing routes ... */)
    .route("/v1/memory/:id", get(memory::get_by_id).delete(memory::delete))
    .route("/v1/context/project/:project", get(context::get_project_context))
    .route("/v1/audit/log", post(audit::post_audit))
    .layer(middleware::from_fn_with_state(rate_state.clone(), rate_limit::rate_limit))
    .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));
```

Order matters: `from_fn_with_state` layers run bottom-up. With auth layered last (outermost), auth runs first; rate_limit runs second and sees the populated `AuthContext`. Unauthenticated requests get 401 before they touch the rate limiter, so an unauth flood doesn't allocate buckets.

`main.rs` change is `+0` lines (router.rs owns the state) — `main.rs` only calls `api::router::build(conn, config)` today.

### Gap 7 — DB indexes

Added in `run_v9` after the audit-logs ALTERs:

```sql
CREATE INDEX IF NOT EXISTS idx_memories_scope      ON memories(scope);
CREATE INDEX IF NOT EXISTS idx_memories_type       ON memories(type);
CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_org_ts   ON audit_logs(org_id, timestamp DESC);
```

The fourth index (`org_id, timestamp DESC`) is a bonus from this analysis: the hash-chain SELECT (`WHERE org_id = ?1 ORDER BY timestamp DESC LIMIT 1`) runs on every audit insert, and existing `list_audit` already filters/orders the same way. Without it, every audit write triggers a full scan of `audit_logs` filtered by org. This is the highest-impact index in the set.

`scope` and `type` are low-cardinality, so the indexes mainly help when combined with `org_id` — SQLite's planner will choose them when a query is `WHERE org_id = ? AND scope = ?`. Confirmed via `EXPLAIN QUERY PLAN` in tests.

### Gap 8 — Remove `store_memory()`

Call sites (from grep):

| File | Lines | Action |
|---|---|---|
| `src/db/queries.rs` | 150 (declaration), 1761, 1780, 1893, 1907–08, 1920, 1943–45, 1964, 2102–04 | Delete declaration; migrate test callers to `upsert_memory` with a small helper |
| `tests/integration_test.rs` | 100, 126 | Migrate to `upsert_memory` |

No HTTP handler, MCP tool, or admin-UI path calls `store_memory()` directly — `SqliteStore::store()` calls `upsert_memory()` (queries.rs line 58 in store/sqlite.rs). Removal is safe.

Migration helper for tests:

```rust
fn legacy_store(conn: &Connection, org_id: &str, user_id: &str,
                project: &str, tool: &str, content: &str, tags: &[String]) -> Memory {
    let req = StoreMemoryRequest {
        project: Some(project.into()),
        tool: tool.into(),
        content: content.into(),
        tags: Some(tags.to_vec()),
        title: None, memory_type: None, scope: None, topic_key: None, session_id: None,
    };
    upsert_memory(conn, org_id, user_id, &req).unwrap()
}
```

Place this as a private test-mod helper (cfg(test)) in both `queries.rs` and `integration_test.rs`. Each call site becomes a one-line swap. No production behaviour change — `upsert_memory` without a `topic_key` always INSERTs (verified by the existing test `upsert_memory_no_topic_key_always_inserts`).

## 4. ADR-style decisions

### ADR-1: Migration number is v9, not v3

- **Context.** Proposal text says "Migration v3"; the codebase already runs v1..v8.
- **Decision.** Call the new migration `run_v9` and append it to `run_all`. Update `run_all_sets_user_version_to_8` test to expect 9.
- **Rejected.** "Squash v3..v8 and rename" — would break every existing deployment.
- **Consequence.** All proposal references to "v3" mean "v9" in code.

### ADR-2: 404 (not 403) for cross-tenant memory access

- **Context.** Proposal asks for "tenant isolation" on `GET /v1/memory/:id`.
- **Decision.** Return 404 when a memory ID belongs to a different tenant. Both "doesn't exist" and "exists but not yours" collapse to the same response.
- **Rejected.** 403 with explicit "owned by another org" — leaks existence of cross-tenant rows.
- **Consequence.** Matches existing `delete_memory_wrong_org_returns_false` behaviour. Single SELECT, single decision branch.

### ADR-3: Per-tenant audit chain, not global

- **Context.** Hash chain needs a notion of "previous hash."
- **Decision.** Chain is partitioned by `org_id`. Each org has an independent chain.
- **Rejected.** Single global chain across all orgs.
- **Consequence.** No cross-tenant info leak via verification. The first audit row for each new org has `previous_hash = NULL`. Two orgs writing concurrently never block on each other (different keys in the predecessor SELECT).

### ADR-4: Canonical record uses stored JSON form

- **Context.** The hash input must be deterministic.
- **Decision.** Use `serde_json::to_string(&metadata)` exactly once (when building the row) and reuse the same string for both the `metadata` column and the hash input.
- **Rejected.** Re-canonicalize (sort keys, fixed float format) — adds a `canonical-json` dependency and a class of "stored ≠ hashed" bugs.
- **Consequence.** Verifiers read the stored `metadata` string verbatim and hash that. Trivial round-trip.

### ADR-5: Rate-limit key is user_id, not api_key_id

- **Context.** Multiple plausible keys; both work.
- **Decision.** Use `auth.user_id`.
- **Rejected.** `api_key_id` (not in `AuthContext`); raw key hash (would punish key rotation).
- **Consequence.** Rotation does not reset the quota. Per-key throttling is a future additive change (add `api_key_id` to AuthContext, switch key).

### ADR-6: Tier stored in `organizations.plan` column

- **Context.** Proposal cites per-tier quotas; no column exists.
- **Decision.** Add `plan TEXT NOT NULL DEFAULT 'free'` in `run_v9`.
- **Rejected.** Env-var per process (fails multi-tenant story); separate `org_plans` table (over-engineered for a single string).
- **Consequence.** All existing orgs default to `free` on migration. Plan is mutable via existing `update_org` paths (or raw SQL); admin UI exposure is a future change.

### ADR-7: Bucket eviction is lazy, not background

- **Context.** Long-running process; map could grow unbounded.
- **Decision.** Sweep stale buckets opportunistically every Nth request (atomic counter, N=1024).
- **Rejected.** Background tokio task (adds `main.rs` complexity and shutdown coordination).
- **Consequence.** No tokio task added. Sweep is O(n) but amortized across requests. Worst case: a burst of requests that all hit the eviction tick get one slow request; acceptable.

### ADR-8: `log_audit` keeps its signature

- **Context.** `log_audit` is called in many places (`store/sqlite.rs`, several queries).
- **Decision.** Keep `log_audit` as a public function; reimplement it as a thin wrapper over `insert_audit_log_chained`.
- **Rejected.** Rename + migrate every call site.
- **Consequence.** Minimum diff outside the audit module. All existing audit writes automatically join the chain.

### ADR-9: Project context queried by name, not id

- **Context.** URL is `/v1/context/project/:project` — `:project` is a path segment.
- **Decision.** Treat `:project` as the project name (TEXT column on `memories`), the same way `?project=` works on `GET /v1/memory`.
- **Rejected.** Treat as UUID — would force callers to fetch the project ID first, breaking ergonomic URL usage.
- **Consequence.** Consistent with the existing list endpoint. Permission check uses the project name (same as `delete_memory` does).

## 5. Affected files (file-by-file)

| File | Change |
|---|---|
| `apps/backend/Cargo.toml` | Add `dashmap = "5"` to dependencies |
| `apps/backend/src/db/migrations.rs` | Add `pub fn run_v9(conn)`. Append to `run_all`. ALTER `audit_logs` add `previous_hash TEXT`, `current_hash TEXT`. ALTER `organizations` add `plan TEXT NOT NULL DEFAULT 'free'`. CREATE 4 indexes. Set `user_version = 9`. Update `run_all_sets_user_version_to_8` test to expect 9 (or rename to `run_all_sets_user_version_to_9`). |
| `apps/backend/src/db/queries.rs` | Add `insert_audit_log_chained`; rewrite `log_audit` as wrapper. Add `get_memory_by_id_for_org` (thin wrapper around the existing SqliteStore::get logic, exposed at the queries layer for handlers that need owner+project before calling the store — or skip and use store.get directly). Add `get_project_context` returning `(Vec<Memory>, Vec<String>, Option<String>)`. Add `"audit:write"` to admin permissions in `get_role_permissions`. Update `list_audit` + `list_all_audit` SELECTs to include `previous_hash, current_hash`. **Delete `store_memory()`** (line 150). Migrate `cfg(test)` callers via `legacy_store` helper. |
| `apps/backend/src/store/sqlite.rs` | No interface change. Existing `SqliteStore::get` already returns the right row. |
| `apps/backend/src/api/memory.rs` | Add `pub async fn get_by_id(State, Extension<AuthContext>, Path<String>) -> Result<Json<Memory>, ApiError(404\|403)>`. Reuses `store.get()` and `require_permission`. |
| `apps/backend/src/api/context.rs` | NEW. `pub async fn get_project_context(State, Extension<AuthContext>, Path<String>) -> Result<Json<ProjectContext>, ApiError>`. Three queries via a single `queries::get_project_context` helper. |
| `apps/backend/src/api/audit.rs` | Add `pub async fn post_audit(State, Extension<AuthContext>, Json<ExternalAuditRequest>) -> Result<(StatusCode::CREATED, Json<AuditEntry>), ApiError>`. Validation per Gap 4 § "Validation rejection". Calls `insert_audit_log_chained`. |
| `apps/backend/src/api/rate_limit.rs` | NEW. `RateLimitState`, `Bucket`, `TierQuota`, `quota_for(plan)`, `rate_limit` middleware fn. Lazy eviction. Tier cache on bucket. |
| `apps/backend/src/api/router.rs` | Construct `RateLimitState`. Add routes: `GET /v1/memory/:id`, `GET /v1/context/project/:project`, `POST /v1/audit/log`. Layer `rate_limit` middleware inside `protected` (below auth in code, so it runs after auth at runtime). |
| `apps/backend/src/api/mod.rs` | `pub mod context; pub mod rate_limit;` |
| `apps/backend/src/models/types.rs` | Add `previous_hash: Option<String>` and `current_hash: Option<String>` to `AuditEntry` (with `#[serde(default)]`). Add `pub struct ProjectContext { project, recent_memories, tools, last_activity }`. Add `pub struct ExternalAuditRequest { action, resource_type, resource_id?, metadata?, timestamp? }`. |
| `apps/backend/tests/integration_test.rs` | Replace `queries::store_memory` calls at lines 100 and 126 with the local `legacy_store` helper (or inline `upsert_memory`). Add integration tests for the four new routes per the Success Criteria. |

## 6. API contract changes

### New endpoints

| Method | Path | Auth | Permission | Success | Notable errors |
|---|---|---|---|---|---|
| GET | `/v1/memory/:id` | required | `memory:read` (project-aware) | 200 `Memory` | 404, 403 |
| GET | `/v1/context/project/:project` | required | `memory:read` for that project | 200 `ProjectContext` | 403 |
| POST | `/v1/audit/log` | required | `audit:write` (admin by default) | 201 `AuditEntry` | 400 validation, 403 |

### Modified responses

- **`GET /v1/audit`** — entries now include `previous_hash` and `current_hash` fields. Both are nullable for backward-compat with pre-v9 rows. No breaking change for clients ignoring unknown fields.
- **All endpoints under `/v1/*` (protected group)** — may now return `429 Too Many Requests` with `Retry-After` header when the per-user bucket is empty. Body shape is the standard `ApiError { error, code: "rate_limited" }`.

### No breaking changes

- `Memory` struct fields unchanged.
- All existing 2xx responses unchanged.
- Existing audit clients see two extra nullable fields — additive only.

## 7. Tradeoffs & risks (architectural)

| Concern | Tradeoff taken | Why acceptable |
|---|---|---|
| Hash chain breaks if an admin deletes an audit row | We don't prevent deletes — chain just "skips" | Audit-row deletion is itself an audit event in mature systems; out of scope for this change |
| `plan` column added but admin UI doesn't expose it | Inconsistency for ~1 release | Admin UI is explicitly out of scope per proposal; internal API can set it via direct DB until next change |
| Rate limit is per-user, but enterprise plans usually want per-org | Slight quota mismatch | Switch is one line (`auth.user_id` → `auth.org_id`); we can add per-org as a future ADR |
| Lazy eviction may miss a sweep cycle under steady-state low traffic | Stale entries hold memory for hours | Worst case ~few MB; acceptable for OSS single-node |
| `project_id` parity assumption: a fresh-DB v1 schema doesn't list `project_id` | Run-time path requires v1+v6 — must run all migrations | Already the case; `run` calls `run_all`; no production deployment skips v6 |
| Timestamp client-override on `POST /v1/audit/log` allows backdating | Chain still binds insertion order via `previous_hash` | Verifier can flag out-of-order timestamps; full signature scheme is future work |

## 8. Validation strategy (high-level)

Detailed test plan lives in spec/tasks. Architecturally, four properties must hold:

1. **Chain continuity.** Inserting N audit rows, then iterating the table by insertion order, recomputing `sha256(prev_hash || canonical)` for each row must yield the stored `current_hash` for every row.
2. **Tenant isolation.** Inserting N rows in org A and M rows in org B must produce two independent chains; no cross-tenant hashing.
3. **Idempotency of v9.** Running `run_v9` twice on a v8 DB must succeed and leave the DB byte-identical the second time (`PRAGMA user_version = 9` both times; no duplicate columns; no duplicate index errors thanks to `IF NOT EXISTS`).
4. **Rate limit fairness.** After 100 requests in 1 minute on `free`, request 101 returns 429 with `Retry-After`. After waiting the indicated seconds, the next request succeeds.

## 9. Open questions / explicit non-decisions

- **Should the chain hash also cover `user_id`?** Current canonical record does not. Including it ties a row's hash to its actor; excluding it means actor can be edited without invalidating the chain. Decision: **exclude for now** to match the proposal's documented canonical fields; revisit if/when signatures land.
- **Should `audit:write` be granted to `member` by default?** No — external tool tokens that need to ingest audit should be issued as a dedicated role or explicit custom role with that permission. Keeps the default least-privilege.
- **Per-org rate limiting** is not in this change. The token-bucket key is `user_id`; switching to org_id is a future surgical change.
