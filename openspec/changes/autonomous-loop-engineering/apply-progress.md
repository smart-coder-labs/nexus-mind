# Apply Progress: Autonomous Loop Engineering

## Completed Work Units

- **Work unit**: 1 — Schema and receipts
- **Delivery**: Feature Branch Chain, PR 1 targets the draft/no-merge tracker branch.
- **Scope**: Tasks 1.1 and 1.2 only. No policy, RBAC, worker, or later-task changes were started.
- **Migration operation**: Not run against a user database. Verification used only in-memory SQLite fixtures.
- **Work unit**: 2 — Policy, profiles, provenance, and RBAC routes.
- **Delivery**: Feature Branch Chain, PR 2 targets the PR 1 branch.
- **Scope**: Tasks 1.3 and 1.4 only. No worker, leases, GitHub/QA, dashboard, or later-task changes were started.
- **Work unit**: 3 — Isolated Claude Code worker.
- **Delivery**: Feature Branch Chain, first child PR targets `feature/autonomous-loop-engineering`.
- **Scope**: Tasks 2.1 and 2.2 only. No lease/orchestration, GitHub/QA, dashboard, or later-task changes were started.

## Completed Tasks

- [x] 1.1 RED: Added `apps/backend/tests/automation_db.rs` for v57 organization scope, immutable receipts, idempotent callbacks, and revocation denial.
- [x] 1.2 GREEN: Added user-applied v57 and database query helpers for durable runs, attempts, immutable callback receipts, and revocation evidence.
- [x] 1.3 RED: Added policy tests for provider denial, project widening denial, read-only write denial, required extension failure, approved implementation provenance, and injected allowlist rejection.
- [x] 1.4 GREEN: Added managed Claude Code profile resolution, immutable profile provenance DTOs, and RBAC-protected automation profile routes.
- [x] 2.1 RED: Added TypeScript runtime-contract tests rejecting injected args, interactive TTY, non-canonical sandbox paths, unpinned extensions, malformed events, and unsupported event types.
- [x] 2.2 GREEN: Added an isolated Claude Code adapter with fixed noninteractive argv, strict generated settings, approved-extension validation, stream event parsing, redaction, and ephemeral-secret teardown.

### TDD Cycle Evidence

| Task | Test File | Layer | Safety Net | RED | GREEN | TRIANGULATE | REFACTOR |
|---|---|---|---|---|---|---|---|
| 1.1 | `apps/backend/tests/automation_db.rs` | Integration (SQLite) | N/A (new test file); existing migration suite: 154/154 before change | ✅ Written first; initial compile failed because automation query APIs did not exist | ✅ `cargo test --manifest-path apps/backend/Cargo.toml --test automation_db`: 3/3 | ✅ Scope rejection, exact replay, conflicting replay, and post-revocation denial paths | ✅ Minimal fixture helpers and focused assertions retained |
| 1.2 | `apps/backend/tests/automation_db.rs` | Integration (SQLite) | ✅ Existing migration suite: 154/154 before change | ✅ v57 behavior specified before migration/query code | ✅ Automation fixture 3/3; migration suite 154/154; migration idempotency integration test 1/1 | ✅ Valid and cross-org project bindings plus same and conflicting callback replay paths | ✅ Updated legacy current-schema assertions to v57; no behavioral refactor needed |
| 1.3 | `apps/backend/tests/automation_policy.rs` | Unit + Integration (Axum/SQLite) | ✅ `automation_db`: 3/3; router-filtered lib suite: 0 selected, exit 0 | ✅ Written first; initial compile failed because `automation` policy and API modules did not exist | ✅ `cargo test --test automation_policy`: 6/6 | ✅ Unsupported provider, repo widening, read-only write, required-extension failure, approved implementation provenance, RBAC denial, and injected-allowlist rejection | ✅ Extracted pure profile/policy/provenance modules; focused tests remain green |
| 1.4 | `apps/backend/tests/automation_policy.rs` | Unit + Integration (Axum/SQLite) | ✅ `automation_db`: 3/3 before route/module edits | ✅ Route/profile contracts referenced missing modules in task 1.3 RED | ✅ `cargo test --test automation_policy`: 6/6; `cargo test --test automation_db`: 3/3 | ✅ Authorized profile listing and untrusted payload rejection exercise different authorization paths | ✅ No additional refactor needed; `rustfmt --check` passes on all changed automation files |
| 2.1 | `workers/src/claude_code.test.ts` | Runtime contract (Vitest) | N/A (new worker test and source files) | ✅ Written first; `vitest run workers/src/claude_code.test.ts` failed because `./claude_code` did not exist | ✅ `./apps/admin/node_modules/.bin/vitest run workers/src/claude_code.test.ts`: 4/4 | ✅ Covers fixed argv/no shell, injected argv, TTY, canonical sandbox paths, pinned extensions, valid/malformed/unsupported stream events | ✅ Parser, sandbox, provider types, and invocation construction remain separated |
| 2.2 | `workers/src/claude_code.test.ts` | Runtime contract (Vitest) | N/A (new worker source files) | ✅ Uses the task 2.1 test-first contract before adapter implementation | ✅ `./apps/admin/node_modules/.bin/vitest run workers/src/claude_code.test.ts`: 4/4 | ✅ Strict settings include profile caps; composition harness redacts a secret and verifies teardown leaves no secret to redact | ✅ Extracted pure settings, extension-validation, event-parsing, and sandbox helpers; focused tests stay green |

