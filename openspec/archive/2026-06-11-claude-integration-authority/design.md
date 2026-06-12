# Design: Claude Integration Authority

## 0. Design north star

Every byte Claude reads from a NexusMind surface (tool description, CLAUDE.md, session-start additionalContext, user-prompt-submit systemMessage) MUST act like a **system-prompt instruction**, not a tutorial. We are not describing tools — we are commanding behavior. The competing baseline (Engram, ToolSearch defaults, generic MCP descriptions) is descriptive. NexusMind wins by being imperative.

Rules for every string we write in this change:

1. Lead with an imperative verb: `ALWAYS`, `CALL`, `BEFORE`, `DO NOT`, `MUST`.
2. Anchor to a concrete trigger ("immediately after ANY decision", "on the user's FIRST message").
3. Name the required parameters inline so Claude does not have to infer.
4. Stay under 60 words per tool description (system-prompt budget matters).
5. Use sentence case and code-style identifiers (`store_memory`) — Claude treats these as canonical names.

## 1. Component map

```
apps/nexusmind-mcp/
├── src/
│   ├── index.ts          [modify]  rewrite 4 descriptions; register get_memory + delete_memory
│   └── client.ts         [modify]  add getMemoryById, deleteMemory
└── plugin/scripts/
    ├── session-start.sh        [modify]  project search + type labels + full protocol
    ├── user-prompt-submit.sh   [modify]  5-part injection on every prompt
    └── subagent-stop.sh        [modify]  keyword quality gate

apps/nexusmind-claude-plugin/
└── plugin/scripts/
    ├── session-start.sh        [modify]  byte-identical to nexusmind-mcp version
    ├── user-prompt-submit.sh   [modify]  byte-identical
    └── subagent-stop.sh        [modify]  byte-identical

apps/nexus-mind/
├── CLAUDE.md             [modify]  full MANDATORY PROTOCOL doc
└── .mcp.json             [modify]  env placeholder + npx
```

Two-repo sync rule: the three plugin scripts MUST be byte-identical between `nexusmind-mcp/plugin/scripts/` and `nexusmind-claude-plugin/plugin/scripts/`. Drift = bug. Verify-phase enforces this with `diff -q`.

## 2. Exact new MCP tool descriptions (`src/index.ts`)

These strings are the second argument to `server.tool(...)`. They go directly into Claude's system prompt.

### `store_memory`

```
ALWAYS call immediately after ANY decision, bug fix, convention, or non-obvious discovery — do NOT wait to be asked. Mandatory in practice: title (verb + what), type (architecture | bugfix | decision | discovery | config | pattern | feedback | preference | session_summary | feature | refactoring), and project. Call this BEFORE moving to the next task.
```

### `search_memory`

```
Call BEFORE starting any work that might have been done before. This is your FIRST action when a user's message references a project, feature, bug, or module you don't already have context on. If unsure whether to search — search. Pass keywords from the user's message as query.
```

### `list_memories`

```
Utility browse for recent memories, optionally filtered by project, type, or scope. Prefer search_memory when you have keywords. Use list_memories only when exploring the project or auditing recent activity.
```

### `get_context`

```
Call at the START of every session that involves significant work. Returns all team knowledge grouped by type — architecture, decisions, patterns, bugs fixed, discoveries. This is the canonical bootstrap for nexus-mind work; do not skip it on substantial sessions.
```

### `get_memory` (NEW)

```
Fetch FULL untruncated content for a single memory by id. search_memory and list_memories return previews (often 120-300 chars); when you need to act on or quote the full record, call get_memory(id). Use the id returned by search_memory.
```

### `delete_memory` (NEW)

```
Delete a memory permanently. The USER must request deletion explicitly — DO NOT delete autonomously. Required: confirm: true. Without confirm: true this tool refuses and returns an error. Backend hard-deletes; there is no undo. Use only for stale, incorrect, or explicitly retired memories.
```

## 3. Exact `src/index.ts` changes

### 3.1 Add types-aware imports

Replace the existing import line:

```ts
import { storeMemory, searchMemories, listMemories } from './client.js'
```

with:

