# Autonomous Agent Control Plane Specification

## Permission-gated lifecycle

The system MUST authorize every autonomous-agent operation through dedicated permissions and MUST NOT branch on
the caller's role name. The built-in admin and super-user role templates MUST receive the complete permission
set in the migration; other roles MUST receive none by default.

- GIVEN a caller lacks the exact permission required by an operation
- WHEN they call its endpoint regardless of their role name
- THEN the API returns 403 and persists no state or secret

- GIVEN a custom role has `autonomous_agent:create` but lacks `autonomous_agent:enable`
- WHEN its user creates and then tries to enable a definition
- THEN creation succeeds in disabled state and enablement returns 403

- GIVEN a caller with `autonomous_agent:create` creates a valid definition
- WHEN creation succeeds
- THEN revision 1 exists in `disabled` state and cannot be scheduled until exact-revision validation succeeds

The permission set MUST be `autonomous_agent:read`, `autonomous_agent:create`, `autonomous_agent:update`,
`autonomous_agent:enable`, `autonomous_agent:run`, `autonomous_agent:cancel`, and
`autonomous_agent:manage_connectors`.

## Immutable revisions and authority envelope

The system MUST create an immutable revision for every material change and MUST pin template version, targets,
capabilities, policy generation, connector revocation generations, configuration hash, and budgets to each run.

- GIVEN an enabled definition is edited
- WHEN the edit changes any material field
- THEN a new revision is appended, the definition becomes disabled, and prior runs still resolve their original revision

## Durable runs, attempts, events, and receipts

The system MUST persist run state outside model context, use leases and idempotent callbacks, and append
immutable receipts for external side effects and verification evidence.

- GIVEN a worker repeats a callback with the same callback ID and payload hash
- WHEN the callback is accepted again
- THEN it is a successful no-op

- GIVEN the same callback ID carries a different hash
- WHEN submitted
- THEN it is rejected and audited as a replay mismatch

## Revocation and fail-closed writes

The system MUST recheck definition, attempt, policy, connector, and target authority immediately before every
external write.

- GIVEN an authorized caller disables an agent or cancels its active run
- WHEN a worker later attempts a GitHub or Slack write
- THEN the write is denied even if the worker holds an unexpired lease or installation token

## Isolation

The system MUST execute agents on the same server as the NexusMind backend through a separately supervised local
worker. It MUST use a disposable workspace pinned to an immutable source SHA, fixed noninteractive invocation,
allowlisted tools/network, resource caps, and secret teardown; it MUST NOT run long agent processes inside an
Axum request handler.

## Claude Code runtime

The worker MUST invoke a pinned Claude Code binary authenticated on the backend server in noninteractive headless
mode. Definitions MUST NOT store Claude credentials. Missing binary, unsupported version, expired authentication,
interactive prompt, or failed headless health check MUST block new execution and surface a sanitized runtime-health
error without exposing authentication material.

The system MUST check Claude authentication at worker startup, periodically, and before granting every run lease.
Expired authentication MUST set runtime health to `reauth_required`, MUST pause new leases, MUST NOT consume a run
attempt or retry budget, and MUST NOT disable or rewrite agent definitions.

- GIVEN Claude Code authentication expires while scheduled runs become due
- WHEN the pre-lease probe detects expiry
- THEN no run receives a lease, no external write occurs, and due work remains durable under its misfire policy

- GIVEN the operator reauthenticates Claude Code on the backend server
- WHEN a periodic or permission-gated manual probe succeeds
- THEN runtime health becomes `ready` and eligible schedules resume without recreating or re-enabling agents

- GIVEN Claude reports an authentication failure after invocation starts
- WHEN the worker classifies the failure
- THEN it stops before publication, records sanitized evidence, marks `reauth_required`, performs no external
  write, and waits for successful reauthentication instead of automatically retrying Claude

NexusMind MUST NOT accept Claude tokens through its API/UI, read or persist Claude credential material, implement
an independent token refresh flow, or launch an interactive authentication process inside an HTTP request.

## Tenant isolation

All definitions, targets, connectors, schedules, runs, events, findings, deliveries, and receipts MUST be
organization scoped. A cross-organization ID lookup MUST return 404 and MUST NOT disclose existence.
