# Spec: backend-completeness

## Scope

Delta for capabilities:

**New**: `memory-fetch-by-id`, `project-context`, `audit-ingest`, `audit-hash-chain`, `rate-limiting`

**Modified**: `memory-storage`, `audit-read`, `memory-list/search`

**Removed**: `store_memory()` dead code

---

# memory-storage (Modified)

## MODIFIED Requirements

### Requirement: project_id Column in memories Table

The `memories` table MUST have a `project_id TEXT` column, nullable, with a foreign
key reference to `projects(id)`. Migration v3 MUST add this column via `ALTER TABLE`
inside a transaction. The column MUST be nullable to preserve backward compatibility
with rows created before migration v3.

`POST /v1/memory/store` MUST accept an optional `project_id` field and persist it.
Queries that reference `project_id` (including `SELECT` in `SqliteStore::get()`) MUST
work correctly once v3 has run.

(Previously: `memories` had no `project_id` column; references to it in `get()` were
a latent SQL failure.)

#### Scenario: Store memory with project_id

- GIVEN a valid API key and a project `"nexusmind"` exists
- WHEN `POST /v1/memory/store` is called with `project_id: "nexusmind"`
- THEN the memory is saved with `project_id = "nexusmind"`
- AND the response body includes `project_id: "nexusmind"`

#### Scenario: Store memory without project_id remains valid

- GIVEN a valid API key
- WHEN `POST /v1/memory/store` is called without `project_id`
- THEN the memory is saved with `project_id = NULL`
- AND HTTP 201 is returned

#### Scenario: Existing memories survive migration v3

- GIVEN a v2 database with 10 memories (no project_id column)
- WHEN migration v3 runs
- THEN all 10 memories remain readable with `project_id = NULL`
- AND no data is lost

---

# memory-fetch-by-id (New)

## Purpose

Tenant-scoped retrieval of a single memory record by its primary key.

## Requirements

### Requirement: GET /v1/memory/:id Handler

The system MUST expose `GET /v1/memory/:id` via `api/memory.rs`, registered in
`router.rs`. The handler MUST delegate to `SqliteStore::get()`, scoped to the
authenticated tenant's `org_id`. The response MUST include all memory fields,
including `project_id`.

The system MUST return HTTP 404 when the `id` does not exist OR belongs to a
different tenant.

#### Scenario: Fetch own memory by id

- GIVEN org A has a memory with `id = "m1"`
- WHEN org A calls `GET /v1/memory/m1`
- THEN HTTP 200 is returned with the full memory record

#### Scenario: Unknown id returns 404

- GIVEN no memory with `id = "ghost"` exists
- WHEN any authenticated tenant calls `GET /v1/memory/ghost`
- THEN HTTP 404 is returned

#### Scenario: Tenant isolation — cannot read another org's memory

- GIVEN org A has memory `id = "m1"`
- WHEN org B calls `GET /v1/memory/m1`
- THEN HTTP 404 is returned (not 403 — tenant existence MUST NOT be leaked)

---

# project-context (New)

## Purpose

Aggregated view of recent activity for a single project, scoped to the calling tenant.

## Requirements

### Requirement: GET /v1/context/project/:project Endpoint

The system MUST expose `GET /v1/context/project/:project` via `api/context.rs`.
The handler MUST query the authenticated tenant's memories scoped by `project_id`.
The response MUST be a JSON object `{ memories, tools, last_activity }` where:
- `memories`: up to 20 most-recent memories ordered by `created_at DESC`
- `tools`: array of distinct tool values from those memories
- `last_activity`: ISO-8601 timestamp of the most-recent `created_at`, or `null` if no memories exist

#### Scenario: Project with memories returns correct shape

- GIVEN org A has 5 memories with `project_id = "nexusmind"` and various tools
- WHEN org A calls `GET /v1/context/project/nexusmind`
- THEN HTTP 200 is returned
- AND `memories` contains up to 20 records, ordered newest first
- AND `tools` contains distinct tool values (no duplicates)
- AND `last_activity` equals the `created_at` of the newest memory

#### Scenario: Project with no memories returns empty shape

- GIVEN org A has no memories with `project_id = "empty-proj"`
- WHEN org A calls `GET /v1/context/project/empty-proj`
- THEN HTTP 200 is returned
- AND `memories` is an empty array, `tools` is an empty array, `last_activity` is `null`

#### Scenario: Cross-tenant isolation

- GIVEN org A has memories for `project_id = "shared-name"`
- WHEN org B calls `GET /v1/context/project/shared-name`
- THEN org B receives only its own memories, not org A's

