# Tasks — Autonomous Agents MVP

**Change:** `autonomous-agents-mvp`
**Project:** `nexus-mind`

TDD is strict for backend and admin. Every implementation task starts RED, then GREEN. This is a high-risk
change (credentials, external writes, shell/process integration, autonomous execution) and must ship as a
feature-branch chain with bounded review receipts, not one large PR.

## Delivery chain

| PR | Slice | Approx. authored lines |
|---|---|---:|
| 1 | Domain schema, RBAC, definition lifecycle | 350–500 |
| 2 | Scheduler, durable queue, leases, cancellation | 350–500 |
| 3 | Colocated worker, Claude Code headless runtime, sandbox and secret broker | 400–650 |
| 4 | GitHub App connector and webhook ingestion | 400–650 |
| 5 | Findings/deliveries plus Slack | 300–450 |
| 6 | QA template end to end | 400–600 |
| 7 | Issue Resolver template end to end | 450–700 |
| 8 | PR Reviewer template end to end | 400–600 |
| 9 | Admin wizard and operations views | 500–800 |
| 10 | E2E, observability, operations docs and pilot gates | 300–500 |

## Phase 0 — Lock threat model and contracts

- [x] 0.1 Document trust boundaries, abuse cases, secret data flow, GitHub permission matrix, RBAC matrix, kill-switch behavior, retention, and incident procedure in `docs/automation-threat-model.md`.
- [x] 0.2 Define the versioned local worker protocol for claim, heartbeat, event, artifact metadata, result, receipt, cancellation and authority recheck; deployment is same-host only in MVP.
- [x] 0.3 Build a malicious-input evaluation corpus: issue/PR/code/log prompt injection, secret canaries, symlink/path escape, oversized output, fork PR, stale SHA, callback replay, and revoked connector.
- [x] 0.4 Reconcile `autonomous-loop-engineering`: retain v57/profiles/provenance, close duplicate tasks, and record this change as the owning product SDD.

## Phase 1 — Definitions, revisions, and authority

- [x] 1.1 RED: migration tests for org scope, immutable revisions, target/connector binding, status transitions, unique names, soft archive, exact permission strings, full grants to built-in admin/super-user templates, and no default grants to other roles.
- [x] 1.2 GREEN: add the next user-applied migration with `agent_definitions`, revisions, targets, schedules, connectors, runs, leases, events, findings, deliveries, work items, and output links; do not auto-run production migration.
- [x] 1.3 RED: API/store tests proving authorization depends only on the exact permission, including allowed custom-role and denied admin-with-grant-removed cases, plus 404 cross-org behavior, disabled-on-create/edit, exact-revision validation, material/no-op edits, and archive constraints.
- [x] 1.4 GREEN: add models, queries, `/v1/autonomous-agents` definition/template/target endpoints, the seven explicit `autonomous_agent:*` permissions, role-template grants, audit integration, and feature flag; add no role-name checks.
- [x] 1.5 Rename the existing UI concept to Agent identities without changing `/v1/agents` wire compatibility.

## Phase 2 — Scheduler and leases

- [x] 2.1 RED: daily/interval/timezone/DST/misfire tests and concurrent scheduler occurrence-idempotency tests using a controllable clock.
- [x] 2.2 GREEN: implement persisted next-due scheduling, atomic occurrence creation, bounded catch-up, and per-org/repo/definition concurrency.
- [x] 2.3 RED: lease claim/heartbeat/expiry/reclaim tests, callback replay mismatch, cancellation race, no-post-revocation write, and durable backpressure.
- [x] 2.4 GREEN: implement worker claim/callback routes, scoped worker auth, lease tokens, heartbeats, attempt recovery, cancellation, event batching, and dead-letter transitions.

## Phase 3 — Colocated Claude Code worker and secret broker