```ts
import { storeMemory, searchMemories, listMemories, getMemoryById, deleteMemory } from './client.js'
```

### 3.2 Replace the 4 existing tool description strings

Find each `server.tool(name, "<old description>", ...)` call and replace the second argument with the exact strings from Section 2. The Zod schemas and handler bodies stay unchanged. Only the description string changes for these four tools.

### 3.3 Add `get_memory` tool

Insert after the `get_context` registration:

```ts
// get_memory — full untruncated content by id
server.tool(
  'get_memory',
  'Fetch FULL untruncated content for a single memory by id. search_memory and list_memories return previews (often 120-300 chars); when you need to act on or quote the full record, call get_memory(id). Use the id returned by search_memory.',
  {
    id: z.string().describe('The memory id (returned by search_memory or list_memories)'),
  },
  async ({ id }) => {
    try {
      const m = await getMemoryById(id)
      const date = new Date(m.created_at).toLocaleString()
      const tagsLine = m.tags.length > 0 ? `Tags: ${m.tags.join(', ')}\n` : ''
      const topicLine = m.topic_key ? `Topic key: ${m.topic_key}\n` : ''
      const text = [
        `id: ${m.id}`,
        `title: ${m.title ?? '(untitled)'}`,
        `type: ${m.type ?? '(none)'}`,
        `project: ${m.project || '(no project)'}`,
        `tool: ${m.tool}`,
        `scope: ${m.scope}`,
        `${tagsLine}${topicLine}created: ${date}`,
        `revision: ${m.revision_count}`,
        '',
        '--- content ---',
        m.content,
      ].join('\n')
      return { content: [{ type: 'text', text }] }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)
```

### 3.4 Add `delete_memory` tool

Insert immediately after `get_memory`:

```ts
// delete_memory — hard delete with explicit confirmation
server.tool(
  'delete_memory',
  'Delete a memory permanently. The USER must request deletion explicitly — DO NOT delete autonomously. Required: confirm: true. Without confirm: true this tool refuses and returns an error. Backend hard-deletes; there is no undo. Use only for stale, incorrect, or explicitly retired memories.',
  {
    id:      z.string().describe('The memory id to delete'),
    confirm: z.boolean().describe('Must be true to perform the deletion. Without this, the tool refuses.'),
  },
  async ({ id, confirm }) => {
    if (confirm !== true) {
      return {
        content: [{
          type: 'text',
          text: 'Refused: delete_memory requires confirm: true. The user must request deletion explicitly. No HTTP request was made.',
        }],
        isError: true,
      }
    }
    try {
      await deleteMemory(id)
      return {
        content: [{ type: 'text', text: `Memory deleted (id: ${id})` }],
      }
    } catch (err) {
      return {
        content: [{ type: 'text', text: `Error: ${(err as Error).message}` }],
        isError: true,
      }
    }
  }
)
```

## 4. Exact `src/client.ts` additions

Append the two new exported functions after `listMemories`. Reuse the existing `request<T>()` helper — no new error handling needed (401, 404, network failure, etc. are already covered).

```ts
export function getMemoryById(id: string): Promise<Memory> {
  return request<Memory>(`/v1/memory/${encodeURIComponent(id)}`, { method: 'GET' })
}

export function deleteMemory(id: string): Promise<void> {
  return request<void>(`/v1/memory/${encodeURIComponent(id)}`, { method: 'DELETE' })
}
```

Notes:
- `request<void>` works because the helper returns `undefined as T` on 204 responses (existing branch at line 45).
- `encodeURIComponent` defends against ids that contain `/`, `?`, or `#` (current ids are UUIDs but the backend is not contractually scoped to UUIDs).

## 5. Exact new `nexus-mind/CLAUDE.md`

Full replacement (the current 76-line file becomes this single canonical protocol doc):

````markdown
# NexusMind — Claude Code MANDATORY PROTOCOL

NexusMind is the single source of truth for this codebase. Before guessing, check it. Before finishing, save to it.

This protocol is MANDATORY and ALWAYS ACTIVE in this repository. It is not a suggestion. If you skip it, the next session starts blind.

## In this repository

