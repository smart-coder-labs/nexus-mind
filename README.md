# NexusMind

**Centralized memory control plane for AI-powered engineering teams.**

Every developer gets an API key. Their AI tools store decisions and context. You see everything from a single admin panel — with a full audit trail.

---

## What it does

- **Team memory**: Claude Code, Cursor, Copilot, and any MCP-compatible tool writes to a shared memory store
- **Multi-tenant**: Multiple orgs, isolated by API key — one deployment serves many teams
- **Admin panel**: Browse memories, manage users, inspect audit logs — no SQL required
- **Audit trail**: Every store, search, and delete is logged with user, tool, and timestamp
- **MCP-native**: Ships with a Model Context Protocol server — Claude Code connects in 30 seconds

---

## Quickstart (2 commands)

```bash
docker compose up -d
./scripts/reset-demo.sh   # seeds demo data, prints admin key
```

Open **http://localhost:3000** and log in with the key printed by the script.

Full setup guide: [docs/RUNNING.md](docs/RUNNING.md)

---

## Demo (Claude Code)

```bash
make mcp-build
export NEXUSMIND_API_KEY=nm_demo_acme_sarah
# Open Claude Code from this repo — .mcp.json is picked up automatically
```

Ask Claude: *"Store a memory: we use snake_case for all REST endpoints. Tag it as convention."*

Switch to the admin panel → Memory Browser — the entry appears instantly.

Full demo script: [demo/MCP_DEMO.md](demo/MCP_DEMO.md)

---

## Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust + Axum 0.7 + SQLite (rusqlite bundled) |
| Admin panel | React 19 + Vite 5 + Tailwind CSS v4 + TanStack Query v5 |
| MCP server | TypeScript + `@modelcontextprotocol/sdk` |
| Auth | API key per user, scoped to org |
| Deployment | Docker Compose (single-file, no external deps) |

---

## Architecture

```
Claude Code / Cursor / any MCP client
        │  stdio
        ▼
   MCP Server (apps/mcp)          API key auth
        │  HTTP
        ▼
   Backend REST API (apps/backend)
        │
        ▼
   SQLite (apps/backend/data/nexusmind.db)
        │
        ▼
   Admin Panel (apps/admin)       read-only queries + management
```

---

## Project layout

```
apps/
  backend/   # Rust + Axum API — multi-tenant, audit trail, health
  admin/     # React admin panel — users, memories, audit log, settings
  mcp/       # TypeScript MCP server — memory + policy tools
scripts/
  reset-demo.sh   # wipe + reseed demo data
  test-mcp.sh     # smoke test: store → search → delete
demo/
  MCP_DEMO.md     # step-by-step live demo script
docs/
  RUNNING.md      # full local + Docker setup guide
```

---

## MCP tools

| Tool | Description |
|------|-------------|
| `store_memory` | Save a decision, convention, finding, or any project context |
| `search_memory` | Semantic search across your team's memories |
| `list_memories` | Browse recent memories, filter by project or tool |
| `list_policies` | List governance policies (model whitelist, budget, PII redaction) |
| `create_policy` | Create a policy — `model_whitelist`, `budget_limit`, or `pii_redact` |
| `update_policy` | Update a policy's name, config, or enabled state (`rule_type` is immutable) |
| `delete_policy` | Delete a policy by id |
| `check_policy` | Evaluate active policies against a prospective request |

---

## Demo keys (after reset-demo.sh)

| Org | User | Role | Key |
|-----|------|------|-----|
| Acme Corp | Admin | admin | `nm_demo_acme_admin` |
| Acme Corp | Sarah Chen | member | `nm_demo_acme_sarah` |
| Acme Corp | Marcus Johnson | member | `nm_demo_acme_marcus` |
| TechStartup | Admin | admin | `nm_demo_techstartup_admin` |
| DevShop | Admin | admin | `nm_demo_devshop_admin` |

---

## Development

```bash
# Backend tests
make test

# Backend (dev server)
make backend

# Admin panel (dev server)
cd apps/admin && npm install && npm run dev

# MCP smoke test
NEXUSMIND_API_KEY=nm_demo_acme_sarah make mcp-test

# Full reset
make reset-demo
```

CI runs on every push — backend build + tests + clippy, admin build, MCP smoke test.

---

## License

MIT
