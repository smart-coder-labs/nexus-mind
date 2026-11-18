# PENDING — issue #488 (measure accepted/merged changes, cost per accepted change)

## Done
- Investigated all call sites (migrations v112 is latest → new `run_v113`; webhook `process_delivery`;
  `auto_merge_pull` / `merge_after_soak`; `factory_ops::economics`; admin Economics card; kpis.md).

## Left
- [ ] Backend: `run_v113` adds `merged_at` + `merge_sha` to `autonomous_agent_output_links`, bump `LATEST_USER_VERSION`.
- [ ] Backend: `queries` helper to stamp a merged link by URL (webhook) and by run (worker).
- [ ] Backend: webhook `pull_request` `closed` + `merged: true` stamps the matching link (org-scoped, URL match).
- [ ] Backend: factory's own merge (`auto_merge_pull` / `merge_after_soak`) stamps that run's PR link.
- [ ] Backend: `Economics` gains `accepted_changes` + `cost_per_accepted_change`, `accepted_changes_tracked: true`.
- [ ] Backend tests: migration, webhook merged/unmerged, cross-tenant same PR number, worker merge, economics.
- [ ] Admin: `FactoryEconomics` field, accepted tile + accepted/proposed count, conditional note, tests both states.
- [ ] Docs: kpis.md no longer lists cost per accepted change as unmeasured.
- [ ] Verification (admin build/test; Rust via cargo if available).
