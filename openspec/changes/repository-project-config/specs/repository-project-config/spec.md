# Delta for Repository Project Configuration

## ADDED Requirements

### Requirement: Versioned Repository Configuration

The system MUST recognize `.nexusmind.yaml` as the canonical repository configuration file. The
document MUST declare `version: 1` and MAY declare repository identity, project mappings, defaults,
agent profiles, and consumer-specific non-secret settings. Unknown versions, unknown fields, and
invalid field types MUST be rejected with diagnostics that identify the offending field.

The configuration MUST NOT accept API keys, access tokens, database credentials, private keys, or
commands to execute.

#### Scenario: Load a valid version-one configuration

- GIVEN a repository containing `.nexusmind.yaml` with `version: 1` and valid project mappings
- WHEN a NexusMind consumer loads repository configuration
- THEN it returns a typed version-one configuration
- AND it preserves the declared repository, project, routing, and agent-profile information

#### Scenario: Reject an unsupported version

- GIVEN a `.nexusmind.yaml` declaring an unsupported version
- WHEN a consumer loads it
- THEN loading fails before any migration, model call, or backend write
- AND the diagnostic identifies the unsupported version

#### Scenario: Reject secret-bearing fields

- GIVEN a `.nexusmind.yaml` containing an API key, token, DSN, private key, or executable command
- WHEN the configuration is validated
- THEN validation fails
- AND the diagnostic states that secrets and executable content do not belong in repository config
- AND the rejected value is not reproduced in logs or diagnostics

### Requirement: Repository-Bounded Discovery

Unless an explicit config path is supplied, a consumer MUST search for `.nexusmind.yaml` from its
working path upward and MUST stop at the Git repository root. It MUST NOT discover a config above
that root. An explicit config path MUST be resolved and validated before use.

#### Scenario: Discover config from a nested directory

- GIVEN `.nexusmind.yaml` at the Git root
- AND the consumer starts under `services/payments/src`
- WHEN it discovers repository configuration
- THEN it loads the root config
- AND it reports that config as the source of the resolution

#### Scenario: Do not inherit config from a parent repository

- GIVEN no `.nexusmind.yaml` exists inside the current Git repository
- AND a file with that name exists above its root
- WHEN discovery runs
- THEN no config is discovered
- AND the parent file has no effect

#### Scenario: Explicit config takes precedence over discovery

- GIVEN a discoverable root config
- AND the operator supplies `--config <path>` naming another valid config
- WHEN the consumer loads configuration
- THEN it uses the explicitly selected config
- AND reports that explicit selection in its resolution explanation

### Requirement: Stable Project And Client Identity

Each configured project MUST have a unique local alias and MUST identify its NexusMind destination
with a stable `project_id`. A project MAY identify its owner with `client_id`. Consumers MUST treat
aliases as local labels and MUST NOT submit an alias where a backend ID is required.

The local resolver MUST NOT claim that an ID exists or belongs to an organization. The backend MUST
continue validating organization membership and authorization on every write.

#### Scenario: Resolve an alias to stable IDs

- GIVEN project alias `payments` declares `project_id: prj-payments` and `client_id: client-acme`
- WHEN a path maps to `payments`
- THEN the resolved destination carries `prj-payments` and `client-acme`
- AND the alias remains available for human-readable explanation

#### Scenario: Backend rejects a foreign destination

- GIVEN a locally valid config references a project from another organization
- WHEN a consumer attempts a backend write using that project ID
- THEN the backend rejects the write according to its existing authorization rules
- AND the local config does not weaken or bypass that decision

### Requirement: Deterministic Multi-Project Path Routing

A repository MAY declare multiple projects and repo-relative path patterns for each project. The
resolver MUST normalize an input path relative to the config root and return either exactly one
project, `unmapped`, or an ambiguity error. It MUST use a documented specificity rule independent of
declaration order.

Patterns MUST NOT escape the repository through absolute paths, `..`, symlink traversal, or an
equivalent representation.

#### Scenario: Route two subtrees to different projects

- GIVEN `services/payments/**` maps to `payments`
- AND `apps/storefront/**` maps to `storefront`
- WHEN both trees are resolved
- THEN every payments path resolves to the payments project
- AND every storefront path resolves to the storefront project

