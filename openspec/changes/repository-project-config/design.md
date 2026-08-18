# Design — Repository Project Configuration

> **Change**: `repository-project-config`
> **Status**: proposed
> **Owner**: backend + tooling + MCP
> **Date**: 2026-08-17
> **Depends on**: proposal and delta spec in this change

This design turns `.nexusmind.yaml` into one deterministic routing contract without making a local
file an authorization boundary. It covers the first delivery: config contract, Rust resolver,
migrator CLI/TUI, and MCP capability filtering. Context, SDD, tasks, usage, and code indexing consume
the same contract in later changes.

---

## 1. Decisions

| Area | Decision |
|---|---|
| Canonical filename | `.nexusmind.yaml` only in v1 |
| Contract | Checked-in JSON Schema plus valid/invalid conformance fixtures |
| Rust parser | `serde_yaml` into structs with `deny_unknown_fields` |
| TypeScript parser | YAML parser plus JSON-Schema validation; no handwritten permissive coercion |
| Routing | Ordered by computed specificity, never declaration order |
| Migration boundary | Scan once, resolve every `SourceItem`, validate inventory, then classify by project |
| Backend model | One existing immutable `MigrationRun` per project; no schema migration |
| TUI ownership | CLI is authoritative; TUI previews resolver output emitted over NDJSON |
| Capabilities | Allow-list profiles over tool descriptor capabilities; backend permissions remain authoritative |
| Environment | No environment matrix in v1; IDs are explicit in the selected config |
| Local overrides | No `.nexusmind.local.yaml` in v1 |
| Config mutation | Read-only; consumers never create backend projects or rewrite the config |

The deliberately small v1 avoids two sources of hidden state: environment inference and layered local
overrides. Both can be added later as explicit schema versions if operational use proves they are
needed.

---

## 2. Canonical schema

The source of truth lives at:

```text
schemas/nexusmind-config-v1.schema.json
schemas/fixtures/nexusmind-config/v1/
  valid/*.yaml
  invalid/*.yaml
  cases.json
```

`cases.json` lists each fixture, expected validity, paths to resolve, and normalized expected results.
The schema uses `additionalProperties: false` at every object boundary. Fixtures cover semantics that
JSON Schema alone cannot express: ambiguous routing, profile cycles, escape paths, duplicate aliases,
and unknown capability names.

### 2.1 YAML shape

```yaml
version: 1

repository:
  id: ecommerce-platform

defaults:
  project: platform
  agent_profile: essential

projects:
  platform:
    project_id: prj-platform
    client_id: client-acme
    paths:
      - "**"
    exclude:
      - "services/payments/**"
      - "apps/storefront/**"

  payments:
    project_id: prj-payments
    client_id: client-acme
    paths:
      - "services/payments/**"

  storefront:
    project_id: prj-storefront
    client_id: client-retail
    paths:
      - "apps/storefront/**"

agents:
  profiles:
    essential:
      capabilities:
        - context.read
        - memory.read
        - memory.write
        - convention.read
        - convention.write
        - task.read
        - task.write
        - sdd.read
        - sdd.write
        - code.read
        - usage.write

    readonly:
      extends: essential
      disable_capabilities:
        - memory.write
        - convention.write
        - task.write
        - sdd.write
        - usage.write
```

Changes from the illustrative proposal shape:

- Root coverage is `"**"`, not `"/"`; every pattern is a repo-relative glob over normalized paths.
- Capabilities are allow/disable lists, not booleans. Absence from the effective set has one meaning:
  not exposed.
- A profile MAY extend exactly one parent. The effective set is the parent's effective set, union the
  child's `capabilities`, minus the child's `disable_capabilities`. A capability named in both child
  lists is invalid, missing parents are invalid, and cycles are rejected with their full alias chain.
- Project-level profile selection is supported via optional `agent_profile` on each project; otherwise
  `defaults.agent_profile` applies.

### 2.2 Typed model

Conceptually, both implementations expose:

```text
RepositoryConfigV1
  version: 1
  repository: { id: NonEmptySlug }
  defaults?: { project?: ProjectAlias, agent_profile?: ProfileName }
  projects: Map<ProjectAlias, ProjectConfig>
  agents?: { profiles: Map<ProfileName, AgentProfile> }

ProjectConfig
  project_id: NonEmptyString
  client_id?: NonEmptyString
  paths: NonEmptyList<RepoGlob>
  exclude?: List<RepoGlob>
  agent_profile?: ProfileName

AgentProfile
  extends?: ProfileName
  capabilities: UniqueList<KnownCapability>
  disable_capabilities?: UniqueList<KnownCapability>
```

Aliases, repository IDs, and profile names use lowercase ASCII slugs (`[a-z0-9][a-z0-9-]{0,63}`).
Backend IDs are opaque non-empty strings capped at 255 bytes; the local parser does not encode the
backend's current UUID/slug implementation.

YAML aliases/anchors are rejected. Duplicate mapping keys are rejected before deserialization. This
prevents a security-sensitive field from having two visually plausible values with parser-dependent
last-key-wins behavior.

---

## 3. Rust module boundary

The config is not migration-specific. It lives in the backend library crate so current consumers can
reuse it without a new workspace/package split:

```text
apps/backend/src/repository_config/
  mod.rs          public types and ConfigSnapshot
  discovery.rs    Git-root-bounded discovery
  parse.rs        bytes → validated RepositoryConfigV1
  routing.rs      compiled patterns and path resolution
  capabilities.rs known vocabulary and profile lookup
  tests.rs
```

Public API:

```rust
pub struct ConfigSnapshot {
    pub config: RepositoryConfigV1,
    pub root: PathBuf,          // process-local only
    pub relative_path: String,  // persisted, normally ".nexusmind.yaml"
    pub sha256: String,
}

pub enum ConfigSelection {
    Explicit(PathBuf),
    DiscoverFrom(PathBuf),
}

pub enum ResolutionStatus {
    Resolved(ResolvedProject),
    Unmapped,
}

pub struct ResolvedProject {
    pub alias: String,
    pub project_id: String,
    pub client_id: Option<String>,
    pub basis: ResolutionBasis,
}

pub enum ResolutionBasis {
    Pattern { pattern: String, specificity: Specificity },
    Default,
    ExplicitOverride { configured: Option<Box<ResolutionBasis>> },
}

pub fn load(selection: ConfigSelection) -> Result<Option<ConfigSnapshot>, ConfigError>;
pub fn compile(snapshot: ConfigSnapshot) -> Result<ProjectResolver, ConfigError>;
pub fn resolve(&self, path: &Path, override_: Option<&DestinationOverride>)
    -> Result<ResolutionStatus, ResolutionError>;
```

`ConfigSnapshot.root` and canonical filesystem paths never serialize. An explicit `AttestationView`
produces only schema version, repository ID, repo-relative config path, hash, selected alias/IDs, and
routing basis. This makes accidental home-directory leakage harder than relying on callers to redact.

The TUI remains a separate crate and does not depend on the backend library because that would rebuild
the backend's heavy embedding/tree-sitter dependency graph. It delegates discovery and resolution to
the runner and consumes protocol events.

---

## 4. Discovery and filesystem safety

Discovery takes the source `--path`, not the process cwd:

1. Resolve the source path without following a non-existent final component.
2. Determine its Git root using `git rev-parse --show-toplevel` with that directory as cwd.
3. Walk parents from the source directory to that root inclusive.
4. Select the first `.nexusmind.yaml` encountered.
5. Stop at the Git root even when it contains no config.

For `--config`, canonicalize both the config and Git root, then require the config to be inside that
root. Symlinked configs whose canonical target escapes are rejected. Discovery never consults `$HOME`
or a parent checkout.

Reading returns the exact bytes and hashes them immediately with SHA-256. The runner retains file
identity metadata plus the hash and rereads the bytes immediately before the first POST. A differing
hash aborts publication. Changes after runs begin do not mutate those runs; every run uses the initial
snapshot attestation.

No config value is interpolated from environment variables. YAML scalar strings remain literal.

---

## 5. Pattern model and specificity

Patterns use gitignore-style glob syntax compiled with Rust `globset` and an equivalent TypeScript
implementation proven by fixtures. Inputs and patterns are normalized to `/` separators and must be
relative, UTF-8 repository paths.

Supported syntax in v1:

