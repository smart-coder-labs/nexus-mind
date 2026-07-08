# Tasks: Harness Sharing

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 900-1,200 |
| 400-line budget risk | High |
| 1000-line budget risk | Medium |
| Chained PRs recommended | Yes |
| Suggested split | PR 1 backend harness API → PR 2 admin UI/docs |
| Delivery strategy | single-pr-default |
| Chain strategy | size-exception |

Decision needed before apply: No
Chained PRs recommended: Yes
Chain strategy: size-exception
400-line budget risk: High

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Backend schema, DTOs, queries, API, permissions, tests | PR 1 | Smaller first slice recommended if line count rises. |
| 2 | Admin harness UI, client types, docs, tests | PR 2 | Depends on backend API contract. |

## Phase 1: Backend RED Tests

- [x] 1.1 Add failing visibility/hash/approval/config-review tests in `apps/backend/src/db/queries.rs` test module.
- [x] 1.2 Add failing Axum route tests in `apps/backend/src/api/harnesses.rs` for unauthorized download, metadata-only recommendations, and rejected secret-bearing snapshots.

## Phase 2: Backend Implementation

- [x] 2.1 Modify `apps/backend/src/db/migrations.rs` to create harness tables, indexes, and seed `harness:*` permissions for admin/super users.
- [x] 2.2 Modify `apps/backend/src/models/types.rs` with harness DTOs, manifest schema, approval requests, recommendation responses, and config review payloads.
- [x] 2.3 Modify `apps/backend/src/db/queries.rs` with org/project-visible catalog queries, immutable version publishing, approvals, downloads, and config-review persistence.
- [x] 2.4 Create `apps/backend/src/api/harnesses.rs` with routes for catalog, versions, publish, approval, download, recommendations, and config reviews.
- [x] 2.5 Modify `apps/backend/src/api/router.rs`, `apps/backend/src/api/mod.rs`, and `apps/backend/src/api/helpers.rs` to wire routes and permission checks.

## Phase 3: Admin RED Tests

- [x] 3.1 Add failing `apps/admin/src/pages/Harnesses.test.tsx` cases for list/create/publish, approval-confirm download, and config review preview-submit.
- [x] 3.2 Add failing client contract tests near `apps/admin/src/api/client.ts` for harness methods and response mapping if existing test patterns support it.

## Phase 4: Admin Implementation

- [x] 4.1 Modify `apps/admin/src/types.ts` and `apps/admin/src/api/client.ts` with harness, approval, recommendation, and config-review contracts.
- [x] 4.2 Create `apps/admin/src/pages/Harnesses.tsx` with filters, create draft modal, publish JSON validation, detail drawer, approval modal, and config review upload.
- [x] 4.3 Modify `apps/admin/src/App.tsx`, `apps/admin/src/components/Layout.tsx`, and `apps/admin/src/pages/Roles.tsx` to add route, nav gating, and permission labels.

## Phase 5: Verification and Docs

- [x] 5.1 Run `cargo test --manifest-path apps/backend/Cargo.toml` and fix only harness-related failures.
- [x] 5.2 Run `cd apps/admin && npm run test && npm run build` and fix harness UI regressions.
- [x] 5.3 Update `docs/CLAUDE_CODE_PLUGIN.md` and `docs/CURSOR_PLUGIN.md` with approval-first recommendation/download flow and no silent local mutation boundary.
