# Spec — frontend-gaps

Behavioral contract for the four areas. Every requirement is testable; the
Tasks phase will map each `REQ-*` ID to one or more test cases.

## Area 1 — Backend export endpoints

### REQ-EXP-001 — Audit CSV export route

The backend MUST expose `GET /v1/audit/export`.

- Authentication: same `auth_mw::auth` middleware that protects `/v1/audit`.
- Authorization: requires `audit:read` permission (admin role by default).
- Query parameters:
  - `format` (optional, default `csv`) — one of `csv`, `json`. Any other value
    returns HTTP 400 with `code: "validation_error"`.
- Filters (optional, identical semantics to `GET /v1/audit`):
  `user_id`, `action`, `resource_type`, `from`, `to`.
- No `limit`/`offset`. The endpoint MUST return all matching rows for the
  caller's `org_id`, up to a hard server cap of 10 000 rows. When the cap is
  hit the response header `X-Export-Truncated: true` MUST be set.
- On success returns HTTP 200 with:
  - `Content-Type: text/csv; charset=utf-8` (for `csv`) or
    `application/json; charset=utf-8` (for `json`).
  - `Content-Disposition: attachment; filename="audit-{YYYY-MM-DD}.csv"`
    (or `.json`), where `{YYYY-MM-DD}` is today's UTC date.

### REQ-EXP-002 — Audit CSV columns

When `format=csv`, the response body MUST be RFC 4180 compliant with the
following header row, in this exact order:

```
id,action,resource_type,resource_id,actor_id,metadata,created_at,previous_hash,current_hash
```

- `metadata` MUST be the JSON-serialized object as a single CSV cell (quoted,
  internal `"` doubled per RFC 4180).
- `resource_id`, `previous_hash` MUST be empty strings (not `null`) when absent.
- `created_at` MUST be RFC 3339 UTC.
- Cells whose first character is `=`, `+`, `-`, or `@` MUST be prefixed with a
  single apostrophe `'` to defuse CSV-injection in spreadsheet apps.

### REQ-EXP-003 — Audit JSON export shape

When `format=json`, the response body MUST be a JSON array of `AuditEntry`
objects identical to what `GET /v1/audit` returns today. The only difference
versus `GET /v1/audit` is the `Content-Disposition` header forcing a download.

### REQ-EXP-004 — Memory CSV export route

The backend MUST expose `GET /v1/memory/export`.

- Authentication and authorization: same as `GET /v1/memory` (any authenticated
  user with the existing memory-list permission).
- Query parameters: `format` (optional, default `csv`, values `csv|json`).
- Server cap of 10 000 rows scoped to the caller's `org_id`.
  `X-Export-Truncated: true` when capped.
- Response headers: same `Content-Type` / `Content-Disposition` pattern as
  REQ-EXP-001 with filename `memories-{YYYY-MM-DD}.csv` or `.json`.

### REQ-EXP-005 — Memory CSV columns

When `format=csv`, header row in this exact order:

```
id,title,type,scope,project,tool,content,created_at
```

- `content` MUST be truncated to the first 500 Unicode scalar values (chars,
  not bytes). When truncation happens, append the literal suffix `…` to that
  cell.
- `title`, `type`, `scope` MUST be empty strings when absent.
- Same RFC 4180 quoting and CSV-injection defusing as REQ-EXP-002.

### REQ-EXP-006 — Memory JSON export shape

When `format=json`, the response body MUST be a JSON array of `Memory` objects
identical to `GET /v1/memory` (no content truncation). `Content-Disposition`
forces a download.

### REQ-EXP-007 — Authorization failures

- Missing/invalid auth on either export endpoint MUST return HTTP 401 with the
  standard `ApiError` JSON body (`code: "unauthorized"`), regardless of `format`.
- Authenticated caller without the required permission MUST return HTTP 403
  with `code: "forbidden"` (matches existing `require_permission` behavior).
- Error responses MUST be `application/json`; the `Content-Disposition` header
  MUST NOT be present on error responses.

### REQ-EXP-008 — Org isolation

Both export endpoints MUST scope rows to the caller's `org_id`. Cross-org rows
MUST NOT appear in any export, even if the underlying query would otherwise
return them.

## Area 2 — Admin UI export buttons

### REQ-UI-001 — AuditLog export buttons

`/pages/AuditLog.tsx` MUST render two buttons in the page header, visually
adjacent to the existing "Export CSV":

- "Export CSV" — triggers download via `GET /v1/audit/export?format=csv`.
- "Export JSON" — triggers download via `GET /v1/audit/export?format=json`.

The existing client-side CSV builder (current lines 68-85) MUST be removed.

### REQ-UI-002 — Memories export buttons

`/pages/Memories.tsx` MUST render two buttons in the page header:

- "Export CSV" — triggers `GET /v1/memory/export?format=csv`.
- "Export JSON" — triggers `GET /v1/memory/export?format=json`.

The existing client-side CSV builder (current lines 246-269) MUST be removed.

