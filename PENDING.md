# PENDING — issue #245 `feat(automation): add isolated Claude Code worker`

## Done

- Read the SDD context: `openspec/changes/autonomous-loop-engineering/{tasks,apply-progress}.md`
  (work units 1–2 landed in the backend; this issue is the execution-plane adapter)
  and D7 of `openspec/changes/autonomous-agents-mvp/design.md` (headless Claude Code runtime rules).

## Left

- [ ] `workers/` package scaffolding (package.json, tsconfig, README)
- [ ] `workers/src/claude_code.ts` — narrow worker contract
- [ ] `workers/src/claude_code.test.ts` — 4 tests
- [ ] CI job for `workers/`
- [ ] Verify