---

# audit-ingest (New)

## Purpose

External tools can write audit records through the HTTP API using the same hash-chain
logic as internal writers.

## Requirements

### Requirement: POST /v1/audit/log Handler

The system MUST expose `POST /v1/audit/log` via `api/audit.rs`, registered in
`router.rs`. The handler MUST require a valid API key (standard auth extractor).

The request body MUST include `actor`, `action`, and `resource`. `payload` (JSON object)
is optional. The system MUST reject missing required fields with HTTP 400 before
touching the audit chain.

The response MUST include the persisted record with both `previous_hash` and
`current_hash` fields populated. The record MUST be visible in subsequent
`GET /v1/audit` responses, ordered within the chain.

#### Scenario: Valid audit record is ingested

- GIVEN a valid API key
- WHEN `POST /v1/audit/log` is called with `{ actor, action, resource }`
- THEN HTTP 201 is returned
- AND the response body includes `previous_hash` and `current_hash`
- AND `GET /v1/audit` subsequently returns the new record

#### Scenario: Missing required field returns 400

- GIVEN a valid API key
- WHEN `POST /v1/audit/log` is called without `action`
- THEN HTTP 400 is returned
- AND no audit row is written

#### Scenario: Unauthenticated request returns 401

- GIVEN no API key is provided
- WHEN `POST /v1/audit/log` is called
- THEN HTTP 401 is returned

---

# audit-hash-chain (New)

## Purpose

Every audit log insert computes a SHA-256 hash chaining the previous record's hash
to the new record's canonical representation, providing tamper-evidence guarantees.

## Requirements

### Requirement: Hash Columns in audit_logs

The `audit_logs` table MUST have `previous_hash TEXT` and `current_hash TEXT` columns.
Migration v3 MUST add these columns in the same transaction as the memories changes.

### Requirement: SHA-256 Chain Computation on Insert

Every insert into `audit_logs` — whether from an internal writer or `POST /v1/audit/log`
— MUST use `insert_audit_log()` in `db/queries.rs`. The function MUST:

1. Within a single transaction, SELECT the most-recent `current_hash` for the tenant scope.
2. Compute `sha256(previous_hash_bytes || canonical_record_bytes)` where the canonical record is a deterministic concatenation of `(timestamp, actor, action, resource, payload_json)`.
3. INSERT the row with `previous_hash` set to the fetched `current_hash` (or empty bytes if first record) and `current_hash` set to the computed hash.
4. Return the persisted row including both hash fields.

The function MUST be the single authoritative path for all audit writes.

#### Scenario: First audit record bootstraps the chain

- GIVEN no audit records exist for the tenant
- WHEN an audit record is inserted
- THEN `previous_hash` is the SHA-256 of an empty byte string (or a documented zero value)
- AND `current_hash` is `sha256(previous_hash || canonical_record)`

#### Scenario: Sequential inserts form a valid chain

- GIVEN N audit records have been inserted for a tenant
- WHEN a further record is inserted
- THEN its `previous_hash` equals the `current_hash` of record N
- AND replaying `sha256(previous_hash || canonical_record)` over all rows reproduces every stored `current_hash`

#### Scenario: Concurrent writes do not corrupt the chain

- GIVEN two audit writes are issued concurrently for the same tenant
- WHEN both complete
- THEN the chain has exactly 2 new records, each correctly linking to the previous

#### Scenario: Cross-tenant chain isolation

- GIVEN org A has audit records forming chain A
- WHEN org B inserts an audit record
- THEN org B's record chains only from org B's last record; org A's chain is unaffected

---

# rate-limiting (New)

## Purpose

Per-API-key token-bucket enforcement applied before route handlers, with tier-aware
quotas and 429 responses on exhaustion.

## Requirements

### Requirement: Per-API-Key Token Bucket Middleware

The system MUST provide a rate-limit middleware in `api/rate_limit.rs`, wired in
`main.rs` before route handlers but after auth. The middleware MUST maintain one
token bucket per API key in an `Arc<DashMap<ApiKeyId, Bucket>>`.

Tier quotas MUST be:
- OSS: 100 requests/minute
- Team: 1000 requests/minute
- Enterprise: 10000 requests/minute

The tier MUST be resolved from the existing API key lookup.

On quota exhaustion the system MUST return HTTP 429 with a `Retry-After` header
indicating seconds until the bucket refills. Requests MUST resume normally once the
bucket refills.

The rate-limit state is in-memory only. A process restart MUST NOT be treated as an
error; the bucket resets are acceptable and MUST be documented.

