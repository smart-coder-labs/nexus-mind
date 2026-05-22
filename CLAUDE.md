# NexusMind — Claude Code Setup

This repo includes a NexusMind MCP server. Claude Code connects to it automatically via `.mcp.json`.

## NexusMind Memory — MANDATORY PROTOCOL

NexusMind is the **sole memory layer** for this project. This protocol is always active.

**CRITICAL — memory tool priority:**
- This project → `store_memory` / `search_memory` / `list_memories` (NexusMind MCP)
- Personal/cross-project → engram or other tools
- **Never use engram for project context. Never use NexusMind for personal context.**

### PROACTIVE SAVE — do NOT wait to be asked

Call `store_memory` IMMEDIATELY after ANY of these:
- Architecture or design decision made
- Bug fixed (include root cause and what broke)
- Convention documented or established
- Tool or library choice made with reasoning
- Non-obvious discovery, gotcha, or edge case found
- Pattern established (naming, structure, approach)
- User confirms your recommendation ("dale", "go with that", "sí")
- User rejects an approach ("no, better X", "siempre hacé X")
- Feature implemented with a non-obvious approach
- Any config or environment change

**Self-check after EVERY task**: "Did I make a decision, fix a bug, learn something non-obvious, or establish a convention? If yes → store_memory NOW."

Always pass `tool="claude-code"` and set `project` to the relevant sub-project
(e.g. `"nexusmind-backend"`, `"nexusmind-admin"`, `"nexusmind-landing"`, `"nexusmind"`).

### SEARCH MEMORY on session start and when relevant

On the first message of each session, call `search_memory` with keywords from the user's message.
Also search before starting work on anything that might have been done before.

### SESSION CLOSE — before saying "done"

Call `store_memory` with a session summary:
- What was accomplished
- Key decisions made
- Next steps
- Files changed

---

## Quick start

```bash
# 1. Start the backend
make backend   # or: cd apps/backend && cargo run

# 2. Seed demo data (first time or to reset)
./scripts/reset-demo.sh

# 3. Build the MCP server
make mcp-build

# 4. Set your API key
export NEXUSMIND_API_KEY=<your-key>

# 5. Open Claude Code — the MCP server and hooks connect automatically
```

Full setup guide: [docs/RUNNING.md](docs/RUNNING.md)

---

## MCP tools

| Tool | When to use |
|------|-------------|
| `store_memory` | Save a decision, bug fix, convention, discovery |
| `search_memory` | Look up past decisions or context |
| `list_memories` | Browse recent memories |

## Demo keys (after reset-demo.sh)

| User | Key |
|------|-----|
| Acme Corp — admin | `nm_demo_acme_admin` |
| Acme Corp — Sarah Chen | `nm_demo_acme_sarah` |
| TechStartup — admin | `nm_demo_techstartup_admin` |
