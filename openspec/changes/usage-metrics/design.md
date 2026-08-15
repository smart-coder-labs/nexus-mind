# Design — Usage Metrics (tokens + execution time) per task→project→client→org

> **Change**: `usage-metrics`
> **Status**: in-progress
> **Depends on**: `u2s-client-model` (clients, `projects.client_id`, `project_visibility` view)
> **Date**: 2026-08-14

## Decisions (confirmed by product owner)
- **Ingestion = hybrid**: an explicit ingest endpoint (real tokens/time reported by the agent/harness) **plus** a best-effort backfill from `sessions` (time/counts only — sessions hold no token data).
- **Granularity = full hierarchy** task → project → client → org, additive rollups.
- **NexusMind cannot measure tokens itself** (it does not run the LLMs). The integration point is HTTP `POST /v1/usage`; a matching MCP `report_usage` tool belongs in the external `@smart-coder-labs/nexusmind-mcp` package (out of this repo — follow-up).

## Schema — migration `run_v59` (current head is v58)
`usage_events`:
- `id TEXT PRIMARY KEY`
- `org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE`
- `user_id TEXT REFERENCES users(id) ON DELETE SET NULL` — who ran it (from auth)
- `client_id TEXT REFERENCES clients(id) ON DELETE SET NULL` — snapshot of the project's client at ingest time
- `project_id TEXT REFERENCES projects(id) ON DELETE SET NULL`
- `task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL`
- `session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL`
- `model TEXT`
- `tokens_in INTEGER NOT NULL DEFAULT 0`
- `tokens_out INTEGER NOT NULL DEFAULT 0`
- `duration_ms INTEGER NOT NULL DEFAULT 0`
- `source TEXT NOT NULL DEFAULT 'ingest'` — `ingest | backfill`
- `event_ts TEXT NOT NULL` — when the work happened (caller-supplied or now)
- `created_at TEXT NOT NULL DEFAULT (datetime('now'))`

Indexes: `(org_id, event_ts)`, `(project_id)`, `(client_id)`, `(task_id)`, `(session_id)`.
Idempotent backfill: `CREATE UNIQUE INDEX idx_usage_backfill_session ON usage_events(session_id) WHERE source='backfill'` → backfill uses `INSERT OR IGNORE`.

Resolution rule (server-side, at ingest): if `task_id` given and `project` absent, derive project from the task's `project` name. Resolve `project` name → existing `project_id` (never auto-create). `client_id` = that project's current `client_id` (snapshot; documented). Unresolvable ids are stored as NULL rather than rejected — telemetry must not 500.

## API
- `POST /v1/usage` — auth: `require_permission(project, "memory:write")` (same authority an agent already needs to write to that project; no new permission/role seeding). Body: `{ project?, task_id?, session_id?, model?, tokens_in?, tokens_out?, duration_ms?, event_ts? }`. Returns `201 { id }`.
- `GET /v1/usage/summary?level=task|project|client|org&from?&to?&client_id?&project_id?` — auth: privileged (admin/super_user). Scoping: `super_user` → org-wide; others → join `project_visibility` on `user_id = auth.user_id`. Returns `{ rows: [{ key_id, key_name, tokens_in, tokens_out, tokens_total, duration_ms, event_count }], totals: {...} }`.
- `POST /v1/usage/backfill` — auth: `super_user`. Inserts one `source='backfill'` row per org session lacking one: `duration_ms = max(0, ended_at - started_at)` when both present else 0, `project_id` from `session.project`, `client_id` from that project, `event_ts = started_at`. Idempotent. Returns `{ inserted }`.

## Code placement (keep disjoint from the in-flight clients-UI work)
- `apps/backend/src/db/migrations.rs` — `run_v59` + wire into `run_all`.
- `apps/backend/src/models/types.rs` — `UsageEvent`, `UsageIngestRequest`, `UsageSummaryRow`, `UsageSummaryResponse`.
- `apps/backend/src/db/usage_queries.rs` — NEW module (ingest, summary rollup, backfill); registered in `db/mod.rs`.
- `apps/backend/src/api/usage.rs` — NEW handlers; registered in `api/mod.rs`.
- `apps/backend/src/api/router.rs` — 3 routes.
- Tests: ingest resolves project/client, summary rolls up per level with visibility scoping, backfill is idempotent.

## Frontend (sequenced AFTER the clients-UI agent — shares admin files)
- `apps/admin/src/pages/Usage.tsx` — date-range + client + project + level filters, a KPI row (total tokens, total time, event count) reusing `StatTile`/`KpiMarquee` from the dashboard, a rollup table, and a "Run backfill" action (super_user). Route under `<AdminRoute>` + nav entry.
- `apps/admin/src/api/client.ts` — `getUsageSummary(...)`, `runUsageBackfill()`. `types.ts` — `UsageSummaryRow`.

## Explicitly deferred
- MCP `report_usage` tool in the external package.
- Per-model cost ($) — needs a price table; out of MVP.
- Timeseries/charts beyond the KPI row + table.
