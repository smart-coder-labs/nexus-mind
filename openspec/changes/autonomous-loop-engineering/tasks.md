# Tasks: Autonomous Loop Engineering

> Ownership reconciliation (2026-08-18): migration v57, profiles, policy and
> provenance remain authoritative foundations. All unfinished autonomous-agent
> product/runtime tasks below are superseded by
> `../autonomous-agents-mvp/`, which owns the same-host Claude Code worker,
> scheduling, GitHub App, QA, issue resolver, PR reviewer and admin surface.

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

- [x] 2.1 Superseded by `autonomous-agents-mvp` Phase 3 malicious/runtime tests.
- [x] 2.2 Superseded by the colocated Rust worker owned by `autonomous-agents-mvp`.
- [x] 2.3 Superseded by `autonomous-agents-mvp` lease, manifest, budget and evaluator tests.
- [x] 2.4 Superseded by `autonomous-agents-mvp` scheduling/lease/runtime implementation.

## Phase 3: GitHub, QA, and Dashboard

- [x] 3.1 Superseded by `autonomous-agents-mvp` Phase 4 and revocation-race gates.
- [x] 3.2 Superseded by the GitHub App and QA implementations owned by `autonomous-agents-mvp`.
- [x] 3.3 Superseded by the permission-gated Autonomous Agents admin surface.

## Phase 4: End-to-End Verification

- [x] 4.1 Superseded by the `autonomous-agents-mvp` external-contract and pilot gates.
- [x] 4.2 Superseded by `docs/autonomous-agents-operations.md`; v57 profile guidance remains retained.
