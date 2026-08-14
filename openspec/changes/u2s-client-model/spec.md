# Spec — u2s Client Model (consultancy grouping)

> **Change**: `u2s-client-model`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-08-13

This spec defines the contracts: data model, HTTP endpoints, permissions, visibility semantics, inheritance resolution, promotion, and migration. The "how" lives in `design.md`.

---

## 1. Data Contract

### 1.1 Table `clients`

```sql
CREATE TABLE IF NOT EXISTS clients (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  slug        TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'active'
              CHECK(status IN ('active','paused','offboarded')),
  archived_at TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(org_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_clients_org_status ON clients(org_id, status);
```

**Constraints**

- `id` is a UUIDv4 string generated server-side at create time. A request body MUST NOT set `id` or `org_id`; the server derives `org_id` from `AuthContext.org_id`.
- `name` MUST be 1–128 chars, non-empty after trim.
- `slug` MUST match `^[a-z0-9][a-z0-9-]{0,63}$`. It is the stable external identifier and is **immutable after create** — renaming a client changes `name` only.
- `status` is enforced by the `CHECK` constraint **and** validated at the handler layer, so the API returns 400 rather than 500.
- `archived_at` is the soft-delete marker, matching the existing `projects.archived_at` convention. Timestamps use `datetime('now')`, matching `projects` (its sibling table), not the ISO-8601-with-ms form used by the v9 audit columns.

### 1.2 Table `client_members`

```sql
CREATE TABLE IF NOT EXISTS client_members (
  id         TEXT PRIMARY KEY,
  client_id  TEXT NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role       TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(client_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_client_members_user ON client_members(user_id);
```

- `role` reuses the `UserRole` vocabulary already parsed by `project_members` (`admin` | `member` | `viewer` | custom). No new vocabulary is introduced.
- Membership is **additive with `project_members`**: effective visibility is the union. Client membership grants visibility over every project of that client; project membership grants visibility over that project only.

### 1.3 Altered columns

```sql
ALTER TABLE projects      ADD COLUMN client_id     TEXT REFERENCES clients(id) ON DELETE RESTRICT;
ALTER TABLE code_projects ADD COLUMN project_id    TEXT REFERENCES projects(id);
ALTER TABLE conventions   ADD COLUMN client_id     TEXT REFERENCES clients(id) ON DELETE CASCADE;
ALTER TABLE policies      ADD COLUMN client_id     TEXT REFERENCES clients(id) ON DELETE CASCADE;
ALTER TABLE memories      ADD COLUMN promoted_from TEXT REFERENCES memories(id);

CREATE INDEX IF NOT EXISTS idx_projects_client      ON projects(org_id, client_id);
CREATE INDEX IF NOT EXISTS idx_conventions_client   ON conventions(org_id, client_id);
CREATE INDEX IF NOT EXISTS idx_policies_client      ON policies(org_id, client_id, enabled);
CREATE INDEX IF NOT EXISTS idx_code_projects_project ON code_projects(project_id);
```

- **`projects.client_id IS NULL` means an internal u2s project.** This is a load-bearing semantic, not an unset value, and MUST NOT be backfilled to a sentinel client.
- `ON DELETE RESTRICT` on `projects.client_id`: deleting a client that still owns projects MUST fail. Offboarding is `status = 'offboarded'`, never a cascade.
- `code_projects.project_id` is nullable so pre-existing rows remain valid. The 1:1 rule (one repo per project) is enforced at the handler layer, returning 409 on a second link to the same `project_id`.

### 1.4 `memories.scope`

The accepted domain widens from `project | personal` to **`org | client | project | personal`**. The column already carries `NOT NULL DEFAULT 'project'`; there is no `CHECK` constraint today and none is added, so validation stays at the handler layer (400 on an unknown value).

| `scope` | Visible to |
|---|---|
| `org` | every member of the organization |
| `client` | members of the owning client, plus members of any of its projects |
| `project` | members of the owning project, plus members of its client |
| `personal` | the authoring user only (unchanged) |

---

## 2. HTTP Contracts

All routes are org-scoped through `AuthContext`. Bodies never carry `org_id`.

### 2.1 `GET /v1/clients`

