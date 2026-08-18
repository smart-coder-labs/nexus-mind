# Design — Autonomous Agents MVP

**Change:** `autonomous-agents-mvp`
**Project:** `nexus-mind`

## D1 — One server, separate supervised processes

The Rust/Axum backend owns definitions, schedules, policies, connectors, run state, findings, deliveries,
leases, approvals, audit events, and receipts. The MVP deploys a supervised worker process on the same server
and in the same release unit as the backend. It claims a short-lived lease, executes inside a disposable local
workspace, and reports typed events through an authenticated local boundary. Keeping it outside the Axum
request process prevents long Claude/test runs from blocking HTTP traffic. Remote workers are not supported.

The existing v57 `automation_runs`, `automation_attempts`, `automation_receipts`, and
`automation_revocations` remain the provenance core. New records reference those IDs.

## D2 — Domain model

New additive tables (all organization scoped):

| Table | Purpose and key constraints |
|---|---|
| `agent_definitions` | Name, template/version, status, project scope, service identity, config revision, policy generation; soft-delete only when no active run. |
| `agent_definition_revisions` | Immutable normalized configuration, capability envelope and hashes; secrets represented only by connector/secret reference IDs. |
| `agent_targets` | Explicit project/repository/environment allowlist; unique per definition/target. |
| `agent_schedules` | Schedule kind/expression, IANA timezone, next due time, misfire policy, enabled flag. |
| `agent_connectors` | Kind, display metadata, encrypted credential reference, scopes, health, revocation generation; never exposes ciphertext/plaintext via DTO. |
| `agent_runs` | Definition revision, trigger, scheduled occurrence key, snapshot SHA, status, budgets, timestamps; links 1:1 to `automation_runs`. |
| `agent_leases` | Attempt, worker, expiry, heartbeat, claim token hash; one active lease per run. |
| `agent_events` | Append-only typed/redacted operational timeline with monotonic sequence. |
| `agent_findings` | Canonical deduplicated QA/review findings with fingerprint, severity, evidence and lifecycle. |
| `agent_deliveries` | One finding/run output per channel and idempotency key; external ID/URL and retry state. |
| `agent_work_items` | GitHub issue/PR input snapshot and eligibility decision. |
| `agent_output_links` | Maps source item/run to issue, review, branch, commit, or draft PR external identity. |

All cross-table references are checked against `org_id`; foreign-org resources are returned as 404. Definition
revisions, events, receipts, finding evidence, and output mappings are append-only.

## D3 — Definition lifecycle

`draft → validating → disabled → enabled → disabled → archived`.

- Creation always produces a disabled definition and revision 1.
- Validation checks schema, permissions, target ownership, connector scopes/health, schedule, commands, budgets,
  template version, worker profile, and a no-side-effect dry run.
- Enabling requires a successful validation against the exact revision hash and connector revocation generations.
- Any material edit creates a new immutable revision and disables the definition until revalidation.
- Disable prevents new occurrences immediately. Cancel additionally revokes the active attempt.
- Archive is allowed only after active attempts finish or are revoked; evidence remains readable.

Authorization never branches on role name. Each endpoint and UI action checks one dedicated permission:

- `autonomous_agent:read` — view definitions, templates, schedules, sanitized runs, findings and evidence;
- `autonomous_agent:create` — create and clone disabled definitions;
- `autonomous_agent:update` — revise definitions, targets and schedules, and archive definitions;
- `autonomous_agent:enable` — validate, enable and disable definitions;
- `autonomous_agent:run` — start a manual run and retry a failed delivery;
- `autonomous_agent:cancel` — cancel and revoke an active run;
- `autonomous_agent:manage_connectors` — install, configure, rotate and revoke connectors.

The migration grants all seven permissions to the built-in `admin` and `super_user` templates. Other roles
receive none by default, but can receive an explicit subset through the existing role machinery. Backend
permission checks are authoritative; the UI uses the same session permission set for visibility and actions.

## D4 — Template contract, not arbitrary prompt

A managed template is versioned code with:

- JSON schema for admin configuration;
- required and optional connector capabilities;
- fixed execution profile and allowed tools/network destinations;
- context compiler recipe and untrusted-input labels;
- bounded workflow state machine;
- output schema, evaluator rules and default budgets;
- migration compatibility and a kill-switch key.

Definitions pin an exact template version. NexusMind may offer an explicit admin upgrade; it never silently
changes a running definition's behavior. Custom free-form executable templates are outside the MVP.

## D5 — Scheduling semantics

