# PENDING — issue #488 (measure accepted/merged changes, cost per accepted change)

## Done
- Investigated all call sites. Confirmed anchors: `LATEST_USER_VERSION = 112` (migrations.rs:12) so the
  new migration is **`run_v113`**; table created in v62 (migrations.rs:1766); `economics` at
  factory_ops.rs:250-295 with `Economics` at :232-248; webhook `process_delivery` at
  autonomous_webhooks.rs:349-430 (`pull_request` block :409-414); factory merges happen in
  `auto_merge_pull` (worker.rs:3952, merge at :3990) and `merge_after_soak` (:4057, merges at
  :4097 and :4134); link created at worker.rs:3313 via `queries::create_autonomous_output_link`
  (queries.rs:19047); admin `FactoryEconomics` types.ts:1619, `Economics` card
  FactoryDigest.tsx:567-621, tests FactoryDigest.test.tsx:64-79/261-267; kpis.md lines 10 and 15.
- Decisions: one org-scoped helper matching `external_url` exactly, used by BOTH the webhook and the
  factory's own merge (the `draft_pr` link belongs to the resolver run, not the merging reviewer run,
  so matching by run_id would miss). `accepted_changes` counts links proposed in the window that have
  `merged_at IS NOT NULL`, so accepted/proposed is a true ratio.
- Reverted stray `apps/landing/.astro/*` deletions left by the previous checkpoint (files restored on
  disk; they are gitignored so they need `git add -f` to leave the diff).

## Left
- [ ] Backend: `run_v113` adds `merged_at` + `merge_sha` to `autonomous_agent_output_links`, bump `LATEST_USER_VERSION`.
- [ ] Backend: `queries::mark_autonomous_output_link_merged` (org-scoped, exact `external_url` match, first merge wins).
- [ ] Backend: webhook `pull_request` `closed` + `merged: true` stamps the matching link.
- [ ] Backend: factory's own merge (`auto_merge_pull` + `merge_after_soak`) stamps the merged PR's link.
- [ ] Backend: `Economics` gains `accepted_changes` + `cost_per_accepted_change`, `accepted_changes_tracked: true`.
- [ ] Backend tests: migration, webhook merged/unmerged, cross-tenant same PR number, helper, economics.
- [x] Admin: `FactoryEconomics` gains `accepted_changes` + `cost_per_accepted_change`; "Cost per accepted
      change" and "Accepted changes" (`n / proposed`) tiles added to the Economics card, both gated on
      `accepted_changes_tracked` so they can never contradict the "not measured yet" note, which still
      renders only while the flag is false. Dev preview fixture
      (`apps/admin/dev/workflows-preview-data.mjs`) updated too. Tests cover tracked-false (note shown,
      tiles hidden), tracked-true with merges, and tracked-true with zero merges (`—`).
- [ ] BLOCKED — Docs: kpis.md. The harness refuses writes under `.claude/**`, and the file lives at
      `.claude/skills/nexus-mind-business/references/kpis.md`. Still to apply by hand:
      line 10: append `, accepted (merged) changes, cost per accepted change` to the Factory economics row;
      line 15: replace the "Not measured yet: accepted (merged) Factory changes..." sentence, since the
      hole is now closed by `db/factory_ops.rs` + the Economics card.
- [ ] Verification: no Rust toolchain on this box (documented in CLAUDE.md) → backend by review + CI;
      admin `node_modules` absent, so vitest/tsc only if `npm ci` is possible.