- literal path segments;
- `*` inside one segment;
- `?` inside one segment;
- `**` as an entire segment for zero or more segments.

Unsupported constructs such as character classes, braces, extglobs, leading `/`, empty segments,
`.`/`..`, and backslashes are rejected. Restricting syntax reduces cross-language disagreement.

### 5.1 Project match

A project matches a path when at least one `paths` pattern matches and no `exclude` pattern matches.
Exclusion belongs only to that project; it does not globally suppress another project's inclusion.

Each matching include pattern receives a lexicographically comparable specificity tuple:

```text
(
  literal_segment_count,
  literal_character_count,
  -double_star_count,
  -single_wildcard_count,
  segment_count
)
```

Higher tuples are more specific. For each project, retain its highest-scoring matching pattern. Across
projects:

- one highest project wins;
- equal highest tuples for the same project are harmless and choose the lexicographically smallest
  pattern for stable explanation;
- equal highest tuples across different projects are `Ambiguous`, regardless of declaration order;
- no match uses `defaults.project` when present, otherwise returns `Unmapped`.

This rule is computable without examining the filesystem and stable across Rust and TypeScript.

### 5.2 Inventory-wide validation

The parser can detect duplicate IDs, invalid defaults, and exact duplicate patterns. Ambiguity that
depends on real paths is detected when resolving the scanned inventory. The migrator accumulates all
resolution errors before failing so the operator fixes the config once rather than one path per run.

---

## 6. Migration pipeline changes

### 6.1 New CLI inputs

```text
--config <path>       explicit config; otherwise discover from --path
--project <id>        existing explicit override, highest precedence
--client <id>         existing explicit override; valid only with a project destination
--require-config      optional CI/operator guard; fail when no config is found
```

Existing invocations remain valid. With no config and no explicit destination, dry-run may scan and
report unresolved routing, but publication still requires a destination.

### 6.2 Source item routing key

`SourceItem` gains a required repo-relative `routing_path: String`. `display_origin` is not reused:
it is presentation text and some connectors include anchors or logical identities that are not paths.

Connector behavior:

- `repo-docs`: document path;
- `git-history`: path set is potentially many files, so one commit can cross projects;
- `claude-memories`: project-scoped assets use their recorded repo path; host-scope assets are
  explicitly unmapped unless an override is supplied;
- `db-schema`: no repository path, therefore requires explicit/default destination in v1.

The git-history connector is the exceptional case. A commit touching multiple configured projects is
split into one source item per resolved project, retaining the same commit SHA plus the project's
relevant file subset in identity/provenance. A commit with no usable changed paths follows default or
unmapped behavior. This prevents one cross-cutting commit from being attributed arbitrarily.

### 6.3 Execution stages

The current flow scans then classifies all items before destination resolution. It becomes:

```text
load + hash config
        ↓
scan connector (no model)
        ↓
resolve/split entire inventory
        ↓
emit routing plan; fail on ambiguity/unmapped publication
        ↓
classify each project group under the existing global token budget
        ↓
recheck config hash
        ↓
create one run per project and stage only that group's candidates
```

Classification remains deterministic in input order: project groups sort by project ID, and items
retain scan order within a group. `--max-tokens` is global across the command, not reset per project.
If it trips, later groups are not classified or published.

Publishing is not globally transactional across HTTP runs. To make partial failure honest:

- groups publish sequentially in stable order;
- every successfully created run is reported immediately;
- a later failure returns non-zero and lists already-created run IDs;
- retry relies on existing candidate/run idempotency and config attestation;
- the runner never auto-cancels or commits earlier runs.

### 6.4 Run attestation

The existing `attestation` object receives:

```json
{
  "repository_config": {
    "schema_version": 1,
    "repository_id": "ecommerce-platform",
    "path": ".nexusmind.yaml",
    "sha256": "sha256:...",
    "project_alias": "payments",
    "project_id": "prj-payments",
    "client_id": "client-acme",
    "selection": "pattern",
    "patterns": ["services/payments/**"]
  }
}
```

For an explicit override, `selection` is `explicit_override` and `configured_destination` records the
non-secret destination that would otherwise have applied. Candidate provenance keeps its existing
source identity; routing provenance belongs to the run because all candidates share it.