Query: `include_archived` (bool, default `false`), `status` (optional filter).
Returns clients **visible to the caller** (§4). `200` with `{ "clients": [...] }`.

### 2.2 `POST /v1/clients`

Body: `{ "name": string, "slug": string, "status"?: string }` → `201` with the created client.
`409` when `(org_id, slug)` already exists. `400` on slug pattern or status violation.

### 2.3 `PATCH /v1/clients/:id`

Body: any of `{ "name"?, "status"? }`. **`slug` is rejected with 400.**
`404` via `hidden_resource_not_found` when the client exists but is not visible to the caller.

### 2.4 `POST /v1/clients/:id/archive`

Sets `archived_at`. Idempotent. Archived clients are excluded from `GET /v1/clients` unless `include_archived=true`, and MUST NOT accept new projects.

### 2.5 `GET|POST|DELETE /v1/clients/:id/members`

Mirrors the existing project-members endpoints in shape and error envelope.

### 2.6 `POST /v1/memories/:id/promote`

Body: `{ "note"?: string }` → `201` with the newly created org-scoped memory.

- Source MUST have `scope IN ('client','project')`. Otherwise `400`.
- Creates a **new** memory with `scope = 'org'`, `client_id`/project association cleared, and `promoted_from = <source id>`.
- The source memory is left untouched.
- Requires `memory:write` **and** visibility of the source (§4). Never invoked implicitly by any other endpoint.

### 2.7 Existing endpoints — additive changes only

- `POST /v1/projects` accepts an optional `client_id`. Omitted or `null` ⇒ internal u2s project.
- `GET /v1/context/:project` and `GET /v1/context` resolve conventions through the three-level chain (§5). Response shape is unchanged.
- Convention and policy create/update accept an optional `client_id`.

---

## 3. Permissions

Two new permission strings, following the existing `<resource>:<action>` vocabulary:

| Permission | Guards |
|---|---|
| `client:read` | `GET /v1/clients`, `GET /v1/clients/:id/members` |
| `client:write` | create, patch, archive, member add/remove |

Promotion requires `memory:write`. No existing permission string changes meaning.

> **Note on `require_permission`.** `api/helpers.rs:137` short-circuits on `auth.role.is_privileged()`, which is `admin || super_user`. That governs **permission checks only**. It MUST NOT be reused for client visibility — see §4.

---

## 4. Visibility & Isolation Semantics

This is the core contract of the change.

1. **Only `super_user` has org-wide visibility.** `admin` is privileged for permission checks but remains membership-scoped for data reads. The codebase already states this (`models/types.rs`, `UserRole::is_super_user`) and `viewer_scope` (`api/context.rs:35`) already implements it by returning `None` for super_user and `Some(user_id)` otherwise. Client visibility MUST use the same discriminator — `is_super_user()`, **not** `is_privileged()`.

2. **A new helper `user_can_view_client(conn, org_id, client_id, viewer_user_id) -> Result<bool>` mirrors `user_can_view_project_name`** (`db/queries.rs:3088`), including its existence-hiding behaviour: when the client does not exist, it returns `true`, so a caller cannot distinguish "absent" from "forbidden" by response code alone.

3. **Every denied cross-client read returns 404 through `hidden_resource_not_found`** (`api/helpers.rs:19`), which writes an audit row with action `resource.hidden_access_denied` and `resource_type = "client"`. A 403 is never returned for a hidden resource, because the status code itself would confirm existence.

4. **Read paths that MUST enforce client scope**: memory list/get/search, convention list/get, policy list, project list/get, context (project and global), code project list/search, audit log read, task and sprint list.

5. **Cross-client isolation is fail-closed.** An unresolvable client association on a row makes it invisible to non-super_user callers rather than visible by default.

---

## 5. Inheritance Resolution Semantics

Conventions and policies resolve **org → client → project**, additively.

- **Org-level** (`client_id IS NULL AND project_id IS NULL`) applies everywhere.
- **Client-level** (`client_id = X AND project_id IS NULL`) applies to every project of client X, **in addition to** org-level.
- **Project-level** (`project_id = P`) applies to project P, **in addition to** the two above.

