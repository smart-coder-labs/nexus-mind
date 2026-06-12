# Archive Report — frontend-gaps

**Status**: VERIFIED AND ARCHIVED
**Date**: 2026-06-11
**Change**: frontend-gaps
**Archived by**: sdd-archive (via sdd-verify)

---

## What Was Implemented

### Area 1 — Backend CSV/JSON Export Endpoints (Rust)

Two new authenticated `GET` endpoints added to `apps/backend`:

- `GET /v1/audit/export` — exports audit log as CSV or JSON, scoped to org, hard cap 10 000 rows, `X-Export-Truncated: true` when capped, RFC 4180 CSV with CSV-injection defusing.
- `GET /v1/memory/export` — exports memories as CSV or JSON, scoped to org, same cap/truncation pattern, content truncated at 500 Unicode scalars with `…` suffix.

Both endpoints reuse existing auth middleware and permission checks. Routes registered in `router.rs`.

**Files changed**: `apps/backend/Cargo.toml`, `apps/backend/src/api/audit.rs`, `apps/backend/src/api/memory.rs`, `apps/backend/src/api/router.rs`, `apps/backend/src/db/queries.rs`

### Area 2 — Admin UI Export Buttons (React)

- `apps/admin/src/lib/download.ts` — shared `downloadExport(url, filename, deps?)` helper: authenticated fetch (Bearer token or cookie), Blob download via `<a>` click, non-2xx throws.
- `apps/admin/src/pages/AuditLog.tsx` — replaced client-side CSV builder with two server-backed export buttons (CSV + JSON), filter forwarding via `URLSearchParams`, `exporting` disabled state, admin-role guard.
- `apps/admin/src/pages/Memories.tsx` — same pattern for memory export.
- `apps/admin/src/auth/AuthContext.tsx` — added `getAuthToken()` module-level export and exported `AuthContext` const.

### Area 3 — Admin Vitest Tests

Vitest + jsdom configured in `apps/admin`. 5 required component tests written and passing:

1. Login form submit
2. AuditLog filter apply
3. AuditLog clear filter
4. AuditLog CSV export (mocked `downloadExport`)
5. Memories JSON export (mocked `downloadExport`)

**Files created**: `apps/admin/src/test/setup.ts`, `apps/admin/src/test/render.tsx`, `apps/admin/src/pages/Login.test.tsx`, `apps/admin/src/pages/AuditLog.test.tsx`, `apps/admin/src/pages/Memories.test.tsx`

**Files modified**: `apps/admin/package.json`, `apps/admin/vite.config.ts`

### Area 4 — Backoffice Dockerfile + CI

- `apps/backoffice/Dockerfile` — multi-stage: `node:20-slim AS builder` → `npm ci` → `npm run build`; `nginx:alpine` serving `/app/dist` on port 80.
- `apps/backoffice/nginx.conf` — SPA fallback + `/v1/` proxy (copied from admin).
- `.github/workflows/ci.yml` — `backoffice` job added (Node 20, `npm ci`, `npm run build`).
- `.github/workflows/ci.yml` — `admin` job now runs `npm run test` after `npm run build` (fixed during verify).
- `docker-compose.yml` — `backoffice` service added (port `3001:80`, depends_on: backend).

---

## Test Results

| Suite | Count | Result |
|-------|-------|--------|
| Backend (Rust / cargo test) | 235 | ALL PASS |
| Admin (Vitest) | 5 | ALL PASS |

Backend breakdown: 216 (main integration suite) + 5 (export tests in audit.rs + memory.rs) + 14 (store/unit tests). Implemented via Strict TDD for Phases 1–2 (RED → GREEN → TRIANGULATE → REFACTOR cycle).

---

## Deviations from Design

| Deviation | Assessment |
|-----------|------------|
| `getAuthToken()` returns `null` (cookie-based auth) | Correct — `credentials: 'include'` added to fetch; design anticipated this |
| `download.ts` adds `credentials: 'include'` | Required for cookie-based session auth |
| `truncate_content` uses `enumerate()` idiom | Cleaner Rust; functionally identical |
| `#[allow(clippy::too_many_arguments)]` on pre-existing function | Pre-existing function, not introduced by this change |

---

## CRITICAL Issues Resolved

| Issue | Resolution |
|-------|------------|
| CRIT-001: Admin CI `npm run test` step missing (REQ-TEST-005) | Fixed inline during verify — step added to `admin` job in `.github/workflows/ci.yml` |

---

## Artifacts Archived

| File | Content |
|------|---------|
| `proposal.md` | Original change proposal |
| `spec.md` | Behavioral spec (35 requirements across 4 areas) |
| `design.md` | Technical design decisions |
| `tasks.md` | Task breakdown with TDD RED/GREEN cycle |
| `apply-progress.md` | Full apply log — all 19 tasks complete |
| `verify-report.md` | Verification report — PASS WITH WARNINGS (1 CRITICAL fixed inline) |
| `archive-report.md` | This file |

---

## Sign-off

**Final status**: VERIFIED — ready for merge / deployment.
**Backend tests**: 235 passing.
**Admin tests**: 5 passing.
**CI**: backend + admin (with test) + backoffice jobs all present and correct.
