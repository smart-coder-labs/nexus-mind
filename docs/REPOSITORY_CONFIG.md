# Repository configuration — `.nexusmind.yaml`

`.nexusmind.yaml` tells NexusMind consumers which NexusMind project (and client) a path inside a
repository belongs to, and optionally which agent capabilities an MCP session may expose there.

It is **public, versioned configuration**: commit it next to the code. It must never contain API
keys, tokens, credentials, commands to execute, or absolute paths. Every consumer rejects a file
that does.

The canonical schema is [`schemas/nexusmind-config-v1.schema.json`](../schemas/nexusmind-config-v1.schema.json).
The shared conformance fixtures in [`schemas/fixtures/nexusmind-config/v1`](../schemas/fixtures/nexusmind-config/v1)
are the contract every implementation (Rust backend tools, TypeScript MCP, plugin hooks) must agree on.

## When you need it

Most repositories do **not** need this file. The Claude Code plugin infers the project from the git
remote (`github.com/org/my-repo.git` → `my-repo`), so a repository whose NexusMind project is named
after it works with no config at all.

Add `.nexusmind.yaml` when one of these is true:

| Situation | Why the file is needed |
|-----------|------------------------|
| The folder is **not a git repository** (a workspace holding several independent clones) | git has no answer, so the plugin would fall back to the folder name, find no index, and stand NexusMind down for the session. |
| A **monorepo** maps subtrees to different NexusMind projects | Only the file can say that `services/payments/**` is one project and `apps/storefront/**` another. |
| The repository name **differs** from the NexusMind project name | Inference would produce a project that does not exist. |
| You want to **narrow the MCP tool surface** for agents working in this repo | `agents.profiles` + `agent_profile` remove capabilities from the exposed tool catalog. |
| You run **`migrate-knowledge`** or the migration TUI against the repo | The migrator routes every scanned item to a project through this file and refuses to guess. |

## Where it lives

Put the file at the **git root**. Discovery walks upward from the working path and stops at the git
root: a file above the repository is ignored (a parent repository or workspace never leaks its config
into a nested clone). When the working directory is not inside a git repository, the walk continues
to the filesystem root and the directory holding the first `.nexusmind.yaml` found becomes the root.

An explicit `--config <path>` (migrator, MCP) skips discovery. The file must still be inside the
repository; a path outside it fails with `CONFIG_OUTSIDE_REPOSITORY`.

## Format

```yaml
version: 1

repository:
  id: commerce-monorepo            # slug: identifies this repo in provenance

defaults:
  project: platform                # alias used when no path rule matches (see Resolution)
  agent_profile: essential         # profile applied when the resolved project sets none

projects:
  platform:                        # alias — local label, must be a slug
    project_id: 5f1c…-uuid          # NexusMind project id (see "Identifiers")
    client_id: 0181…-uuid           # optional: NexusMind client id
    paths: ["**"]
    exclude: ["services/payments/**"]
  payments:
    project_id: 9a2e…-uuid
    client_id: 0181…-uuid
    paths: ["services/payments/**"]
    agent_profile: readonly

agents:
  profiles:
    essential:
      capabilities: [context.read, memory.read, memory.write, task.read]
    readonly:
      extends: essential
      capabilities: []
      disable_capabilities: [memory.write, task.write]
```

### Fields