#### Scenario: Request within quota is served

- GIVEN an OSS API key with fewer than 100 requests in the current minute
- WHEN a request is made
- THEN the request is processed normally (no 429)
- AND the bucket count is decremented by 1

#### Scenario: Request exceeding quota returns 429

- GIVEN an OSS API key that has exhausted its 100 req/min quota
- WHEN a further request is made
- THEN HTTP 429 is returned
- AND the response includes a `Retry-After` header with a positive integer value

#### Scenario: Bucket refills after window elapses

- GIVEN an OSS key that previously returned 429
- WHEN the minute window elapses and a new request is made
- THEN HTTP 200 is returned (quota restored)

#### Scenario: Tier isolation — team key has higher quota

- GIVEN a Team API key (1000 req/min) and an OSS key (100 req/min)
- WHEN the OSS key is exhausted and the Team key is used
- THEN the Team key's requests continue to succeed

---

# memory-list/search (Modified)

## MODIFIED Requirements

### Requirement: DB Indexes on memories Table

The database MUST have three additional indexes on the `memories` table, created
by migration v3:
- `idx_memories_scope` on column `scope`
- `idx_memories_type` on column `type`
- `idx_memories_project_id` on column `project_id`

All indexes MUST be created with `CREATE INDEX IF NOT EXISTS` inside the migration v3
transaction to ensure idempotency.

(Previously: no dedicated indexes on these columns; filtered queries performed full
table scans.)

#### Scenario: Filtered query uses index

- GIVEN migration v3 has run
- WHEN `EXPLAIN QUERY PLAN` is run on a filtered memory query using `scope`, `type`, or `project_id`
- THEN the plan references the corresponding index (`idx_memories_scope`, `idx_memories_type`, or `idx_memories_project_id`)

#### Scenario: Indexes survive a re-run of migration v3

- GIVEN migration v3 was already applied
- WHEN the backend restarts and migrations run again
- THEN no error is raised and indexes remain present

---

# audit-read (Modified)

## MODIFIED Requirements

### Requirement: Audit List Response Includes Hash Fields

`GET /v1/audit` MUST include `previous_hash` and `current_hash` in every returned
audit record once migration v3 has run. Clients that do not read these fields are
unaffected. No change to existing filter or pagination parameters.

(Previously: audit responses included no hash fields.)

#### Scenario: GET /v1/audit returns hash fields

- GIVEN at least one audit record exists after migration v3
- WHEN `GET /v1/audit` is called with a valid API key
- THEN each record in the response includes non-null `previous_hash` and `current_hash`

#### Scenario: Legacy clients ignoring hash fields still work

- GIVEN a client that only reads `actor`, `action`, `resource` from audit responses
- WHEN `GET /v1/audit` is called
- THEN the client receives HTTP 200 and can read its expected fields without error

---

# dead-code-removal (Removed Behavior)

## REMOVED Requirements

### Requirement: store_memory() Internal Helper

(Reason: `store_memory()` in `db/queries.rs` is a deprecated path superseded by the
canonical insert function. It has never been exposed via HTTP or MCP. Removing it
eliminates contributor confusion around two parallel write paths. All callers in
`tests/integration_test.rs` MUST be migrated to the canonical insert path before
deletion. `rg "store_memory\("` in `apps/backend/` MUST return zero hits after this
change.)

---

# Migration v3 (Cross-cutting)

## Requirements

### Requirement: Migration v3 Idempotency and Atomicity

Migration v3 MUST be implemented as a single function `run_v3` in
`db/migrations.rs`, gated by `user_version < 3`. All schema changes — the
`ALTER TABLE` additions to `memories` and `audit_logs`, and the three `CREATE INDEX`
statements — MUST execute inside a single transaction. `user_version` MUST be bumped
to 3 only after all changes succeed. A failure at any step MUST roll back the entire
transaction, leaving the database at v2.

#### Scenario: v3 applies to a fresh database

- GIVEN an empty SQLite file
- WHEN the backend starts and all migrations run
- THEN the database is at `user_version = 3`
- AND `memories` has `project_id`, `audit_logs` has `previous_hash` and `current_hash`, and all three indexes exist

#### Scenario: v3 applies to an existing v2 database

- GIVEN a database at `user_version = 2` with existing rows
- WHEN migration v3 runs
- THEN `user_version` becomes 3, all existing rows are preserved, and new columns default to NULL

#### Scenario: Re-running v3 is a no-op

- GIVEN a database already at `user_version = 3`
- WHEN migrations run again
- THEN no error is raised and schema is unchanged