#### Scenario: Most-specific valid rule wins

- GIVEN a repository-wide rule and a more-specific rule for `services/payments/**`
- WHEN `services/payments/docs/adr.md` is resolved
- THEN the payments rule wins
- AND the explanation identifies both the winning rule and why it was more specific

#### Scenario: Equal-specificity ambiguity fails closed

- GIVEN two different projects have matching rules of equal specificity for one path
- WHEN that path is resolved
- THEN resolution fails as ambiguous
- AND no default project is used to hide the conflict

#### Scenario: Reject a path that escapes the repository

- GIVEN a path or pattern resolves outside the Git root
- WHEN it is validated or resolved
- THEN the operation fails before reading that external target

### Requirement: Explicit And Explainable Defaults

A configuration MAY declare one default project. A path with no matching routing rule MUST resolve
to that project only when the default is explicitly declared; otherwise it MUST be `unmapped`.
Every successful resolution MUST expose the project, optional client, matched rule or default, config
source, and config content hash without exposing an absolute user-home path in persisted provenance.

#### Scenario: Explicit default handles an unmatched path

- GIVEN a valid default project is declared
- AND a repo-relative path matches no project rule
- WHEN the path is resolved
- THEN it resolves to the declared default
- AND the explanation identifies the default rather than a path rule

#### Scenario: Missing default leaves path unmapped

- GIVEN no default project is declared
- AND a path matches no routing rule
- WHEN it is resolved
- THEN the result is `unmapped`
- AND a write-capable consumer requires an explicit destination before proceeding

### Requirement: Destination Precedence And Auditable Overrides

An explicit project or client supplied by an operator MUST take precedence over config resolution.
Config path routing MUST take precedence over the repository default. Consumers MUST validate an
explicit override and MUST report both the override and the config-derived destination it replaced.

#### Scenario: Explicit project overrides path routing

- GIVEN a path resolves to `prj-payments`
- AND the operator explicitly supplies `--project prj-platform`
- WHEN the destination is selected
- THEN `prj-platform` is selected
- AND provenance records that an explicit override replaced `prj-payments`

#### Scenario: Invalid explicit destination does not fall back

- GIVEN an explicit project override is malformed or rejected by the backend
- WHEN the operation starts
- THEN it fails
- AND it does not silently retry using the config-derived or default project

### Requirement: Migration Routing Completes Before Classification

`migrate-knowledge` MUST resolve the complete scanned inventory before invoking a classifier,
opening a migration run, or submitting candidates. Dry-run MUST report counts and paths grouped by
resolved project and MUST report all unmapped or ambiguous items.

If a scan resolves to multiple projects, the migrator MUST create a separate immutable
`MigrationRun` for each project. Every candidate in a run MUST originate from an item resolved to
that run's project.

#### Scenario: Dry-run explains a multi-project scan

- GIVEN a repository containing items for two configured projects and one unmapped item
- WHEN the migrator runs with `--dry-run`
- THEN it reports item counts for both projects
- AND it reports the unmapped item and its reason
- AND it performs no classification
- AND it creates no migration run

#### Scenario: Real scan creates one run per project

- GIVEN all scanned items resolve unambiguously to two projects
- WHEN the migrator publishes the result
- THEN it creates exactly two migration runs
- AND each run carries its own immutable project and client IDs
- AND no candidate is submitted to a run for a different project

#### Scenario: Routing error prevents model spend

- GIVEN at least one scanned item has ambiguous routing
- WHEN a non-dry migration is requested
- THEN the migrator fails before invoking the classifier
- AND it creates no migration run
- AND it identifies every routing error that can be determined from the inventory

### Requirement: Configuration Snapshot In Migration Provenance

Every migration run derived from repository config MUST attest the config schema version, a
cryptographic hash of the exact config bytes used, the repository-relative config location, and the
routing rules or overrides that selected its destination. The migrator MUST detect a config change
between initial resolution and publication and MUST abort rather than publish against a different
snapshot.

#### Scenario: Run records reproducible config provenance