| Field | Required | Type | Notes |
|-------|----------|------|-------|
| `version` | yes | `1` | Only version 1 exists. Any other value fails with `CONFIG_UNSUPPORTED_VERSION`. |
| `repository.id` | yes | slug | Stable identifier of the repository, recorded in migration provenance. |
| `defaults.project` | no | slug | Alias of the project to use when no `paths` rule matches. Must reference a key of `projects`. |
| `defaults.agent_profile` | no | slug | Profile applied when the resolved project declares none. Must reference a key of `agents.profiles`. |
| `projects.<alias>` | yes (≥ 1) | object | The alias is a slug: `^[a-z0-9][a-z0-9-]{0,63}$`. |
| `projects.<alias>.project_id` | yes | string (1–255) | Destination project. See [Identifiers](#identifiers). |
| `projects.<alias>.client_id` | no | string (1–255) | Destination client, for deployments with the client model. |
| `projects.<alias>.paths` | yes (≥ 1) | glob[] | Repo-relative patterns that belong to this project. |
| `projects.<alias>.exclude` | no | glob[] | Patterns removed from this project even if `paths` match. |
| `projects.<alias>.agent_profile` | no | slug | Profile applied to MCP sessions resolved to this project. |
| `agents.profiles.<name>` | no | object | Named capability set. `capabilities` is required (may be `[]`). |
| `agents.profiles.<name>.extends` | no | slug | Inherit another profile. Cycles fail with `CONFIG_PROFILE_CYCLE`. |
| `agents.profiles.<name>.capabilities` | yes | capability[] | Added on top of `extends`. |
| `agents.profiles.<name>.disable_capabilities` | no | capability[] | Removed after inheritance and additions. |

Unknown fields anywhere fail with `CONFIG_INVALID_SCHEMA: unknown field <path>`. Keys named like a
secret (`secret`, `token`, `password`, `api_key`, `private_key`, `credential`, singular or plural,
any case) fail with `CONFIG_SECRET_FIELD_FORBIDDEN` regardless of their value.

### Patterns

`paths` and `exclude` are globs relative to the config root:

- Segments may be literal, contain `*` (any characters within one segment) or `?` (one character),
  or be exactly `**` (zero or more segments).
- `pkg/**` matches everything **below** `pkg`, not the directory itself. To claim the directory and
  its subtree, list both: `["pkg", "pkg/**"]`. `**` alone matches the whole repository.
- Not allowed: a leading `/`, `..`, `.` segments, backslashes, character classes `[...]`, braces
  `{...}`, or `**` glued to other text (`src/**.ts`). These fail with `ROUTING_INVALID_PATTERN`.
- The Claude Code plugin hooks use a deliberately minimal matcher (no YAML parser, must never block
  a session start): they honour literal directory patterns and `dir/**` only, and ignore `exclude`.
  `*` and `?` are honoured by the MCP and the migrator. If the hooks matter to you, keep `paths`
  to literal directories.

### Capabilities

The vocabulary is fixed per schema version. Version 1:

```
context.read
memory.read      memory.write
convention.read  convention.write
project.read     client.read
task.read        task.write
sdd.read         sdd.write
code.read
usage.read       usage.write
migration.run    migration.review
harness.read     harness.write
```

An unknown capability fails with `CONFIG_UNKNOWN_CAPABILITY: <name>`.

A profile only **removes** tools from what the MCP would otherwise expose. It never grants a backend
permission, never bypasses RBAC, and never makes a forbidden call succeed. On a deployment running
the `only_context` MCP profile, the profile is applied on top of the `only_context` allow-list and
can only narrow it further.

## Resolution

Given a path relative to the config root:

1. A project whose `exclude` matches the path is discarded.
2. Among the remaining projects, every `paths` pattern that matches is scored by specificity: more
   literal segments win, then more literal characters, then fewer `**`, then fewer wildcards.
   Declaration order never matters.
3. The best-scoring project wins. Two **different** projects with the same best score fail with
   `ROUTING_AMBIGUOUS`; the default is never used to hide a conflict.
4. If nothing matches, `defaults.project` is used **only for the config root itself**. Any other
   unmatched path is `unmapped`. This keeps a workspace-level file from claiming a foreign clone that
   happens to sit inside it.

An explicit operator choice (`--project <alias>` on the MCP or migrator) overrides path routing.
The alias must exist in the file; otherwise loading fails with `CONFIG_INVALID_REFERENCE`.

## Identifiers

`project_id` and `client_id` are the **backend ids**, not names. Get them from the admin panel
(Projects / Clients pages) or from the `list_projects` and `list_clients` MCP tools. The backend
validates on every write that both belong to the caller's organization; the file cannot widen access.

The **alias** is a local label. Two consumers rely on it in different ways, so the safe convention is:

> **alias = the NexusMind project name, `project_id` = that project's id.**

| Consumer | What it reads from the file |
|----------|-----------------------------|
| `migrate-knowledge` CLI and migration TUI | `project_id` / `client_id` as run destinations; `repository.id`, the file path and its SHA-256 as provenance. Refuses to guess when a path is unmapped or ambiguous. |
| MCP server (`essential`, `reduced_readonly`, `only_context` profiles) | Resolves the alias for the working path to pick `agent_profile`, then filters the tool catalog by the effective capabilities. It does **not** rewrite the `project` argument of tool calls; the agent still passes the project name explicitly. |
| Claude Code plugin hooks (session start, code-index probe) | The **alias** of the project claiming the current directory, used as the project **name** when fetching context and checking `/v1/code/projects`. |

Because the hooks use the alias as a name and the migrator uses `project_id` as an id, an alias
that is not the project name breaks context injection, and a `project_id` that is a name breaks
migrations. Follow the convention above and both work.

## Examples

### Single repository, name differs from the project

```yaml
version: 1
repository:
  id: web-frontend
defaults:
  project: storefront
projects:
  storefront:
    project_id: 6b0d…-uuid
    paths: ["**"]
```

### Monorepo with two projects and a client

```yaml
version: 1
repository:
  id: commerce-monorepo
defaults:
  project: platform
projects:
  platform:
    project_id: 5f1c…-uuid
    client_id: 0181…-uuid
    paths: ["**"]
    exclude: ["services/payments/**"]
  payments:
    project_id: 9a2e…-uuid
    client_id: 0181…-uuid
    paths: ["services/payments/**"]
```

`services/payments/README.md` → `payments` (its rule is more specific than `**`). `docs/adr.md` →
`platform`.

### Workspace folder holding several clones (not a repository)

```yaml
version: 1
repository:
  id: my-workspace
defaults:
  project: nexus-mind            # answer for the workspace root itself
projects:
  nexus-mind:
    project_id: c2d7…-uuid
    paths: ["nexusmind", "nexusmind/**"]
  nexusmind-mcp:
    project_id: 5579…-uuid
    paths: ["nexusmind-mcp", "nexusmind-mcp/**"]
```

A clone inside the workspace that is not listed stays `unmapped` and resolves itself through its own
git remote, as if the workspace file did not exist.

### Read-only agents in one subtree

```yaml
version: 1
repository:
  id: commerce-monorepo
defaults:
  project: platform
  agent_profile: team
projects:
  platform:
    project_id: 5f1c…-uuid
    paths: ["**"]
  vendor:
    project_id: 5f1c…-uuid
    paths: ["third_party/**"]
    agent_profile: readonly
agents:
  profiles:
    team:
      capabilities: [context.read, memory.read, memory.write, convention.read, code.read]
    readonly:
      extends: team
      capabilities: []
      disable_capabilities: [memory.write]
```

An MCP session started under `third_party/` exposes no memory-writing tools.

## Migrator usage

```sh
migrate-knowledge --source repo-docs --path . --config .nexusmind.yaml --dry-run
migrate-knowledge --source repo-docs --path . --require-config --no-llm
```

`--dry-run` prints the resolved groups, unmapped items and routing errors without calling the
classifier or the backend. `--require-config` turns a missing file into an error. The migrator
resolves the whole inventory before classifying, creates one immutable run per project, and re-checks
the file's hash before the first write so a config edited mid-run aborts instead of publishing
against a stale snapshot.

## Errors

| Code | Meaning |
|------|---------|
| `CONFIG_UNSUPPORTED_VERSION` | `version` is not `1`. |
| `CONFIG_INVALID_SCHEMA` | Missing required field, unknown field, empty `projects`, or invalid YAML. The message names the field. |
| `CONFIG_INVALID_REFERENCE` | `defaults.project`, `defaults.agent_profile`, `agent_profile`, `extends` or `--project` names something that is not declared. |
| `CONFIG_UNKNOWN_CAPABILITY` | A capability outside the version-1 vocabulary. |
| `CONFIG_PROFILE_CYCLE` | `extends` forms a loop. |
| `CONFIG_SECRET_FIELD_FORBIDDEN` | A key that looks like a credential. |
| `CONFIG_OUTSIDE_REPOSITORY` | `--config` points outside the repository. |
| `ROUTING_INVALID_PATTERN` | A `paths`/`exclude` glob breaks the rules above. |
| `ROUTING_AMBIGUOUS` | Two projects match a path with equal specificity. |
| `WORKING_PATH_OUTSIDE_REPOSITORY` | `--working-path` resolves outside the config root. |

Changing any of this semantics incompatibly requires a new schema version.
