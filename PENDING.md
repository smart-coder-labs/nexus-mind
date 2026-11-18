# PENDING — issue #245 feat(automation): add isolated Claude Code worker

## Context
Task 3.2 of `openspec/changes/autonomous-agents-mvp/tasks.md`. Mirror the Rust
precedent in `apps/backend/src/automation/{worker,runtime,profiles,policy}.rs` as a
narrow TypeScript worker contract. New `workers/` npm package (none exists today).

## DONE
- [x] Investigated repo: no `workers/` package, no root workspace; `apps/admin` is
      the vitest `^2.1.8`/v2.1.9 + ESM + strict-TS precedent. Node 20, npm (not
      pnpm), per-package change-gated CI, no eslint/prettier contract.
- [x] Extracted the normative clauses (spec.md isolation/runtime-health/connector
      redaction, design.md D6/D7) and the Rust contract surface to mirror:
      fixed argv builder, env allowlist, `ExtensionRef` pinning + policy deny
      reasons, `parse_claude_event_stream` caps, `sanitize_output` patterns,
      `DiskTree::inside` symlink guard, teardown on every exit path.

## LEFT
- [ ] Scaffold `workers/` package: `package.json` (ESM, vitest, tsc typecheck),
      `tsconfig.json` (strict, NodeNext), `vitest.config.ts` (node env),
      `package-lock.json` (CI runs `npm ci`).
- [ ] Implement `workers/src/claude_code.ts`: fixed noninteractive argv builder
      (no arbitrary argv), restricted env, pinned managed-extension validation,
      strict stream-event parsing, canonical sandbox path resolution, event
      redaction, ephemeral secret broker teardown.
- [ ] Add `workers/src/claude_code.test.ts` (4 tests, one per concern cluster).
- [ ] Wire a `workers` job + paths-filter entry into `.github/workflows/ci.yml`.
- [ ] Restore `apps/landing/.astro/*` generated files dropped by the WIP
      checkpoint (unrelated to this issue; must not appear in the PR diff).
- [ ] Verify: `cd workers && npm run test` and `npm run typecheck`.
- [ ] Emit final JSON.
