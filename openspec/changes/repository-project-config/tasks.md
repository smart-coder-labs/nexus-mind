# Tasks — Repository Project Configuration

**Change:** `repository-project-config`
**Project:** `nexus-mind`

`strict_tdd: true` applies to backend and migrator work. Every behavior task starts RED and lands
GREEN before the next phase. Existing user changes in the original checkout are out of scope; all
implementation remains in the isolated worktree.

---

## Review Workload Forecast

| Field | Value |
|---|---|
| Estimated authored changed lines | 1,800–2,600 across two repos |
| Risk classification | High: >400 lines, config/routing, filesystem boundaries, process integration |
| Delivery strategy | Chained changes: contract/resolver → migrator/TUI → MCP |
| Ordinary review | Explicit `review/start(target)` after apply because no valid receipt exists |
| Initial review lenses | Four 4R sweeps: readability, reliability, resilience, risk |

Do not combine the Rust resolver, migration activation, and npm MCP port into one unreviewable commit.
Each phase below must be independently testable and leave explicit CLI behavior working.

---

## Phase 1: Canonical schema and fixtures

- [ ] T-01 RED: establish fixture harness shape
  - Files: `schemas/fixtures/nexusmind-config/v1/cases.json`, backend test module.
  - Add a test that enumerates every case and fails because no schema/parser exists yet.
  - Each case records `fixture`, `valid`, optional `error_code`, and routing expectations.

- [ ] T-02 GREEN: add `schemas/nexusmind-config-v1.schema.json`
  - Strict `additionalProperties: false` at every object.
  - Define version, repository, defaults, projects, routing paths/excludes, agent profiles,
    `extends`, `capabilities`, and `disable_capabilities`.
  - IDs/aliases get the length and slug constraints from design; backend IDs remain opaque strings.

- [ ] T-03 RED/GREEN: valid fixtures
  - Minimal single-project config.
  - Multi-project config with default and excludes.
  - Profile inheritance with a read-only child.
  - Config with no defaults and config with no agents section.

- [ ] T-04 RED/GREEN: structurally invalid fixtures
  - Unknown version and unknown field at each object level.
  - Empty/invalid alias, empty backend ID, duplicate list value, missing project path.
  - Unknown default project/profile and missing profile parent.
  - YAML duplicate keys, aliases/anchors, and non-string keys.

- [ ] T-05 RED/GREEN: safety-invalid fixtures
  - Secret/credential/command-shaped field names.
  - Absolute paths, `..`, backslashes, unsupported glob constructs, and symlink-escape case metadata.
  - Unknown capability, capability in both allow/disable lists, and inheritance cycle.

- [ ] T-06: document fixture governance
  - Schema and fixtures are canonical in `nexusmind`.
  - Add a README stating that consumers must match fixture validity, error semantics, and normalized
    routing output; fixture edits require a schema version decision.

**Gate 1:** fixture inventory is reviewable; schema validation tests pass; no runtime consumer yet.

---

## Phase 2: Rust parser and configuration snapshot

- [ ] T-07 RED: typed parse of the minimal fixture
  - Files: `apps/backend/src/repository_config/{mod.rs,parse.rs,tests.rs}`, `src/lib.rs`.
  - Assert typed aliases, IDs, paths, defaults, and profiles.

- [ ] T-08 GREEN: add parser dependencies and structs
  - Add `serde_yaml` only where required.
  - Use `deny_unknown_fields`; do not deserialize into a permissive `Value` as the final model.
  - Define stable `ConfigError` codes from design.

- [ ] T-09 RED/GREEN: reject YAML ambiguity before typed deserialization
  - Duplicate keys fail instead of last-key-wins.
  - YAML aliases/anchors fail.
  - Diagnostics identify a field/path without echoing rejected raw values.

- [ ] T-10 RED/GREEN: semantic validation
  - Unique project IDs/aliases, valid default references, non-empty path lists.
  - Known capabilities, valid parent references, no allow/disable collision.
  - DFS profile-cycle detection returns the complete alias chain.

- [ ] T-11 RED/GREEN: exact-byte snapshot hash
  - `ConfigSnapshot.sha256` is computed from exact file bytes with the existing `sha2` stack.
  - Same semantic YAML with different bytes has a different hash.
  - Serialization view excludes root/canonical absolute paths.

- [ ] T-12 RED/GREEN: secret-field rejection and redacted errors
  - Assert all prohibited field categories from the spec.
  - Source scanning confirms diagnostics never interpolate YAML values for these errors.

- [ ] T-13: execute all canonical validity fixtures in Rust
  - Every valid fixture parses; every invalid fixture returns its expected stable condition.

**Gate 2:** `cargo test --manifest-path apps/backend/Cargo.toml repository_config::parse`.

---

## Phase 3: Git-bounded discovery

