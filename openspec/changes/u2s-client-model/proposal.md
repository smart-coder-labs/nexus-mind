# Proposal — u2s Client Model (consultancy grouping)

> **Change**: `u2s-client-model`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-08-13

---

## 1. Intent

### Problem

NexusMind models exactly one grouping level below the tenant: `organizations → projects`. `projects` is flat — `(id, org_id, name, description, archived_at)` with `UNIQUE(org_id, name)` — and there is no concept of a **client** anywhere in the schema.

That is a fit problem for a software consultancy. u2s has clients; each client has one or more projects; u2s also runs projects of its own. Today the only two ways to express that are both wrong:

- **One org per client** — isolation works, but the "company brain" disappears. A developer needs an API key per client, conventions established by u2s must be duplicated into every org, and nothing can be reused across engagements.
- **Flat projects named `acme-billing`, `acme-web`, …** — the grouping lives in a naming convention that nothing enforces, no query can aggregate by client, and access control cannot be expressed at the client level.

Every downstream feature inherits the gap, because they are all built on the `(org, project)` pair: `conventions` resolves org-wide + project (`api/context.rs:73`), `policies` carries `project_id`, context loading is keyed by project name, and audit rows cannot be filtered by client.

Two pre-existing defects are tolerable while a single organization holds a single GitHub account, and stop being tolerable the moment NexusMind custodies credentials for several clients:

1. **`github_connections.access_token` is stored in plaintext.** `queries.rs:14860` performs `INSERT OR REPLACE` with no cipher, while `code_projects.github_token_encrypted` does use `token_cipher::encrypt` (`api/code.rs:377`). The capability exists and is simply not applied on this path.
2. **`github_connections` has `PRIMARY KEY (org_id)`** — one GitHub connection per organization. A consultancy needs one per client, because each client has its own GitHub organization.

### Why now

1. **It is the base of everything else in the u2s plan.** Token metrics attribute to client/project/task; the migration writes memories scoped to a client; the promotion flow moves knowledge from a client to u2s. None of these can be specified before the client entity exists.
2. **The two defects are prerequisites of this change, not neighbours of it.** The `github_connections` primary key cannot become `(org_id, client_id, github_login)` before `clients` exists. Fixing the cipher on the same table in the same migration avoids touching it twice.
3. **The migration cost only grows.** `memories.project` is a free-form string (`TEXT NOT NULL DEFAULT 'default'`). Every day of new memories is another row to resolve against real projects later.
4. **The schema is ready.** Current migration is `v57`; `projects`, `project_members`, `conventions`, and `policies` all exist and follow the established idempotent-migration pattern.

### Success looks like

- A user assigned only to client A **cannot read, search, or list** any memory, convention, or project belonging to client B — and each denied attempt lands in `audit_logs`.
- Conventions established by u2s apply to every project without being duplicated, and a client-level convention adds to (never silently replaces) the org-level one.
- A project with `client_id IS NULL` is an internal u2s project and behaves identically in every other respect.
- Promoting a memory from a client to u2s requires an explicit human action and records lineage back to its source.
- No readable token remains in the `.db` file, and two clients with different GitHub accounts coexist without overwriting each other.

---

## 2. Scope

### In scope (MVP)

1. **DB migration `run_v58`** — idempotent, following the established pattern:
   - `clients` table: `(id, org_id, name, slug, status, archived_at, created_at)`, `UNIQUE(org_id, slug)`, `status ∈ {active, paused, offboarded}`.
   - `client_members` table: `(client_id, user_id, role, created_at)`, `UNIQUE(client_id, user_id)`.
   - `projects.client_id` — nullable FK, `ON DELETE RESTRICT`. **`NULL` means internal u2s project.**
   - `code_projects.project_id` — nullable FK, one repo per project (1:1, enforced in application code, not by constraint, so existing rows stay valid).
   - `conventions.client_id` and `policies.client_id` — nullable FKs enabling the third inheritance level.
   - `memories.scope` — widen the accepted set from `project | personal` to `org | client | project | personal`.
   - `memories.promoted_from` — nullable FK to the source memory, for promotion lineage.
2. **Three-level inheritance resolution** — `org → client → project`, additive at every level, extending the existing sumative behaviour of `list_conventions_visible`. Same treatment for policy resolution.
3. **Access control** — client-level membership checks alongside the existing project-level ones, `super_user` retaining org-wide visibility. Denied reads return the hidden-resource 404 already used by `hidden_resource_not_found` and write one audit row.
4. **`promote_memory`** — creates a new `scope=org` memory from a client-scoped source, records `promoted_from`, and requires an explicit call. Never automatic.
5. **Token encryption fix** — apply `token_cipher::encrypt` on the `github_connections` write path and migrate existing rows in `run_v58`.
6. **`github_connections` primary key** — `(org_id)` → `(org_id, client_id, github_login)`.
7. **Project resolution for existing memories** — resolve `memories.project` against `projects.name` on exact match; everything unresolved is left untouched and reported, not guessed.

