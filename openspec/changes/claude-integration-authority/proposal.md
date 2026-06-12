# Proposal: Claude Integration Authority

## Intent

Today the NexusMind Claude integration is technically functional but contextually **passive**. The MCP tool descriptions are neutral ("Store a memory, decision, or piece of context"), the `CLAUDE.md` is a 4-line reminder that competes with framework-level rules (Engram, SDD, project conventions), the `user-prompt-submit` hook emits a one-line systemMessage, and `.mcp.json` ships a hardcoded production API key plus an absolute path that breaks for anyone cloning the repo. Two backend endpoints that already exist — `GET /v1/memory/:id` and `DELETE /v1/memory/:id` — are not surfaced through the MCP tool layer, so Claude cannot read full untruncated memories or remove stale ones.

The result: when Claude has a competing instruction set (e.g. another memory tool, a heavier framework rule, or just no explicit nudge), NexusMind loses. We want Claude to treat NexusMind as the **single source of truth** for this codebase — more persistent than session context, more instructional than competing tools, more proactive than any other framework. That requires making every surface that Claude reads (tool descriptions, CLAUDE.md, hook-injected system messages, session-start additionalContext) carry a behavioral mandate, and closing the two MCP gaps so Claude has the full API.

## Scope

### In Scope
- Rewrite all 4 existing MCP tool descriptions (`store_memory`, `search_memory`, `list_memories`, `get_context`) in `nexusmind-mcp/src/index.ts` as behavioral mandates that go directly into Claude's system prompt.
- Add 2 new MCP tools: `get_memory` (full untruncated content by ID) and `delete_memory` (remove stale/incorrect memories).
- Add 2 new backend client functions in `nexusmind-mcp/src/client.ts`: `getMemoryById(id)` → `GET /v1/memory/:id`, `deleteMemory(id)` → `DELETE /v1/memory/:id`.
- Rewrite `nexus-mind/CLAUDE.md` from a 4-line reminder into a full MANDATORY PROTOCOL that survives compaction (proactive save triggers, first-message search rule, type glossary for all 13 types, topic_key guidance, session close rule, post-compaction recovery, "single source of truth" framing).
- Rewrite `user-prompt-submit.sh` in BOTH `nexusmind-mcp` and `nexusmind-claude-plugin` to inject a 5-part system-prompt block on every prompt: (1) recent session memories, (2) project-specific memories, (3) behavioral mandate, (4) save-after-decision reminder, (5) format hint.
- Rewrite `session-start.sh` in BOTH repos to add a project-specific search (`POST /v1/memory/search { query: $PROJECT, limit: 15 }`), format results with type labels (not raw snippets), and include the full protocol (not just "save after decisions").
- Align `subagent-stop.sh` in BOTH repos to a single quality-gated pattern: only capture if the output contains decision-like keywords (`decided`, `fixed`, `error`, `warning`, `convention`, `architecture`, `discovered`, `issue`, `solution`).
- Fix `nexus-mind/.mcp.json`: replace the hardcoded API key with `${NEXUSMIND_API_KEY}`, replace the absolute path with `npx -y @smart-coder-labs/nexusmind-mcp`, document the env-placeholder convention.

### Out of Scope
- Backend endpoint changes (`GET`/`DELETE` already implemented in `apps/backend`).
- New memory types or schema migrations (covered by `memory-schema-v2`).
- Policy engine integration with new tools (covered by `policy-engine`).
- Hook protocol changes for non-Claude tools (Cursor rules, Windsurf, etc.).
- A migration script to rewrite existing `.mcp.json` files on user machines — we document the new format and let users update on next setup.
- IDE-side caching, telemetry, or analytics about which tools Claude actually calls.
- Renaming any of the existing tools (`store_memory`, `search_memory`, etc.) — purely additive plus description rewrites.

## Capabilities

