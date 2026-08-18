# Proposal — Autonomous Agents MVP

**Change:** `autonomous-agents-mvp`
**Project:** `nexus-mind`
**Status:** proposed
**Date:** 2026-08-18

## Problem

NexusMind currently uses the word “agent” for API-key-backed identities and has an early automation
foundation (`automation_runs`, attempts, immutable receipts, revocations, managed execution profiles),
but an admin cannot define a goal, connect bounded resources, schedule execution, or choose where an
agent publishes its results. The unfinished `autonomous-loop-engineering` change proves part of the
authority model but does not yet provide a product-level autonomous agent lifecycle.

Teams need reusable agents that can operate without a human starting every run:

- a QA agent that tests selected projects on a schedule and reports bugs to NexusMind, GitHub, Slack,
  or a configured combination;
- an Issue Resolver that takes eligible GitHub issues, implements a bounded fix, verifies it, and opens
  a draft pull request;
- a PR Reviewer that reviews eligible pull requests, publishes evidence-backed feedback, and re-runs
  when new commits arrive;
- later, read-only operational agents such as a log analyst that recommends improvements.

The unsafe shortcut would be to store arbitrary prompts plus long-lived credentials and let a model
run shell commands on a cron. The MVP instead needs a durable control plane, isolated execution,
least-privilege connectors, explicit capabilities, evidence, budgets, revocation, and idempotent output.

## Goals

1. Gate every autonomous-agent operation exclusively through dedicated permissions. The migration grants
   the complete permission set to the built-in `admin` and `super_user` role templates, making the feature
   admin-only by default without hard-coded role checks.
2. Ship three versioned, NexusMind-managed templates: **QA**, **GitHub Issue Resolver**, and
   **GitHub PR Reviewer**.
3. Support manual runs and recurring schedules with IANA timezone, including daily-at-time and fixed
   interval schedules.
4. Connect selected GitHub repositories through a GitHub App with repository-scoped installations and
   least-privilege permissions; support Slack through an encrypted webhook/bot credential connector.
5. Store user-provided test commands, target URLs, non-secret configuration, and references to encrypted
   credentials separately. Never persist or emit plaintext secrets in agent definitions, prompts, logs,
   receipts, Slack messages, issues, or pull requests.
6. Execute every run on the same server as the NexusMind backend through a colocated worker process and a
   durable lease, in an isolated disposable workspace pinned to a repository SHA, with time, token, cost,
   tool, network, file, and concurrency budgets.
7. Make every meaningful input, decision, side effect, and output inspectable in NexusMind through an
   immutable event/receipt trail.
8. Produce useful output without unattended merge, deployment, branch-protection bypass, secret access
   expansion, destructive commands, or scope expansion.

## Non-goals (MVP)

- Arbitrary user-authored agent templates or arbitrary executable prompts.
- Autonomous merge, release, deployment, production mutation, database writes, or incident remediation.
- Remote/customer-hosted runners, multi-provider/model selection, or marketplace templates.
- Full log-analysis agent implementation; the connector/capability model must allow it later.
- Jira/Linear synchronization, email delivery, mobile notifications, or conversational multi-agent teams.
- Claiming “100% successful” execution. The product guarantees bounded, observable, recoverable operation;
  external systems and tests can still fail.

## Product decisions

### Agent identity versus agent definition

The current `/agents` API and page represent service identities/API keys. This change introduces
`agent_definitions` as runnable configurations. A definition has a dedicated service identity, but the two
concepts remain separate: rotating an identity credential does not rewrite the definition or its history.
The admin UI relabels the existing list as **Agent identities** and adds **Autonomous agents** as the
primary view.

### Canonical findings

NexusMind findings are canonical. A QA discovery is first committed idempotently as a NexusMind finding;
GitHub issues and Slack notifications are delivery projections with their own status and external IDs.
This preserves evidence when a connector is unavailable and prevents duplicate issues/messages on retry.

### GitHub authority

All automated GitHub writes use a GitHub App installation token minted just in time. Existing GitHub OAuth
remains a user connection for its current use cases and is not accepted as automation authority. Repository
selection is an explicit allowlist stored on each agent definition.

### Autonomy boundary

Enabling an agent is the `autonomous_agent:enable` holder's standing approval for exactly the selected template, targets, schedule,
capabilities, outputs, and budgets. A run pauses or fails closed if it requires anything outside that
envelope. Creating a GitHub issue, comment/review, or draft PR may be preapproved in the definition; merge,
deployment, destructive actions, permission widening, and material scope expansion are never preapproved.

### Claude Code runtime

