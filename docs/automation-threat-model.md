# Autonomous agents: threat model and trust contract
Status: MVP security contract. Owner: `autonomous-agents-mvp` SDD.

## Trust boundaries

The NexusMind API, SQLite control plane and same-host worker are trusted. Agent configuration, repository contents, issues, pull requests, logs, test output and Claude output are untrusted. GitHub, Slack and target credentials are encrypted connector secrets and are released only to the narrow orchestrator operation that needs them. Claude Code authentication belongs to the host operator: NexusMind does not read its credential files, accept its token over HTTP, refresh it or start an interactive login.

The worker invokes an absolute Claude binary directly, without a shell or TTY, with a fixed authority prompt and a disposable workspace. Repository identifiers are strict `owner/repository` values. GitHub installation tokens are minted just in time and are not persisted. Connector secrets and raw Claude stderr are never written to events.

## Authority matrix

API access is based exclusively on exact permissions. Role names never authorize these endpoints.

| Permission | Authority |
|---|---|
| `autonomous_agent:read` | Definitions, runs, sanitized events/findings/runtime/connectors |
| `autonomous_agent:create` | Create a disabled definition |
| `autonomous_agent:update` | Revise definitions, schedules and targets |
| `autonomous_agent:enable` | Validate, enable/disable and probe runtime |
| `autonomous_agent:run` | Manual runs and delivery-only retries |
| `autonomous_agent:cancel` | Cancel queued/running work |
| `autonomous_agent:manage_connectors` | Write-only secret rotation and revocation |

Built-in `admin` and `super_user` receive all seven grants. Removing a grant removes authority even if the user retains either role name. Other built-in roles receive none.

## External authority

The GitHub App should request only Metadata read, Contents read/write, Issues read/write, Pull requests read/write and Checks read. Subscribe to issues, pull requests, installations and installation repositories. QA may create/update labeled issues. Issue Resolver may push only a `nexusmind/run-*` branch and create a draft PR; it cannot merge, deploy or force-push. PR Reviewer may publish COMMENT or REQUEST_CHANGES only; APPROVE and merge are absent from code and policy. Slack may post only through an explicitly stored webhook.

Every webhook is HMAC verified, size bounded and replay keyed by GitHub delivery ID plus payload hash. Every external write reacquires the connector and a fresh installation token, so revocation fails closed. Outputs use stable delivery idempotency keys.

## Abuse cases and controls

- Prompt injection in issue/PR/code/logs: data is delimited as untrusted and cannot modify fixed authority.
- Secret exfiltration: credentials are withheld from Claude; stored outputs are bounded and sanitized; test canaries must never appear.
- Path/symlink escape: workspaces are generated outside repositories, git receives no local source path, and the whole workspace is removed after a run.
- Stale PR: the fetched head SHA must equal the webhook SHA before analysis; publication reacquires authority.
- Oversized work: body, process output, changed files/lines and wall time are bounded.
- Duplicate callbacks/writes: delivery, occurrence, work item and output uniqueness constraints fail closed.
- Expired Claude session: the pre-lease probe sets `reauth_required`; no lease or external write occurs until an operator reauthenticates on the host and Check again succeeds.
- Kill switch: `AUTONOMOUS_AGENTS_ENABLED=false` prevents worker startup; disabling a definition stops new occurrences; connector revocation stops new external writes.

## Retention and incident response

Keep sanitized events, findings and delivery receipts according to the organization retention policy. Never retain installation tokens, Slack webhook values, Claude auth state or raw environment dumps. On suspected exposure: disable the feature flag, revoke the connector at the provider and NexusMind, rotate the encrypted secret, preserve sanitized run IDs/timestamps, inspect external writes, then re-enable definitions individually after validation.

## Malicious evaluation corpus

The release drill covers: instruction injection in issue/PR/code/log text; unique secret canaries; repository symlink/path escape; output above 1 MiB; fork PR; mismatched/stale head SHA; repeated webhook with same and changed payload; revoked connector immediately before publish; unauthorized admin with a removed grant; expired Claude session before lease and during execution; diff/file budget overflow; and ambiguous Slack/GitHub failures. Passing means no secret in prompts/events/results, no unauthorized write, no duplicate write and a durable, explainable terminal state.
