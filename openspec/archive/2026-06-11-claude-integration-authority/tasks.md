# Tasks: Claude Integration Authority

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 280–360 |
| 400-line budget risk | Medium |
| Chained PRs recommended | No |
| Suggested split | Single PR |
| Delivery strategy | single-pr |
| Chain strategy | size-exception |

Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: size-exception
400-line budget risk: Medium

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | All 7 task groups | PR 1 | Single PR; changes are config + descriptions, low review complexity |

---

## Phase 1: API Client (`client.ts`)

- [ ] 1.1 In `apps/nexusmind-mcp/src/client.ts`, append `getMemoryById(id)` function after `listMemories`: calls `GET /v1/memory/${encodeURIComponent(id)}` via existing `request<Memory>()`. Verify: function is exported, compiles with `tsc --noEmit`.
- [ ] 1.2 In `apps/nexusmind-mcp/src/client.ts`, append `deleteMemory(id)` function: calls `DELETE /v1/memory/${encodeURIComponent(id)}` via `request<void>()`. Verify: exported, 204 branch in `request()` returns `undefined as T` (already exists at line 45).

## Phase 2: MCP Tools (`index.ts`)

Depends on Phase 1 (import additions).

- [ ] 2.1 Replace the import line: add `getMemoryById, deleteMemory` to the named imports from `'./client.js'`. Verify: no unused-import TS error.
- [ ] 2.2 Replace `store_memory` description string (second arg to `server.tool`) with the exact mandate from design §2. Verify: description contains "ALWAYS", "do NOT wait", names `title`, `type`, `project`, is ≤ 60 words.
- [ ] 2.3 Replace `search_memory` description string with mandate from design §2. Verify: contains "FIRST action", "if unsure whether to search — search".
- [ ] 2.4 Replace `list_memories` description string with utility framing from design §2. Verify: does NOT contain "ALWAYS" or "MUST"; references `search_memory`.
- [ ] 2.5 Replace `get_context` description string with session-start mandate from design §2. Verify: contains "START of every session", lists "architecture, decisions, patterns, bugs fixed".
- [ ] 2.6 Add `get_memory` tool registration after `get_context` block, using exact handler from design §3.3. Verify: tool named `get_memory`, input schema has `id: z.string()`, handler calls `getMemoryById(id)`, error path returns `isError: true`.
- [ ] 2.7 Add `delete_memory` tool registration after `get_memory`, using exact handler from design §3.4. Verify: input schema has `id: z.string()` and `confirm: z.boolean()`; `confirm !== true` returns `isError: true` with no HTTP call; happy path calls `deleteMemory(id)`.
- [ ] 2.8 Run `tsc --noEmit` from `apps/nexusmind-mcp/`. Verify: zero TypeScript errors.

## Phase 3: Bash Hooks — `nexusmind-mcp` (canonical source)

Scripts 3.1, 3.2, 3.3 are independent and can run in parallel.

- [ ] 3.1 Rewrite `apps/nexusmind-mcp/plugin/scripts/subagent-stop.sh` with keyword quality gate from design §8. Verify: file begins with correct shebang; `KEYWORD_RE` variable is present; `grep -iEq` gate skips when no keyword matches; payload shape contains `type: "discovery"`, `tool: "claude-code-subagent"`, `project`, `content` (truncated at 2000 chars).
- [ ] 3.2 Rewrite `apps/nexusmind-mcp/plugin/scripts/session-start.sh` with project search + type labels + full protocol body from design §7. Verify: `POST /v1/memory/search` with `query=$PROJECT, limit=15` is present; `format_memories` helper function exists; output contains "Project Memories —", "Recent Team Memories", all 6 tools named, "IMMEDIATELY", post-compaction block, health-check guard preserved.
- [ ] 3.3 Rewrite `apps/nexusmind-mcp/plugin/scripts/user-prompt-submit.sh` with 5-part per-prompt injection from design §6. Verify: no `/tmp` state file; two `curl` calls per prompt (recent + project); output JSON has single `systemMessage` field; empty results render `(none)`; no output when `NEXUSMIND_API_KEY` unset.

## Phase 4: Bash Hooks — `nexusmind-claude-plugin` (sync)

Depends on Phase 3 (canonical source must exist first).

