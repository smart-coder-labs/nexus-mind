# Proposal: Team Tasks

## Intent

Give NexusMind teams a first-class task system so members can create, assign, organize, and track work inside the same product that already holds their projects, memories, and harnesses — and so agents (Claude Code, Codex, Cursor) can answer "what do I still owe on this project?" without leaving their session. Today there is no representation of team work in NexusMind: assignments live in people's heads or in a separate tool, and the SDD/openspec flow that actually resolves the work has no back-reference to the task that requested it. This change introduces a rich, mini-Jira task layer (tasks, multiple assignees, labels, subtasks, comments, sprints, retrospectives), a many-to-many link between tasks and openspec changes that **auto-resolves** a task when its linked change is verified or archived, and an agent-facing surface that both **pulls** task data on demand (MCP tools) and **pushes** a pending-task reminder at session start. Every operation is permission-gated and project-scoped, reusing the existing auth, membership, and role-template machinery so tasks inherit the same access boundary as the rest of the product.

## Scope

### In Scope
- Core task lifecycle: create/read/update/delete tasks with `title`, `description`, `status`, `priority`, `due_date`, `project`, `created_by`, and timestamps; project-membership scoping and soft-delete via `archived_at`.
- Multiple assignees per task via a `task_assignees` join table, with assign-to-another-user gated behind a dedicated permission.
- Organization: labels/tags on tasks, and subtasks (parent/child) within a task tree.
- Collaboration: threaded comments on tasks.
- Task↔openspec-change links: a many-to-many join keyed by the openspec change's kebab-case **folder name** (no DB representation of specs), validated against the active and archived openspec trees.
- Auto-resolve: when a linked openspec change is verified or archived, its linked tasks transition to `done` through an explicit backend endpoint invoked by the openspec flow (not an implicit backend event).
- Sprints and sprint retrospectives, reconciled with the existing `create_sprint_retrospective` MCP tool.
- Agent tools (PULL): MCP tools for listing my tasks, listing/filtering project tasks, creating/updating tasks, assigning, commenting, linking to specs, resolving-by-spec, and sprint operations.
- Agent reminder (PUSH): a SessionStart hook that fetches the current user's pending-task count for the project and injects a text reminder.
- New granular permission strings (`task:read`, `task:write`, `task:assign`, `task:delete`, `task:manage`) granted to existing role templates via a migration.
- Admin frontend: a Tasks page (list/board + detail) wired to the API client, routing, and permission-gated navigation.

### Out of Scope (v1)
- Real-time notifications beyond the SessionStart reminder (no websockets, email, Slack, or in-app push on task change).
- External integrations / two-way sync with Jira, GitHub Issues, Linear, or any third-party tracker.
- Time-tracking, estimates/story points burndown math, and velocity analytics.
- Gantt charts, roadmap/timeline views, and dependency graphs between tasks (subtask parent/child is the only hierarchy in v1).
- Cross-organization tasks or task sharing across org boundaries (tasks stay org- and project-scoped).
- Custom fields, custom statuses/workflows, and per-org workflow configuration (status set is fixed in v1).
- Automatic bidirectional openspec creation from a task (linking is manual/tooling-driven; only the resolve direction is automated).
- Landing/marketing surfaces.

## Capabilities

### New Capabilities
- `team-tasks-core`: Task CRUD with status, priority, due date, project scoping, soft-delete, and the new `task:*` permission gating.
- `team-tasks-assignment`: Multiple assignees per task via a join table, with `task:assign`-gated assignment and org-membership validation.
- `team-tasks-organization`: Labels/tags and parent/child subtasks for structuring work.
- `team-tasks-collaboration`: Threaded comments on tasks.
- `team-tasks-spec-links`: Many-to-many links between tasks and openspec change folder names, validated against the openspec trees, with auto-resolve of linked tasks to `done` on verify/archive.
- `team-tasks-sprints`: Sprints and sprint retrospectives, reconciled with the existing `create_sprint_retrospective` tool.
- `team-tasks-agent-tools`: MCP pull tools over the task API plus the SessionStart push reminder.
- `team-tasks-admin-ui`: Admin frontend Tasks page, API client methods, routing, and permission-gated navigation.

