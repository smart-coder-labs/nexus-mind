# Design — u2s Client Model (consultancy grouping)

> **Change**: `u2s-client-model`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-08-13

This document is the implementation blueprint. It assumes the reader has read `proposal.md` (the "why") and `spec.md` (the "what"). It describes the "how": file layout, the visibility predicate, the migration rebuild, Rust signatures, and the TDD order.

---

## 1. Architecture overview

The change inserts one level into a hierarchy that already exists, and one predicate into queries that already filter by visibility.

```
organizations
     │
     ├── clients                    ← NEW
     │      └── projects (client_id NOT NULL)
     │
     └── projects (client_id NULL)  ← internal u2s work

visibility(user) = super_user                     → everything
                 ∪ project_members(user)          → those projects
                 ∪ client_members(user)           → every project of those clients
```

The critical design property: **the visibility rule is written once and reused**, not copied into each query. Today `user_can_view_project_name` (`db/queries.rs:3088`) and `list_sessions_visible` (`db/queries.rs:3113`) each embed their own `EXISTS` clause. Adding a third membership path to hand-written duplicates is how isolation bugs get shipped — one query gets updated, another does not, and the gap is invisible until a client sees another client's data.

So this change introduces a single canonical SQL fragment and routes every visibility check through it.

---

## 2. File-by-file change list

| File | Change |
|---|---|
| `db/migrations.rs` | `run_v58` — tables, columns, indexes, `github_connections` rebuild + encryption |
| `db/queries.rs` | `VISIBLE_PROJECT_IDS` fragment; `user_can_view_client`; client CRUD; membership queries; rewrite `user_can_view_project_name`, `list_sessions_visible`, `list_conventions_visible` to use the fragment; encrypt on `github_connections` write; `promote_memory`; `report_project_resolution` |
| `models/types.rs` | `Client`, `ClientStatus`, `ClientMember`, `CreateClientRequest`, `UpdateClientRequest`, `PromoteMemoryRequest`, `ProjectResolutionReport` |
| `api/clients.rs` | **new** — client CRUD + members handlers |
| `api/memory.rs` | `promote_memory` handler; widen `scope` validation |
| `api/context.rs` | three-level convention resolution |
| `api/policy.rs` | client-level policy resolution |
| `api/projects.rs` | accept optional `client_id` on create |
| `api/code.rs` | link `project_id`; keep `token_cipher::encrypt` usage as the reference |
| `api/router.rs` | mount `/v1/clients` routes |
| `api/middleware.rs` / RBAC | register `client:read`, `client:write` |

---

## 3. The visibility predicate (the crux)

One constant, used everywhere:

```rust
/// Set of project ids a viewer may see. `?org` = org_id, `?uid` = viewer user id.
/// Callers that pass `viewer_user_id = None` (super_user) MUST skip this filter
/// entirely rather than substituting a wildcard.
pub const VISIBLE_PROJECT_IDS: &str = "
    SELECT p.id FROM projects p
    WHERE p.org_id = :org
      AND (
            EXISTS (SELECT 1 FROM project_members pm
                     WHERE pm.project_id = p.id AND pm.user_id = :uid)
         OR EXISTS (SELECT 1 FROM client_members cm
                     WHERE cm.client_id = p.client_id AND cm.user_id = :uid)
      )
";
```

`p.client_id` is `NULL` for internal u2s projects, so the second `EXISTS` yields no rows for them — an internal project is visible only through direct project membership. That is intentional: internal work is not automatically visible to everyone, it follows the same membership rule as client work.

`user_can_view_client` mirrors `user_can_view_project_name` exactly, **including its existence-hiding branch**:

```rust
pub fn user_can_view_client(
    conn: &Connection,
    org_id: &str,
    client_id: &str,
    viewer_user_id: Option<&str>,
) -> Result<bool> {
    let Some(vid) = viewer_user_id else { return Ok(true) };   // super_user
    let visible: i64 = conn.query_row(
        "SELECT CASE
                  WHEN NOT EXISTS (SELECT 1 FROM clients c WHERE c.org_id = ?1 AND c.id = ?2) THEN 1
                  WHEN EXISTS (SELECT 1 FROM client_members cm
                                WHERE cm.client_id = ?2 AND cm.user_id = ?3) THEN 1
                  WHEN EXISTS (SELECT 1 FROM projects p
                                JOIN project_members pm ON pm.project_id = p.id
                                WHERE p.org_id = ?1 AND p.client_id = ?2 AND pm.user_id = ?3) THEN 1
                  ELSE 0
                END",
        rusqlite::params![org_id, client_id, vid],
        |row| row.get(0),
    )?;
    Ok(visible != 0)
}
```

