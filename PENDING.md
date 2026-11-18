# PENDING — issue #488 (accepted/merged change economics)

## DONE
- Investigated: migrations.rs (LATEST_USER_VERSION = 112, next is v113), factory_ops.rs `economics`,
  autonomous_webhooks.rs `process_delivery`, worker.rs merge paths, queries.rs link insert,
  admin FactoryDigest.tsx / types.ts / test, kpis.md location.
- Plan fixed (deviations from the issue text, all noted for the PR body):
  - `kind` CHECK only admits `draft_pr` for PRs, so the merge stamp matches on `external_url`.
  - the webhook stamp must land before the `pull_request` early-returns in `process_delivery`.
  - the factory merges in `auto_merge_pull`'s skip path and in `merge_after_soak`, so every
    successful `merge_github_pull` call site stamps.

## LEFT
- [ ] Backend: migration v113 (`merged_at`, `merge_sha`), queries helper, `Economics` additive
      fields, webhook stamp, worker stamp, tests.
- [ ] Admin: `types.ts`, FactoryDigest.tsx tile + counts, FactoryDigest.test.tsx both states.
- [ ] Docs: kpis.md no longer lists cost per accepted change as unmeasured.
- [ ] Verify: admin vitest; backend via CI (no Rust toolchain on this box).