### Modified Capabilities / Touched Surfaces
- Role-template permission seeding (migration v5 templates such as `tmpl_dev_senior`, `tmpl_dev_junior`): a new migration grants the `task:*` permissions to the appropriate templates so existing roles can act on tasks.
- Existing `create_sprint_retrospective` MCP tool: reconciled with `team-tasks-sprints` — investigate whether it currently hits a real backend route or is a client-side stub, and either back it with the new sprint endpoints or converge naming rather than duplicating.
- `generate_daily_standup` MCP tool: noted only — may optionally surface task context later, not changed in this proposal.

## Approach

**Rust backend.** Add a `run_v51+` migration chain creating `tasks`, `task_assignees`, `task_labels`, `task_comments`, `task_spec_links`, `sprints`, and `sprint_retrospectives` (plus a `task_id`/`sprint_id` linkage), following existing table conventions (`id TEXT PRIMARY KEY`, `org_id` FK cascade, ISO-8601 timestamps, nullable `archived_at`, explicit indexes, idempotent `run_all`). Add DTOs and denormalized display structs (e.g. `TaskAssignee` mirroring `HarnessOwner`) in `src/models/types.rs`, free-function queries in `src/db/queries.rs` (reusing `visibility_predicate` for project scoping and `user_belongs_to_org` for assignee validation), and a new `src/api/tasks.rs` handler file registered in `src/api/mod.rs` + `src/api/router.rs`. Every handler is gated via `require_permission(&conn, &auth, Some(project), "task:...")` and follows the existence-leak rule (404 on reads, 403 on writes for non-members). A dedicated migration grants the new permission strings to role templates. Sprints/retrospectives are gated behind `task:manage`.

**Auto-resolve mechanism.** Because openspec verify/archive is a filesystem/skill flow with no native backend event, expose an explicit endpoint `POST /v1/tasks/resolve-by-spec { spec_change_name }` that looks up all tasks linked to that change name and transitions them to `done`. The openspec `sdd-verify`/`sdd-archive` flow (via an MCP tool `resolve_tasks_for_spec` or a hook) calls this endpoint. Link creation validates the change name against `openspec/changes/<name>/` (active) and `openspec/changes/archive/<date>-<name>/` (archived); the link stores only the string.

**MCP server.** Add a `// ── Tasks ──` section in `nexusmind-mcp/src/index.ts` registering pull tools (`list_my_tasks`, `list_tasks`, `create_task`, `update_task`, `assign_task`, `add_task_comment`, `link_task_spec`, `resolve_tasks_for_spec`, sprint tools) via `server.tool()`, each a thin wrapper over one backend route in `src/client.ts` using the existing Bearer-token client. Destructive tools (`delete_task`) require an explicit `confirm` guard. `list_my_tasks` resolves the API key's user server-side and filters by `project` + `status`. Extend/add a SessionStart hook under `src/hooks/` that calls `listMyTasks` and injects a "You have N pending tasks in <project>" line (the hook fetches and injects text itself; it cannot force tool calls).