No database migration is required. The backend already accepts attestation and enforces project/client
organization coherence when creating the immutable run.

---

## 7. NDJSON protocol and TUI

The runner adds events before classification:

```text
config_loaded { repository_id, relative_path, sha256, project_count }
routing_group  { alias, project_id, client_id?, item_count, sample_paths }
routing_issue  { path, kind: "unmapped"|"ambiguous", candidates, detail }
routing_ready  { groups, mapped_items, unmapped_items, ambiguous_items }
run_created    { alias, project_id, run_id }
```

`sample_paths` is bounded; NDJSON must not dump an entire monorepo into TUI memory. The final routing
summary has counts, while a CLI prose mode can print all issues up to an explicit display cap and write
a machine-readable report when requested later.

The TUI adds `config_path` and `require_config` to `RunConfig` but does not parse YAML. Its source screen
can run a dry preview through the child, then render grouped destinations and blockers. Manual project
and client fields remain override inputs and are labelled as such. `to_args`, safe command rendering,
and protocol contract tests cover the new flags/events.

Unknown future NDJSON events retain the existing fail-visible behavior so runner/TUI version drift is
not silently ignored.

---

## 8. Agent profiles and MCP integration

### 8.1 Capability vocabulary

The schema publishes a v1 enum grouped by domain:

```text
context.read
memory.read, memory.write
convention.read, convention.write
project.read
client.read
task.read, task.write
sdd.read, sdd.write
code.read
usage.read, usage.write
migration.run, migration.review
harness.read, harness.write
```

This vocabulary describes local tool exposure, not backend grants. It is intentionally coarser than
individual tool names so profiles survive catalog evolution.

Every MCP `ToolDefinition` changes from free-form tags alone to an explicit
`required_capabilities: string[]`. Existing search-oriented tags may remain as `keywords`; they must
not double as enforcement metadata. A tool is exposed only when all required capabilities are in the
effective profile.

### 8.2 Profile selection

The MCP process resolves its active project from an explicit working path/config selection at startup:

```text
--config <path>
--project <alias-or-id>
--agent-profile <name>
```

Precedence is explicit profile > resolved project's `agent_profile` > `defaults.agent_profile` > the
existing unfiltered behavior for backward compatibility. `NEXUSMIND_MCP_TOOL_PROFILE` continues to
choose presentation mode (`legacy`, `essential`, `reduced_readonly`); it is separate from the agent
capability profile.

Terminology:

- **tool profile**: how tools are presented over MCP;
- **agent profile**: which capability domains are locally exposed;
- **permissions**: what the backend authorizes for the bearer key.

### 8.3 Catalog construction

The TypeScript flow becomes:

```text
allDefinitions
  → filterByAgentCapabilities(...)
  → essential direct registration OR ToolFabric(filteredDefinitions)
  → backend permission check on every call
```

Filtering happens once at startup. `load_tool` and `execute_tool` receive the already-filtered registry,
so a hidden tool handle cannot bypass discovery. The legacy 148-tool catalog lacks typed definitions;
v1 logs a clear warning and preserves legacy behavior rather than pretending it is filtered. Full
legacy filtering is a separate catalog-conversion change. Users who require capability enforcement use
`essential` or `reduced_readonly`.

Rust and TypeScript both execute the canonical fixtures. TypeScript does not import Rust code, and the
main repo does not become a runtime dependency of the published npm package. Release packaging copies
the schema and fixtures into `nexusmind-mcp`; CI compares their SHA-256 against the canonical copies.

---

## 9. Error model

Errors have stable codes and human context:

```text
CONFIG_NOT_FOUND
CONFIG_UNSUPPORTED_VERSION
CONFIG_INVALID_SCHEMA
CONFIG_DUPLICATE_KEY
CONFIG_SECRET_FIELD
CONFIG_OUTSIDE_REPOSITORY
CONFIG_CHANGED
CONFIG_UNKNOWN_CAPABILITY
ROUTING_INVALID_PATTERN
ROUTING_AMBIGUOUS
ROUTING_UNMAPPED
ROUTING_OVERRIDE_INVALID
```

Diagnostics may contain repo-relative paths, aliases, field paths, and pattern text. They never include
raw secret-shaped values, source contents, API keys, DSNs, or absolute home paths. Batch routing errors
include a bounded sample plus total count.