The MVP uses Claude Code installed and authenticated by the NexusMind operator on the same server as the
backend. A colocated supervised worker invokes the pinned Claude Code CLI in noninteractive headless mode.
Agent definitions do not store Anthropic credentials and users do not authenticate Claude per agent: the
server's Claude Code session is runtime infrastructure. Validation fails closed when the binary, supported
version, authenticated session, or headless health check is unavailable. Claude never runs inline in an Axum
request handler, so long executions cannot block backend HTTP traffic.

Claude Code authentication is explicitly renewable and may expire at any time. NexusMind checks it at startup,
periodically, and immediately before leasing a run. An expired session moves runtime health to
`reauth_required`, pauses new leases without disabling agent definitions, and alerts authorized operators.
The operator reauthenticates Claude Code on that same server using the official CLI flow; NexusMind neither
captures nor refreshes Claude session tokens. A successful probe returns runtime health to `ready` and resumes
scheduling with the configured misfire policy.

## MVP user experience

A caller with `autonomous_agent:create` selects **Create agent** in the admin application, chooses a managed template, chooses projects/repositories, configures
its template-specific fields, connects required services, chooses delivery channels, defines a schedule and
timezone, reviews the computed capability summary, saves it disabled, runs a dry validation, and explicitly
enables it. The detail page shows next run, health, latest runs, findings, outputs, cost, receipts, and controls
for Run now, Disable, Cancel run, Rotate/revoke connector, and Clone.

## Capabilities

- `autonomous-agent-control-plane`
- `autonomous-agent-scheduling`
- `autonomous-agent-connectors`
- `autonomous-agent-templates`
- `autonomous-agent-operations-ui`

## Success criteria

- A caller holding the required permissions can create each of the three templates, validate it, enable it,
  run it manually, and schedule it; admin and super-user roles receive those grants by default.
- A scheduled invocation is claimed once despite scheduler/worker retry and is recoverable after a worker
  crash without duplicating external writes.
- QA can create a NexusMind finding and independently deliver it to enabled GitHub and Slack outputs.
- Issue Resolver can turn one eligible issue into one tested draft PR with evidence and a stable backlink.
- PR Reviewer can review an eligible head SHA and does not duplicate a review for the same policy/template
  version and head SHA.
- Revoking/disable/cancel prevents new side effects, and workers re-check authority immediately before each
  external write.
- Secret-canary tests prove plaintext connector and target credentials never enter prompts, logs, events,
  findings, issues, reviews, PR bodies, or API responses.
- End-to-end pilot: 20 scheduled/manual runs across at least two test repositories, zero cross-org access,
  zero duplicate external writes, zero writes after revocation, and every side effect linked to a receipt.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Prompt injection from issues, PRs, code, logs, or pages | Mark all external content untrusted; fixed system policy; tool allowlists; no instructions from retrieved content may widen capabilities. |
| Credential leakage | Envelope encryption, secret references, just-in-time injection, redaction, canary tests, teardown, no secret-return API. |
| Duplicate issues/comments/PRs | Stable idempotency keys, uniqueness constraints, external ID mapping, reconcile-before-retry. |
| Runaway cost or loops | Per-run/org budgets, attempt caps, wall-clock timeout, concurrency limits, circuit breaker, kill switch. |
| Unsafe code changes | Disposable sandbox, pinned base SHA, path/diff limits, tests and secret scan, independent evaluator, draft PR only. |
| Scheduler downtime or DST ambiguity | Durable due-time query, leases, explicit IANA timezone, documented DST policy, bounded catch-up. |
| SQLite contention | Short transactions only; workers never hold DB locks; leases/event batches are bounded. |

## Rollout and rollback

The feature is off by default behind `AUTONOMOUS_AGENTS_ENABLED`. Roll out to one internal organization and
test repositories, then a small allowlist of pilot organizations. Disabling the flag stops scheduling and
new leases; the global kill switch revokes active attempts. Routes remain read-only for historical evidence.
Schema changes are additive and are left in place on rollback. GitHub App installations and Slack connectors
can be revoked independently.

## Dependencies

- Reuse v57 run/attempt/receipt/revocation primitives and managed profile authorization from
  `autonomous-loop-engineering`; extend them rather than create a second run engine.
- Complete the isolated worker/lease protocol before enabling any write-capable template.
- GitHub App registration and webhook secret configuration are deployment prerequisites.
- Claude Code must be installed at the pinned supported version and authenticated on the NexusMind backend
  server before agents can be validated or enabled.
- Operations must include a tested Claude Code session-expiry and reauthentication runbook; expiration must
  cause zero external writes and must not require recreating agent definitions.
