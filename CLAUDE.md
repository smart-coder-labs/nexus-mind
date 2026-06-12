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
| `architecture` | System design, component relationships, infrastructure decisions |
| `bugfix` | Root cause + fix. Always include WHAT broke and WHY |
| `decision` | Explicit choices made between alternatives |
| `discovery` | Non-obvious findings about code, APIs, or behavior |
| `pattern` | Established conventions — naming, structure, approach |
| `config` | Environment setup, tool config, credentials schema |
| `preference` | User working style, code style, tool preferences |
| `project` | Goals, stakeholders, milestones, business context |
| `session_summary` | End-of-session state — always save before saying "done" |
| `feature` | Implemented feature with approach and rationale |
| `refactoring` | What changed, why, what to watch for |
| `feedback` | Corrections and confirmations from the user |
| `manual` | Ad hoc save not fitting another type |

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

## Quick start

```bash
# 1. Start the backend
make backend   # or: cd apps/backend && cargo run

# 2. Seed demo data (first time or to reset)
./scripts/reset-demo.sh

# 3. Build the MCP server (only needed for local dev)
make mcp-build

# 4. Set your API key
export NEXUSMIND_API_KEY=<your-key>

# 5. Open Claude Code — the MCP server and hooks connect automatically
```

Full setup guide: [docs/RUNNING.md](docs/RUNNING.md)

---

## Demo keys (after reset-demo.sh)

| User | Key |
|------|-----|
| Acme Corp — admin | `nm_demo_acme_admin` |
| Acme Corp — Sarah Chen | `nm_demo_acme_sarah` |
| TechStartup — admin | `nm_demo_techstartup_admin` |

---

NexusMind is the single source of truth for this codebase. Before guessing, check it. Before finishing, save to it.
