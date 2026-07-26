# Tasks: Autonomous Loop Engineering

## Review Workload Forecast

| Field | Value |
|---|---|
| Estimated changed lines | 2,800–3,800 authored lines |
| 800-line session budget risk | High |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | Feature Branch Chain: PR 1 → PR 7 |
| Delivery strategy | auto-chain |
| Chain strategy | feature-branch-chain |

Decision needed before apply: No
Chained PRs recommended: Yes
Chain strategy: feature-branch-chain
400-line budget risk: High

## Phase 1: Persistence and Authority

- [x] 1.1 RED: Add `apps/backend/tests/automation_db.rs` for v57 org scope, immutable receipts, idempotent callbacks, and revocation denial.
- [x] 1.2 GREEN: Add user-applied v57 plus `apps/backend/src/db/{migrations,queries}.rs` records/triggers; do not run migrations.
- [x] 1.3 RED: Test unauthorized profile/provider, repo widening, read-only writes, and extension failure in `apps/backend/tests/automation_policy.rs`.
- [x] 1.4 GREEN: Create `apps/backend/src/automation/{mod,profiles,policy,provenance}.rs` and RBAC DTO/routes in `apps/backend/src/api/automation.rs`.

## Phase 2: Worker and Governed Execution

- [x] 2.1 RED: In `workers/src/claude_code.test.ts`, reject injected argv, shell/path escape, TTY, unpinned extensions, and malformed stream events.
- [x] 2.2 GREEN: Create `workers/src/{worker,provider,claude_code,sandbox,events}.ts` with fixed noninteractive invocation, strict settings, caps, redaction, and secret teardown.
- [ ] 2.3 RED: Test deterministic manifests, expired/cancelled lease, cost/tool/network cap, and evaluator block in `apps/backend/tests/automation_orchestration.rs`.
- [ ] 2.4 GREEN: Add `apps/backend/src/automation/leases.rs`, context/gates/evaluation wiring, and `lib.rs`/`config.rs` registration.

## Phase 3: GitHub, QA, and Dashboard

- [ ] 3.1 RED: Test signature/replay/org-repo binding and revocation-before-recheck prevents merge/handoff in `apps/backend/tests/automation_github.rs`.
- [ ] 3.2 GREEN: Add App-only writes and preserved QA handoff in `apps/backend/src/automation/{github,qa}.rs` and `api/webhooks.rs`.
- [ ] 3.3 RED/GREEN: Add `apps/admin/src/pages/Automation.test.tsx` then `Automation.tsx`, `types.ts`, `api/client.ts`, `App.tsx`, and `Layout.tsx` for RBAC, evidence, deny, cancel, revoke, and QA branch validation.

## Phase 4: End-to-End Verification

- [ ] 4.1 Add `tests/automation/e2e.test.ts` proving receipt provenance, no post-stop writes, QA human validation, and rollback evidence across fake worker/App.
- [ ] 4.2 Document profile flags, pilot rollout, user-applied migration, and kill-switch verification in `docs/automation-operations.md`.
