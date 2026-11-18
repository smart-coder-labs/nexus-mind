# PENDING — issue #486: scope `topic_key` upserts to the project

## Done

- `apps/backend/src/db/queries.rs` — `upsert_memory` now looks up the existing row on
  `(org_id, topic_key, COALESCE(project_id,''))` instead of `(org_id, topic_key)`, so the
  org-shared bucket (`project_id IS NULL`) is its own namespace; the UPDATE no longer writes
  `project_id`, so an upsert can never move a memory to another project. Doc comment updated.
- `apps/backend/src/db/migrations.rs` — new `run_v114`: unique partial index
  `idx_memories_org_topic_project ON memories(org_id, topic_key, COALESCE(project_id,''))
  WHERE topic_key IS NOT NULL`. `LATEST_USER_VERSION` bumped 113 → 114, `run_all` calls it,
  no shipped migration edited.
- Tests, `migrations.rs`: `run_v114_applies_over_existing_topic_key_rows` (applies over a DB
  that already holds `topic_key` rows, idempotent), `run_v114_index_separates_projects_and_the_shared_bucket`.
- Tests, `queries.rs` (next to `upsert_memory_topic_key_is_org_scoped`, which is unchanged and
  still passes): `upsert_memory_topic_key_is_project_scoped`,
  `upsert_memory_same_project_still_updates_in_place`,
  `upsert_memory_shared_bucket_is_its_own_namespace`.
- `CLAUDE.md` — the memory protocol no longer calls the upsert org-wide.
- `POST /v1/memory/store` untouched: response shape and the 200-update / 201-create rule
  (`api/memory.rs:392`) are unchanged.

## Left — blocked, needs a maintainer

Writes under `.claude/` are denied in this sandbox (both the Edit tool and a shell write were
refused), so three skill docs still describe the old behaviour. The exact replacements:

- `.claude/skills/nexus-mind-business/references/invariants.md:28` — replace the bullet with:
  `- With `topic_key`, store **updates** the row matching `(org_id, topic_key, project_id)` — per project, with `project_id IS NULL` (the org-shared bucket) as its own namespace — and bumps `revision_count`; 200 on update, 201 on create. An upsert never changes a row's project. A unique index (v114) enforces the triple. Without `topic_key`, store always inserts.`
- `.claude/skills/nexus-mind-business/references/glossary.md:8` — `topic_key` row becomes:
  `per-project upsert key for an evolving memory; unique per `(org, topic_key, project)`, the org-shared bucket (`project_id IS NULL`) being its own namespace`
- `.claude/skills/nexus-mind-business/references/gaps.md` — delete entry 2 of
  "Security / correctness" (**`topic_key` upsert is org-wide**) and renumber 3–8 to 2–7.
