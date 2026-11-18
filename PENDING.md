# PENDING — issue #486: scope `topic_key` upserts to the project

## DONE
- `apps/backend/src/db/queries.rs` — `upsert_memory` now matches on `(org_id, topic_key, project_id)` via
  `COALESCE(project_id, '') = COALESCE(?3, '')` (NULL = org-shared bucket, its own namespace); the UPDATE no
  longer writes `project_id` (`id` is now `?8`), so a row never moves project; doc comment rewritten.
- `apps/backend/src/db/migrations.rs` — `LATEST_USER_VERSION` 112 → 113; new `run_v113` creating
  `idx_memories_org_topic_project` (`UNIQUE ... (org_id, topic_key, COALESCE(project_id,''))` where
  `topic_key IS NOT NULL`), registered after `run_v112` in `run_all`. No shipped migration edited.
- Tests in `queries.rs` (`v2 upsert tests` section): `upsert_memory_topic_key_is_scoped_per_project`,
  `upsert_memory_same_project_same_topic_key_updates_in_place`,
  `upsert_memory_org_shared_bucket_does_not_match_project_row`. Cross-org test left untouched.
- `apps/backend/src/bin/import_sdd.rs` — test fixture `import_legacy_memories_replays_the_memories_oldest_first`
  seeded two rows with the same `(org, topic_key, NULL project)`; the newer one now lives in a `legacy` project
  so it no longer violates the new index. Importer behaviour and the oldest-first assertion are unchanged.
- `CLAUDE.md:33` — no longer claims the upsert is org-wide.

## LEFT / BLOCKED
- [ ] Static cross-check of the new test code (signatures, formatting, data safety) — subagent running.
- [x] BLOCKED: `.claude/skills/nexus-mind-business/references/{invariants.md,glossary.md,gaps.md}` cannot be
  written in this sandbox (edits under `.claude/` are refused). Needed edits, for a follow-up:
  - `invariants.md:28` — key is `(org_id, topic_key, project_id)`, per project, NULL bucket separate, row never
    moves project.
  - `glossary.md:8` — `topic_key` is a per-project (within an org) upsert key, not org-wide.
  - `gaps.md` — remove entry 2 of `## Security / correctness` and renumber 3-8.
  - `apps/backend/AGENTS.md:28` also still says "add `run_v112` (next number)" — now stale.
- [x] BLOCKED: no Rust toolchain / no approval for `cargo` in this sandbox, so `cargo test` and `cargo clippy`
  were not run here; verification happens in the worker's mandatory verification step.
