# Design — SDD Artifacts

**Change:** `sdd-artifacts`
**Project:** nexus-mind
**Depends on:** `proposal.md`
**Date:** 2026-07-11

---

## 1. Domain model

Three levels, mirroring the on-disk structure exactly:

```
openspec/changes/team-tasks/          →  sdd_changes         (one row per change folder)
openspec/changes/team-tasks/design.md →  sdd_artifacts       (one row per file, identified by kind)
  (each time the harness rewrites it) →  sdd_artifact_revisions  (immutable, content-hashed)
```

`sdd_changes` is the unit that everything links to — tasks, sprints, memories. Artifacts hang off it.
Revisions hang off artifacts. That is the whole model.

### Artifact kinds

The `kind` enum is the artifact set from `openspec-convention.md`, one-to-one:

| `kind` | On disk | Written by |
|---|---|---|
| `exploration` | `exploration.md` | sdd-explore |
| `proposal` | `proposal.md` | sdd-propose |
| `spec` | `specs/{capability}/spec.md` | sdd-spec |
| `design` | `design.md` | sdd-design |
| `tasks` | `tasks.md` | sdd-tasks, updated by sdd-apply |
| `apply-progress` | `apply-progress.md` | sdd-apply |
| `verify-report` | `verify-report.md` | sdd-verify |
| `archive-report` | `archive-report.md` | sdd-archive |
| `state` | `state.yaml` | orchestrator |

`spec` is the only kind that repeats within a change — once per capability. That's what `capability`
discriminates. Every other kind appears at most once, which is exactly the uniqueness constraint below.

### Phases

`sdd_changes.phase` tracks the DAG position: `explore | propose | spec | design | tasks | apply | verify | archive`.
It is advisory — the artifact inventory is the ground truth for what actually exists. Phase exists so
the admin can show a pipeline and `/sdd-continue` can resume without reading every artifact.

---

## 2. Schema (migration v53)

Following the house conventions: `TEXT` uuid PKs, `org_id` on root tables only, ISO-8601 `TEXT`
timestamps defaulting to `datetime('now')`, soft delete via nullable `archived_at`, child tables
inherit `org_id` through `ON DELETE CASCADE`.

```sql
-- Root entity. org-scoped, project by NAME (matches tasks.project — see proposal D4).
CREATE TABLE IF NOT EXISTS sdd_changes (
  id          TEXT PRIMARY KEY,
  org_id      TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  project     TEXT NOT NULL,
  name        TEXT NOT NULL,                          -- kebab-case folder name, e.g. "team-tasks"
  title       TEXT,
  status      TEXT NOT NULL DEFAULT 'active',         -- active | archived | abandoned
  phase       TEXT NOT NULL DEFAULT 'propose',        -- explore..archive
  repo_url    TEXT,                                   -- provenance: github repo
  repo_ref    TEXT,                                   -- provenance: branch or commit
  sprint_id   TEXT REFERENCES sprints(id) ON DELETE SET NULL,
  created_by  TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
  archived_at TEXT,
  UNIQUE(org_id, project, name)
);
CREATE INDEX IF NOT EXISTS idx_sdd_changes_org_project_status ON sdd_changes(org_id, project, status);
CREATE INDEX IF NOT EXISTS idx_sdd_changes_name              ON sdd_changes(org_id, name);
CREATE INDEX IF NOT EXISTS idx_sdd_changes_sprint            ON sdd_changes(sprint_id);

-- One row per artifact file. No org_id — inherited via change_id cascade.
CREATE TABLE IF NOT EXISTS sdd_artifacts (
  id              TEXT PRIMARY KEY,
  change_id       TEXT NOT NULL REFERENCES sdd_changes(id) ON DELETE CASCADE,
  kind            TEXT NOT NULL,
  capability      TEXT NOT NULL DEFAULT '',           -- '' except for kind='spec'  ← see note
  path            TEXT,                               -- openspec/changes/team-tasks/design.md
  latest_revision INTEGER NOT NULL DEFAULT 0,
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(change_id, kind, capability)
);
CREATE INDEX IF NOT EXISTS idx_sdd_artifacts_change ON sdd_artifacts(change_id, kind);

-- Immutable content revisions. Append-only.
CREATE TABLE IF NOT EXISTS sdd_artifact_revisions (
  id           TEXT PRIMARY KEY,
  artifact_id  TEXT NOT NULL REFERENCES sdd_artifacts(id) ON DELETE CASCADE,
  revision     INTEGER NOT NULL,                      -- 1-based, monotonic per artifact
  content      TEXT NOT NULL,
  content_hash TEXT NOT NULL,                         -- sha256 hex of content
  byte_size    INTEGER NOT NULL,
  git_commit   TEXT,
  git_path     TEXT,
  source       TEXT NOT NULL DEFAULT 'agent',         -- agent | admin | import
  created_by   TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(artifact_id, revision)
);
CREATE INDEX IF NOT EXISTS idx_sdd_revisions_artifact ON sdd_artifact_revisions(artifact_id, revision DESC);
CREATE INDEX IF NOT EXISTS idx_sdd_revisions_hash     ON sdd_artifact_revisions(artifact_id, content_hash);

-- M:N change ↔ memory.
CREATE TABLE IF NOT EXISTS sdd_change_memories (
  id         TEXT PRIMARY KEY,
  change_id  TEXT NOT NULL REFERENCES sdd_changes(id) ON DELETE CASCADE,
  memory_id  TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  relation   TEXT NOT NULL DEFAULT 'produced',        -- produced | informed
  linked_by  TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(change_id, memory_id)
);
CREATE INDEX IF NOT EXISTS idx_sdd_change_memories_memory ON sdd_change_memories(memory_id);

-- FTS5 over the LATEST revision of each artifact. Standalone (not external-content):
-- the memories_fts trigger pattern assumes a 1:1 row mapping, which we don't have
-- (many revisions → one indexed doc). Maintained explicitly by upsert_sdd_artifact.
CREATE VIRTUAL TABLE IF NOT EXISTS sdd_artifacts_fts USING fts5(
  artifact_id  UNINDEXED,
  change_name,
  kind,
  capability,
  content
);
```