- GIVEN a migration uses `.nexusmind.yaml`
- WHEN its project run is created
- THEN the run attestation contains the schema version and content hash
- AND it identifies the routing basis for that project
- AND it contains neither config secrets nor an absolute user-home path

#### Scenario: Config changes during migration

- GIVEN the config was hashed before scanning
- AND its bytes change before the first migration run is created
- WHEN publication begins
- THEN publication aborts
- AND no run is created from the stale resolution

### Requirement: TUI Presents Resolved Destinations

The migration TUI MUST load the same configuration contract and resolver as the CLI. Before running,
it MUST show every resolved project, its client when present, item count, routing source, and any
unmapped or ambiguous items. Manually entered destination values MUST follow the same override and
provenance rules as CLI flags.

#### Scenario: TUI previews several project runs

- GIVEN a selected source path resolves to three projects
- WHEN the operator reaches confirmation
- THEN the TUI previews three planned migration runs with their item counts
- AND the operator can inspect routing warnings before classification or publication

#### Scenario: CLI and TUI resolve identically

- GIVEN identical config bytes, repository root, and source paths
- WHEN the CLI and TUI resolve them
- THEN they produce the same project, client, status, and matched-rule result for every path

### Requirement: Agent Capabilities Reduce Local Tool Exposure

The config MAY define named agent profiles containing capabilities from a versioned, validated
vocabulary. Capabilities MUST distinguish functional domain and operation where applicable, such as
`memory.read`, `memory.write`, `sdd.read`, `sdd.write`, and `migration.run`.

A selected profile MAY reduce which NexusMind tools are exposed to an agent. It MUST NOT grant a
backend permission, override RBAC, or make a forbidden tool call succeed. Unknown capabilities and
cyclic profile inheritance MUST be rejected.

#### Scenario: Read-only profile hides writes

- GIVEN a selected profile enables `memory.read` and disables `memory.write`
- WHEN the MCP tool catalog is constructed
- THEN memory read tools remain discoverable
- AND memory write tools are not exposed by that profile

#### Scenario: Local capability cannot grant permission

- GIVEN a profile enables `sdd.write`
- AND the caller lacks backend permission `sdd:write`
- WHEN the caller attempts an SDD write
- THEN the backend rejects it
- AND the config does not change the caller's authorization

#### Scenario: Reject an unknown capability

- GIVEN an agent profile contains a capability outside the supported vocabulary
- WHEN the config is validated
- THEN validation fails with the unknown capability name
- AND the consumer does not silently ignore it

### Requirement: Cross-Implementation Compatibility

The Rust consumers in `nexusmind` and the TypeScript MCP implementation MUST conform to one
canonical version-one schema and shared conformance fixtures. For every fixture, both
implementations MUST agree on validity and, for valid routing fixtures, on the normalized resolution
result.

#### Scenario: Rust and TypeScript resolve a fixture identically

- GIVEN a canonical valid fixture containing overlapping project patterns and an explicit default
- WHEN both implementations validate and resolve its test paths
- THEN they return byte-equivalent normalized project, client, matched-rule, and status fields

#### Scenario: Both implementations reject an invalid fixture

- GIVEN a canonical fixture with an unknown field, ambiguous mapping, escape path, or inheritance cycle
- WHEN both implementations validate it
- THEN both reject it
- AND each diagnostic identifies the same invalid semantic condition

### Requirement: Backward Compatibility Without Silent Inference

Repositories without `.nexusmind.yaml` MUST retain the existing explicit-argument workflow. Existing
`--client`, `--project`, API URL, API key, DSN, include, and exclude inputs MUST remain supported.
Absence of config MUST NOT cause a consumer to infer a backend project from the directory name or Git
remote without an explicit operator action.

#### Scenario: Existing migrator invocation remains valid

- GIVEN a repository has no `.nexusmind.yaml`
- WHEN the operator invokes the migrator with the existing required destination arguments
- THEN the migration behaves as before this change
- AND no config is required

#### Scenario: Repository name is not an implicit project

- GIVEN a repository has no config and no explicit project
- WHEN a write-capable consumer needs a project destination
- THEN it reports that the destination is unresolved
- AND it does not submit the repository directory name as a project ID
