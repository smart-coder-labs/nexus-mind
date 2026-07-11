# Tasks: SDD Artifacts

> STRICT TDD MODE ACTIVE (`openspec/config.yaml`: `strict_tdd: true`, `tdd_scope: backend_and_admin`). Every backend and admin unit of behavior is a RED (write the failing test) → GREEN (make it pass) pair, in that order. MCP items follow RED→GREEN too, to keep the change internally consistent (`tdd_scope` does not force it). Harness (PR-10) is prose/skill editing — no test runner exists for it, so it is GREEN-only with manual smoke gates.
>
> Test / gate commands:
> - Backend: `cargo test --manifest-path apps/backend/Cargo.toml` + `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` + `cargo fmt --manifest-path apps/backend/Cargo.toml -- --check`
> - Admin: `cd apps/admin && npm run test && npx tsc -b`
> - MCP: `cd nexusmind-mcp && npm test` (`pretest: tsc`; **new test files MUST be added to the `test` script's `tsx --test` file list in `nexusmind-mcp/package.json` or they do not run**)
>
> Locked decisions restated here as acceptance criteria. Design §10 (A1–A8) resolves every ambiguity the spec phase surfaced:
> - **Migration versions**: v53 = 4 tables + FTS5 virtual table + indexes; v54 = `sdd:read` / `sdd:write` / `sdd:delete` grants. `apps/backend/tests/integration_test.rs:329` asserts `user_version == 52` today — it MUST become `54`.
> - **The `capability` sentinel trap** (design §2): `capability TEXT NOT NULL DEFAULT ''`, never nullable. SQLite treats every `NULL` as distinct inside a `UNIQUE` constraint, so a nullable `capability` would silently let `(change, 'design', NULL)` be inserted twice and `UNIQUE(change_id, kind, capability)` would not hold. Load-bearing; test 1.7 asserts it.
> - **A1 — the hash is compared against the LATEST revision only.** Content A → B → A appends **revision 3**; it does not resurrect revision 1. A revert is an event and must appear in the history.
> - **Idempotency by content hash** (D2): `upsert_sdd_artifact` returns `created_revision: false` and writes **no** revision row, **no** FTS row, and does **not** bump `updated_at` when the hash matches. `PUT /v1/sdd/artifacts` therefore returns **200 always, never 201**.
> - **A2 — the 1 MB 422 is ATOMIC.** Size is validated before any write. A rejected oversized save MUST leave **no change row and no artifact row** behind.
> - **A3 — `link_sdd_change_memory` is idempotent, and re-linking with a DIFFERENT `relation` UPDATES the row** (`ON CONFLICT DO UPDATE SET relation = excluded.relation`). Agents must be able to call it freely.
> - **A4 — `global_search` returns 200 with an EMPTY SDD facet** for a caller without `sdd:read` — never 403. Mirrors the `users` facet gating at `search.rs:70`.
> - **A5 — in `nexusmind` mode both writes (file + store) must succeed or the phase fails loudly.** Silent degradation to single persistence is the exact failure this change exists to prevent.
> - **A6 — the `<Markdown>` primitive MUST NOT execute embedded HTML/script.** Do **not** add `rehype-raw`. This is a MUST, not an accident of a library default.
> - **A7 — "the admin is read-only" applies to artifact CONTENT only.** No create/edit/delete of artifacts or revisions from the UI. The admin MAY patch a change's `phase` / `status` / `sprint_id` and MAY link/unlink memories — that is curation, not authorship.
> - **A8 — an archived change remains a valid `link_task_spec` target.** `sdd_change_exists` carries no `archived_at` predicate, matching the FS check that globs the archive tree.
> - **FTS5 is standalone, not external-content**: many revisions map to one indexed doc, so the `memories_fts` trigger pattern does not apply. `upsert_sdd_artifact` maintains `sdd_artifacts_fts` explicitly — delete-then-insert on `artifact_id` on every new revision.
> - **`spec_change_exists` behavior change** (D5): DB first (`queries::sdd_change_exists`), filesystem second (existing `spec_change_exists(root, name)` at `apps/backend/src/api/tasks.rs:375`, kept verbatim as the fallback, including its permissive-on-unreadable-root branch). This changes an existing shipped endpoint — it gets its own regression test.
> - **`GlobalSearchResult` gains `sdd_changes: Vec<SddChangeSummary>`** — purely additive. Backend (`models/types.rs:1025`) and admin (`types.ts:280`) ship together; `CommandPalette.tsx:13`'s `EMPTY_RESULT` must be updated or `tsc -b` fails.
> - **Artifacts are addressable by natural key**: `GET /v1/sdd/artifacts?project=&change_name=&kind=&capability=` (design §4), alongside `GET /v1/sdd/artifacts/:id`.

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | ~3800 total (PR-1 ~380, PR-2 ~620, PR-3 ~520, PR-4 ~250, PR-5 ~300, PR-6 ~450, PR-7 ~280, PR-8 ~340, PR-9 ~460, PR-10 ~340) |
| 400-line budget risk | High overall; per-PR: PR-2 / PR-3 / PR-6 / PR-9 over budget (High), rest Low-Medium |
| Chained PRs recommended | Yes |
| Suggested split | PR-1 → PR-2 → PR-3 → {PR-4, PR-5, PR-6 in parallel off PR-3} → PR-8 → PR-9 → PR-10; **PR-7 has no backend dependency and starts immediately, in parallel with PR-1** |
| Delivery strategy | ask-on-risk |
| Chain strategy | pending (resolve with the orchestrator before apply) |

### Suggested Work Units

| PR | Branch name | Scope | Est. lines | Depends on |
|----|-------------|-------|-----------:|------------|
| PR-1 | `sdd-artifacts/pr1-migrations-models` | migrations v53 + v54, entities, version-assertion bump | ~380 | — |
| PR-2 | `sdd-artifacts/pr2-store` | `db/queries.rs` SDD section | ~620 | PR-1 |
| PR-3 | `sdd-artifacts/pr3-api` | `api/sdd.rs` + routes + permission matrix | ~520 | PR-2 |
| PR-4 | `sdd-artifacts/pr4-integration` | `spec_change_exists` DB-first, change→tasks, global-search facet | ~250 | PR-3 |
| PR-5 | `sdd-artifacts/pr5-importer` | `bin/import_sdd.rs` | ~300 | PR-3 |
| PR-6 | `sdd-artifacts/pr6-mcp` | 7 MCP tools + client fns + 2 test files | ~450 | PR-3 |
| PR-7 | `sdd-artifacts/pr7-markdown-primitive` | `remark-gfm` + `<Markdown>` extraction + 4 call sites repointed | ~280 | — |
| PR-8 | `sdd-artifacts/pr8-admin-list` | admin types/client/nav/route + `/sdd` list page | ~340 | PR-3, PR-7 |
| PR-9 | `sdd-artifacts/pr9-admin-detail` | `ChangeDetail` drawer, curation, cross-links, search group | ~460 | PR-8 |
| PR-10 | `sdd-artifacts/pr10-harness` | `nexusmind` persistence mode + 10 `sdd-*` skills + publish | ~340 | PR-6 |

---

## PR-1 — migrations + models (v53 schema, v54 permissions)

**Goal**: create `sdd_changes`, `sdd_artifacts`, `sdd_artifact_revisions`, `sdd_change_memories` + the `sdd_artifacts_fts` virtual table + indexes (v53); grant `sdd:read` / `sdd:write` / `sdd:delete` to the seeded role templates (v54); define the entity/request structs and the `kind` / `phase` / `status` enums. No store functions, no HTTP surface.

**Satisfies (groundwork)**: `sdd-artifact-store` — "Artifact Identity Is (change, kind, capability) With an Empty-String Capability Sentinel" (the schema half), "SDD Operations Are Gated by sdd:read, sdd:write, and sdd:delete" (the grant matrix), "SDD Changes Are Org-Scoped and Uniquely Keyed by Project and Name" (the uniqueness constraint). `sdd-artifact-links` — "Changes Link to Memories Many-to-Many With a Relation" (the cascade), "A Change Belongs to One Project and Optionally One Sprint" (the `SET NULL`).

**Est. changed lines**: ~380
**Depends on**: — (first PR; PR-7 may run in parallel)

### Checklist

- [x] 1.1 RED: migration test `run_v53_creates_sdd_tables_with_expected_columns` in `apps/backend/src/db/migrations.rs` — after `run_all` on a fresh `:memory:` connection, `sdd_changes`, `sdd_artifacts`, `sdd_artifact_revisions`, `sdd_change_memories` all exist in `sqlite_master` with every column from design §2 (assert via `PRAGMA table_info`).
- [x] 1.2 GREEN: implement `pub fn run_v53(conn: &Connection) -> Result<()>` in `apps/backend/src/db/migrations.rs` — `CREATE TABLE IF NOT EXISTS` for the 4 tables per design §2 (verbatim columns, FKs, defaults), guarded by `PRAGMA user_version < 53`, tailed with `PRAGMA user_version = 53;`. Mirror the doc-comment style of `run_v51`.
- [x] 1.3 RED: migration test `run_v53_creates_fts_virtual_table` — `sdd_artifacts_fts` exists in `sqlite_master`, is an fts5 table, and a raw `INSERT INTO sdd_artifacts_fts (artifact_id, change_name, kind, capability, content) VALUES (...)` followed by `SELECT ... WHERE sdd_artifacts_fts MATCH 'rate'` returns the row (proves the 5 columns and the `UNINDEXED` `artifact_id` are right).
- [x] 1.4 GREEN: add the `CREATE VIRTUAL TABLE IF NOT EXISTS sdd_artifacts_fts USING fts5(artifact_id UNINDEXED, change_name, kind, capability, content)` statement to `run_v53` in `apps/backend/src/db/migrations.rs`. Add an inline comment: standalone, NOT external-content — the `memories_fts` trigger pattern assumes a 1:1 row mapping we do not have (many revisions → one indexed doc); maintained explicitly by `upsert_sdd_artifact` (PR-2).
- [x] 1.5 RED: migration test `run_v53_creates_indexes` — `idx_sdd_changes_org_project_status`, `idx_sdd_changes_name`, `idx_sdd_changes_sprint`, `idx_sdd_artifacts_change`, `idx_sdd_revisions_artifact`, `idx_sdd_revisions_hash`, `idx_sdd_change_memories_memory` all present in `sqlite_master`.
- [x] 1.6 GREEN: add the 7 `CREATE INDEX IF NOT EXISTS` statements to `run_v53` in `apps/backend/src/db/migrations.rs`.
- [x] 1.7 RED: migration test `run_v53_capability_empty_string_sentinel_enforces_uniqueness` — **the trap**. Insert an `sdd_artifacts` row with `kind='design'` and no explicit `capability` (relies on `DEFAULT ''`), then insert a second identical row: it MUST fail with a constraint error. Additionally assert `PRAGMA table_info(sdd_artifacts)` reports `capability` as `notnull=1` with `dflt_value` `''` — because a nullable `capability` would make every `NULL` distinct under SQLite's `UNIQUE` semantics and the constraint would silently not hold (Spec: sdd-artifact-store / "Artifact Identity …" / "Omitted capability MUST NOT create a duplicate artifact row").
- [x] 1.8 GREEN: declare `capability TEXT NOT NULL DEFAULT ''` (never nullable) and `UNIQUE(change_id, kind, capability)` on `sdd_artifacts` in `run_v53`; add the load-bearing comment explaining the SQLite `NULL`-distinctness rationale (design §2).
- [x] 1.9 RED: migration test `run_v53_spec_kind_repeats_per_capability` — two `sdd_artifacts` rows with the same `change_id` and `kind='spec'` but different `capability` (`'sdd-artifact-store'`, `'sdd-artifact-links'`) both insert successfully, confirming `spec` is the only kind that repeats within a change (Spec: "Spec artifacts are discriminated by capability").
- [x] 1.10 GREEN: confirm the `UNIQUE(change_id, kind, capability)` composite from 1.8 already permits this (no schema change expected — this task closes the loop with the regression assertion).
- [x] 1.11 RED: migration test `run_v53_unique_constraints` — `UNIQUE(org_id, project, name)` on `sdd_changes`, `UNIQUE(artifact_id, revision)` on `sdd_artifact_revisions`, and `UNIQUE(change_id, memory_id)` on `sdd_change_memories` each reject a duplicate insert with a constraint error.
- [x] 1.12 GREEN: verify/adjust the three `UNIQUE` composites in `run_v53`.
- [x] 1.13 RED: migration test `run_v53_fk_cascade_and_restrict` — deleting an `sdd_changes` row cascades to `sdd_artifacts`, `sdd_artifact_revisions` (transitively), and `sdd_change_memories`; **deleting a `memories` row cascades to `sdd_change_memories`** (Spec: sdd-artifact-links / "Changes Link to Memories …" / "Deleting the memory removes the link"); **deleting a `sprints` row sets `sdd_changes.sprint_id` to NULL and does not delete the change** (Spec: sdd-artifact-links / "A Change Belongs to One Project and Optionally One Sprint" / "Deleting the sprint leaves the change intact"); deleting a `users` row referenced by `sdd_changes.created_by` / `sdd_artifact_revisions.created_by` / `sdd_change_memories.linked_by` is RESTRICTed. (Enable `PRAGMA foreign_keys = ON` in the test connection — check `db/connection.rs` already does.)
- [x] 1.14 GREEN: verify/adjust every `ON DELETE CASCADE` / `SET NULL` / `RESTRICT` clause in `run_v53` to satisfy 1.13.
- [x] 1.15 RED: migration test `run_v53_is_idempotent` — `run_all` twice on the same connection does not error, and the table/index counts in `sqlite_master` are unchanged.
- [x] 1.16 GREEN: confirm the `PRAGMA user_version < 53` guard makes double-invocation a no-op in `run_v53`.
- [x] 1.17 RED: migration test `run_v54_grants_sdd_perms` — after `run_all`, the seeded `roles` rows have `permissions` JSON containing exactly the design §2 grant matrix: `tmpl_dev_junior` → `sdd:read`,`sdd:write`; `tmpl_dev_senior` → `sdd:read`,`sdd:write`,`sdd:delete`; `tmpl_security_officer` → `sdd:read`; `tmpl_auditor` → `sdd:read`.
- [x] 1.18 GREEN: implement `pub fn run_v54(conn: &Connection) -> Result<()>` in `apps/backend/src/db/migrations.rs`, copying `run_v52`'s `json_insert(permissions, '$[#]', ?1) WHERE NOT EXISTS (SELECT 1 FROM json_each(roles.permissions) WHERE value = ?1)` shape exactly; guard `PRAGMA user_version < 54`, tail `PRAGMA user_version = 54;`.
- [x] 1.19 RED: migration test `run_v54_is_idempotent` — `run_all` twice does not duplicate any `sdd:*` string in any template's `permissions` JSON array.
- [x] 1.20 GREEN: confirm the `NOT EXISTS`/`json_each` membership guard in `run_v54` satisfies 1.19.
- [x] 1.21 RED: migration test `run_v54_preserves_existing_permissions` — the `task:*` strings granted by `run_v52` (and any `harness:*` / `session:*` strings) are still present and unchanged on every template after `run_v54`.
- [x] 1.22 GREEN: confirm `run_v54` only appends (never replaces) the `permissions` array in `apps/backend/src/db/migrations.rs`.
- [x] 1.23 RED: migration test `run_all_sets_user_version_to_54` in `apps/backend/src/db/migrations.rs` — `PRAGMA user_version` is `54` after `run_all` on a fresh DB.
- [x] 1.24 GREEN: append `run_v53(conn)?;` and `run_v54(conn)?;` to `run_all()` in `apps/backend/src/db/migrations.rs`, immediately after `run_v52(conn)?;` (currently line 57).
- [x] 1.25 RED: update `apps/backend/tests/integration_test.rs` `migration_idempotency` (~line 329) — change `assert_eq!(version, 52)` to `assert_eq!(version, 54)` and its comment to "(54 after the sdd-artifacts migrations)". **This is the version-assertion bump; it fails RED until 1.24 lands.**
- [x] 1.26 GREEN: run `cargo test --manifest-path apps/backend/Cargo.toml --test integration_test` and confirm the bumped assertion passes; grep the repo for any other hard-coded `52` migration-version assertion (`grep -rn "user_version" apps/backend/src apps/backend/tests`) and bump those too.
- [x] 1.27 RED: unit test `sdd_artifact_kind_from_str_round_trips` in `apps/backend/src/models/types.rs` — all 9 kinds (`exploration`, `proposal`, `spec`, `design`, `tasks`, `apply-progress`, `verify-report`, `archive-report`, `state`) parse and `Display` back to their exact on-disk string; an unrecognized string returns `Err` (Spec: "Reject an unrecognized artifact kind").
- [x] 1.28 GREEN: implement `pub enum SddArtifactKind` with hand-rolled `FromStr` + `Display` in `apps/backend/src/models/types.rs`, mirroring `TaskStatus`'s pattern (~line 1990).
- [x] 1.29 RED: unit test `sdd_phase_from_str_round_trips` — all 8 phases (`explore`, `propose`, `spec`, `design`, `tasks`, `apply`, `verify`, `archive`) parse and `Display` back; unknown returns `Err`. Plus `sdd_phase_ordering` asserting the DAG order used to infer "furthest phase present" (needed by PR-5's importer). Plus `sdd_status_from_str` for `active | archived | abandoned`.
- [x] 1.30 GREEN: implement `pub enum SddPhase` (with an `fn rank(&self) -> u8` ordering helper) and `pub enum SddStatus`, both with `FromStr` / `Display`, in `apps/backend/src/models/types.rs`.
- [x] 1.31 GREEN (no dedicated RED — struct definitions, covered transitively by PR-2/PR-3 tests): add the entity structs to `apps/backend/src/models/types.rs` in a new `// ── SDD artifacts ──` block after the Task block (~line 2018): `SddChange` (with `#[serde(default)] artifacts: Vec<SddArtifact>`, `task_links: Vec<Task>`, `memory_links: Vec<Memory>` hydrated only on detail reads), `SddChangeSummary` (id, project, name, title, phase, status — for `GlobalSearchResult`), `SddArtifact`, `SddArtifactDetail` (artifact + latest revision content), `SddRevision` (with content), `SddRevisionMeta` (**no content field at all** — the type itself enforces the metadata-only contract), `SddSearchHit` (artifact_id, change_name, kind, capability, snippet).
- [x] 1.32 GREEN (no dedicated RED): add the request structs to `apps/backend/src/models/types.rs`: `UpsertChangeRequest`, `PatchChangeRequest` (**`title`, `status`, `phase`, `sprint_id` only — no `project`, no `name`; the identity tuple is not patchable**, Spec: "Identity fields are not patchable"), `SaveArtifactRequest` (`project`, `change_name`, `kind`, `capability: Option<String>`, `content`, `path: Option<String>`, `git_commit: Option<String>`, `git_ref: Option<String>`), `LinkChangeMemoryRequest` (`memory_id`, `relation`), and `SddChangeFilters` (`project`, `status`, `phase`, `sprint_id`, `include_archived`), the last `Default`-derived like `TaskListFilters`.
- [x] 1.33 GREEN: `cargo fmt --manifest-path apps/backend/Cargo.toml` and fix any `clippy -D warnings` findings (unused imports, `#[allow(dead_code)]` on structs not yet consumed by a store fn until PR-2 lands).

### Gate

- [x] 1.34 GATE: `cargo test --manifest-path apps/backend/Cargo.toml` passes (including `tests/integration_test.rs` with the bumped 54 assertion).
- [x] 1.35 GATE: `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` is clean.
- [x] 1.36 GATE: `cargo fmt --manifest-path apps/backend/Cargo.toml -- --check` is clean.

---

## PR-2 — store layer (`db/queries.rs` SDD section)

**Goal**: the whole `// ── SDD artifacts ──` section of `apps/backend/src/db/queries.rs` — change CRUD, the `upsert_sdd_artifact` workhorse (transactional, hash de-duplicated, 1 MB-capped **atomically**, FTS-maintaining), revision reads, FTS5 search, memory links, task join, and `sdd_change_exists`. No HTTP surface.

**Satisfies**: `sdd-artifact-store` — all 12 requirements at the store layer. `sdd-artifact-links` — "Tasks Join to Changes by Name, Not by a Foreign Key", "A Change Exposes the Tasks Linked to It" (store half), "Changes Link to Memories Many-to-Many With a Relation" (store half), "A Change Belongs to One Project and Optionally One Sprint".

**Est. changed lines**: ~620
**Depends on**: PR-1

### Checklist

- [x] 2.1 RED: query test `upsert_sdd_change_creates_row_with_defaults` in `apps/backend/src/db/queries.rs` (`#[cfg(test)] mod tests`) — upserting with only `project` + `name` defaults `status='active'`, `phase='propose'`, sets `created_by` to the caller and populates both timestamps (Spec: sdd-artifact-store / "SDD Changes Are Org-Scoped …" / "Create a change").
- [x] 2.2 GREEN: implement `pub fn upsert_sdd_change(conn, org_id, created_by, req: &UpsertChangeRequest) -> Result<SddChange>` in `apps/backend/src/db/queries.rs` under a new `// ── SDD artifacts ──` section placed after the Tasks section (~line 6349).
- [x] 2.3 RED: query test `upsert_sdd_change_is_idempotent_on_org_project_name` — upserting twice with the same `(org_id, project, name)` and a new `title` returns the same `id`, updates the `title`, and creates exactly one row (Spec: "Re-submitting the same project and name upserts, not duplicates").
- [x] 2.4 GREEN: make `upsert_sdd_change` resolve-then-update (`SELECT id ... WHERE org_id=?1 AND project=?2 AND name=?3`, else `INSERT`) rather than blind-inserting.
- [x] 2.5 RED: query test `upsert_sdd_change_same_name_in_two_projects_are_two_changes` — `("nexus-mind", "team-tasks")` and `("kasymir", "team-tasks")` in the same org produce two distinct rows, neither overwriting the other. Plus `upsert_sdd_change_accepts_an_unregistered_project_name` — a project name with no `projects` record is stored verbatim and the change is created (D4: project is a name string, not a FK) (Spec: "The same name in two projects is two changes", "An unregistered project name is accepted").
- [x] 2.6 GREEN: confirm the `(org_id, project, name)` composite key handles both, and that no `projects` FK exists anywhere in the SDD tables.
- [x] 2.7 RED: query test `get_sdd_change_hydrates_artifact_inventory` — `get_sdd_change` returns the change with its `artifacts[]` populated (metadata only, **no content**) ordered by `kind`.
- [x] 2.8 GREEN: implement `pub fn get_sdd_change(conn, org_id, id) -> Result<Option<SddChange>>` in `apps/backend/src/db/queries.rs`, hydrating `artifacts` via a second query; `Ok(None)` when absent or out-of-org.
- [x] 2.9 RED: query test `get_sdd_change_org_isolation` — a change in org B is invisible to a `get_sdd_change` call scoped to org A (returns `Ok(None)`, not the row).
- [x] 2.10 GREEN: confirm `org_id` is in the `WHERE` of `get_sdd_change` (house rule: org in the `WHERE`, never in the caller).
- [x] 2.11 RED: query test `get_sdd_change_by_name_resolves_project_scoped_name` — `(org, project, name)` resolves; a same-`name` change in a different `project` is not returned.
- [x] 2.12 GREEN: implement `pub fn get_sdd_change_by_name(conn, org_id, project, name) -> Result<Option<SddChange>>`.
- [x] 2.13 RED: query test `list_sdd_changes_filters_by_project_status_phase_sprint` — each `SddChangeFilters` field narrows the result set independently and in combination; filtering by `sprint_id` returns exactly the changes assigned to that sprint, and a change with **no** `sprint_id` is still returned by an unfiltered list (Spec: sdd-artifact-links / "A Change Belongs to One Project and Optionally One Sprint" / "List the changes in a sprint", "A change without a sprint is valid"; sdd-artifact-store / "Filter changes by project and phase").
- [x] 2.14 GREEN: implement `pub fn list_sdd_changes(conn, org_id, filters: &SddChangeFilters) -> Result<Vec<SddChange>>` with dynamic `WHERE` (`String` + `push_str` + `Vec<&dyn ToSql>`, the `list_tasks` pattern). Returns **metadata only — never artifact content**.
- [x] 2.15 RED: query test `list_sdd_changes_excludes_archived_by_default_and_includes_them_on_request` — a change with `archived_at` set is absent by default and present with `include_archived: true` (Spec: "Archived changes are listable on request").
- [x] 2.16 GREEN: add the `archived_at IS NULL` default predicate to `list_sdd_changes`, bypassed by `include_archived`.
- [x] 2.17 RED: query test `patch_sdd_change_updates_title_status_phase_sprint_and_bumps_updated_at` — each field patches independently; `updated_at` moves; patching `sprint_id` assigns the change to that sprint (Spec: "Patch a change's phase"; sdd-artifact-links / "Assign a change to a sprint").
- [x] 2.18 GREEN: implement `pub fn patch_sdd_change(conn, org_id, id, req: &PatchChangeRequest) -> Result<SddChange>`, validating `phase` / `status` through the PR-1 enums and returning the string sentinel `anyhow!("invalid_phase")` / `anyhow!("invalid_status")` on a bad value.
- [x] 2.19 RED: query test `patch_sdd_change_cannot_alter_project_or_name` — **the identity tuple is immutable**. `PatchChangeRequest` carries no `project`/`name` field, so those columns are never named in the `UPDATE`; after any patch, `(project, name)` is byte-identical (Spec: "Identity fields are not patchable" — the 4xx rejection lands at the API layer in 3.23).
- [x] 2.20 GREEN: confirm `patch_sdd_change`'s `UPDATE` statement never names the `project` or `name` column.
- [x] 2.21 RED: query test `patch_sdd_change_rejects_invalid_phase_atomically` — patching `{phase: "shipped", title: "New"}` returns `Err("invalid_phase")` and **neither** the phase **nor** the title is changed (Spec: sdd-artifact-agent-tools / "update_sdd_change …" / "An invalid phase value is rejected atomically").
- [x] 2.22 GREEN: validate every enum field in `patch_sdd_change` **before** issuing the `UPDATE` (parse-then-write, not write-then-parse).
- [x] 2.23 RED: query test `archive_sdd_change_sets_archived_at_and_preserves_artifacts` — soft delete only; `archived_at` is set, the change drops out of the default list, and every artifact and revision remains retrievable via `get_sdd_change` by id (Spec: "Soft-archive a change", "Archived change's artifacts survive").
- [x] 2.24 GREEN: implement `pub fn archive_sdd_change(conn, org_id, id) -> Result<bool>` (plain `UPDATE ... SET archived_at = datetime('now')` — **no `DELETE`**).
- [x] 2.25 RED: query test `upsert_sdd_artifact_creates_change_artifact_and_revision_1` — calling with a `(project, change_name)` that does not exist yet creates the `sdd_changes` row, the `sdd_artifacts` row, revision `1`, sets `latest_revision = 1`, and returns `created_revision == true`, all in one atomic operation (Spec: "First save creates revision 1", "Saving an artifact for an unknown change creates the change").
- [x] 2.26 GREEN: implement `pub fn upsert_sdd_artifact(conn, org_id, created_by, req: &SaveArtifactRequest, source: &str) -> Result<(SddArtifact, bool)>` in `apps/backend/src/db/queries.rs` — one `conn.unchecked_transaction()` covering all 5 steps of design §3: resolve-or-create change → resolve-or-create artifact → hash → short-circuit → insert revision → replace FTS row.
- [x] 2.27 RED: query test `upsert_sdd_artifact_returns_created_revision_false_and_creates_no_revision_when_hash_unchanged` — **the de-dup contract (D2)**. Save the same content twice: the second call returns `created_revision == false`, `COUNT(*) FROM sdd_artifact_revisions` stays at 1, `latest_revision` stays at 1, `sdd_artifacts.updated_at` is byte-identical to its pre-call value, and the FTS row count for that `artifact_id` stays at 1 (Spec: "Identical re-save creates NO revision", "An idempotent re-save does not disturb the index").
- [x] 2.28 GREEN: implement the `sha256(content)` hash short-circuit in `upsert_sdd_artifact` — compare against the **latest** revision's `content_hash`; on match, return `(artifact, false)` **before** any write (no revision, no FTS write, no `updated_at` bump). Use the `sha2` crate already in `apps/backend/Cargo.toml` (harness manifest hashing) — no new dependency.
- [x] 2.29 RED: query test `upsert_sdd_artifact_appends_revision_2_on_changed_content` — different content → `created_revision == true`, revision `2` inserted, `latest_revision == 2`, revision `1`'s `content`, `content_hash`, and `byte_size` are **unchanged** and still individually retrievable (Spec: "Changed content appends a revision", "Revision content never changes after creation").
- [x] 2.30 GREEN: implement the append path in `upsert_sdd_artifact` — `INSERT` revision `latest_revision + 1`, `UPDATE sdd_artifacts SET latest_revision = ?, updated_at = datetime('now')`, all inside the transaction.
- [x] 2.31 RED: query test `upsert_sdd_artifact_revert_to_earlier_content_appends_revision_3` — **A1**. Save content A (rev 1), then content B (rev 2), then content A again: the store MUST append **revision 3** containing A. It MUST NOT reuse, resurrect, or renumber revision 1; `latest_revision` becomes 3; revision 1 still exists independently. The hash comparison is made **only against the latest revision**, never against the full history — a revert is an event and must appear in the history (Spec: "Reverting to earlier content appends a new revision").
- [x] 2.32 GREEN: confirm `upsert_sdd_artifact`'s hash lookup selects `WHERE artifact_id = ?1 ORDER BY revision DESC LIMIT 1` (latest only) and **not** `WHERE artifact_id = ?1 AND content_hash = ?2` (any-revision), which would wrongly collapse the revert. Add the comment naming A1.
- [x] 2.33 RED: query test `upsert_sdd_artifact_revision_numbering_is_monotonic_and_gapless_per_artifact` — two artifacts under one change, each at revision 1; two further changed saves to one of them take it to revision 3 while the other stays at revision 1; revision numbers are `1,2,3` with no gaps and no reuse (Spec: "Revision numbering is monotonic per artifact").
- [x] 2.34 GREEN: confirm `latest_revision + 1` (not a global counter, not a `MAX()` across artifacts) is the revision source in `upsert_sdd_artifact`.
- [x] 2.35 RED: query test `upsert_sdd_artifact_replaces_fts_row_on_new_revision` — **the FTS maintenance contract**. Save revision 1 containing the token `ALPHAWORD`, then revision 2 containing `BETAWORD`. `search_sdd_artifacts(org, "ALPHAWORD")` returns **zero** hits and `search_sdd_artifacts(org, "BETAWORD")` returns **one**; `SELECT COUNT(*) FROM sdd_artifacts_fts WHERE artifact_id = ?` is exactly `1` — the index tracks the latest revision only and never accumulates, so an artifact with five revisions all mentioning a term contributes exactly one hit (Spec: "A term removed by a newer revision stops matching", "An artifact contributes at most one search hit").
- [x] 2.36 GREEN: implement the delete-then-insert FTS maintenance in `upsert_sdd_artifact` — `DELETE FROM sdd_artifacts_fts WHERE artifact_id = ?1` then `INSERT INTO sdd_artifacts_fts (artifact_id, change_name, kind, capability, content) VALUES (...)`, inside the same transaction, only on the append path.
- [x] 2.37 RED: query test `upsert_sdd_artifact_rejects_content_over_1mb_atomically` — **A2**. Content of `1_048_577` bytes for a change name that does **not** yet exist returns `Err("artifact_too_large")` **and leaves no partial state**: `SELECT COUNT(*) FROM sdd_changes WHERE name = 'oversized'` is `0`, `COUNT(*) FROM sdd_artifacts` is `0`, `COUNT(*) FROM sdd_artifact_revisions` is `0`. Also assert the pre-existing-artifact case: an oversized save against an existing artifact leaves its `latest_revision` and `updated_at` untouched (Spec: "Oversized content is rejected with 422", "A rejected oversized save leaves no partial state").
- [x] 2.38 GREEN: add the `if req.content.len() > 1_048_576 { return Err(anyhow!("artifact_too_large")) }` guard as the **first statement** of `upsert_sdd_artifact` — before the transaction opens and before the change/artifact rows are resolved-or-created. (The transaction is the belt; this guard is the braces — both must hold, per A2.)
- [x] 2.39 RED: query test `upsert_sdd_artifact_accepts_content_just_under_the_cap` — content of `1_048_575` bytes is accepted and its recorded `byte_size` equals the submitted content's byte length exactly (Spec: "Content at or under the cap is accepted").
- [x] 2.40 GREEN: compute `byte_size` as `content.len()` (bytes, not chars) in `upsert_sdd_artifact`.
- [x] 2.41 RED: query test `upsert_sdd_artifact_defaults_capability_to_empty_string` — a `kind='design'` save with `capability: None` persists `capability = ''` (not NULL); a second `design` save with `capability` explicitly null resolves to the **same** artifact row and the artifact count for that change is unchanged (Spec: "Two saves of the same kind converge on one artifact", "Omitted capability MUST NOT create a duplicate artifact row").
- [x] 2.42 GREEN: normalize `req.capability.as_deref().unwrap_or("")` in `upsert_sdd_artifact`'s artifact-resolution lookup **and** insert.
- [x] 2.43 RED: query test `upsert_sdd_artifact_spec_capabilities_have_independent_revision_histories` — two `spec` saves with distinct capabilities produce two artifacts; revising one leaves the other's `latest_revision` untouched (Spec: "Spec artifacts are discriminated by capability").
- [x] 2.44 GREEN: confirm the artifact-resolution lookup keys on `(change_id, kind, capability)` — all three.
- [x] 2.45 RED: query test `upsert_sdd_artifact_persists_provenance_and_source_without_clobbering_earlier_revisions` — `git_commit`, `git_path` (from `req.path`), `byte_size`, and `source` land on the revision row; a **later** revision saved with no provenance does **not** overwrite the earlier revision's `git_commit`/`git_path` (revisions are immutable) (Spec: "Git provenance is recorded per revision").
- [x] 2.46 GREEN: wire the provenance columns into the revision `INSERT` **only** (never an `UPDATE` of an existing revision row); accept `source` as a parameter (`'agent' | 'admin' | 'import'`) so PR-5's importer can pass `'import'`.
- [x] 2.47 RED: query test `no_store_function_updates_or_deletes_a_revision` — an API-surface invariant assertion: the SDD section of `queries.rs` exposes **no** `update_sdd_artifact_revision` / `delete_sdd_artifact_revision` fn, and no `UPDATE sdd_artifact_revisions` or `DELETE FROM sdd_artifact_revisions` statement appears anywhere in the file (assert with a source scan over `include_str!("queries.rs")` or an equivalent module-level check) (Spec: "No API mutates an existing revision").
- [x] 2.48 GREEN: confirm the invariant — revisions are written by `upsert_sdd_artifact`'s `INSERT` and removed only by `ON DELETE CASCADE` from the parent change. Add the invariant comment to the section header.
- [x] 2.49 RED: query test `upsert_sdd_artifact_does_not_mutate_the_changes_phase` — **phase is advisory**. Saving a `design` artifact to a change in phase `spec` leaves `phase == 'spec'`; saving a `verify-report` to a change in phase `propose` **succeeds** (no out-of-order rejection) and still leaves `phase == 'propose'` (Spec: sdd-artifact-store / "Phase Is Advisory Metadata, Not a Write Gate" — both scenarios).
- [x] 2.50 GREEN: confirm `upsert_sdd_artifact` never reads, writes, or gates on `phase`. Add the comment: the artifact inventory is the ground truth; advancing the phase requires an explicit `patch_sdd_change`.
- [x] 2.51 RED: query test `upsert_sdd_artifact_org_isolation` — an org-B caller saving `(project, name)` identical to an org-A change creates a **separate** change under org B; org A's change, artifacts, and revisions are unmodified (Spec: "Cross-org save does not hijack another org's change").
- [x] 2.52 GREEN: confirm `org_id` scopes the change-resolution `SELECT` inside `upsert_sdd_artifact`.
- [x] 2.53 RED: query test `get_sdd_artifact_returns_latest_revision_content` — `SddArtifactDetail` carries the latest revision's **complete, untruncated** content plus the revision number and `content_hash` (Spec: "Fetching an artifact returns its latest revision's full content").
- [x] 2.54 GREEN: implement `pub fn get_sdd_artifact(conn, org_id, id) -> Result<Option<SddArtifactDetail>>` in `apps/backend/src/db/queries.rs`, joining through `sdd_changes` for the `org_id` predicate (artifacts have no `org_id` column of their own — it is inherited via `change_id`).
- [x] 2.55 RED: query test `get_sdd_artifact_by_kind_resolves_spec_by_capability` — `(project, change_name, 'spec', Some("sdd-artifact-store"))` returns that capability's spec, not another's; `(…, 'design', None)` resolves via the `''` sentinel; a kind that has no artifact returns `Ok(None)` — **not** an artifact with empty content (Spec: sdd-artifact-agent-tools / "A missing artifact reports not-found, not an empty document").
- [x] 2.56 GREEN: implement `pub fn get_sdd_artifact_by_kind(conn, org_id, project, change_name, kind, capability) -> Result<Option<SddArtifactDetail>>` — the natural-key lookup behind `GET /v1/sdd/artifacts?project=&change_name=&kind=&capability=` (design §4).
- [x] 2.57 RED: query test `list_sdd_artifact_revisions_returns_metadata_only_newest_first` — the returned `SddRevisionMeta` rows carry `revision`, `content_hash`, `byte_size`, `source`, `git_path`, `git_commit`, `created_by`, `created_at` and **no `content` field at all**; ordered `revision DESC` (Spec: "Revision list carries metadata but no content").
- [x] 2.58 GREEN: implement `pub fn list_sdd_artifact_revisions(conn, org_id, artifact_id) -> Result<Vec<SddRevisionMeta>>` — the `SELECT` must not name the `content` column.
- [x] 2.59 RED: query test `get_sdd_artifact_revision_returns_full_content_for_a_specific_rev` — fetching revision `1` after revision `3` exists returns revision 1's original content byte-for-byte, complete and untruncated; revision 3's content is not returned (Spec: "Fetching a specific revision returns that revision's full content").
- [x] 2.60 GREEN: implement `pub fn get_sdd_artifact_revision(conn, org_id, artifact_id, revision) -> Result<Option<SddRevision>>`.
- [x] 2.61 RED: query test `search_sdd_artifacts_returns_snippets_scoped_to_org` — an FTS5 `MATCH` over content returns an `SddSearchHit` carrying a `snippet()` excerpt **and** the change name, kind, and capability needed to fetch the full document; a matching artifact in org B is **not** returned to an org-A search (Spec: "A term in the latest revision is findable", "Search never crosses the organization boundary").
- [x] 2.62 GREEN: implement `pub fn search_sdd_artifacts(conn, org_id, q, limit) -> Result<Vec<SddSearchHit>>` — `JOIN sdd_artifacts a ON a.id = fts.artifact_id JOIN sdd_changes c ON c.id = a.change_id WHERE sdd_artifacts_fts MATCH ?1 AND c.org_id = ?2`, using `snippet(sdd_artifacts_fts, 4, '<b>', '</b>', '…', 24)` and `LIMIT ?3`. Mirror the `search_memories_visible` FTS shape (`queries.rs:320`).
- [x] 2.63 RED: query test `search_sdd_artifacts_spans_changes_and_honours_the_limit` — matching artifacts under three different changes all come back; with `limit: 5` against 20 matches, at most 5 rows are returned (Spec: sdd-artifact-agent-tools / "Search spans changes, not just the current one", "Results honour the limit").
- [x] 2.64 GREEN: confirm `limit` is bound into the `LIMIT` clause, not applied client-side.
- [x] 2.65 RED: query test `search_sdd_artifacts_sanitizes_fts_query_syntax` — a query containing FTS5 metacharacters (e.g. `foo"bar`, a bare `*`) does not error the statement (returns empty or matches literally) rather than propagating a `SqliteFailure`.
- [x] 2.66 GREEN: reuse/extract the existing FTS query-sanitization helper used by `search_memories_visible`; do not hand-roll a second escaper.
- [x] 2.67 RED: query test `link_sdd_change_memory_is_idempotent_and_rejects_cross_org_memory` — linking `(change_id, memory_id)` and then linking the same pair again succeeds both times and leaves exactly one row; linking a memory belonging to org B from an org-A change returns `Err("memory_not_found")` and creates no row (Spec: "Link a memory produced by a change", "Re-linking the same pair creates no duplicate", "Cross-org memory link returns 404").
- [x] 2.68 GREEN: implement `pub fn link_sdd_change_memory(conn, org_id, change_id, memory_id, relation, linked_by) -> Result<()>` — validate that **both** the change and the memory are in `org_id`, then insert.
- [x] 2.69 RED: query test `link_sdd_change_memory_with_a_different_relation_updates_the_existing_row` — **A3**. Link `(change, memory)` with `relation: "informed"`, then link the same pair with `relation: "produced"`: the call succeeds, there is still exactly **one** link row, and its `relation` is now `"produced"`. Re-linking with the **same** relation remains a silent no-op success.
- [x] 2.70 GREEN: implement the link as `INSERT INTO sdd_change_memories (...) VALUES (...) ON CONFLICT(change_id, memory_id) DO UPDATE SET relation = excluded.relation` (relies on `UNIQUE(change_id, memory_id)` from PR-1). **Not `INSERT OR IGNORE`** — that would silently drop the relation change.
- [x] 2.71 RED: query test `unlink_sdd_change_memory_removes_the_link_but_not_the_memory` — the link is gone, the `memories` row still exists; a second unlink returns `false` (Spec: "Unlink a memory").
- [x] 2.72 GREEN: implement `pub fn unlink_sdd_change_memory(conn, org_id, change_id, memory_id) -> Result<bool>`.
- [x] 2.73 RED: query test `list_sdd_change_memories_returns_hydrated_memories`.
- [x] 2.74 GREEN: implement `pub fn list_sdd_change_memories(conn, org_id, change_id) -> Result<Vec<Memory>>`, reusing the existing memory row-mapper.
- [x] 2.75 RED: query test `list_tasks_for_sdd_change_joins_task_spec_links_by_name` — three tasks linked to `"sdd-artifacts"` via `task_spec_links.spec_change_name` are returned for the change whose `name` is `"sdd-artifacts"`, each with its status and title; an archived task is excluded; a task in another org is excluded; a change with no links returns an **empty vec, not an error** (D3: the join key is the **name**, not a new FK) (Spec: "Linked tasks are returned for a change", "A change with no linked tasks returns an empty list").
- [x] 2.76 GREEN: implement `pub fn list_tasks_for_sdd_change(conn, org_id, change_name, viewer: Option<&str>) -> Result<Vec<Task>>` in `apps/backend/src/db/queries.rs`, joining `tasks` → `task_spec_links` on `spec_change_name = ?change_name`, reusing `map_task_row`.
- [x] 2.77 RED: query test `list_tasks_for_sdd_change_applies_task_visibility` — two tasks linked to the change, one in a project the viewer is **not** a member of: only the visible task is returned, and nothing in the result reveals that another linked task exists (Spec: "Tasks the caller cannot see are excluded").
- [x] 2.78 GREEN: apply the existing `visibility_predicate("project", "?N")` (the helper `list_tasks` uses) inside `list_tasks_for_sdd_change`, passing `None` for a privileged viewer.
- [x] 2.79 RED: query test `a_spec_link_created_before_the_change_existed_resolves_once_the_change_appears` — link a task to `"sdd-artifacts"` while **no** `sdd_changes` row of that name exists; later create the change in the same org and project; `list_tasks_for_sdd_change(org, "sdd-artifacts")` now returns the task, with **no re-linking** and no mutation of `task_spec_links` (Spec: sdd-artifact-links / "Tasks Join to Changes by Name, Not by a Foreign Key" / "A link created before the change existed resolves once the change appears").
- [x] 2.80 GREEN: confirm the join is a pure name join with no FK and no materialized edge — and assert by source scan that **no `change_id` column was added to `tasks`** and no `Task` field references a change id (Spec: "The link survives with no duplicate source of truth").
- [x] 2.81 RED: query test `sdd_change_exists_matches_by_name_across_projects_and_respects_org` — **A8**. Returns `true` for a change with that `name` in **any** project of the org (the task-link key is project-agnostic), `true` for an **archived** change (an archived change remains a legitimate link target — matching the FS check that globs the archive tree), `false` for an unknown name, and `false` for a name that only exists in another org.
- [x] 2.82 GREEN: implement `pub fn sdd_change_exists(conn, org_id, name) -> Result<bool>` — `SELECT EXISTS(SELECT 1 FROM sdd_changes WHERE org_id = ?1 AND name = ?2)` with **no** `archived_at` predicate.
- [x] 2.83 GREEN: `cargo fmt --manifest-path apps/backend/Cargo.toml`; resolve `clippy -D warnings` (expect `too_many_arguments` on `upsert_sdd_artifact` — take `&SaveArtifactRequest` rather than `#[allow]`-ing it).

### Gate

- [x] 2.84 GATE: `cargo test --manifest-path apps/backend/Cargo.toml` passes.
- [x] 2.85 GATE: `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` is clean.
- [x] 2.86 GATE: `cargo fmt --manifest-path apps/backend/Cargo.toml -- --check` is clean.

---

## PR-3 — API (`api/sdd.rs` + routes)

**Goal**: the 14 `/v1/sdd/*` handlers with the full permission / 404-not-403 / org-isolation / 422 / idempotency test matrix. Handler shape copied from `api/tasks.rs`: `State` → `Extension(auth)` → `Path` → `Query` → `AppJson`, `require_permission` first, local `db_err` mapping the store's string sentinels.

**Satisfies**: `sdd-artifact-store` — "SDD Operations Are Gated by sdd:read, sdd:write, and sdd:delete", "SDD Data Is Isolated Per Organization and Never Leaks Existence", "List Endpoints Return Metadata Only, Never Artifact Content", "Artifact Content Is Capped at 1 MB" (HTTP half), "Artifact Revisions Are Immutable and Append-Only" (the no-mutation-endpoint half), "Changes Are Soft-Archived, Never Hard-Deleted", "Change Listing Supports Filtering, and Change Metadata Is Patchable", "Artifacts Are Full-Text Searchable Over Their Latest Revision Only" (HTTP half), "Saving an Artifact Is Idempotent by Content Hash" (HTTP half). `sdd-artifact-links` — "Changes Link to Memories Many-to-Many With a Relation" (HTTP half).

**Est. changed lines**: ~520
**Depends on**: PR-2

### Checklist

- [x] 3.1 GREEN (scaffold, no dedicated RED — every test below fails to compile without it): create `apps/backend/src/api/sdd.rs` with the module header, `use` block, `fn db_err(e: anyhow::Error)` mapping `artifact_too_large` → 422 `{code: "artifact_too_large"}`, `invalid_phase` / `invalid_status` / `invalid_kind` → 422, `memory_not_found` → 404, everything else → 500; `fn not_found()` → 404 `{code: "not_found"}`. Register `pub mod sdd;` in `apps/backend/src/api/mod.rs`.
- [x] 3.2 GREEN (test harness): add the `#[cfg(test)] mod tests` block at the bottom of `apps/backend/src/api/sdd.rs`, copying the `api/tasks.rs:619-745` harness verbatim — `make_store()`, `app(store)` router builder, `setup_with_key()` (via `q::bootstrap`), `create_member_with_id()`, `admin_user_id()`, `add_member_to_project()`, `post_json`/`put_json`/`get_json`/`patch_json`/`delete_req` helpers, `body_json()`. Add a `grant_perm(store, user_id, perms: &[&str])` helper that attaches a custom role holding exactly the listed permission strings, so the per-permission 403 tests can isolate one string at a time.
- [x] 3.3 RED: HTTP test `list_sdd_changes_denied_without_sdd_read` in `apps/backend/src/api/sdd.rs` — a caller with `sdd:write` but not `sdd:read` gets `403` on `GET /v1/sdd/changes` and no metadata is returned (Spec: "Read denied without sdd:read").
- [x] 3.4 GREEN: implement `pub async fn list_changes_handler` — `require_permission(&conn, &auth, None, "sdd:read")`, parses `project` / `status` / `phase` / `sprint_id` / `include_archived` query params into `SddChangeFilters`, calls `queries::list_sdd_changes`.
- [x] 3.5 RED: HTTP test `list_sdd_changes_returns_metadata_only_never_content` — with a change carrying a 36 KB `design` artifact, the JSON body contains no `content` key at any nesting level (assert on the serialized `serde_json::Value`, not the Rust type) (Spec: "Change list carries no content").
- [x] 3.6 GREEN: confirm `SddChange`'s serialization in the list path omits content; add a `#[serde(skip_serializing_if)]` if the type leaks it.
- [x] 3.7 RED: HTTP test `list_sdd_changes_org_isolation` — an org-B key never sees an org-A change.
- [x] 3.8 GREEN: confirm `auth.org_id` is the only org the handler passes to the store.
- [x] 3.9 RED: HTTP test `privileged_role_bypasses_sdd_permission_checks` — an org **admin** (and a `super_user`) with **no explicit `sdd:*` grant** can list, save, patch, and archive successfully (Spec: "Privileged roles bypass the permission check").
- [x] 3.10 GREEN: confirm every SDD handler goes through `require_permission`, which already honours `UserRole::is_privileged()` — no per-handler bypass code.
- [x] 3.11 RED: HTTP test `create_sdd_change_denied_without_sdd_write` — `403` with `sdd:read` only; no row created.
- [x] 3.12 GREEN: implement `pub async fn create_change_handler` — `POST /v1/sdd/changes`, `require_permission(..., "sdd:write")`, calls `queries::upsert_sdd_change`.
- [x] 3.13 RED: HTTP test `create_sdd_change_upserts_by_project_and_name` — posting the same `(project, name)` twice returns the same `id` both times (no duplicate, no 409) (Spec: "Re-submitting the same project and name upserts, not duplicates").
- [x] 3.14 GREEN: confirm `create_change_handler` relies on `upsert_sdd_change`'s resolve-then-update path.
- [x] 3.15 RED: HTTP test `get_sdd_change_returns_404_for_other_org_not_403` — an org-B caller **holding `sdd:read`** fetching an org-A change id gets `404`, not `403` and not `200`; a never-existed id also gets `404`, with no signal that the id is valid (Spec: "Cross-org artifact fetch returns 404, not 403", "Unknown artifact id returns 404").
- [x] 3.16 GREEN: implement `pub async fn get_change_handler` — `GET /v1/sdd/changes/:id`, `sdd:read`, hydrated (artifact inventory + task links + memory links), `not_found()` on `Ok(None)`.
- [x] 3.17 RED: HTTP test `get_sdd_change_hydrates_artifacts_tasks_and_memories` — the detail body carries `artifacts[]`, `task_links[]`, `memory_links[]`; an **archived** change fetched by id still returns its full artifact inventory and every revision remains retrievable (Spec: "Archived change's artifacts survive").
- [x] 3.18 GREEN: wire `queries::list_tasks_for_sdd_change` and `queries::list_sdd_change_memories` into `get_change_handler`; do not filter archived changes on the by-id read.
- [x] 3.19 RED: HTTP test `patch_sdd_change_denied_without_sdd_write` — `403`; the change is unmodified (Spec: "Patch denied without sdd:write").
- [x] 3.20 GREEN: implement `pub async fn patch_change_handler` — `PATCH /v1/sdd/changes/:id`, `sdd:write`, `title` / `status` / `phase` / `sprint_id`.
- [x] 3.21 RED: HTTP test `patch_sdd_change_rejects_invalid_phase_with_422_atomically` — `{"phase": "shipped", "title": "New"}` → `422 {code: "invalid_phase"}`, and **neither** the phase **nor** the title is applied (Spec: "An invalid phase value is rejected atomically").
- [x] 3.22 GREEN: confirm the store's `invalid_phase` sentinel maps to 422 in `sdd.rs`'s `db_err` and that `patch_sdd_change` validates before writing (2.22).
- [x] 3.23 RED: HTTP test `patch_sdd_change_rejects_project_or_name_with_4xx` — a `PATCH` body containing `{"project": "other"}` or `{"name": "renamed"}` returns a 4xx validation error and the change's identity tuple is unchanged (Spec: "Identity fields are not patchable").
- [x] 3.24 GREEN: add `#[serde(deny_unknown_fields)]` to `PatchChangeRequest` (or an explicit reject-if-present check in `patch_change_handler`) so an identity field in the body is a 422, not a silent no-op.
- [x] 3.25 RED: HTTP test `delete_sdd_change_requires_sdd_delete_not_just_write` — a caller with `sdd:read` **and** `sdd:write` but **not** `sdd:delete` gets `403`; `archived_at` stays NULL (Spec: "Archive denied without sdd:delete").
- [x] 3.26 GREEN: implement `pub async fn delete_change_handler` — `DELETE /v1/sdd/changes/:id`, `require_permission(..., "sdd:delete")`, calls `queries::archive_sdd_change` (soft), returns `204`.
- [x] 3.27 RED: HTTP test `delete_unknown_or_cross_org_sdd_change_returns_404` (Spec: "Archiving an unknown or cross-org change returns 404").
- [x] 3.28 GREEN: return `not_found()` when `archive_sdd_change` reports `false`.
- [x] 3.29 RED: HTTP test `put_sdd_artifact_denied_without_sdd_write` — `403` on `PUT /v1/sdd/artifacts` for a caller with `sdd:read` only; **no** change, artifact, or revision row is created (Spec: "Save denied without sdd:write").
- [x] 3.30 GREEN: implement `pub async fn put_artifact_handler` — `PUT /v1/sdd/artifacts`, `sdd:write`, body `SaveArtifactRequest`, calls `queries::upsert_sdd_artifact` with `source = "agent"`, returns `(StatusCode::OK, Json(json!({"artifact": a, "created_revision": created})))`.
- [x] 3.31 RED: HTTP test `put_sdd_artifact_returns_200_not_201_and_created_revision_true_on_first_save` — **the idempotency contract at the HTTP boundary**. First `PUT` → `200` (never `201`) with `created_revision: true` and `latest_revision: 1` (Spec: "First save creates revision 1" — "the response status is 200, not 201").
- [x] 3.32 GREEN: confirm `put_artifact_handler` always returns `StatusCode::OK` on the success path (design §4: "200 always (idempotent), never 201").
- [x] 3.33 RED: HTTP test `put_sdd_artifact_second_identical_save_returns_created_revision_false` — the same body twice → the second response is `200 { created_revision: false }` and `GET /v1/sdd/artifacts/:id/revisions` still lists exactly one revision.
- [x] 3.34 GREEN: confirm the handler surfaces the store's `bool` unchanged (no re-derivation).
- [x] 3.35 RED: HTTP test `put_sdd_artifact_over_1mb_returns_422_and_creates_nothing` — **A2 at the HTTP boundary**. A >1 MB `content` for a brand-new change name → `422 {code: "artifact_too_large"}`, and afterwards no change of that name exists, no artifact row exists, and no revision row exists (Spec: "Oversized content is rejected with 422", "A rejected oversized save leaves no partial state").
- [x] 3.36 GREEN: confirm `artifact_too_large` maps to 422 in `sdd.rs`'s `db_err`. **Check the body-size limit**: Axum's default `RequestBodyLimit` (2 MB) must not reject the 1 MB+ payload before the handler sees it — if it does, raise the limit on this route only via `DefaultBodyLimit::max(...)` in `apps/backend/src/api/router.rs` so the client gets the 422, not a 413.
- [x] 3.37 RED: HTTP test `put_sdd_artifact_rejects_unknown_kind_with_422_and_creates_nothing` — `{"kind": "not-a-kind"}` for a brand-new change name → `422 {code: "invalid_kind"}`, and **no change and no artifact** are created (Spec: "Reject an unrecognized artifact kind" — "MUST NOT create an artifact or a change").
- [x] 3.38 GREEN: validate `kind` through `SddArtifactKind::from_str` in `put_artifact_handler` **before** touching the store; map to 422.
- [x] 3.39 RED: HTTP test `get_sdd_artifact_by_id_returns_latest_content_and_denies_without_sdd_read` — `200` with the complete, untruncated latest-revision content and its revision number for a caller with `sdd:read`; `403` for a caller without it.
- [x] 3.40 GREEN: implement `pub async fn get_artifact_handler` — `GET /v1/sdd/artifacts/:id`, `sdd:read`, returns `SddArtifactDetail`.
- [x] 3.41 RED: HTTP test `get_sdd_artifact_by_natural_key_resolves_change_kind_and_capability` — `GET /v1/sdd/artifacts?project=nexus-mind&change_name=sdd-artifacts&kind=spec&capability=sdd-artifact-store` returns that capability's spec; omitting `capability` for `kind=design` resolves via the `''` sentinel; a kind with no artifact returns `404`, **not** a 200 with empty content (Spec: sdd-artifact-agent-tools / "Addressable by change name and kind", "A missing artifact reports not-found, not an empty document").
- [x] 3.42 GREEN: implement `pub async fn get_artifact_by_key_handler` — `GET /v1/sdd/artifacts` (collection route, `Query<ArtifactKeyParams>`), `sdd:read`, calls `queries::get_sdd_artifact_by_kind`, `not_found()` on `Ok(None)`.
- [x] 3.43 RED: HTTP test `get_sdd_artifact_from_other_org_returns_404` — org isolation on **both** the by-id and the natural-key path (artifacts have no `org_id` column; the check goes through `change_id` — this proves the join is actually there).
- [x] 3.44 GREEN: confirm both artifact handlers pass `auth.org_id` to the store, which joins through `sdd_changes`.
- [x] 3.45 RED: HTTP test `list_artifact_revisions_returns_metadata_without_content` — `GET /v1/sdd/artifacts/:id/revisions` for a 3-revision artifact returns three entries with revision number, byte size, content hash, source, and author, and **no `content` key** (Spec: "Revision list carries metadata but no content").
- [x] 3.46 GREEN: implement `pub async fn list_artifact_revisions_handler` — `sdd:read`, calls `queries::list_sdd_artifact_revisions`.
- [x] 3.47 RED: HTTP test `get_artifact_revision_returns_full_content_for_older_rev` — `GET /v1/sdd/artifacts/:id/revisions/1` after rev 3 exists returns rev 1's complete content; a nonexistent rev returns `404`.
- [x] 3.48 GREEN: implement `pub async fn get_artifact_revision_handler` — `sdd:read`, `Path((id, rev)): Path<(String, i64)>`.
- [x] 3.49 RED: HTTP test `no_endpoint_mutates_or_deletes_a_revision` — `PUT`, `PATCH`, and `DELETE` against `/v1/sdd/artifacts/:id/revisions/:rev` all return `405 Method Not Allowed` (no such route is registered) and the revision remains intact (Spec: "No API mutates an existing revision").
- [x] 3.50 GREEN: confirm the revision routes are registered with `get(...)` only in `apps/backend/src/api/router.rs`.
- [x] 3.51 RED: HTTP test `search_sdd_artifacts_denied_without_sdd_read` (403, Spec: "Search denied without sdd:read") and `search_sdd_artifacts_returns_snippets_and_honours_limit` (200; each hit carries `snippet`, `change_name`, `kind`, `capability`; `?limit=5` against 20 matches returns at most 5).
- [x] 3.52 GREEN: implement `pub async fn search_handler` — `GET /v1/sdd/search?q=&limit=`, `sdd:read`, clamps `limit` to `1..=50` (the `search.rs` precedent), calls `queries::search_sdd_artifacts`.
- [x] 3.53 RED: HTTP test `search_sdd_artifacts_with_empty_q_returns_empty_list_not_500` — `?q=` (or whitespace) short-circuits to `[]` (the `get_global_search` precedent at `search.rs:53`).
- [x] 3.54 GREEN: add the `q.trim().is_empty()` short-circuit to `search_handler`.
- [x] 3.55 RED: HTTP test `list_change_artifacts_returns_inventory_without_content` — `GET /v1/sdd/changes/:id/artifacts`, `sdd:read`, returns kind / capability / path / latest_revision / timestamps and no content.
- [x] 3.56 GREEN: implement `pub async fn list_change_artifacts_handler`.
- [x] 3.57 RED: HTTP test `link_change_memory_denied_without_sdd_write` (403; the change's memory links are unchanged — Spec: "Memory link denied without sdd:write") and `link_change_memory_is_idempotent` — `POST /v1/sdd/changes/:id/memories` twice with the same `memory_id` succeeds both times and creates exactly one link row.
- [x] 3.58 GREEN: implement `pub async fn link_change_memory_handler` — `sdd:write`, body `{memory_id, relation}` (`relation` defaults to `produced`), calls `queries::link_sdd_change_memory`.
- [x] 3.59 RED: HTTP test `relinking_a_memory_with_a_different_relation_updates_it` — **A3 at the HTTP boundary**. `POST` with `relation: "informed"`, then `POST` the same pair with `relation: "produced"`: both succeed, one link row exists, and its `relation` on the change's memory links is now `"produced"`.
- [x] 3.60 GREEN: confirm the handler passes `relation` straight through to the store's `ON CONFLICT DO UPDATE` (2.70).
- [x] 3.61 RED: HTTP test `link_change_memory_with_other_org_memory_returns_404` — a memory id from org B → `404` (not 403, not 422 — the memory does not exist from this caller's view), no link created, no signal that the id is valid (Spec: "Cross-org memory link returns 404").
- [x] 3.62 GREEN: confirm `memory_not_found` maps to 404 in `sdd.rs`'s `db_err`.
- [x] 3.63 RED: HTTP test `unlink_change_memory_denied_without_sdd_write` (403) and returns `204` on success / `404` when the link is absent; the memory itself is not deleted.
- [x] 3.64 GREEN: implement `pub async fn unlink_change_memory_handler` — `DELETE /v1/sdd/changes/:id/memories/:memory_id`, `sdd:write`.
- [x] 3.65 GREEN: mount every route on the `protected` router in `apps/backend/src/api/router.rs`, **static paths first** (the static-first discipline `/v1/tasks/resolve-by-spec` had to follow at `router.rs:224`): `GET /v1/sdd/search`, then `GET|PUT /v1/sdd/artifacts` (collection: natural-key read + the workhorse write), **then** `GET /v1/sdd/artifacts/:id`, `GET /v1/sdd/artifacts/:id/revisions`, `GET /v1/sdd/artifacts/:id/revisions/:rev`; then `GET|POST /v1/sdd/changes`, `GET|PATCH|DELETE /v1/sdd/changes/:id`, `GET /v1/sdd/changes/:id/artifacts`, `POST /v1/sdd/changes/:id/memories`, `DELETE /v1/sdd/changes/:id/memories/:memory_id`. Add `sdd` to the `use crate::api::{...}` import list at `router.rs:13`.
- [x] 3.66 RED: HTTP test `sdd_routes_require_authentication` — every `/v1/sdd/*` path with no `Authorization` header returns `401` (proves the routes landed on the `protected` router, not the public one).
- [x] 3.67 GREEN: confirm the routes are chained onto `protected` in `apps/backend/src/api/router.rs`, behind `auth_mw::auth`.
- [x] 3.68 GREEN: `cargo fmt` + resolve `clippy -D warnings` on `apps/backend/src/api/sdd.rs`.

### Gate

- [x] 3.69 GATE: `cargo test --manifest-path apps/backend/Cargo.toml` passes.
- [x] 3.70 GATE: `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` is clean.
- [x] 3.71 GATE: `cargo fmt --manifest-path apps/backend/Cargo.toml -- --check` is clean.

---

## PR-4 — integration: `spec_change_exists` DB-first, change→tasks, global-search facet

**Goal**: close the three seams between the new SDD domain and what already ships — the Fly.io `spec_change_exists` hole (D5), the change→tasks read, and the additive `GlobalSearchResult` field.

**Satisfies**: `sdd-artifact-links` — "Link Creation Validates the Change Name Against the Openspec Trees" (MODIFIED), "A Change Exposes the Tasks Linked to It", "Global Search Includes an SDD Facet".

**Est. changed lines**: ~250
**Depends on**: PR-3

### Checklist

- [x] 4.1 RED: unit test `spec_change_exists_returns_true_when_change_is_in_the_db` in `apps/backend/src/api/tasks.rs` — with an `sdd_changes` row named `"sdd-artifacts"` in the org and `OPENSPEC_ROOT` pointed at an empty temp dir (so the FS branch cannot pass), the new DB-first `spec_change_exists` returns `true` without requiring a filesystem read (Spec: "Link to a change known to the SDD store succeeds without touching the filesystem"). **Hold `openspec_root_env_lock()`** for the duration — `OPENSPEC_ROOT` is process-global and `cargo test` runs threads in parallel (see the comment at `tasks.rs:635`).
- [x] 4.2 GREEN: add `pub fn spec_change_exists(conn: &Connection, org_id: &str, name: &str) -> bool` to `apps/backend/src/api/tasks.rs` — `if queries::sdd_change_exists(conn, org_id, name).unwrap_or(false) { return true }` then fall through to the filesystem check. **Rename the existing FS fn (currently `spec_change_exists(root, name)` at `tasks.rs:375`) to `fs_spec_change_exists(root, name)`, keeping its body byte-for-byte** — including the permissive-on-unreadable-root branch. Update its doc comment to say it is now the fallback.
- [x] 4.3 RED: unit test `spec_change_exists_falls_back_to_the_active_filesystem_tree` — no `sdd_changes` row, but `OPENSPEC_ROOT` points at a temp dir containing `openspec/changes/local-only/` → `true` (Spec: "Link to an on-disk change not yet in the store succeeds via fallback").
- [x] 4.4 GREEN: confirm the fall-through ordering in the new `spec_change_exists` (DB first, FS second).
- [x] 4.5 RED: unit test `spec_change_exists_falls_back_to_the_archived_filesystem_tree` — no DB row, but `openspec/changes/archive/2026-01-15-old-change/` exists → `true` (Spec: "Link to an archived on-disk change succeeds via fallback").
- [x] 4.6 GREEN: confirm `fs_spec_change_exists`'s archive-glob branch is untouched by the rename.
- [x] 4.7 RED: unit test `spec_change_exists_preserves_permissive_unreadable_root_behavior` — no DB row **and** an unreadable/absent openspec root → still `true`; a name that validated before this change still validates on a local backend, so no previously-working link becomes invalid (Spec: "Existing valid links keep working after the change").
- [x] 4.8 GREEN: confirm `fs_spec_change_exists`'s `Err(_)` arm is untouched by the rename.
- [x] 4.9 RED: HTTP test `link_task_spec_rejects_unknown_change_with_422_when_db_is_authoritative` — **the behavior change**. With a populated `sdd_changes` table and a **deployed-like** root (no readable repository root, so the FS check cannot permissively pass), `POST /v1/tasks/:id/spec-links` with `{"spec_change_name": "does-not-exist"}` returns `422 {code: "unknown_spec"}` and creates no link row — **not the 201 it returned before this change** (Spec: "A typo'd change name is now rejected in production"; proposal §7 criterion 4).
- [x] 4.10 GREEN: repoint `link_task_spec_handler` in `apps/backend/src/api/tasks.rs` to call the new DB-first `spec_change_exists(&conn, &auth.org_id, &name)`; leave the 422 `unknown_spec` mapping as-is.
- [x] 4.11 RED: HTTP test `link_task_spec_validation_is_org_scoped` — an `sdd_changes` row named `"secret-change"` exists in org **B**; an org-**A** caller with no readable root attempting to link to `"secret-change"` gets `422`, and the response reveals nothing about the other org's record (Spec: "Validation is org-scoped").
- [x] 4.12 GREEN: confirm `sdd_change_exists` is called with `auth.org_id` (2.82's `WHERE org_id = ?1`).
- [x] 4.13 RED: HTTP test `link_task_spec_still_succeeds_for_a_change_that_exists_only_in_the_db` — a change in `sdd_changes` with no folder on disk links successfully, proving the DB branch is load-bearing in production where no `openspec/` exists. Plus: an **archived** change is still a valid link target (A8).
- [x] 4.14 GREEN: confirm 4.10's wiring covers both.
- [x] 4.15 RED: HTTP test `get_sdd_change_tasks_requires_both_sdd_read_and_task_read` in `apps/backend/src/api/sdd.rs` — `GET /v1/sdd/changes/:id/tasks` returns `403` for a caller holding `sdd:read` but not `task:read`, **and** `403` for a caller holding `task:read` but not `sdd:read` (Spec: "Linked-tasks read denied without task:read").
- [x] 4.16 GREEN: implement `pub async fn list_change_tasks_handler` in `apps/backend/src/api/sdd.rs` — two `require_permission` calls (`sdd:read`, then `task:read`), resolves the change first (404 if invisible), then calls `queries::list_tasks_for_sdd_change(&conn, &auth.org_id, &change.name, viewer)`. Mount `GET /v1/sdd/changes/:id/tasks` in `apps/backend/src/api/router.rs`.
- [x] 4.17 RED: HTTP test `get_sdd_change_tasks_returns_linked_tasks_scoped_to_visibility` — three linked tasks come back with status and title for a caller who can see all three projects; a task in a project the caller is **not** a member of is excluded and its existence is not revealed; a change with no links returns `200 []`, not an error (Spec: "Linked tasks are returned for a change", "Tasks the caller cannot see are excluded", "A change with no linked tasks returns an empty list").
- [x] 4.18 GREEN: pass `viewer = if auth.role.is_privileged() { None } else { Some(&auth.user_id) }` into the store call, matching the `search.rs:70` precedent.
- [x] 4.19 RED: HTTP test `global_search_returns_sdd_changes_facet` in `apps/backend/src/api/search.rs` — a query matching an artifact's content or a change's title returns the change in a new top-level `sdd_changes` array of `SddChangeSummary` (name, title, project, phase); an org-B change never appears (Spec: "A matching change appears in the SDD facet", "The SDD facet is org-scoped").
- [x] 4.20 GREEN: add `pub sdd_changes: Vec<SddChangeSummary>` to `GlobalSearchResult` in `apps/backend/src/models/types.rs:1025` and populate it in `get_global_search` (`apps/backend/src/api/search.rs`) — call `queries::search_sdd_artifacts` and dedupe to distinct changes. **Update both `GlobalSearchResult` constructions** in `search.rs` (the empty-`q` short-circuit at :54 and the main return at :85) or the file will not compile.
- [x] 4.21 RED: HTTP test `global_search_sdd_facet_is_empty_without_sdd_read_never_403` — **A4**. A caller with `memory:search` but **not** `sdd:read` gets `200` with its other facets populated and an **empty** `sdd_changes` array — never a `403` on the whole search (Spec: "The SDD facet is empty without sdd:read").
- [x] 4.22 GREEN: guard the `search_sdd_artifacts` call in `get_global_search` behind a permission check that yields `vec![]` rather than propagating a 403, mirroring the `users`-facet gating at `search.rs:70`.
- [x] 4.23 RED: HTTP test `global_search_response_shape_is_backward_compatible` — the existing five keys (`memories`, `users`, `projects`, `policies`, `conventions`) are all still present with unchanged shapes and the same results as before the facet was added (Spec: "Existing global-search facets are unaffected"; the proposal §6 response-shape risk).
- [x] 4.24 GREEN: `cargo fmt` + resolve `clippy -D warnings`.

### Gate

- [x] 4.25 GATE: `cargo test --manifest-path apps/backend/Cargo.toml` passes — in particular the **pre-existing** `api/tasks.rs` spec-link tests (`spec_change_exists_*`, `link_spec_rejects_unknown_change_name`, `spec_change_exists_treats_unreadable_root_as_advisory_pass`) still pass after the rename and the DB-first flip.
- [x] 4.26 GATE: `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` is clean.
- [x] 4.27 GATE: `cargo fmt --manifest-path apps/backend/Cargo.toml -- --check` is clean.

---

## PR-5 — importer (`bin/import_sdd.rs`)

**Goal**: a one-shot, idempotent binary that backfills `openspec/changes/**` (7 active + 4 archived) and the legacy `sdd/*` memories into the new tables. Shaped after `apps/backend/src/bin/backfill_embeddings.rs`.

**Satisfies**: `sdd-harness-persistence` — the migration clause of the REMOVED requirement ("Legacy `sdd/*` memories already in the memory table are imported into the artifact store and tagged, not deleted"). Populates the store so `sdd-artifact-links`' DB-first `spec_change_exists` has a real referent on day one.

**Est. changed lines**: ~300
**Depends on**: PR-3

### Checklist

- [ ] 5.1 GREEN (scaffold): create `apps/backend/src/bin/import_sdd.rs` with the `main()` shape of `apps/backend/src/bin/backfill_embeddings.rs` — arg parsing (`--db`, `--org-id`, `--project`, `--root`, `--dry-run`), `connect()` + `migrations::run_all()`, tracing init. Register the bin in `apps/backend/Cargo.toml`: `[[bin]] name = "import-sdd" path = "src/bin/import_sdd.rs"` (after the existing `backfill-embeddings` entry at :15).
- [ ] 5.2 GREEN: extract the importable logic into `pub fn`s inside `import_sdd.rs` (not buried in `main`) so it is unit-testable: `pub fn kind_for_path(rel: &Path) -> Option<(SddArtifactKind, String /* capability */)>`, `pub fn scan_change_dir(dir: &Path) -> Result<Vec<DiscoveredArtifact>>`, `pub fn infer_phase(kinds: &[SddArtifactKind]) -> SddPhase`, `pub fn import_filesystem(conn, org_id, user_id, project, root, dry_run) -> Result<ImportStats>`, `pub fn import_legacy_memories(conn, org_id, user_id, project, dry_run) -> Result<ImportStats>`. Add a `#[cfg(test)] mod tests` block at the bottom.
- [ ] 5.3 RED: unit test `kind_for_path_maps_every_artifact_filename` in `apps/backend/src/bin/import_sdd.rs` — `proposal.md`→`proposal`, `design.md`→`design`, `tasks.md`→`tasks`, `exploration.md`→`exploration`, `apply-progress.md`→`apply-progress`, `verify-report.md`→`verify-report`, `archive-report.md`→`archive-report`, `state.yaml`→`state`, `specs/sdd-artifact-store/spec.md`→`(spec, "sdd-artifact-store")`, and an unrecognized file (`README.md`) → `None`.
- [ ] 5.4 GREEN: implement `kind_for_path` per the design §1 artifact-kind table.
- [ ] 5.5 RED: unit test `import_filesystem_creates_change_and_artifacts_from_a_temp_tree` — build a temp `openspec/changes/demo/` with `proposal.md` + `design.md` + `specs/cap-a/spec.md`; after import, `get_sdd_change_by_name(org, project, "demo")` returns a change with 3 artifacts, each at revision 1, `source='import'`, `git_path` set to the repo-relative path.
- [ ] 5.6 GREEN: implement `import_filesystem` — walk `openspec/changes/*/`, call `queries::upsert_sdd_artifact` with `source = "import"` per file.
- [ ] 5.7 RED: unit test `import_filesystem_strips_date_prefix_and_marks_archive_changes` — a temp `openspec/changes/archive/2026-05-01-old-change/proposal.md` imports as `name = "old-change"` with `status='archived'`, `phase='archive'`.
- [ ] 5.8 GREEN: implement the archive-tree branch of `import_filesystem` — strip the `YYYY-MM-DD-` prefix, force `status`/`phase` via `patch_sdd_change`.
- [ ] 5.9 RED: unit test `infer_phase_picks_the_furthest_kind_present` — `[proposal, design]`→`design`; `[proposal, spec, design, tasks]`→`tasks`; `[proposal, design, tasks, verify-report]`→`verify`; `[proposal]`→`propose`. Uses `SddPhase::rank` from PR-1.
- [ ] 5.10 GREEN: implement `infer_phase` and call it for **active** changes only (archive changes are forced to `archive` by 5.8).
- [ ] 5.11 RED: unit test `import_filesystem_sets_git_commit_when_available` — with a `git rev-parse HEAD` that succeeds, `git_commit` is the 40-char sha; when `git` is absent or the dir is not a repo, `git_commit` is `None` and the import still succeeds (no panic, no `unwrap`).
- [ ] 5.12 GREEN: implement a `fn git_head(root: &Path) -> Option<String>` helper using `std::process::Command`, tolerant of failure.
- [ ] 5.13 RED: unit test `import_legacy_memories_converts_sdd_topic_keys_to_artifacts` — seed a memory with `topic_key = "sdd/demo/design"` and `type='architecture'`; after import, an artifact `(demo, design)` exists carrying that memory's content, `source='import'`. A `topic_key` whose artifact token maps to no known kind is skipped and logged, not panicked on.
- [ ] 5.14 GREEN: implement `import_legacy_memories` — `SELECT ... FROM memories WHERE topic_key LIKE 'sdd/%' ORDER BY created_at ASC`, parse `sdd/{change}/{artifact-type}`, map the token to a `SddArtifactKind`, call `upsert_sdd_artifact` with `source = "import"`.
- [ ] 5.15 RED: unit test `import_orders_memory_before_filesystem_so_the_file_wins_as_the_latest_revision` — the same `(change, kind)` present in **both** sources imports as revision 1 = the memory's content and revision 2 = the file's content, so `get_sdd_artifact` returns the file. Ordering is by the memories' `created_at`.
- [ ] 5.16 GREEN: sequence `main()` as `import_legacy_memories` **first**, then `import_filesystem` — so the filesystem, which is newer and reviewable, lands as the latest revision.
- [ ] 5.17 RED: unit test `import_is_idempotent_on_second_run` — running the full import twice produces **zero** new revisions on the second pass (`created_revision == false` everywhere; total revision count unchanged). This is the proposal §6 double-insert risk.
- [ ] 5.18 GREEN: confirm idempotency comes for free from `upsert_sdd_artifact`'s hash de-dup (PR-2) — the importer must not have its own insert path.
- [ ] 5.19 RED: unit test `import_tags_legacy_memories_sdd_migrated_and_never_deletes_them` — after `import_legacy_memories`, every source memory still exists and now carries the `sdd-migrated` tag; `COUNT(*) FROM memories` is unchanged (Spec: sdd-harness-persistence / REMOVED-requirement migration clause).
- [ ] 5.20 GREEN: implement the tagging step in `import_legacy_memories` (append `sdd-migrated` to `memories.tags`, idempotently). **No `DELETE` statement may appear in `import_sdd.rs`** — deletion is a separate, explicit user decision.
- [ ] 5.21 RED: unit test `import_dry_run_writes_nothing` — `--dry-run` reports the same stats but leaves the DB at zero `sdd_changes` rows.
- [ ] 5.22 GREEN: implement the `--dry-run` short-circuit in `import_filesystem` / `import_legacy_memories`.
- [ ] 5.23 GREEN: `cargo fmt` + resolve `clippy -D warnings` on `apps/backend/src/bin/import_sdd.rs`. Note: CI runs clippy **without** `--all-targets` (`openspec/config.yaml`), so run it once with `--all-targets` locally to be sure the bin's tests are lint-clean too.
- [ ] 5.24 GREEN: document the invocation in `docs/RUNNING.md` (or the nearest existing ops doc) — one line: `cargo run --bin import-sdd -- --db <path> --org-id <id> --project nexus-mind --root <repo-root>`.

### Gate

- [ ] 5.25 GATE: `cargo test --manifest-path apps/backend/Cargo.toml` passes.
- [ ] 5.26 GATE: `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` is clean.
- [ ] 5.27 GATE: manual smoke — `cargo run --manifest-path apps/backend/Cargo.toml --bin import-sdd -- --dry-run --root .` against the real repo reports the 7 active + 4 archived changes and does not error.

---

## PR-6 — MCP tools (`nexusmind-mcp`)

**Goal**: exactly 7 SDD tools + their client fns, in a new `// ── SDD Artifacts ──` section. `get_sdd_artifact` returns the **full document**, not a preview — that is what makes the harness work.

**Satisfies**: `sdd-artifact-agent-tools` — all 8 requirements.

**Est. changed lines**: ~450
**Depends on**: PR-3

### Checklist

- [x] 6.1 RED: create `nexusmind-mcp/src/sdd-client.test.ts` — mock `globalThis.fetch`, set env **before** `await import('./client.js')` (the `tasks-client.test.ts` pattern); assert `saveSddArtifact`, `getSddArtifact`, `getSddArtifactByKey`, `getSddArtifactRevision`, `listSddChanges`, `getSddChange`, `updateSddChange`, `searchSddArtifacts`, `linkSddChangeMemory` each hit the correct verb + path + query params.
- [x] 6.2 GREEN: add the types to `nexusmind-mcp/src/client.ts` in a `// ── SDD Artifacts ──` section after the task block (~line 1416): `SddChange`, `SddArtifact`, `SddArtifactDetail`, `SddRevisionMeta`, `SddSearchHit`, `SaveSddArtifactInput`, `ListSddChangesInput`, `UpdateSddChangeInput`, `LinkSddChangeMemoryInput`.
- [x] 6.3 GREEN: add the client fns to `nexusmind-mcp/src/client.ts`, each a thin `request<T>()` wrapper: `saveSddArtifact(input)` → `PUT /v1/sdd/artifacts`; `getSddArtifact(id)` → `GET /v1/sdd/artifacts/:id`; **`getSddArtifactByKey({project, change_name, kind, capability})` → `GET /v1/sdd/artifacts?project=&change_name=&kind=&capability=`** (the natural-key route from design §4, implemented in PR-3 task 3.42 — a real route, not client-side resolution); `getSddArtifactRevision(id, rev)` → `GET /v1/sdd/artifacts/:id/revisions/:rev`; `listSddChanges(input)`; `getSddChange(id)`; `updateSddChange(id, input)` → `PATCH`; `searchSddArtifacts(q, limit)`; `linkSddChangeMemory(changeId, input)`.
- [x] 6.4 GREEN: **add `src/sdd-client.test.ts` to the `test` script's `tsx --test` file list in `nexusmind-mcp/package.json`** — it does not run otherwise.
- [x] 6.5 RED: create `nexusmind-mcp/src/sdd-tools.test.ts` — spawn `dist/index.js` over stdio (`StdioClientTransport`) against a fake in-process HTTP backend, following `tasks-tools.test.ts` exactly. First test: `save_sdd_artifact_tool_enforces_sdd_write` — a key lacking `sdd:write` fails the tool call and creates **no** change, artifact, or revision on the fake backend (Spec: "save_sdd_artifact enforces sdd:write").
- [x] 6.6 GREEN: register the `save_sdd_artifact` tool in `nexusmind-mcp/src/index.ts` in a new `// ── SDD Artifacts ──` section placed after `// ── Tasks ──` (which ends ~line 3298) — zod shape `{ project, change_name, kind, capability?, content, path?, git_commit?, git_ref? }`, calls `client.saveSddArtifact`. Description in house style: what it does + when to call it + the `sdd:write` gate + **"idempotent by content hash: re-saving identical content creates no revision, so call it freely on every phase"**.
- [x] 6.7 RED: tool test `save_sdd_artifact_reports_whether_a_revision_was_created` — at revision 2, an identical re-save responds "content unchanged, still at revision 2" and no revision is created on the fake backend; an edited save responds "revision 3 created" (Spec: "Re-saving identical content creates no revision", "Saving edited content appends a revision").
- [x] 6.8 GREEN: implement a `formatSddArtifact(artifact, createdRevision)` helper in `nexusmind-mcp/src/index.ts` that branches on `created_revision` and always names the current revision number.
- [x] 6.9 RED: tool test `save_sdd_artifact_for_an_unknown_change_creates_the_change` — saving a `proposal` for `(nexus-mind, new-thing)` when no such change exists creates both, and the tool response identifies the created change (Spec: "Saving for an unknown change creates the change").
- [x] 6.10 GREEN: confirm the tool surfaces the change identity from the backend response (no separate create call — the save **is** the create).
- [x] 6.11 RED: tool test `save_sdd_artifact_oversized_content_fails_the_call_and_writes_nothing` — >1 MB content → the tool call **fails** with a size error (it does not report success), and the fake backend holds no change, artifact, or revision (Spec: "Oversized content fails the tool call and writes nothing"; A2).
- [x] 6.12 GREEN: confirm the `save_sdd_artifact` handler surfaces `ApiError.code` (`artifact_too_large`) as a tool failure and never reports success on a 4xx (Spec: "…MUST surface a backend rejection as a tool failure rather than reporting success").
- [x] 6.13 RED: tool test `save_sdd_artifact_persists_spec_per_capability` — two `kind: "spec"` saves with distinct `capability` values create two artifacts; neither overwrites the other (Spec: "A spec artifact is saved per capability").
- [x] 6.14 GREEN: confirm `capability` is forwarded verbatim by the tool.
- [x] 6.15 RED: tool test `get_sdd_artifact_returns_full_content_not_a_preview` — a fake-backend artifact whose latest revision is 36 KB comes back **byte-identical and whole** in the tool's response: not truncated, not ellipsized, not summarized. This is the single behavior that makes the harness work (Spec: "A large design document is returned in full").
- [x] 6.16 GREEN: register the `get_sdd_artifact` tool in `nexusmind-mcp/src/index.ts` — zod shape `{ artifact_id?, project?, change_name?, kind?, capability?, revision? }` (by id **or** by natural key), calls `client.getSddArtifact` / `client.getSddArtifactByKey`. Description must say **"returns the FULL document — this is the cross-phase read (sdd-design reads the proposal; sdd-tasks reads spec + design)"**. The handler MUST NOT truncate or elide.
- [x] 6.17 RED: tool test `get_sdd_artifact_by_change_and_kind_resolves_the_spec_capability` — `{change_name, kind: 'spec', capability: 'sdd-artifact-store'}` returns that capability's spec, not another's; `{change_name, kind: 'design'}` with no `capability` resolves the design; no `artifact_id` is required (Spec: "Addressable by change name and kind").
- [x] 6.18 GREEN: wire `capability` through `getSddArtifactByKey`.
- [x] 6.19 RED: tool test `get_sdd_artifact_accepts_an_explicit_revision_number` — with revisions 1..3, `{artifact_id, revision: 2}` returns revision 2's full content and **not** revision 3's; omitting `revision` defaults to the latest (Spec: "An explicit revision returns that revision's full content").
- [x] 6.20 GREEN: route the `revision` param through `client.getSddArtifactRevision(id, rev)` in the `get_sdd_artifact` handler; default to the latest when it is absent.
- [x] 6.21 RED: tool test `get_sdd_artifact_missing_reports_not_found_not_an_empty_document` — a change with no `design` artifact → the tool reports **not-found**; it MUST NOT return an empty string a caller could mistake for an empty design (Spec: "A missing artifact reports not-found, not an empty document").
- [x] 6.22 GREEN: branch on the backend's `404` in the `get_sdd_artifact` handler and return a structured not-found text, never `""`.
- [x] 6.23 RED: tool test `get_sdd_artifact_cross_org_reports_not_found` — an artifact in an org the calling key does not belong to reports not-found and returns no content, exactly as the REST endpoint would (Spec: "The tools grant no authority the API does not").
- [x] 6.24 GREEN: confirm the tool adds no authority — it forwards the key and surfaces the backend's 404.
- [x] 6.25 RED: tool test `search_sdd_artifacts_enforces_sdd_read` — a key lacking `sdd:read` fails the call and no artifact content or metadata reaches the agent. Plus `read_only_caller_can_read_but_not_write` — a key with `sdd:read` and without `sdd:write` succeeds on `get_sdd_artifact` and fails on `save_sdd_artifact` with a permission error (Spec: "search_sdd_artifacts enforces sdd:read", "A read-only caller can read but not write").
- [x] 6.26 GREEN: confirm no tool caches, elevates, or re-signs credentials.
- [x] 6.27 RED: tool test `list_sdd_changes_filters_and_omits_content` — `{project}` returns only that project's changes each with phase and status; `{phase: "design"}` returns only design-phase changes; the response for a change with a 36 KB design artifact **contains no markdown content** (Spec: "Listing changes for a project", "Filtering by phase", "The listing contains no artifact content").
- [x] 6.28 GREEN: register `list_sdd_changes` in `nexusmind-mcp/src/index.ts` — zod shape `{ project?, status?, phase?, sprint_id?, include_archived? }`, calls `client.listSddChanges`, formats via a `formatSddChangeList` helper that emits metadata only. Description: "powers `/sdd-status`".
- [x] 6.29 RED: tool test `get_sdd_change_returns_the_artifact_inventory_as_recoverable_state` — the response enumerates each artifact's kind, capability, path, and latest revision, plus linked tasks and linked memories, and **does not inline content**; with a stale `phase: "spec"` and a `design` artifact present, the inventory still lists the design so a resuming agent can see the design step already produced an artifact (Spec: "A fresh session recovers a change with no checkout", "The inventory contradicts a stale phase and the inventory wins", "The inventory omits content").
- [x] 6.30 GREEN: register `get_sdd_change` — zod shape `{ change_id?, project?, change_name? }`, calls `client.getSddChange`. Description: "the artifact inventory *is* the recoverable DAG state — call this to resume a change without a checkout".
- [x] 6.31 RED: tool test `update_sdd_change_transitions_phase_and_denies_without_sdd_write` — `{change_id, phase: 'apply'}` with `sdd:write` transitions the change and the tool confirms the transition; a `sdd:read`-only key fails the call and the change is unmodified (Spec: "Advance a change to the apply phase", "Transition denied without sdd:write").
- [x] 6.32 GREEN: register `update_sdd_change` — zod shape `{ change_id, phase?, status?, title?, sprint_id? }`, calls `client.updateSddChange`.
- [x] 6.33 RED: tool test `update_sdd_change_invalid_phase_is_rejected_atomically_and_unknown_change_reports_not_found` — `{phase: "shipped", title: "New"}` fails with a validation error and **neither** the phase **nor** the title is changed on the fake backend; an unknown change id reports not-found and **no change is created as a side effect** (Spec: "An invalid phase value is rejected atomically", "Unknown change reports not-found").
- [x] 6.34 GREEN: confirm the tool never falls back to a create-on-miss path for an unknown change, and surfaces the backend's 422/404 as a tool failure.
- [x] 6.35 RED: tool test `search_sdd_artifacts_returns_identifiers_sufficient_to_fetch_and_honours_the_limit` — each hit carries a snippet plus change name, kind, and capability, which the agent can pass straight to `get_sdd_artifact`; matches under three different changes all appear; `{limit: 5}` against 20 matches returns at most 5 (Spec: "Find the spec that covers a topic", "Search spans changes, not just the current one", "Results honour the limit").
- [x] 6.36 GREEN: register `search_sdd_artifacts` — zod shape `{ query, limit? }`, calls `client.searchSddArtifacts`, formats hits with their full natural key.
- [x] 6.37 RED: tool test `link_sdd_change_memory_ties_a_decision_to_a_change_and_is_idempotent` — `{change_id, memory_id, relation: 'produced'}` creates the link and the memory appears among the change's linked memories; re-calling the same pair succeeds and creates no duplicate; an unknown/invisible `memory_id` reports not-found and **creates no link** (Spec: "sdd-apply links a decision it recorded", "Re-linking the same memory is a no-op", "Linking an invisible memory fails without writing").
- [x] 6.38 GREEN: register `link_sdd_change_memory` — zod shape `{ change_id, memory_id, relation: z.enum(['produced','informed']).default('produced') }`, calls `client.linkSddChangeMemory`. Description: "called by `sdd-apply` / `sdd-verify` to tie the decisions they record back to the change that produced them".
- [x] 6.39 RED: tool test `exactly_seven_sdd_tools_are_registered` — the MCP server's tool list contains precisely `save_sdd_artifact`, `get_sdd_artifact`, `list_sdd_changes`, `get_sdd_change`, `update_sdd_change`, `search_sdd_artifacts`, `link_sdd_change_memory` and no other SDD tool (Spec: "The system MUST expose exactly seven MCP tools").
- [x] 6.40 GREEN: confirm no eighth SDD tool leaked in (no `create_sdd_change`, no `delete_sdd_change` — archival is admin/API-only).
- [x] 6.41 GREEN: **add `src/sdd-tools.test.ts` to the `test` script's `tsx --test` file list in `nexusmind-mcp/package.json`** — it does not run otherwise. Verify by running `npm test` and confirming the new file's test names appear in the output.
- [x] 6.42 GREEN: bump `nexusmind-mcp/package.json` `version` `0.8.3` → `0.9.0` (7 new tools, additive — minor).
- [x] 6.43 GREEN: add a `nexusmind-mcp/CHANGELOG.md` entry for `0.9.0` documenting the 7 SDD tools and that `save_sdd_artifact` is idempotent by content hash.
- [x] 6.44 GREEN: add the `// ── SDD Artifacts ──` tool set to the tool table in `nexusmind-mcp/README.md` (the tool count moves 126 → 133 — grep for a hard-coded count in README/CLAUDE.md and update it).

### Gate

- [x] 6.45 GATE: `cd nexusmind-mcp && npm test` passes (`pretest: tsc` clean), and the output visibly includes tests from **both** `src/sdd-client.test.ts` and `src/sdd-tools.test.ts`.
- [x] 6.46 GATE: `cd nexusmind-mcp && npx tsc --noEmit` is clean.

---

## PR-7 — admin: `remark-gfm` + the `<Markdown>` primitive

**Goal**: add `remark-gfm` (absent today) and extract the four copy-pasted `components={{...}}` override maps into one `<Markdown>` primitive, repointing all four existing call sites. **Prerequisite, not a nice-to-have**: `tasks.md` is entirely GFM task lists and tables — without `remark-gfm` the flagship artifact renders as a wall of literal `- [ ]`.

**Satisfies**: `sdd-artifact-admin` — "A Single Shared Markdown Primitive Renders GFM Across the Admin" (all 4 scenarios).

**Est. changed lines**: ~280
**Depends on**: — (no backend dependency; starts immediately, in parallel with PR-1)

### Checklist

- [x] 7.1 GREEN: add `"remark-gfm": "^4.0.0"` to `dependencies` in `apps/admin/package.json` and run `npm install` (commit the lockfile change). **Do NOT add `rehype-raw`** — A6 forbids it.
- [x] 7.2 RED: create `apps/admin/src/components/ui/Markdown/Markdown.test.tsx` — render `<Markdown content={'| a | b |\n|---|---|\n| 1 | 2 |'} />` and assert a real `<table>` with `<th>a</th>` / `<td>1</td>` is in the DOM and the pipe characters are **not** shown as text (**fails without `remark-gfm`**) (Spec: "A GFM table renders as a table").
- [x] 7.3 GREEN: create `apps/admin/src/components/ui/Markdown/Markdown.tsx` — a `<Markdown content, className? />` component wrapping `ReactMarkdown` with `remarkPlugins={[remarkGfm]}` and the shared `components={{...}}` override map lifted verbatim from `apps/admin/src/pages/Memories.tsx:63-136` (`MemoryMarkdown`) — h1/h2/h3, p, ul/ol/li, code/pre, a, blockquote, hr, using the existing Tailwind tokens (`text-text-primary`, `border-border-primary`, no new hex values). Add `apps/admin/src/components/ui/Markdown/index.ts` re-exporting it, matching the `ui/Badge/index.ts` convention.
- [x] 7.4 RED: test `markdown_renders_gfm_task_list_checkboxes` in `apps/admin/src/components/ui/Markdown/Markdown.test.tsx` — `- [ ] Write the migration` and `- [x] Write the spec` render as `<input type="checkbox">` elements (one unchecked, one checked); the literal characters `- [ ]` are **not** shown as text. **This is the `tasks.md` requirement** (Spec: "A GFM task list renders as checkboxes").
- [x] 7.5 GREEN: add `li`/`input` overrides to the `components` map so the GFM task-list checkbox renders styled and `disabled` (read-only — the admin never writes artifact content, per A7).
- [x] 7.6 RED: test `markdown_does_not_execute_embedded_html_or_script` — **A6**. Render content containing `<script>window.__pwned = true</script>` and `<img src=x onerror="window.__pwned = true">`: after render, `window.__pwned` is `undefined`, no `<script>` element exists in the container, and no inline event-handler attribute is present in the output DOM; the content renders as inert markdown output (Spec: "Embedded HTML in artifact content is not executed").
- [x] 7.7 GREEN: confirm `react-markdown`'s default (no `rehype-raw`) already inertizes raw HTML — and add a load-bearing comment in `Markdown.tsx` stating that `rehype-raw` MUST NOT be added, because agent-authored artifact content is rendered here. Assert by source scan that `rehype-raw` appears nowhere in `apps/admin/package.json`.
- [x] 7.8 RED: test `markdown_renders_strikethrough_and_autolinks` — the remaining GFM features (`~~x~~`, bare URLs) render as `<del>` and `<a>`.
- [x] 7.9 GREEN: confirm `remarkGfm` covers them (no extra config expected).
- [x] 7.10 RED: test `markdown_wide_table_scrolls_horizontally_without_breaking_layout` — the `table` override is wrapped in an `overflow-x-auto` container (assert the wrapper class), so a wide `tasks.md` table does not blow out the drawer width.
- [x] 7.11 GREEN: add the `table` override with its `overflow-x-auto` wrapper to `Markdown.tsx`.
- [x] 7.12 GREEN: repoint call site 1 — `apps/admin/src/pages/Memories.tsx`: delete the local `MemoryMarkdown` (lines 63-136) and the `import ReactMarkdown from 'react-markdown'` (line 4); import and use `<Markdown>` from `../components/ui/Markdown`.
- [x] 7.13 GREEN: repoint call site 2 — `apps/admin/src/pages/Conventions.tsx`: replace the inline `<ReactMarkdown components={{...}}>` block (lines 678-707) with `<Markdown>`; drop the now-unused `react-markdown` import (line 4). **Preserve the Raw/Preview toggle at lines 655-676 untouched** — it is the precedent PR-9's artifact viewer copies.
- [x] 7.14 GREEN: repoint call site 3 — `apps/admin/src/components/OrgMemoryGraph.tsx`: replace the inline block (lines 442-482) with `<Markdown>`; drop the `react-markdown` import (line 4).
- [x] 7.15 GREEN: repoint call site 4 — `apps/admin/src/pages/memories/MemoryGraphTab.tsx`: replace the inline block (lines 468-508) with `<Markdown>`; drop the `react-markdown` import (line 4).
- [x] 7.16 RED: confirm-by-running — the existing suites that cover the four call sites (`apps/admin/src/pages/Memories.test.tsx`, `apps/admin/src/pages/Graph.test.tsx`, and any `Conventions` / `MemoryGraphTab` coverage) still pass **unchanged** after the repoint. If any assertion depended on a class or DOM shape the shared map alters, fix `Markdown.tsx` to preserve the original markup rather than editing the test.
- [x] 7.17 GREEN: `grep -rn "react-markdown\|ReactMarkdown" apps/admin/src` — the only remaining hit must be `apps/admin/src/components/ui/Markdown/Markdown.tsx`. Zero other files import `react-markdown` directly and no view retains its own duplicated override map (Spec: "The existing call sites use the same primitive").
- [x] 7.18 GREEN: add `Markdown` to the barrel export in `apps/admin/src/components/ui/` if such an index exists (mirror how `Badge`/`Table` are exported).

### Gate

- [x] 7.19 GATE: `cd apps/admin && npm run test` passes — **including the four pre-existing suites that cover the repointed call sites**, unmodified.
- [x] 7.20 GATE: `cd apps/admin && npx tsc -b` is clean.
- [x] 7.21 GATE: `cd apps/admin && npm run build` succeeds (`remark-gfm` resolves in the Vite bundle).

---

## PR-8 — admin: types, client, nav, route, `/sdd` list page

**Goal**: the SDD section's list view — mirroring `Tasks.tsx`: module-scope `createClient()`, filter bar, skeleton → `EmptyState` → table, permission gate.

**Satisfies**: `sdd-artifact-admin` — "SDD Navigation and Route Are Gated by sdd:read", "The SDD Page Lists Changes With a Phase Pipeline Driven by Real Artifacts".

**Est. changed lines**: ~340
**Depends on**: PR-3, PR-7

### Checklist

- [x] 8.1 GREEN (types, no dedicated RED — covered transitively by 8.3+): add to `apps/admin/src/types.ts` — `SddChange`, `SddChangeSummary`, `SddArtifact`, `SddArtifactDetail`, `SddRevisionMeta`, `SddSearchHit`, and the `SddPhase` / `SddStatus` / `SddArtifactKind` string unions, mirroring the backend `models/types.rs` shapes exactly.
- [x] 8.2 GREEN: add `sdd_changes: SddChangeSummary[]` to the `GlobalSearchResult` interface in `apps/admin/src/types.ts:280`, **and add `sdd_changes: []` to `EMPTY_RESULT` in `apps/admin/src/components/CommandPalette.tsx:13`** — `tsc -b` fails without the second edit.
- [x] 8.3 RED: create `apps/admin/src/pages/Sdd.test.tsx` (written FIRST) — `vi.mock('../api/client', ...)` returning a fixture `SddChange[]`; `renderWithProviders(<Sdd />)`; asserts each change's `name`, `title`, `project`, and `status` render in the table, across all projects (Spec: "The SDD Page Lists Changes …").
- [x] 8.4 GREEN: add the SDD methods to `NexusMindClient` in `apps/admin/src/api/client.ts` — **reads**: `listSddChanges(params)`, `getSddChange(id)`, `getSddChangeArtifacts(id)`, `getSddChangeTasks(id)`, `getSddArtifact(id)`, `listSddArtifactRevisions(id)`, `getSddArtifactRevision(id, rev)`, `searchSddArtifacts(q, limit)`; **curation writes (A7 — permitted; these touch change metadata and links, never artifact content)**: `patchSddChange(id, input)`, `linkSddChangeMemory(id, input)`, `unlinkSddChangeMemory(id, memoryId)`. **No artifact-save method may exist on the admin client** — the admin never authors artifact content. Implement `apps/admin/src/pages/Sdd.tsx` (module-scope `createClient()`, the `Tasks.tsx` shape) using `ui/Table`, `ui/Badge`, `ui/Select`, `ui/EmptyState`, `ui/Skeleton`, reusing existing Tailwind tokens (`bg-accent-blue`, `text-text-primary`, `rounded-[18px]`) — **no new hex values**.
- [x] 8.5 RED: test `sdd_list_shows_skeleton_while_loading_then_the_table` — a skeleton renders while the query is in flight and is replaced by the table (or the empty state) once it settles (Spec: "Loading state precedes data").
- [x] 8.6 GREEN: wire `useQuery(['sdd-changes', filters], ...)` (the house query-key convention from design §7) with the `Skeleton` → table transition in `apps/admin/src/pages/Sdd.tsx`.
- [x] 8.7 RED: test `sdd_list_renders_empty_state_when_no_changes_match_filters` — zero results renders `EmptyState`, not an empty table (Spec: "Empty state when no change matches").
- [x] 8.8 GREEN: render `ui/EmptyState` when the fetched list is empty.
- [x] 8.9 RED: test `sdd_list_filter_bar_by_project_phase_and_status_refetches` — changing each `Select` mutates the `['sdd-changes', filters]` query key and narrows the visible rows (Spec: "Filtering by phase updates the list").
- [x] 8.10 GREEN: implement the three-`Select` filter bar in `apps/admin/src/pages/Sdd.tsx`, forwarding `project` / `phase` / `status` to `client.listSddChanges`.
- [x] 8.11 RED: test `sdd_list_renders_a_phase_pipeline_driven_by_which_artifacts_exist` — a change whose `phase` field is `spec` but which has **both** a `design` and a `tasks` artifact shows the design and tasks pipeline steps as present; the display is not limited to the `spec` step. **The artifact inventory is the ground truth, not `change.phase`** (Spec: "The pipeline reflects the artifact inventory, not a stale phase").
- [x] 8.12 GREEN: implement a `<PhasePipeline artifacts={change.artifacts} />` badge row (`propose → spec → design → tasks → apply → verify`) in `apps/admin/src/pages/Sdd.tsx`, deriving completeness from the artifact kinds present, using `ui/Badge`.
- [x] 8.13 RED: test `sdd_page_redirects_to_401_without_sdd_read` — the mocked auth context lacking `sdd:read` yields `<Navigate to="/401">` and **no SDD data is fetched** (assert the client mock was never called), matching `Tasks.tsx`'s `canRead` gate (Spec: "Direct navigation without permission is denied").
- [x] 8.14 GREEN: add the `canRead` permission gate to `apps/admin/src/pages/Sdd.tsx`, placed **before** the `useQuery` fires.
- [x] 8.15 RED: test `nav_item_sdd_visible_with_sdd_read` in the `Layout` test — the "SDD" item appears in the **Knowledge** group when the mocked session's `permissions` include `sdd:read` (Spec: "Nav item visible with sdd:read").
- [x] 8.16 GREEN: add `{ label: 'SDD', href: '/sdd', icon: FileStack, adminOnly: true, requiredPermission: 'sdd:read' }` to the **Knowledge** group of `NAV_GROUPS` in `apps/admin/src/components/Layout.tsx` (after the `Tasks` entry at line 157); import `FileStack` from `lucide-react`.
- [x] 8.17 RED: test `nav_item_sdd_hidden_without_sdd_read` — the item is absent when the mocked permissions lack `sdd:read` (Spec: "Nav item hidden without sdd:read").
- [x] 8.18 GREEN: confirm the existing filter logic at `Layout.tsx:196-203` handles it; no change beyond the new entry.
- [x] 8.19 GREEN: register the lazy route in `apps/admin/src/App.tsx` — `const SddArtifacts = lazy(() => import('./pages/Sdd'))` + `<Route path="/sdd" element={<SddArtifacts />} />`, wrapped in the same guard component the `/tasks` route uses.
- [x] 8.20 RED: test `sdd_list_deep_links_a_change_by_query_param` — mounting `/sdd?change=sdd-artifacts` selects that change (the target PR-9's task cross-links and search results point at).
- [x] 8.21 GREEN: read the `change` search param in `apps/admin/src/pages/Sdd.tsx` and use it to select the row (drawer opening lands in PR-9).

### Gate

- [x] 8.22 GATE: `cd apps/admin && npm run test` passes.
- [x] 8.23 GATE: `cd apps/admin && npx tsc -b` is clean (in particular the `GlobalSearchResult` + `CommandPalette` `EMPTY_RESULT` pair from 8.2).
- [x] 8.24 GATE: `cd apps/admin && npm run build` succeeds.

---

## PR-9 — admin: `ChangeDetail` drawer, curation, cross-links, global-search group

**Goal**: the artifact reader — a right-side drawer with artifact tabs, `<Markdown>` rendering, a Raw/Preview toggle, a revision selector, linked tasks/memories — plus the curation controls A7 permits, the task↔change cross-link, and the SDD group in global search.

**Satisfies**: `sdd-artifact-admin` — "The Change Detail Drawer Shows Artifact Tabs With Revisions and a Raw Toggle", "The Admin Is Read-Only Over Artifacts", "The Task Detail Cross-Links to the SDD Change", "SDD Results Appear in the Admin Global Search".

**Est. changed lines**: ~460
**Depends on**: PR-8

### Checklist

- [x] 9.1 RED: create `apps/admin/src/pages/sdd/ChangeDetail.test.tsx` (written FIRST) — mock the client; `renderWithProviders(<ChangeDetail changeId="c1" />)` asserts the drawer opens with one tab per artifact kind that **exists** on the change; a change with a `proposal` and a `design` but **no** `tasks` artifact renders Proposal and Design tabs and **no** Tasks tab (Spec: "Only existing artifact kinds get tabs").
- [x] 9.2 GREEN: implement `apps/admin/src/pages/sdd/ChangeDetail.tsx` as a right-side drawer — `<Modal position="right" size="lg">` (already supported by `ui/Modal`), tabs derived from the change's `artifacts[]` inventory (never a static array), `useQuery(['sdd-change', id])`.
- [x] 9.3 RED: test `change_detail_renders_the_selected_artifact_as_rendered_markdown` — the `tasks` tab's content renders through `<Markdown>` with **real checkboxes and a real table**, not literal `- [ ]` (the PR-7 primitive earning its keep, end to end).
- [x] 9.4 GREEN: fetch the artifact via `useQuery(['sdd-artifact', artifactId])` → `client.getSddArtifact` and render `<Markdown content={artifact.content} />`.
- [x] 9.5 RED: test `change_detail_raw_preview_toggle_switches_between_source_and_render` — Raw displays the artifact's markdown source verbatim and unrendered; switching back restores the rendered preview (Spec: "The raw toggle shows unrendered source").
- [x] 9.6 GREEN: implement the Raw/Preview toggle in `apps/admin/src/pages/sdd/ChangeDetail.tsx`, copying the precedent at `apps/admin/src/pages/Conventions.tsx:655-676`.
- [x] 9.7 RED: test `change_detail_specs_tab_lists_one_entry_per_capability` — a change with three `spec` artifacts shows all three capabilities as selectable, and selecting one renders that capability's spec content (Spec: "The Specs tab lists one entry per capability").
- [x] 9.8 GREEN: implement the capability sub-list inside the Specs tab.
- [x] 9.9 RED: test `change_detail_revision_selector_refetches_and_renders_the_selected_revision` — with the artifact at revision 3, selecting revision 1 fires `getSddArtifactRevision(id, 1)` and renders revision 1's content **in place of** revision 3's; the query key is `['sdd-artifact-revision', id, rev]` (Spec: "Selecting an older revision refetches and renders it").
- [x] 9.10 GREEN: implement the revision selector (`rev 3 ▾`) in `apps/admin/src/pages/sdd/ChangeDetail.tsx` — populated from `useQuery(['sdd-artifact-revisions', artifactId])` → `client.listSddArtifactRevisions` (metadata only), refetching content on selection.
- [x] 9.11 RED: test `change_detail_revision_selector_shows_timestamp_and_source_per_revision` — `rev 2 · agent · 2026-07-11` style rows; no diff UI is rendered (explicitly out of scope, proposal §4).
- [x] 9.12 GREEN: format the revision options from `SddRevisionMeta`.
- [x] 9.13 RED: test `change_detail_renders_linked_tasks_and_memories` — two linked tasks render with their status `Badge` and link to `/tasks?task=<id>`; one linked memory renders with title + type and links to `/memories?id=<id>` (Spec: "Linked tasks and memories are shown"; proposal §7 criterion 3, change→task direction).
- [x] 9.14 GREEN: implement the Linked Tasks section (`useQuery(['sdd-change-tasks', id])` → `client.getSddChangeTasks`) and the Linked Memories section (from `getSddChange`'s `memory_links[]`) in `apps/admin/src/pages/sdd/ChangeDetail.tsx`.
- [x] 9.15 RED: test `change_detail_presents_no_artifact_edit_save_or_delete_control` — **the read-only contract**. Rendering any artifact tab as a user **holding `sdd:write`** shows no edit, save, or delete-artifact control for the artifact **content**, and no editable input bound to `artifact.content` (Spec: "No artifact editing control exists").
- [x] 9.16 RED: test `admin_issues_no_artifact_save_request` — navigating the `/sdd` section and opening artifacts and revisions issues **only SDD read requests** for artifact content; assert the mocked client exposes no artifact-save method and that no `PUT /v1/sdd/artifacts` call is made by any admin code path (Spec: "The admin issues no artifact writes").
- [x] 9.17 GREEN: confirm `apps/admin/src/api/client.ts` exposes **no** artifact-save method (8.4) and that `ChangeDetail.tsx` renders content read-only.
- [x] 9.18 RED: test `change_detail_allows_curation_of_phase_status_and_sprint` — **A7 unscoped**: a user with `sdd:write` can patch the change's `phase` / `status` / `sprint_id` from the drawer; the mutation calls `client.patchSddChange` and invalidates `['sdd-change', id]` and `['sdd-changes']`. A user **without** `sdd:write` does not see the controls. (Curation of change metadata is explicitly permitted — it is not artifact authorship.)
- [x] 9.19 GREEN: implement the curation controls (phase/status `Select`s, sprint picker) in `apps/admin/src/pages/sdd/ChangeDetail.tsx`, gated on `sdd:write`.
- [x] 9.20 RED: test `change_detail_allows_linking_and_unlinking_memories` — **A7 unscoped**: with `sdd:write`, linking a memory calls `client.linkSddChangeMemory` and unlinking calls `client.unlinkSddChangeMemory`; both invalidate `['sdd-change', id]`. Without `sdd:write`, the controls are hidden.
- [x] 9.21 GREEN: implement the memory link/unlink controls in the Linked Memories section, gated on `sdd:write`.
- [x] 9.22 GREEN: wire the drawer into `apps/admin/src/pages/Sdd.tsx` — clicking a table row (or arriving via `?change=<name>` from PR-8's 8.21) opens `<ChangeDetail>`.
- [x] 9.23 RED: test `task_detail_linked_specs_are_links_into_the_sdd_section_with_the_change_phase` in `apps/admin/src/pages/tasks/TaskDetail.test.tsx` — the "Linked Specs" section (currently bare strings) renders each name as a navigable link to `/sdd?change=<name>` and shows the change's phase beside it (Spec: "A linked spec name navigates to its change", "The change's phase is shown next to the link").
- [x] 9.24 RED: test `task_detail_dangling_spec_link_renders_without_breaking_the_view` — a linked name with **no** matching SDD change is still displayed, is **not** rendered as a link to a non-existent change, and the task detail renders without error (Spec: "A dangling spec link renders without breaking the view").
- [x] 9.25 GREEN: update the Linked Specs section of `apps/admin/src/pages/tasks/TaskDetail.tsx` — resolve each `spec_links[]` name against `client.listSddChanges`, rendering a link + phase `Badge` on a match and inert plain text on a miss.
- [x] 9.26 RED: test `global_search_renders_an_sdd_changes_result_group` in `apps/admin/src/pages/Search.test.tsx` (create if absent) — a mocked `GlobalSearchResult` carrying two `sdd_changes` renders an SDD group listing both with their name and phase; selecting one navigates to `/sdd?change=<name>` (Spec: "SDD results are grouped and navigable").
- [x] 9.27 GREEN: add the SDD result group to `apps/admin/src/pages/Search.tsx` (it destructures `GlobalSearchResult` at :134).
- [x] 9.28 RED: test `no_sdd_results_means_no_sdd_group` — a search returning memory results and an **empty** `sdd_changes` facet renders **no** SDD group (not an empty one) and the memory results render normally. This also covers the caller-without-`sdd:read` case, whose facet is empty per A4 (Spec: "No SDD results means no SDD group", "A user without sdd:read sees no SDD group").
- [x] 9.29 GREEN: guard the SDD group's render on `sdd_changes.length > 0` in `Search.tsx`.
- [x] 9.30 RED: test `command_palette_includes_sdd_changes_in_flattened_results` — `flattenResults` (`apps/admin/src/components/CommandPalette.tsx:40`) folds `sdd_changes` into the flat result list with an `sdd` kind and a `/sdd?change=<name>` href.
- [x] 9.31 GREEN: extend `flattenResults` in `apps/admin/src/components/CommandPalette.tsx`.
- [x] 9.32 RED: test `search_ui_tolerates_a_response_without_the_sdd_changes_key` — an older backend response missing `sdd_changes` renders without crashing and no previously-present group is lost (the additive-field risk from the consumer side; Spec: "Existing global-search facets are unaffected").
- [x] 9.33 GREEN: default `sdd_changes` to `[]` at every read site in `Search.tsx` and `CommandPalette.tsx`.

### Gate

- [x] 9.34 GATE: `cd apps/admin && npm run test` passes (including the pre-existing `TaskDetail.test.tsx` and `Search`/`CommandPalette` coverage).
- [x] 9.35 GATE: `cd apps/admin && npx tsc -b` is clean.
- [x] 9.36 GATE: `cd apps/admin && npm run build` succeeds.
- [ ] 9.37 GATE: manual smoke against a locally seeded backend — `/sdd` lists the imported changes and the `tasks` artifact of `sdd-artifacts` renders with working checkboxes and tables (proposal §7 criterion 2).

---

## PR-10 — harness: the `nexusmind` persistence mode + 10 `sdd-*` skills

**Goal**: close the loop (D7) — a store nobody writes to is a dead table. The `sdd-*` skills live **outside this repo**, at `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/`. Every path below is absolute and in that tree.

**Satisfies**: `sdd-harness-persistence` — all 7 requirements (5 ADDED, 1 MODIFIED "Artifact Store Mode Resolution", 1 REMOVED "SDD Artifacts Are Saved as Memories With capture_prompt Disabled").

**Est. changed lines**: ~340
**Depends on**: PR-6 (the MCP tools must exist before the skills can call them)

> No test runner exists for skill markdown, so these are GREEN-only, gated by manual end-to-end smoke runs (10.27–10.30). Keep each skill edit mechanical and identical in shape — the three bullets in design §8.

### Checklist

- [ ] 10.1 GREEN: add the `nexusmind` mode to `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/_shared/persistence-contract.md` — extend the mode list at "Mode Resolution" (line 5) from `engram | openspec | hybrid | none` to **`nexusmind | openspec | hybrid | engram | none`**, and add the "Mode Roles" bullet (line ~13): **`nexusmind`: reads full documents via `get_sdd_artifact`; writes `save_sdd_artifact` + the filesystem; project files yes; history = revisions + git.**
- [ ] 10.2 GREEN: add the `nexusmind` row to the "Mode Comparison" table (line ~20) and the "Behavior Per Mode" table (line ~33) of `persistence-contract.md` — read via `get_sdd_artifact`, write to **both** `save_sdd_artifact` and the filesystem, project files **yes**, history via **revisions plus git**. All five modes must appear (Spec: "The mode comparison covers all five modes").
- [ ] 10.3 GREEN: make `nexusmind` the **recommended default** in `persistence-contract.md`'s "Mode Resolution" default rule (line ~9): if the NexusMind SDD tools are available → `nexusmind`; else if Engram → `engram`; else `none`. It MUST be selected without an explicit user choice when the SDD tools are present, and `engram` MUST NOT be selected in that case (Spec: "nexusmind is the default when the SDD tools are available").
- [ ] 10.4 GREEN: mark `engram` **deprecated** in `persistence-contract.md` — keep the existing "### `engram` mode limitation" section (line ~29), state explicitly that re-running a phase **overwrites** the previous artifact with no revision history, and name `nexusmind` as the replacement. **`engram` MUST NOT be removed** and MUST remain selectable and functional for repos still on the old contract (Spec: "engram remains selectable and functional", "The contract states engram's limitation").
- [ ] 10.5 GREEN: add a "### `nexusmind` mode" section to `persistence-contract.md` stating the **both-writes-or-fail** rule (**A5**): a phase MUST NOT report success unless **both** the filesystem write and `save_sdd_artifact` succeeded; a failure of either MUST be surfaced to the user, never silently swallowed or degraded to single persistence. Inherited from the `hybrid` contract's "both writes MUST succeed" (Spec: "A failed store write is reported, not hidden").
- [ ] 10.6 GREEN: state in `persistence-contract.md` that in `nexusmind` mode `save_sdd_artifact` MUST be supplied the artifact's **repository-relative path**, and the **current git commit** when one is resolvable (Spec: "The mode records the artifact's git provenance").
- [ ] 10.7 GREEN: add the `nexusmind` row to the "State Persistence (Orchestrator)" table in `persistence-contract.md` (line ~54): write = `save_sdd_artifact(kind: 'state')` **and** `openspec/changes/{change}/state.yaml`; read = `get_sdd_change` (the artifact inventory is the recoverable DAG state) with the filesystem as fallback (Spec: "The orchestrator's state survives as an artifact").
- [ ] 10.8 GREEN: **delete the `capture_prompt: false` paragraph** from `persistence-contract.md` (the footnote at line ~65) — the mandatory flag, the "do not infer this from `type`" caveat, and the older-schema fallback all go. It existed only because SDD artifacts and human decisions shared the `memories` table. Add a one-line note that it remains applicable to the deprecated `engram` mode only (Spec: REMOVED / "SDD Artifacts Are Saved as Memories With capture_prompt Disabled").
- [ ] 10.9 GREEN: add the "Common Rules" bullet for `nexusmind` in `persistence-contract.md` (line ~69): write project files per `openspec-convention.md` **and** call `save_sdd_artifact` for every artifact; **never write SDD content to the memory store**.
- [ ] 10.10 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/_shared/openspec-convention.md` — add the filename → artifact `kind` mapping table so the on-disk layout and the `save_sdd_artifact` `kind` enum are documented as one-to-one (including `specs/{capability}/spec.md` → `kind: spec` + `capability`).
- [ ] 10.11 GREEN: add a **missing-dependency rule** to `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/_shared/sdd-phase-common.md` — in `nexusmind` mode, a phase whose declared required input (per `openspec/config.yaml`'s `phases.*.required_inputs`) has no artifact MUST **report the missing dependency and stop**; it MUST NOT proceed with empty input or fabricate the input (Spec: "A missing dependency artifact stops the phase").
- [ ] 10.12 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-init/SKILL.md` — offer `nexusmind` as the `artifact_store` mode when writing `openspec/config.yaml`, and make it the recommended default when the NexusMind SDD tools are connected.
- [ ] 10.13 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-explore/SKILL.md` — in the sub-agent prompt block, replace `mem_save(topic_key: "sdd/{change}/exploration", type: "architecture", capture_prompt: false, ...)` with `save_sdd_artifact(project, change_name, kind: "exploration", content, path)`; replace any dependency read (`mem_search` + `mem_get_observation`) with `get_sdd_artifact(change_name, kind)`.
- [ ] 10.14 GREEN: same mechanical change in `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-propose/SKILL.md` — write `kind: "proposal"`.
- [ ] 10.15 GREEN: same in `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-spec/SKILL.md` — write `kind: "spec"` **with `capability`, one `save_sdd_artifact` call per capability**, alongside the three `specs/{capability}/spec.md` files (Spec: "Spec artifacts are persisted per capability"); read `get_sdd_artifact(change, "proposal")`.
- [ ] 10.16 GREEN: same in `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-design/SKILL.md` — write `kind: "design"`; read `get_sdd_artifact(change, "proposal")` and pass the **complete** proposal text to the design sub-agent, never a truncated preview (Spec: "sdd-design reads the full proposal").
- [ ] 10.17 GREEN: same in `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-tasks/SKILL.md` — write `kind: "tasks"`; read **each capability spec** via `get_sdd_artifact(change, "spec", capability)` **and** `get_sdd_artifact(change, "design")`, all in full (Spec: "sdd-tasks reads both the spec and the design").
- [ ] 10.18 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-apply/SKILL.md` — write `kind: "apply-progress"` and re-write `kind: "tasks"` as checkboxes are ticked (identical content is a no-op thanks to hash de-dup, so it may call `save_sdd_artifact` unconditionally); read `get_sdd_artifact(change, "tasks")`; **call `update_sdd_change(phase: 'apply')` on entry** and **`link_sdd_change_memory(change_id, memory_id, relation: 'produced')` for every decision, bugfix, and discovery it records via `store_memory`** (Spec: "sdd-apply advances the phase", "sdd-apply links the memories it produced").
- [ ] 10.19 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-apply/strict-tdd.md` if it names the persistence mode or the `mem_save` call shape.
- [ ] 10.20 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-verify/SKILL.md` (and `strict-tdd-verify.md` if it names the store) — write `kind: "verify-report"`; read spec + tasks + apply-progress via `get_sdd_artifact`; it already calls `resolve_tasks_for_spec` — **add `update_sdd_change(phase: 'verify')`** and `link_sdd_change_memory` for the findings it records. Transitions go through `update_sdd_change`, **never** by editing artifact content.
- [ ] 10.21 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-archive/SKILL.md` — write `kind: "archive-report"`; **add `update_sdd_change(phase: 'archive', status: 'archived')`**; the change's artifacts and revisions remain retrievable afterwards (Spec: "sdd-archive marks the change archived").
- [ ] 10.22 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/sdd-onboard/SKILL.md` — teach the `nexusmind` mode and the new tool names; drop any instruction to search the memory store for `sdd/*` topic keys (it now reads `list_sdd_changes` / `get_sdd_change`).
- [ ] 10.23 GREEN: update `/home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/_shared/sdd-status-contract.md` — the status/`/sdd-continue` reads become `list_sdd_changes` / `get_sdd_change` in `nexusmind` mode (the artifact inventory *is* the DAG state, recoverable with no checkout); keep the `engram` and `openspec` branches intact.
- [ ] 10.24 GREEN: grep the whole skills tree for stragglers — `grep -rn "sdd/{change\|topic_key: \"sdd/\|mem_get_observation\|capture_prompt" /home/cesar/byte4bit/kasymir/kasymir-backoffice-ui/.claude/skills/` must return **zero** hits inside any `sdd-*` skill's `nexusmind` branch (hits inside an explicitly-labelled deprecated-`engram` branch are expected and fine).
- [ ] 10.25 GREEN: package the updated skills as an **immutable harness version** in the NexusMind harness library — `build_harness_manifest_from_path` over the skills dir, then `create_harness` (name: `sdd`, or update the existing one) + `publish_harness_version`, so `nexus-mind`, `kasymir`, and the rest install one shared SDD harness instead of each drifting a private copy (Spec: "The updated skills are published as a version").
- [ ] 10.26 GREEN: update `openspec/config.yaml` in **this** repo — `artifact_store: hybrid` (line 75) → `artifact_store: nexusmind`, now that the mode exists.

### Gate

- [ ] 10.27 GATE: manual end-to-end smoke (proposal §7 criterion 1; Spec: "A phase writes the file and the artifact", "Re-running a phase with edits produces revision 2", "Re-running a phase with no edits produces no revision", "History is not destroyed by a rerun") — run `/sdd-design` on a scratch change against a live backend; confirm `openspec/changes/{change}/design.md` exists on disk **and** `get_sdd_artifact(change, "design")` returns the same content; edit the file, re-run, confirm the artifact reaches **revision 2** with revision 1 still individually retrievable; re-run again with no edits and confirm **no revision 3** is created.
- [ ] 10.28 GATE: manual smoke (**A5**) — point the harness at a backend that rejects the save; confirm the phase **fails loudly** and is not reported as successfully persisted (Spec: "A failed store write is reported, not hidden").
- [ ] 10.29 GATE: manual smoke (proposal §7 criterion 5; Spec: "A fresh machine recovers a change with no checkout") — from a **fresh session with no checkout**, `get_sdd_change` recovers the change's phase, status, and artifact inventory, and `get_sdd_artifact` returns each artifact's full content, and the change can be continued.
- [ ] 10.30 GATE: `cd nexusmind-mcp && npm test` still passes (the skills changed, not the server — the no-regression check on the tools the skills now depend on).

---

## Coverage Checklist (every requirement mapped to at least one RED/GREEN task pair)

### sdd-artifact-store (`specs/sdd-artifact-store/spec.md` — 12 requirements)

- **SDD Changes Are Org-Scoped and Uniquely Keyed by Project and Name** — create with defaults: 2.1/2.2. Upsert, not duplicate: 2.3/2.4, 3.13/3.14. Same name in two projects + unregistered project name: 2.5/2.6. Uniqueness constraint: 1.11/1.12.
- **Artifact Identity Is (change, kind, capability) With an Empty-String Capability Sentinel** — the sentinel + uniqueness: 1.7/1.8, 2.41/2.42. Spec repeats per capability: 1.9/1.10, 2.43/2.44. Reject unknown kind (no artifact, no change): 1.27/1.28, 3.37/3.38.
- **Saving an Artifact Is Idempotent by Content Hash** — first save = rev 1, 200 not 201: 2.25/2.26, 3.31/3.32. Identical re-save creates no revision: 2.27/2.28, 3.33/3.34. Changed content appends: 2.29/2.30. **Revert appends rev 3 (A1): 2.31/2.32.** Unknown change is created: 2.25/2.26, 6.9/6.10.
- **Artifact Revisions Are Immutable and Append-Only** — content never changes: 2.29/2.30. **No API mutates a revision: 2.47/2.48 (store), 3.49/3.50 (HTTP 405).** Git provenance per revision, never clobbered: 2.45/2.46. Monotonic gapless numbering: 2.33/2.34.
- **Artifact Content Is Capped at 1 MB** — 422: 2.37/2.38, 3.35/3.36. **Atomic, no partial state (A2): 2.37/2.38, 3.35/3.36.** Just under the cap accepted, byte_size exact: 2.39/2.40.
- **SDD Operations Are Gated by sdd:read, sdd:write, and sdd:delete** — read denied: 3.3/3.4, 3.51/3.52. Write denied: 3.11/3.12, 3.19/3.20, 3.29/3.30, 3.57/3.58, 3.63/3.64. Delete denied: 3.25/3.26. **Privileged bypass: 3.9/3.10.**
- **SDD Data Is Isolated Per Organization and Never Leaks Existence** — cross-org 404 not 403: 3.15/3.16, 3.43/3.44. Cross-org save does not hijack: 2.51/2.52. Search never crosses: 2.61/2.62. Unknown id 404: 3.15/3.16. Store level: 2.9/2.10.
- **List Endpoints Return Metadata Only, Never Artifact Content** — change list: 3.5/3.6. Revision list: 2.57/2.58, 3.45/3.46. Artifact fetch returns full latest content: 2.53/2.54, 3.39/3.40. Specific revision full content: 2.59/2.60, 3.47/3.48.
- **Artifacts Are Full-Text Searchable Over Their Latest Revision Only** — findable + snippet + identity: 2.61/2.62, 3.51/3.52. Removed term stops matching / one hit per artifact: 2.35/2.36. Idempotent re-save does not disturb the index: 2.27/2.28. Denied without sdd:read: 3.51/3.52.
- **Changes Are Soft-Archived, Never Hard-Deleted** — soft archive + excluded from list: 2.23/2.24, 3.25/3.26. Artifacts survive: 2.23/2.24, 3.17/3.18. Listable on request: 2.15/2.16. Unknown/cross-org 404: 3.27/3.28.
- **Change Listing Supports Filtering, and Change Metadata Is Patchable** — filters: 2.13/2.14. Patch phase: 2.17/2.18, 3.19/3.20. Patch denied: 3.19/3.20. **Identity fields not patchable: 2.19/2.20, 3.23/3.24.**
- **Phase Is Advisory Metadata, Not a Write Gate** — **out-of-order artifact accepted + save does not mutate phase: 2.49/2.50.** Surfaced in the admin and the tools: 8.11/8.12, 6.29/6.30.

### sdd-artifact-links (`specs/sdd-artifact-links/spec.md` — 1 MODIFIED + 5 ADDED)

- **[MODIFIED] Link Creation Validates the Change Name Against the Openspec Trees** — store first, no FS read: 4.1/4.2. FS fallback, active tree: 4.3/4.4. FS fallback, archive tree: 4.5/4.6. Typo'd name now 422 in production: 4.9/4.10. Org-scoped validation: 4.11/4.12. Existing valid links keep working: 4.7/4.8. Archived change still linkable (A8): 2.81/2.82, 4.13/4.14.
- **Tasks Join to Changes by Name, Not by a Foreign Key** — **link created before the change existed resolves once the change appears: 2.79/2.80.** No `change_id` FK / no duplicate source of truth: 2.79/2.80 (source-scan assertion).
- **A Change Exposes the Tasks Linked to It** — linked tasks returned: 2.75/2.76, 4.17/4.18. **Invisible tasks excluded: 2.77/2.78, 4.17/4.18.** Denied without task:read: 4.15/4.16. Empty list, not an error: 2.75/2.76, 4.17/4.18.
- **Changes Link to Memories Many-to-Many With a Relation** — link produced: 2.67/2.68, 3.57/3.58. No duplicate on re-link: 2.67/2.68, 3.57/3.58. **Different relation UPDATES the row (A3): 2.69/2.70, 3.59/3.60.** Cross-org 404: 2.67/2.68, 3.61/3.62. Denied without sdd:write: 3.57/3.58, 3.63/3.64. Deleting the memory removes the link: 1.13/1.14. Unlink: 2.71/2.72, 3.63/3.64.
- **A Change Belongs to One Project and Optionally One Sprint** — assign to sprint: 2.17/2.18. List the changes in a sprint: 2.13/2.14. Sprint delete clears, does not remove the change: 1.13/1.14. Change without a sprint is valid: 2.13/2.14.
- **Global Search Includes an SDD Facet** — facet populated + org-scoped: 4.19/4.20. **Empty facet, never 403, without sdd:read (A4): 4.21/4.22.** Existing facets unaffected: 4.23/4.24, 9.32/9.33.

### sdd-artifact-agent-tools (`specs/sdd-artifact-agent-tools/spec.md` — 8 requirements)

- **The SDD Tools Are Thin Permissioned Wrappers Over the SDD API** — save enforces sdd:write: 6.5/6.6. Search enforces sdd:read + read-only caller: 6.25/6.26. No authority the API does not grant: 6.23/6.24. Backend rejection surfaces as a tool failure: 6.11/6.12. **Exactly seven tools: 6.39/6.40.**
- **save_sdd_artifact Is Idempotent and Reports Whether a Revision Was Created** — identical re-save / edited save: 6.7/6.8. Unknown change is created: 6.9/6.10. **Oversized fails and writes nothing: 6.11/6.12.** Spec per capability: 6.13/6.14.
- **get_sdd_artifact Returns the Full Document, Never a Preview** — full 36 KB document: 6.15/6.16. By change name + kind: 6.17/6.18. **Explicit revision number: 6.19/6.20.** **Missing artifact reports not-found, not an empty document: 6.21/6.22.**
- **list_sdd_changes Reports Change Inventory Without Content** — **filters + no content: 6.27/6.28.**
- **get_sdd_change Returns the Artifact Inventory as Recoverable State** — fresh session with no checkout / inventory beats a stale phase / no inlined content: 6.29/6.30.
- **update_sdd_change Performs Phase and Status Transitions** — advance phase + denied without sdd:write: 6.31/6.32. **Invalid phase rejected atomically + unknown change reports not-found: 6.33/6.34.**
- **search_sdd_artifacts Searches Every Change in the Organization** — **identifiers sufficient to fetch, spans changes, honours the limit: 6.35/6.36** (backed by 2.63/2.64).
- **link_sdd_change_memory Ties Decisions Back to the Change** — link, idempotent re-link, invisible memory fails without writing: 6.37/6.38.

### sdd-artifact-admin (`specs/sdd-artifact-admin/spec.md` — 7 requirements)

- **SDD Navigation and Route Are Gated by sdd:read** — nav visible: 8.15/8.16. Nav hidden: 8.17/8.18. Direct nav denied, no data fetched: 8.13/8.14, 8.19.
- **The SDD Page Lists Changes With a Phase Pipeline Driven by Real Artifacts** — list renders: 8.3/8.4. **Pipeline from the inventory, not a stale phase: 8.11/8.12.** Filter by phase: 8.9/8.10. Empty state: 8.7/8.8. Loading skeleton: 8.5/8.6.
- **A Single Shared Markdown Primitive Renders GFM Across the Admin** — task-list checkboxes: 7.4/7.5. GFM tables: 7.2/7.3. All four call sites repointed, no duplicated map: 7.12–7.15, 7.17. **Embedded HTML/script never executed (A6): 7.6/7.7.**
- **The Change Detail Drawer Shows Artifact Tabs With Revisions and a Raw Toggle** — only existing kinds get tabs: 9.1/9.2. Specs tab per capability: 9.7/9.8. Revision selector refetches: 9.9/9.10, 9.11/9.12. Raw toggle: 9.5/9.6. Linked tasks + memories: 9.13/9.14. Markdown render: 9.3/9.4.
- **The Admin Is Read-Only Over Artifacts** — **no artifact edit/save/delete control: 9.15.** **No artifact-save request issued, and no such client method exists: 9.16/9.17** (backed by 8.4). Per A7 the curation controls at 9.18–9.21 patch change *metadata* and memory links only — never artifact content.
- **The Task Detail Cross-Links to the SDD Change** — link navigates: 9.23/9.25. Phase shown beside the link: 9.23/9.25. **Dangling link renders inert without breaking the view: 9.24/9.25.**
- **SDD Results Appear in the Admin Global Search** — group rendered + navigable: 9.26/9.27, 9.30/9.31. **No results → no group (also covers the caller without sdd:read): 9.28/9.29.**

### sdd-harness-persistence (`specs/sdd-harness-persistence/spec.md` — 5 ADDED + 1 MODIFIED + 1 REMOVED)

- **The nexusmind Persistence Mode Writes to Both the Artifact Store and the Filesystem** — file + artifact: 10.1/10.2, 10.13–10.22, gate 10.27. **Failed store write reported, not hidden (A5): 10.5, gate 10.28.** Git provenance supplied: 10.6. Spec artifacts per capability: 10.15.
- **Re-Running a Phase Appends a Revision Instead of Overwriting** — rev 2 on edit / no rev on no-edit / history not destroyed: gate 10.27 (backed by store tests 2.29/2.30, 2.27/2.28, 2.33/2.34).
- **Cross-Phase Dependency Reads Return Full Documents** — sdd-design reads the full proposal: 10.16. sdd-tasks reads spec + design: 10.17. Fresh machine, no checkout: 10.23, gate 10.29. **Missing dependency stops the phase: 10.11.**
- **Phase and Status Transitions Are Pushed to the Change Record** — sdd-apply advances the phase: 10.18. sdd-apply links the memories it produced: 10.18. sdd-verify transitions: 10.20. sdd-archive marks archived: 10.21. Orchestrator state survives as an artifact: 10.7.
- **The Harness Is Published to the NexusMind Harness Library** — 10.25.
- **[MODIFIED] Artifact Store Mode Resolution** — five-mode set + `nexusmind` default: 10.1, 10.3. `engram` deprecated but selectable, its limitation stated: 10.4. Mode-comparison table covers all five: 10.2.
- **[REMOVED] SDD Artifacts Are Saved as Memories With capture_prompt Disabled** — the paragraph and every `capture_prompt` reference deleted from the contract and from every sub-agent prompt block: **10.8**, 10.13–10.22, verified by the grep at 10.24. Migration clause (legacy memories imported and tagged, never deleted): 5.13–5.20.

**All 5 capabilities, 40 requirements, and every listed scenario map to at least one RED/GREEN task pair. No orphaned requirement.**
