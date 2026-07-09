# Tasks: Harness Format Variants

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | ~700-950 changed lines |
| 1000-line budget risk | Low |
| 400-line budget risk | High |
| Chained PRs recommended | No; amend PR #210 under explicit 1000-line budget |
| Suggested split | Single PR amendment with work-unit commits |
| Delivery strategy | single-pr-default |
| Chain strategy | size-exception |

Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: size-exception
400-line budget risk: High
1000-line budget risk: Low

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Backend ownership + typed validation | PR #210 | Tests with code; no UI dependency |
| 2 | Admin upload/builder + config review UX | PR #210 | Includes Vitest coverage |
| 3 | Docs/spec archive readiness | PR #210 | Verify and archive updates |

## Phase 1: Backend RED Tests

- [x] 1.1 Add failing migration/query tests in `apps/backend/src/db/queries.rs` for `owner_user_id` backfill, owner join/display, owner filter, and org validation.
- [x] 1.2 Add failing route tests in `apps/backend/src/api/harnesses.rs` for default owner, admin-assigned owner, seven accepted formats, and unsafe manifest 422s.
- [x] 1.3 Add failing validator tests in `apps/backend/src/models/types.rs` for agent, skill file/folder, command, hook `.sh`, output-style, plugin JSON, and theme JSON structures.

## Phase 2: Backend GREEN/REFACTOR

- [x] 2.1 Update `apps/backend/src/db/migrations.rs` with v49 `harnesses.owner_user_id`, backfill from `created_by`, and `(org_id, owner_user_id, status)` index.
- [x] 2.2 Update `apps/backend/src/models/types.rs` with owner DTOs, create input owner field, typed `HarnessFormat`, components, provenance, and security contracts.
- [x] 2.3 Update `apps/backend/src/db/queries.rs` to default owner to caller, validate assigned owners by org, join owner metadata, filter visible harnesses by owner, and validate manifests.
- [x] 2.4 Update `apps/backend/src/api/harnesses.rs` to accept `owner_user_id`, support `?owner_user_id=`, return owner metadata, and map manifest validation failures to 422.

## Phase 3: Admin RED Tests

- [x] 3.1 Add failing `apps/admin/src/api/client.harnesses.test.ts` cases for owner fields, owner filtering, typed manifest payloads, and omitted local mutation fields.
- [x] 3.2 Add failing `apps/admin/src/pages/Harnesses.test.tsx` cases for owner display/filter, seven format templates, file/folder packaging, JSON validation, and executable warning copy.
- [x] 3.3 Add failing `apps/admin/src/pages/Harnesses.test.tsx` coverage for redesigned Claude config review: upload, automatic redaction summary, safe preview, and no raw low-level secret fields.

## Phase 4: Admin GREEN/REFACTOR

- [x] 4.1 Update `apps/admin/src/types.ts` and `apps/admin/src/api/client.ts` with owner, owner filter, typed format/component, and security metadata types.
- [x] 4.2 Update `apps/admin/src/pages/Harnesses.tsx` with owner controls, guided format templates, direct file/folder upload controls, manifest preview, hashes/relative paths, and warnings.
- [x] 4.3 Redesign the `apps/admin/src/pages/Harnesses.tsx` Claude config review section around upload → automatic redaction → preview → approve, hiding raw low-level fields where feasible.

## Phase 5: Apply Verification

- [x] 5.1 Run `cargo test --manifest-path apps/backend/Cargo.toml` and `cd apps/admin && npm run test`; fix only failures tied to this change.

## Next Phases (Non-Apply Notes)

- `sdd-verify` owns verification evidence in `openspec/changes/harness-format-variants/verify-report.md`.
- `sdd-archive` owns merging deltas into `openspec/specs/harness-library/spec.md`, `openspec/specs/harness-config-review/spec.md`, and `openspec/specs/harness-install-approval/spec.md`.
