# NexusMind — Claude Code Plugin

Team memory for AI coding agents. Store decisions, bugs, and conventions — shared across the whole team.

## What it does

- Injects your team's recent memories into every Claude Code session as context
- Provides `store_memory`, `search_memory`, and `list_memories` tools via MCP
- Reminds Claude Code to save decisions proactively throughout the session
- Passively captures subagent output for team context

## Requirements

- Node.js 18+
- A NexusMind API key (`NEXUSMIND_API_KEY`)
- Claude Code

## Install

### Option 1 — one-liner

```bash
curl -fsSL https://raw.githubusercontent.com/smart-coder-labs/nexus-mind/main/plugin/claude-code/install.sh | bash
```

### Option 2 — clone and run

```bash
git clone https://github.com/smart-coder-labs/nexus-mind.git
bash nexus-mind/plugin/claude-code/install.sh
```

### Option 3 — Claude plugin marketplace

```bash
claude plugin marketplace add smart-coder-labs/nexus-mind
claude plugin install nexusmind
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXUSMIND_API_KEY` | — | Your NexusMind API key (required) |
| `NEXUSMIND_BASE_URL` | `https://nexusmind-backend.fly.dev` | Backend URL (optional, for self-hosting) |

Set these in your shell profile or pass them before running Claude Code:

```bash
export NEXUSMIND_API_KEY=your-key-here
```

## What gets installed

The installer:
1. Adds the `nexusmind` MCP server to `~/.claude/settings.json`
2. Adds lifecycle hooks (SessionStart, UserPromptSubmit, SubagentStop, Stop)
3. Writes `NEXUSMIND_API_KEY` and `NEXUSMIND_BASE_URL` to `~/.bashrc` and `~/.zshrc`

## MCP Tools

| Tool | When to use |
|------|-------------|
| `store_memory` | Save a decision, bug fix, convention, or discovery |
| `search_memory` | Look up past decisions or context |
| `list_memories` | Browse recent team memories |

## Uninstall

Remove the `nexusmind` entry from `~/.claude/settings.json` under both `mcpServers` and `hooks`.
Remove the `NEXUSMIND_API_KEY` and `NEXUSMIND_BASE_URL` exports from your shell profile.