Supported kinds are `manual`, `daily`, and `interval` (minimum 15 minutes). Daily schedules store local time
plus IANA timezone. The scheduler computes and persists the next UTC occurrence. During fall-back DST, the
first matching instant runs and the duplicate is skipped; during spring-forward, the nonexistent local time
runs at the next valid instant. Misfire policy defaults to `run_once` with a 24-hour grace; missed occurrences
collapse into one run, never a burst.

Occurrence idempotency key:

`sha256(definition_id | revision | schedule_id | scheduled_for_utc)`.

The scheduler inserts the run and advances `next_run_at` atomically. Workers claim with compare-and-swap,
heartbeat, and bounded lease expiry. An expired lease can create another attempt for the same run, but all
external writes retain run/output idempotency keys.

## D6 — Connector and secret boundary

The GitHub connector is a GitHub App installation bound to selected repositories. Installation tokens are
minted just in time, short lived, and never persisted. Required App permissions for MVP:

- metadata: read;
- contents: read; contents: write only for Issue Resolver branches;
- issues: write only when QA GitHub delivery or Issue Resolver status comments are enabled;
- pull requests: write for draft PR creation and PR reviews;
- checks: read; statuses/checks write only if the reviewer publishes a named NexusMind check;
- webhooks: issues, issue_comment (optional command), pull_request, pull_request_review, and installation/repo
  changes.

Slack uses either an incoming webhook limited to one destination or a bot token with an explicit channel
allowlist. Target application credentials (for example QA login) are separate encrypted secret references.
Secrets are decrypted only into the worker's ephemeral secret broker, mapped to named inputs, redacted from
subprocess output, then destroyed. Template configuration may refer to `secret_ref`, never a secret value.

## D7 — Claude Code headless runtime on the backend server

Claude Code is installed and authenticated once by the NexusMind server operator on the backend host. The
colocated worker invokes only a configured absolute binary path and pinned supported version, using the CLI's
noninteractive headless/print mode with a fixed argument builder, machine-readable output, no TTY, a bounded
working directory, and an explicit timeout. Prompts and repository content cannot add CLI flags or change the
binary path.

The Claude login/session belongs to the host deployment, not an organization, definition, or connector, and
is never copied into NexusMind tables or returned by an API. A readiness probe verifies binary presence,
version, and authenticated headless execution without exposing session material. If it fails, scheduling
pauses, new runs become `blocked_runtime`, and Runtime health instructs the operator to authenticate Claude
Code on that server. Existing processes remain cancellable through the supervised process tree.

Runtime health is persisted as `ready`, `degraded`, `reauth_required`, or `unavailable`, with sanitized reason,
last successful probe, last failed probe, and next probe time. Probes run at worker startup, periodically, and
immediately before a lease is granted. Authentication expiry is not a normal run failure and does not consume
an agent attempt or retry budget: due runs remain durable and unleased. The scheduler applies the schedule's
normal misfire/catch-up policy after authentication returns rather than launching a backlog burst.

Reauthentication is an operator action performed with the official Claude Code CLI on the backend server.
NexusMind may show copyable operational instructions, but must not start an interactive login inside an HTTP
request, accept Claude tokens through its UI/API, inspect credential files, or implement its own refresh flow.
After reauthentication, the periodic probe—or an authorized `Check again` action—marks the runtime `ready` and
automatically resumes eligible scheduling without redefining or re-enabling agents.

If Claude reports an authentication failure after a lease or invocation has started, the worker stops the
workflow before publication, records only sanitized runtime evidence, revokes the attempt, sets
`reauth_required`, and does not perform GitHub, Slack, branch, or other external writes. This classification
does not trigger an automatic model retry; the run is retried only after runtime readiness returns.

## D8 — GitHub webhook and polling model

Webhook ingestion verifies HMAC, delivery ID uniqueness, event/action allowlist, installation-to-org mapping,
and repository binding before enqueueing. Scheduled reconciliation polling repairs missed webhooks. Event
payloads are minimized and redacted before persistence. Fork PRs are read-only in MVP; no untrusted fork code
runs with write credentials.

Immediately before every GitHub write, the backend rechecks definition enabled state, attempt revocation,
installation/repository binding, pinned policy generation, and requested capability.

## D9 — QA template

Configuration: selected projects/repositories, target environment/URL, test adapter, allowlisted commands or
Playwright suite, credential references, schedule, severity threshold, retry count, and outputs
(`nexusmind`, `github_issue`, `slack`). NexusMind output is always on.

Workflow:

