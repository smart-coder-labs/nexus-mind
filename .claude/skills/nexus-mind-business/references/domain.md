# Domain

## Hierarchy

organization → client (optional) → project (optional `parent_id`) → memories, tasks, SDD changes, code projects, usage. Usage events resolve task → project → client → org (`apps/backend/src/models/types.rs`, near `UsageEvent`).

Schema: hand-written migrations in `apps/backend/src/db/migrations.rs` (plus module migrations in `db/workflows.rs`, `db/workflow_execution.rs`, `db/workspace_secrets.rs`, `db/qa_coverage.rs`). Rust types: `apps/backend/src/models/types.rs`. Queries: `apps/backend/src/db/queries.rs` (very large; grep, don't read).

| Entity | What it is |
|---|---|
| organization, user, api_key | tenant, person, credential (also used for web sessions) |
| client | customer of the org (active / paused / offboarded); projects and conventions can belong to one |
| project, project_member | unit of work; per-project role overrides the global role |
| memory | team knowledge item (content, type, tags, scope `org|client|project|personal`, optional `topic_key`, `session_id`) |
| session | an AI tool's working session that memories link to |
| convention | org/client rule agents should follow |
| collection, tag | memory organisation |
| sdd_spec, sdd_change, sdd_artifact (+ revisions) | spec-driven development documents, versioned |
| sprint, task | team tasks; tasks link to specs |
| harness (+ versions, config reviews, install approvals) | shareable bundle of skills/agents/hooks/MCP config |
| agent | machine identity with its own keys |
| autonomous agent (definition, run, finding, target, connector, schedule) | scheduled/triggered agents from templates |
| factory runner, workspace, workflow definition/run/step/event/artifact/approval/publication/intake | the Factory |
| code project/file/chunk/symbol/edge | the code index (tree-sitter chunks, embeddings) |
| policy | memory/usage policies (`policy/mod.rs`) |
| factory action policy | per-action Factory autonomy (`factory/policy_engine.rs`) |
| webhook, backup, audit_log, usage_event, migration_run/candidate | operations |

## Surfaces

- **Backend** `apps/backend` — REST under `/v1/*` (router: `src/api/router.rs`), `/internal/*` for the backoffice, Factory runner routes outside the protected router. Embeddings: fastembed (nomic-embed-text). Binaries: `nexusmind`, `nexusmind-seed`, `backfill-embeddings`.
- **Admin** `apps/admin` — org operations + Factory UI. Build profiles `full` and `only-context` (`VITE_ADMIN_PROFILE`).
- **Backoffice** `apps/backoffice` — platform superadmin across orgs.
- **Landing** `apps/landing` — public site; copy in `src/lib/features.ts`.
- **MCP server / plugins** — external repo; read with `git -C ../nexusmind-mcp show origin/main:<path>`. Default profile is `legacy` (full catalog); `essential`, `reduced_readonly`, `only_context`, `factory_operator` are narrower.
- **nexus-cli** — interactive coding-agent harness (`apps/nexus-cli/README.md`). **migrator-tui** — knowledge-migration TUI.

## Deploy topology (from `.github/workflows/deploy.yml`)

- Backend: arm64 image to GHCR → Oracle Cloud k3s behind Traefik (namespace `nexusmind`, `deploy/oracle/k8s/`), `api.nexusmind.smartcoderlabs.com`. Fly (`apps/backend/fly.toml`) is a manual rollback only.
- Admin, landing, backoffice: Cloudflare Pages (`nexusmind-admin`, `nexusmind-landing`, `nexusmind-backoffice`).
- u2s: manual `deploy-u2s.yml` → single-org docker compose box (`deploy/u2s/`), admin built with `only-context`.
- Deploys run on push to `main` (and to `agent/autonomous-agents-mvp`). Merging is deploying.
- `infrastructure/terraform` and `firebase.json` describe GCP/Firebase that no workflow uses (treat as legacy).
