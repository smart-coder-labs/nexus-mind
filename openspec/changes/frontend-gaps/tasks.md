# Tasks: frontend-gaps

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 480–600 (5 files backend + 6 files frontend + 2 CI/Docker) |
| 400-line budget risk | High |
| Chained PRs recommended | No — delivery strategy is `auto-chain` / single PR fine per user |
| Suggested split | Single PR (user confirmed under-400-line exception OK for this scope) |
| Delivery strategy | auto-chain |
| Chain strategy | stacked-to-main |

Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: stacked-to-main
400-line budget risk: High

> Note: The 400-line budget is technically exceeded but the change is approved
> as a single PR. All 5 areas are independently revertable with no shared schema
> or migration coupling. `size:exception` is implicitly accepted given the
> `auto-chain` delivery strategy and user's single-PR sign-off.

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Backend export endpoints (T-01) | PR 1 | Strict TDD; self-contained Rust change |
| 2 | Admin UI + Vitest (T-02 → T-04) | PR 2 | Depends on PR 1 for real integration; unit tests mock the client |
| 3 | Backoffice Dockerfile + CI (T-05) | PR 2 or standalone | No runtime deps; can land with or after PR 2 |

---

## Phase 1: Foundation (Cargo dep + test infra)

### T-01-RED — Backend: write failing tests for export endpoints

- [x] 1.1 In `apps/backend/src/api/audit.rs` (test module), write a failing test `export_csv_returns_200` that calls `GET /v1/audit/export?format=csv` via `app_with_post_audit` and asserts HTTP 200 + `Content-Type: text/csv`.
  - **Spec**: REQ-EXP-001, REQ-EXP-002
  - **Note**: `csv` crate not yet in `Cargo.toml`; test will fail to compile — that IS the RED.

- [x] 1.2 Write failing test `export_json_returns_200` asserting HTTP 200 + `Content-Type: application/json`.
  - **Spec**: REQ-EXP-003

- [x] 1.3 Write failing test `export_enforces_auth` asserting that a request without a valid token returns HTTP 401.
  - **Spec**: REQ-EXP-007

- [x] 1.4 Write failing test `export_truncation_header` asserting `X-Export-Truncated: true` is present when rows equal the 10 000 cap.
  - **Spec**: REQ-EXP-001

- [x] 1.5 Write failing test `memory_export_csv_returns_200` in `apps/backend/src/api/memory.rs` (test module).
  - **Spec**: REQ-EXP-004, REQ-EXP-005

- [x] 1.6 Write failing test `memory_export_content_truncated` asserting `content` cell is capped at 500 Unicode scalars with `…` suffix.
  - **Spec**: REQ-EXP-005

- [x] 1.7 Run `cargo test --manifest-path apps/backend/Cargo.toml` — confirm all 6 new tests are RED (compile error or assertion failure).

---

## Phase 2: Core Implementation (Backend GREEN)

### T-01-GREEN — Backend: implement export handlers

- [x] 2.1 Add `csv = "1"` to `[dependencies]` in `apps/backend/Cargo.toml`.
  - **Spec**: REQ-EXP-001 (prerequisite)

- [x] 2.2 In `apps/backend/src/api/audit.rs`, add `ExportParams`, `ExportFormat`, `default_csv`, `defuse`, and `audit_rows_to_csv` exactly as specified in design. Implement `pub async fn export(...)` handler.
  - **Files**: `apps/backend/src/api/audit.rs`
  - **Spec**: REQ-EXP-001, REQ-EXP-002, REQ-EXP-003, REQ-EXP-007, REQ-EXP-008

- [x] 2.3 In `apps/backend/src/api/memory.rs`, add `truncate_content`, `memory_rows_to_csv`, and `pub async fn export(...)` handler mirroring the audit shape. Use the existing memory-list permission — do NOT introduce a new permission.
  - **Files**: `apps/backend/src/api/memory.rs`
  - **Spec**: REQ-EXP-004, REQ-EXP-005, REQ-EXP-006, REQ-EXP-007, REQ-EXP-008

- [x] 2.4 In `apps/backend/src/api/router.rs`, register the two new routes on the `protected` router:
  ```
  .route("/v1/audit/export",  get(audit::export))
  .route("/v1/memory/export", get(memory::export))
  ```
  - **Files**: `apps/backend/src/api/router.rs`
  - **Spec**: REQ-EXP-001, REQ-EXP-004