- [ ] 4.1 Copy `subagent-stop.sh` from `nexusmind-mcp/plugin/scripts/` to `nexusmind-claude-plugin/plugin/scripts/`. Verify: `diff -q` on both paths exits 0.
- [ ] 4.2 Copy `session-start.sh` from `nexusmind-mcp/plugin/scripts/` to `nexusmind-claude-plugin/plugin/scripts/`. Verify: `diff -q` exits 0.
- [ ] 4.3 Copy `user-prompt-submit.sh` from `nexusmind-mcp/plugin/scripts/` to `nexusmind-claude-plugin/plugin/scripts/`. Verify: `diff -q` exits 0.

## Phase 5: CLAUDE.md Rewrite

Independent — can run in parallel with Phases 3–4.

- [ ] 5.1 Replace `apps/nexus-mind/CLAUDE.md` entirely with the canonical protocol document from design §5. Verify: contains "single source of truth", "PROACTIVE SAVE TRIGGERS" section with ≥ 9 triggers and self-check directive, "WHEN TO SEARCH" section with first-message rule, all 13 type glossary entries, topic_key USE/DO NOT guidance with ≥ 2 example keys, "SESSION CLOSE (MANDATORY)" section with `type: "session_summary"` and ≥ 4 required content fields, "AFTER COMPACTION" 3-step protocol, "Configuration" section naming `${NEXUSMIND_API_KEY}` and `npx`.

## Phase 6: `.mcp.json` Portability

Independent — can run in parallel. Note: if `npx` resolve is needed before publish, intermediate state (env-placeholder key + absolute path) is acceptable until `nexusmind-mcp` is published at the new version.

- [ ] 6.1 Replace `apps/nexus-mind/.mcp.json` with the portable form from design §9: `command: "npx"`, `args: ["-y", "@smart-coder-labs/nexusmind-mcp"]`, `env.NEXUSMIND_API_KEY: "${NEXUSMIND_API_KEY}"`, `env.NEXUSMIND_BASE_URL` as literal URL. Verify: `grep -E 'nm_[a-f0-9]{16,}'` returns no match; file contains `${NEXUSMIND_API_KEY}`; no `/Volumes`, `/Users`, `/home` paths present.

## Phase 7: Integration Smoke Test

Depends on Phases 1–6 all complete.

- [ ] 7.1 Build `nexusmind-mcp`: run `npm run build` in `apps/nexusmind-mcp/`. Verify: `dist/index.js` is produced with zero errors.
- [ ] 7.2 Confirm both `subagent-stop.sh` files are byte-identical: run `diff -q apps/nexusmind-mcp/plugin/scripts/subagent-stop.sh apps/nexusmind-claude-plugin/plugin/scripts/subagent-stop.sh`. Verify: exit 0, no output.
- [ ] 7.3 Confirm both `session-start.sh` files are byte-identical: `diff -q` on both paths. Verify: exit 0.
- [ ] 7.4 Confirm both `user-prompt-submit.sh` files are byte-identical: `diff -q` on both paths. Verify: exit 0.
- [ ] 7.5 Confirm `.mcp.json` has no secret: `grep -E 'nm_[a-f0-9]{16,}' apps/nexus-mind/.mcp.json` → no match.
- [ ] 7.6 Spot-check `index.ts` tool count: built server registers exactly 6 tools (`store_memory`, `search_memory`, `list_memories`, `get_context`, `get_memory`, `delete_memory`).

---

## Spec coverage map

| Task(s) | Spec requirement |
|---------|-----------------|
| 2.2 | mcp-tool-authority / store_memory mandate |
| 2.3 | mcp-tool-authority / search_memory mandate |
| 2.4 | mcp-tool-authority / list_memories utility framing |
| 2.5 | mcp-tool-authority / get_context session-start mandate |
| 1.1, 2.6 | mcp-memory-fetch / get_memory tool |
| 1.2, 2.7 | mcp-memory-delete / delete_memory tool with confirmation gate |
| 5.1 | claude-protocol-doc (all sub-requirements) |
| 3.3, 4.3 | prompt-injection-protocol / 5-part injection on every prompt |
| 3.2, 4.2 | session-bootstrap-protocol / project search + type labels + full protocol |
| 3.1, 4.1 | subagent-capture-gate / keyword gate + byte-identical + aligned payload |
| 6.1 | mcp-config-portability / no key, no absolute path, npx |