**Admin frontend.** Add `Task`/`TaskComment`/`Sprint` types to `src/types.ts`, typed CRUD methods to the `NexusMindClient` in `src/api/client.ts`, and a lazy-loaded `src/pages/Tasks.tsx` (list/board + detail, with a `src/pages/tasks/` subfolder if it grows) reusing `ui/*` primitives (`Table`, `Modal`, `Select`, `Badge`, `EmptyState`, `Button`) and Apple design tokens. Register the route in `src/App.tsx` and a `requiredPermission: 'task:read'` nav item in `src/components/Layout.tsx`.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `apps/backend/src/db/migrations.rs` | New | `run_v51+` migrations: tasks, task_assignees, task_labels, task_comments, task_spec_links, sprints, retrospectives, indexes; plus a role-template permission-grant migration. |
| `apps/backend/src/models/types.rs` | Modified | Add `Task`, `CreateTaskRequest`, `TaskAssignee`, `TaskComment`, `Sprint`, `SprintRetrospective`, and related DTOs/display structs. |
| `apps/backend/src/db/queries.rs` | Modified | Add task/assignee/label/comment/link/sprint queries + parallel `count_*` fns, reusing `visibility_predicate` and `user_belongs_to_org`. |
| `apps/backend/src/api/tasks.rs` | New | Axum handlers for task CRUD, assignment, labels, subtasks, comments, spec-links, resolve-by-spec, and sprints, each permission-gated. |
| `apps/backend/src/api/mod.rs`, `router.rs` | Modified | Register `pub mod tasks;` and mount routes on the `protected` router. |
| `apps/backend/src/api/helpers.rs` | Noted | Reuse `require_permission` / `AppJson`; no change expected. |
| `nexusmind-mcp/src/client.ts` | Modified | Add `Task`/input types + thin typed fns (`createTask`, `listTasks`, `listMyTasks`, `updateTask`, `assignTask`, `addTaskComment`, `linkTaskSpec`, `resolveTasksForSpec`, sprint fns). |
| `nexusmind-mcp/src/index.ts` | Modified | Register the `// ── Tasks ──` MCP tool set (pull + `confirm`-guarded `delete_task`); reconcile `create_sprint_retrospective`. |
| `nexusmind-mcp/src/hooks/` | New/Modified | SessionStart hook that fetches pending-task count and injects the reminder line. |
| `nexusmind-mcp/package.json`, `CHANGELOG.md`, `README.md` | Modified | Add new test files to the `test` script, bump version, changelog + tool-list entries. |
| `apps/admin/src/types.ts`, `src/api/client.ts` | Modified | Add task/sprint types and typed CRUD client methods. |
| `apps/admin/src/pages/Tasks.tsx` (+ `pages/tasks/`) | New | Tasks list/board + detail UI using existing primitives and design tokens. |
| `apps/admin/src/App.tsx`, `src/components/Layout.tsx` | Modified | Lazy route + `task:read` permission-gated nav item. |
| `openspec/config.yaml` | Noted | Change already registered with `strict_tdd: true`, `tdd_scope: backend_and_admin`. |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Auto-resolve has no native backend event to hang off | High | Make resolution explicit: a `resolve-by-spec` endpoint + `resolve_tasks_for_spec` MCP tool called by the `sdd-verify`/`sdd-archive` flow; document that resolution is triggered, not automatic. |
| Rich scope (multiple assignees + subtasks + labels + comments + sprints) inflates surface and blows the PR/line budget | High | Slice into ordered, independently-reviewable chained PRs (one capability slice per work unit, ~400 lines each); ship core → assignment → organization → collaboration → spec-links → sprints → agent-tools → admin-ui. |
| SessionStart hook cannot force tool calls | Med | Hook fetches the pending count itself and injects text; pull tools remain the on-demand path — the reminder is advisory, not authoritative. |
| Spec-link name can point to a non-existent or renamed change folder | Med | Validate against active (`openspec/changes/<name>/`) and archived (`openspec/changes/archive/<date>-<name>/`) trees at link time; store only the string; tolerate later renames as dangling links surfaced in UI. |
| Duplicating the existing `create_sprint_retrospective` tool | Med | Investigate the current tool's backend wiring first; converge on the new sprint endpoints instead of adding a parallel path. |
| Permission grants to role templates could over- or under-authorize existing roles | Med | Grant `task:*` conservatively per template in the migration; `task:assign`/`task:delete`/`task:manage` only to senior/admin templates; verify against seeded template JSON. |
| Multiple assignees + project scoping leak existence to non-members | Med | Enforce the existing existence-leak rule (404 read / 403 write) and `visibility_predicate` on every list/read handler; cover with membership-visibility tests. |