- [x] 2.5 Run `cargo test --manifest-path apps/backend/Cargo.toml` — confirm all 6 new tests GREEN and existing 224 tests still pass.
  - **Note**: Total must be ≥ 230. Any regression is a blocker.

- [x] 2.6 Run `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` — zero warnings.

---

## Phase 3: Admin UI (Standard Mode — component-level, no TDD RED/GREEN cycle required)

### T-02 — Download helper + page export buttons

- [x] 3.1 Create `apps/admin/src/lib/download.ts` with `downloadExport(url, filename, deps?)` and `todayStamp()` exactly as specified in design.
  - If `getAuthToken` does not exist in `apps/admin/src/auth/AuthContext.tsx`, add it as a sibling export that returns the current session token without a React hook.
  - **Files**: `apps/admin/src/lib/download.ts`, `apps/admin/src/auth/AuthContext.tsx`
  - **Spec**: REQ-UI-003, REQ-UI-005

- [x] 3.2 In `apps/admin/src/pages/AuditLog.tsx`:
  - Remove lines 68-85 (client-side `handleExportCsv`) and the single export button (lines 109-116).
  - Add `exporting` state and `handleExport(format)` callback.
  - Render two buttons ("Export CSV", "Export JSON") forwarding current applied filters via `URLSearchParams`.
  - Disable both buttons while `exporting !== null`.
  - Guard rendering with `session.user.role === 'admin'`.
  - **Files**: `apps/admin/src/pages/AuditLog.tsx`
  - **Spec**: REQ-UI-001, REQ-UI-003, REQ-UI-004, REQ-UI-005, REQ-UI-006

- [x] 3.3 In `apps/admin/src/pages/Memories.tsx`:
  - Remove lines 246-269 (client-side CSV builder).
  - Add the same `exporting` state, `handleExport` callback, and two export buttons using `/v1/memory/export`.
  - **Files**: `apps/admin/src/pages/Memories.tsx`
  - **Spec**: REQ-UI-002, REQ-UI-003, REQ-UI-004, REQ-UI-005

---

## Phase 4: Admin Vitest Setup

### T-03 — Install test runner and configure

- [x] 4.1 In `apps/admin/package.json`:
  - Add scripts: `"test": "vitest run"`, `"test:watch": "vitest"`.
  - Add devDependencies: `vitest@^2.1.8`, `@testing-library/react@^16.1.0`, `@testing-library/jest-dom@^6.5.0`, `@testing-library/user-event@^14.5.2`, `jsdom@^25.0.0`.
  - **Files**: `apps/admin/package.json`
  - **Spec**: REQ-TEST-001

- [x] 4.2 In `apps/admin/vite.config.ts`:
  - Change import from `'vite'` to `'vitest/config'`.
  - Add `test` block: `environment: 'jsdom'`, `globals: true`, `setupFiles: ['./src/test/setup.ts']`, `css: false`, `include: ['src/**/*.{test,spec}.{ts,tsx}']`.
  - **Files**: `apps/admin/vite.config.ts`
  - **Spec**: REQ-TEST-001

- [x] 4.3 Create `apps/admin/src/test/setup.ts` with a single line: `import '@testing-library/jest-dom'`.
  - **Files**: `apps/admin/src/test/setup.ts`
  - **Spec**: REQ-TEST-002

- [x] 4.4 Create `apps/admin/src/test/render.tsx` with a `renderWithProviders(ui)` helper wrapping in `MemoryRouter`, `QueryClientProvider` (fresh client per test), and `AuthProvider` seeded with a mock admin session.
  - **Files**: `apps/admin/src/test/render.tsx`
  - **Spec**: REQ-TEST-004

- [x] 4.5 Run `cd apps/admin && npm install` to generate/update `package-lock.json`. Verify lockfile is consistent.

### T-04 — Write the 5 required Vitest component tests

- [x] 4.6 Create `apps/admin/src/pages/Login.test.tsx`:
  - Mock `createClient`; render `<Login />`; fill email + password; submit; assert mocked auth client called with entered credentials.
  - **Spec**: REQ-TEST-003 test 1, REQ-TEST-004

