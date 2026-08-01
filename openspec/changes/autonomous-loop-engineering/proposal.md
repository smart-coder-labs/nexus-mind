# Proposal: Autonomous Loop Engineering

## Intent

Enable NexusMind to turn requirements into traceable engineering work while keeping policy and QA controls outside the model runtime. The MVP uses non-interactive Claude Code in isolated workers.

## Scope

### In Scope
- GitHub App policy, durable runs, gates, evaluation, PR traceability, and Automation dashboard.
- Claude Code executed only as an isolated child process (`--print` with stream JSON) or through the official Agent SDK; never as an interactive terminal.
- Managed, versioned, allowlisted execution profiles: `read-only`, `implementation`, and `qa-deploy`. Project policy selects permitted profiles.
- Profiles bind model, turn/cost/time limits, tools, settings, strict MCP, approved extensions, network, credentials, and output schema.
- Immutable provenance for profile/version, inputs, tools, costs, gates, decisions, and GitHub actions.

### Out of Scope
- Providers other than Claude Code, arbitrary repository-defined extensions, bypass permissions, host execution, or unbounded runs.
- Autonomous production merge/deployment or replacing existing QA deployment and human QA behavior.

## Capabilities

### New Capabilities
- `autonomous-run-orchestration`: Governed runs, workers, gates, evaluation, cancellation, and PR evidence.
- `claude-execution-profiles`: Versioned organization-managed profiles and project allowlists.
- `automation-provenance-governance`: Receipts, cost ledger, access controls, and stop/revocation.
- `automation-governance-dashboard`: Configuration, evidence, and emergency controls.

### Modified Capabilities
None; no living specifications exist yet.

## Approach

The Rust/Axum control plane is the sole policy, credential, and GitHub App authority. It leases isolated workers a managed profile; workers cannot self-authorize extensions, credentials, or merges. `read-only` cannot write; `implementation` may create policy-approved PRs; `qa-deploy` preserves current QA behavior and only invokes its approved handoff.

## Affected Areas

| Area | Impact | Description |
|---|---|---|
| `apps/backend/src/automation/` | New | Policy, profiles, leases, receipts, GitHub control |
| `apps/backend/src/db/` | Modified | User-applied automation/profile/audit records |
| `workers/src/` | New | Isolated Claude Code execution adapter |
| `apps/admin/src/pages/Automation.tsx` | New | Policy, cost, and evidence controls |

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Extension or credential escalation | Med | Managed allowlists, strict MCP, scoped ephemeral credentials |
| Cost/runaway execution | Med | Profile caps, leases, cancellation, cost ledger |
| QA regression | Low | Preserve QA flow; explicit `qa-deploy` handoff and evidence |

## Rollback Plan

Disable profiles, increment policy generation, cancel leases, revoke credentials/App access, and retain receipts. Revert QA changes through the existing process.

## Dependencies

- Claude Code/official Agent SDK, GitHub App, isolated worker substrate, existing QA deployment, authorized administrators.

## Success Criteria

- [ ] Only project-allowed managed profiles execute Claude Code non-interactively.
- [ ] No repository can authorize arbitrary extensions or bypass permissions.
- [ ] Every run records profile provenance, costs, tool/evidence receipts, and final decision.
- [ ] Existing QA behavior remains intact.
