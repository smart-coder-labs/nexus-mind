# Design: Team Tasks

This document is the architectural HOW for the `team-tasks` change. It is implementation-ready and drives the RED→GREEN TDD task breakdown. It reuses the existing NexusMind machinery verbatim (migrations, `require_permission`, `visibility_predicate`, `user_belongs_to_org`, the `AppJson` extractor, the existence-leak rule, the MCP thin-client + hook pattern, and the Apple-token admin UI) and adds a self-contained task layer on top.

---

## 0. Architecture approach

- **Pattern**: screaming, layered, feature-sliced. One vertical feature (`tasks`) cutting through migration → model → query → handler → route on the backend, mirrored by client → tool → hook on the MCP, and by types → client → page on the admin. No new architectural primitive is introduced — every new file has an existing sibling that is the canonical template.
- **Boundaries**: tasks are **org-scoped and project-scoped**, exactly like sessions/harnesses. Project scoping is enforced by the SAME `visibility_predicate` / project-membership check used by sessions (404 on reads for non-members, 403 on writes). Assignee/comment/link/sprint child rows inherit the task's project boundary transitively — every child handler re-checks the parent task's visibility before acting.
- **Layering rule**: handlers never write raw SQL; all SQL lives in `queries.rs` free functions. Handlers own permission gating + visibility + HTTP shaping only.
- **Spec identity**: openspec changes have NO DB row. The `task_spec_links` join stores the kebab-case change-folder name string only; validity is a filesystem check at link time, tolerated as dangling afterwards.

---

## 1. Data model

### 1.1 Migration versioning decision

Current max is **v50** (`run_v50`, `harness_config_review_comments`). Next free is **v51**.

Decision: **two migrations**, `run_v51` (all task tables + indexes) and `run_v52` (role-template permission grants). They are split because they have different idempotency shapes: v51 is pure `CREATE TABLE IF NOT EXISTS` (guard `user_version >= 51`), v52 is a data UPDATE to the seeded `roles` JSON (guard `user_version >= 52`) that must be independently testable (`run_v52_grants_task_perms`, `run_v52_is_idempotent`) without coupling table creation to permission policy. Both are appended to `run_all()` after `run_v50(conn)?;` in order. Both follow the existing `PRAGMA user_version` guard + `PRAGMA user_version = N;` tail pattern.

All task tables follow house conventions: `id TEXT PRIMARY KEY` (uuid v4 in Rust), `org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE`, ISO-8601 `TEXT` timestamps defaulting to `datetime('now')`, soft-delete via nullable `archived_at TEXT`, booleans as `INTEGER`, explicit `CREATE INDEX IF NOT EXISTS`.

### 1.2 Tables (all created in `run_v51`)

**`tasks`** — core entity.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | uuid v4 |
| `org_id` | TEXT NOT NULL | → `organizations(id)` ON DELETE CASCADE |
| `project` | TEXT NOT NULL | project **name** string (mirrors `sessions.project`; scoping via name like sessions, not a project_id FK — keeps org-shared/unregistered projects working) |
| `title` | TEXT NOT NULL | |
| `description` | TEXT | nullable |
| `status` | TEXT NOT NULL DEFAULT `'backlog'` | validated app-side against `TaskStatus` enum |
| `priority` | TEXT NOT NULL DEFAULT `'medium'` | validated app-side (`low`/`medium`/`high`/`urgent`) |
| `due_date` | TEXT | nullable ISO-8601 date |
| `parent_id` | TEXT | nullable, → `tasks(id)` ON DELETE CASCADE — **subtasks live on this self-FK, no separate table** |
| `sprint_id` | TEXT | nullable, → `sprints(id)` ON DELETE SET NULL |
| `created_by` | TEXT NOT NULL | → `users(id)` ON DELETE RESTRICT |
| `created_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |
| `updated_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |
| `archived_at` | TEXT | nullable soft-delete |

Indexes: `idx_tasks_org_project_status ON tasks(org_id, project, status)`, `idx_tasks_org_parent ON tasks(org_id, parent_id)`, `idx_tasks_sprint ON tasks(sprint_id)`.

> Note: `sprint_id` references `sprints`, which is created in the same `run_v51` batch. SQLite does not enforce FK ordering at table-creation time (it resolves references lazily), so a single `execute_batch` with `sprints` defined before or after `tasks` is fine; we define `sprints` **before** `tasks` in the batch for readability.

**`task_assignees`** — many assignees per task (join).

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | uuid v4 |
| `task_id` | TEXT NOT NULL | → `tasks(id)` ON DELETE CASCADE |
| `user_id` | TEXT NOT NULL | → `users(id)` ON DELETE CASCADE |
| `assigned_by` | TEXT NOT NULL | → `users(id)` ON DELETE RESTRICT |
| `assigned_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |
| | | UNIQUE(`task_id`, `user_id`) |

Index: `idx_task_assignees_user ON task_assignees(user_id)` (drives `list_my_tasks`).

**`task_labels`** — labels/tags on a task. Decision: **no separate label-definitions table in v1**. Labels are free-text strings scoped to a task row (like memory tags), which matches how tags already work in this codebase and avoids a label-admin CRUD surface. `task:manage` is still reserved for future label governance.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | uuid v4 |
| `task_id` | TEXT NOT NULL | → `tasks(id)` ON DELETE CASCADE |
| `label` | TEXT NOT NULL | free-text tag |
| `created_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |
| | | UNIQUE(`task_id`, `label`) |