### Test Summary

- **Total tests written**: 13 automation tests across three completed work units.
- **Total tests passing**: 6/6 policy tests; 3/3 automation persistence tests; 4/4 worker runtime-contract tests.
- **Layers used**: Unit (3), Integration (6), Runtime contract (4).
- **Approval tests**: None — additive schema/query work.
- **Pure functions created**: Worker event parsing, extension validation, strict-settings generation, and sandbox-path validation.

## Work Unit Evidence

| Evidence | Exact result |
|---|---|
| Focused test | `cargo test --manifest-path apps/backend/Cargo.toml --test automation_db` — exit 0; 3 passed, 0 failed. The requested `cargo test -p nexusmind-backend automation::db` cannot run because this repository has no root workspace manifest and the backend package is named `nexusmind`; the manifest-scoped command is the repository-equivalent focused command. |
| Runtime harness | In-memory SQLite migration/append-only fixture via `cargo test --manifest-path apps/backend/Cargo.toml --test automation_db` — exit 0; verifies cross-org project rejection, immutable receipt update rejection, exact replay idempotency, conflicting replay rejection, and post-revocation callback denial while retaining prior receipt. |
| Regression verification | `cargo test --manifest-path apps/backend/Cargo.toml db::migrations --lib` — exit 0; 154 passed. `cargo test --manifest-path apps/backend/Cargo.toml --test integration_test migration_idempotency` — exit 0; 1 passed. |
| Rollback boundary | Revert `apps/backend/src/db/migrations.rs` v57, automation query helpers in `apps/backend/src/db/queries.rs`, and `apps/backend/tests/automation_db.rs`; this removes only automation persistence/receipt behavior. |
| Focused test (Work Unit 2) | `cargo test --test automation_policy` from `apps/backend` — exit 0; 6 passed, 0 failed. |
| Runtime harness (Work Unit 2) | In-memory SQLite + Axum auth harness via `cargo test --test automation_policy` — exit 0; 6 passed, 0 failed; verifies `automation:write` denial and admin profile-provenance route behavior without a user database. |
| Regression verification (Work Unit 2) | `cargo test --test automation_db` — exit 0; 3 passed, 0 failed. `rustfmt --edition 2021 --check src/automation/mod.rs src/automation/profiles.rs src/automation/policy.rs src/automation/provenance.rs src/api/automation.rs tests/automation_policy.rs` — exit 0. |
| Rollback boundary (Work Unit 2) | Revert `apps/backend/src/automation/{mod,profiles,policy,provenance}.rs`, `apps/backend/src/api/automation.rs`, the API registrations in `apps/backend/src/{lib.rs,api/mod.rs,api/router.rs}`, and `apps/backend/tests/automation_policy.rs`; this removes only governed profile authorization and RBAC route behavior. |
| Focused test (Work Unit 3) | `./apps/admin/node_modules/.bin/vitest run workers/src/claude_code.test.ts` — exit 0; 1 file passed, 4 tests passed, 0 failed. |
| Runtime harness (Work Unit 3) | In-process isolated-worker composition via `prepareWorkerAttempt` in `workers/src/claude_code.test.ts` — exit 0; validates managed profile settings/caps, approved extension materialization, stream redaction, and ephemeral-secret teardown without requiring a real Claude binary or credentials. |
| Regression verification (Work Unit 3) | `./apps/admin/node_modules/.bin/tsc --noEmit --target ES2022 --module ESNext --moduleResolution bundler --strict workers/src/provider.ts workers/src/events.ts workers/src/sandbox.ts workers/src/claude_code.ts workers/src/worker.ts` — exit 0. |
| Rollback boundary (Work Unit 3) | Revert `workers/src/{worker,provider,claude_code,sandbox,events}.ts` and `workers/src/claude_code.test.ts`; this removes only the isolated Claude Code worker adapter and leaves control-plane policy and persistence intact. |

## Deviations and Issues

- None — implementation follows the schema-and-receipts boundary.
- Repository-wide `cargo fmt --check` is already non-clean in unrelated files. Targeted `rustfmt --check` also reports unrelated pre-existing formatting drift in the large database modules, so no formatter changes were applied.
- The authorization route intentionally fails closed pending the later durable project-binding/lease resolver. It does not accept allowlists or extension authority from a repository or worker payload.

## Remaining Tasks

- [ ] 2.3 through 4.2 remain pending.
