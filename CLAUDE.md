# NexusMind — Claude Code Setup

This repo includes a NexusMind MCP server. Claude Code connects to it automatically via `.mcp.json`.

## NexusMind Memory

Call these tools DIRECTLY — no ToolSearch needed. Use the full MCP tool name.

### `mcp__nexusmind__store_memory`
```
content   (required) Full memory content
title     (optional) Short searchable title
type      (optional) architecture | bugfix | decision | discovery | config | pattern | feedback | preference | project | session_summary | feature | refactoring | manual
topic_key (optional) Stable key to upsert (same key updates existing memory)
project   (optional) e.g. "nexusmind-backend", "nexusmind-admin", "nexusmind"
tool      (optional) defaults to "claude-code"
tags      (optional) array of strings
scope     (optional) "project" (default) | "personal"
```

### `mcp__nexusmind__search_memory`
```
query  (required) Search text
limit  (optional) Max results, default 10
```

### `mcp__nexusmind__list_memories`
```
project (optional) Filter by project
type    (optional) Filter by type
limit   (optional) Max results, default 20
```

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
| `mcp__nexusmind__store_memory` | Save a decision, bug fix, convention, discovery |
| `mcp__nexusmind__search_memory` | Look up past decisions or context |
| `mcp__nexusmind__list_memories` | Browse recent memories |

## Demo keys (after reset-demo.sh)

| User | Key |
|------|-----|
| Acme Corp — admin | `nm_demo_acme_admin` |
| Acme Corp — Sarah Chen | `nm_demo_acme_sarah` |
| TechStartup — admin | `nm_demo_techstartup_admin` |
