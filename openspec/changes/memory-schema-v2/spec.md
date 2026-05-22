# Spec: memory-schema-v2

## Scope

Delta for capabilities: `memory-upsert` (new), `session-tracking` (new),
`memory-taxonomy` (new), `memory-storage` (modified), `memory-search` (modified).

No pre-existing domain specs — full specs written for all five.

---

# memory-taxonomy Specification

## Purpose

Structured metadata fields on the `memories` table: `type`, `title`, `scope`.

## Requirements

### Requirement: Type Field

The `memories` table MUST have a `type` TEXT column (nullable, default NULL).
Valid values: `architecture`, `bugfix`, `decision`, `discovery`, `config`,
`pattern`, `feedback`, `preference`, `project`, `session_summary`, `feature`,
`refactoring`, `manual`.
The system MAY accept NULL and unknown type values without error (forward-compat).

#### Scenario: Store memory with valid type

- GIVEN a valid API key
- WHEN `POST /v1/memory/store` is called with `type: "bugfix"`
- THEN the memory is saved with `type = "bugfix"`
- AND the response includes `type: "bugfix"`

#### Scenario: Store memory without type

- GIVEN a valid API key
- WHEN `POST /v1/memory/store` is called without a `type` field
- THEN the memory is saved with `type = NULL`
- AND existing clients receive no error

### Requirement: Title Field

The `memories` table MUST have a `title` TEXT column (nullable, default NULL).
`POST /v1/memory/store` MUST accept an optional `title` field and persist it.

#### Scenario: Store memory with title

- GIVEN a valid API key
- WHEN `POST /v1/memory/store` is called with `title: "JWT auth middleware"`
- THEN the memory is saved with that title
- AND `GET /v1/memory` response entries include the `title` field

### Requirement: Scope Field

The `memories` table MUST have a `scope` TEXT NOT NULL DEFAULT `'project'` column.
Valid values: `project`, `personal`.

#### Scenario: Default scope

- GIVEN a valid API key
- WHEN `POST /v1/memory/store` is called without a `scope` field
- THEN the memory is saved with `scope = "project"`

#### Scenario: Explicit personal scope

- GIVEN a valid API key
- WHEN `POST /v1/memory/store` is called with `scope: "personal"`
- THEN the memory is saved with `scope = "personal"`

### Requirement: Type and Scope Filters on List

`GET /v1/memory` MUST accept optional query params `type` and `scope`.
When provided, results MUST be restricted to memories matching the given value(s).
Filters MUST be combinable.

#### Scenario: Filter by type

- GIVEN memories exist with types `bugfix` and `decision`
- WHEN `GET /v1/memory?type=bugfix` is called
- THEN only `bugfix` memories are returned

#### Scenario: Filter by scope

- GIVEN memories with `scope=project` and `scope=personal` exist
- WHEN `GET /v1/memory?scope=personal` is called
- THEN only `personal` memories are returned

#### Scenario: Combined filter

- GIVEN various memories exist
- WHEN `GET /v1/memory?type=bugfix&scope=project` is called
- THEN only memories with both `type=bugfix` AND `scope=project` are returned

#### Scenario: No matching results

- GIVEN no memories have `type=config`
- WHEN `GET /v1/memory?type=config` is called
- THEN an empty list is returned with HTTP 200

---

# memory-upsert Specification

## Purpose

Topic-key-based upsert: when a memory with the same `(org_id, topic_key)` already
exists, UPDATE it instead of INSERT. Tracks revision count and content dedup via hash.

## Requirements

### Requirement: Upsert on topic_key Match

When `topic_key` is provided in `POST /v1/memory/store`, the system MUST check for
an existing memory with the same `(org_id, topic_key)`. If found, it MUST UPDATE
`content`, `title`, `type`, `normalized_hash`, and increment `revision_count` by 1.
If not found, it MUST INSERT a new row with `revision_count = 1`.

The `memories` table MUST have a `topic_key` TEXT column (nullable).
The `memories` table MUST have a `revision_count` INTEGER NOT NULL DEFAULT 1 column.

#### Scenario: First store with topic_key inserts

- GIVEN no memory exists with `topic_key = "arch/auth-model"` for the org
- WHEN `POST /v1/memory/store` with `topic_key: "arch/auth-model"` is called
- THEN a new memory row is inserted with `revision_count = 1`

#### Scenario: Second store with same topic_key updates

- GIVEN a memory exists with `topic_key = "arch/auth-model"` and `revision_count = 1`
- WHEN `POST /v1/memory/store` with the same `topic_key` and new content is called
- THEN the existing row is updated (no new row)
- AND `revision_count` becomes 2
- AND the response reflects the updated content

#### Scenario: topic_key is org-scoped

- GIVEN org A has a memory with `topic_key = "k1"`
- WHEN org B stores a memory with `topic_key = "k1"`
- THEN org B gets a new row; org A's memory is unchanged

#### Scenario: No topic_key — always inserts

- GIVEN any state
- WHEN `POST /v1/memory/store` is called without `topic_key`
- THEN a new row is always inserted regardless of content similarity

### Requirement: Normalized Hash for Dedup Detection

The `memories` table MUST have a `normalized_hash` TEXT column (nullable).
On every INSERT or UPDATE, the system MUST compute SHA-256 of
`content.trim().to_lowercase()` and store it in `normalized_hash`.
The hash is informational — the system MUST NOT reject duplicate content.

#### Scenario: Hash computed on insert

- GIVEN a new memory is stored
- WHEN `GET /v1/memory` returns the memory
- THEN the row has a non-null `normalized_hash`

