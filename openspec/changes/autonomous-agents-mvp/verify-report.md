# Verification report — Autonomous Agents MVP

Date: 2026-08-18

## Automated evidence

- `cargo test --manifest-path apps/backend/Cargo.toml --lib autonomous` — 19 passed.
- `cargo test --manifest-path apps/backend/Cargo.toml --lib automation::` — 15 passed.
- Concurrent scheduler scanner test — passed; two SQLite connections created exactly one occurrence/run.
- `cargo check --manifest-path apps/backend/Cargo.toml --bin autonomous_worker` — passed.
- `cargo clippy --manifest-path apps/backend/Cargo.toml --lib -- -D warnings` — passed.
- Full backend library suite — 1,301/1,301 passed.
- A full admin run executed concurrently with a clean Rust rebuild had 238 passes and 32 timeouts/failures in pre-existing Graph, Projects, Usage, Tasks, SDD and Backups tests; autonomous client tests passed. This run is recorded as resource-contention evidence, not claimed green.
- `npx vitest run src/pages/AutonomousAgents.test.tsx src/components/Layout.test.tsx src/api/client.autonomous-agents.test.ts` without the Rust build — 7/7 passed.
- `npm run build` in `apps/admin` — passed.
- Scoped `git diff --check` for every file in this change — passed.
- Post-apply `gentle-ai review validate --gate post-apply` — denied again with `authority_corrupted` at `receipt-discovery`; repository policy requires explicit maintainer action and forbids silently starting a replacement review.

Covered contracts include exact-permission authorization (including an admin whose persisted grant is removed and a permitted custom role), disabled/revision validation lifecycle, org isolation, immutable records, schema v62, HMAC tamper rejection, webhook/callback replay mismatch, occurrence/finding/cancellation idempotency, concurrent schedule scanning, definition/repository/org concurrency, lease token/expiry recovery, session-expiry requeue without attempt consumption, post-revocation publication denial, org kill switch, retention, 06:00 Bogotá and DST conversion, minimum interval, missing/unsupported/unauthenticated Claude runtime, fixed prompt authority, bounded stream parsing, deterministic evaluator/context manifest, exact-value secret canary redaction, Slack destination validation, GitHub marker reconciliation and repository path validation.

The web backend no longer owns long Claude processes. `autonomous_worker` is a distinct same-host process, refuses in-memory or pre-v62 databases, and is included in the backend image. The runbook requires one separately supervised worker using the backend OS account and persistent database.

## Manual/provider gates

The following need deployment credentials and are release gates, not silently claimed as local successes:

1. Authenticate the pinned Claude Code binary as the backend OS account and exercise expiry → `reauth_required` → host reauthentication → Check again.
2. Install the development GitHub App in two fixture repositories and replay clean, duplicate, changed-payload, stale-head, fork and revoked-connector webhooks.
3. Run QA at 06:00 in NexusMind-only, Slack and GitHub issue modes; confirm one deduplicated finding and no canary leak.
4. Resolve one eligible and one blocked issue; confirm only the eligible case creates a bounded draft PR.
5. Review clean/problematic/stale/fork PR fixtures; confirm COMMENT/REQUEST_CHANGES only and no duplicate review.
6. Cancel a live Claude run and inspect process cleanup, lease release and terminal timeline.

These gates have not been marked complete. They require a real authenticated
Claude host account, provider installations/webhooks and two controlled pilot
repositories; none are present in this checkout.

## Known external blocker

`apps/backend/src/bin/migrate_knowledge.rs` already contains unresolved merge-conflict markers. It is unrelated and was preserved. Consequently commands that compile every binary, and repository-wide `git diff --check`, fail until its owner resolves that conflict. Library targets and the admin application are independently verifiable.

The repository review authority inventory is also currently reported as corrupted/invalidated by `gentle-ai`. Per the repository review policy, this requires explicit maintainer repair/recovery before a valid post-apply receipt can be issued. This is a release gate; it is not bypassed by the green test suites.
