# PENDING — issue #245 feat(automation): add isolated Claude Code worker

## DONE
- Investigated: no `workers/` directory exists in any branch or commit; no prior
  `claude_code.ts`/`claude_code.test.ts` shape to stay consistent with.
- Confirmed the live owning tasks are `openspec/changes/autonomous-agents-mvp/tasks.md`
  3.1 (RED) / 3.2 (GREEN); `autonomous-loop-engineering` 2.1/2.2 are marked superseded.
- Collected the Rust-side invariants to mirror (`apps/backend/src/automation/worker.rs`,
  `policy.rs`, `profiles.rs`): fixed argv, allowed-tool/permission-mode table, env
  allowlist, stream limits + error codes, `[REDACTED]` literal, denial reasons.

## LEFT
- [ ] Add `workers/` TypeScript package: `package.json`, `tsconfig.json`.
- [ ] Add `workers/src/claude_code.ts` — narrow worker contract.
- [ ] Add `workers/src/claude_code.test.ts` — tests per guarantee.
- [ ] Update this file and emit the final JSON.

## NOTES / RISKS
- This sandbox has no Bash approval for `node`/`npm`, and `node_modules` is installed
  nowhere in the repo, so the vitest run cannot be executed locally; verification relies
  on the factory/CI step.
- No root Node project exists, so the issue's `vitest run` at repo root is not
  reproducible as written. The suite is scoped to the new `workers/` package instead.
- No `package-lock.json` can be generated offline, so wiring a `npm ci`-based CI job for
  `workers/` is intentionally left as follow-up.
