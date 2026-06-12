# Archive Report: claude-integration-authority

**Status**: VERIFIED
**Archived**: 2026-06-11
**Verdict**: PASS — 0 CRITICAL, 0 WARNING, 0 SUGGESTION

---

## What Was Implemented

### 1. MCP Tool Descriptions — 4 Behavioral Mandates
All four existing tool descriptions in `nexusmind-mcp/src/index.ts` were rewritten as authoritative behavioral mandates:
- `store_memory`: "ALWAYS call immediately after ANY decision…do NOT wait to be asked"
- `search_memory`: "FIRST action when a user's message references a project/feature/bug…if unsure whether to search — search"
- `get_context`: "Call at the START of every session that involves significant work"
- `list_memories`: Utility framing only — no mandatory keywords, references `search_memory` as preferred

### 2. Two New MCP Tools
- `get_memory(id)`: Returns full untruncated memory record by ID. Calls `GET /v1/memory/:id`. All fields rendered (title, type, project, scope, tags, topic_key, created_at, revision_count). Error path returns `isError: true`.
- `delete_memory(id, confirm)`: Hard delete with explicit confirmation gate. `confirm !== true` → `isError: true` with no HTTP call. Only triggers `DELETE /v1/memory/:id` when `confirm === true`. Description mandates user must request explicitly.

### 3. `client.ts` — Two New Functions
- `getMemoryById(id)`: `GET /v1/memory/${encodeURIComponent(id)}` → `Promise<Memory>`
- `deleteMemory(id)`: `DELETE /v1/memory/${encodeURIComponent(id)}` → `Promise<void>` (204 branch already handled)

### 4. `CLAUDE.md` — Full Mandatory Protocol Document
Complete rewrite of `nexus-mind/CLAUDE.md` as a self-contained protocol:
- "NexusMind is the single source of truth" in opening and footer
- PROACTIVE SAVE TRIGGERS: 10 conditions + self-check directive
- WHEN TO SEARCH: first-message rule ("search BEFORE responding")
- Type glossary: all 13 types with one-line definitions
- topic_key guidance: USE/DO NOT use + 4 example keys
- SESSION CLOSE (MANDATORY): `type: "session_summary"` + 5 required fields
- AFTER COMPACTION: 3-step ordered protocol
- Configuration section: `${NEXUSMIND_API_KEY}` env placeholder + npx documented
- Project defaults: `nexus-mind` + 4 per-app alternates

### 5. `user-prompt-submit.sh` — Rich 5-Part Per-Prompt Injection
Replaces the old first-call-only + 15-minute periodic gate. Every prompt emits:
1. Last 5 recent memories (`nexusmind-recent` fenced block)
2. Last 5 project-specific memories via search (`nexusmind-project` fenced block)
3. MANDATORY behavioral mandate (literal text)
4. Save reminder
5. Format hint (`type`, `title`, `project` always required)
No /tmp state file. Stateless per execution. Empty results → `(none)` placeholder. No API key → clean exit 0.

### 6. `session-start.sh` — Project-Specific Search + Full Protocol Body
- `POST /v1/memory/search` with `query=$PROJECT, limit=15` for project-specific memories
- `format_memories` helper: `- [type] title` format, 120-char truncation
- Project Memories section first, Recent Team Memories section second
- Full PROACTIVE SAVE, WHEN TO SEARCH, SESSION CLOSE, AFTER COMPACTION protocol in heredoc
- All 6 tools named in tools list
- "IMMEDIATELY" in proactive save rule
- Health-check guard preserved; project search failure does not block protocol emission

### 7. `subagent-stop.sh` — Quality Gate
- Minimum output length: 100 chars
- Keyword regex: 21 terms (superset of spec's 14: decided, decision, fixed, error, warning, convention, architecture, discovered, discovery, issue, solution, implemented, changed, added, removed, refactored, pattern, config, gotcha, caveat, note, important)
- Payload shape: `type: "discovery"`, `tool: "claude-code-subagent"`, `project`, `content` (max 2000 chars)
- `metadata.passive_capture` removed (old divergence eliminated)
- `title: "Subagent: {project}"` added

### 8. Both Repos Synced — Byte-Identical
All three hook scripts (`subagent-stop.sh`, `session-start.sh`, `user-prompt-submit.sh`) are byte-identical between:
- `nexusmind-mcp/plugin/scripts/`
- `nexusmind-claude-plugin/plugin/scripts/`

Confirmed via `diff -q` → empty output, exit 0.

### 9. `.mcp.json` Portability
- `command: "npx"`, `args: ["-y", "@smart-coder-labs/nexusmind-mcp"]`
- API key: `${NEXUSMIND_API_KEY}` env placeholder — no literal `nm_*` value
- No absolute filesystem paths
- `NEXUSMIND_BASE_URL` hardcoded to production URL (not a machine-local path)

---

## Build Evidence

- `npm run build` (nexusmind-mcp): Exit 0, zero TypeScript errors
- All 6 tool names present in `dist/index.js`: `store_memory`, `search_memory`, `list_memories`, `get_context`, `get_memory`, `delete_memory`
- `grep -E 'nm_[a-f0-9]{16,}' .mcp.json`: 0 matches

---

## Files Changed

| File | Change |
|------|--------|
| `nexusmind-mcp/src/client.ts` | Added `getMemoryById`, `deleteMemory` |
| `nexusmind-mcp/src/index.ts` | Rewrote 4 tool descriptions, added `get_memory` + `delete_memory` tools, updated imports |
| `nexusmind-mcp/dist/` | Rebuilt clean |
| `nexusmind-mcp/plugin/scripts/subagent-stop.sh` | Keyword quality gate, aligned payload |
| `nexusmind-mcp/plugin/scripts/session-start.sh` | Project search, type labels, full protocol body |
| `nexusmind-mcp/plugin/scripts/user-prompt-submit.sh` | 5-part per-prompt injection, no /tmp state |
| `nexusmind-claude-plugin/plugin/scripts/subagent-stop.sh` | Byte-identical copy |
| `nexusmind-claude-plugin/plugin/scripts/session-start.sh` | Byte-identical copy |
| `nexusmind-claude-plugin/plugin/scripts/user-prompt-submit.sh` | Byte-identical copy |
| `nexus-mind/CLAUDE.md` | Full rewrite to mandatory protocol document |
| `nexus-mind/.mcp.json` | npx + env placeholder, no hardcoded key or absolute path |
