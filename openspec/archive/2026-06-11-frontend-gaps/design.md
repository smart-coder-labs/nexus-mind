# Design — frontend-gaps

Concrete implementation choices. Code shapes below are illustrative — Apply
must compile them against the actual current sources, but the structure is
fixed by this design.

## Area 1 — Backend export endpoints

### Cargo.toml addition

```toml
# apps/backend/Cargo.toml
csv = "1"
```

No other dependency changes. `chrono` and `serde_json` are already present.

### Route registration

Both routes go on the `protected` router (same layer that already gates
`/v1/audit` and `/v1/memory`). Add to `apps/backend/src/api/router.rs`:

```rust
.route("/v1/audit/export",  get(audit::export))
.route("/v1/memory/export", get(memory::export))
```

Place `audit::export` immediately after `.route("/v1/audit", get(audit::query))`
and `memory::export` immediately after the memory list route, to keep
diffability tight.

### Handler structure (audit)

New code in `apps/backend/src/api/audit.rs`:

```rust
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension,
};
use serde::Deserialize;

const EXPORT_HARD_CAP: i64 = 10_000;

#[derive(Deserialize)]
pub struct ExportParams {
    #[serde(flatten)]
    pub filters: AuditFilters, // reuses user_id/action/resource_type/from/to
    #[serde(default = "default_csv")]
    pub format: ExportFormat,
}

#[derive(Deserialize, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat { Csv, Json }

fn default_csv() -> ExportFormat { ExportFormat::Csv }

pub async fn export(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Query(params): Query<ExportParams>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "audit:read")?;

    let entries = queries::list_audit(
        &conn,
        &ctx.org_id,
        params.filters.user_id.as_deref(),
        params.filters.action.as_deref(),
        params.filters.resource_type.as_deref(),
        params.filters.from.as_deref(),
        params.filters.to.as_deref(),
        EXPORT_HARD_CAP,
        0,
    ).map_err(db_err)?;

    let truncated = entries.len() as i64 == EXPORT_HARD_CAP;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let (content_type, filename, body) = match params.format {
        ExportFormat::Csv => {
            let body = audit_rows_to_csv(&entries).map_err(db_err)?;
            ("text/csv; charset=utf-8", format!("audit-{today}.csv"), body)
        }
        ExportFormat::Json => {
            let body = serde_json::to_vec_pretty(&entries).map_err(|e| db_err(e.into()))?;
            ("application/json; charset=utf-8", format!("audit-{today}.json"), body)
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static_owned(content_type));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap(),
    );
    if truncated {
        headers.insert("x-export-truncated", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, headers, body).into_response())
}
```

Notes:

- `HeaderValue::from_static_owned` is a placeholder — Apply must use the
  correct constructor (`from_str` is fine; the constants are static strings
  but the lifetime matters for `from_static`).
- `audit_rows_to_csv` is the dedicated serializer below.

### CSV serializer (audit)

```rust
fn audit_rows_to_csv(entries: &[AuditEntry]) -> anyhow::Result<Vec<u8>> {
    let mut wtr = csv::WriterBuilder::new()
        .quote_style(csv::QuoteStyle::Necessary)
        .from_writer(Vec::new());

    wtr.write_record([
        "id", "action", "resource_type", "resource_id",
        "actor_id", "metadata", "created_at",
        "previous_hash", "current_hash",
    ])?;

    for e in entries {
        let metadata = serde_json::to_string(&e.metadata).unwrap_or_else(|_| "{}".to_string());
        wtr.write_record([
            defuse(&e.id),
            defuse(&e.action),
            defuse(&e.resource_type),
            defuse(e.resource_id.as_deref().unwrap_or("")),
            defuse(&e.user_id),
            defuse(&metadata),
            defuse(&e.timestamp),
            defuse(e.previous_hash.as_deref().unwrap_or("")),
            defuse(&e.current_hash),
        ])?;
    }

    Ok(wtr.into_inner()?)
}

/// CSV-injection defuse: prefix risky leading chars with `'`.
fn defuse(s: &str) -> String {
    match s.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') => format!("'{s}"),
        _ => s.to_string(),
    }
}
```

### Handler structure (memory)

`apps/backend/src/api/memory.rs` follows the same shape. Differences:

- Permission gate uses the existing memory-list permission (mirror whatever
  `memory::list` already calls — do NOT introduce a new permission).
- Reuse `queries::list_memories` with `limit = EXPORT_HARD_CAP, offset = 0`.
- CSV header: `id,title,type,scope,project,tool,content,created_at`.
- `content` truncation:

```rust
fn truncate_content(s: &str) -> String {
    const MAX: usize = 500;
    let mut count = 0;
    let mut end = 0;
    for (idx, _) in s.char_indices() {
        if count == MAX {
            end = idx;
            break;
        }
        count += 1;
    }
    if count < MAX {
        s.to_string()
    } else {
        format!("{}…", &s[..end])
    }
}
```

Note: counts Unicode scalar values, not bytes. This matters for emoji and
multi-byte content. Test case in REQ-EXP-005.

### Error envelopes

Re-use the existing `ApiError` / `(StatusCode, Json<ApiError>)` pattern.
Validation errors for unknown `format` values surface as a `serde` deserialize
failure, which axum already maps to 400. We do NOT need to write a custom
validator.

## Area 2 — Admin UI export buttons

### New file: `apps/admin/src/lib/download.ts`

```ts
import { getAuthToken } from '../auth/AuthContext'

