# Apply Progress: Harness Format Variants

## Status

- Mode: Strict TDD
- Delivery: single PR with approved size exception for PR #210
- Apply-phase status: complete
- Completed implementation/apply tasks: 14/14
- Deferred phase-owned tasks: 5.2 archive merge, 5.3 verify report
- Next recommended: sdd-verify
- Remediation status: critical verification blockers fixed on 2026-07-08
- Follow-up remediation status: upload-derived manifests now capture inline content with real SHA-256, and non-privileged owner reassignment is blocked.

## Completed Tasks

- [x] 1.1 Backend migration/query RED tests for harness ownership.
- [x] 1.2 Backend route RED tests for owner defaults, owner filtering, and manifest validation behavior.
- [x] 1.3 Backend validator RED tests for all supported typed harness formats.
- [x] 2.1 Migration v49 adds `harnesses.owner_user_id`, backfills from `created_by`, and adds owner/status index.
- [x] 2.2 Backend model types now include owner DTOs, `HarnessFormat`, manifest components, provenance, and security contracts.
- [x] 2.3 Backend queries default/validate owners, join owner metadata, filter visible harnesses by owner, and validate typed manifests.
- [x] 2.4 Backend API accepts `owner_user_id`, supports `?owner_user_id=`, returns owner metadata, and maps manifest validation failures to 422.
- [x] 3.1 Admin API client RED tests cover owner fields, owner filtering, typed manifest publish payloads, and no local mutation fields.
- [x] 3.2 Admin page RED tests cover owner display/filter, seven format templates, file upload packaging, JSON validation, and executable warning copy.
- [x] 3.3 Admin page RED tests cover config review explanatory flow and absence of raw low-level fields.
- [x] 4.1 Admin types/API client include owner, owner filter, typed format/component, and security metadata types.
- [x] 4.2 Admin harness page includes owner controls, guided format templates, upload controls, manifest preview, hashes/relative paths, and warnings.
- [x] 4.3 Admin config review section presents upload → automatic redaction → preview → approve flow.
- [x] 5.1 Required backend/admin verification commands were run; failures tied to this change were fixed.

## TDD Cycle Evidence

| Task | RED | GREEN | REFACTOR |
|------|-----|-------|----------|
| 1.1 / 2.1-2.3 | Added failing ownership migration/query coverage for backfill, owner joins, owner filters, and org validation. | Implemented v49 migration and query owner behavior. | Updated latest migration-version assertions to 49 after full-suite failures. |
| 1.2 / 2.4 | Added failing route coverage for owner defaults/assignment/filtering and manifest validation behavior. | Updated harness API request/query handling and validation error mapping. | Kept `created_by` as provenance while exposing `owner_user_id` as catalog ownership. |
| 1.3 / 2.2-2.3 | Added failing typed manifest validator coverage for supported formats and unsafe/mismatched structures. | Implemented `schema_version: "1.1"`, `format`, components, provenance, security, and format/component validation. | Centralized validation error strings for API/query reuse. |
| 3.1 / 4.1 | Added failing admin client tests for owner mapping/filtering and typed publish payloads. | Updated admin types and client filter serialization. | Preserved generic manifest compatibility via `HarnessManifest | Record<string, unknown>`. |
| 3.2 / 4.2 | Added failing admin page tests for owner display/filter, format templates, upload metadata, and executable warnings. | Implemented guided template builder, upload-derived manifest entries, owner filter/display, and warnings. | Fixed async owner-filter test timing and TypeScript duplicate field typing. |
| 3.3 / 4.3 | Added failing config review UX tests for the explanatory upload/redaction/preview/approval flow. | Updated config review copy and preview behavior. | Kept existing JSON entry fields but removed raw low-level framing from the UI. |
| Remediation: executable approvals | Added failing backend query/API tests requiring warning acknowledgement for hook/plugin approvals and an admin approval-modal test that disables approval until acknowledgement. | Backend now rejects missing `warning_acknowledged: true` for high-trust manifests; admin persists the acknowledgement in approval metadata. | Kept enforcement scoped to manifests with warning metadata to avoid changing normal markdown harness approvals. |
| Remediation: folder upload packaging | Added failing admin runtime coverage for two files with `webkitRelativePath`. | Existing packaging path now has passing coverage proving folder components include normalized entries, media type, size, and hash metadata. | No production refactor required beyond the adjacent JSON editor changes. |
| Remediation: plugin/theme JSON and Windows paths | Added failing admin coverage for invalid plugin/theme JSON before publish and backend validator coverage for Windows absolute paths. | Admin rejects non-object/invalid JSON before publish; backend rejects Windows absolute component paths. | Reused existing `parseJsonObject` and added a small Windows path helper. |
| Follow-up remediation: upload content + owner policy | Added failing admin runtime coverage for inline file/folder/plugin content + SHA-256 capture and a failing backend route test for non-privileged cross-owner assignment. | Upload-derived manifests now persist inline content with real SHA-256 hashes and block unsupported multi-file flows; backend only allows privileged users to assign a different owner. | Kept the change scoped to upload-derived manifests and the create-harness permission boundary. |

