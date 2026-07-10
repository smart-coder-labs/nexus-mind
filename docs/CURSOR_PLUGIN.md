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

## Harness Library — agent tools

A **harness** is a reusable, shareable bundle of agent configuration. NexusMind hosts a shared library so a setup one teammate builds can be discovered, previewed, and installed from Cursor — the same library Claude Code and Codex agents use. Cursor is a first-class target (it replaced `opencode`).

Cursor's AI drives the library through MCP tools. The backend only records approval state and serves immutable manifests — it never touches your filesystem. Diff computation, path resolution, and file writes all happen locally in the MCP process.

### Tools

| Tool | Purpose |
|------|---------|
| `recommend_harnesses` | Suggest relevant harnesses for a target tool (metadata only) |
| `list_harnesses` | Browse published harnesses, optionally filtered by `target` or owner |
| `get_harness_version` | Read a specific version's manifest for preview — no approval, no install |
| `list_harness_config_reviews` | List submitted config reviews, optionally by `status` |
| `plan_harness_install` | **Phase 1** — dry-run: compute the install diff, write nothing |
| `apply_harness_install` | **Phase 2** — record approval, then materialize files to disk |
| `build_harness_manifest_from_path` | Build a manifest from local files (runs a secret scan) |
| `create_harness` | Create a new harness record (`slug`, `name`) |
| `publish_harness_version` | Publish a version with its manifest |
| `create_harness_config_review` | Submit a redacted local config for team review |

Set `target_tool: "cursor"` when planning a Cursor install. Targets are `claude`, `codex`, or `cursor`.

### Two-phase, approval-first install

Installs are never one-shot. Nothing is written until you have seen the exact diff and confirmed it.

**Phase 1 — `plan_harness_install`** (writes nothing)

Inputs: `harness_id`, `version`, `target_tool`, `target_scope` (`user` or `project`, defaults to `project`). It reads the version manifest (readable without approval), resolves where each component would land, hashes any existing on-disk files, and returns a `diff` — one entry per component with `destination`, `relative_path`, `action` (`create` / `overwrite` / `skip`), `sha256`, `existing_sha256`, `size_bytes`, `executable`, and an optional `warning` — plus `warnings[]` and `requires_acknowledgement`. This phase only reads files, so it is a true dry run. Show the diff to the user and get explicit confirmation before proceeding.

**Phase 2 — `apply_harness_install`** (writes to disk)

Inputs: everything from the plan plus `manifest_hash`, and the acknowledgement flags below when required. It records backend approval (persisting a `(user_id, harness_version_id, manifest_hash)` row that the backend requires before serving the download), verifies the manifest hash (drift → `result_status: "hash_mismatch"`), re-runs the plan to refuse unconfirmed overwrites, then materializes files and records the result. `result_status` is one of `installed`, `failed`, `hash_mismatch`, or `overwrite_not_confirmed`; the response also lists `written[]`, `skipped[]`, and any `errors`.

### Acknowledgement gates

| Flag | When required |
|------|---------------|
| `warning_acknowledged: true` | The plan reports `requires_acknowledgement: true` — i.e. the harness includes an executable format (`hook` or `claude_code_plugin`). Without it, apply bails with `warning_acknowledgement_required`. |
| `overwrite_confirmed: true` | The diff contains an `overwrite` action for an existing local file. Without it, apply returns `overwrite_not_confirmed`. |

Both are opt-in booleans and must come from an explicit user decision, never a default.

### Where files land (Cursor)

Destinations resolve under `.cursor/` — `<project>/.cursor/` for `project` scope or `~/.cursor/` for `user` scope. Cursor supports a **narrower** set of formats than Claude Code:

| Format | Destination |
|--------|-------------|
| `agent` | `rules/` |
| `claude_code_plugin` | single `mcp.json` + settings merge |

Formats `skill`, `command`, `hook`, `output_style`, and `theme` are **not** supported for Cursor — `skill` and `output_style` in particular are Claude-centric and have no Cursor destination. Planning an unsupported format/target pair fails with *"Unsupported format/tool combination"* at plan time, so nothing is ever written.

NexusMind does not silently edit `.cursor/mcp.json`, global Cursor settings, shell profiles, or project files from the backend. It only records approval state and serves immutable manifests for local tools to preview and apply.

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
