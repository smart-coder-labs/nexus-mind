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

Backfills two sources into the SDD artifact store:

- **the filesystem** — `openspec/changes/**`;
- **the legacy memories** — every memory whose `topic_key` starts with `sdd/`.

Idempotent — a second run creates zero revisions (the store de-duplicates by content
hash), so it is safe to re-run after every archive.

### The two sources live in two different places

This is the whole shape of the tool, and the reason it takes the flags it does:

| Source | Where it lives | How it is imported |
|--------|----------------|--------------------|
| `openspec/` tree | a **developer checkout** — the Fly.io image is a slim runtime with no checkout | pushed over the HTTP API |
| `sdd/*` memories | the **production database** — already there, no tree needed | written database-direct, inside the container |

Neither machine has both halves, so each half is run where its source is.

### 1. Filesystem → production, from a checkout

Run this from the repo root, against the remote backend. It never opens a database.

```bash
export NEXUSMIND_BASE_URL=https://api.nexusmind.dev
export NEXUSMIND_API_KEY=nm_live_…        # needs sdd:read, sdd:write and sdd:delete

cargo run --bin import-sdd -- \
  --api-url "$NEXUSMIND_BASE_URL" --api-key "$NEXUSMIND_API_KEY" \
  --skip-memories \
  --project nexus-mind --root . \
  --dry-run                                # drop --dry-run to actually write
```

`--api-url` / `--api-key` default to `NEXUSMIND_BASE_URL` / `NEXUSMIND_API_KEY`, so
with those exported the two flags can be omitted. The org is the API key's own — there
is no `--org-id` on this path. `sdd:delete` is needed because archiving an
`openspec/changes/archive/*` folder soft-archives its change.

### 2. Memories → the same database, inside the container

The memories are rows in the production database, so this half runs where the database
is. There is no openspec tree in the image, hence `--skip-filesystem`.

```bash
fly ssh console -a nexusmind-backend
/app/import-sdd --db /data/nexusmind.db --skip-filesystem --project nexus-mind --dry-run
# then again without --dry-run
```

Run this half **before** the filesystem half: the memory is the older record and lands
as revision 1, so the file — newer and reviewable — lands on top of it and wins the read.

### 3. All-in-one, against a local dev database

Both halves at once, both database-direct. Only works where the checkout and the
database sit on the same disk, which in practice means your laptop.

```bash
cargo run --bin import-sdd -- --db ./data/nexusmind.db --project nexus-mind --root . --dry-run
# --org-id may be omitted when the database holds a single org.
```

### Flags

| Flag | Meaning |
|------|---------|
| `--db <path>` | SQLite file. Required for the memory half and for a local all-in-one run. |
| `--api-url <url>` / `--api-key <key>` | Push the **filesystem** half over `PUT /v1/sdd/artifacts` instead of touching a database. Env: `NEXUSMIND_BASE_URL` / `NEXUSMIND_API_KEY`. |
| `--skip-filesystem` | Import the memories only. |
| `--skip-memories` | Walk `openspec/` only. |
| `--dry-run` | Report exactly what would be written; write nothing. |

Passing neither `--db` nor `--api-url`, or asking for the memory half without `--db`,
is refused up front with a message — the run does not start and die on its first write.

> Note: `--api-url` falls back to `NEXUSMIND_BASE_URL`. If that is exported in your shell,
> even a `--db` run sends its **filesystem** half to that backend rather than to the file.
> `unset NEXUSMIND_BASE_URL` before an all-in-one local import. The startup banner says
> which destination each half chose — read it.

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
