# Apply Progress: Harness Sharing

## Status

Backend and admin/docs slices completed under approved `size-exception`. Verification remediation is complete for the critical blockers; the change is ready for SDD verification rerun.

## Completed Tasks

- [x] 1.1 Add failing visibility/hash/approval/config-review tests in `apps/backend/src/db/queries.rs` test module.
- [x] 1.2 Add failing Axum route tests in `apps/backend/src/api/harnesses.rs` for unauthorized download, metadata-only recommendations, and rejected secret-bearing snapshots.
- [x] 2.1 Modify `apps/backend/src/db/migrations.rs` to create harness tables, indexes, and seed `harness:*` permissions for admin/super users.
- [x] 2.2 Modify `apps/backend/src/models/types.rs` with harness DTOs, manifest schema, approval requests, recommendation responses, and config review payloads.
- [x] 2.3 Modify `apps/backend/src/db/queries.rs` with org/project-visible catalog queries, immutable version publishing, approvals, downloads, and config-review persistence.
- [x] 2.4 Create `apps/backend/src/api/harnesses.rs` with routes for catalog, versions, publish, approval, download, recommendations, and config reviews.
- [x] 2.5 Modify `apps/backend/src/api/router.rs`, `apps/backend/src/api/mod.rs`, and `apps/backend/src/api/helpers.rs` to wire routes and permission checks.
- [x] 3.1 Add failing `apps/admin/src/pages/Harnesses.test.tsx` cases for list/create/publish, approval-confirm download, and config review preview-submit.
- [x] 3.2 Add failing client contract tests near `apps/admin/src/api/client.ts` for harness methods and response mapping if existing test patterns support it.
- [x] 4.1 Modify `apps/admin/src/types.ts` and `apps/admin/src/api/client.ts` with harness, approval, recommendation, and config-review contracts.
- [x] 4.2 Create `apps/admin/src/pages/Harnesses.tsx` with filters, create draft modal, publish JSON validation, detail drawer, approval modal, and config review upload.
- [x] 4.3 Modify `apps/admin/src/App.tsx`, `apps/admin/src/components/Layout.tsx`, and `apps/admin/src/pages/Roles.tsx` to add route, nav gating, and permission labels.
- [x] 5.1 Run `cargo test --manifest-path apps/backend/Cargo.toml` and fix only harness-related failures.
- [x] 5.2 Run `cd apps/admin && npm run test && npm run build` and fix harness UI regressions.
- [x] 5.3 Update `docs/CLAUDE_CODE_PLUGIN.md` and `docs/CURSOR_PLUGIN.md` with approval-first recommendation/download flow and no silent local mutation boundary.

## TDD Cycle Evidence

