# NexusMind — Cursor Plugin

Team memory for Cursor. The same decisions, conventions, and bug fixes your team stores from Claude Code are instantly available inside Cursor — no duplicate setup, same backend.

---

## How it works

NexusMind uses the [MCP protocol](https://modelcontextprotocol.io), which Cursor supports natively since v0.45. The same MCP server used by Claude Code connects to Cursor through a `.cursor/mcp.json` config file.

```
Cursor ──MCP──► nexusmind-mcp ──HTTP──► NexusMind backend
Claude Code ──MCP──► nexusmind-mcp ──HTTP──► NexusMind backend
                                       ↑
                               same memories, same team
```

---

## Requirements

- Cursor v0.45+
- Node.js 18+
- A NexusMind API key

---

## Setup

### Option A — npx (recommended, always latest)

Create `.cursor/mcp.json` in your project root:

```json
{
  "mcpServers": {
    "nexusmind": {
      "command": "npx",
      "args": ["-y", "@smart-coder-labs/nexusmind-mcp"],
      "env": {
        "NEXUSMIND_API_KEY": "nm_your_key_here",
        "NEXUSMIND_BASE_URL": "https://your-nexusmind-url.com"
      }
    }
  }
}
```

### Option B — local binary (faster cold start)

```bash
npm install -g @smart-coder-labs/nexusmind-mcp
```

```json
{
  "mcpServers": {
    "nexusmind": {
      "command": "nexusmind-mcp",
      "env": {
        "NEXUSMIND_API_KEY": "nm_your_key_here",
        "NEXUSMIND_BASE_URL": "https://your-nexusmind-url.com"
      }
    }
  }
}
```

### Global config (all projects)

Place the same config at `~/.cursor/mcp.json` to enable NexusMind across every project.

---

## MCP Tools

Once connected, Cursor's AI has access to these tools:

| Tool | Description |
|------|-------------|
| `store_memory` | Save a decision, bug fix, convention, or discovery |
| `search_memory` | Full-text search over team memories |
| `list_memories` | Browse recent memories, filtered by project or type |
| `get_context` | **Cursor-specific** — fetch team context as a formatted block for rules or notepads |

---

## Cursor Rules injection (`get_context`)

The `get_context` tool returns team memories grouped by type as a markdown block — designed to be used in Cursor rules or notepads for persistent project context.

### Using as a Cursor Rule

1. Open Cursor → Settings → Rules (or create `.cursor/rules/nexusmind.mdc`)
2. Ask Cursor: *"Call get_context for project nexusmind and paste the result here"*
3. Cursor fetches live team memories and injects them as a rule

The output looks like:

```markdown
## NexusMind Team Context — nexusmind
> Last updated: May 23, 2026 · 12 memories

### Architecture & Design
- Auth model uses API keys, no JWT — validated per-request via SHA-256 hash
- SqliteStore wraps Arc<Mutex<Connection>>, exposes conn() for non-memory handlers

### Decisions
- Use anyhow::Result throughout handlers for consistent error propagation
- store.conn().lock() must be split into two lines to avoid E0716

### Bugs & Fixes
- Double slash in reset URLs — fixed by trimming trailing slash from APP_BASE_URL
```

### Using as a Notepad

1. Open Cursor → Notepads → New notepad → name it `Team Context`
2. Ask Cursor: *"Call get_context for project nexusmind and add the result to this notepad"*
3. Reference the notepad in any chat with `@Team Context`

---

## Demo flow

```
1. Developer A (Claude Code) fixes a bug and stores the memory:
   store_memory("Fixed N+1 query in UserList — added .include(:author)", type: "bugfix", project: "myapp")

2. Developer B (Cursor) starts a new session and asks:
   "What bugs have we fixed in myapp recently?"
   → Cursor calls search_memory("bugfix myapp") and surfaces the fix.

3. Before a big refactor, Developer B injects team context:
   → Cursor calls get_context(project: "myapp")
   → Pastes result into a Cursor notepad
   → Now every chat in that session has full team context
```

---

## Self-hosted

If you're running NexusMind on-premise, set `NEXUSMIND_BASE_URL` to your server URL:

```json
"env": {
  "NEXUSMIND_API_KEY": "nm_your_key",
  "NEXUSMIND_BASE_URL": "http://localhost:8080"
}
```

---

## Troubleshooting

| Issue | Fix |
|-------|-----|
| Tools not appearing in Cursor | Restart Cursor after adding `.cursor/mcp.json` |
| `NEXUSMIND_BASE_URL is not set` | Add `NEXUSMIND_BASE_URL` to the `env` block in `.cursor/mcp.json` |
| `NexusMind backend not reachable` | Check that your NexusMind backend is running and accessible |
| `Invalid API key` | Verify `NEXUSMIND_API_KEY` — get a new key from your admin panel → Users |