- [ ] 3.1 RED: reject missing/unsupported Claude binary, unauthenticated server session, interactive prompts, arbitrary argv, shell interpolation, path/symlink escape, TTY, unpinned extensions, uncontrolled network, malformed event streams, budget overrun, and secret persistence.
- [ ] 3.2 GREEN: complete the supervised same-host `workers/` process with Claude Code's pinned noninteractive headless invocation, absolute binary path, authenticated-runtime readiness probe, machine-readable stream parsing, disposable workspaces, pinned SHA checkout, resource/tool/network caps, process-tree cancellation, output truncation/redaction, and teardown.
- [x] 3.3 RED: cover authentication expiry before lease, expiry between probe and process start, zero attempt-budget consumption, durable due runs, no backlog burst, reauthentication recovery, manual `Check again`, and zero external writes while `reauth_required`.
- [x] 3.4 GREEN: add startup, periodic, and pre-lease runtime-health checks with `ready`, `degraded`, `reauth_required`, and `unavailable`; pause leasing on expiry and automatically resume through normal misfire policy after a successful probe.
- [x] 3.5 RED/GREEN: expose sanitized runtime health and permission-gated `Check again`; never accept tokens, inspect credential files, implement token refresh, or launch interactive login from an HTTP request.
- [ ] 3.6 RED/GREEN: implement opaque target/connector secret references and an ephemeral broker; Claude authentication remains host infrastructure and is never copied into this broker or the database. Run canary tests across prompts, env diagnostics, subprocess output, events, artifacts, receipts and failures.
- [x] 3.7 RED/GREEN: deterministic context manifest with evidence IDs/hashes, trust labels, stable ranking/budgeting, SDD/code/memory citations, and independent evaluator context.

## Phase 4 — GitHub App

- [ ] 4.1 Provision a NexusMind GitHub App for development/pilot with the minimal permission/event matrix in D6; document separate dev/prod credentials and rotation.
- [ ] 4.2 RED: installation/org/repository mapping, permission sufficiency, repository removal, revocation generation, just-in-time token, and OAuth/PAT rejection tests.
- [x] 4.3 GREEN: implement installation onboarding/status/revoke APIs and encrypted App private-key/webhook-secret configuration; never return secrets.
- [x] 4.4 RED: webhook HMAC, delivery replay, accepted action, payload size, org/repo binding, fork behavior, and missed-webhook reconciliation tests.
- [x] 4.5 GREEN: implement verified webhook ingestion, minimized payload persistence, durable triggers, reconciliation polling, and pre-write authority recheck.

## Phase 5 — Canonical findings, deliveries, and Slack

- [x] 5.1 RED/GREEN: finding fingerprint/upsert/occurrence/resolution tests and APIs with bounded sanitized evidence.
- [x] 5.2 RED/GREEN: delivery state machine with stable idempotency key, reconcile-before-retry, independent channel failure, retry/backoff, dead letter, and receipt/external mapping.
- [ ] 5.3 RED: Slack destination allowlist, credential revocation, redaction, payload limits, rate limit, retry, and duplicate-message tests.
- [x] 5.4 GREEN: encrypted Slack webhook/bot connector and sanitized Block Kit delivery with NexusMind backlink.

## Phase 6 — QA template

- [x] 6.1 Define/version QA template schema, fixed workflow, supported MVP adapter (Playwright plus allowlisted command), default budgets, output schema, and evaluator.
- [ ] 6.2 RED: target credential isolation, health failure, deterministic test invocation, timeout, bounded reproduction, trace/screenshot sanitation, fingerprint dedupe, and partial delivery tests.
- [x] 6.3 GREEN: implement QA workflow and always-on NexusMind finding delivery.
- [x] 6.4 RED/GREEN: optional GitHub issue projection with `nexusmind-qa` label, hidden fingerprint, update/reopen policy, external mapping, and no duplicate issue on retry.
- [ ] 6.5 E2E: daily 06:00 timezone occurrence and manual run across two pilot projects; exercise NexusMind-only, Slack-only-plus-NexusMind, and all-channel configurations.

## Phase 7 — Issue Resolver template

