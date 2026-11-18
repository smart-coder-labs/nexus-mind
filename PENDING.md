# PENDING — issue #245 `feat(automation): add isolated Claude Code worker`

## Done

- Read the `autonomous-loop-engineering` SDD (`tasks.md`, `apply-progress.md`,
  `exploration.md`) — architecture point 2 "Isolated execution plane" is the
  contract this worker implements.
- Read the control-plane half already shipped:
  `apps/backend/src/automation/{profiles,policy}.rs` (`ExtensionRef`,
  managed profiles `read-only` / `implementation` / `qa-deploy`,
  `validate_extensions`) and the `[REDACTED]` / prompt-on-stdin conventions in
  `apps/backend/src/automation/{worker,sandboxed}.rs`.

## Left

- [ ] Add `workers/` npm package (package.json, tsconfig.json, README.md).
- [ ] Add `workers/src/claude_code.ts` — the narrow execution-plane contract.
- [ ] Add `workers/src/claude_code.test.ts` — 4 vitest tests.
- [ ] Wire a `workers` job into `.github/workflows/ci.yml`.
- [ ] Verify: `npx tsc -b` and `npx vitest run` from `workers/`.
