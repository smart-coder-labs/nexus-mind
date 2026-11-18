# PENDING — issue #245 feat(automation): add isolated Claude Code worker

## Context
Task 3.2 of `openspec/changes/autonomous-agents-mvp/tasks.md`. Mirror the Rust
precedent in `apps/backend/src/automation/{worker,runtime,profiles}.rs` as a narrow
TypeScript worker contract. New `workers/` npm package (none exists today).

## DONE
- [x] Investigated repo: no `workers/` package, no root workspace; `apps/admin` is
      the vitest `^2.1.8`/v2.1.9 + ESM + strict-TS precedent. Prior art and the
      normative spec/design clauses identified.

## LEFT
- [ ] Scaffold `workers/` package: `package.json` (ESM, vitest, tsc typecheck),
      `tsconfig.json` (strict, node), `vitest.config.ts` (node env).
- [ ] Implement `workers/src/claude_code.ts`: fixed noninteractive argv builder
      (no arbitrary argv), pinned managed-extension validation, strict stream-event
      parsing, canonical sandbox path resolution, event redaction, ephemeral secret
      teardown.
- [ ] Add `workers/src/claude_code.test.ts` (4 tests, one per concern cluster).
- [ ] Wire a `workers` job into `.github/workflows/ci.yml`.
- [ ] Verify: `cd workers && npm run test` and `npm run typecheck`.
- [ ] Emit final JSON.
