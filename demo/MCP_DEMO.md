# NexusMind + Claude Code — Live Demo

**Duration**: ~5 minutes  
**What it shows**: Claude Code storing and retrieving team memories in real time

---

## Setup (before the demo)

```bash
# Terminal 1 — start backend
cargo run --manifest-path apps/backend/Cargo.toml

# Terminal 2 — seed + build MCP
./scripts/reset-demo.sh
make mcp-build

# Terminal 3 — set key for Sarah Chen (member of Acme Corp)
export NEXUSMIND_API_KEY=nm_demo_acme_sarah
```

Open the admin panel at `http://localhost:3000` with key `nm_demo_acme_admin`.

---

## Demo flow

### Step 1 — Store a memory via Claude Code

Open Claude Code in this repo (Terminal 3 with the key exported).

Ask Claude:

> "Store a memory: we decided to use snake_case for all REST API endpoints in this project. Tag it as convention and api."

Claude calls `store_memory` → responds: `Memory stored (id: abc123)`

**Switch to the admin panel** → Memory Browser → the new entry appears instantly with:
- User: Sarah Chen
- Tool: claude-code
- Tags: convention, api

---

### Step 2 — Retrieve from a different session

Open a new Claude Code session (or simulate with a different key):

```bash
export NEXUSMIND_API_KEY=nm_demo_acme_marcus
```

Ask Claude:

> "What naming conventions does our team use for API endpoints?"

Claude calls `search_memory("API endpoint naming conventions")` → returns:

```
Found 1 result(s) for "API endpoint naming conventions":
• [claude-code] nexusmind — we decided to use snake_case for all REST API endpoints
  [convention, api] (19/5/2026)
```

**The memory Sarah stored is now available to Marcus** — across sessions, across tools.

---

### Step 3 — Show the audit trail

Switch to admin panel → Audit Log.

You'll see:
- `store` by Sarah Chen at 14:32
- `search` by Marcus Johnson at 14:33

> "Your compliance team has a full record of every AI interaction."

---

## Verify with inspector (developer mode)

```bash
make mcp-inspect
# Open the URL shown → Tools tab → call list_memories
```

---

## Smoke test (CI-safe)

```bash
NEXUSMIND_API_KEY=nm_demo_acme_sarah make mcp-test
```

Expected output:
```
==> Checking backend health...  OK
==> Storing a test memory...    Stored id: <uuid>
==> Searching for the memory... Found 1 result(s)
==> Cleaning up...              Deleted <uuid>

All MCP smoke tests passed.
```