#### Scenario: Same content produces same hash

- GIVEN two stores with content `"  Hello World  "` and `"hello world"`
- WHEN both are stored (without topic_key)
- THEN both rows have the same `normalized_hash`

---

# session-tracking Specification

## Purpose

Sessions group memories under a logical work unit. Memories MAY reference a session.

## Requirements

### Requirement: Sessions Table

The database MUST have a `sessions` table with columns:
`id TEXT PRIMARY KEY`, `org_id TEXT NOT NULL REFERENCES organizations(id)`,
`project TEXT NOT NULL`, `directory TEXT NOT NULL DEFAULT ''`,
`started_at TEXT NOT NULL DEFAULT (datetime('now'))`, `ended_at TEXT`,
`summary TEXT`.

### Requirement: Create Session

`POST /v1/sessions` MUST create a session row and return `{ "id": "<id>" }`.
`project` is REQUIRED. `directory` and `summary` are optional.

#### Scenario: Create session

- GIVEN a valid API key
- WHEN `POST /v1/sessions` is called with `{ "project": "nexusmind" }`
- THEN HTTP 201 is returned with `{ "id": "<generated-id>" }`
- AND the session exists in the database for the calling org

#### Scenario: Missing project returns error

- GIVEN a valid API key
- WHEN `POST /v1/sessions` is called without `project`
- THEN HTTP 422 is returned

### Requirement: Update Session

`PATCH /v1/sessions/:id` MUST accept `ended_at` and/or `summary` fields and update them.
The session MUST belong to the calling org. Updating a non-existent session or one
belonging to another org MUST return HTTP 404.

#### Scenario: Close session with summary

- GIVEN session `s1` exists for org A
- WHEN `PATCH /v1/sessions/s1` with `{ "ended_at": "2026-01-01T00:00:00Z", "summary": "Done" }`
- THEN HTTP 200 is returned
- AND `ended_at` and `summary` are persisted

#### Scenario: Wrong org cannot update

- GIVEN session `s1` belongs to org A
- WHEN org B calls `PATCH /v1/sessions/s1`
- THEN HTTP 404 is returned

### Requirement: Memory-Session Linkage

The `memories` table MUST have a `session_id` TEXT column (nullable),
REFERENCES `sessions(id)`.
`POST /v1/memory/store` MUST accept an optional `session_id` field.
If provided, it MUST be validated as belonging to the calling org; otherwise HTTP 422.

#### Scenario: Link memory to session

- GIVEN session `s1` exists for org A
- WHEN `POST /v1/memory/store` with `session_id: "s1"` is called by org A
- THEN the memory is saved with `session_id = "s1"`

#### Scenario: Invalid session_id rejected

- GIVEN no session with id `"ghost"` exists for org A
- WHEN `POST /v1/memory/store` with `session_id: "ghost"` is called
- THEN HTTP 422 is returned

---

# memory-storage (Modified) Specification

## Purpose

`POST /v1/memory/store` accepts new optional fields introduced in schema v2.
Existing callers that omit new fields MUST continue to work without change.

## Requirements

### Requirement: Backwards-Compatible Optional Fields

The store endpoint MUST accept `type`, `title`, `topic_key`, `scope`, `session_id`
as optional fields. All default to NULL / schema default if absent.
Existing required fields (`content`, `project`) remain unchanged.

#### Scenario: Legacy request succeeds

- GIVEN a client sends only `{ "content": "...", "project": "p" }`
- WHEN `POST /v1/memory/store` is called
- THEN HTTP 201 is returned with `scope = "project"`, `type = NULL`, `title = NULL`

#### Scenario: Full v2 request succeeds

- GIVEN all new fields are provided
- WHEN `POST /v1/memory/store` is called
- THEN HTTP 201 is returned and all fields are persisted

### Requirement: Migration Idempotency

The v2 migration MUST run without error on a fresh database and on a database that
has already run v2. Re-running MUST NOT duplicate columns, tables, or triggers.

#### Scenario: Fresh database

- GIVEN an empty SQLite file
- WHEN the backend starts and runs all migrations
- THEN `memories` has all v2 columns and `sessions` table exists

#### Scenario: Already-migrated database

- GIVEN a database where v2 migration was already applied
- WHEN migrations run again
- THEN no error is raised and schema is unchanged

---

# memory-search (Modified) Specification

## Purpose

FTS index expanded to include `title` and `type` so search matches on metadata.

## Requirements

### Requirement: FTS Includes title and type

The `memories_fts` virtual table MUST index the columns `content`, `tags`, `title`, `type`.
The system MUST rebuild (drop + recreate) this table and its triggers during migration v2.
Backfill MUST populate `title` and `type` from existing rows after recreate.

#### Scenario: Search matches on title

- GIVEN a memory with `title = "JWT auth middleware"` and unrelated `content`
- WHEN `POST /v1/memory/search` with `query: "JWT auth"` is called
- THEN that memory appears in results

#### Scenario: Search matches on type

- GIVEN a memory with `type = "bugfix"` and unrelated content and title
- WHEN `POST /v1/memory/search` with `query: "bugfix"` is called
- THEN that memory appears in results

#### Scenario: Existing content search still works

- GIVEN a memory whose content contains the word "migration"
- WHEN `POST /v1/memory/search` with `query: "migration"` is called
- THEN that memory appears in results

#### Scenario: FTS backfill after migration

- GIVEN an existing database with 10 memories, all with NULL title
- WHEN migration v2 runs
- THEN all 10 memories are indexed in the new FTS table
- AND searches on their content return results as before
