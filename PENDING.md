# PENDING — issue #488: measure accepted (merged) changes

## Done
- Explored: `factory_ops::economics`, `autonomous_agent_output_links` (v62), webhook `pull_request` branch, `auto_merge_pull`/`merge_after_soak`, admin Economics card.

## Left
- [ ] migrations: `run_v114` adds `merged_at`/`merge_sha` to `autonomous_agent_output_links`; bump `LATEST_USER_VERSION` to 114; test.
- [ ] queries: stamp-merge helpers (by PR URL for webhooks, by run+number for factory merges).
- [ ] webhooks: `pull_request` `closed` + `merged: true` stamps the org's link matching the PR URL (cross-tenant test).
- [ ] worker: factory merges stamp the run's PR link.
- [ ] `Economics`: additive `accepted_changes`, `cost_per_accepted_change`, `accepted_changes_tracked: true`.
- [ ] admin: types.ts field, Economics tile + accepted/proposed count, note only while untracked, tests for both states.
- [ ] `.claude/skills/nexus-mind-business/references/kpis.md`: drop the unmeasured note.
- [ ] Verify: `cargo test` (targeted), `npx vitest run FactoryDigest`, `npx tsc -b`.