| Task | Test File | Layer | Safety Net | RED | GREEN | TRIANGULATE | REFACTOR |
|------|-----------|-------|------------|-----|-------|-------------|----------|
| 1.1 | `apps/backend/src/db/queries.rs` | Unit | ⚠️ Initial baseline compile exceeded 120s before code changes | ✅ Query tests written for visibility, hash/approval, config review | ✅ `cargo test --manifest-path apps/backend/Cargo.toml harness --lib`; ✅ full backend suite | ✅ Covered visible org/project rows, inaccessible project rows, hash mismatch, valid redacted upload, rejected secret-bearing upload | ✅ Helpers extracted for manifest validation, hash calculation, JSON mapping |
| 1.2 | `apps/backend/src/api/harnesses.rs` | Integration | N/A (new route module) | ✅ Axum route tests written for unauthorized download, metadata-only recommendations, secret-bearing snapshot rejection | ✅ `cargo test --manifest-path apps/backend/Cargo.toml harness --lib`; ✅ full backend suite | ✅ Covered 403 without manifest exposure, recommendation response without manifest body, 422 secret rejection | ✅ Route helpers centralized DB/lock error mapping |
| 2.1 | `apps/backend/src/db/migrations.rs` | Unit/integration | ✅ Existing migration tests run in full backend suite | ✅ Harness tests required tables and v48 migration before implementation | ✅ `cargo test --manifest-path apps/backend/Cargo.toml`; ✅ `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` | ✅ Idempotency and full migration tests updated for user_version 48 | ✅ Migration kept additive and non-destructive |
| 2.2 | `apps/backend/src/models/types.rs` | Compile-time/unit | ✅ Full type/model suite in backend tests | ✅ Tests and handlers referenced harness DTOs before complete implementation | ✅ Full backend suite passed | ✅ DTOs exercised through query and API tests | ➖ None needed |
| 2.3 | `apps/backend/src/db/queries.rs` | Unit | ⚠️ Baseline compile timeout; existing suite later passed | ✅ Query tests drove visibility, immutable hash/download, approvals, config review persistence | ✅ Full backend suite passed | ✅ Multiple branches covered: visible vs hidden, matching vs mismatched hash, accepted vs rejected config review | ✅ Pure helpers for manifest validation and secret indicator recursion |
| 2.4 | `apps/backend/src/api/harnesses.rs` | Integration | N/A (new route module) | ✅ Route tests drove recommendation/download/config-review boundaries | ✅ Full backend suite passed | ✅ Metadata-only and secret rejection paths covered | ✅ Safe audit metadata added for download, approval, config review |
| 2.5 | `apps/backend/src/api/router.rs`, `apps/backend/src/api/mod.rs`, `apps/backend/src/db/queries.rs` | Integration/compile-time | ✅ Full backend route suite passed | ✅ API tests required routed harness module | ✅ Full backend suite and clippy passed | ✅ Authenticated admin and unauthorized member paths covered | ✅ Reused existing `require_permission` pattern; no helper changes needed |
| 5.1 | Backend test command | Verification | N/A | N/A | ✅ `cargo test --manifest-path apps/backend/Cargo.toml` passed | N/A | ✅ `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` passed |
| 3.1 | `apps/admin/src/pages/Harnesses.test.tsx` | Integration | N/A (new page test file) | ✅ Page tests written for target filtering, create/publish, approval-confirm download, and config review preview-submit before marking task complete | ✅ `npm run test -- src/pages/Harnesses.test.tsx src/api/client.harnesses.test.ts` passed | ✅ Covered list filter, create draft, publish JSON, approval-before-download, and redaction report preview branches | ✅ Assertions stay user-visible and avoid CSS/class coupling |
| 3.2 | `apps/admin/src/api/client.harnesses.test.ts` | Unit/contract | ✅ Existing client request behavior covered by full admin suite after implementation | ✅ Client contract tests written for list filter, publish/approve/download, and config review contracts before marking task complete | ✅ `npm run test -- src/pages/Harnesses.test.tsx src/api/client.harnesses.test.ts` passed | ✅ Covered exact version URLs, manifest-hash approval, metadata-only download response, and redacted config review mapping | ✅ Reused existing `NexusMindClient` request helper patterns |
| 4.1 | `apps/admin/src/types.ts`, `apps/admin/src/api/client.ts` | Unit/contract | ✅ Harness client contract tests passed before task completion | ✅ Harness client tests referenced the typed API methods and response contracts | ✅ Relevant contract tests and full admin suite passed | ✅ Covered catalog, version publishing, approval, download, recommendations, and config-review methods/types | ✅ Kept contracts colocated with existing admin API/type files |
| 4.2 | `apps/admin/src/pages/Harnesses.tsx` | Integration | ✅ Harness page tests passed before task completion | ✅ Harness page tests drove the new page behavior | ✅ Relevant page tests and full admin suite passed | ✅ Covered filters, create modal, publish validation, approval modal, and config review upload preview | ✅ Shared modal/form helper logic kept local to the page for this first slice |
| 4.3 | `apps/admin/src/App.tsx`, `apps/admin/src/components/Layout.tsx`, `apps/admin/src/pages/Roles.tsx` | Compile-time/integration | ✅ Full admin suite passed before task completion | ✅ Route/nav/permission behavior was introduced only after page/client tests existed | ✅ Full admin test and build passed | ✅ Route is available at `/harnesses`; sidebar is gated by `harness:read`; role editor exposes all harness permissions | ✅ Followed existing admin route/nav/permission label patterns |
| 5.2 | Admin test/build command | Verification | N/A | N/A | ✅ `npm run test && npm run build` passed in `apps/admin` | N/A | ✅ Build completed with existing Vite chunk-size warning only |
| 5.3 | `docs/CLAUDE_CODE_PLUGIN.md`, `docs/CURSOR_PLUGIN.md` | Documentation | N/A | N/A | ✅ Docs updated and admin build/test unaffected | N/A | ✅ Documented approval-first flow and backend no-local-mutation boundary |