**On `capability TEXT NOT NULL DEFAULT ''`** — the obvious modelling would be a nullable column, but
SQLite treats every `NULL` as distinct inside a `UNIQUE` constraint, so `(change, 'design', NULL)` could
be inserted twice and the uniqueness we depend on would silently not exist. The empty-string sentinel
makes `UNIQUE(change_id, kind, capability)` actually hold. This is load-bearing; a test asserts it.

### Permissions (migration v54)

New strings: `sdd:read`, `sdd:write`, `sdd:delete`. Granted to the seeded role templates exactly as v52
did for `task:*`:

| Template | Grants |
|---|---|
| `tmpl_dev_junior` | `sdd:read`, `sdd:write` |
| `tmpl_dev_senior` | `sdd:read`, `sdd:write`, `sdd:delete` |
| `tmpl_security_officer` | `sdd:read` |
| `tmpl_auditor` | `sdd:read` |

Admin and `super_user` bypass via `UserRole::is_privileged()`, as everywhere else.

`tests/integration_test.rs` asserts the migration version — bump `52` → `54`.

---

## 3. Store layer (`db/queries.rs`, new `// ── SDD artifacts ──` section)

Free functions, `anyhow::Result`, org in the `WHERE`, `Ok(None)` for not-found, string sentinels for
errors — the house style. No new store trait.

```rust
pub fn upsert_sdd_change(conn, org_id, created_by, req: &UpsertChangeRequest) -> Result<SddChange>;
pub fn get_sdd_change(conn, org_id, id) -> Result<Option<SddChange>>;             // hydrates artifacts[]
pub fn get_sdd_change_by_name(conn, org_id, project, name) -> Result<Option<SddChange>>;
pub fn list_sdd_changes(conn, org_id, filters: &SddChangeFilters) -> Result<Vec<SddChange>>;
pub fn patch_sdd_change(conn, org_id, id, req: &PatchChangeRequest) -> Result<SddChange>;
pub fn archive_sdd_change(conn, org_id, id) -> Result<bool>;

/// THE workhorse. Idempotent: if content_hash == latest revision's hash, returns the
/// existing artifact untouched and creates NO revision. Creates the change if absent.
pub fn upsert_sdd_artifact(conn, org_id, created_by, req: &SaveArtifactRequest)
    -> Result<(SddArtifact, bool /* created_revision */)>;

pub fn get_sdd_artifact(conn, org_id, id) -> Result<Option<SddArtifactDetail>>;   // + latest content
pub fn get_sdd_artifact_by_kind(conn, org_id, project, change_name, kind, capability)
    -> Result<Option<SddArtifactDetail>>;
pub fn list_sdd_artifact_revisions(conn, org_id, artifact_id) -> Result<Vec<SddRevisionMeta>>;  // no content
pub fn get_sdd_artifact_revision(conn, org_id, artifact_id, revision) -> Result<Option<SddRevision>>;
pub fn search_sdd_artifacts(conn, org_id, q, limit) -> Result<Vec<SddSearchHit>>; // FTS5 + snippet()

pub fn link_sdd_change_memory(conn, org_id, change_id, memory_id, relation, linked_by) -> Result<()>;
pub fn unlink_sdd_change_memory(conn, org_id, change_id, memory_id) -> Result<bool>;
pub fn list_sdd_change_memories(conn, org_id, change_id) -> Result<Vec<Memory>>;
pub fn list_tasks_for_sdd_change(conn, org_id, change_name) -> Result<Vec<Task>>;  // via task_spec_links
pub fn sdd_change_exists(conn, org_id, name) -> Result<bool>;                      // for spec_change_exists
```