### New Capabilities
- `mcp-tool-authority`: behavioral-mandate tool descriptions that act as system-prompt instructions for Claude (covers all 6 tools including the 2 new ones).
- `mcp-memory-fetch`: `get_memory` MCP tool that returns full untruncated content for a memory ID.
- `mcp-memory-delete`: `delete_memory` MCP tool that removes a memory by ID (with confirmation contract).
- `claude-protocol-doc`: structured CLAUDE.md protocol document with triggers, type glossary, topic_key guidance, and session-close + post-compaction recovery rules.
- `prompt-injection-protocol`: 5-part system-prompt injection from `user-prompt-submit.sh` covering session context, project context, behavioral mandate.
- `session-bootstrap-protocol`: project-aware session-start additionalContext with type-labeled memories and full protocol reminder.
- `subagent-capture-gate`: aligned, keyword-gated subagent-output capture across both Claude plugin repos.
- `mcp-config-portability`: env-placeholder + npx-based `.mcp.json` convention so the repo is clone-and-run for any developer with a key.

### Modified Capabilities
- None at the spec level. All existing MCP tool **signatures** are preserved; only descriptions change. All hook **contracts** (stdin payload, expected JSON output) are preserved; only the content of the emitted messages changes. The HTTP API is untouched.

## Approach

1. **Backend client first** (`client.ts`): add `getMemoryById` and `deleteMemory` calling endpoints that already exist.
2. **Tool descriptions and new tools** (`index.ts`): rewrite the 4 existing descriptions as imperative mandates; register `get_memory` and `delete_memory` using the new client functions; add a confirmation parameter for `delete_memory` to require explicit `confirm: true`.
3. **CLAUDE.md protocol**: rewrite as a structured protocol with sections mirroring the global Engram convention but scoped to NexusMind. The doc explicitly names NexusMind as the "single source of truth" for this codebase and always passes `project="nexus-mind"`.
4. **Hooks rewrite** in both `nexusmind-mcp/plugin/scripts/` and `nexusmind-claude-plugin/plugin/scripts/`:
   - `session-start.sh`: keep current health-check guard; add a second curl call to `POST /v1/memory/search` with `query=$PROJECT, limit=15`; format with type labels; emit the full protocol.
   - `user-prompt-submit.sh`: replace one-line systemMessage with a 5-part block: recent session memories (last 5), project-specific memories (last 5, fetched via search), behavioral mandate, save-after-decision reminder, format hint.
   - `subagent-stop.sh`: add `grep -iE` keyword gate before storing; align both repos to the same script.