---

## 10. TDD and verification strategy

Implementation follows the repo's strict backend TDD rule.

### 10.1 Contract and parser

1. Valid minimal and multi-project fixtures parse.
2. Unknown version/field, duplicate YAML key, YAML alias, secret field, empty ID, invalid reference, and
   unknown capability fail with stable codes.
3. Snapshot hash is over exact bytes; persisted view excludes absolute paths.
4. Discovery stops at Git root and rejects explicit/symlink escapes.

### 10.2 Router

1. Literal, `*`, `?`, and whole-segment `**` semantics.
2. Specificity tuple ordering and declaration-order independence.
3. Project-local exclusion, default, unmapped, and equal-score ambiguity.
4. Windows separators normalize identically without accepting backslashes in config patterns.
5. Rust results match every canonical fixture.

### 10.3 Migrator

1. Resolution happens before any classifier invocation.
2. Dry-run emits groups/issues and makes no HTTP request.
3. Two projects create two run bodies with isolated candidate batches.
4. Global token budget is not reset between groups.
5. Config mutation before publish aborts with no run.
6. Partial HTTP failure reports already-created run IDs and exits non-zero.
7. Existing no-config explicit CLI tests remain green.
8. Connector-specific routing covers repo docs, git-history splits, host-scope Claude assets, and DB
   schema explicit/default behavior.

### 10.4 TUI

1. New args render safely and API keys remain redacted.
2. Protocol parser accepts every routing event and rejects unknown runner drift visibly.
3. Confirmation displays all groups and blocks on issues.
4. Real-runner contract test proves event compatibility.

### 10.5 MCP

1. TypeScript validates the same fixtures and normalized cases as Rust.
2. Read-only profile omits write definitions from essential and reduced catalogs.
3. A fabricated hidden handle cannot load or execute.
4. Enabling a capability does not bypass a mocked backend permission denial.
5. Tool-profile and agent-profile precedence remain independent.
6. No-config startup preserves existing catalogs.

Verification commands remain proportional to touched components:

```text
cargo test --manifest-path apps/backend/Cargo.toml repository_config
cargo test --manifest-path apps/backend/Cargo.toml --bin migrate-knowledge
cargo test --manifest-path apps/migrator-tui/Cargo.toml
cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings
cargo fmt --check --manifest-path apps/backend/Cargo.toml
npm test                         # in nexusmind-mcp
npm run build                    # in nexusmind-mcp
```

---

## 11. Rollout

1. Land schema, fixtures, Rust parser/router, and read-only validation first.
2. Add migrator dry-run routing and TUI preview; keep publication behind existing explicit IDs until
   tests prove parity.
3. Enable config-derived publication with one run per project.
4. Port schema/fixtures and capability filtering to `nexusmind-mcp` in its own worktree/PR.
5. Publish example config and migration guide; do not auto-create `.nexusmind.yaml` in existing repos.
6. Observe routing errors and explicit override rates before connecting context/SDD/tasks/code/usage.

Rollback is configuration-free: explicit `--project`/`--client` remains supported. MCP filtering is
opt-in through an agent profile; removing profile selection restores the existing catalog without a
backend or data migration.

---

## 12. Consequences and deferred work

### Positive

- Project attribution becomes explainable and reproducible across a monorepo.
- The migrator preserves its immutable per-run project model and backend authorization.
- TUI and CLI cannot drift because the runner owns resolution.
- Capability profiles reduce agent noise without being mistaken for security permissions.
- Fixtures provide a practical contract across Rust and TypeScript.

### Costs

- `SourceItem` and connector tests must gain routing paths.
- Git-history needs principled splitting for cross-project commits.
- A second parser implementation remains necessary in the npm package.
- Restricted glob syntax is less expressive than gitignore but substantially safer to reproduce.

### Deferred

- Environment-specific ID maps and local override files.
- Automatic backend project lookup or config generation.
- Capability filtering for the legacy 148-tool catalog.
- Native consumption by context, memories, SDD, tasks, code indexing, and usage.
- A backend repository registry or centralized policy that can constrain local configs.
- One atomic server-side operation that creates multiple migration runs.