## Tests Run

- `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` — passed.
- `cargo test --manifest-path apps/backend/Cargo.toml publish_download_and_approval_preserve_manifest_hash --lib` — passed.
- `cargo test --manifest-path apps/backend/Cargo.toml db::migrations::tests --lib` — passed after updating expected user_version to 48.
- `cargo test --manifest-path apps/backend/Cargo.toml migration_idempotency --test integration_test` — passed after updating expected user_version to 48.
- `cargo test --manifest-path apps/backend/Cargo.toml` — passed.
- `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` — passed.
- `npm run test -- src/pages/Harnesses.test.tsx src/api/client.harnesses.test.ts` from `apps/admin` — passed; 2 files, 7 tests.
- `npm run test` from `apps/admin` — passed; 12 files, 73 tests.
- `npm run build` from `apps/admin` — passed; Vite reported pre-existing large chunk warning for bundles over 500 kB.

## Remaining Tasks

- None.

## Notes

- The backend does not mutate local Claude/Codex/OpenCode files. It only stores harness metadata/manifests, approval state, recommendations, and redacted config reviews.
- Recommendation responses intentionally omit manifest content and include approval metadata only.
- Download and approval audit metadata stores IDs, target, status, and manifest hash only; no manifest bodies or local config content are logged.
- Admin UI download flow records approval before manifest download and tells users local tools must show a diff before applying changes.
- Plugin docs now describe the approval-first recommendation/download contract for Claude Code and Cursor.

## Remediation Progress — 2026-07-07

### Fixes Completed

- [x] Manifest download now requires a persisted approval row for the requesting user, exact harness version, and immutable manifest hash before returning manifest content.
- [x] Backend query and route tests now assert download is blocked before approval and succeeds after approval.
- [x] Local install result reporting is implemented with a backend query, route, API client method, and tests; recorded metadata stores install status/counts without raw local file contents.
- [x] `Graph.test.tsx` now targets the empty-state description instead of the broad `/Select a project/i` text shared by the select placeholder and empty-state title.

### Remediation TDD Cycle Evidence

| Task | Test File | Layer | Safety Net | RED | GREEN | TRIANGULATE | REFACTOR |
|------|-----------|-------|------------|-----|-------|-------------|----------|
| Approval-before-download | `apps/backend/src/db/queries.rs`, `apps/backend/src/api/harnesses.rs` | Unit + integration | ✅ `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` passed before changes | ✅ Query/API tests failed until approval-aware download signature and route behavior were implemented | ✅ `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` passed: 8/8 | ✅ Covered before-approval denial and after-approval success with exact manifest hash | ✅ Reused existing approval rows and safe audit metadata |
| Local install result recording | `apps/backend/src/db/queries.rs`, `apps/backend/src/api/harnesses.rs`, `apps/admin/src/api/client.harnesses.test.ts` | Unit + integration/contract | ✅ Existing harness backend and admin focused tests passed before implementation; client test failed RED on missing method | ✅ Tests referenced missing install-result query/route/client method first | ✅ Backend harness tests and admin focused client/Graph tests passed | ✅ Covered persisted install status plus metadata count and asserted raw file contents are absent | ✅ Kept install result in approval metadata to avoid new migration churn |
| Admin full test blocker | `apps/admin/src/pages/Graph.test.tsx` | Integration | ✅ Focused Graph test passed in isolation; verify report showed full-suite failure from broad text matching | ✅ Existing assertion matched multiple `Select a project` nodes in full suite | ✅ `npm run test` passed: 12 files, 73 tests | ✅ Empty-state test now asserts user-visible description that is unique to the empty state | ➖ None needed |