5. **`.mcp.json`** in `nexus-mind/`: replace the literal API key string with `${NEXUSMIND_API_KEY}` (Claude Code expands env vars in `.mcp.json` env blocks), swap absolute path for `npx -y @smart-coder-labs/nexusmind-mcp`, add a comment-free README note in `CLAUDE.md` explaining the convention.
6. **No DB or backend changes** — every capability already exists server-side.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `nexusmind-mcp/src/index.ts` | Modified | Rewrite 4 tool descriptions; add `get_memory` + `delete_memory` registrations |
| `nexusmind-mcp/src/client.ts` | Modified | Add `getMemoryById(id)` and `deleteMemory(id)` |
| `nexusmind-mcp/plugin/scripts/session-start.sh` | Modified | Add project-specific search; type-labelled formatting; full protocol body |
| `nexusmind-mcp/plugin/scripts/user-prompt-submit.sh` | Modified | 5-part injection on every prompt with session + project memories |
| `nexusmind-mcp/plugin/scripts/subagent-stop.sh` | Modified | Add keyword quality gate before capture |
| `nexusmind-claude-plugin/plugin/scripts/session-start.sh` | Modified | Mirror of nexusmind-mcp version (same content) |
| `nexusmind-claude-plugin/plugin/scripts/user-prompt-submit.sh` | Modified | Mirror of nexusmind-mcp version (same content) |
| `nexusmind-claude-plugin/plugin/scripts/subagent-stop.sh` | Modified | Mirror of nexusmind-mcp version (same content) |
| `nexus-mind/CLAUDE.md` | Modified | Rewrite as MANDATORY PROTOCOL with full type glossary and recovery rules |
| `nexus-mind/.mcp.json` | Modified | `${NEXUSMIND_API_KEY}` placeholder + `npx` command, no absolute paths |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Tool descriptions become so long Claude's system prompt budget suffers | Med | Keep each description under 60 words; one-sentence mandate + one example |
| `delete_memory` is called accidentally by Claude during a normal flow | Med | Require `confirm: true` parameter; tool description explicitly says "USER must request deletion explicitly" |
| `${NEXUSMIND_API_KEY}` env expansion not supported in older Claude Code | Low | Claude Code 1.0+ supports env-var expansion in `.mcp.json`; document min version in CLAUDE.md |
| `npx -y` adds startup latency on first run | Low | One-time pull; subsequent runs cache; acceptable for clone-and-run portability |
| 5-part user-prompt injection inflates context on every prompt | Med | Cap each section to top-5 results, ~120 chars each (~3 KB total per prompt) |
| Hook keyword gate misses real decisions (e.g. user writes "we'll use X") | Low | Keyword list is permissive (9 triggers); false negatives are acceptable — the agent still has direct `store_memory` access |
| The new CLAUDE.md conflicts with the user's global Engram protocol | Low | Explicit framing: NexusMind is the project-scoped store; Engram is the cross-session memory; both can coexist |
| Two-repo hook duplication drifts over time | Med | Document that the two plugin scripts MUST stay in sync; verify-phase checks file hash equality |

## Rollback Plan

1. **MCP tools**: revert `src/index.ts` and `src/client.ts` to previous commit; republish `@smart-coder-labs/nexusmind-mcp` at previous version; users on `npx -y @smart-coder-labs/nexusmind-mcp` automatically pick up the prior tools.
2. **Hooks**: each script is a single file in `plugin/scripts/`; `git revert` restores prior behavior; no state migration required (state file in `/tmp/nexusmind-session-*` is regenerated each session).
3. **CLAUDE.md**: pure documentation revert; no runtime impact.
4. **`.mcp.json`**: users keep their local override if they edited; we revert the committed default. Provide a one-line `git diff` snippet so users can re-apply if their personal env didn't have `NEXUSMIND_API_KEY` set.
5. No DB / backend rollback — server is untouched.

## Dependencies

- Backend endpoints already implemented: `GET /v1/memory/:id`, `DELETE /v1/memory/:id`, `POST /v1/memory/search`.
- Claude Code 1.0+ for `.mcp.json` env-var expansion.
- Published npm package `@smart-coder-labs/nexusmind-mcp` for the `npx` path.

## Success Criteria

- [ ] All 6 MCP tool descriptions are imperative behavioral mandates (verbs first: ALWAYS / CALL / FETCH / DO NOT).
- [ ] `get_memory(id)` returns full untruncated content; `delete_memory(id, confirm: true)` returns 204.
- [ ] `delete_memory` called without `confirm: true` returns an `isError` response with a clear refusal message.
- [ ] `nexus-mind/CLAUDE.md` covers: proactive triggers, first-message search rule, all 13 type definitions, topic_key upsert guidance, session close rule, post-compaction recovery, single-source-of-truth framing.
- [ ] `session-start.sh` outputs project-specific memories with `[type] title` labels alongside the last-10 recency block.
- [ ] `user-prompt-submit.sh` emits a 5-part system-prompt block on every prompt (not just first + periodic).
- [ ] Both Claude plugin repos ship identical `subagent-stop.sh` files (byte-equal) with the keyword gate active.
- [ ] `nexus-mind/.mcp.json` contains no literal API key and no absolute filesystem path.
- [ ] A fresh clone of `nexus-mind` with only `NEXUSMIND_API_KEY` set in the shell connects to NexusMind successfully via `.mcp.json`.
