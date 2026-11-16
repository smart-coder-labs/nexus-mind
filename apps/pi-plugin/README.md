# NexusMind for Pi

Installable Pi package: the existing NexusMind MCP profiles, with native lifecycle hooks instead of Claude shell scripts. Requires Pi **1.4+**, Node **22.19+**, and a NexusMind API key.

## Install from this checkout

From the `nexus-mind` root:

```bash
npm ci --ignore-scripts --prefix apps/pi-plugin
pi install ./apps/pi-plugin
export NEXUSMIND_API_KEY=your-key
# Optional: your own backend instead of the hosted default
export NEXUSMIND_BASE_URL=https://nexusmind.example.com
export NEXUSMIND_MCP_TOOL_PROFILE=only_context
pi
```

Restart Pi after installation or environment changes. Trust the project before connecting: the MCP reads its `.nexusmind.yaml`. `/mcp` shows the connection. To try without installing, use `pi -e ./apps/pi-plugin/index.ts` after `npm ci`.

This package registers `nexusmind` itself. Do **not** also configure it in `~/.pi/agent/mcp.json` or `.pi/mcp.json`: a file entry overrides the registered server, while hooks would still use the launch environment. Remove a previous registration with `pi mcp remove nexusmind` (add `--local` for a project entry).

The package is not published yet. After publication, installation will be `pi install npm:@smart-coder-labs/nexusmind-pi`.

## Profiles

`NEXUSMIND_MCP_TOOL_PROFILE` is passed to the bundled **nexusmind-mcp 0.20.0**, without duplicating its tool catalog:

| Profile | Behavior |
|---|---|
| `essential` (default) | Daily tools, memory protocol and automatic capture. |
| `only_context` | Memory, conventions, projects/clients and code context; no tasks, SDD or harness catalog. Same memory hooks. |
| `reduced_readonly` | MCP's handle-based `find_tools/load_tool/execute_tool/fetch` catalog. Context injection, but **no automatic writes or save gate**. The MCP itself also permits non-destructive writes; this name is not an authorization boundary. |
| `factory_operator` | Factory operator tools and context pack. No automatic memory reads, writes or save gate. |
| `legacy` | Full legacy catalog. Memory hooks enabled. Like the MCP, this profile does not apply repository capability filtering. |

Unknown profiles fail rather than silently exposing the legacy catalog. Tools use Pi's direct MCP exposure; the selected profile controls their number.

The MCP's existing resolver handles `.nexusmind.yaml` aliases, paths, exclusions and inherited `agents.profiles`. Hooks reuse that resolver and honor `memory.read`/`context.read`, `memory.write`, `convention.read` and `code.read`. Invalid configuration stops registration and automatic requests rather than widening permissions. Backend authorization still applies.

Project identity: matching YAML alias → git origin repository name → git root basename → current directory basename. The alias should be the NexusMind project name; `project_id` remains its UUID. See [repository configuration](../../docs/REPOSITORY_CONFIG.md).

## Hooks

| Claude equivalent | Pi event | Behavior |
|---|---|---|
| SessionStart | `session_start` + first `before_agent_start` | Registers MCP, restores branch-local markers, injects project memories/conventions and memory protocol. Probes code-index freshness; recommends NexusMind search only when current, otherwise allows local search. |
| UserPromptSubmit | `before_agent_start` | Additional recall context for recall-intent prompts, including Spanish. Project-scoped, not global team memories. |
| Stop | `agent_before_settle` | One continuation per session when a completed turn looks decision-like and no successful `store_memory` occurred. Failed saves do not count. Never retries an aborted/error run or loops. |
| PreCompact | `session_before_compact` | Awaits a best-effort snapshot before compaction, without changing Pi's compaction output. |
| PostCompact | `session_compact` | Reinjects protocol and remote context on the next model run. |
| SessionEnd | `session_shutdown` | Fallback snapshot unless the current user request already saved a session summary. |
| SubagentStop | `tool_result` for `subagent` | Captures successful decision-like textual results as unverified discoveries. Pi has no universal SubagentStop event: async completion notices are not captured here. Children loading this package get their own session hooks. |

Snapshots contain only text from the last 15 assistant messages on the **active branch**, capped at 6,000 characters; no thinking blocks or raw tool outputs. They upsert into `session-snapshot/<session-id>` and skip unchanged text, including after reload. Snapshots are explicitly labeled excerpts, not model-authored summaries. Automatic captures set `tool: pi`; model-issued stores retain the MCP's own attribution defaults.

### Environment

| Variable | Default |
|---|---|
| `NEXUSMIND_API_KEY` | Required; missing key produces a visible notice and disables integration. |
| `NEXUSMIND_BASE_URL` | `https://nexusmind-backend.fly.dev` |
| `NEXUSMIND_MCP_TOOL_PROFILE` | `essential` |
| `NEXUSMIND_PROMPT_INJECT` | `minimal`: recall only. `full`: every prompt. `off`: no per-prompt recall. Startup/post-compaction context still runs. |
| `NEXUSMIND_STOP_GATE` | `on`; `off` disables the save reminder. |
| `NEXUSMIND_AUTO_CAPTURE` | `on`; `off` disables transcript/subagent storage. Explicit MCP saves remain available. |

Credentials and backend URL come **only from the launch environment**, never from repository MCP files or Claude settings. URL requires HTTPS, except loopback HTTP for development; redirects are refused. Hook requests time out after six seconds, warn on failed capture, and leave the local Pi transcript intact. Network/auth failures do not block coding or compaction. Shutdown capture cannot run after SIGKILL or a machine failure.

## Checks

```bash
cd apps/pi-plugin
npm run check
npm test
npm pack --dry-run
```

The runnable test uses a local HTTP backend, the actual published MCP over stdio for all five profiles, and a Pi-event harness for routing, capability restrictions, trust, recall, bounded continuation, capture deduplication, reload and failure paths. No production credentials or model calls.

Known upstream dependency issue: `npm audit --omit=dev` reports the MCP's Zod 3 dependency (GHSA-mrhx-c7xc-m3w5); no compatible patched release is currently offered. Upgrading that dependency belongs in `nexusmind-mcp`, not a forced Zod 4 override here.
