# PENDING — issue #488: measure accepted (merged) changes

## Done
- Read `factory_ops::economics`, `autonomous_webhooks::process_delivery`, `worker::auto_merge_pull` /
  `merge_after_soak`, the admin Economics card and its tests.

## Left
- [ ] `run_v114` in `migrations.rs`: `merged_at` + `merge_sha` on `autonomous_agent_output_links`, bump
      `LATEST_USER_VERSION`, wire into `run_all`, add a migration test.
- [ ] `factory_ops::record_merged_change(conn, org, pr_url, merge_sha)` — org-scoped, matches on
      `external_url`, never the bare number.
- [ ] Webhook: `pull_request` `closed` + `merged: true` stamps the link.
- [ ] Factory merges (`auto_merge_pull` skip path, `merge_after_soak`) stamp the link.
- [ ] `Economics`: additive `accepted_changes` + `cost_per_accepted_change`, `accepted_changes_tracked: true`.
- [ ] Admin: `types.ts`, `FactoryDigest.tsx` tile + accepted/proposed count, both states in
      `FactoryDigest.test.tsx`.
- [ ] `.claude/skills/nexus-mind-business/references/kpis.md`: drop the "not measured yet" entry.
- [ ] Verify: `cargo test` (targeted), `npx vitest run FactoryDigest`, `npx tsc -b`.