- Default `project` for every call: `nexus-mind`
- Alternates for cross-repo work: `nexusmind-backend`, `nexusmind-admin`, `nexusmind-mcp`, `nexusmind-landing`
- Default `tool`: `claude-code`

## Available MCP tools

Call these DIRECTLY by full name — no ToolSearch.

| Tool | When to use |
|------|-------------|
| `mcp__nexusmind__store_memory` | Save a decision, bug fix, convention, discovery — PROACTIVELY |
| `mcp__nexusmind__search_memory` | First action on any prompt that references prior work |
| `mcp__nexusmind__list_memories` | Browse recent memories (utility) |
| `mcp__nexusmind__get_context` | Bootstrap a significant session — all team knowledge grouped by type |
| `mcp__nexusmind__get_memory` | Fetch FULL untruncated content by id (previews are not enough) |
| `mcp__nexusmind__delete_memory` | Remove a memory — ONLY when the user explicitly asks |

## PROACTIVE SAVE TRIGGERS

Call `store_memory` IMMEDIATELY and WITHOUT BEING ASKED after any of these:

- Architecture or design decision made
- Convention documented or established
- Bug fix completed (include root cause)
- Feature implemented with non-obvious approach
- Tool or library choice made with tradeoffs
- Configuration or environment change
- Non-obvious discovery about the codebase
- Gotcha, edge case, or unexpected behavior
- Pattern established (naming, structure)
- User preference or constraint learned

Self-check after EVERY task: "Did I make a decision, fix a bug, learn something non-obvious, or establish a convention? If yes, call store_memory NOW."

## Required fields on every `store_memory` call

- `title` — verb + what, 5-10 words ("Fixed N+1 in memory listing")
- `type` — pick from the glossary below
- `project` — always `nexus-mind` in this repo
- `content` — structured: **What**, **Why**, **Where**, **Learned**
- `topic_key` (recommended for evolving topics) — see topic_key section

## Type glossary

| Type | Use for |
|------|---------|
| `architecture` | System structure, layering, boundary decisions |
| `bugfix` | A bug that was diagnosed and fixed (include root cause) |
| `decision` | An explicit choice between alternatives (with the tradeoff) |
| `discovery` | A non-obvious finding about the codebase, env, or library |
| `config` | Environment, tooling, infra, or runtime configuration change |
| `pattern` | Naming, structure, or code pattern established as a convention |
| `feedback` | User correction or confirmation of your approach |
| `preference` | User style or workflow preference (cross-task) |
| `project` | Project-level facts (stack, owners, scope) |
| `session_summary` | End-of-session summary (mandatory before "done") |
| `feature` | A completed feature implementation worth recalling |
| `refactoring` | Structural code change without behavior change |
| `manual` | Catch-all when no type fits |

## topic_key guidance

USE `topic_key` for topics that EVOLVE — saving again with the same key UPDATES the existing memory instead of creating a duplicate.

- Examples: `architecture/auth-model`, `config/deploy-pipeline`, `pattern/repo-naming`, `convention/commit-style`
- Use it when you expect to revise the same decision later
- DO NOT use it for one-shot records (a single bug fix, a single session summary)

If unsure of the right key, search first — if a similar topic exists, reuse its key.

## WHEN TO SEARCH

Call `search_memory` PROACTIVELY when:

- The user's FIRST message of a session references a project, feature, bug, or module → search BEFORE responding
- Starting work on something that might have been done before
- The user uses words like "remember", "recall", "we did", "how did we" — search every time
- You are about to make a non-trivial decision — check whether one already exists

If unsure whether to search — search.

## SESSION CLOSE (MANDATORY)

Before saying "done", "that's it", "finished" (or the equivalent in any language), call:

```
store_memory({
  type: "session_summary",
  title: "Session: <one-line>",
  project: "nexus-mind",
  content: """
  ## Goal
  <what we were working on>

  ## Accomplished
  - <completed items with key details>

  ## Discoveries
  - <technical findings, gotchas>

  ## Next Steps
  - <what remains>

  ## Relevant Files
  - path/to/file — <what changed>
  """
})
```

This is NOT optional. If you skip this, the next session starts blind.

## AFTER COMPACTION

