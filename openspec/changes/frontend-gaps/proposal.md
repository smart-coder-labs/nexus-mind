# Proposal — frontend-gaps

## Intent

Close four well-known gaps between the admin UI, the backoffice UI, the backend
audit/memory APIs, and the CI pipeline. The current admin frontend has
client-side-only "Export CSV" buttons that operate on whatever page of data the
user happens to have loaded; there is no server-authoritative export, no JSON
export at all, and no tests covering the React surface. The `backoffice` app
ships without a Dockerfile or a CI job, so it cannot be built or deployed
through the same pipeline as `admin`.

We want one consistent story: the backend owns export, the admin UI just
triggers downloads, both frontends are buildable in CI, and the admin app has a
test suite that survives refactors.

### Why now

- Customers (Acme Corp pilot) have asked for full audit-log exports for SOC2
  review, not just "what is currently on screen". The on-page CSV in
  `AuditLog.tsx` (lines 68-85) is paginated to 50 rows and is misleading for
  compliance use.
- `Memories.tsx` has the same problem (lines 246-269): exports only the
  hydrated list/search result, never the full corpus.
- The `apps/backoffice` workspace is silently absent from `ci.yml`; a broken
  build there can land on `main` undetected.
- We added strict TDD to the project but `apps/admin` has zero tests, so the
  policy is unenforceable on the frontend.

### Success looks like

- `GET /v1/audit/export?format=csv|json` and `GET /v1/memory/export?format=csv|json`
  return the entire org-scoped dataset with proper `Content-Disposition` headers,
  gated by the existing admin-role permission system.
- The admin "Export CSV" / "Export JSON" buttons download the server response
  via fetch + blob, regardless of pagination.
- `apps/admin` runs `vitest` in CI with at least 5 meaningful component tests.
- `apps/backoffice` has a Dockerfile mirroring `apps/admin/Dockerfile` and a CI
  job that runs `npm ci && npm run build` on every push and PR.

## Scope

### In scope

1. **Backend export endpoints** (Rust / axum)
   - `GET /v1/audit/export?format=csv|json`
   - `GET /v1/memory/export?format=csv|json`
   - RFC 4180 CSV serialization via the `csv` crate.
   - Permission gates: `audit:read` for audit export, the existing memory list
     permission for memory export.
   - `Content-Disposition: attachment; filename=…` for both formats.

2. **Admin UI export buttons** (React / TypeScript)
   - Replace the existing client-side CSV builders in `AuditLog.tsx` and
     `Memories.tsx` with calls to the new server endpoints.
   - Add a sibling "Export JSON" button.
   - Use authenticated `fetch` + `Blob` + `URL.createObjectURL` (no token in
     query string).

3. **Admin Vitest suite**
   - Install `vitest`, `@testing-library/react`, `@testing-library/jest-dom`,
     `jsdom`, `@testing-library/user-event`.
   - Configure Vitest via `vite.config.ts` (`test` block) with `jsdom`
     environment and a `src/test/setup.ts` for `@testing-library/jest-dom`.
   - Write 5 component tests: Login submit, AuditLog filter apply,
     AuditLog clear filter, AuditLog CSV export button triggers correct URL,
     Memories JSON export button triggers correct URL.
   - Add `"test": "vitest run"` and `"test:watch": "vitest"` scripts.

4. **Backoffice Dockerfile + CI**
   - Mirror `apps/admin/Dockerfile` (multi-stage Node build, nginx serve).
   - Add a `backoffice` job to `.github/workflows/ci.yml` modeled on `admin`.

### Out of scope

- Refactoring the existing `Memories.tsx` markdown renderer or modal layout.
- Adding new fields to the audit log or memory schemas.
- Changing the auth model, rate limits, or permission tables.
- Streaming/chunked CSV. We build the full body in memory; org-scoped audit and
  memory tables are small enough (10s of MB worst case) that streaming is
  premature.
- Internationalization of button labels or downloaded filenames.
- E2E coverage of the download flow — Vitest component tests stub `fetch`; an
  E2E pass is a follow-up change.
- Backoffice tests. This proposal only unblocks its build pipeline.

## Approach

### Backend (Area 1)

Add a single `audit::export` handler and a single `memory::export` handler.
Each handler:

1. Reads the existing permission gate (`require_permission(&conn, &ctx, …)`).
2. Reuses the same DB query as the list endpoint but with a higher hard cap
   (10 000 rows for the first cut — see Design for the rationale).
3. Branches on `?format=` (default `csv`) into either:
   - `csv::Writer::from_writer(Vec::new())` building an RFC 4180 body, OR
   - `serde_json::to_vec_pretty(&rows)`.
4. Returns an `axum::response::Response` with `Content-Type`,
   `Content-Disposition`, and the body bytes.

We add `csv = "1"` to `apps/backend/Cargo.toml` (no other deps).

**Rationale for not streaming**: org-scoped audit volume for the Acme pilot is
under 50 000 rows total. A buffered response is simpler, lets us set
`Content-Length` honestly, and works with our existing JSON-error wrapper. If
multi-million-row exports ever become real, we add `?cursor=` pagination — that
is a separate spec.

**Rationale for `?format=` instead of two routes**: `/export?format=csv` keeps
the surface narrow, mirrors how Stripe/GitHub do it, and lets us add `?format=ndjson`
later without another route.

### Admin UI (Area 2)

Introduce a tiny `src/lib/download.ts` helper:

```ts
export async function downloadExport(
  url: string,
  filename: string,
  fetcher: typeof fetch = fetch,
)
```

It runs an authenticated fetch (the existing `createClient` already attaches
the Bearer token via the `Authorization` header on the `fetch` it wraps), pulls
the blob, and triggers an anchor click. `AuditLog.tsx` and `Memories.tsx` import
it, drop the old hand-rolled CSV code, and render two buttons each.

**Rationale for blob over `window.location.href` redirect**: we cannot put a
Bearer token in a URL without leaking it to server logs and the browser history.
The blob path keeps auth in the `Authorization` header.

### Tests (Area 3)

Vitest + jsdom is the de-facto standard for Vite-based React 19 projects and
shares the same `vite.config.ts` we already maintain. We add a `test` block,
not a separate `vitest.config.ts`, so dev/test config drift is impossible.

We mock `fetch` per test with `vi.fn()` rather than MSW: the 5 tests in scope
do not need HTTP-layer fidelity, only "did the component call the right URL with
the right method?".

### Backoffice (Area 4)

Straight copy of the `apps/admin` pattern. No design decisions, just
plumbing — the change is justified by the gap, not by novelty.

## Risks and open questions

- **CSV injection**: cells starting with `=`, `+`, `-`, `@` can execute as
  formulas in Excel. Resolved in Design — we prefix at-risk cells with `'`.
- **Large exports**: 10 000-row cap is a guess. Need a quick check against
  the largest demo org before we ship; if any org is already past 5 000 audit
  rows, we revise the cap upward and document the memory ceiling.
- **CI cache**: adding `backoffice` doubles the npm install time on CI. We
  reuse `actions/setup-node` cache keyed on `apps/backoffice/package-lock.json`
  — but `apps/backoffice` does not currently have a lockfile committed. The
  Tasks phase must verify/generate it before the CI job will work.
- **Vitest + React 19**: Vitest 2.x supports React 19; we pin Vitest `^2.1`
  and `@testing-library/react` `^16` (the version that ships React 19 types).
  If those versions drift before apply, Design must be revisited.
