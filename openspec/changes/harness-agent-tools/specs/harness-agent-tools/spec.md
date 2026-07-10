# Harness Agent Tools Specification

## Purpose

Give agents (Claude Code, Codex, Cursor) an approval-first MCP surface to recommend, plan-install, apply-install, create, and publish shared harnesses, without adding new authority beyond existing `harness:*` backend permissions and without any silent local file mutation.

## Requirements

### Requirement: Permissioned Read Tools Are Metadata-Only

The system MUST expose `recommend_harnesses(target?)`, `list_harnesses`, `get_harness_version`, and `list_harness_config_reviews` as MCP tools gated by `harness:read`, and MUST NOT download or return installable manifest content from any of these tools.

#### Scenario: Recommend harnesses without manifest download

- GIVEN an agent session holds `harness:read` for an organization
- WHEN the agent calls `recommend_harnesses` with an optional `target`
- THEN the tool returns harness metadata (id, format, owner, version, warning metadata) filtered to accessible harnesses
- AND no manifest content or file bytes are downloaded or returned

#### Scenario: List and inspect without download

- GIVEN an agent session holds `harness:read`
- WHEN the agent calls `list_harnesses` or `get_harness_version`
- THEN the tool returns catalog/version metadata only
- AND does not fetch or expose manifest component content

#### Scenario: List config reviews requires permission

- GIVEN an agent session lacks `harness:read`
- WHEN the agent calls `list_harness_config_reviews`
- THEN the tool MUST deny the call
- AND MUST NOT return any config review metadata

### Requirement: Plan Install Produces a Diff and Writes Nothing

The system MUST implement `plan_harness_install(harness_id, version, target_tool, scope)` to download the immutable manifest, resolve per-tool destination paths for each component, and return a diff describing destination path, create/overwrite status, and executable warnings per file. `plan_harness_install` MUST NOT write to any local path.

#### Scenario: Plan install returns full diff

- GIVEN a published harness version is accessible to the caller
- WHEN the agent calls `plan_harness_install` with a supported `target_tool` and `scope`
- THEN the response lists every component with its resolved destination path and create-vs-overwrite status
- AND no file is written to disk as part of this call

#### Scenario: Executable component flagged in plan

- GIVEN the manifest includes a `hook` or `claude_code_plugin` component
- WHEN `plan_harness_install` resolves the diff
- THEN the affected entries are marked with an executable/plugin warning
- AND the response indicates that acknowledgement will be required to apply

#### Scenario: Unsupported format-to-tool pair refused at plan time

- GIVEN a manifest format has no valid destination mapping for the requested `target_tool`
- WHEN the agent calls `plan_harness_install` for that pair
- THEN the system MUST refuse the plan with a validation error naming the unsupported format/tool combination
- AND MUST NOT return a partial or best-effort diff

### Requirement: Apply Install Requires Confirmation and Records Result

The system MUST implement `apply_harness_install(...)` to run only after explicit user confirmation of the plan, MUST record `approve_install` with the manifest hash and warning acknowledgement (when required) before writing, MUST materialize files to disk exactly as diffed, and MUST call `record_install_result` after writing.

#### Scenario: Apply after confirmation writes and records

- GIVEN a user has reviewed and confirmed a `plan_harness_install` diff
- WHEN the agent calls `apply_harness_install` with that confirmation
- THEN the system records `approve_install` with the manifest hash
- AND writes each file to its resolved destination exactly as diffed
- AND calls `record_install_result` after writing completes

#### Scenario: Executable format requires explicit acknowledgement

- GIVEN the plan includes a `hook` or `claude_code_plugin` component with an executable warning
- WHEN the agent calls `apply_harness_install` without an explicit warning-acknowledgement flag
- THEN the system MUST refuse to write any file
- AND MUST NOT call `record_install_result`

#### Scenario: Apply refuses without prior plan confirmation

- GIVEN no confirmed plan or manifest hash is supplied
- WHEN the agent calls `apply_harness_install` directly
- THEN the system MUST reject the call
- AND MUST NOT write any file or record an install result

#### Scenario: Manifest hash mismatch blocks apply