- [ ] T-14 RED: nested discovery finds the nearest config inside one Git root
  - Test with a real temporary Git repository and nested source directory.

- [ ] T-15 RED: discovery stops at Git root
  - Parent `.nexusmind.yaml` is ignored.
  - A non-Git directory does not fall back to `$HOME` or filesystem parents.

- [ ] T-16 RED: explicit selection wins and is repository-bounded
  - Explicit valid config overrides discovered config.
  - Explicit path outside root and symlink target outside root fail with `CONFIG_OUTSIDE_REPOSITORY`.

- [ ] T-17 GREEN: implement `discovery.rs`
  - Resolve from source `--path`, not process cwd.
  - Invoke Git without a shell and capture failure as a typed diagnostic.
  - Canonicalize roots/targets before containment checks.

- [ ] T-18 RED/GREEN: `--require-config` behavior
  - Missing config is `Ok(None)` normally and `CONFIG_NOT_FOUND` when required.

**Gate 3:** parser + discovery suite; no migrator wiring.

---

## Phase 4: Deterministic Rust router

- [ ] T-19 RED: supported pattern syntax
  - Literal segments, segment `*`, segment `?`, and whole-segment `**` match canonical paths.
  - Add `globset` only after tests demonstrate the required semantics.

- [ ] T-20 RED: reject unsupported or escaping patterns
  - Character classes, braces, extglobs, leading slash, empty/dot/parent segments, and config
    backslashes return `ROUTING_INVALID_PATTERN`.

- [ ] T-21 RED: specificity tuple is declaration-order independent
  - Literal segments/chars outrank wildcards exactly as design specifies.
  - Reverse project and pattern declaration order; normalized resolution remains identical.

- [ ] T-22 RED: project-local exclusion
  - Excluding a path from one project does not suppress a valid match in another.
  - An excluded root match can fall through to explicit default.

- [ ] T-23 RED: default, unmapped, and ambiguity
  - Explicit default handles no match.
  - No default yields `Unmapped`.
  - Equal best specificity across projects yields `ROUTING_AMBIGUOUS`, never default.

- [ ] T-24 RED: explanation is stable and non-secret
  - Result includes alias, IDs, selected pattern/score or default, config relative path/hash.
  - Persistable view has no absolute path.

- [ ] T-25 GREEN: implement compiled `ProjectResolver`
  - Compile once per snapshot.
  - Stable tie-breaking only within the same project; cross-project tie is an error.

- [ ] T-26 RED/GREEN: explicit destination overrides
  - Project override wins and records configured destination it replaced.
  - Client override without a project is invalid.
  - Invalid override never falls back.

- [ ] T-27: run all canonical routing cases in Rust
  - Serialize a normalized result shape suitable for byte-level comparison in TypeScript.

**Gate 4:** complete config library passes without modifying migrator behavior.

---

## Phase 5: Make connector items routable

- [ ] T-28 RED: `SourceItem` requires `routing_path`
  - Update constructor fixtures first so compiler failures enumerate every connector/call site.
  - `display_origin` remains presentation-only and is never parsed as a path.

- [ ] T-29 GREEN: repo-docs routing paths
  - Every section carries its document's normalized repo-relative path.
  - Existing identity/display behavior remains unchanged.

- [ ] T-30 GREEN: Claude-memory routing paths
  - Project-scoped assets use recorded repo paths.
  - Host-scope/global assets carry no route and require default/override at inventory resolution.

- [ ] T-31 RED/GREEN: git-history project splitting
  - A commit touching one project yields one item.
  - A commit touching two projects yields one item per project with only relevant file paths.
  - Split identities retain commit SHA plus project routing provenance and are stable across rescans.
  - Commits without usable paths use default/unmapped behavior.

- [ ] T-32 GREEN: db-schema and noop routing behavior
  - DB schema is pathless and requires default/override.
  - Noop fixtures declare an explicit deterministic routing path or test override as appropriate.

- [ ] T-33: regression gate for all connector suites
  - No source identity acquires an absolute path.
  - Existing candidate classification/fallback semantics are unchanged.

---

## Phase 6: Migrator planning before classification

- [ ] T-34 RED: CLI accepts `--config` and `--require-config`
  - Existing `--project`, `--client`, config-free and dry-run invocations remain compatible.

- [ ] T-35 RED: complete inventory resolves before classifier invocation
  - Inject a classifier spy and ambiguous/unmapped inventory.
  - Assert zero model calls and zero HTTP calls on routing failure.

- [ ] T-36 RED: dry-run reports grouped routing
  - Two project groups plus unmapped/ambiguous counts.
  - No classification and no backend writes.
  - Bounded samples, complete totals.

