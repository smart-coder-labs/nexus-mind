# Per-agent runtimes

Pending, not-yet-deployed corrections to targets, Markdown portability, atomic
uploads and instruction validation: [HARNESS_VALIDATION_FIXES.md](HARNESS_VALIDATION_FIXES.md).

Agent creation/editing now stores `config.runtime`. A server-derived
`config.runtime_resolved` is included in the immutable agent revision and its
configuration hash. Clients cannot supply that derived snapshot as authority.
No database migration is required.

## Configuration

```json
{
  "executor": "pi",
  "runtime": {
    "nexusmind_profile": "essential",
    "acknowledge_executable": true,
    "harnesses": [],
    "components": [
      {
        "id": "backend-review",
        "kind": "skill",
        "files": {
          "SKILL.md": "---\nname: backend-review\ndescription: Review backend contracts\n---\nCheck compatibility and authorization."
        }
      },
      {
        "id": "docs",
        "kind": "mcp",
        "server": {
          "url": "https://example.com/mcp",
          "headers": { "Authorization": "${DOCS_AUTHORIZATION}" }
        }
      }
    ]
  }
}
```

`env` and `headers` accept whole environment references, not literal credentials.
For headers, provision the complete value (for example `Bearer ...`) in the
referenced environment variable. Stdio MCPs use `command`, `args`, and `env`.
HTTP MCPs use an HTTPS `url` and optional `headers`; SSE is not supported.

Profiles: `essential`, `only_context`, `reduced_readonly`, `factory_operator`,
`legacy`. NexusMind cannot be removed or shadowed by an additional component.
Selecting a profile does not bypass backend authorization or sandbox egress policy.

| Executor | Additional resources |
| --- | --- |
| Claude | Skills, scripts, self-contained plugins, prompt templates, MCPs |
| Pi | Skills, scripts, self-contained extensions, prompt templates, MCPs |
| Codex | Skills, scripts, MCPs |
| Nexus / OpenShell | MCPs; credentials must come from OpenShell providers |

Components can be disabled with `enabled: false`; workflow overrides can disable
inherited resources without editing the original agent. Pi extensions declare
custom tool names with `tools: ["browser"]`; builtin write tools and wildcards
cannot be enabled through this field. Extension entrypoints are `index.ts` or
`index.js`. Claude plugin metadata can be supplied as `files["plugin.json"]`;
it is staged into `.claude-plugin/plugin.json`. Files use safe relative paths.

Executable resources and MCPs require `acknowledge_executable: true`. Skills with
non-Markdown supporting files also require acknowledgement. Scripts are staged
and advertised, not automatically executed; existing role/tool restrictions still
apply. This feature does not grant shell access to a read-only reviewer.

## Registered harnesses

Each selection pins `id`, `version`, `manifest_hash`, and explicit component paths.
The editor previews the manifest before requesting an auditable install approval
for `target_tool=<executor>, target_scope=agent-runtime`. Saving checks visibility,
organization, target compatibility, publication/revocation status, approval, and
hash before copying selected content into the revision.

Self-contained `skill`, `command`, and `hook` catalog formats are supported when
compatible with the executor. A remote `plugin_marketplace` declaration is not an
executable bundle: it is rejected, not downloaded or silently ignored. Import
reviewed, self-contained plugin/extension files as additional components instead.
There are no implicit npm/git downloads, mutable `latest` versions, or global
package installations. Pi is now an allowed harness manifest target.

## Workflow steps

`options.agent_id` selects a reusable agent's runtime and executor. Optional
`options.executor` and `options.runtime` override that step only. Runtime override
fields inherit individually; component/harness arrays replace their inherited
arrays. Empty arrays explicitly remove additional resources, never NexusMind.
Schedules, publication permissions, and agent triggers are not inherited.

Saving resolves the current agent revision once and stores
`options.runtime_snapshot`, including its revision ID, effective configuration,
and runtime plan. Caller-supplied snapshots are discarded. Later agent edits do
not alter saved workflow definitions or existing runs. Starting a run rechecks
catalog visibility, approval and revocation without choosing new versions.

Runtime-enabled steps require `agent-runtime:v1` and `executor:<executor>` runner
capabilities. The installed adapter must actually match that executor; there is
no fallback. VM drivers must explicitly declare `supports_agent_runtime: true`
and apply the supplied runtime plan inside their isolated guest.

Select an agent or enable step customization in the workflow phase editor to
use the versioned runtime. NexusMind is mandatory in that selection. Existing
workspace infrastructure, role bindings, phase prompts, and legacy definitions
without this selection retain their current behavior.

## Operator provisioning

Local autonomous workers require `AUTONOMOUS_NEXUSMIND_MCP_CONFIG` (default
`/app/nexusmind-mcp.json`) containing a NexusMind stdio server, and a scoped
`NEXUSMIND_API_KEY` (or a key in that trusted server configuration). Resources and
MCP configuration are staged in a private temporary directory, removed after the
invocation. Pi shares only its existing `auth.json`, not global resources or
settings. This does not turn local Pi into a sandbox: local mode remains unsafe.

For additional local MCP credentials, explicitly allow environment names in
`AUTONOMOUS_RUNTIME_ALLOWED_ENV=DOCS_AUTHORIZATION,OTHER_KEY`. Their values are
redacted from captured transcripts. Autonomous task pods do **not** receive host
secret references; credentialed additional MCPs require a scoped sandbox proxy
route and are currently rejected at validation/execution rather than receiving
host secrets. Public MCPs remain subject to the existing egress policy.

Workspace runners require a trusted `nexusmind_mcp` server object in runner
configuration. Supply its scoped key/proxy through the adapter's existing
`environment_refs`, never through the runner registration token. Example:

```json
{
  "nexusmind_mcp": {
    "command": "nexusmind-mcp",
    "env": {
      "NEXUSMIND_BASE_URL": "https://api.example.com",
      "NEXUSMIND_API_KEY": "${NEXUSMIND_API_KEY}"
    }
  }
}
```

The runner's pinned image must contain the executor and selected MCP executables.
Pi adapters use `format: pi`; provider authentication must be provisioned inside
the isolated runtime. Codex adapters use `format: codex`, a provider-only
`codex_config`, and `usd_per_million: [input, cached_input, output]`. The operator
remains responsible for provider-side budget limits and approved egress networks.
The runtime configuration cannot grant network access or mount host resources.

## Checks

- Backend: `cargo test -j 1 --lib agent_runtime`.
- UI: `npm test -- src/pages/factory/AgentRuntimeEditor.test.tsx` in `apps/admin`.
- Runner: `python3 -m unittest discover -s scripts/factory -p test_agent_runtime.py`.

Validated locally: 14 targeted Rust tests, 35 UI tests, and 44 Python tests passed
(5 environment-dependent Python tests skipped). Admin production build, backend
`cargo check --tests`, and Nexus CLI `cargo check --locked` passed. These checks
use fixtures; no live model/provider end-to-end execution was claimed.

Release integration is based on `dcf9842` (latest published runner fixes), not
the old dirty checkout. Phase-specific prompts, long socket support, private/ZIP
exclusions, managed model relay, workspace settings and readonly mounts are retained.
No new migration is required. Provision the scoped MCP/provider settings above
before opting isolated workflow steps into additional runtimes.
