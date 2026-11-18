# PENDING — issue #486: scope `topic_key` upserts to the project

## DONE
- Pulled NexusMind memories for context on the memory module / org-scoped upsert.

## LEFT
- Investigate `upsert_memory` + migrations + inline tests + docs (subagent).
- Implement: project-scoped upsert lookup, drop `project_id` from UPDATE, new `run_vN` unique index, bump `LATEST_USER_VERSION`.
- Extend inline tests near `upsert_memory_topic_key_is_org_scoped`.
- Update CLAUDE.md, skills references: invariants.md, glossary.md, gaps.md.
- Verify: `cargo fmt`, `cargo clippy`, `cargo test` for the backend.
- Emit final JSON.