- [ ] T-37 GREEN: introduce `MigrationPlan`
  - Load/hash config, scan, resolve/split inventory, accumulate all issues.
  - Sort groups by project ID; preserve scan order inside each group.
  - Separate planning from execution so tests do not require network or Claude.

- [ ] T-38 RED/GREEN: explicit legacy path
  - No config + explicit destination creates one group exactly as before.
  - No config + no destination may dry-run but cannot publish.
  - Directory/repo name is never submitted as an implicit project ID.

- [ ] T-39 RED: global token budget spans groups
  - Classifying the first group spends from one budget.
  - Later groups do not receive a reset and stop when the global ceiling trips.

- [ ] T-40 GREEN: classify by planned group
  - Preserve bulk/parallel/no-LLM behavior inside groups.
  - Do not mix candidates across destinations.

**Gate 6:** all plan/classification tests pass while publication can remain explicit-only behind an
internal switch during implementation.

---

## Phase 7: Per-project publication and provenance

- [ ] T-41 RED: two groups create two run requests
  - Assert exact `project_id`, `client_id`, source kind/ref, and isolated candidate batches.
  - Assert no backend schema change is required.

- [ ] T-42 RED: config attestation shape
  - Version, repository ID, relative config path, exact-byte hash, alias/IDs, selection, and patterns.
  - Explicit override includes configured destination it replaced.
  - No absolute root or secret-shaped value.

- [ ] T-43 RED: config mutation aborts before first POST
  - Change bytes after plan/classification; expect `CONFIG_CHANGED`, zero created runs.

- [ ] T-44 GREEN: publish planned groups sequentially
  - Re-read/hash once immediately before publication.
  - Create one immutable run per group and stage only its candidates.
  - Emit each run ID as soon as created.

- [ ] T-45 RED/GREEN: partial HTTP failure is explicit
  - First group succeeds, second fails.
  - Process exits non-zero and reports already-created run IDs.
  - No auto-cancel, commit, deletion, or hidden retry of the first run.

- [ ] T-46 RED/GREEN: backend authorization remains final
  - Foreign/mismatched project/client receives existing backend error.
  - Migrator does not retry against default or another route.

- [ ] T-47: compatibility regression
  - Existing single-project explicit invocation produces the same run body except additive
    attestation/event metadata when a config is used.

---

## Phase 8: NDJSON protocol and migrator TUI

- [ ] T-48 RED: backend event serialization
  - `config_loaded`, `routing_group`, `routing_issue`, `routing_ready`, `run_created` exact wire shapes.
  - Bound `sample_paths`; full source contents never enter events.

- [ ] T-49 RED: TUI protocol parser mirrors all new events
  - Unknown event still fails visibly rather than being silently discarded.

- [ ] T-50 RED/GREEN: `RunConfig` arguments
  - Add config path and require-config.
  - Manual client/project fields remain explicit overrides.
  - Display command is safe; API key/DSN behavior remains redacted/out of argv.

- [ ] T-51 RED: TUI routing preview state
  - Render project/client, item count, routing source, and issue totals.
  - Confirmation is blocked on ambiguity/unmapped publication.
  - Dry preview itself creates no migration run.

- [ ] T-52 GREEN: wire preview through child runner
  - TUI does not parse YAML or reproduce specificity logic.
  - Cancellation still kills the child and cannot leave a classifier spending tokens.

- [ ] T-53: real-binary protocol contract test
  - Build/run the real migrator fixture and prove CLI/TUI event compatibility.
  - Include multi-project fixture rather than only the existing single ADR scan.

**Gate 8:** backend binary tests + full migrator-TUI tests.

---

## Phase 9: Documentation and read-only validator

- [ ] T-54 RED/GREEN: add a validation/explanation mode
  - Prefer `migrate-knowledge --validate-config` in v1 rather than a second binary.
  - Output stable JSON on request: config validity, groups, issues, and resolution explanations.
  - It performs no model or backend write.

- [ ] T-55: operator documentation
  - Document schema, multi-project example, routing specificity, override precedence, dry-run, CI use,
    secret prohibition, and backward compatibility.
  - Explain that capabilities are exposure filters and not permissions.

- [ ] T-56: add this repository's example fixture, not an active production config
  - Do not invent backend project/client IDs or commit a live `.nexusmind.yaml` without owner input.

---

## Phase 10: Coordinated `nexusmind-mcp` worktree

Create a separate sibling worktree from the `nexusmind-mcp` repo, with its own `.codegraph/` index;
never copy or symlink the main repo index.

- [ ] T-57: create branch/worktree for MCP integration
  - Suggested branch: `feat/repository-project-config`.
  - Copy schema/fixtures through an explicit sync script or packaging step, not manual drift.

- [ ] T-58 RED: schema/fixture hash contract
  - npm tests assert packaged schema/fixture SHA-256 matches canonical artifacts from `nexusmind`.
  - Release build includes required schema assets.

