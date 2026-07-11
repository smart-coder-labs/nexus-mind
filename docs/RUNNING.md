# Running NexusMind locally

## Prerequisites

- Rust toolchain (1.85+) — `rustup update`
- Node.js 20+
- Docker + Docker Compose (optional, for containerized run)

---

## Option A — Local dev (recommended for development)

### 1. Seed demo data + start backend

```bash
# From repo root
./scripts/reset-demo.sh       # builds seed binary, wipes DB, inserts demo data
make backend                  # runs: cd apps/backend && cargo run
```

Verify: `curl -s http://localhost:8080/v1/health` → `{"status":"ok"}`

### 2. Start the admin panel

```bash
cd apps/admin && npm install && npm run dev
```

Open: http://localhost:5173  
Login key: `nm_demo_acme_admin`

### 3. Build and connect the MCP server (Claude Code)

```bash
make mcp-build                          # compiles apps/mcp/
export NEXUSMIND_API_KEY=nm_demo_acme_sarah
```

Open Claude Code from the repo root — `.mcp.json` is picked up automatically.  
Verify with `/mcp` in the Claude Code chat — `nexusmind` should appear with 3 tools.

---

## Option B — Docker Compose

```bash
make demo          # builds everything, seeds data, starts backend + admin
```

- Backend: http://localhost:8080
- Admin:   http://localhost:3000

> The MCP server still needs to run locally (it communicates with Claude Code via stdio).
> Run `make mcp-build` and set `NEXUSMIND_API_KEY` as above.

---

## Demo keys (after reset-demo.sh)

| Org | User | Role | Key |
|-----|------|------|-----|
| Acme Corp | Admin User | admin | `nm_demo_acme_admin` |
| Acme Corp | Sarah Chen | member | `nm_demo_acme_sarah` |
| Acme Corp | Marcus Johnson | member | `nm_demo_acme_marcus` |
| TechStartup | Admin User | admin | `nm_demo_techstartup_admin` |
| DevShop | Admin User | admin | `nm_demo_devshop_admin` |

---

## MCP smoke test

```bash
# Backend must be running + NEXUSMIND_API_KEY set
NEXUSMIND_API_KEY=nm_demo_acme_sarah make mcp-test
```

Expected:
```
==> Checking backend health...  OK
==> Storing a test memory...    Stored id: <uuid>
==> Searching for the memory... Found 1 result(s)
==> Cleaning up...              Deleted <uuid>
All MCP smoke tests passed.
```

## MCP interactive inspector

```bash
NEXUSMIND_API_KEY=nm_demo_acme_sarah make mcp-inspect
# Opens a browser — use the Tools tab to call store_memory / search_memory / list_memories
```

---

## SDD artifact import (one-shot backfill)

Backfills `openspec/changes/**` and the legacy `sdd/*` memories into the SDD
artifact store. Idempotent — a second run creates zero revisions, so it is safe to
re-run after every archive.

```bash
cargo run --bin import-sdd -- --db <path> --org-id <id> --project nexus-mind --root <repo-root>
# Add --dry-run first — it reports exactly what it would write, and writes nothing.
# --org-id may be omitted when the database holds a single org.
```

The legacy `sdd/*` memories are **tagged** `sdd-migrated`, never removed: whether to
retire them is your call, made after you can see the imported artifacts in the admin.

---

## Makefile reference

| Command | What it does |
|---------|-------------|
| `make backend` | Start backend from `apps/backend/` (correct DB path) |
| `make reset-demo` | Rebuild seed binary + wipe + reseed |
| `make mcp-build` | Install deps + compile MCP server |
| `make mcp-test` | Smoke test: store → search → delete |
| `make mcp-inspect` | Open MCP inspector in browser |
| `make dev` | Start everything via Docker Compose |
| `make test` | Run Rust backend tests |
| `make logs` | Follow Docker Compose logs |
| `make clean` | Stop and remove Docker containers |
