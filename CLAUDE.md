# NexusMind — Claude Code Setup

This repo includes a NexusMind MCP server. Claude Code connects to it automatically via `.mcp.json`.

## NexusMind Memory

Tools: `store_memory` · `search_memory` · `list_memories`  
Always pass `tool="claude-code"` and the relevant `project` (`"nexusmind-backend"`, `"nexusmind-admin"`, `"nexusmind"`, etc).

**Save** after any decision, bug fix, convention, or non-obvious discovery.  
**Search** before starting work on something that might have been done before.

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