If you see a compaction message or "FIRST ACTION REQUIRED":

1. IMMEDIATELY call `store_memory` with `type: "session_summary"` and the compacted summary content — this persists what was done before compaction.
2. Call `search_memory(query: "nexus-mind")` to recover broader context.
3. Only THEN continue working.

Do not skip step 1. Without it, everything done before compaction is lost from memory.

## Configuration

The repo's `.mcp.json` uses an environment placeholder `${NEXUSMIND_API_KEY}` — set this in your shell before launching Claude Code:

```bash
export NEXUSMIND_API_KEY=<your-key>
```

The MCP server is launched via `npx -y @smart-coder-labs/nexusmind-mcp` — no local checkout required.

---

NexusMind is the single source of truth for this codebase. Before guessing, check it. Before finishing, save to it.
````

## 6. Exact new `user-prompt-submit.sh` (both repos, byte-identical)

```bash
#!/usr/bin/env bash
# user-prompt-submit.sh — NexusMind Claude Code plugin: UserPromptSubmit hook
# Emits a 5-part system message on EVERY prompt with session + project memories
# and a behavioral mandate. No first-call / periodic gating.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_helpers.sh
source "${SCRIPT_DIR}/_helpers.sh"

# Parse stdin
INPUT="$(cat)"
cwd="$(echo "$INPUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('cwd',''))" 2>/dev/null || true)"

# Guard
if [[ -z "${NEXUSMIND_API_KEY:-}" ]]; then
  exit 0
fi

NEXUSMIND_BASE_URL="${NEXUSMIND_BASE_URL:-https://nexusmind-backend.fly.dev}"

# Project detection
if [[ -n "$cwd" ]]; then
  pushd "$cwd" &>/dev/null || true
fi
PROJECT="$(detect_project)"
if [[ -n "$cwd" ]]; then
  popd &>/dev/null || true
fi

# Section 1: last 5 recent memories
RECENT_BLOCK="(none)"
RECENT_JSON="$(curl -sf --max-time 5 \
  -H "Authorization: Bearer ${NEXUSMIND_API_KEY}" \
  "${NEXUSMIND_BASE_URL}/v1/memory?limit=5" 2>/dev/null || true)"
if [[ -n "$RECENT_JSON" ]]; then
  PARSED="$(echo "$RECENT_JSON" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    items = data if isinstance(data, list) else data.get('memories', data.get('items', data.get('data', [])))
    lines = []
    for m in items[:5]:
        t = m.get('type') or 'general'
        title = m.get('title') or (m.get('content','').split('\n')[0][:120])
        lines.append(f'- [{t}] {title}')
    print('\n'.join(lines))
except Exception:
    pass
" 2>/dev/null || true)"
  if [[ -n "$PARSED" ]]; then RECENT_BLOCK="$PARSED"; fi
fi

# Section 2: last 5 project-specific memories (via search)
PROJECT_BLOCK="(none)"
PROJECT_JSON="$(curl -sf --max-time 5 \
  -X POST \
  -H "Authorization: Bearer ${NEXUSMIND_API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{\"query\": \"${PROJECT}\", \"limit\": 5}" \
  "${NEXUSMIND_BASE_URL}/v1/memory/search" 2>/dev/null || true)"
if [[ -n "$PROJECT_JSON" ]]; then
  PARSED="$(echo "$PROJECT_JSON" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    items = data if isinstance(data, list) else data.get('memories', data.get('items', data.get('data', [])))
    lines = []
    for m in items[:5]:
        t = m.get('type') or 'general'
        title = m.get('title') or (m.get('content','').split('\n')[0][:120])
        lines.append(f'- [{t}] {title}')
    print('\n'.join(lines))
except Exception:
    pass
" 2>/dev/null || true)"
  if [[ -n "$PARSED" ]]; then PROJECT_BLOCK="$PARSED"; fi
fi

# Build the 5-part message
MESSAGE="$(cat <<EOF
## NexusMind — Per-Prompt Protocol (project: ${PROJECT})

### 1) Recent session memories
\`\`\`nexusmind-recent
${RECENT_BLOCK}
\`\`\`

### 2) Project-specific memories — ${PROJECT}
\`\`\`nexusmind-project
${PROJECT_BLOCK}
\`\`\`

### 3) MANDATORY behavioral rule
MANDATORY: call \`search_memory\` with keywords from this message before responding if the message references existing work. Save any decision you make to NexusMind. Do not skip this.

### 4) Save reminder
After completing any decision, bug fix, or non-obvious discovery, call \`store_memory\` BEFORE moving on.

### 5) Format hint
When you call \`store_memory\`, always set \`type\`, always set \`title\`, always set \`project\`.
EOF
)"

# Emit as a single JSON object via stdout
python3 -c "
import json, sys
print(json.dumps({'systemMessage': sys.argv[1]}))
" "$MESSAGE"
```