- GIVEN the manifest hash at apply time differs from the hash confirmed during planning
- WHEN the agent calls `apply_harness_install`
- THEN the system MUST reject the write
- AND MUST require a new `plan_harness_install` and confirmation before retrying

### Requirement: Format-to-Tool Applicability Matrix Governs Installs

The system MUST enforce a format-to-tool applicability matrix at plan time, distinguishing Claude-centric formats (`skill`, `output_style`, `claude_code_plugin`) from formats portable across Claude Code, Codex, and Cursor (`agent`, `command`, `hook`, `file`, `folder`, `theme` where applicable to the target).

#### Scenario: Claude-only format rejected for non-Claude target

- GIVEN a manifest declares `skill` or `output_style` format
- WHEN the agent calls `plan_harness_install` with `target_tool` set to `codex` or `cursor`
- THEN the system MUST refuse the plan
- AND MUST state that the format is Claude Code-only

#### Scenario: Portable format resolves for each supported tool

- GIVEN a manifest declares a format valid for all three tools (for example `agent` or `command`)
- WHEN the agent calls `plan_harness_install` once per `target_tool` in `{claude, codex, cursor}`
- THEN each call resolves distinct, tool-appropriate destination paths
- AND each plan succeeds without cross-tool path leakage

#### Scenario: Cursor destination resolution

- GIVEN a supported format for Cursor
- WHEN the agent calls `plan_harness_install` with `target_tool` set to `cursor`
- THEN destinations resolve under the Cursor project configuration directory (`.cursor/`)
- AND the diff reflects Cursor-specific paths, not Claude or Codex paths

#### Scenario: Codex destination resolution uses conservative default

- GIVEN a supported format for Codex
- WHEN the agent calls `plan_harness_install` with `target_tool` set to `codex`
- THEN destinations resolve under the documented conservative Codex default (`~/.codex/`)
- AND ambiguous or undocumented component types are refused rather than guessed

### Requirement: Build Manifest From Local Path

The system MUST implement `build_harness_manifest_from_path(path, format, targets)`, gated by `harness:write`, to read local files at the given path, compute a sha256 hash per component, inline content up to 64KiB per component, run a secret scan, and produce a valid `schema_version` 1.1 manifest (`format`, `targets`, `components`, `provenance`, `security`).

#### Scenario: Build a valid manifest from local files

- GIVEN local files exist at the given path matching the declared `format`
- WHEN the agent calls `build_harness_manifest_from_path`
- THEN the resulting manifest includes `schema_version: 1.1`, `format`, `targets`, `components` with sha256 and inlined content, `provenance`, and `security`
- AND each inlined component is at most 64KiB

#### Scenario: Refuse manifest build on secret-scan hit

- GIVEN a local file at the given path contains a detected secret indicator
- WHEN the agent calls `build_harness_manifest_from_path`
- THEN the system MUST refuse to produce the manifest
- AND MUST NOT inline or hash the offending content into any returned artifact

#### Scenario: Component exceeding inline limit is rejected

- GIVEN a local file exceeds 64KiB
- WHEN the agent calls `build_harness_manifest_from_path`
- THEN the system MUST reject that component with a size validation error
- AND MUST NOT silently truncate the content

### Requirement: Create and Publish Wrappers Require Write Permission

The system MUST implement `create_harness` and `publish_harness_version` as thin, permissioned wrappers over existing backend endpoints, gated by `harness:write`, adding no authority beyond that scope.

#### Scenario: Create harness with write permission

- GIVEN an agent session holds `harness:write` for an organization
- WHEN the agent calls `create_harness` with valid metadata
- THEN the harness is created under that organization
- AND the response includes the harness identifier

#### Scenario: Publish version from a built manifest

- GIVEN a valid manifest was produced by `build_harness_manifest_from_path`
- WHEN the agent calls `publish_harness_version` with that manifest
- THEN the version is published immutably with a manifest hash
- AND the response matches `harness-library` publish behavior

#### Scenario: Deny create or publish without write permission

- GIVEN an agent session lacks `harness:write`
- WHEN the agent calls `create_harness` or `publish_harness_version`
- THEN the system MUST deny the call
- AND MUST NOT create or publish any harness data