`upsert_sdd_artifact` in one transaction:

1. Resolve or create the `sdd_changes` row for `(org_id, project, name)`.
2. Resolve or create the `sdd_artifacts` row for `(change_id, kind, capability)`.
3. `sha256(content)`. If it equals the latest revision's `content_hash` → return `(artifact, false)`.
   **No revision, no FTS write, no `updated_at` bump.**
4. Otherwise insert revision `latest_revision + 1`, bump `latest_revision`, touch `updated_at`.
5. Replace the artifact's row in `sdd_artifacts_fts` (delete-then-insert on `artifact_id`).

Step 3 is what lets the harness call `save_sdd_artifact` unconditionally on every phase without
generating garbage. Reject `content` over 1 MB with `anyhow!("artifact_too_large")` → 422.

`sha2` is already in the backend's dependency tree (harness manifest hashing) — no new crate.

---

## 4. API (`api/sdd.rs`, routes in `router.rs`)

Standard handler shape: `State` → `Extension(auth)` → `Path` → `Query` → `AppJson`, `require_permission`
first, `Result<T, (StatusCode, Json<ApiError>)>` out, local `db_err` mapping the store's string sentinels.
Not-found and not-visible both 404 (the existence-leak rule from `tasks.rs`).

| Method | Path | Perm | Notes |
|---|---|---|---|
| GET | `/v1/sdd/changes` | `sdd:read` | filters: `project`, `status`, `phase`, `sprint_id`, `include_archived`. **Metadata only, never content.** |
| POST | `/v1/sdd/changes` | `sdd:write` | upsert by `(project, name)` |
| GET | `/v1/sdd/changes/:id` | `sdd:read` | hydrated: artifact inventory (no content), task links, memory links |
| PATCH | `/v1/sdd/changes/:id` | `sdd:write` | `phase`, `status`, `title`, `sprint_id` |
| DELETE | `/v1/sdd/changes/:id` | `sdd:delete` | soft — sets `archived_at` |
| GET | `/v1/sdd/changes/:id/artifacts` | `sdd:read` | |
| GET | `/v1/sdd/changes/:id/tasks` | `sdd:read` + `task:read` | joins `task_spec_links` on `name` |
| POST | `/v1/sdd/changes/:id/memories` | `sdd:write` | `{memory_id, relation}` |
| DELETE | `/v1/sdd/changes/:id/memories/:memory_id` | `sdd:write` | |
| **PUT** | **`/v1/sdd/artifacts`** | `sdd:write` | **the workhorse.** Body: `{project, change_name, kind, capability?, content, path?, git_commit?, git_ref?, source?}`. Returns `{artifact, created_revision: bool}`. 200 always (idempotent), never 201. |
| GET | `/v1/sdd/artifacts?project=&change_name=&kind=&capability=` | `sdd:read` | lookup by natural key → single artifact + latest content. This is how `get_sdd_artifact` resolves `(change, kind)` without a round-trip. |
| GET | `/v1/sdd/artifacts/:id` | `sdd:read` | + latest revision content |
| GET | `/v1/sdd/artifacts/:id/revisions` | `sdd:read` | metadata only |
| GET | `/v1/sdd/artifacts/:id/revisions/:rev` | `sdd:read` | full content |
| GET | `/v1/sdd/search?q=&limit=` | `sdd:read` | FTS5, returns snippets |

