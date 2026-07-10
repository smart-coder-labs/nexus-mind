# NexusMind — Claude Code Plugin

Team memory for AI coding agents. Decisions, bugs, and conventions — shared across the whole team, persisted across sessions.

## How it works

NexusMind uses a three-layer approach (modeled after [Engram](https://github.com/Gentleman-Programming/engram)):

| Layer | Mechanism | Effect |
|-------|-----------|--------|
| **System prompt** | `SessionStart` hook stdout → `additionalContext` | Protocol + recent team memories injected on every session start |
| **Lifecycle hooks** | `UserPromptSubmit`, `SubagentStop`, `Stop` | Proactive save reminders every 15 min, passive subagent capture |
| **MCP tools** | `store_memory`, `search_memory`, `list_memories` | Claude can read/write memories at any point |

Claude never has to be told to save — the protocol is in its context from token 0.

## Requirements

- Node.js 18+
- A NexusMind API key
- Claude Code

## Install

```bash
npx nexusmind-setup
```

The interactive installer:
1. Asks for your `NEXUSMIND_API_KEY`
2. Adds the `nexusmind` MCP server to `~/.claude/settings.json`
3. Merges lifecycle hooks (SessionStart, UserPromptSubmit, SubagentStop, Stop)
4. Writes env vars to `~/.zshrc` and `~/.bashrc`

Then restart your shell and open Claude Code — NexusMind connects automatically.

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXUSMIND_API_KEY` | — | Your NexusMind API key (required) |
| `NEXUSMIND_BASE_URL` | `https://nexusmind-backend.fly.dev` | Backend URL (for self-hosting) |

## MCP Tools

| Tool | Description |
|------|-------------|
| `store_memory` | Save a decision, bug fix, convention, or discovery |
| `search_memory` | Full-text search over past team memories |
| `list_memories` | Browse recent memories, filter by project or tool |

Always pass `tool="claude-code"` and `project="<your-project>"` when storing.

## Harness Library — agent tools

A **harness** is a reusable, shareable bundle of agent configuration — a skill, an agent, a slash command, a hook, an output style, a Claude Code plugin, or a theme. NexusMind hosts a shared library of harnesses so a convention one teammate builds can be discovered, previewed, and installed by everyone else.

Agents (in Claude Code, Codex, or Cursor) drive this library through MCP tools. The backend only records approval state and serves immutable manifests — it never touches your filesystem. All diff computation, path resolution, and file writes happen locally in the MCP process.

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

Targets are one of `claude`, `codex`, or `cursor`. Formats are one of `agent`, `skill`, `command`, `hook`, `output_style`, `claude_code_plugin`, or `theme`.

### Two-phase, approval-first install

Installs are never one-shot. Nothing is written until you have seen the exact diff and confirmed it.

**Phase 1 — `plan_harness_install`** (writes nothing)

Inputs: `harness_id`, `version`, `target_tool`, `target_scope` (`user` or `project`, defaults to `project`). It reads the version manifest (readable without approval), resolves where each component would land, hashes any existing on-disk files, and returns:

- `diff` — one entry per component with `destination`, `relative_path`, `action` (`create` / `overwrite` / `skip`), `sha256`, `existing_sha256`, `size_bytes`, `executable`, and an optional `warning`.
- `warnings[]` and `requires_acknowledgement` — set when the harness contains executable formats.

This phase only reads files and imports nothing that can write to disk, so it is a true dry run. Show the diff to the user and get an explicit confirmation before proceeding.

**Phase 2 — `apply_harness_install`** (writes to disk)

Inputs: everything from the plan plus `manifest_hash`, and the acknowledgement flags below when required. It:

1. Records backend approval — persists a `(user_id, harness_version_id, manifest_hash)` approval row. The backend refuses the manifest download without a matching approved row.
2. Verifies the manifest hash — a drift returns `result_status: "hash_mismatch"` and stops.
3. Re-runs the plan and refuses any unconfirmed overwrite.
4. Materializes files and records the install result back on the approval row.

`result_status` is one of `installed`, `failed`, `hash_mismatch`, or `overwrite_not_confirmed`. The response also lists `written[]`, `skipped[]`, and any `errors`.

### Acknowledgement gates

Two flags on `apply_harness_install` guard destructive or executable actions:

| Flag | When required |
|------|---------------|
| `warning_acknowledged: true` | The plan reports `requires_acknowledgement: true` — i.e. the harness includes an executable format (`hook` or `claude_code_plugin`). Without it, apply bails with `warning_acknowledgement_required`. |
| `overwrite_confirmed: true` | The diff contains an `overwrite` action for an existing local file. Without it, apply returns `overwrite_not_confirmed` — *"this install would overwrite one or more existing files — re-run apply_harness_install with overwrite_confirmed: true to proceed"*. |

Both are opt-in booleans and must come from an explicit user decision, never a default.

### Where files land (Claude Code)

Destinations resolve under `~/.claude/` for `user` scope, or `<project>/.claude/` for `project` scope:

| Format | Destination |
|--------|-------------|
| `agent` | `agents/` |
| `skill` | `skills/` |
| `command` | `commands/` |
| `hook` | `hooks/` + merge into `settings.json` |
| `output_style` | `output-styles/` |
| `claude_code_plugin` | `plugins/` + merge into `settings.json` |
| `theme` | `themes/` |

Claude Code supports every format. (Codex and Cursor support a narrower set — see their plugin docs.)

The backend never writes to `~/.claude/settings.json`, shell profiles, hooks, MCP configs, or project files. It only records approval state and serves immutable manifests for local tools to preview and apply.

## What Claude does automatically

- **Session start** — searches NexusMind for context related to the first message
- **After any decision, bug fix, or convention** — calls `store_memory` immediately
- **Every 15 minutes** — reminded to save if nothing has been stored recently
- **Session close** — saves a summary (what was done, decisions, files changed, next steps)

## Hooks lifecycle

```
claude starts
  └── SessionStart (startup) → session-start.sh
        → outputs protocol + last 15 memories as additionalContext

user types first message
  └── UserPromptSubmit → user-prompt-submit.sh
        → systemMessage: "NexusMind tools available, use store_memory proactively"

context compacted
  └── SessionStart (compact) → post-compaction.sh
        → outputs protocol + ordered recovery instructions

subagent finishes
  └── SubagentStop (async) → subagent-stop.sh
        → passive capture of subagent output to /v1/memory/store

session ends
  └── Stop (async) → session-stop.sh
        → no-op (cleanup only)
```

## Uninstall

Remove the `nexusmind` key from `~/.claude/settings.json` under `mcpServers` and the matching entries under `hooks`. Remove the env var exports from your shell profile.

## Self-hosting

Set `NEXUSMIND_BASE_URL` to your own backend URL before running `npx nexusmind-setup`. The plugin points all API calls to that URL.

See [RUNNING.md](./RUNNING.md) for self-hosting the backend.
