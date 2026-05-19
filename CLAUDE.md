# NexusMind — Claude Code Setup

This repo includes a NexusMind MCP server. Claude Code connects to it automatically via `.mcp.json`.

## IMPORTANT: Which memory tool to use

This project uses **NexusMind** as the team memory layer. There may be other memory-related
MCP tools available in your Claude Code setup (e.g. engram, personal memory plugins).

**Use NexusMind tools for anything related to this project:**
- Decisions, conventions, architecture choices → `store_memory`
- Looking up past decisions or context → `search_memory`
- Browsing what the team has stored → `list_memories`

**Do NOT use other memory tools (engram, personal plugins, etc.) for project context.**
Those are for personal/cross-project memory. NexusMind is the source of truth for this repo.

When in doubt: if it's about this codebase, use NexusMind.

---

## Quick start

```bash
# 1. Start the backend (must run from apps/backend for correct DB path)
make backend   # or: cd apps/backend && cargo run

# 2. Seed demo data (first time or to reset)
./scripts/reset-demo.sh

# 3. Build the MCP server
make mcp-build

# 4. Set your API key
export NEXUSMIND_API_KEY=nm_demo_acme_sarah

# 5. Open Claude Code — the MCP server connects automatically
```

Full setup guide: [docs/RUNNING.md](docs/RUNNING.md)

---

## Available MCP tools

| Tool | When to use |
|------|-------------|
| `store_memory` | Save a decision, convention, finding, or any project context |
| `search_memory` | Look up what the team has decided or learned before |
| `list_memories` | Browse recent memories, filter by project or tool |

## Demo keys (after reset-demo.sh)

| User | Key |
|------|-----|
| Acme Corp — admin | `nm_demo_acme_admin` |
| Acme Corp — Sarah Chen | `nm_demo_acme_sarah` |
| TechStartup — admin | `nm_demo_techstartup_admin` |

## Verify the connection

```bash
# Interactive inspector — open the URL shown, use the Tools tab
NEXUSMIND_API_KEY=nm_demo_acme_sarah make mcp-inspect
```