The `NOT EXISTS … THEN 1` branch looks wrong at first glance and is deliberate: it makes "absent" and "forbidden" indistinguishable to the caller, matching the established behaviour. Removing it would turn every 404 into an existence oracle.

**The `viewer_scope` discriminator is `is_super_user()`, never `is_privileged()`.** `api/context.rs:35` already implements this correctly; the new code follows it. `require_permission` keeps using `is_privileged()` — permissions and visibility are separate axes and this change must not conflate them.

---

## 4. Migration `run_v58`

Guarded on `PRAGMA user_version` like every predecessor. Three stages.

**Stage 1 — additive.** `CREATE TABLE IF NOT EXISTS` for `clients` and `client_members`; each `ALTER TABLE … ADD COLUMN` as its own statement with `let _ =` to tolerate `duplicate column` on re-run; indexes with `IF NOT EXISTS`.

**Stage 2 — `github_connections` rebuild.** SQLite cannot alter a primary key in place.

```sql
CREATE TABLE github_connections_new (
  org_id         TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  client_id      TEXT REFERENCES clients(id) ON DELETE CASCADE,
  github_login   TEXT NOT NULL DEFAULT '',
  access_token   TEXT NOT NULL,          -- ciphertext after this migration
  token_type     TEXT NOT NULL DEFAULT 'bearer',
  scopes         TEXT NOT NULL DEFAULT '',
  github_user_id INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (org_id, client_id, github_login)
);
```

Rows are copied **in Rust, not in SQL**, because each `access_token` must pass through `token_cipher::encrypt` (`api/code.rs:37`). The loop:

1. `SELECT COUNT(*)` before → `n_before`.
2. Read each row; encrypt; insert into `_new`. **If encryption returns `None` for any row, abort the whole migration** — copying a plaintext token forward would defeat the change.
3. `SELECT COUNT(*)` from `_new` → `n_after`. If `n_before != n_after`, abort.
4. `DROP TABLE github_connections; ALTER TABLE github_connections_new RENAME TO github_connections;`

All of it inside one transaction so an abort leaves the original table intact.

> **`NEXUSMIND_TOKEN_ENCRYPTION_KEY` must be present for stage 2.** If it is unset, `token_cipher::encrypt` cannot produce ciphertext and the migration MUST fail loudly at startup rather than silently skipping encryption. This makes the key a hard deployment dependency — recorded as such for the AWS module change.

**Stage 3 — write path.** `db/queries.rs:14860` currently does `INSERT OR REPLACE INTO github_connections … ` with the raw token. It is changed to encrypt first. Stage 2 without stage 3 is a regression: history gets encrypted while every new write re-introduces plaintext.

---

## 5. Rust models

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Client {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub archived_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub name: String,
    pub slug: String,
    #[serde(default = "default_active_status")]
    pub status: String,
}

/// `slug` is intentionally absent — it is immutable after create (spec §2.3).
#[derive(Debug, Deserialize)]
pub struct UpdateClientRequest {
    pub name: Option<String>,
    pub status: Option<String>,
}
```

`default_active_status()` already exists in `models/types.rs`. Slug validation is a free function `validate_slug(&str) -> Result<(), String>` so it can be unit-tested without a DB.

**Resolved open question — `client_members.role` reuses `UserRole`.** `require_permission` (`helpers.rs:141`) already parses a member role string into `UserRole` via `get_project_member_role`. A second vocabulary would mean a second parse path and a second branch in permission resolution, for no expressive gain: the roles a consultancy needs at the client level (admin / member / viewer) are exactly the ones that exist. `get_client_member_role` mirrors `get_project_member_role` in shape and return type.

---

## 6. Inheritance resolution

`list_conventions_visible` (`db/queries.rs:14497`) gains a `client: Option<&str>` parameter. The `WHERE` clause becomes additive across three levels:

```sql
WHERE org_id = ?1
  AND (
        (client_id IS NULL AND project_id IS NULL)          -- org level
     OR (client_id = :client AND project_id IS NULL)        -- client level
     OR (project_id = :project)                             -- project level
  )