### REQ-UI-003 — Download mechanism

Both pages MUST use a shared `downloadExport(url, filename)` helper located at
`src/lib/download.ts` that:

1. Issues an authenticated `fetch` (Bearer token in `Authorization` header,
   sourced from `AuthContext` / `createClient`'s existing mechanism). The token
   MUST NOT appear in the URL or in any query parameter.
2. Reads the response body as a `Blob`.
3. Triggers a download by creating a temporary `<a>` element, setting
   `href = URL.createObjectURL(blob)`, calling `click()`, and revoking the
   object URL.
4. On non-2xx responses, MUST NOT trigger a download. It MUST throw an `Error`
   whose `message` is the server-provided `error` field if present, otherwise
   `HTTP {status}`.

### REQ-UI-004 — Button disabled state

Export buttons MUST be disabled while a download is in flight (to prevent
double-click). After completion (success or failure) they MUST be re-enabled.

### REQ-UI-005 — Filename derivation

The filename passed to `downloadExport` MUST match the format requested:
`audit-{YYYY-MM-DD}.csv|json` and `memories-{YYYY-MM-DD}.csv|json`. The date
is computed client-side from `new Date().toISOString().slice(0, 10)`.

### REQ-UI-006 — Permission visibility

The audit export buttons MUST only render when `session.user.role === 'admin'`
(matches the existing `AdminRoute` gate on `/audit`). Memory export buttons
render for any authenticated user.

## Area 3 — Admin Vitest tests

### REQ-TEST-001 — Test runner configured

`apps/admin` MUST run `npm run test` and `npm run test:watch`, both backed by
Vitest with `jsdom` environment. `npm run test` MUST exit non-zero on any test
failure.

### REQ-TEST-002 — Test setup file

A `src/test/setup.ts` MUST exist and MUST import `@testing-library/jest-dom`
so DOM matchers like `toBeInTheDocument` are available in every test.

### REQ-TEST-003 — Required test cases

At minimum, the following 5 tests MUST exist and pass:

1. **Login submit** — `pages/Login.test.tsx` renders `<Login />`, fills email
   and password, submits, and asserts that the mocked auth client was called
   with the entered credentials.
2. **AuditLog filter apply** — Renders `<AuditLog />` with mocked client.
   User selects an action filter and clicks "Apply". Asserts that the next
   `getAuditLog` call was made with `{ action: <selected> }` and that page
   index reset to 0.
3. **AuditLog clear filter** — Starting from an applied filter, user clicks
   "Clear". Asserts that draft state is reset (empty selects) and the next
   `getAuditLog` call has no filter fields.
4. **AuditLog CSV export** — User clicks "Export CSV". Asserts that the
   `downloadExport` helper was called with a URL ending in
   `/v1/audit/export?format=csv` and a filename matching
   `/^audit-\d{4}-\d{2}-\d{2}\.csv$/`.
5. **Memories JSON export** — User clicks "Export JSON". Asserts that
   `downloadExport` was called with a URL ending in
   `/v1/memory/export?format=json` and a filename matching
   `/^memories-\d{4}-\d{2}-\d{2}\.json$/`.

### REQ-TEST-004 — No real network

No test in this suite may issue a real network request. `fetch` and the
api client MUST be stubbed via `vi.fn()` or `vi.mock()`.

### REQ-TEST-005 — CI integration

The `admin` job in `.github/workflows/ci.yml` MUST run `npm run test` after
`npm run build`. Test failures MUST fail the job.

## Area 4 — Backoffice Dockerfile + CI

### REQ-BO-001 — Dockerfile present

`apps/backoffice/Dockerfile` MUST exist and MUST follow the same multi-stage
pattern as `apps/admin/Dockerfile`:

- Stage 1: `node:20-alpine`, run `npm ci`, run `npm run build`.
- Stage 2: `nginx:alpine`, copy build output, expose port 80.

### REQ-BO-002 — Docker build succeeds

`docker build apps/backoffice` MUST succeed against a clean checkout from a
fresh working copy.

### REQ-BO-003 — CI job present

`.github/workflows/ci.yml` MUST contain a `backoffice` job that:

- Runs on `ubuntu-latest`.
- Checks out the repo.
- Sets up Node 20 with npm cache keyed on `apps/backoffice/package-lock.json`.
- Runs `cd apps/backoffice && npm ci`.
- Runs `cd apps/backoffice && npm run build`.
- Fails the workflow if either command exits non-zero.

### REQ-BO-004 — Lockfile committed

`apps/backoffice/package-lock.json` MUST be committed (it is currently
missing). The Tasks phase generates it via `npm install` and verifies it
matches `package.json`.

### REQ-BO-005 — Docker compose updated (if applicable)

If `docker-compose.yml` exists at the repo root and currently lists `admin`,
it MUST also list `backoffice` with a build context of `./apps/backoffice` and
a unique host port. If no compose file exists or `admin` is not in it, this
requirement is a no-op (record the finding in the apply progress).