## Verification Evidence

- `cd apps/admin && npm run test -- Harnesses.test.tsx client.harnesses.test.ts` — PASS (9 tests)
- `cd apps/admin && npm run test` — PASS (12 files, 75 tests)
- `cd apps/admin && npm run build` — PASS
- `cargo test --manifest-path apps/backend/Cargo.toml db::migrations::tests` — PASS (122 tests)
- `cargo test --manifest-path apps/backend/Cargo.toml` — PASS (658 unit tests, integration/doc tests pass)
- `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` — PASS
- `cargo test --manifest-path apps/backend/Cargo.toml executable_harness_approval_requires_and_persists_warning_acknowledgement` — RED then PASS after implementation
- `cargo test --manifest-path apps/backend/Cargo.toml executable_approval_requires_warning_acknowledgement_metadata` — RED then PASS after implementation
- `cargo test --manifest-path apps/backend/Cargo.toml typed_harness_manifest_rejects_mismatched_or_unsafe_structures` — RED for Windows absolute path, then PASS after implementation
- `cd apps/admin && npm run test -- Harnesses.test.tsx` — RED then PASS (7 tests)
- `cargo test --manifest-path apps/backend/Cargo.toml` — PASS (660 unit tests, integration/doc tests pass)
- `cd apps/admin && npm run test` — PASS (12 files, 77 tests)
- `cd apps/admin && npm run build` — PASS with existing Vite chunk-size warning
- `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` — PASS
- `cd apps/admin && npm run test -- Harnesses.test.tsx client.harnesses.test.ts` — PASS (13 tests)
- `cd apps/admin && npm run build` — PASS with existing Vite chunk-size warning
- `cargo test --manifest-path apps/backend/Cargo.toml --lib api::harnesses::tests::` — PASS (9 tests)

## Deferred Tasks

- [ ] 5.2 Archive-phase task: merge the accepted deltas into `openspec/specs/harness-library/spec.md`, `openspec/specs/harness-config-review/spec.md`, and `openspec/specs/harness-install-approval/spec.md` during `sdd-archive`.
- [ ] 5.3 Verify-phase task: create `openspec/changes/harness-format-variants/verify-report.md` during `sdd-verify` before archive.

These tasks intentionally remain unchecked in `tasks.md` during apply because OpenSpec convention assigns verify-report creation to `sdd-verify` and main spec merging/change archival to `sdd-archive`.

## Notes

- `cargo fmt --manifest-path apps/backend/Cargo.toml -- --check` was run as an extra check and reports pre-existing formatting diffs across backend tests outside this change's scope; the required gates above pass.
- No code or docs implementation tasks remain for apply. Archive/spec merge tasks remain for the verify/archive phase.
- Remediation fixed the verification report's CRITICAL findings: executable/plugin approval acknowledgement is now enforced/persisted, folder upload packaging has runtime coverage, and plugin/theme JSON rejection before publish has runtime coverage.
- Adjacent warning addressed: backend typed manifest path validation now rejects Windows absolute paths such as `C:\\Users\\...`.
- Follow-up remediation addressed the fresh review CRITICAL: upload-derived manifests now embed reconstructable content with real SHA-256 hashes, and single-file formats reject unsupported multi-file picker flows before publish.
- Adjacent warning addressed: non-privileged harness writers can no longer forge another user as the harness owner during create.
