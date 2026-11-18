# PENDING — issue #488 (measure accepted/merged changes, cost per accepted change)

## Done
- `migrations.rs`: `run_v114` adds `merged_at` + `merge_sha` to `autonomous_agent_output_links`;
  `LATEST_USER_VERSION` 113 → 114; test `run_v114_adds_merged_at_to_output_links_and_defaults_it_to_null`.
  No shipped migration edited.
- `queries.rs`: `mark_autonomous_output_link_merged_by_url` (webhook, URL match, org-scoped, first merge wins)
  and `mark_autonomous_output_link_merged_for_run` (factory's own merge, run + PR number).
- `api/autonomous_webhooks.rs`: `process_delivery` stamps `merged_at` on `pull_request` `closed` with
  `merged: true`, before any routing early-return. Test covers unmerged close, same PR number in another
  org, another PR, and a replayed delivery.
- `automation/worker.rs`: `record_factory_merge` helper, called at all three factory merge sites
  (`auto_merge_pull` skip path, and both `merge_after_soak` paths). Test
  `a_merge_the_factory_performs_records_the_accepted_change` (incl. cross-tenant and cross-run).
- `db/factory_ops.rs`: `Economics` gains `accepted_changes` and `cost_per_accepted_change`;
  `accepted_changes_tracked` is now `true`. Existing fields untouched. Two new tests plus a cross-tenant one.
- `api/factory.rs`: doc comment only; the endpoint is otherwise unchanged (additive response).
- Admin: `types.ts` additive fields, `FactoryDigest.tsx` "Cost per accepted change" tile +
  "N of M accepted" / "M proposed" counts, note only while untracked; both states in
  `FactoryDigest.test.tsx`. `npx vitest run src/pages/FactoryDigest.test.tsx` → 14 passed; `npx tsc -b` clean.

## Left
- [ ] Backend test run in progress: `economics`, `run_v114`, `a_merged_pull_request`,
      `a_merge_the_factory_performs`, `run_all_ends_on_the_latest`.

## Not done (blocked)
- `.claude/skills/nexus-mind-business/references/kpis.md` still lists cost per accepted change as
  unmeasured: writes under `.claude/` are denied by this harness's permissions, and per `CLAUDE.md`
  harness/skill files ship in their own `chore(harness)` PR. One line to change:
  the Factory economics KPI row should also list "accepted (merged) changes, cost per accepted change",
  and the "Not measured yet:" line should drop it.