Behavior change vs current:
- No state file in `/tmp` (no first-call vs periodic gating).
- Two curl calls per prompt (≤ 5s timeout each, total budget ≤ 10s).
- Output is a single JSON object with one `systemMessage` field.
- Empty results render as `(none)` so the structure is preserved.

## 7. Exact new `session-start.sh` (both repos, byte-identical)

```bash
#!/usr/bin/env bash
# session-start.sh — NexusMind Claude Code plugin: SessionStart hook
# Emits additionalContext with project search + recency + full protocol body.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_helpers.sh
source "${SCRIPT_DIR}/_helpers.sh"

INPUT="$(cat)"
cwd="$(echo "$INPUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('cwd',''))" 2>/dev/null || true)"

NEXUSMIND_BASE_URL="${NEXUSMIND_BASE_URL:-https://nexusmind-backend.fly.dev}"

# Guard: API key
if [[ -z "${NEXUSMIND_API_KEY:-}" ]]; then
  cat <<'EOF'
<!-- NexusMind NOT CONNECTED: NEXUSMIND_API_KEY is not set. Memory tools will not be available.
     Run: export NEXUSMIND_API_KEY=<your-key>
     Then restart Claude Code. -->
EOF
  exit 0
fi

# Guard: backend health
if ! curl -sf --max-time 5 "${NEXUSMIND_BASE_URL}/v1/health" &>/dev/null; then
  cat <<'EOF'
<!-- NexusMind NOT CONNECTED: backend is unreachable. Memory tools will not be available.
     Check NEXUSMIND_BASE_URL or your network connection. -->
EOF
  exit 0
fi

# Project detection
if [[ -n "$cwd" ]]; then
  pushd "$cwd" &>/dev/null || true
fi
PROJECT="$(detect_project)"
if [[ -n "$cwd" ]]; then
  popd &>/dev/null || true
fi

format_memories() {
  python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    items = data if isinstance(data, list) else data.get('memories', data.get('items', data.get('data', [])))
    lines = []
    for m in items[:int(sys.argv[1])]:
        t = (m.get('type') or 'general')
        title = m.get('title') or (m.get('content','').split('\n')[0][:120].replace('\n',' '))
        lines.append(f'- [{t}] {title}')
    print('\n'.join(lines))
except Exception:
    pass
" "$1"
}

# Project-specific search
PROJECT_BLOCK=""
PROJECT_JSON="$(curl -sf --max-time 8 \
  -X POST \
  -H "Authorization: Bearer ${NEXUSMIND_API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{\"query\": \"${PROJECT}\", \"limit\": 15}" \
  "${NEXUSMIND_BASE_URL}/v1/memory/search" 2>/dev/null || true)"
if [[ -n "$PROJECT_JSON" ]]; then
  PROJECT_BLOCK="$(echo "$PROJECT_JSON" | format_memories 15)"
fi

# Recency list
RECENT_BLOCK=""
RECENT_JSON="$(curl -sf --max-time 8 \
  -H "Authorization: Bearer ${NEXUSMIND_API_KEY}" \
  "${NEXUSMIND_BASE_URL}/v1/memory?limit=15" 2>/dev/null || true)"
if [[ -n "$RECENT_JSON" ]]; then
  RECENT_BLOCK="$(echo "$RECENT_JSON" | format_memories 10)"
fi

# Full protocol body
cat <<PROTOCOL
## NexusMind — ACTIVE PROTOCOL (project: ${PROJECT})

NexusMind is the single source of truth for this codebase. Before guessing, check it. Before finishing, save to it.

### Tools available
store_memory — save decisions, bugs, discoveries, conventions PROACTIVELY (do not wait to be asked)
search_memory — first action on any prompt that references prior work
list_memories — utility browse
get_context — bootstrap a significant session
get_memory — full untruncated content by id (previews are not enough)
delete_memory — only when the user explicitly asks; requires confirm: true

### PROACTIVE SAVE RULE
Call store_memory IMMEDIATELY after ANY decision, bug fix, discovery, or convention — not just when asked.
Always pass tool="claude-code" and project="${PROJECT}".

ALWAYS set \`type\` — pick the closest match:
- architecture — design decisions, patterns, system structure
- bugfix — bug fixes (include root cause)
- decision — explicit choices made (library, approach, tradeoff)
- discovery — non-obvious findings, gotchas, edge cases
- config — environment, tooling, infrastructure changes
- pattern — naming conventions, code patterns, team standards
- feedback — user corrections or confirmations of your approach
- preference — user style or workflow preferences
- session_summary — end-of-session summary
- feature — completed feature implementations
- refactoring — structural code changes without behavior change

ALWAYS provide \`title\` — short (5-10 word) searchable title.
Use \`topic_key\` for evolving topics — same key updates existing memory instead of creating a duplicate.

### WHEN TO SEARCH
- User's FIRST message references a feature or problem → search_memory with keywords BEFORE responding
- Starting work on something that might have been done before → search_memory
- User asks to recall anything → search_memory
- About to make a non-trivial decision → search_memory first

### SESSION CLOSE (MANDATORY)
Before saying "done", call store_memory with type="session_summary":
- What was accomplished
- Key decisions and why
- Files changed
- Next steps

This is NOT optional. If you skip this, the next session starts blind.

### AFTER COMPACTION
1. IMMEDIATELY call store_memory with type="session_summary" and the compacted content.
2. Call search_memory(query: "${PROJECT}") to recover broader context.
3. Only THEN continue working.
PROTOCOL

if [[ -n "$PROJECT_BLOCK" ]]; then
  cat <<EOF

### Project Memories — ${PROJECT}
${PROJECT_BLOCK}
EOF
fi

if [[ -n "$RECENT_BLOCK" ]]; then
  cat <<EOF

### Recent Team Memories (last 10)
${RECENT_BLOCK}
EOF
fi
```

