# Autonomous Agent Operations UI Specification

## Admin navigation and access

The admin app MUST provide Autonomous Agents, Templates, Runs, Findings, Connections, and Runtime health views.
Every query, mutation, and control MUST be visible or enabled from the caller's permissions, never from their
role name, and MUST independently enforce the same permission in the backend.

## Creation wizard

The wizard, visible with `autonomous_agent:create`, MUST collect a managed template, explicit targets, template configuration, connector references,
outputs, schedule/timezone, and budgets; show validation errors inline; show a human-readable authority summary;
save disabled; and require explicit validation and enablement.

Secret values MUST be write-only. After save, the UI shows only configured/not-configured, health, scope, and
rotation metadata.

## Operations

The agent detail MUST show status, revision/template, next run, targets, capability envelope, connector health,
budgets, recent runs, findings, and controls for run-now, disable, cancel, clone, and archive.

The run detail MUST show a sanitized ordered timeline, source snapshot, attempts/leases, budget consumption,
verification evidence, findings, deliveries, external links, and immutable receipts.

## Partial failures and recovery

The UI MUST distinguish succeeded, partial, failed, cancelled, blocked-policy, budget-exhausted, and dead-letter
states. It MUST permit a caller with `autonomous_agent:run` to retry only a failed delivery without rerunning tests or code generation and
without creating duplicate external output.

The Runtime health view MUST show whether the local Claude Code binary, supported version, server authentication,
headless probe, and colocated worker are ready. It MUST never display Claude authentication material.

When authentication expires, the view MUST show `reauth_required`, the last successful probe, sanitized
server-side reauthentication instructions, paused-scheduling impact, and a `Check again` action gated by
`autonomous_agent:enable`. It MUST NOT accept or display Claude tokens, credential paths, credential-file
contents, or raw authentication output.

## Compatibility

The existing API-key-backed agent list MUST remain available as Agent identities. Existing identity creation,
rotation, revocation, and activity views MUST continue to work without becoming runnable definitions implicitly.
