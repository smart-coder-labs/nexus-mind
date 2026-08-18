# Apply progress — Autonomous Agents MVP

**Change:** `autonomous-agents-mvp`
**Applied:** 2026-08-18

## Implemented

- Additive migrations v61–v62: seven exact permissions, immutable definition revisions/events, validations, targets, schedules, encrypted connectors, webhook receipts, durable runs/leases, findings, deliveries and output/work-item mappings.
- Permission-only control plane. The autonomous API uses `require_explicit_permission`; role names cannot bypass it. Built-in admin and super-user effective permissions contain all seven grants.
- Managed/versioned QA, GitHub Issue Resolver and GitHub PR Reviewer templates. New and materially edited agents are disabled until their exact revision validates.
- Timezone-aware daily/interval schedules, minimum interval, stable occurrence keys, collapsed misfires, atomic claim, one active definition run, lease expiry recovery, cancellation and append-only timeline.
- Colocated feature-flagged `autonomous_worker` binary, separately supervised from the HTTP backend, with absolute Claude binary, fresh pre-lease and pre-spawn auth checks, `reauth_required`, noninteractive direct argv invocation, fixed authority prompt, bounded/redacted output, disposable repository workspace and cancellation.
- Ephemeral target-secret broker: encrypted opaque connector references are resolved only for allowlisted QA subprocesses, injected as validated environment variables, redacted by exact value and never added to Claude's environment or persisted plaintext.
- GitHub App connector, just-in-time installation tokens, strict repository identifiers, HMAC/replay-safe minimized webhook ingestion and issue/PR triggers. PAT/OAuth inputs are not supported.
- Canonical QA finding dedupe plus NexusMind, Slack and optional GitHub issue delivery. Delivery retry is independent of Claude execution.
- Issue Resolver obtains an eligible issue, checks configured labels, pins and rechecks the base SHA, applies bounded changes, protects excluded paths, scans the diff for secrets, runs allowlisted verification argv, pushes only `nexusmind/run-*` and creates a draft PR.
- PR Reviewer checks out the webhook head, analyzes a bounded base-to-head diff, rejects forks/drafts by default, rechecks the remote head and publishes only COMMENT or REQUEST_CHANGES. There is no approve/merge/deploy path.
- Findings support occurrence dedupe and resolution. Delivery retries use bounded exponential backoff/dead-letter state; QA GitHub issues retain their external mapping and are updated/reopened rather than duplicated. Runs with failed optional channels finish `partial`.
- Organization kill switch, configurable retention cleanup and permission-gated operational metrics are available in the Runtime panel.
- Admin Automation surface for create/validate/enable/disable/run, schedules, runs/cancel, findings/deliveries/retry, write-only connections/revoke and runtime reauthentication guidance.
- Threat model, malicious corpus, local worker protocol and single-server operations/runbook documentation.

## Deliberate MVP boundaries

- The worker is a separate same-host, single-server process. Remote workers, Kubernetes-wide coordination and arbitrary shell commands are not supported.
- Connector onboarding accepts operator-provisioned GitHub App credentials; NexusMind does not create the provider App.
- Browser evidence is represented through allowlisted Playwright commands and structured findings. Artifact blob storage remains post-MVP; a deterministic independent schema/secret evaluator gates publication in this MVP.
- Missed GitHub triggers are reconciled by polling. Ambiguous successful writes reconcile by QA fingerprint, resolver run branch, or reviewer run marker before retrying, while stable local receipts remain authoritative.
- Production pilot drills require a real authenticated Claude host, GitHub App installation, Slack webhook and test repositories; they cannot be executed in this source checkout.

## Verification status

See `verify-report.md`. Focused backend, complete backend library (1,301 tests), the separate worker binary, clippy, focused admin tests and production admin build were run. The all-binaries/full-workspace gate remains affected by the pre-existing unresolved conflict in `apps/backend/src/bin/migrate_knowledge.rs`, which this change intentionally did not modify. Real-provider pilot gates and the corrupted review authority remain explicitly open.
