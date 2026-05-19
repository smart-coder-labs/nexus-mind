# NexusMind — Claude Code Setup

This repo includes a NexusMind MCP server. Claude Code connects to it automatically via `.mcp.json`.

## Quick start

```bash
# 1. Start the backend (must run from apps/backend for correct DB path)
make backend   # or: cd apps/backend && cargo run

# 2. Seed demo data (first time)
./scripts/reset-demo.sh

# 3. Build the MCP server
cd apps/mcp && npm install && npm run build && cd ../..

# 4. Set your API key
export NEXUSMIND_API_KEY=nm_demo_acme_sarah

# 5. Open Claude Code — the MCP server connects automatically
```

## Available MCP tools

| Tool | Description |
|------|-------------|
| `store_memory` | Save a decision, convention, or finding for the team |
| `search_memory` | Full-text search across all team memories |
| `list_memories` | Browse recent memories, filter by project or tool |

## Demo keys (after reset-demo.sh)

| User | Key |
|------|-----|
| Acme Corp — admin | `nm_demo_acme_admin` |
| Acme Corp — Sarah Chen | `nm_demo_acme_sarah` |
| TechStartup — admin | `nm_demo_techstartup_admin` |

## Verify the connection

```bash
NEXUSMIND_API_KEY=nm_demo_acme_sarah \
  npx @modelcontextprotocol/inspector node apps/mcp/dist/index.js
```

Open the inspector URL → Tools tab → try `list_memories`.
