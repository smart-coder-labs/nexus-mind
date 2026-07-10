# NexusMind — Codex Plugin

Team memory and a shared harness library for Codex. The same decisions, conventions, and bug fixes your team stores from Claude Code and Cursor are available inside Codex through the same MCP server and the same backend.

---

## How it works

NexusMind uses the [MCP protocol](https://modelcontextprotocol.io). The same MCP server used by Claude Code and Cursor connects to Codex, so memories and the harness library are shared across all three tools.

```
Codex       ──MCP──► nexusmind-mcp ──HTTP──► NexusMind backend
Claude Code ──MCP──► nexusmind-mcp ──HTTP──► NexusMind backend
Cursor      ──MCP──► nexusmind-mcp ──HTTP──► NexusMind backend
                                      ↑
                              same memories, same team
```

---

## Requirements

- Codex with MCP server support
- Node.js 18+
- A NexusMind API key

---

## Setup

Add the `nexusmind` MCP server to your Codex MCP configuration (typically `~/.codex/config.toml` or the equivalent MCP config for your Codex setup):

```toml
[mcp_servers.nexusmind]
command = "npx"
args = ["-y", "@smart-coder-labs/nexusmind-mcp"]

[mcp_servers.nexusmind.env]
NEXUSMIND_API_KEY = "nm_your_key_here"
NEXUSMIND_BASE_URL = "https://your-nexusmind-url.com"
```

For a faster cold start, install the binary globally (`npm install -g @smart-coder-labs/nexusmind-mcp`) and set `command = "nexusmind-mcp"`.

---

## MCP Tools

Once connected, Codex's agent has access to the memory tools (`store_memory`, `search_memory`, `list_memories`) and the harness library tools described below.

---

## Harness Library — agent tools

A **harness** is a reusable, shareable bundle of agent configuration. NexusMind hosts a shared library so a setup one teammate builds can be discovered, previewed, and installed from Codex — the same library Claude Code and Cursor agents use.

Codex's agent drives the library through MCP tools. The backend only records approval state and serves immutable manifests — it never touches your filesystem. Diff computation, path resolution, and file writes all happen locally in the MCP process.

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

Set `target_tool: "codex"` when planning a Codex install. Targets are `claude`, `codex`, or `cursor`.

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

### Where files land (Codex)

Destinations resolve under `~/.codex/` for `user` scope, or `<project>/.codex/` for `project` scope. Codex supports a **narrow** set of formats:

| Format | Destination |
|--------|-------------|
| `agent` | `agents/` |
| `command` | `prompts/` |

Formats `skill`, `hook`, `output_style`, `claude_code_plugin`, and `theme` are **not** supported for Codex — `skill` and `output_style` in particular are Claude-centric and have no Codex destination. Planning an unsupported format/target pair fails with *"Unsupported format/tool combination"* at plan time, so nothing is ever written.

> **Conservative default.** The `~/.codex/` destinations above (`agents/`, `prompts/`) are a deliberately conservative default chosen while upstream Codex configuration conventions are still stabilizing. They may be widened or adjusted once Codex publishes clearer, canonical locations for agent and prompt assets. Treat them as the current safe mapping, not a permanent contract.

NexusMind does not silently edit `~/.codex/` configuration, shell profiles, or project files from the backend. It only records approval state and serves immutable manifests for local tools to preview and apply.

---

## Self-hosted

If you're running NexusMind on-premise, set `NEXUSMIND_BASE_URL` to your server URL in the `env` block of your Codex MCP config.

See [RUNNING.md](./RUNNING.md) for self-hosting the backend.

## Troubleshooting (Windows)

First step for any problem: `npx @smart-coder-labs/nexusmind-mcp doctor`. It reports the API key the current process sees vs. the Windows user registry vs. `config.toml`, checks that the server launches via npx, and validates the key against the backend.

| Symptom | Cause | Fix |
|---------|-------|-----|
| `connection closed: initialize response` (MCP won't start) | Corrupted npx cache — `npx -y <pkg>@latest` can't resolve the server bin | `doctor` (or re-running `setup`) clears it automatically; otherwise `npm cache clean --force`. Then restart Codex |
| Hooks show "Failed", no approve prompt | Fixed in `nexusmind-mcp` ≥ 0.8.2. Older versions wrote the hook command in a form Codex couldn't spawn on Windows | Upgrade and re-run `setup`, then run `/hooks` and approve the NexusMind hooks (the fix changes each hook's hash, so re-approval is required once) |
| `Invalid API key` after rotating the key | `setx` doesn't update already-running programs; the old key lingers in the running Codex | Fully quit and reopen Codex from the Start menu (not from an existing terminal) |

Codex hooks are trusted by hash (`config.toml` `[hooks.state]`); any change to a hook's command requires re-approval via `/hooks`.
