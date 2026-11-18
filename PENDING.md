# PENDING — issue #488 (Measure accepted/merged changes, cost per accepted change)

## DONE
- Checked repo state: clean tree at `ef78eee`, fresh start (no prior WIP).

## LEFT
- Investigate: migrations.rs v62 links table + LATEST_USER_VERSION, factory_ops::economics, autonomous_webhooks.rs pull_request branch, worker.rs auto_merge_pull, FactoryDigest.tsx/test, types.ts, references/kpis.md.
- Implement migration `run_vN` adding `merged_at` + `merge_sha` to `autonomous_agent_output_links`.
- Stamp merges from webhook (closed + merged) matched by `external_url`, org-scoped.
- Stamp merges from `auto_merge_pull`.
- Extend `Economics` with `accepted_changes`, `cost_per_accepted_change`, `accepted_changes_tracked: true`.
- Admin: tile + accepted/proposed count, types.ts field, tests for both states.
- Update `references/kpis.md`.
- Verify: cargo test (backend), admin tests, fmt/clippy/lint.