- [x] 4.7 Create `apps/admin/src/pages/AuditLog.test.tsx` — filter apply test:
  - Mock `createClient`; render `<AuditLog />`; select action filter; click "Apply"; assert `getAuditLog` called with `{ action: <selected> }` and page index reset to 0.
  - **Spec**: REQ-TEST-003 test 2, REQ-TEST-004

- [x] 4.8 AuditLog clear filter test (same file):
  - Starting from applied filter state, click "Clear"; assert draft state reset and next `getAuditLog` call has no filter fields.
  - **Spec**: REQ-TEST-003 test 3, REQ-TEST-004

- [x] 4.9 AuditLog CSV export test (same file):
  - Mock `src/lib/download` (`downloadExport: vi.fn()`, `todayStamp: () => '2026-06-11'`); click "Export CSV"; assert `downloadExport` called with URL ending `/v1/audit/export?format=csv` and filename matching `/^audit-\d{4}-\d{2}-\d{2}\.csv$/`.
  - **Spec**: REQ-TEST-003 test 4, REQ-TEST-004

- [x] 4.10 Create `apps/admin/src/pages/Memories.test.tsx` — JSON export test:
  - Mock download module; click "Export JSON"; assert `downloadExport` called with URL ending `/v1/memory/export?format=json` and filename matching `/^memories-\d{4}-\d{2}-\d{2}\.json$/`.
  - **Spec**: REQ-TEST-003 test 5, REQ-TEST-004

- [x] 4.11 Run `cd apps/admin && npm run test` — all 5 tests pass, exit 0.

---

## Phase 5: Backoffice Dockerfile + CI

### T-05 — Backoffice containerization and CI

- [x] 5.1 Check whether `apps/backoffice/nginx.conf` exists. If not, copy from `apps/admin/nginx.conf`.
  - **Files**: `apps/backoffice/nginx.conf`
  - **Spec**: REQ-BO-001

- [x] 5.2 Create `apps/backoffice/Dockerfile` mirroring the admin pattern:
  - Stage 1: `node:20-slim AS builder` (match admin's actual base, not design's `node:20-alpine`), `npm install`, `npm run build` with `ARG VITE_API_URL`.
  - Stage 2: `nginx:alpine`, copy `/app/dist`, copy `nginx.conf`, expose 80.
  - **Files**: `apps/backoffice/Dockerfile`
  - **Spec**: REQ-BO-001

- [x] 5.3 Run `cd apps/backoffice && npm install` to generate `apps/backoffice/package-lock.json`. Verify it matches `package.json`.
  - **Spec**: REQ-BO-004

- [x] 5.4 In `.github/workflows/ci.yml`:
  - Add `npm run test` step to `admin` job after the Build step (uses `cd apps/admin && npm run test`).
  - Append a new `backoffice` job after `admin`: `ubuntu-latest`, checkout, Node 20 with `cache-dependency-path: apps/backoffice/package-lock.json`, `npm ci`, `npm run build` (with `VITE_API_URL` secret).
  - **Files**: `.github/workflows/ci.yml`
  - **Spec**: REQ-TEST-005, REQ-BO-003

- [x] 5.5 Check for `docker-compose.yml` at repo root. If it exists and contains an `admin` service, add `backoffice` service with `build.context: ./apps/backoffice` and port `3001:80`. If absent or no `admin` service, record the skip in apply progress.
  - **Spec**: REQ-BO-005

- [x] 5.6 Verify `docker build -f apps/backoffice/Dockerfile apps/backoffice` succeeds locally (or note that CI will be the verification gate if Docker is unavailable locally).
  - **Spec**: REQ-BO-002

---

## Dependency Order

```
Phase 1 (T-01 RED)
  → Phase 2 (T-01 GREEN)   [parallel with Phase 3 if needed, no shared files]
  → Phase 3 (T-02)         [download.ts and page changes; no backend dep at code level]
  → Phase 4 (T-03 → T-04)  [must follow T-02 so components exist to test]
  → Phase 5 (T-05)         [fully independent; can land in any order]
```

T-05 (Backoffice) has zero runtime dependencies on Areas 1-3. It can be applied as an isolated commit at any point after Phase 1.
