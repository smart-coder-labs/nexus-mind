# Modules

Admin route (`apps/admin/src/App.tsx`) → page (`apps/admin/src/pages/`) → API prefix → backend module (`apps/backend/src/api/`). The client for every call is `apps/admin/src/api/client.ts`.

| Route | Page | API | Module |
|---|---|---|---|
| `/` | `Dashboard.tsx` | `/v1/admin/dashboard` | `admin.rs` (privileged) |
| `/users` | `Users.tsx` | `/v1/users`, `/v1/admin/users` | `users.rs`, `admin.rs` |
| `/roles` | `Roles.tsx` | `/v1/roles` | `admin.rs` |
| `/projects` | `Projects.tsx` | `/v1/projects` | `admin.rs` |
| `/clients` | `Clients.tsx` | `/v1/clients` | `clients.rs` |
| `/usage` | `Usage.tsx` | `/v1/usage` | `usage.rs` |
| `/code` | `Code.tsx` | `/v1/code` | `code.rs` |
| `/api-keys` | `ApiKeys.tsx` | `/v1/admin/keys` | `admin.rs` |
| `/agents` | `Agents.tsx` | `/v1/admin/keys`, `/v1/roles` | `admin.rs` |
| `/memories` | `Memories.tsx` | `/v1/memory`, `/v1/admin/memories` | `memory.rs`, `admin.rs` |
| `/search` | `Search.tsx` | `/v1/search` | `search.rs` |
| `/graph` | `Graph.tsx` | `/v1/memory/graph`, `/v1/code/graph` | `memory.rs`, `code.rs` |
| `/collections`, `/tags` | `Collections.tsx`, `Tags.tsx` | `/v1/admin/collections`, `/v1/admin/tags` | `admin.rs` |
| `/conventions` | `Conventions.tsx` | `/v1/conventions` | `conventions.rs` |
| `/sessions` | `Sessions.tsx` | `/v1/sessions` | `sessions.rs` |
| `/policies` | `Policies.tsx` | `/v1/policies`, `/v1/policy/check` | `policy.rs` |
| `/webhooks` | `Webhooks.tsx` | `/v1/webhooks` | `webhooks.rs` |
| `/audit` | `AuditLog.tsx` | `/v1/audit` | `audit.rs` |
| `/settings` | `Settings.tsx` | `/v1/admin/org`, `/v1/auth/change-password` | `admin.rs`, `auth.rs` |
| `/backups` | `Backups.tsx` | `/v1/backups` | `backup.rs` |
| `/harnesses` | `Harnesses.tsx` | `/v1/harnesses`, `/v1/harness-config-reviews`, `/v1/harness-recommendations` | `harnesses.rs` |
| `/tasks` | `Tasks.tsx` | `/v1/tasks`, `/v1/sprints` | `tasks.rs` |
| `/sdd` | `Sdd.tsx` | `/v1/sdd` | `sdd.rs` |
| `/migrations` | `Migrations.tsx` | `/v1/migrations` | `migrations.rs` |
| `/factory` | `factory/FactoryHome.tsx` | `/v1/factory/workflow-catalog`, `/v1/autonomous-agents` | `workflows.rs`, `autonomous_agents.rs` |
| `/factory/agents`, `/factory/templates` | `AutonomousAgents.tsx` | `/v1/autonomous-agent*` | `autonomous_agents.rs` |
| `/factory/workflows` | `factory/workflows/` | `/v1/factory/{workflows,workflow-runs,workspaces,runners,workflow-intakes}` | `workflows.rs`, `workflow_execution.rs`, `workflow_runtime.rs`, `workspace_secrets.rs` |
| `/factory/runs`, `/factory/findings` | `factory/FactoryRuns.tsx`, `factory/FactoryFindings.tsx` | `/v1/autonomous-agent-runs`, `-findings`, `/v1/factory/*` | `autonomous_agents.rs`, `factory.rs` |
| `/factory/inbox` | `FactoryDigest.tsx` | `/v1/factory/digest`, `/decisions` | `factory.rs` |
| `/factory/intake` | `factory/FactoryInputs.tsx` | `/v1/factory/intake/*` | `factory.rs`, `workflow_execution.rs` |
| `/factory/laboratory` | `factory/FactoryLaboratory.tsx` | `/v1/factory/{evaluations,shadow,lookback}` | `factory_evals.rs`, `factory.rs`, `factory_lookback.rs` |
| `/factory/settings` | `factory/FactoryConfiguration.tsx` | `/v1/factory/{policies,model-config,watchdog,bot}` | `factory.rs`, `factory_model_config.rs` |

Not in the admin (used by MCP/CLI): `/v1/context`, `/v1/docs`, `/v1/automation`. Backoffice: `/internal/*` (`internal.rs`). Non-HTTP backend modules: `src/factory/` (engine), `src/automation/` (agent runtime, merge gate), `src/indexer/` + `src/retrieval/` (code index/search), `src/migration/` (knowledge migration), `src/policy/`, `src/backup/`.