## Rollback Plan

The change is additive and layered. MCP tools and the SessionStart hook are removed by unregistering them in `nexusmind-mcp` with no backend impact. The admin Tasks page is removed by dropping the route + nav item. The backend surface is inert until routes are mounted; unmounting the task routes disables the API without touching data. The new tables use guarded `run_vN` migrations that only run forward; a rollback leaves the tables in place (harmless, unreferenced) rather than dropping data. The permission-grant migration is the only broadly-visible change — reverting it removes `task:*` from templates, which safely denies task access. No existing capability is mutated destructively, so partial rollback of any single slice is safe.

## Dependencies

- Existing backend auth/permission stack: `AuthContext`, `require_permission`, role templates seeded in migration v5, `project_members` scoping, `user_belongs_to_org`, `visibility_predicate`.
- Existing migration framework (`run_vN` + `PRAGMA user_version` + `run_all`), next free version v51+.
- `nexusmind-mcp` runtime: `@modelcontextprotocol/sdk`, `server.tool()` registration, Bearer-token client, existing hook framework under `src/hooks/`.
- Existing `create_sprint_retrospective` MCP tool (to reconcile, not duplicate).
- Admin stack: React 19 + Vite + react-router-dom 6 + tanstack-query v5 + Tailwind 4, `NexusMindClient`, `ui/*` primitives, Apple design tokens.
- The openspec `sdd-verify`/`sdd-archive` flow as the caller of the resolve-by-spec path.

## Success Criteria

- [ ] A member with `task:write` can create, read, update, and soft-delete tasks scoped to a project they belong to.
- [ ] Non-members get 404 on task reads and 403 on task writes (existence-leak rule holds).
- [ ] A task can carry multiple assignees; assigning another user requires `task:assign` and validates org membership.
- [ ] Labels, subtasks, and comments attach to a task and round-trip through the API.
- [ ] A task links to one or more openspec changes (many-to-many), validated against the active/archived trees.
- [ ] Verifying or archiving a linked openspec change transitions its linked tasks to `done` via the resolve-by-spec path.
- [ ] Agents can list their pending tasks and create/update/assign/comment/link via MCP tools; `delete_task` refuses without `confirm`.
- [ ] Session start injects a "You have N pending tasks in <project>" reminder for the API key's user.
- [ ] The new `task:*` permissions are granted to role templates and enforced on every handler.
- [ ] The admin Tasks page lists and manages tasks, gated behind `task:read` in navigation.
- [ ] Sprints/retrospectives work end-to-end and do not duplicate `create_sprint_retrospective`.
- [ ] Backend, admin, and MCP test suites pass; implementation followed RED→GREEN per task.

## Assumptions

- The status set is fixed in v1 (e.g. `todo` / `in_progress` / `done`, exact set finalized in spec/design); custom workflows are out of scope.
- openspec changes are identified solely by their kebab-case folder name; there is no DB row for a spec, and dangling links (renamed/deleted folders) are tolerated and surfaced, not blocked.
- Auto-resolve is triggered by the openspec flow calling `resolve-by-spec`; if that call does not run, tasks remain open — this is acceptable for v1.
- Role-template permission grants can introduce new permission strings ad hoc; `task:manage` covers sprint/label administration.
- `list_my_tasks` maps the MCP `NEXUSMIND_API_KEY` to a user server-side; `project` is an explicit tool argument.
- The exact reconciliation of `create_sprint_retrospective` (back it with a real route vs. rename) is resolved during design after inspecting its current wiring.
- Sprint↔task association shape (a sprint groups tasks) is refined in spec/design; v1 keeps it a simple grouping without burndown analytics.