```

When the project has no client, `:client` binds to `NULL` and the middle branch matches nothing — the chain collapses to org → project, which is correct for internal u2s work and is not an error path.

Ordering by `weight` and the `MAX_CONTEXT_CONVENTIONS` cap of 50 apply to the merged result, unchanged. Policy resolution in `api/policy.rs` takes the identical shape.

---

## 7. Promotion

`promote_memory(conn, org_id, source_id, actor_user_id, note) -> Result<Memory>`:

1. Load source; reject unless `scope IN ('client','project')` → 400.
2. Insert a **new** memory: same `title`/`content`/`type`, `scope = 'org'`, `project`/`project_id`/`client_id` cleared, `promoted_from = source_id`, `user_id = actor_user_id`.
3. Write an audit row with action `memory.promoted`.
4. Return the new memory. **The source is never modified.**

Deliberately absent: any content rewriting. The spec puts sanitization out of scope; a half-working automatic redactor is worse than an explicit human decision, because it invites trust it has not earned.

---

## 8. Router & RBAC wiring

```rust
.route("/v1/clients", get(clients::list).post(clients::create))
.route("/v1/clients/:id", patch(clients::update))
.route("/v1/clients/:id/archive", post(clients::archive))
.route("/v1/clients/:id/members", get(clients::list_members).post(clients::add_member))
.route("/v1/clients/:id/members/:user_id", delete(clients::remove_member))
.route("/v1/memories/:id/promote", post(memory::promote))
```

`client:read` and `client:write` are added to the default role permission sets alongside the existing `project:*` entries.

---

## 9. Tests — TDD order

`openspec/config.yaml` sets `strict_tdd: true` with `tdd_scope: backend_and_admin`. Tests are written **before** the implementation, in this order:

**9.1 Migration** (`db/migrations.rs` test module)
- `run_v58_creates_clients_and_members`
- `run_v58_is_idempotent` — apply twice, assert schema and row counts unchanged
- `run_v58_rebuilds_github_connections_preserving_row_count`
- `run_v58_encrypts_existing_tokens` — assert no plaintext token remains
- `run_v58_aborts_when_encryption_key_missing` — assert original table intact

**9.2 Visibility** (`db/queries.rs` test module) — the isolation core
- `client_member_sees_all_projects_of_that_client`
- `project_member_sees_only_that_project`
- `non_member_cannot_see_other_client_projects`
- `super_user_sees_every_client`
- `admin_without_membership_does_not_see_client` ← guards the `is_privileged` trap
- `user_can_view_client_returns_true_for_nonexistent_client` ← guards the existence oracle
- `internal_project_visible_only_via_project_membership`

**9.3 Inheritance**
- `org_convention_applies_to_every_client_project`
- `client_convention_adds_to_org_convention` (asserts *both* present, not replacement)
- `internal_project_resolves_org_then_project`

**9.4 Promotion**
- `promote_creates_org_scoped_copy_with_lineage`
- `promote_leaves_source_unchanged`
- `promote_rejects_org_scoped_source`

**9.5 Handler-level** (`tests/integration_test.rs`)
- `cross_client_read_returns_404_not_403`
- `cross_client_read_writes_hidden_access_denied_audit_row`
- `delete_client_with_projects_returns_422`
- `duplicate_slug_returns_409`

Gates: `cargo test --manifest-path apps/backend/Cargo.toml` and `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings`.

---

## 10. Performance & operational notes

- The `VISIBLE_PROJECT_IDS` fragment is a correlated subquery over `project_members` and `client_members`; both are covered by indexes on `user_id`. No N+1 — it composes into the caller's single statement.
- `run_v58` is O(rows in `github_connections`), which is small (one row per org today). The 5 s budget in the spec is dominated by the `ALTER TABLE` statements, not the rebuild.
- The migration must run with `NEXUSMIND_TOKEN_ENCRYPTION_KEY` set. Deployments that lack it fail at startup — intentional, and a hard dependency for the AWS module.

---

## 11. Rollout

1. Ship the migration and the models with no route wiring — schema lands, nothing changes behaviourally.
2. Wire the visibility fragment into the existing queries; the isolation tests turn green while every existing test stays green (no client rows exist yet, so every project is reached via project membership exactly as before).
3. Mount `/v1/clients` and create the first client.
4. Assign existing projects to clients via `PATCH`, one at a time, verifying context resolution after each.

Step 2 is the risky one and is also the one fully covered by §9.2. Steps 1 and 3 are additive; step 4 is reversible (`client_id` back to `NULL`).

---

## 12. Process note

`openspec/config.yaml` sets `artifact_store: nexusmind`, which requires every artifact to be written to both the filesystem and the NexusMind artifact store. **The NexusMind backend is unreachable in this session**, so `proposal.md`, `spec.md`, and this document exist on the filesystem only and must be replayed into the artifact store once the backend is up.