- [x] 7.1 Define/version schema for labels, exclusions, base branch, path/diff/test limits, triggers and concurrency.
- [ ] 7.2 RED: eligibility, duplicate work, pinned base, stale base, prompt injection, excluded paths, diff cap, formatter/linter/test failure, secret scan, evaluator block, and revocation-before-publish.
- [ ] 7.3 GREEN: implement deterministic context, disposable branch/worktree, bounded edit workflow, verification receipts, independent evaluator, and cleanup.
- [x] 7.4 RED/GREEN: create one draft PR with issue/run/evidence backlinks and external mapping; prohibit merge/deploy/force push and reconcile ambiguous GitHub timeouts.
- [ ] 7.5 E2E: resolve representative small issues in two test repositories, including one blocked run and one successful draft PR.

## Phase 8 — PR Reviewer template

- [x] 8.1 Define/version review rubric, severity/publication policy, diff/branch/path/check configuration and default budgets.
- [ ] 8.2 RED: immutable review identity, duplicate webhook, stale head, draft/filter handling, fork safety, oversized diff, prompt injection, line mapping, and permission revocation.
- [x] 8.3 GREEN: implement bounded diff/context analysis, safe optional checks, structured findings, head recheck, and NexusMind review record.
- [x] 8.4 RED/GREEN: publish COMMENT or REQUEST_CHANGES idempotently; explicitly reject APPROVE, merge, push, or inline publication against stale SHA.
- [ ] 8.5 E2E: review clean/problematic/stale/fork PR fixtures and prove no duplicate review for the same identity.

## Phase 9 — Admin product surface

- [x] 9.1 Add typed client/domain models and query keys for templates, definitions/revisions, schedules, connectors, runs, events, findings and deliveries.
- [x] 9.2 RED/GREEN: permission-gated template wizard with target selectors, write-only secret inputs, schedule/timezone editor, budgets, output choices, authority summary, validation and explicit enable; never inspect role names.
- [x] 9.3 RED/GREEN: Agents/Templates/Runs/Findings/Connections/Runtime views plus Claude Code server readiness, local worker health, detail timeline, receipts, budget/evidence/output drill-down and connector health.
- [x] 9.4 RED/GREEN: run-now, disable, cancel, clone, archive, connector revoke/rotate and delivery-only retry; all optimistic UI reconciles with authoritative backend state.
- [ ] 9.5 Accessibility, focus, loading/empty/error/partial/dead-letter states, timezone rendering, responsive layout, and no secret values in DOM/query cache/error telemetry.

## Phase 10 — Operations and pilot

- [x] 10.1 Add metrics/traces/alerts for scheduling, leases, costs, policy denials, connector health, redactions, findings and deliveries; verify labels do not leak secrets.
- [x] 10.2 Add retention/cleanup jobs, SQLite contention/load tests, backup/restore coverage, worker rolling-restart recovery, and global/per-org/template kill switches.
- [x] 10.3 Write `docs/autonomous-agents-operations.md`: single-server deployment, installing/pinning/authenticating and reauthenticating Claude Code on the backend server, session-expiry detection, headless health checks, process supervision, GitHub App setup, secret rotation, troubleshooting, incident response, rollback, and pilot checklist.
- [ ] 10.4 Drill Claude session expiry with queued schedules: verify `reauth_required`, operator alert, no leases/writes/attempt cost, successful server reauthentication, `Check again`, and bounded catch-up without a burst.
- [ ] 10.5 Run the malicious corpus and end-to-end pilot success criteria from the proposal; attach evidence and unresolved limitations to `verify-report.md`.
- [x] 10.6 Run backend tests/clippy, admin tests/typecheck/build, worker tests/typecheck, migration verification and external-contract fakes in CI.

## Review gates

- [ ] After each apply slice, start/reuse the required content-bound review receipt according to repository policy.
- [ ] Pre-commit, pre-push and pre-PR validate the existing receipt; they never start or reset review.
- [ ] Security/credential/GitHub/process slices receive four 4R lenses; any escalated receipt stops the chain.
- [ ] No production enablement until the E2E pilot, secret-canary suite, revocation race suite, duplicate-write suite, and kill-switch drill pass.
