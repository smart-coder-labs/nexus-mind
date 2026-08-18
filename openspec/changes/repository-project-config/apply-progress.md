# Apply Progress — Repository Project Configuration

> **Change:** `repository-project-config`
> **Started:** 2026-08-17
> **Status:** implementation complete; verification/review in progress

## Completed in the current apply pass

- Added the canonical v1 JSON Schema and initial cross-language fixture manifest.
- Added Rust dependencies `serde_yaml` and `globset`.
- Added `repository_config` library module with:
  - typed version-one model and unknown-field rejection;
  - semantic project/default/profile/capability validation;
  - effective capability inheritance with disable lists and cycle detection;
  - exact-byte SHA-256 `ConfigSnapshot` and path-safe attestation view;
  - explicit and Git-root-bounded config discovery;
  - symlink/canonical-path containment checks;
  - restricted glob compilation, deterministic specificity, exclusions, default, unmapped, and
    cross-project ambiguity detection.
- Added thirteen focused Rust tests; all pass.
- Added optional repo-relative `routing_path` to `SourceItem` and populated it for repo docs and
  Claude filesystem assets. Pathless sources explicitly carry `None`.
- Confirmed the `migrate-knowledge` test binary compiles after the connector contract change.
- Added redacted rejection for secret/executable fields, YAML alias rejection, profile-cycle tests,
  and explicit destination override validation.
- Added pure inventory planning (`plan_paths`) that groups item indices by immutable destination and
  accumulates unmapped indices before any classifier or HTTP dependency is involved.
- Wired `--config` and `--require-config` into `migrate-knowledge` discovery and planning.
- The runner now resolves immediately after scanning. Non-dry execution fails before classification
  on unmapped inventory, classifies each destination independently under one global budget, rechecks
  the config before publication, and creates/stages one run per destination.
- Added config arguments to migrator TUI `RunConfig`; the TUI delegates resolution to the child.
- Added structured config/routing/run events and mirrored them in the TUI protocol/log view.
- Added the TypeScript config consumer to `nexusmind-mcp`, including bounded discovery, strict keys,
  routing, profile inheritance, capability filtering, and registry filtering for reduced profiles.
- Added operator documentation in `docs/REPOSITORY_CONFIG.md`.

## Evidence

```text
cargo test --manifest-path apps/backend/Cargo.toml repository_config --lib
13 passed; 0 failed

cargo test --manifest-path apps/backend/Cargo.toml --bin migrate-knowledge --no-run
success

cargo run --quiet --manifest-path apps/backend/Cargo.toml --bin migrate-knowledge -- \
  --source noop --path . \
  --config schemas/fixtures/nexusmind-config/v1/valid/multi-project.yaml \
  --dry-run --no-llm
routing — groups=1 unmapped=0; no classification or post

cargo test --manifest-path apps/migrator-tui/Cargo.toml
119 passed; 0 failed

npm test (nexusmind-mcp isolated worktree)
290 passed; 0 failed

npm run build (nexusmind-mcp isolated worktree)
success
```

Cargo required network access for the newly declared crates; the user approved the scoped
`cargo test` command prefix. `Cargo.lock` now pins `serde_yaml` and `unsafe-libyaml`; `globset` was
already present transitively and is now a direct dependency.

## Deviations and discoveries

- The design described `routing_path` as required, but DB schema, host-scope assets, noop inputs, and
  the current Git commit model legitimately lack one. The implementation uses `Option<String>`;
  `None` must resolve through an explicit/default destination or remain unmapped.
- `git_history::Commit` currently has no changed-file list. Cross-project commit splitting cannot be
  implemented by routing metadata alone; T-31 must extend the Git reader/protocol first. Until then,
  git-history items are pathless and require a default/override.
- The fixture catalog currently covers the primary valid/invalid version and routing contract. The
  Rust unit suite carries the remaining safety and semantic edge cases; expanding every edge into a
  standalone cross-language file remains follow-up fixture-governance work.

## Next apply checkpoint

1. Run final formatting, compilation, full relevant suites, and diff checks.
2. Start or reuse the mandatory content-bound post-apply review receipt.
3. Record review findings and final verification evidence.