### Out of scope (post-MVP)

- **Token and time accounting** (`usage_events`, session attribution) — next change; depends on this one.
- **MCP tool-catalog reduction** (136 → ~8 resident tools) — independent change, no schema dependency.
- **Offline local queue** in the MCP client — independent change.
- **Data migration and ingestion** (Claude Code memories, repo `.md`, git/GitHub, Postgres/Supabase schemas) — separate change; depends on this one.
- **AWS deployment module** (`infrastructure/terraform/aws/`, embeddings and code indexing enabled) — separate change, no schema dependency, can proceed in parallel.
- **Client-facing panel.** Confirmed out for v1; the client is a grouping entity, not an authenticated actor.
- **Automatic sanitization on promotion.** Lineage is recorded and the action is explicit; content rewriting is not attempted.
- **Physical isolation per client.** Deliberately traded away — see Risks.

---

## 3. Approach

### Shape

```sql
-- run_v58 (abbreviated)
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

ALTER TABLE projects      ADD COLUMN client_id  TEXT REFERENCES clients(id) ON DELETE RESTRICT;
ALTER TABLE code_projects ADD COLUMN project_id TEXT REFERENCES projects(id);
ALTER TABLE conventions   ADD COLUMN client_id  TEXT REFERENCES clients(id) ON DELETE CASCADE;
ALTER TABLE policies      ADD COLUMN client_id  TEXT REFERENCES clients(id) ON DELETE CASCADE;
ALTER TABLE memories      ADD COLUMN promoted_from TEXT REFERENCES memories(id);
```

Resolution order for conventions and policies becomes **org → client → project**, each level adding to the previous. This is not a new mechanism: `get_project_context` already unions org-wide and project-scoped conventions (`api/context.rs:73`); the change inserts one level in the middle.

Visibility is `client_members ∪ project_members`, with `super_user` unchanged. A user with client membership sees every project of that client; a user with only project membership sees just that project.

### Rationale

- **Client inside the org, not one org per client.** The whole point is reuse across engagements — a shared convention set, a promotable playbook, one API key per developer. Physical separation would deliver isolation and destroy the reason for the project. This was decided explicitly and the cost is accepted; it is mitigated below.
- **`client_id` nullable rather than a synthetic "u2s" client row.** Internal work is genuinely not a client engagement. A sentinel row would need special-casing in billing, reporting, and offboarding anyway.
- **Additive inheritance, never override.** A client-level rule that silently replaced an org-level one would make u2s's own standards unenforceable — precisely the opposite of a company brain.
- **`ON DELETE RESTRICT` on `projects.client_id`.** Deleting a client with live projects must fail loudly; offboarding is a status transition, not a cascade.
- **One migration for the schema and both defects.** They touch the same table and the PK change depends on `clients` existing.
- **Human-gated promotion.** A leak here is a contractual breach, not a bug. Lineage makes it auditable after the fact; the explicit action prevents it beforehand.

### Risks & open questions

| Risk | Mitigation |
|---|---|
| **Isolation becomes a permissions problem, not a physical barrier.** Direct consequence of the client-inside-org decision. | Isolation tests are acceptance criteria, not optional: cross-client read, search, list, and context must all fail closed and audit. |
| **`memories.project` resolution.** Free-form strings will not all map to real projects. | Exact-match only; unresolved rows keep their current value and are reported for manual triage. No heuristics, no guessing. |
| **Existing `github_connections` rows.** Changing the PK requires a table rebuild in SQLite. | Rebuild inside `run_v58` following the existing pattern; encrypt tokens during the copy. Verify row count before and after. |
| **Promotion leak.** A sanitization mistake exports client material into u2s assets. | Explicit action + lineage + audit. Automatic sanitization deliberately out of scope. |
| **Wider `memories.scope` domain.** Existing rows default to `project`. | Column already has `NOT NULL DEFAULT 'project'`; widening the accepted set is backward-compatible. |

**Open question (non-blocking):** whether `client_members` roles should reuse the existing project role vocabulary or get their own. Defaulting to reuse; revisit in `design.md` if the permission checks read awkwardly.

**Process note:** `openspec/config.yaml` sets `artifact_store: nexusmind`, requiring every artifact to be written to both the filesystem and the NexusMind artifact store. **The NexusMind backend is unreachable in this session**, so this artifact exists on the filesystem only. It must be replayed into the artifact store once the backend is up, or the dual-write invariant is silently broken for this change.