- [ ] T-59 RED/GREEN: TypeScript parser and semantic validator
  - Add YAML and JSON-Schema dependencies deliberately.
  - Reject duplicate keys, aliases, unknowns, secrets, invalid refs/cycles, and unsupported patterns.
  - Match every canonical validity/error case.

- [ ] T-60 RED/GREEN: TypeScript resolver parity
  - Same normalization, pattern syntax, specificity tuple, exclusions, default and ambiguity.
  - Normalized routing outputs match `cases.json` byte-for-byte.

- [ ] T-61 RED: separate search keywords from required capabilities
  - Extend `ToolDefinition` with `required_capabilities`.
  - Existing descriptor `capabilities` tags either migrate to `keywords` or become explicitly typed;
    do not silently reinterpret free-form search metadata as authorization-like input.

- [ ] T-62 GREEN: annotate curated definitions
  - Map every essential/reduced tool to the versioned capability vocabulary.
  - Add a completeness test: no curated definition lacks a required-capability declaration.

- [ ] T-63 RED/GREEN: agent profile selection
  - CLI inputs: `--config`, `--project`, `--agent-profile`.
  - Precedence: explicit profile > project profile > default profile > existing unfiltered behavior.
  - Keep `NEXUSMIND_MCP_TOOL_PROFILE` independent as presentation mode.

- [ ] T-64 RED/GREEN: filter before server/fabric construction
  - Essential registers only filtered definitions.
  - Reduced constructs `ToolFabric` only from filtered definitions.
  - Fabricated hidden handle cannot load or execute.

- [ ] T-65 RED/GREEN: backend denial remains authoritative
  - Enabling a capability exposes a tool but does not make a mocked permission denial succeed.
  - Empty held-permissions behavior remains deferred to backend as today.

- [ ] T-66: legacy behavior is explicit
  - When agent capability filtering is requested with legacy tool profile, warn that v1 cannot filter
    the legacy catalog and fail closed or require explicit opt-out; do not claim filtering occurred.
  - Preserve legacy behavior when no agent profile is selected.

- [ ] T-67: npm gates and versioning
  - Register every new test file in the explicit `package.json` test script.
  - `npm test`, `npm run build`, package-content inspection, and appropriate semver bump.

---

## Phase 11: Cross-repo integration and compatibility

- [ ] T-68: run canonical fixtures in both repos at pinned commits
  - Record schema/fixture hashes and normalized result artifact.
  - Any disagreement blocks release.

- [ ] T-69: end-to-end essential profile fixture
  - Start MCP in a configured subproject with read-only agent profile.
  - Assert correct project/profile resolution, read tools present, write tools absent.

- [ ] T-70: end-to-end multi-project migrator fixture
  - Repo docs route to at least two fake backend projects.
  - Dry-run spends/writes nothing; real run produces isolated staged runs and attestations.

- [ ] T-71: backward-compatibility suite
  - Config-free migrator explicit flags.
  - Config-free MCP essential/reduced/legacy startup.
  - Existing TUI single-project path.

---

## Phase 12: Apply completion, review, and verification

- [ ] T-72: update `apply-progress.md`
  - Record each completed task, deviations, fixture/schema hashes, test evidence, and any deferred item.

- [ ] T-73: ordinary bounded implementation review
  - After apply completes, check for an existing content-bound receipt.
  - If none exists, explicitly start `review/start(target)` once.
  - High-risk classification requires four initial 4R lens sweeps: readability, reliability,
    resilience, and risk.
  - Address findings without restarting review budget; scope changes require maintainer action.

- [ ] T-74: backend verification
  - `cargo test --manifest-path apps/backend/Cargo.toml`
  - `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings`
  - Targeted `cargo fmt --check`; do not format unrelated dirty files.

- [ ] T-75: TUI verification
  - `cargo test --manifest-path apps/migrator-tui/Cargo.toml`
  - Real-runner protocol contract test with built binary.

- [ ] T-76: MCP verification
  - `npm test`
  - `npm run build`
  - Inspect packed files for schema/fixtures and absence of secrets/local paths.

- [ ] T-77: write `verify-report.md`
  - Trace every spec requirement to tests/evidence.
  - Record CodeGraph status, review receipt, remaining risks, and exact cross-repo commits.

---

## Required implementation invariants

- Routing validation completes before model spend or network writes.
- One migration run contains candidates for exactly one immutable project.
- Config never grants backend permission.
- No persisted artifact/event includes an absolute home path or raw secret-shaped value.
- CLI is the single routing authority for the migrator TUI.
- Rust and TypeScript conform to the same schema and fixture semantics.
- Existing explicit workflows remain operational without config.
- No implementation task modifies or discards unrelated dirty-worktree changes.