export interface DownloadDeps {
  fetcher?: typeof fetch
  createObjectURL?: (b: Blob) => string
  revokeObjectURL?: (u: string) => void
}

export async function downloadExport(
  url: string,
  filename: string,
  deps: DownloadDeps = {},
): Promise<void> {
  const fetcher = deps.fetcher ?? fetch
  const createURL = deps.createObjectURL ?? URL.createObjectURL
  const revokeURL = deps.revokeObjectURL ?? URL.revokeObjectURL

  const token = getAuthToken()
  const headers: Record<string, string> = {}
  if (token) headers.Authorization = `Bearer ${token}`

  const res = await fetcher(url, { method: 'GET', headers })
  if (!res.ok) {
    let message = `HTTP ${res.status}`
    try {
      const body = await res.json()
      if (body?.error) message = body.error
    } catch {
      // body wasn't JSON, keep generic message
    }
    throw new Error(message)
  }

  const blob = await res.blob()
  const objectUrl = createURL(blob)
  const a = document.createElement('a')
  a.href = objectUrl
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  revokeURL(objectUrl)
}

export function todayStamp(): string {
  return new Date().toISOString().slice(0, 10)
}
```

The `DownloadDeps` parameter exists purely for testability — Vitest tests
stub `fetcher`, `createObjectURL`, and `revokeObjectURL` via the deps bag
instead of monkey-patching globals.

`getAuthToken` must be a small export from `AuthContext` that returns the
current session's token without requiring a React hook (so it works outside
the component tree, e.g. inside this helper). If it doesn't exist today,
Apply adds it as a sibling export to `useAuth`.

### AuditLog button rendering

In `apps/admin/src/pages/AuditLog.tsx`, replace lines 68-85 (the local
`handleExportCsv`) and lines 109-116 (the single button) with:

```tsx
const [exporting, setExporting] = useState<null | 'csv' | 'json'>(null)

const handleExport = useCallback(async (format: 'csv' | 'json') => {
  setExporting(format)
  try {
    await downloadExport(
      `/v1/audit/export?format=${format}`,
      `audit-${todayStamp()}.${format}`,
    )
  } finally {
    setExporting(null)
  }
}, [])
```

```tsx
<div className="flex gap-2">
  <button
    onClick={() => handleExport('csv')}
    disabled={exporting !== null}
    className="text-xs ... disabled:opacity-30"
    aria-label="Export audit log as CSV"
  >
    {exporting === 'csv' ? 'Exporting…' : 'Export CSV'}
  </button>
  <button
    onClick={() => handleExport('json')}
    disabled={exporting !== null}
    className="text-xs ... disabled:opacity-30"
    aria-label="Export audit log as JSON"
  >
    {exporting === 'json' ? 'Exporting…' : 'Export JSON'}
  </button>
</div>
```

Memories.tsx gets the same treatment with `/v1/memory/export` and
`memories-{date}.{format}`.

### Filter handling

Apply MUST forward the currently applied audit filters into the export URL
(e.g. `?format=csv&action=store&user_id=abc`). The server already accepts
these query params (REQ-EXP-001). Use `URLSearchParams`:

```ts
const params = new URLSearchParams({ format })
if (filters.user_id)       params.set('user_id', filters.user_id)
if (filters.action)        params.set('action', filters.action)
if (filters.resource_type) params.set('resource_type', filters.resource_type)
if (filters.from)          params.set('from', filters.from)
if (filters.to)            params.set('to', filters.to)
await downloadExport(`/v1/audit/export?${params}`, ...)
```

## Area 3 — Vitest configuration

### Package.json additions

```jsonc
// apps/admin/package.json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "devDependencies": {
    // existing deps preserved
    "@testing-library/jest-dom": "^6.5.0",
    "@testing-library/react": "^16.1.0",
    "@testing-library/user-event": "^14.5.2",
    "jsdom": "^25.0.0",
    "vitest": "^2.1.8"
  }
}
```

### vite.config.ts changes

```ts
import { defineConfig } from 'vitest/config'  // <-- swap from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { resolve } from 'path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': resolve(__dirname, 'src') },
  },
  server: {
    port: 3000,
    proxy: {
      '/v1': { target: 'http://localhost:8080', changeOrigin: true },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    include: ['src/**/*.{test,spec}.{ts,tsx}'],
  },
})
```

`vitest/config` re-exports `defineConfig` with the `test` field typed. The
existing `vite` dev/build flow continues to work because Vitest's config is
a strict superset.

### Setup file

```ts
// apps/admin/src/test/setup.ts
import '@testing-library/jest-dom'
```

### Mocking the api client

Tests stub the `createClient` factory rather than `fetch` directly, because
that is the seam the page components already use:

```ts
// inside a test file
import { vi } from 'vitest'
import * as clientModule from '../api/client'