Index: `idx_task_labels_label ON task_labels(label)` (drives label filter).

**`task_comments`** — threaded comments.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | uuid v4 |
| `task_id` | TEXT NOT NULL | → `tasks(id)` ON DELETE CASCADE |
| `user_id` | TEXT NOT NULL | → `users(id)` ON DELETE CASCADE (author) |
| `body` | TEXT NOT NULL | |
| `created_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |

Index: `idx_task_comments_task ON task_comments(task_id, created_at)`.

> "Threaded" in v1 means a flat, chronologically-ordered list per task (mirrors `harness_config_review_comments`). No nested reply tree — out of scope, documented in the spec.

**`task_spec_links`** — many-to-many task ↔ openspec change name.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | uuid v4 |
| `task_id` | TEXT NOT NULL | → `tasks(id)` ON DELETE CASCADE |
| `spec_change_name` | TEXT NOT NULL | kebab-case folder name string; NO FK (specs have no DB row) |
| `linked_by` | TEXT NOT NULL | → `users(id)` ON DELETE RESTRICT |
| `created_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |
| | | UNIQUE(`task_id`, `spec_change_name`) |

Index: `idx_task_spec_links_change ON task_spec_links(spec_change_name)` — this is the lookup key for resolve-by-spec.

**`sprints`** — a sprint groups tasks (grouping only, no burndown).

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | uuid v4 |
| `org_id` | TEXT NOT NULL | → `organizations(id)` ON DELETE CASCADE |
| `project` | TEXT NOT NULL | project name string (same scoping as tasks) |
| `name` | TEXT NOT NULL | e.g. "Sprint 42" |
| `goal` | TEXT | nullable |
| `starts_at` | TEXT | nullable ISO date |
| `ends_at` | TEXT | nullable ISO date |
| `status` | TEXT NOT NULL DEFAULT `'planned'` | `planned`/`active`/`completed` (app-validated) |
| `created_by` | TEXT NOT NULL | → `users(id)` ON DELETE RESTRICT |
| `created_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |
| `archived_at` | TEXT | nullable |
| | | UNIQUE(`org_id`, `project`, `name`) |

Index: `idx_sprints_org_project_status ON sprints(org_id, project, status)`.

Task↔sprint association is the nullable `tasks.sprint_id` FK (a task belongs to at most one sprint) — no join table needed for v1.

**`sprint_retrospectives`** — one or more retro notes per sprint.

| Column | Type | Notes |
|--------|------|-------|
| `id` | TEXT PK | uuid v4 |
| `sprint_id` | TEXT NOT NULL | → `sprints(id)` ON DELETE CASCADE |
| `org_id` | TEXT NOT NULL | → `organizations(id)` ON DELETE CASCADE (denormalized for scoping) |
| `went_well` | TEXT | nullable |
| `went_wrong` | TEXT | nullable |
| `action_items` | TEXT | nullable |
| `created_by` | TEXT NOT NULL | → `users(id)` ON DELETE RESTRICT |
| `created_at` | TEXT NOT NULL DEFAULT `(datetime('now'))` | |

Index: `idx_sprint_retros_sprint ON sprint_retrospectives(sprint_id, created_at)`.

### 1.3 Table count

**7 new tables**: `tasks`, `task_assignees`, `task_labels`, `task_comments`, `task_spec_links`, `sprints`, `sprint_retrospectives` (+ the `roles` table is modified by v52, not created).

### 1.4 `run_v52` — permission grants

Grants the new `task:*` strings into the seeded template `roles.permissions` JSON arrays. Because permissions are a JSON string column, the migration uses `json_insert`/`json_set` on each template row (guarded so re-running is a no-op via the `user_version` gate; the JSON mutation is written to be idempotent by checking membership first, matching the "idempotency is tested" convention).

Grant matrix (locked):

| Template | task:read | task:write | task:assign | task:delete | task:manage |
|----------|:---:|:---:|:---:|:---:|:---:|
| `tmpl_dev_junior` | ✅ | ✅ | — | — | — |
| `tmpl_dev_senior` | ✅ | ✅ | ✅ | ✅ | — |
| `tmpl_security_officer` | ✅ | — | — | — | — |
| `tmpl_auditor` | ✅ | — | — | — | — |

Reconciled with the brief's "read+write→all dev templates, assign→senior/lead, delete+manage→senior/admin". There is no `lead` template in migration v5 (`tmpl_dev_senior`, `tmpl_dev_junior`, `tmpl_security_officer`, `tmpl_auditor`), so **senior gets assign+delete**; **`task:manage` is granted to no template** and is therefore admin-only (admins bypass `require_permission` entirely via `is_privileged()`). **R1 RESOLVED**: junior gets `task:read`+`task:write` (juniors create/edit tasks; assigning to others stays senior-gated via `task:assign`), per the brief's "read+write→all dev templates". See §11 R1.

---

## 2. Rust models (`src/models/types.rs`)

### 2.1 `TaskStatus` enum (hand-rolled `FromStr`/`Display`, mirrors `Role`)

```rust
pub enum TaskStatus { Backlog, Todo, InProgress, InReview, Done, Cancelled }
```

- `FromStr`: `"backlog"|"todo"|"in_progress"|"in_review"|"done"|"cancelled"` → variant; anything else → `Err`.
- `Display`: reverse mapping to the same snake_case strings.
- Stored as TEXT; parsed on write (invalid → 422), serialized as the snake_case string.

### 2.2 Status transition matrix (app-side, validated in the handler/query layer)

`fn can_transition(from: TaskStatus, to: TaskStatus) -> bool`. Allowed edges:

| From \ To | backlog | todo | in_progress | in_review | done | cancelled |
|-----------|:---:|:---:|:---:|:---:|:---:|:---:|
| backlog | — | ✅ | ✅ | — | — | ✅ |
| todo | ✅ | — | ✅ | — | — | ✅ |
| in_progress | ✅ | ✅ | — | ✅ | ✅ | ✅ |
| in_review | — | — | ✅ | — | ✅ | ✅ |
| done | — | — | ✅ | — | — | — |
| cancelled | ✅ | — | — | — | — | — |

Rules encoded: any active state can be `cancelled`; `done` is reached only from `in_progress` or `in_review`; `done`/`cancelled` can be reopened (to `in_progress` / `backlog` respectively); same→same is a no-op allowed (idempotent PATCH). **Auto-resolve bypasses the matrix** — resolve-by-spec forces `done` from any non-terminal state (documented exception; it is a system transition, not a user edit). An illegal transition returns `422 { code: "invalid_transition" }`.

### 2.3 Response structs (Serialize) and Request DTOs (Deserialize)

- `Task { id, org_id, project, title, description, status, priority, due_date, parent_id, sprint_id, created_by, created_at, updated_at, archived_at, assignees: Vec<TaskAssignee>, labels: Vec<String>, comment_count: i64, spec_links: Vec<String>, subtask_count: i64 }` — the list endpoint returns a lean variant; the detail endpoint hydrates `assignees`/`labels`/`spec_links`.
- `TaskAssignee { id, name, email }` — denormalized display, **mirrors `HarnessOwner`** exactly (joined from `users`).
- `TaskComment { id, task_id, user_id, author_name, body, created_at }`.
- `Sprint { id, org_id, project, name, goal, starts_at, ends_at, status, created_by, created_at, archived_at, task_count: i64 }`.
- `SprintRetrospective { id, sprint_id, went_well, went_wrong, action_items, created_by, author_name, created_at }`.
- Request DTOs (deserialize-only, `Request` suffix): `CreateTaskRequest { project, title, description?, status?, priority?, due_date?, parent_id?, sprint_id? }`, `PatchTaskRequest { title?, description?, status?, priority?, due_date?, sprint_id? }` (all `Option`, `Default`), `AssignTaskRequest { user_ids: Vec<String> }`, `AddLabelRequest { label }`, `AddCommentRequest { body }`, `LinkSpecRequest { spec_change_name }`, `ResolveBySpecRequest { spec_change_name }`, `CreateSprintRequest { project, name, goal?, starts_at?, ends_at? }`, `PatchSprintRequest { name?, goal?, starts_at?, ends_at?, status? }`, `CreateRetrospectiveRequest { went_well?, went_wrong?, action_items? }`.

---

## 3. Backend API (`src/api/tasks.rs`)

All handlers: `State(store)`, `Extension(auth)`, `AppJson<...>` for bodies, return `Result<(StatusCode, Json<T>), (StatusCode, Json<ApiError>)>`, local `db_err/lock_err/not_found/forbidden`. Every read gates on project visibility (404 leak rule); every write calls `require_permission(&conn, &auth, Some(&project), "task:...")` (403) AND visibility (404). Child-resource handlers first load the parent task, apply the parent's visibility, then act.

### 3.1 Endpoint table

| # | Method | Path | Permission | Request | Response |
|---|--------|------|-----------|---------|----------|
| 1 | GET | `/v1/tasks` | `task:read` | query: `project?`, `assignee=me`, `status?`, `sprint?`, `label?`, `parent_id?`, `include_archived?`, `limit?`, `offset?` | `Vec<Task>` (lean) |
| 2 | POST | `/v1/tasks` | `task:write` | `CreateTaskRequest` | `201 Task` |
| 3 | GET | `/v1/tasks/:id` | `task:read` | — | `Task` (hydrated) |
| 4 | PATCH | `/v1/tasks/:id` | `task:write` | `PatchTaskRequest` | `Task` (status change validated by transition matrix) |
| 5 | DELETE | `/v1/tasks/:id` | `task:delete` | — | `204` (soft-delete: sets `archived_at`) |
| 6 | GET | `/v1/tasks/:id/subtasks` | `task:read` | — | `Vec<Task>` (children by `parent_id`) |
| 7 | POST | `/v1/tasks/:id/assignees` | `task:assign` | `AssignTaskRequest` | `200 Vec<TaskAssignee>` (validates each via `user_belongs_to_org`) |
| 8 | DELETE | `/v1/tasks/:id/assignees/:user_id` | `task:assign` | — | `204` |
| 9 | POST | `/v1/tasks/:id/labels` | `task:write` | `AddLabelRequest` | `200 Vec<String>` |
| 10 | DELETE | `/v1/tasks/:id/labels/:label` | `task:write` | — | `204` |
| 11 | GET | `/v1/tasks/:id/comments` | `task:read` | — | `Vec<TaskComment>` |
| 12 | POST | `/v1/tasks/:id/comments` | `task:write` | `AddCommentRequest` | `201 TaskComment` |
| 13 | GET | `/v1/tasks/:id/spec-links` | `task:read` | — | `Vec<String>` |
| 14 | POST | `/v1/tasks/:id/spec-links` | `task:write` | `LinkSpecRequest` | `201` (validates folder exists; `422 { code: "unknown_spec" }` if neither active nor archived tree has it) |
| 15 | DELETE | `/v1/tasks/:id/spec-links/:name` | `task:write` | — | `204` |
| 16 | POST | `/v1/tasks/resolve-by-spec` | `task:write` | `ResolveBySpecRequest` | `200 { resolved: Vec<String> }` (ids transitioned to `done`) |
| 17 | GET | `/v1/sprints` | `task:read` | query: `project?`, `status?`, `include_archived?`, `limit?`, `offset?` | `Vec<Sprint>` |
| 18 | POST | `/v1/sprints` | `task:manage` | `CreateSprintRequest` | `201 Sprint` |
| 19 | GET | `/v1/sprints/:id` | `task:read` | — | `Sprint` |
| 20 | PATCH | `/v1/sprints/:id` | `task:manage` | `PatchSprintRequest` | `Sprint` |
| 21 | DELETE | `/v1/sprints/:id` | `task:manage` | — | `204` (soft-delete) |
| 22 | GET | `/v1/sprints/:id/retrospectives` | `task:read` | — | `Vec<SprintRetrospective>` |
| 23 | POST | `/v1/sprints/:id/retrospectives` | `task:manage` | `CreateRetrospectiveRequest` | `201 SprintRetrospective` |

**Endpoint count: 23.**

### 3.2 List filters, pagination, resolve-by-spec

- **List (`GET /v1/tasks`)**: dynamic `WHERE` built with `String + push_str` + `Vec<&dyn ToSql>`, always ANDed with `visibility_predicate("project", "?N")` (via the name-based project check used by sessions). `assignee=me` resolves `auth.user_id` and joins `task_assignees`. `sprint`/`label`/`status`/`parent_id` are optional equality filters. Pagination via `resolve_list_pagination(limit, offset)` from `helpers.rs` (opt-in; no params → full set), with a parallel `count_tasks` kept in lockstep. By default `archived_at IS NULL` unless `include_archived=true`.
- **resolve-by-spec (`POST /v1/tasks/resolve-by-spec`)**: looks up `task_spec_links` by `spec_change_name`, loads each linked task in the caller's org, and for every task not already `done`/`cancelled` forces `status='done'` + bumps `updated_at`. Returns the list of transitioned ids. It is org-scoped (only tasks in `auth.org_id`) and permission-gated by `task:write`. It is idempotent (already-`done` tasks are skipped).

### 3.3 Route registration

`src/api/mod.rs`: add `pub mod tasks;`. `src/api/router.rs`: import `tasks`, add the 23 routes to the `protected` router (auth + blanket audit middleware apply automatically). `resolve-by-spec` is registered **before** `/v1/tasks/:id` is not a concern (distinct path segment `resolve-by-spec` vs `:id` — Axum matches the literal segment; but to be safe, register the literal `/v1/tasks/resolve-by-spec` route in the builder — Axum 0.7 prioritizes it correctly as it is a separate route entry).

---

## 4. `src/db/queries.rs` function signatures

All are `fn(&Connection, ...) -> Result<T>`, raw rusqlite, `?N` params, reusing `visibility_predicate` and `user_belongs_to_org`.

```rust
// tasks
pub fn create_task(conn, org_id, created_by, &CreateTaskRequest) -> Result<Task>
pub fn get_task(conn, org_id, task_id) -> Result<Option<Task>>              // hydrated
pub fn patch_task(conn, org_id, task_id, &PatchTaskRequest) -> Result<Option<Task>>
pub fn soft_delete_task(conn, org_id, task_id) -> Result<bool>
pub fn list_tasks(conn, org_id, viewer: Option<&str>, filters: &TaskListFilters, limit, offset) -> Result<Vec<Task>>
pub fn count_tasks(conn, org_id, viewer: Option<&str>, filters: &TaskListFilters) -> Result<i64>
pub fn list_subtasks(conn, org_id, parent_id) -> Result<Vec<Task>>
// assignees
pub fn set_task_assignees(conn, org_id, task_id, assigned_by, user_ids: &[String]) -> Result<Vec<TaskAssignee>>  // validates org membership per user
pub fn remove_task_assignee(conn, task_id, user_id) -> Result<bool>
pub fn list_task_assignees(conn, task_id) -> Result<Vec<TaskAssignee>>
// labels
pub fn add_task_label(conn, task_id, label) -> Result<Vec<String>>
pub fn remove_task_label(conn, task_id, label) -> Result<bool>
// comments
pub fn add_task_comment(conn, task_id, user_id, body) -> Result<TaskComment>
pub fn list_task_comments(conn, task_id) -> Result<Vec<TaskComment>>
// spec links
pub fn link_task_spec(conn, task_id, linked_by, spec_change_name) -> Result<()>
pub fn unlink_task_spec(conn, task_id, spec_change_name) -> Result<bool>
pub fn list_task_spec_links(conn, task_id) -> Result<Vec<String>>
pub fn resolve_tasks_by_spec(conn, org_id, spec_change_name) -> Result<Vec<String>>  // returns transitioned ids
// sprints
pub fn create_sprint(conn, org_id, created_by, &CreateSprintRequest) -> Result<Sprint>
pub fn get_sprint(conn, org_id, sprint_id) -> Result<Option<Sprint>>
pub fn patch_sprint(conn, org_id, sprint_id, &PatchSprintRequest) -> Result<Option<Sprint>>
pub fn soft_delete_sprint(conn, org_id, sprint_id) -> Result<bool>
pub fn list_sprints(conn, org_id, viewer: Option<&str>, project: Option<&str>, status: Option<&str>, include_archived, limit, offset) -> Result<Vec<Sprint>>
// retrospectives
pub fn create_retrospective(conn, sprint_id, org_id, created_by, &CreateRetrospectiveRequest) -> Result<SprintRetrospective>
pub fn list_retrospectives(conn, sprint_id) -> Result<Vec<SprintRetrospective>>
```

`spec_change_name` filesystem validity is checked in the **handler** (it needs the openspec root path, not a DB concern): a helper `fn spec_change_exists(name: &str) -> bool` that checks `openspec/changes/<name>/` and glob `openspec/changes/archive/*-<name>/`.

---

## 5. MCP tools (`nexusmind-mcp/src/index.ts` + `src/client.ts`)

New `// ── Tasks ──` section. Each tool is a thin wrapper over one backend route via a typed `client.ts` fn. `project` is always an explicit arg; identity (`me`) is resolved server-side from `NEXUSMIND_API_KEY`.

### 5.1 Tool list

| # | Tool | Args (zod) | Hits | Notes |
|---|------|-----------|------|-------|
| 1 | `list_my_tasks` | `project?`, `status?` | `GET /v1/tasks?assignee=me` | "me" derives from the API key server-side; no user arg |
| 2 | `list_tasks` | `project?`, `status?`, `sprint?`, `label?`, `assignee?`, `limit?` | `GET /v1/tasks` | general filtered read |
| 3 | `get_task` | `task_id` | `GET /v1/tasks/:id` | hydrated detail |
| 4 | `create_task` | `project`, `title`, `description?`, `priority?`, `due_date?`, `parent_id?`, `sprint_id?` | `POST /v1/tasks` | |
| 5 | `update_task` | `task_id`, `title?`, `description?`, `status?`, `priority?`, `due_date?`, `sprint_id?` | `PATCH /v1/tasks/:id` | status validated by matrix server-side |
| 6 | `delete_task` | `task_id`, `confirm: z.boolean()` | `DELETE /v1/tasks/:id` | **confirm guard** — refuses with no HTTP call if `confirm !== true` (copies `delete_memory`) |
| 7 | `assign_task` | `task_id`, `user_ids: z.array(z.string())` | `POST /v1/tasks/:id/assignees` | |
| 8 | `add_task_comment` | `task_id`, `body` | `POST /v1/tasks/:id/comments` | |
| 9 | `add_task_label` | `task_id`, `label` | `POST /v1/tasks/:id/labels` | |
| 10 | `link_task_spec` | `task_id`, `spec_change_name` | `POST /v1/tasks/:id/spec-links` | |
| 11 | `resolve_tasks_for_spec` | `spec_change_name` | `POST /v1/tasks/resolve-by-spec` | called by sdd-verify/sdd-archive flow |
| 12 | `list_sprints` | `project?`, `status?` | `GET /v1/sprints` | |
| 13 | `create_sprint` | `project`, `name`, `goal?`, `starts_at?`, `ends_at?` | `POST /v1/sprints` | |
| 14 | `create_sprint_retrospective` | `sprint_id`, `went_well?`, `went_wrong?`, `action_items?` | `POST /v1/sprints/:id/retrospectives` | **RECONCILED — see §5.3** |

**MCP tool count: 14** (13 new + 1 repurposed).

Each handler: call client fn → `format*` helper → `{ content: [{type:'text', text}], isError? }` in try/catch. Add `formatTask`, `formatTaskList`, `formatSprint` helpers.

### 5.2 `client.ts` additions

Types: `Task`, `TaskAssignee`, `TaskComment`, `Sprint`, `SprintRetrospective`, `CreateTaskInput`, `UpdateTaskInput`, `AssignTaskInput`, `CreateSprintInput`, `CreateRetrospectiveInput`. Fns (one route each): `listTasks(params)`, `listMyTasks(params)`, `getTask(id)`, `createTask(input)`, `updateTask(id, input)`, `deleteTask(id)`, `assignTask(id, userIds)`, `addTaskComment(id, body)`, `addTaskLabel(id, label)`, `linkTaskSpec(id, name)`, `resolveTasksForSpec(name)`, `listSprints(params)`, `createSprint(input)`, `createSprintRetrospective(sprintId, input)`. `listMyTasks` just sets `assignee=me`.

### 5.3 `create_sprint_retrospective` reconciliation (INVESTIGATED — decision)

**Finding**: the current `create_sprint_retrospective` at `src/index.ts:2955` is a **pure client-side stub** — it calls `listMemories(...)`, filters by tag client-side, and formats markdown. It hits **no backend route** and persists nothing.

**Decision: repurpose it to back the real endpoint (breaking its old semantics, keeping the name).** The tool is renamed in behavior only: it now takes `sprint_id` + retro fields and `POST`s to `/v1/sprints/:id/retrospectives`, persisting a real `SprintRetrospective`. Rationale: keeping the same tool name avoids breaking agent muscle memory / prompts that already reference it; the old memory-aggregation behavior was cosmetic and duplicated `generate_daily_standup`'s pattern, so no real capability is lost. The old aggregation is NOT kept as a second tool (avoids the "parallel path" risk in the proposal). `generate_daily_standup` stays untouched. **HUMAN SANITY-CHECK**: this changes an existing tool's signature/behavior — confirm no external automation depends on the old memory-aggregation output. See §11 R2.

### 5.4 Tests + release hygiene

New test files, added to `package.json` `test` script's `tsx --test` list: `src/tasks-client.test.ts` (mock `globalThis.fetch`, set env before `await import('./client.js')`) and `src/tasks-tools.test.ts` (spawn `dist/index.js` over stdio against a fake in-process HTTP backend). Bump `package.json` version 0.7.1 → 0.8.0, add `CHANGELOG.md` entry, update `README.md` tool list.

---

## 6. SessionStart push hook

**Location**: extend the existing `src/hooks/session-start.ts` (do NOT add a new hook file — SessionStart already exists and is wired). Add one more best-effort block after the memory blocks.

**What it fetches**: `listMyTasks({ project, status: 'in_progress' })` OR a pending count — but the client fn returns full task rows, so the hook counts locally: pending = tasks whose status is not `done`/`cancelled`. Wrapped in `withTimeout(..., FETCH_TIMEOUT_MS)` and try/catch (omit block on failure, exactly like the memory blocks).

**Injected text** (appended to `lines` before `emitAdditionalContext`):

```
### Pending Tasks — <project>
You have N pending task(s) in <project>. Call list_my_tasks to see them.
- <title> [<status>] (due <due_date|—>)
- ... (up to 5, then "…and M more")
```

If N is 0, inject a single line: `No pending tasks in <project>.` (or omit entirely — decision: **omit** to keep the injection budget lean, matching how empty memory blocks are omitted).

**Known limitation** (documented in spec + code comment): hooks cannot force tool calls. The hook fetches and injects text itself; the reminder is advisory. If the backend is unreachable or the key is unset, the block is silently omitted (`exitClean(0)`), never blocking session start.

**Test**: extend `src/hooks/session-start` coverage (new `src/hooks/session-start.test.ts` following `pre-compact.test.ts`'s spawn-against-fake-backend pattern) — assert the pending-tasks line appears when the fake backend returns tasks, and is absent when it returns `[]` or errors. Add to `package.json` test list.

---

## 7. Frontend (`apps/admin`)

### 7.1 Breakdown

- **Types** (`src/types.ts`): `Task`, `TaskAssignee`, `TaskComment`, `Sprint`, `TaskStatus` union, `TaskPriority` union.
- **Client** (`src/api/client.ts`): add to `NexusMindClient`: `listTasks(params)`, `getTask(id)`, `createTask(input)`, `updateTask(id, input)`, `deleteTask(id)`, `assignTask(id, userIds)`, `addTaskComment(id, body)`, `linkTaskSpec(id, name)`, `listSprints(params)`, `createSprint(input)`. Each `request<T>()` with `credentials:'include'`, 403→`/401` (existing behavior).
- **Pages**:
  - `src/pages/Tasks.tsx` — top-level page. **Decision: ship LIST view in v1, board view deferred** to a follow-up slice (keeps the first UI PR under budget; the list view covers all CRUD + filters). A view toggle placeholder is added but board is a later work unit. List uses `ui/Table`, status/priority `ui/Badge` pills, `ui/Select` for status/assignee filters, `ui/Modal` for create/edit, `ui/EmptyState`, `ui/Button` (pill CTAs).
  - `src/pages/tasks/TaskDetail.tsx` (or a `ui/Modal`-based detail drawer) — assignees, labels, comments, spec-links. Introduced in a later slice if the list PR grows too large; v1 can inline detail in a modal.
- **Routing/nav**: `src/App.tsx` lazy route `const Tasks = lazy(() => import('./pages/Tasks'))` + `<Route path="/tasks" .../>`. `src/components/Layout.tsx` nav item with `requiredPermission: 'task:read'`.

### 7.2 tanstack-query keys

- `['tasks', filters]` — list (filters object in the key so filter changes refetch).
- `['task', id]` — detail (hydrated).
- `['task-comments', id]`, `['task-subtasks', id]`.
- `['sprints', filters]`, `['sprint', id]`.
- Mutations (`createTask`/`updateTask`/`deleteTask`/`assignTask`/`addComment`) call `qc.invalidateQueries({ queryKey: ['tasks'] })` and the relevant detail key (mirrors `Sessions.tsx`).

### 7.3 Design tokens

Reuse existing Tailwind classes only: `bg-accent-blue`, `text-text-primary`, `text-text-quaternary`, `border-border-primary`, `rounded-[18px]` (cards), `rounded-full` (pill CTAs). Status/priority pills via `ui/Badge` with existing color variants. NO new hex values. Board columns (later slice) reuse card radius + surface-color elevation (no new shadow token).

### 7.4 Tests

`src/pages/Tasks.test.tsx` FIRST (RED), following `src/pages/Projects.test.tsx`: `renderWithProviders`, `vi.mock('../api/client', ...)` returning fixture task arrays typed against the real interface, `getByRole`/`getByText`/`waitFor`, stub `window.confirm` for delete. Run `npm run test` (vitest run).

---

## 8. Auto-resolve flow (end-to-end)

```
sdd-verify OR sdd-archive skill completes for change <name>
        │
        ▼
Skill invokes MCP tool  resolve_tasks_for_spec({ spec_change_name: <name> })
        │  (thin client)
        ▼
client.resolveTasksForSpec(name)  →  POST /v1/tasks/resolve-by-spec { spec_change_name }
        │  (Bearer key → org + user, require_permission task:write)
        ▼
handler → queries::resolve_tasks_by_spec(conn, org_id, name)
        │  SELECT task_id FROM task_spec_links WHERE spec_change_name = ?1
        │  for each linked task in org not already done/cancelled:
        │      UPDATE tasks SET status='done', updated_at=now WHERE id=?  (matrix bypassed — system transition)
        ▼
returns { resolved: [ids...] }  →  tool prints "Resolved N task(s) linked to <name>"
```

**Where the skill call is wired**: in the SDD skill definitions for `sdd-verify` and `sdd-archive` (the openspec flow), a post-success step calls `resolve_tasks_for_spec` with the change folder name. This is documented in the spec as a required integration point; the backend endpoint + MCP tool are the contract, the skill wiring is the trigger. If the call does not run, tasks stay open (acceptable per proposal assumption). The design does NOT add a backend filesystem watcher — resolution is explicitly triggered, never an implicit backend event.

---

## 9. Migration / versioning / backward-compat

- **Versions**: `run_v51` (7 tables + indexes, `PRAGMA user_version = 51`), `run_v52` (grant `task:*` to templates, `PRAGMA user_version = 52`). Appended to `run_all()` in order after v50. Each gets `run_vNN_creates_*`, `run_vNN_is_idempotent`, and (v51) FK/cascade + UNIQUE tests; (v52) `grants_task_perms` + `is_idempotent` + `preserves_existing_permissions`.
- **Backward-compat**: purely additive. No existing table altered except `roles.permissions` JSON (append-only per template; existing perms preserved and asserted). Existing rows unaffected. Rollback leaves the tables inert (guarded forward-only migrations); reverting v52 removes `task:*` from templates and safely denies task access (admins still bypass). MCP/hook/UI are removable without backend impact (unregister tools / drop route / drop nav item).
- **Idempotency**: v52 JSON mutation must be self-idempotent (check membership before `json_insert`) so a re-run after a `user_version` reset does not duplicate permission strings — tested.

---

## 10. PR slicing plan (feeds sdd-tasks)

Ordered, chained, each targeting < ~400 lines where feasible; every impl task is RED→GREEN. Dependencies are strictly linear except where noted.

| PR | Slice | Capability | Contents | Depends on | Est. lines |
|----|-------|-----------|----------|-----------|-----------|
| 1 | **core-migration+model** | team-tasks-core | `run_v51` (all 7 tables + indexes) + `run_v52` grants + migration tests + `TaskStatus` enum + transition matrix + `Task`/DTO structs | — | ~350 |
| 2 | **core-api** | team-tasks-core | `queries` create/get/patch/soft-delete/list/count_tasks + `tasks.rs` handlers 1–5 + routes + sessions-style test suite (CRUD + 401/403/404 + visibility) | PR1 | ~400 |
| 3 | **assignment** | team-tasks-assignment | assignee queries + handlers 7–8 + `assignee=me` list filter + org-membership validation + tests | PR2 | ~300 |
| 4 | **organization** | team-tasks-organization | labels (9–10) + subtasks (6, `parent_id` create + list) + label/parent list filters + tests | PR2 | ~300 |
| 5 | **collaboration** | team-tasks-collaboration | comments queries + handlers 11–12 + tests | PR2 | ~220 |
| 6 | **spec-links + auto-resolve** | team-tasks-spec-links | spec-link queries + handlers 13–16 + `spec_change_exists` fs validation + resolve-by-spec + tests | PR2 | ~350 |
| 7 | **sprints** | team-tasks-sprints | sprints + retrospectives migrations already in PR1; queries + handlers 17–23 + tests | PR2 | ~380 |
| 8 | **mcp-client+tools** | team-tasks-agent-tools | `client.ts` types+fns + `index.ts` 14 tools + `delete_task` confirm + reconcile `create_sprint_retrospective` + client/tool tests + package.json/version/changelog | PR2–7 (routes exist) | ~400 |
| 9 | **mcp-session-start-push** | team-tasks-agent-tools | extend `session-start.ts` + `session-start.test.ts` | PR8 | ~150 |
| 10 | **admin-ui** | team-tasks-admin-ui | types + client methods + `Tasks.tsx` list + create/edit modal + route + nav + `Tasks.test.tsx` | PR2 (+3–6 for full detail) | ~400 |

**PR slice count: 10.** PRs 3–7 are parallelizable after PR2 (all depend only on core-api); PR8 needs the routes it wraps to exist. If board view is desired, it becomes PR11 (admin-ui-board), keeping PR10 lean.

---

## 11. Trade-offs, alternatives, open risks

**Decisions & alternatives considered:**
- **Subtasks = `parent_id` self-FK, not a separate table.** Alt: `task_subtasks(parent_id, child_id)` join. Rejected — a task has exactly one parent (a tree, not a DAG); a self-FK is simpler, cascades naturally, and matches `projects.parent_id` precedent already in the codebase.
- **Labels = free-text per-task rows, no label-definitions table.** Alt: `label_defs` + `task_label_assignments`. Rejected for v1 — matches existing memory-tag ergonomics, avoids a label-admin CRUD surface; `task:manage` reserved for future governance.
- **`tasks.project` = name string (like sessions), not a `project_id` FK.** Chosen to preserve org-shared/unregistered-project behavior and reuse the session visibility path verbatim. Alt: FK to `projects.id` (like harnesses) — rejected because it would force every task into a registered project and break the org-shared case the session test suite exercises.
- **`TaskStatus` as a real enum (not a bare String).** Per locked decision — exhaustiveness for the transition matrix. Priority stays a validated String (no transition rules).
- **Sprint↔task = nullable `sprint_id` (one sprint per task), not a join.** v1 grouping only; a join would imply multi-sprint membership we explicitly don't want.
- **`create_sprint_retrospective` repurposed to a real route** (see §5.3) rather than adding a parallel `record_sprint_retrospective` tool — avoids the duplication risk called out in the proposal.

**Open risks needing human sanity-check:**
- **R1 — junior permission scope. RESOLVED (PR0):** `tmpl_dev_junior` = `task:read`+`task:write` — juniors create/edit tasks; assigning to others stays senior-gated via `task:assign`. Implemented in `run_v52`.
- **R2 — repurposing `create_sprint_retrospective`** changes an existing tool's args/behavior. Confirm no external prompt/automation relies on its old memory-aggregation output. If it does, keep the old tool and add `create_sprint_retro` for the persisted one.
- **R3 — no `lead` template exists** in migration v5, so "assign→senior/lead" collapses to senior-only. If a lead template is expected, it must be seeded first (out of this change's scope).
- **R4 — resolve-by-spec is `task:write`-gated** and org-scoped; the sdd-verify/archive skill must run under a key that holds `task:write` in the relevant project. If skills run under a low-privilege key, auto-resolve silently no-ops (403). Confirm the skill's execution identity.
- **R5 — spec-link fs validation** reads the openspec tree from the backend process's working dir; if the backend runs elsewhere than the repo root, `spec_change_exists` always fails. Confirm the backend has access to the openspec path, or make validation advisory (warn, don't block) — design currently blocks with 422.
