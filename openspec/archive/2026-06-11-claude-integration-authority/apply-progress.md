# Apply Progress: Claude Integration Authority

## Status: done (all phases complete)

---

## Phase 1: API Client (`client.ts`)

- [x] 1.1 Added `getMemoryById(id)` after `listMemories` — calls `GET /v1/memory/${encodeURIComponent(id)}` via `request<Memory>()`. Exported. `tsc --noEmit` passes.
- [x] 1.2 Added `deleteMemory(id)` — calls `DELETE /v1/memory/${encodeURIComponent(id)}` via `request<void>()`. Exported. 204 branch in `request()` returns `undefined as T` (existing at line 45).

## Phase 2: MCP Tools (`index.ts`)

- [x] 2.1 Added `getMemoryById, deleteMemory` to the named imports from `'./client.js'`.
- [x] 2.2 Replaced `store_memory` description with behavioral mandate containing "ALWAYS", "do NOT wait", names `title`, `type`, `project`.
- [x] 2.3 Replaced `search_memory` description containing "FIRST action" and "if unsure whether to search — search".
- [x] 2.4 Replaced `list_memories` description with utility framing referencing `search_memory`. Does NOT contain "ALWAYS" or "MUST".
- [x] 2.5 Replaced `get_context` description containing "START of every session" and lists "architecture, decisions, patterns, bugs fixed, discoveries".
- [x] 2.6 Added `get_memory` tool registration after `get_context`. Named `get_memory`, schema has `id: z.string()`, handler calls `getMemoryById(id)`, error path returns `isError: true`.
- [x] 2.7 Added `delete_memory` tool after `get_memory`. Schema has `id: z.string()` and `confirm: z.boolean()`. `confirm !== true` returns `isError: true` with no HTTP call. Happy path calls `deleteMemory(id)`.
- [x] 2.8 `tsc --noEmit` from `apps/nexusmind-mcp/` — zero TypeScript errors. `npm run build` — zero errors, `dist/index.js` produced.

## Phase 3: Bash Hooks — `nexusmind-mcp` (canonical source)

- [x] 3.1 Rewrite `subagent-stop.sh` with keyword quality gate from design §8. KEYWORD_RE with 21 terms; min length 100; title field added; passive_capture metadata removed; type/tool/project/content payload shape aligned.
- [x] 3.2 Rewrite `session-start.sh` with project search + type labels + full protocol body from design §7. POST /v1/memory/search with query=$PROJECT, limit=15; format_memories helper; Project Memories + Recent Team Memories sections; full PROACTIVE SAVE + WHEN TO SEARCH + SESSION CLOSE + AFTER COMPACTION protocol; health-check guard preserved; all 6 tools named.
- [x] 3.3 Rewrite `user-prompt-submit.sh` with 5-part per-prompt injection from design §6. No /tmp state file; two curl calls per prompt (recent + project search, ≤5s timeout each); output is single JSON {systemMessage}; empty results render (none); exits 0 when NEXUSMIND_API_KEY unset.

## Phase 4: Bash Hooks — `nexusmind-claude-plugin` (sync)

- [x] 4.1 Copy `subagent-stop.sh` from `nexusmind-mcp/plugin/scripts/` to `nexusmind-claude-plugin/plugin/scripts/`. diff -q → IDENTICAL.
- [x] 4.2 Copy `session-start.sh`. diff -q → IDENTICAL.
- [x] 4.3 Copy `user-prompt-submit.sh`. diff -q → IDENTICAL.

## Phase 5: CLAUDE.md Rewrite

- [x] 5.1 Replaced `apps/nexus-mind/CLAUDE.md` with canonical protocol document from design §5.
  - Contains "single source of truth" statement in opening section
  - PROACTIVE SAVE TRIGGERS section with 10 triggers + self-check directive
  - WHEN TO SEARCH section with first-message rule (search BEFORE responding)
  - All 13 type glossary entries with one-line definitions
  - topic_key guidance with USE / DO NOT use + 4 example keys
  - SESSION CLOSE (MANDATORY) with type: "session_summary" and 5 required content fields
  - AFTER COMPACTION 3-step protocol
  - Configuration section naming ${NEXUSMIND_API_KEY} and npx
  - Quick start section and demo keys preserved from original

## Phase 6: `.mcp.json` Portability

- [x] 6.1 Replaced `apps/nexus-mind/.mcp.json` with portable form from design §9.
  - `command: "npx"`, `args: ["-y", "@smart-coder-labs/nexusmind-mcp"]`
  - `NEXUSMIND_API_KEY: "${NEXUSMIND_API_KEY}"` (env placeholder, no literal key)
  - `NEXUSMIND_BASE_URL: "https://api.nexusmind.smartcoderlabs.com"` (literal production URL)
  - No absolute paths (`/Volumes`, `/Users`, `/home`) — verified
  - `grep -E 'nm_[a-f0-9]{16,}'` returns no match — verified

## Phase 7: Integration Smoke Test

- [x] 7.1 Build `nexusmind-mcp`: `npm run build` — exits 0, `dist/index.js` produced. Zero TypeScript errors.
- [x] 7.2 Confirm both `subagent-stop.sh` files are byte-identical — diff -q exit 0, no output.
- [x] 7.3 Confirm both `session-start.sh` files are byte-identical — diff -q exit 0, no output.
- [x] 7.4 Confirm both `user-prompt-submit.sh` files are byte-identical — diff -q exit 0, no output.
- [x] 7.5 Confirm `.mcp.json` has no secret — grep nm_[a-f0-9]{16,} returns no match.
- [x] 7.6 Spot-check tool count — all 6 tool names present in dist/index.js: store_memory, search_memory, list_memories, get_context, get_memory, delete_memory.

---

## Files changed

- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-mcp/src/client.ts` — added `getMemoryById`, `deleteMemory`
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-mcp/src/index.ts` — rewrote 4 descriptions, added 2 new tool registrations, updated imports
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-mcp/dist/` — rebuilt (clean)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-mcp/plugin/scripts/subagent-stop.sh` — keyword quality gate, min length 100, title field, aligned payload (Phase 3.1)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-mcp/plugin/scripts/session-start.sh` — project search + type labels + full protocol body (Phase 3.2)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-mcp/plugin/scripts/user-prompt-submit.sh` — 5-part per-prompt injection, no /tmp state (Phase 3.3)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-claude-plugin/plugin/scripts/subagent-stop.sh` — byte-identical copy (Phase 4.1)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-claude-plugin/plugin/scripts/session-start.sh` — byte-identical copy (Phase 4.2)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexusmind-claude-plugin/plugin/scripts/user-prompt-submit.sh` — byte-identical copy (Phase 4.3)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexus-mind/CLAUDE.md` — full rewrite to mandatory protocol document (Phase 5)
- `/Volumes/Realtek/work-environment/personal/smartcoder/apps/nexus-mind/.mcp.json` — switched to npx + env placeholder, removed hardcoded key and absolute path (Phase 6)
