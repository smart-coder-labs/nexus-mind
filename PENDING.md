# PENDING — issue #486: scope `topic_key` upserts to the project

## Done
- Located the single lookup site (`upsert_memory`, apps/backend/src/db/queries.rs:2956) and
  confirmed only `upsert_memory` writes `topic_key` in production code, so the new unique
  index cannot collide with existing rows.

## Left
- queries.rs: key the lookup on `(org_id, topic_key, COALESCE(project_id,''))`, drop
  `project_id` from the UPDATE, refresh the doc comment.
- migrations.rs: add `run_v114` (unique index), bump `LATEST_USER_VERSION` to 114, add a test.
- queries.rs tests: per-project rows, same-project update keeps project_id, NULL bucket.
- Docs: CLAUDE.md, invariants.md:28, glossary.md, remove gaps.md entry 2.
- `cargo test` on the touched filters.