### Remediation Tests Run

- `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` — passed; 8 tests.
- `cargo test --manifest-path apps/backend/Cargo.toml` — passed; 652 lib tests, 6 backup API tests, 4 backup restore tests, 17 HTTP auth tests, 24 integration tests, doc-tests passed.
- `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` — passed; Cargo reported the existing sqlx-postgres future-incompatibility warning.
- `npm run test -- src/api/client.harnesses.test.ts src/pages/Graph.test.tsx` from `apps/admin` — passed; 2 files, 9 tests.
- `npm run test` from `apps/admin` — passed; 12 files, 73 tests.
- `npm run build` from `apps/admin` — passed; Vite reported the existing large chunk warning.
- `rustfmt --edition 2021 --check apps/backend/src/db/queries.rs apps/backend/src/api/harnesses.rs apps/backend/src/api/router.rs apps/backend/src/models/types.rs` — passed for changed Rust files. Full `cargo fmt --manifest-path apps/backend/Cargo.toml --check` still reports pre-existing formatting drift in unrelated backend test files.

### Remaining Blockers

- None for the critical verification blockers addressed in this remediation batch. Full SDD verification should be rerun.

## Fresh Review Remediation — 2026-07-07

### Fixes Completed

- [x] Approval and download now apply the same project-scoped harness visibility boundary used by catalog/inspection before resolving the harness version for non-privileged callers.
- [x] Focused backend query and Axum route tests now prove a non-privileged user with `harness:install`/`harness:download` permissions cannot approve or download a project-scoped harness outside project membership by ID.
- [x] Config review validation now scans both `redacted_config` and `redaction_report` for secret indicators, including NexusMind-style `nm_*` API keys.
- [x] Redaction reports and install-result metadata now reject nested suspicious raw content keys such as `raw_file_contents`, `raw_shell_content`, `raw_hook_content`, `shell_profile`, and `hook_args`.

### Remediation TDD Cycle Evidence

| Task | Test File | Layer | Safety Net | RED | GREEN | TRIANGULATE | REFACTOR |
|------|-----------|-------|------------|-----|-------|-------------|----------|
| Project-scoped approval/download visibility | `apps/backend/src/db/queries.rs`, `apps/backend/src/api/harnesses.rs` | Unit + integration | ✅ `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` passed before changes: 8/8 | ✅ Tests first referenced project-visible approval/download boundaries and failed before signature/visibility implementation | ✅ `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` passed: 10/10 | ✅ Covered hidden project denial plus allowed project success at query layer, and hidden project denial through Axum for a custom non-privileged role with install/download permissions | ✅ Shared `get_visible_harness_version` helper reuses existing project visibility semantics |
| Redaction report and install metadata raw-content boundary | `apps/backend/src/db/queries.rs` | Unit | ✅ Same harness safety net passed before changes | ✅ Tests first asserted `nm_*` in `redaction_report`, raw shell report content, and nested install `raw_file_contents` are rejected | ✅ `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` passed: 10/10 | ✅ Covered accepted redacted report, failed scan status, redacted config secret, report secret, report raw shell content, and nested install metadata raw file content | ✅ Added recursive suspicious-key scan and extended existing recursive secret scan |

### Remediation Tests Run

- `cargo test --manifest-path apps/backend/Cargo.toml harness --lib` — passed; 10 tests.
- `cargo test --manifest-path apps/backend/Cargo.toml` — passed; 654 lib tests, 6 backup API tests, 4 backup restore tests, 17 HTTP auth tests, 24 integration tests, doc-tests passed.
- `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` — passed; Cargo reported the existing sqlx-postgres future-incompatibility warning.
- `rustfmt --edition 2021 --check apps/backend/src/db/queries.rs apps/backend/src/api/harnesses.rs` — passed.

### Remaining Blockers

- None for the fresh review blockers addressed in this remediation batch.