## 8. Exact new `subagent-stop.sh` (both repos, byte-identical)

```bash
#!/usr/bin/env bash
# subagent-stop.sh — NexusMind Claude Code plugin: SubagentStop hook (async)
# Quality-gated passive capture: only stores outputs that contain decision-like
# keywords. Both Claude plugin repos ship this file byte-identical.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_helpers.sh
source "${SCRIPT_DIR}/_helpers.sh"

if [[ -z "${NEXUSMIND_API_KEY:-}" ]]; then
  exit 0
fi

INPUT="$(cat)"
subagent_output="$(echo "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('stdout', ''))
except Exception:
    pass
" 2>/dev/null || true)"

# Skip very short outputs
if [[ -z "$subagent_output" || "${#subagent_output}" -lt 50 ]]; then
  exit 0
fi

# Quality gate: must contain at least one decision-like keyword
KEYWORD_RE='decided|decision|fixed|error|warning|convention|architecture|discovered|discovery|issue|solution|bug|gotcha|pattern'
if ! echo "$subagent_output" | grep -iEq "$KEYWORD_RE"; then
  exit 0
fi

NEXUSMIND_BASE_URL="${NEXUSMIND_BASE_URL:-https://nexusmind-backend.fly.dev}"
PROJECT="$(detect_project)"

PAYLOAD="$(python3 -c "
import json, sys
content = sys.argv[1]
project = sys.argv[2]
if len(content) > 2000:
    content = content[:2000] + '... [truncated]'
print(json.dumps({
    'content': content,
    'type': 'discovery',
    'tool': 'claude-code-subagent',
    'project': project,
}))
" "$subagent_output" "$PROJECT" 2>/dev/null || true)"

if [[ -n "$PAYLOAD" ]]; then
  curl -sf --max-time 10 \
    -X POST \
    -H "Authorization: Bearer ${NEXUSMIND_API_KEY}" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD" \
    "${NEXUSMIND_BASE_URL}/v1/memory/store" &>/dev/null || true
fi

exit 0
```

