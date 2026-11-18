# PENDING — issue #486: project-scope `topic_key` upserts

## Done
- `apps/backend/src/db/queries.rs` — `upsert_memory` matches on
  `(org_id, topic_key, COALESCE(project_id,''))`; the UPDATE no longer writes
  `project_id`, so an upsert never moves a row to another project. Doc comment updated.
- `apps/backend/src/db/migrations.rs` — new `run_v114` creating
  `idx_memories_org_topic_project` (UNIQUE, partial on `topic_key IS NOT NULL`,
  `COALESCE(project_id,'')`); falls back to a non-unique index when a legacy DB already
  holds duplicate `(org, topic_key, project)` groups so startup cannot abort.
  `LATEST_USER_VERSION` bumped to 114 and `run_v114` wired into `run_all`.
  No shipped migration edited.
- Tests added in `queries.rs`: `upsert_memory_topic_key_is_project_scoped`,
  `upsert_memory_second_store_from_the_same_project_updates_in_place`,
  `upsert_memory_org_shared_bucket_is_its_own_namespace`. In `migrations.rs`:
  `run_v114_indexes_topic_key_per_project_on_a_database_that_already_has_memories`,
  `run_v114_keeps_pre_upsert_duplicate_topic_keys_with_a_non_unique_index`,
  `run_v114_is_idempotent`. All pass (`cargo test --lib upsert_memory`, `--lib run_v114`),
  and the existing `upsert_memory_topic_key_is_org_scoped` cross-org test still passes.
- `CLAUDE.md` — the store_memory protocol no longer calls the upsert org-wide.

## Left
- Blocked: `.claude/skills/nexus-mind-business/references/{invariants.md,glossary.md,gaps.md}`
  could not be written (the harness denies writes under `.claude/`). Replacement text is
  in the PR summary for a maintainer to apply.
- Verification in flight: `cargo test --lib db::migrations::`, `--lib api::memory::`,
  `--bin import_sdd`, `--test integration_test`.
