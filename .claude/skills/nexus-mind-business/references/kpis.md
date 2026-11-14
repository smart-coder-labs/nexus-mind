# KPIs

All computed in the backend. The UI shows them; it never derives new ones.

| KPI | Where computed | Shown in |
|---|---|---|
| Dashboard: stats, usage, memory trends, activity, agent activity, heatmap, contributors, memory health (duplicates, stale, untagged — overlapping, no "healthy %"), onboarding, conventions; 7/30/90-day periods | `get_dashboard_data` in `db/queries.rs`, type `DashboardData` in `models/types.rs` | `/` |
| Usage: tokens in/out/total, duration, event count per key and time bucket; source `ingest` or `backfill` | `db/usage_queries.rs` (`/v1/usage/summary`, `/timeseries`) | `/usage` |
| Factory human digest: held merges, approved merges, blocked runs, waiting tasks, unlabeled shadow decisions | `HumanDigest` in `db/factory_ops.rs` | `/factory/inbox` |
| Factory economics: runs, cost USD, spend by model, tier choices, frontier avoidance, proposed changes, cost per proposed change | `db/factory_ops.rs` | Factory home / laboratory |
| Evaluations | `api/factory_evals.rs` | `/factory/laboratory` |
| Project stats | `/v1/projects/:id/stats` | `/projects` |
| Admin stats: duplicates, tags, usage | `/v1/admin/stats/*` | dashboard cards |

Not measured yet: accepted (merged) Factory changes, so there is no cost per accepted change (`db/factory_ops.rs` says so). Good discover material.
