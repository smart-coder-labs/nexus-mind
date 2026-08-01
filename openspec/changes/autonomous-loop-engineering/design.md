# Design: Autonomous Loop Engineering

## Technical Approach

Add an organization-scoped automation domain to the Rust/Axum control plane and a Claude Code-only isolated worker runtime. The control plane resolves immutable policy/profile snapshots, creates signed leases and scoped ephemeral credentials, and alone performs GitHub/QA writes. Workers execute `claude --print --output-format stream-json` (or an equivalent official Agent SDK adapter) non-interactively, validate streamed events, and publish receipts only.

## Architecture Decisions

| Decision | Alternatives / tradeoff | Choice and rationale |
|---|---|---|
| Provider boundary | Provider calls inside orchestration | `ExecutionProvider` with one `ClaudeCodeProvider`; it isolates CLI/SDK transport while rejecting non-Claude providers. |
| Runtime protocol | Free-form stdout or interactive TTY | Fixed argv, no shell, stream-JSON parser and final schema validation; malformed/unknown required events fail closed. |
| Authority resolution | Repository config or mutable profile | Versioned org profile plus stricter project/repository binding resolves to an immutable snapshot and generation. Repositories cannot widen it. |
| Extension surface | Local discovery/fallback | Resolve approved harness artifact/version/hash centrally; materialize only verified MCP/plugins/skills/hooks. Strict generated settings/MCP files omit repo settings and deny unpinned required extensions. |
| Isolation and secrets | Host process and long-lived tokens | Ephemeral sandbox/worktree, read-only root for `read-only`, per-attempt secret injection, egress/tool allowlists, no DB/merge token. Sandbox destruction and secret revocation follow exit. |
| Limits and stop | Best-effort post-run metering | Supervisor enforces wall time, turns, tokens, cost, tool/network calls; cancellation/profile revocation increments generation, terminates process, and blocks every receipt/GitHub action. |
| QA promotion | Worker merge or CI equals approval | Preserve the existing control-plane QA recheck, App merge, normal deployment handoff, signed webhook evidence, and separate human validation. |

## Data Flow

```text
Dashboard -> policy/profile resolver -> immutable lease + artifacts + secret refs
worker sandbox -> Claude adapter -> stream events -> usage/events/receipt API
control plane -> gates/evaluator -> final generation/branch recheck -> QA handoff
revoke/cancel -> generation increment -> process kill + write/receipt denial
```

The adapter maps normalized `started`, `assistant`, `tool`, `usage`, `result`, and `error` events to append-only provenance. It validates event ordering, lease identity, byte limits, and output schema; raw output is bounded/redacted and content-addressed. A privileged action requires an active lease and current policy generation immediately before use.

## File Changes

| File | Action | Description |
|---|---|---|
| `apps/backend/src/lib.rs`, `config.rs` | Modify | Register automation and worker/feature configuration. |
| `apps/backend/src/automation/{mod,profiles,policy,leases,provenance,github,qa}.rs` | Create | Resolver, snapshots, limits, receipts, App writes, preserved QA transition. |
| `apps/backend/src/db/{migrations,queries}.rs` | Modify | User-applied v57 profiles/versions/bindings, runs/leases, grants, usage/events, append-only receipts and revocation triggers. |
| `apps/backend/src/api/{router,automation,webhooks}.rs`, `models/types.rs` | Modify/Create | RBAC endpoints, worker callbacks, DTOs, verified GitHub evidence. |
| `workers/src/{worker,provider,claude_code,sandbox,events}.ts` | Create | Adapter, fixed CLI/SDK invocation, JSON stream parser, sandbox and secret lifecycle. |
| `apps/admin/src/{App.tsx,types.ts,api/client.ts,pages/Automation.tsx,components/Layout.tsx}` | Create/Modify | Profile/version, binding, usage/evidence, cancellation/revocation, QA controls. |

## Interfaces / Contracts

```rust
ExecutionProfileVersion { id, profile_id, version, provider: "claude-code",
 model, settings_hash, artifact_refs, tool_allowlist, egress_allowlist, caps, output_schema_hash }
RunLease { id, run_id, profile_version_id, policy_generation, expires_at, secret_grants }
AttemptEvent { attempt_id, sequence, kind, payload_hash, usage, occurred_at } // append-only
```

`POST /v1/automation/runs/:id/cancel` and profile revocation are generation-changing. Worker endpoints authenticate a lease, never accept profile/settings/artifact authority from the worker, and return denial on expiry/revocation. Dashboard reads require `automation:read`; policy/profile/QA decisions require `automation:write`; enablement, secret bindings, and emergency control require `automation:admin` plus pilot approval.

## Testing Strategy

| Layer | What to test | Approach |
|---|---|---|
| Unit | resolution precedence, artifact hashes, event state machine, caps | RED Rust/TypeScript tests with fake clock/process. |
| Integration | SQLite scope/append-only constraints, leases, revoke race, callback idempotency | in-memory SQLite and Axum `oneshot`. |
| Runtime contract | fixed argv, strict config, stream parsing, sandbox/secret/egress denial | fake CLI/SDK and sandbox fixture. |
| GitHub/Admin E2E | QA recheck/handoff, RBAC, profile UI and kill switch | fake App plus Vitest/RTL API flow. |

## Threat Matrix

| Boundary | Status | Safe failure / RED test |
|---|---|---|
| Routes/webhooks | Applicable | Authenticate lease/signature, replay and org/repo bind; deny without evidence disclosure. |
| Shell/subprocess/worktree | Applicable | Fixed argv/no shell, canonical sandbox paths; reject injected args, path escape, TTY. |
| VCS/PR | Applicable | App-only scoped token; revocation before final recheck never merges/handoffs. |
| Process integration | Applicable | Pinned image/artifacts and allowlisted egress/tools; expired/cancelled lease cannot emit privileged action. |

## Migration / Rollout

Additive v57 is user-applied; do not operate the migration runner. Flags default off: `automation_read_only`, `automation_shadow`, `automation_write`, `automation_qa_merge`, plus benchmark-gated retrieval flags. Roll out read-only, shadow, one human-approved QA repository, then cohorts. Kill switch disables profiles, increments generations, kills leases, revokes secrets/App access, and retains receipts; QA rollback remains the existing revert process.

## Open Questions

- [ ] Confirm the pilot's GitHub App permissions, sandbox substrate, Claude CLI/Agent SDK version, and approved QA branch classifier.