**Route-ordering gotcha:** `/v1/sdd/artifacts` (PUT, static) must be registered **before** any
`/v1/sdd/:id`-shaped route. There is no such route here, but keep the static-first discipline anyway —
`tasks.rs` had to do this for `/v1/tasks/resolve-by-spec`.

### The `spec_change_exists` fix (D5)

`api/tasks.rs:375-391` currently stats the local filesystem and returns `true` when the root is
unreadable — i.e. always true in production. Replace with:

```rust
fn spec_change_exists(conn: &Connection, org_id: &str, name: &str) -> bool {
    if queries::sdd_change_exists(conn, org_id, name).unwrap_or(false) { return true }
    fs_spec_change_exists(&repo_root(), name)   // existing FS check, kept as fallback
}
```

DB first (real in production), filesystem second (still works for a local backend inside a checkout
whose changes were never pushed to NexusMind). The permissive-on-unreadable-root behaviour of the FS
path is preserved, so nothing that links today stops linking — but once the importer has run, a typo'd
change name in production hits neither branch and correctly 422s. **This is a behaviour change on an
existing endpoint; it gets its own test.**

---

## 5. Importer (`bin/import_sdd.rs`)

A one-shot binary, in the shape of `bin/backfill_embeddings.rs`.

**Source A — the filesystem.** Walk `openspec/changes/*/` and `openspec/changes/archive/*/`:
- Folder name → `sdd_changes.name` (archive folders: strip the `YYYY-MM-DD-` prefix, set
  `status='archived'`, `phase='archive'`).
- Each `*.md` → an artifact of the matching kind; `specs/{capability}/spec.md` → `kind='spec'`.
- `source='import'`, `git_path` set, `git_commit` from `git rev-parse HEAD` if available.
- Phase for active changes is inferred from the artifact inventory (the furthest kind present).