A narrower level MUST NOT remove or replace a broader one. Ordering within the merged set continues to use `weight` as `list_conventions_visible` already does (`db/queries.rs:14497`), and the existing `MAX_CONTEXT_CONVENTIONS` cap of 50 applies to the merged result.

For an internal u2s project (`client_id IS NULL`), the chain collapses to **org → project**; no client level exists and this is not an error.

---

## 6. Migration Semantics (`run_v58`)

Idempotent, guarded on `PRAGMA user_version` like every prior migration. Current version is `v57`.

1. Create `clients` and `client_members`; add the columns and indexes in §1.3. Each `ALTER TABLE` is a separate statement, tolerating `duplicate column` errors per the established pattern.
2. **`github_connections` rebuild.** SQLite cannot alter a primary key in place. Create `github_connections_new` with `PRIMARY KEY (org_id, client_id, github_login)` and `client_id` nullable, copy every row with `client_id = NULL`, drop the old table, rename. **Row count before and after MUST match**, and the migration MUST abort on mismatch.
3. **Token encryption.** During the copy, re-write `access_token` through `token_cipher::encrypt` (`api/code.rs:37`). Rows whose token fails to encrypt MUST abort the migration rather than being copied in plaintext.
4. **No project backfill.** `memories.project` is not rewritten by the migration. Resolution is a separate, explicitly-invoked operation (§7).

The write path in `db/queries.rs:14860` (`INSERT OR REPLACE INTO github_connections`) MUST be changed to encrypt before insert, in the same change. A migration that encrypts history while the write path still stores plaintext is a regression, not a fix.

---

## 7. Project Resolution (reporting, not mutation)

A read-only operation reports how existing `memories.project` values map to `projects.name`:

- **Exact string match only.** No case folding, no fuzzy matching, no prefix heuristics.
- Output: resolved count, unresolved count, and the distinct unresolved values with their row counts.
- **It MUST NOT write.** Assigning `project_id` and `client_id` to legacy memories is a separate operator action, out of scope here.

---

## 8. Error Envelope

Unchanged — the existing `ApiError` shape. Status codes used by this change:

| Code | Condition |
|---|---|
| `400` | slug pattern, unknown status, unknown `scope`, `slug` in PATCH body, promoting a non-client-scoped memory |
| `403` | permission missing (`client:read` / `client:write`) on an otherwise visible resource |
| `404` | resource absent **or** hidden from the caller (§4.3) |
| `409` | duplicate `(org_id, slug)`; second `code_projects` link to the same `project_id` |
| `422` | deleting a client that still owns projects (`ON DELETE RESTRICT`) |

---

## 9. Non-Functional Requirements (this change only)

- Client-scope filtering MUST NOT add an N+1 query to list endpoints; visibility resolves in the same SQL statement, as `list_conventions_visible` already does.
- `run_v58` MUST complete in under 5 s on a database with 100k memories.
- No endpoint response shape changes, so existing MCP tools and the admin panel keep working without modification.

---

## 10. Acceptance Criteria

1. A user who is a member of client A only receives **404** — not 403 — on every read path in §4.4 targeting client B, and each attempt writes a `resource.hidden_access_denied` audit row.
2. A `super_user` sees every client; an `admin` without membership does **not**.
3. An org-level convention appears in `GET /v1/context/:project` for a project of any client and for an internal u2s project, without being duplicated in the database.
4. A client-level convention appears only for projects of that client, alongside — never instead of — the org-level ones.
5. Creating a project with no `client_id` succeeds and yields an internal u2s project whose context resolves org → project.
6. Deleting a client that owns projects fails with `422`; archiving it succeeds and blocks new projects.
7. `promote_memory` on a client-scoped memory creates an org-scoped copy with `promoted_from` set, leaves the source unchanged, and rejects a source with `scope = 'org'` or `'personal'` with `400`.
8. After `run_v58`, `grep`-ing the raw `.db` file for a known plaintext token yields **no match**, and `github_connections` row count is identical to the pre-migration count.
9. Two clients with different `github_login` values coexist in `github_connections` without overwriting each other.
10. `run_v58` is re-runnable: applying it twice leaves the schema and data unchanged.
11. Project resolution reports counts without mutating a single row.