const mockClient = {
  listUsers:     vi.fn().mockResolvedValue([]),
  getAuditLog:   vi.fn().mockResolvedValue([]),
  listMemories:  vi.fn().mockResolvedValue([]),
  // ...
}
vi.spyOn(clientModule, 'createClient').mockReturnValue(mockClient as any)
```

For the download tests, mock `src/lib/download.ts`:

```ts
vi.mock('../lib/download', () => ({
  downloadExport: vi.fn().mockResolvedValue(undefined),
  todayStamp: () => '2026-06-11',
}))
```

`AuthProvider` and `QueryClientProvider` need real wrappers. Add a tiny
`src/test/render.tsx` with a `renderWithProviders(ui)` helper that wraps
in `MemoryRouter`, `QueryClientProvider` (fresh client per test), and
`AuthProvider` (seeded with a mock admin session via context override or
a test-only `AuthContext.Provider` value).

### CI integration

Add a step to the `admin` job in `.github/workflows/ci.yml` between Install
and Build (or after Build — either is fine; we choose after Build to keep
the failure ordering "compile first, then test"):

```yaml
- name: Test
  run: cd apps/admin && npm run test
```

No `VITE_API_URL` is needed; tests mock the client.

## Area 4 — Backoffice Dockerfile + CI

### `apps/backoffice/Dockerfile`

Mirror of `apps/admin/Dockerfile`. Exact contents:

```dockerfile
# syntax=docker/dockerfile:1

# ── Build stage ────────────────────────────────────────────────────────────
FROM node:20-alpine AS build
WORKDIR /app

COPY package.json package-lock.json* ./
RUN npm ci

COPY . .
RUN npm run build

# ── Runtime stage ──────────────────────────────────────────────────────────
FROM nginx:alpine
COPY --from=build /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
```

If `apps/backoffice/nginx.conf` does not exist, copy the one from
`apps/admin/` (single-page-app fallback to `index.html`) into the backoffice
directory. Apply phase decides between copy and reference based on what's
actually there.

### CI job

Append to `.github/workflows/ci.yml` after the `admin` job:

```yaml
  backoffice:
    name: Backoffice (React)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: npm
          cache-dependency-path: apps/backoffice/package-lock.json

      - name: Install deps
        run: cd apps/backoffice && npm ci

      - name: Build
        run: cd apps/backoffice && npm run build
        env:
          VITE_API_URL: ${{ secrets.VITE_API_URL }}
```

### Lockfile

`apps/backoffice/package-lock.json` is currently missing. Apply runs
`npm install` (not `npm ci`) inside `apps/backoffice` to generate it, then
commits the resulting lockfile. Without it, `npm ci` in CI fails immediately.

### docker-compose.yml

If `docker-compose.yml` at repo root exists and already has an `admin`
service, add (mirroring its structure):

```yaml
  backoffice:
    build:
      context: ./apps/backoffice
    ports:
      - "3001:80"   # choose next free port; verify against existing services
    depends_on:
      - backend
```

If no compose file exists or admin isn't there, skip this and record the
skip in apply progress (matches REQ-BO-005's no-op clause).

## Cross-cutting concerns

### Strict TDD ordering

This project runs Strict TDD Mode. The Tasks phase MUST sequence each
requirement as:

1. Write the failing test (Vitest or `cargo test`).
2. Run the test, confirm RED.
3. Write the minimum code to make it GREEN.
4. Refactor only with tests still green.

For backend export endpoints, the test infrastructure already exists
(`app_with_post_audit` pattern in `audit.rs::tests`). Reuse it.

### Backwards compatibility

- `GET /v1/audit` and `GET /v1/memory` are unchanged.
- The existing UI "Export CSV" button keyboard shortcut/aria-label is replaced,
  but no other admin page imports the old `handleExportCsv` callbacks, so the
  blast radius is contained to these two files.

### Rollback

Each area is independently revertable:

- Area 1: drop the two routes + `csv` dep. No data migration.
- Area 2: revert the two page files + delete `src/lib/download.ts`.
- Area 3: revert `vite.config.ts`, `package.json`, `src/test/`, CI step.
- Area 4: delete `Dockerfile`, delete `lockfile`, revert CI workflow.

No shared schema or migration glues them together.