**Source B — legacy memories.** Every memory whose `topic_key` matches `sdd/{change}/{artifact-type}`
becomes an artifact revision with `source='import'`. Where the same artifact exists in both sources,
the filesystem wins (it's newer and reviewable) and the memory becomes revision 1 with the file as
revision 2 — the ordering is by the memories' `created_at`, so history reads correctly.

Idempotent by construction: it calls `upsert_sdd_artifact`, so a second run creates zero revisions.

The importer **does not delete** the legacy memories. It tags them `sdd-migrated`. Whether to archive
them is the user's call, made after they can see the imported artifacts in the admin.

---

## 6. MCP tools (`nexusmind-mcp`, new `// ── SDD Artifacts ──` section)

Client fns into `src/client.ts` near the task block; tools into `src/index.ts` after `// ── Tasks ──`.
Descriptions in house style: what it does + when to call it + the permission + idempotency semantics.

| Tool | Wraps | Purpose |
|---|---|---|
| `save_sdd_artifact` | `PUT /v1/sdd/artifacts` | The write path. Called by every `sdd-*` skill. Idempotent by content hash. |
| `get_sdd_artifact` | `GET /v1/sdd/artifacts/:id` or by `(change, kind)` | **The cross-phase read.** `sdd-design` reads the proposal; `sdd-tasks` reads spec + design. Returns FULL content — previews are useless here. |
| `list_sdd_changes` | `GET /v1/sdd/changes` | Powers `/sdd-status`. |
| `get_sdd_change` | `GET /v1/sdd/changes/:id` | Powers `/sdd-continue` — the artifact inventory *is* the recoverable DAG state. |
| `update_sdd_change` | `PATCH /v1/sdd/changes/:id` | Phase transitions. |
| `search_sdd_artifacts` | `GET /v1/sdd/search` | FTS across every change in the org. |
| `link_sdd_change_memory` | `POST /v1/sdd/changes/:id/memories` | Called by `sdd-apply` / `sdd-verify` to tie decisions back to the spec. |

`get_sdd_artifact` is the tool that makes the harness work: today a sub-agent in the `engram` mode does
`mem_search` → `mem_get_observation` and gets a truncated preview it must hope is complete. Here it gets
the document.

---

## 7. Admin (`apps/admin`)

**Route + nav.** `src/App.tsx`: lazy `SddArtifacts` → `<Route path="/sdd" ...>`. `Layout.tsx` `NAV_GROUPS`,
group **Knowledge** (next to Tasks and Memories): `{ label: 'SDD', href: '/sdd', icon: FileStack, adminOnly: true, requiredPermission: 'sdd:read' }`.

**`<Markdown>` primitive — extract first, then use.** `react-markdown` is installed but there is **no
`remark-gfm`**, and the same 70-line `components={{...}}` override map is copy-pasted in four places
(`Memories.tsx:63-136`, `Conventions.tsx:678-707`, `OrgMemoryGraph.tsx:442`, `MemoryGraphTab.tsx:468`).
`tasks.md` is *entirely* GFM task lists and tables, so without `remark-gfm` the flagship artifact renders
as a wall of literal `- [ ]`. So: add `remark-gfm`, extract `src/components/ui/Markdown/`, point the four
existing call sites at it, then build the SDD section on top. The extraction is a prerequisite, not a
nice-to-have — and it pays for itself immediately across Memories and Conventions.

**Pages.**
- `pages/Sdd.tsx` — change list, mirroring `Tasks.tsx`: module-scope `createClient()`, filter bar
  (project / phase / status), skeleton → `EmptyState` → table, permission gate `canRead` → `<Navigate to="/401">`.
  A phase pipeline badge row per change (`propose → spec → design → tasks → apply → verify`), driven by
  which artifacts actually exist.
- `pages/sdd/ChangeDetail.tsx` — right-side drawer (`Modal position="right" size="lg"`, already supported).
  Artifact tabs (Proposal / Specs / Design / Tasks / Verify), each rendering `<Markdown>` with a
  **Raw / Preview toggle** (the `Conventions.tsx:655-676` precedent), a revision dropdown (`rev 3 ▾`)
  that refetches `/revisions/:rev`, and Linked Tasks / Linked Memories sections.

**Cross-links.** `pages/tasks/TaskDetail.tsx` already renders a "Linked Specs" section of bare strings.
Make each one a link into `/sdd?change=<name>`, and show the change's phase next to it.

**Global search.** Add `sdd_changes: SddChangeSummary[]` to `GlobalSearchResult` (backend + `types.ts`)
and a result group in the search UI. Additive field; the admin ignores unknown keys today.

**Query keys**, following the house convention: `['sdd-changes', filters]`, `['sdd-change', id]`,
`['sdd-artifact', id]`, `['sdd-artifact-revision', id, rev]`, `['sdd-change-tasks', id]`.

---

## 8. Harness (`sdd-*` skills)

**New persistence mode `nexusmind`** in `_shared/persistence-contract.md`, joining `engram | openspec | hybrid | none`:

| Mode | Read from | Write to | Project files | History |
|---|---|---|---|---|
| `engram` | memory store | memory store | never | ❌ upsert overwrites |
| `openspec` | filesystem | filesystem | yes | ✅ git |
| `hybrid` | engram + fs | both | yes | ✅ git |
| **`nexusmind`** | **`get_sdd_artifact`** | **`save_sdd_artifact` + filesystem** | **yes** | **✅ revisions + git** |
| `none` | prompt | nowhere | never | ❌ |

`nexusmind` is what `hybrid` was trying to be, done properly: it writes the file **and** the indexed,
versioned artifact, and it reads full documents instead of memory previews. It becomes the recommended
default, and `engram` is marked deprecated in the contract (not removed — other repos still run it).

**Skill updates.** Each of the ten `sdd-*` skills changes in the same mechanical way:
- Sub-agent prompt block: `mem_save(topic_key: "sdd/{change}/{artifact}", type: "architecture", ...)`
  → `save_sdd_artifact(project, change_name, kind, content, path)`.
- Dependency reads: `mem_search` + `mem_get_observation` → `get_sdd_artifact(change_name, kind)`.
- `sdd-apply` additionally calls `update_sdd_change(phase: 'apply')` and `link_sdd_change_memory` for
  the decisions it records.
- `sdd-verify` / `sdd-archive` already call `resolve_tasks_for_spec`; they now also
  `update_sdd_change(phase, status)`.

The `capture_prompt: false` dance in the current contract exists only because SDD artifacts and human
decisions share the `memories` table. Once artifacts have their own table, that entire paragraph is
deleted — a good sign the model is right.

**Publish.** Package the updated skills as a harness version in the NexusMind harness library
(`create_harness` / `publish_harness_version`), so `nexus-mind`, `kasymir`, and the rest install the same
SDD harness instead of each repo drifting its own copy.

---

## 9. PR breakdown

Chained, each independently green (`cargo test` + `clippy -D warnings` for backend; `npm test` + `tsc -b`
for admin/mcp). Backend PRs are strict-TDD per `openspec/config.yaml` (`strict_tdd: true`,
`tdd_scope: backend_and_admin`).

| PR | Scope | Depends on |
|---|---|---|
| **PR-1** | Migrations v53 (4 tables + FTS5) + v54 (permissions); `models/types.rs` entities & requests; bump the version assertion in `tests/integration_test.rs` | — |
| **PR-2** | `db/queries.rs` — the whole SDD section, incl. `upsert_sdd_artifact` (hash de-dup, FTS maintenance) and `search_sdd_artifacts` | PR-1 |
| **PR-3** | `api/sdd.rs` handlers + `router.rs` routes; permission/404/org-isolation test matrix | PR-2 |
| **PR-4** | `spec_change_exists` → DB-first (+ its behaviour-change test); `GET /v1/sdd/changes/:id/tasks`; SDD facet in `global_search` | PR-3 |
| **PR-5** | `bin/import_sdd.rs` — filesystem + legacy-memory importer, idempotent | PR-3 |
| **PR-6** | **mcp**: client fns + the 7 tools + `sdd-client.test.ts` / `sdd-tools.test.ts` (both registered in `package.json` `test`) | PR-3 |
| **PR-7** | **admin**: `remark-gfm` + extract `<Markdown>` primitive + repoint the 4 existing call sites | — (parallel with backend) |
| **PR-8** | **admin**: `types.ts` + `client.ts` SDD block; `/sdd` list page + nav + route | PR-3, PR-7 |
| **PR-9** | **admin**: `ChangeDetail` drawer (artifact tabs, revisions, raw/preview), task↔change cross-links, global-search group | PR-8 |
| **PR-10** | **harness**: `nexusmind` mode in the persistence contract; 10 `sdd-*` skills updated; published to the harness library | PR-6 |

PR-7 has no backend dependency and can start immediately, in parallel with PR-1.

---

## 10. Resolved ambiguities

The spec phase surfaced places where §1–§9 were silent. Decided here so the spec and the implementation
agree:

| # | Question | Decision |
|---|---|---|
| A1 | Content goes A → B → A. Does the third save resurrect revision 1 or append revision 3? | **Append revision 3.** The hash is compared against the *latest* revision only. A revert is an event and must appear in the history; collapsing it would hide that it happened. |
| A2 | The 1 MB cap is checked at step 3, but steps 1–2 already created the change and artifact rows. | **The 422 is atomic.** Validate size before any write (or roll back the transaction). A rejected save MUST leave no change and no artifact behind. |
| A3 | `link_sdd_change_memory` called twice — error, or no-op? And with a *different* `relation`? | **Idempotent.** Re-linking the same pair is a no-op success. Re-linking with a different `relation` **updates** the existing row (`ON CONFLICT DO UPDATE SET relation = excluded.relation`). Agents must be able to call it freely. |
| A4 | What does `global_search` return for a caller without `sdd:read`? | **200 with an empty SDD facet — never 403.** Gating the whole search on a brand-new permission would break global search for every existing user. Mirrors how the `users` facet is gated on `is_privileged()` in `search.rs:70`. |
| A5 | In `nexusmind` mode, the file write succeeds but `save_sdd_artifact` fails (or vice versa). | **Both legs must succeed or the phase fails loudly.** Inherited from the `hybrid` contract ("both writes MUST succeed"). Silent degradation to single persistence is the exact failure this change exists to prevent. |
| A6 | The admin renders markdown that agents wrote. Embedded HTML/script? | **Never executed.** `react-markdown` does not render raw HTML unless `rehype-raw` is added — do not add it. Stated as a MUST so it is a requirement, not an accident of a library default. |
| A7 | "The admin is read-only" (D6) — read-only over *what*? | **Over artifact content only.** No create/edit/delete of artifacts or revisions from the UI. The admin MAY patch a change's `phase`/`status`/`sprint_id` and MAY link/unlink memories — those are curation, not authorship. |
| A8 | Is an archived change still a valid `link_task_spec` target? | **Yes.** `sdd_change_exists` carries no `archived_at` predicate, matching the existing FS check, which globs the archive tree. Tasks routinely link to a change that later archives. |

**Deliberately deferred** (a follow-up change, not this one): semantic search over artifacts
(needs `sdd_artifact_chunks` + a per-chunk embeddings table + heading-level chunking — the existing
`memory_embeddings` is FK'd to `memories.id` and a single vector for a 36 KB document is worthless),
revision diff UI (no diff lib installed), and code/symbol links.
