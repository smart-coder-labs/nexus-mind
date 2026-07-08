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

## Harness Library approval flow

NexusMind may recommend reusable Claude Code harnesses, but recommendations are metadata only until a user approves an exact version and manifest hash.

1. Review the recommended harness name, targets, provenance, compatibility, and `manifest_hash`.
2. Approve the exact version before downloading the manifest.
3. Let the local Claude Code setup tool show a file diff.
4. Apply changes only after confirming that local diff.

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
