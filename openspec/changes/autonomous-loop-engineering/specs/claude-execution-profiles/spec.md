# Claude Execution Profiles Specification

## Purpose

Define organization-managed, versioned authority for the Claude Code MVP.

## Requirements

### Requirement: Provider and Profile Authorization

The system MUST permit only Claude Code in the MVP. An organization profile MAY be allowed by a project policy only when the project binding is active; project policy MUST be no broader than organization policy. Repositories MUST NOT self-authorize profiles or widen their authority.

#### Scenario: Authorized profile selection

- GIVEN an active organization profile allowlisted for a project
- WHEN an authorized run requests that profile
- THEN the system leases its pinned version

#### Scenario: Unauthorized selection

- GIVEN a profile absent from the project allowlist or a different provider
- WHEN execution is requested
- THEN the system MUST deny execution and record the reason

### Requirement: Fixed Profile Semantics

`read-only` MUST prohibit repository writes, PR publication, deployment handoff, and write-capable credentials. `implementation` MAY create only policy-approved PRs and MUST NOT merge or deploy. `qa-deploy` MUST invoke only the existing approved QA handoff and preserve its human validation; it MUST NOT replace QA behavior or deploy production.

#### Scenario: Read-only write attempt

- GIVEN a `read-only` attempt
- WHEN it requests a repository write or PR publication
- THEN the system MUST deny the action

#### Scenario: QA handoff

- GIVEN an eligible `qa-deploy` run with passing required gates
- WHEN it reaches its approved handoff
- THEN existing QA evidence and human validation remain required

### Requirement: Pinned Execution Surface

A profile version MUST pin its model, approved MCP servers, plugins, skills, hooks, tools, settings, network destinations, ephemeral credentials, output schema, and turn, timeout, and cost caps. MCP configuration MUST be strict. Unpinned, unavailable, failing, or repository-defined extensions MUST be unavailable to the attempt and MUST fail closed when required.

#### Scenario: Unapproved extension

- GIVEN a worker configuration containing an unpinned plugin or MCP server
- WHEN the attempt starts
- THEN it MUST not load that extension and records the denial

#### Scenario: Required extension fails

- GIVEN a required pinned extension fails validation or startup
- WHEN execution begins
- THEN the attempt MUST fail without fallback to an unapproved extension