## 9. Exact new `nexus-mind/.mcp.json`

```json
{
  "mcpServers": {
    "nexusmind": {
      "command": "npx",
      "args": ["-y", "@smart-coder-labs/nexusmind-mcp"],
      "env": {
        "NEXUSMIND_API_KEY": "${NEXUSMIND_API_KEY}",
        "NEXUSMIND_BASE_URL": "https://api.nexusmind.smartcoderlabs.com"
      }
    }
  }
}
```

Rationale:
- `npx -y` resolves the latest published version. Acceptable startup latency on cold cache; instant on warm cache.
- `${NEXUSMIND_API_KEY}` is expanded by Claude Code at MCP-server-launch time. Users set the env var in their shell or `.envrc`.
- `NEXUSMIND_BASE_URL` stays a literal because there is one production URL; users who run a local backend override it in their shell.

## 10. Cross-cutting decisions

### Why "rewrite descriptions" instead of "add a new tool with mandate"

System-prompt budget is finite. We already pay the cost of registering 4 tools — using their description slots for behavior is free real estate. Adding a separate `please_read_this` tool that nobody calls is wasted budget.

### Why 5-part injection on every prompt (not first + periodic)

Claude reads system prompts the way humans read room signs: it sees them once and then ignores them. By refreshing on every prompt we keep the most relevant 10 memories (5 recent + 5 project) visible at all times. The cost is ~3 KB of context per turn — acceptable, and the recent memories are themselves valuable context.

### Why a single keyword list instead of a per-type classifier

A regex `grep -iEq` is 100 µs and fails open (no false ALLOWS — only false REJECTS). A classifier would add an HTTP call to a model. We can iterate the keyword list later; we cannot iterate Claude's behavior under load.

### Why two repos ship byte-identical scripts

`nexusmind-mcp` ships the MCP server itself and bundles plugin scripts. `nexusmind-claude-plugin` is a thin plugin that wraps the MCP server. Today the scripts diverge in ways that look intentional but aren't (e.g., `subagent-stop.sh` payload shape). Aligning them via `diff -q` is the cheapest enforcement we have.

### Why `delete_memory` requires `confirm: true` and not a typed phrase

Claude does not have a typing reflex — it will pass `confirm: "yes"` or `confirm: "DELETE"` happily. A boolean is the only signal the SDK can mechanically refuse on. The tool description carries the social rule ("USER must request deletion explicitly").

### What is NOT covered by this design

- Programmatic test of "Claude actually called `search_memory` on the first prompt" — that requires a Claude-Code-side eval harness which we do not have. The spec scenarios test the surfaces, not Claude's compliance.
- Migration of existing committed `.mcp.json` files on user machines. Users update on next pull.
- Renaming of any tool. `store_memory` stays `store_memory` to preserve all existing call sites.

## 11. Implementation order

1. `client.ts` additions (smallest, no downstream coupling).
2. `index.ts` description rewrites + new tool registrations (depends on 1).
3. `subagent-stop.sh` aligned in `nexusmind-mcp`, then `cp` to `nexusmind-claude-plugin`.
4. `session-start.sh` aligned in `nexusmind-mcp`, then `cp` to `nexusmind-claude-plugin`.
5. `user-prompt-submit.sh` aligned in `nexusmind-mcp`, then `cp` to `nexusmind-claude-plugin`.
6. `nexus-mind/.mcp.json` swap (after 1+2 are published, otherwise `npx` cannot pull the new tools).
7. `nexus-mind/CLAUDE.md` rewrite (last — references the new tools).

Steps 3, 4, 5 are independent and can be done in parallel. Step 6 requires publishing `@smart-coder-labs/nexusmind-mcp` at the new version after steps 1+2. Until publish, `nexus-mind/.mcp.json` can remain on absolute path with the env-placeholder API key as an intermediate state.
