# PENDING — issue #488: measure accepted (merged) changes

## Done
- `migrations.rs`: `run_v114` adds `merged_at`/`merge_sha` (+ `(org_id, external_url)` index) to
  `autonomous_agent_output_links`; `LATEST_USER_VERSION` bumped to 114; wired into `run_all`;
  `run_v114_adds_the_merge_stamp_to_output_links_and_keeps_rows` test. No shipped migration edited.
- `queries.rs`: `record_autonomous_output_link_merge_by_url` (webhook, URL-matched, org-scoped) and
  `record_autonomous_run_pull_merge` (factory's own merge, run + PR number); `output_link_merge_tests`
  covers cross-tenant isolation, same-number collisions and idempotency.
- `autonomous_webhooks.rs`: `merged_pull_request` + stamping in `process_delivery`'s `pull_request`
  branch; tests for merged/unmerged/cross-tenant/out-of-repo URLs.
- `worker.rs`: `record_factory_merge` called at all three sites where the factory itself merges
  (`auto_merge_pull` skip path, both `merge_after_soak` paths). Best-effort, never fails the merge.
- `factory_ops.rs::economics`: `accepted_changes`, `cost_per_accepted_change`,
  `accepted_changes_tracked: true`; existing fields unchanged; new test
  `economics_counts_only_merged_changes_as_accepted`.
- Admin: `types.ts` additive fields; `FactoryDigest.tsx` accepted count + cost-per-accepted tiles,
  note only while untracked; `FactoryDigest.test.tsx` covers both states.

## Left
- Verification: `cargo check --tests` / targeted `cargo test` and `npx vitest run FactoryDigest`
  (first build + `npm ci` in progress).
- BLOCKED (needs a maintainer): `.claude/skills/nexus-mind-business/references/kpis.md` still lists
  cost per accepted change as unmeasured. The harness denied writes under `.claude/`. The two edits
  needed: add "accepted (merged) changes, cost per accepted change" to the Factory economics row, and
  drop the trailing "Not measured yet: accepted (merged) Factory changes…" line.