1. Snapshot target/repo and resolve secrets.
2. Run health check and configured test plan in the QA execution profile.
3. Normalize failures; redact artifacts; capture bounded logs, screenshots/traces and source evidence.
4. Reproduce once when policy permits and compute a stable fingerprint from project, suite/test, normalized
   failure and affected component.
5. Upsert the canonical finding; update occurrence count instead of duplicating an open finding.
6. Independently deliver to enabled channels. A GitHub failure does not erase the NexusMind finding or block
   Slack; each delivery has its own retry/dead-letter state.

GitHub issues use a managed title/body, label `nexusmind-qa`, evidence link, run ID and hidden fingerprint.
Slack messages contain summary/severity/link, not raw logs or secrets.

## D10 — Issue Resolver template

Configuration: repositories, eligible labels, excluded labels/paths, base branch, maximum changed files/lines,
test commands, code-owner/approval policy, concurrency, and schedule/webhook triggers.

Eligibility requires an open non-PR issue, repo allowlist, required label, no exclusion label, no existing
active work item/output for the issue head/policy, bounded scope, and no prompt-requested authority expansion.
The worker compiles deterministic NexusMind/code/SDD context, creates a branch from a pinned base SHA,
implements in a disposable sandbox, runs formatter/linter/tests and secret scan, then an independent evaluator
checks requirement coverage and limits. Only a passing receipt may create a draft PR. The PR body links the
issue, NexusMind run, tests, evidence and limitations. The agent never merges or deploys.

## D11 — PR Reviewer template

Configuration: repositories, base branches, draft handling, path filters, required checks, review rubric,
maximum diff size, and whether to publish a GitHub review, NexusMind check, or both.

The immutable review identity is `(repo, PR number, head SHA, template version, policy generation)`. The
reviewer reads the diff and independently assembled context, treats PR text/code as untrusted, runs bounded
static/test checks where safe, and emits structured findings with file/line only when the head SHA still
matches. A stale review is stored in NexusMind but not published inline. It may publish `COMMENT` or
`REQUEST_CHANGES` according to configured severity policy; it never publishes `APPROVE`, merges, pushes to the
author branch, or exposes secrets.

## D12 — Budgets and failure states

Every revision pins maximum wall time, attempts, tokens/cost, tool calls, network destinations, artifact bytes,
changed files/lines, and concurrent runs. Defaults are template-owned and callers with
`autonomous_agent:update` may only reduce them in MVP.
Terminal states: `succeeded`, `partial`, `failed`, `cancelled`, `blocked_policy`, `blocked_runtime`,
`budget_exhausted`, and `dead_letter`. Partial means canonical output succeeded but one or more optional
deliveries failed.

Retries use exponential backoff with jitter and classify errors as transient/permanent. Model/evaluator policy
failures are not blindly retried. Circuit breakers disable scheduling for repeated authentication, secret,
or policy failures and surface an admin alert.

## D13 — API and admin surface

New `/v1/autonomous-agents` resources cover templates, definitions/revisions, validation, enable/disable,
schedules, targets, connectors, runs/events/findings/deliveries, manual run, cancel, and retry-delivery.
The local worker boundary uses scoped service authentication, lease tokens and callback idempotency. It binds
to loopback or equivalent local IPC by default and never accepts browser cookies.

The admin page has tabs: Agents, Templates, Runs, Findings, and Connections. Creation is a template-driven
wizard. Every write shows the computed authority envelope before confirmation. Run detail shows a timeline,
snapshot, budgets, sanitized evidence, outputs, delivery state, and receipts. Secret fields are write-only and
display only presence, last rotation, and health.

Runtime health shows `reauth_required` distinctly from worker/backend failure, the last successful check,
sanitized remediation instructions, and a permission-gated `Check again` action. It never renders tokens,
credential paths, session file contents, or raw Claude CLI authentication output.

## D14 — Observability and retention

Metrics cover due/claimed/completed runs, lease expiry, queue latency, duration, cost, findings, delivery
latency/failure, policy denials, redactions, and connector health by template/org without secret labels.
Structured logs carry org/run/attempt IDs and are redacted. Default retention: detailed artifacts 30 days,
events 90 days, findings/output mappings/audit/receipts according to organization audit policy. Deletion jobs
must preserve immutable receipt hashes and external backlinks while removing expired sensitive payloads.

## D15 — Compatibility and migration

The existing `/v1/agents` identity API remains compatible. The current `Agents.tsx` behavior moves under the
Agent identities tab. Existing v57 records remain valid. The incomplete `autonomous-loop-engineering` tasks for
worker, leases, GitHub and QA are absorbed by this change and must not be implemented as a parallel engine.
