# Tasks: Team Tasks

> STRICT TDD MODE ACTIVE (`openspec/config.yaml`: `strict_tdd: true`, `tdd_scope: backend_and_admin`). Every backend/admin implementation item is a RED (failing test) -> GREEN (implementation) pair. MCP items also follow RED->GREEN per project convention even though `tdd_scope` does not force it, to keep the whole change internally consistent.
>
> Test commands:
> - Backend: `cd apps/backend && cargo test`
> - Admin: `cd apps/admin && npm run test` (vitest run)
> - MCP: `cd nexusmind-mcp && npm test` (tsx --test; `pretest: tsc`; new test files MUST be added to the `test` script's file list in `package.json`)
>
> Locked decisions restated here for acceptance criteria:
> - Status set: `backlog` / `todo` / `in_progress` / `in_review` / `done` / `cancelled` (fixed, no custom statuses).
> - Permissions: `task:read`, `task:write`, `task:assign`, `task:delete`, `task:manage`. Grant matrix (run_v52): `tmpl_dev_junior` = read+write (create/edit; assigning to others stays senior-gated); `tmpl_dev_senior` = read+write+assign+delete; `tmpl_security_officer`/`tmpl_auditor` = read only; `task:manage` granted to no template (admin-only via privilege bypass).
> - `spec_change_exists` filesystem validation for spec-links is **ADVISORY**: an unmatched name is still rejected at link-creation time (4xx, per spec `team-tasks-spec-links`), but the check itself does not hard-fail the request if the openspec root is unreadable — it logs and treats an unreadable root as "cannot confirm, allow" rather than blocking all linking. This resolves design's open risk R5: unreadable root does not brick spec-links.
> - `create_sprint_retrospective` is repurposed (not duplicated) to back the real `POST /v1/sprints/:id/retrospectives` endpoint.

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | ~3130 total (PR1 ~350, PR2 ~400, PR3 ~300, PR4 ~340, PR5 ~230, PR6 ~370, PR7 ~380, PR8 ~400, PR9 ~150, PR10 ~400, optional PR11 board ~300) |
| 400-line budget risk | High overall; per-PR risk: PR2/PR6/PR7/PR8/PR10 at or near budget (Medium-High), rest Low-Medium |
| Chained PRs recommended | Yes |
| Suggested split | PR1 -> PR2 -> {PR3, PR4, PR5, PR6, PR7 in parallel off PR2} -> PR8 -> PR9 -> PR10 -> (optional PR11) |
| Delivery strategy | ask-on-risk (per orchestrator cache) |
| Chain strategy | pending (resolve with orchestrator before apply) |

Decision needed before apply: Yes
Chained PRs recommended: Yes
Chain strategy: pending
400-line budget risk: High

### Suggested Work Units

| PR | Branch name | Capabilities | Est. lines | Depends on |
|----|-------------|--------------|-----------:|------------|
| PR0 | `team-tasks/pr0-migrations-models` | team-tasks-core (data layer) | ~350 | — |
| PR1 | `team-tasks/pr1-core-api` | team-tasks-core | ~400 | PR0 |
| PR2 | `team-tasks/pr2-assignment` | team-tasks-assignment | ~300 | PR1 |
| PR3 | `team-tasks/pr3-organization` | team-tasks-organization | ~340 | PR1 |
| PR4 | `team-tasks/pr4-collaboration` | team-tasks-collaboration | ~230 | PR1 |
| PR5 | `team-tasks/pr5-spec-links` | team-tasks-spec-links | ~370 | PR1 |
| PR6 | `team-tasks/pr6-sprints` | team-tasks-sprints | ~380 | PR1 |
| PR7 | `team-tasks/pr7-mcp-tools` | team-tasks-agent-tools (pull) | ~400 | PR1, PR2, PR3, PR4, PR5, PR6 (needs all routes mounted) |
| PR8 | `team-tasks/pr8-mcp-session-start` | team-tasks-agent-tools (push) | ~150 | PR7 |
| PR9 | `team-tasks/pr9-admin-ui-list` | team-tasks-admin-ui | ~400 | PR1, PR2, PR3, PR4, PR5 |
| PR10 (optional) | `team-tasks/pr10-admin-ui-board` | team-tasks-admin-ui (board view) | ~300 | PR9 |

> Note: design.md's PR numbering (PR1..PR10) is renumbered here as PR0..PR9 (+ optional PR10) to keep a single, unambiguous zero-based sequence for `sdd-apply` batches; the design's PR1 (migration+model) = this file's PR0, design's PR2 (core-api) = PR1, and so on through design's PR8/9/10 = this file's PR7/8/9. PR3-PR6 (assignment/organization/collaboration/spec-links) and PR6 (sprints) are parallelizable once PR1 lands, since they only depend on core-api's routes and query helpers, not on each other.

---

## PR0 — core-migration+model (team-tasks-core: data layer)

**Goal**: create all 7 task tables + indexes, grant `task:*` permissions to role templates, and define the `TaskStatus` enum + transition matrix + core model structs. No HTTP surface yet.

**Satisfies**: `team-tasks-core` / "Fixed Task Status Set" (status enum + validation), status-transition groundwork for "Task Updates Require Write Permission and Validate Status Transitions". Grant-matrix groundwork for every permission-gated requirement across all 7 backend capabilities.

**Est. changed lines**: ~350
**Depends on**: — (first PR)

### Checklist

- [x] 0.1 RED: migration test `run_v51_creates_task_tables` in `apps/backend/src/db/migrations.rs` asserting all 7 tables (`tasks`, `task_assignees`, `task_labels`, `task_comments`, `task_spec_links`, `sprints`, `sprint_retrospectives`) exist with expected columns after `run_all` on a fresh `:memory:` connection.
- [x] 0.2 GREEN: implement `run_v51` in `apps/backend/src/db/migrations.rs` — `CREATE TABLE IF NOT EXISTS` for all 7 tables per design section 1.2 (columns, FKs `ON DELETE CASCADE/SET NULL/RESTRICT`, `UNIQUE` composites), guard `PRAGMA user_version` < 51, tail `PRAGMA user_version = 51;`. Append `run_v51(conn)?;` to `run_all()` after `run_v50(conn)?;`.
- [x] 0.3 RED: migration test `run_v51_creates_indexes` asserting `idx_tasks_org_project_status`, `idx_tasks_org_parent`, `idx_tasks_sprint`, `idx_task_assignees_user`, `idx_task_labels_label`, `idx_task_comments_task`, `idx_task_spec_links_change`, `idx_sprints_org_project_status`, `idx_sprint_retros_sprint` exist via `sqlite_master`.
- [x] 0.4 GREEN: add the 9 `CREATE INDEX IF NOT EXISTS` statements to `run_v51`.
- [x] 0.5 RED: migration test `run_v51_is_idempotent` — running `run_all` twice on the same connection does not error and table/index counts are unchanged.
- [x] 0.6 GREEN: ensure `run_v51`'s guard makes double-invocation a no-op (covered by the `user_version` gate already in 0.2; add explicit assertion if guard logic needs adjustment).
- [x] 0.7 RED: migration test `run_v51_fk_cascade_and_unique` — deleting a task cascades to `task_assignees`/`task_labels`/`task_comments`/`task_spec_links`; deleting a sprint cascades to `sprint_retrospectives` and SETs `tasks.sprint_id` NULL; `UNIQUE(task_id, user_id)` on `task_assignees`, `UNIQUE(task_id, label)` on `task_labels`, `UNIQUE(task_id, spec_change_name)` on `task_spec_links`, `UNIQUE(org_id, project, name)` on `sprints` are enforced (insert duplicate -> constraint error).
- [x] 0.8 GREEN: verify/adjust FK `ON DELETE` clauses and `UNIQUE` composites in `run_v51` to satisfy 0.7 (implementation already drafted in 0.2; this task closes any gaps found by the cascade test).
- [x] 0.9 RED: migration test `run_v52_grants_task_perms` — after `run_all`, the seeded `roles` rows for `tmpl_dev_junior`, `tmpl_dev_senior`, `tmpl_security_officer`, `tmpl_auditor` have `permissions` JSON containing exactly the grant matrix from design section 1.4 (junior: `task:read`,`task:write`; senior: `task:read`,`task:write`,`task:assign`,`task:delete`; security_officer/auditor: `task:read`; no template gets `task:manage`).
- [x] 0.10 GREEN: implement `run_v52` in `apps/backend/src/db/migrations.rs` — `json_insert`/`json_set` on each template's `permissions` column per the grant matrix, guarded by `PRAGMA user_version` < 52, checking membership before insert (idempotency requirement). Append `run_v52(conn)?;` to `run_all()` after `run_v51(conn)?;`. Tail `PRAGMA user_version = 52;`.
- [x] 0.11 RED: migration test `run_v52_is_idempotent` — running `run_all` twice does not duplicate permission strings in any template's JSON array.
- [x] 0.12 GREEN: adjust `run_v52`'s `json_insert` guard (existence check via `json_each`/`instr` before insert) to satisfy 0.11.
- [x] 0.13 RED: migration test `run_v52_preserves_existing_permissions` — pre-existing permission strings on each template (e.g. `harness:read`, `session:write`) remain present and unchanged after `run_v52` runs.
- [x] 0.14 GREEN: confirm `run_v52` only appends (never replaces) the `permissions` array; fix if the JSON mutation is destructive.
- [x] 0.15 RED: unit test in `apps/backend/src/models/types.rs` for `TaskStatus::from_str` — all 6 valid strings (`backlog`,`todo`,`in_progress`,`in_review`,`done`,`cancelled`) parse to the correct variant; an unrecognized string returns `Err` (Spec: team-tasks-core / "Fixed Task Status Set" / "Reject an unrecognized status value").
- [x] 0.16 GREEN: implement `TaskStatus` enum with hand-rolled `FromStr`/`Display` in `apps/backend/src/models/types.rs`, mirroring `Role`'s pattern.
- [x] 0.17 RED: unit test for `TaskStatus::Display` — each variant serializes back to its exact snake_case string.
- [x] 0.18 GREEN: implement `Display` for `TaskStatus` (covered alongside 0.16 if not already complete).
- [x] 0.19 RED: unit test for `can_transition(from, to) -> bool` — table-driven test asserting every edge in design section 2.2's transition matrix (including same-state no-op = true, and `done`/`cancelled` reopen edges) and that all non-listed edges return `false` (Spec: team-tasks-core / "Task Updates Require Write Permission and Validate Status Transitions" / "Status transition to an invalid target state is rejected").
- [x] 0.20 GREEN: implement `fn can_transition(from: TaskStatus, to: TaskStatus) -> bool` in `apps/backend/src/models/types.rs` per the matrix.
- [x] 0.21 GREEN (no dedicated RED — struct definitions, covered transitively by PR1 handler tests): add response/DTO structs to `apps/backend/src/models/types.rs`: `Task`, `TaskAssignee` (mirrors `HarnessOwner`), `TaskComment`, `Sprint`, `SprintRetrospective`, `CreateTaskRequest`, `PatchTaskRequest`, `AssignTaskRequest`, `AddLabelRequest`, `AddCommentRequest`, `LinkSpecRequest`, `ResolveBySpecRequest`, `CreateSprintRequest`, `PatchSprintRequest`, `CreateRetrospectiveRequest` per design section 2.3.

---

## PR1 — core-api (team-tasks-core: CRUD handlers)

**Goal**: task CRUD queries + handlers + routes with permission gating, visibility scoping, filtering, and pagination.

**Satisfies**: `team-tasks-core` — all 6 requirements ("Fixed Task Status Set" enforcement at the write boundary, "Task Creation Requires Write Permission", "Task Reads Are Scoped to Project Membership", "Task Updates Require Write Permission and Validate Status Transitions", "Task Deletion Is a Soft-Delete Gated by Delete Permission", "Task Listing Supports Filtering and Pagination").

**Est. changed lines**: ~400
**Depends on**: PR0

### Checklist

- [x] 1.1 RED: `apps/backend/src/db/queries.rs` test `create_task_persists_defaults` — creating with only `title`+`project` defaults `status` to `backlog`, sets `created_by`/timestamps (Spec: "Create a task with minimal fields").
- [x] 1.2 GREEN: implement `pub fn create_task(conn, org_id, created_by, &CreateTaskRequest) -> Result<Task>` in `apps/backend/src/db/queries.rs`.
- [x] 1.3 RED: query test `create_task_rejects_invalid_status` — status outside the fixed set fails to construct `TaskStatus` before insert (Spec: "Reject an unrecognized status value").
- [x] 1.4 GREEN: validate `status`/`priority` parsing in `create_task` (or the handler layer) before persisting, returning a typed error the handler maps to 4xx.
- [x] 1.5 RED: query test `get_task_hydrates_relations` — `get_task` returns assignees/labels/spec_links/comment_count/subtask_count populated.
- [x] 1.6 GREEN: implement `pub fn get_task(conn, org_id, task_id) -> Result<Option<Task>>` (hydrated) in `queries.rs`.
- [x] 1.7 RED: query test `patch_task_updates_fields_and_bumps_updated_at` (Spec: "Update task fields with write permission").
- [x] 1.8 GREEN: implement `pub fn patch_task(conn, org_id, task_id, &PatchTaskRequest) -> Result<Option<Task>>`.
- [x] 1.9 RED: query test `patch_task_rejects_illegal_transition` — patching `status` from `done` to `todo` returns a typed transition error and leaves status unchanged (Spec: "Status transition to an invalid target state is rejected").
- [x] 1.10 GREEN: wire `can_transition` (from PR0) into `patch_task`'s status-change path; return `Err` variant mapped to 422 `{ code: "invalid_transition" }` by the handler.
- [x] 1.11 RED: query test `soft_delete_task_sets_archived_at_and_excludes_from_list` (Spec: "Soft-delete a task with delete permission").
- [x] 1.12 GREEN: implement `pub fn soft_delete_task(conn, org_id, task_id) -> Result<bool>`; ensure `list_tasks`/`get_task` filter `archived_at IS NULL` by default.
- [x] 1.13 RED: query test `list_tasks_filters_by_project_status_priority` + parallel `count_tasks_matches_filtered_set` (Spec: "List tasks filtered by status", "Paginated list reports an accurate total").
- [x] 1.14 GREEN: implement `pub fn list_tasks(conn, org_id, viewer, filters: &TaskListFilters, limit, offset) -> Result<Vec<Task>>` and `pub fn count_tasks(...) -> Result<i64>` with dynamic `WHERE` (`String` + `push_str` + `Vec<&dyn ToSql>`), reusing `visibility_predicate("project", "?N")`, `LIMIT`/`OFFSET`.
- [x] 1.15 RED: HTTP test in `apps/backend/src/api/tasks.rs` (`#[cfg(test)] mod tests`, `tower::oneshot`) — `POST /v1/tasks` with `task:write` creates and returns `201 Task`; without `task:write` returns `403` and no row is created (Spec: "Create task denied without write permission").
- [x] 1.16 GREEN: implement `pub async fn create_task_handler` in `apps/backend/src/api/tasks.rs` — `State(store)`, `Extension(auth)`, `AppJson<CreateTaskRequest>`, `require_permission(&conn, &auth, Some(&project), "task:write")`, calls `queries::create_task`, returns `(StatusCode::CREATED, Json(task))`.
- [x] 1.17 RED: HTTP test `get_task_returns_404_for_non_member` — a caller outside the task's project gets `404`, not `403` (Spec: "Non-member read returns 404, not 403").
- [x] 1.18 GREEN: implement `get_task_handler` — visibility check via `visibility_predicate`-backed query; not-found or not-visible both fall through to `not_found()`.
- [x] 1.19 RED: HTTP test `list_tasks_scoped_to_membership` — member sees project tasks; non-member sees empty/`404` per project-scoped list semantics (Spec: "Member reads tasks in their project").
- [x] 1.20 GREEN: implement `list_tasks_handler` — parses query filters (`project?`, `status?`, `priority?`, `limit?`, `offset?`), calls `queries::list_tasks`/`count_tasks`, applies existence-leak rule.
- [x] 1.21 RED: HTTP test `patch_task_denied_without_write_permission` — `403`, task unmodified (Spec: "Update denied without write permission").
- [x] 1.22 GREEN: implement `patch_task_handler` — `require_permission(..., "task:write")`, calls `queries::patch_task`, maps `can_transition` failure to 422.
- [x] 1.23 RED: HTTP test `delete_task_requires_delete_permission_not_just_write` — caller with `task:write` but not `task:delete` gets `403`, `archived_at` stays null (Spec: "Delete denied without delete permission").
- [x] 1.24 GREEN: implement `delete_task_handler` — `require_permission(..., "task:delete")`, calls `queries::soft_delete_task`, returns `204`.
- [x] 1.25 RED: HTTP test `delete_nonexistent_or_invisible_task_returns_404` (Spec: "Delete a non-existent or already-archived task").
- [x] 1.26 GREEN: ensure `delete_task_handler` resolves visibility before permission check consistently with the existence-leak rule (visibility 404 takes precedence over permission 403 when caller cannot see the task at all).
- [x] 1.27 RED: HTTP test `list_tasks_pagination_reports_accurate_total` — `limit`/`offset` params return a page `<= limit`, `X-Total-Count`/body total matches `count_tasks` (Spec: "Paginated list reports an accurate total").
- [x] 1.28 GREEN: wire pagination response shape (reuse `resolve_list_pagination` from `helpers.rs`) in `list_tasks_handler`.
- [x] 1.29 GREEN: register `pub mod tasks;` in `apps/backend/src/api/mod.rs`.
- [x] 1.30 GREEN: import `tasks` and mount `GET/POST /v1/tasks`, `GET/PATCH/DELETE /v1/tasks/:id` on the `protected` router in `apps/backend/src/api/router.rs`.

---

## PR2 — assignment (team-tasks-assignment)

**Goal**: multiple assignees per task via `task_assignees`, `task:assign`-gated, org-membership-validated, idempotent re-assign, denormalized display.

**Satisfies**: `team-tasks-assignment` — all 4 requirements.

**Est. changed lines**: ~300
**Depends on**: PR1

### Checklist

- [x] 2.1 RED: query test `set_task_assignees_returns_denormalized_display` — assigning 2 users returns `TaskAssignee{id,name,email}` entries, not bare ids (Spec: "Assign multiple users to a task").
- [x] 2.2 GREEN: implement `pub fn set_task_assignees(conn, org_id, task_id, assigned_by, user_ids: &[String]) -> Result<Vec<TaskAssignee>>` in `apps/backend/src/db/queries.rs`, joining `users` for display fields.
- [x] 2.3 RED: query test `set_task_assignees_rejects_user_outside_org` — assigning a user belonging only to org B fails with a typed validation error, no row created (Spec: "Reject assigning a user from another organization").
- [x] 2.4 GREEN: call `user_belongs_to_org(conn, org_id, user_id)` per user inside `set_task_assignees`; short-circuit with `Err` on first failing id, no partial writes (wrap in a transaction).
- [x] 2.5 RED: query test `set_task_assignees_rejects_nonexistent_user` (Spec: "Reject assigning a non-existent user").
- [x] 2.6 GREEN: extend `user_belongs_to_org`-based check to also cover "user id does not exist at all" (same negative path).
- [x] 2.7 RED: query test `set_task_assignees_is_idempotent_for_duplicate` — assigning an already-assigned user twice results in exactly one row (Spec: "Re-assign an already-assigned user").
- [x] 2.8 GREEN: use `INSERT OR IGNORE` (relying on `UNIQUE(task_id, user_id)` from PR0) or an existence pre-check in `set_task_assignees`.
- [x] 2.9 RED: query test `remove_task_assignee_deletes_row`.
- [x] 2.10 GREEN: implement `pub fn remove_task_assignee(conn, task_id, user_id) -> Result<bool>`.
- [x] 2.11 RED: query test `list_task_assignees_returns_display_data`.
- [x] 2.12 GREEN: implement `pub fn list_task_assignees(conn, task_id) -> Result<Vec<TaskAssignee>>`.
- [x] 2.13 RED: HTTP test `assign_denied_without_task_assign` — caller with `task:write` but not `task:assign` gets `403`, no assignee row created (Spec: "Assign denied without task:assign").
- [x] 2.14 GREEN: implement `POST /v1/tasks/:id/assignees` handler in `apps/backend/src/api/tasks.rs` — loads parent task, applies visibility, `require_permission(..., "task:assign")`, calls `queries::set_task_assignees`.
- [x] 2.15 RED: HTTP test `unassign_denied_without_task_assign` (Spec: "Unassign denied without task:assign").
- [x] 2.16 GREEN: implement `DELETE /v1/tasks/:id/assignees/:user_id` handler, same permission gate.
- [x] 2.17 RED: HTTP test `assign_succeeds_with_task_assign_permission` (Spec: "Assign succeeds with task:assign").
- [x] 2.18 GREEN: close any gaps in handler wiring found by 2.17.
- [x] 2.19 RED: HTTP test `read_assignees_requires_only_task_read` — a caller with `task:read` (not `task:assign`) sees assignees on `GET /v1/tasks/:id` (Spec: "Read assignees requires only read permission").
- [x] 2.20 GREEN: confirm `get_task_handler` (PR1) hydrates `assignees` via `list_task_assignees` regardless of `task:assign`.
- [x] 2.21 RED: MCP-relevant backend test `list_tasks_assignee_me_filter` — `GET /v1/tasks?assignee=me` resolves `auth.user_id` and joins `task_assignees` to filter (groundwork for `list_my_tasks`, Spec dependency: team-tasks-agent-tools "list_my_tasks returns only the caller's assigned tasks").
- [x] 2.22 GREEN: extend `TaskListFilters`/`list_tasks` query to support `assignee: Option<AssigneeFilter>` (`Me(user_id)` variant) and mount `assignee=me` parsing in `list_tasks_handler`.
- [x] 2.23 GREEN: mount `POST /v1/tasks/:id/assignees` and `DELETE /v1/tasks/:id/assignees/:user_id` routes in `apps/backend/src/api/router.rs`.

---

## PR3 — organization (team-tasks-organization)

**Goal**: labels (attach/detach/filter) + subtasks (`parent_id`, one-level nesting, cross-project rejection, parent-delete preserves subtasks).

**Satisfies**: `team-tasks-organization` — all 3 requirements.

**Est. changed lines**: ~340
**Depends on**: PR1

### Checklist

- [x] 3.1 RED: query test `add_task_label_appends_to_list` (Spec: "Attach a label to a task").
- [x] 3.2 GREEN: implement `pub fn add_task_label(conn, task_id, label) -> Result<Vec<String>>` in `queries.rs`.
- [x] 3.3 RED: query test `remove_task_label_removes_it` (Spec: "Remove a label from a task").
- [x] 3.4 GREEN: implement `pub fn remove_task_label(conn, task_id, label) -> Result<bool>`.
- [x] 3.5 RED: query test `list_tasks_filter_by_label` (Spec: "Filter task list by label").
- [x] 3.6 GREEN: extend `TaskListFilters`/`list_tasks` with a `label: Option<String>` filter (join `task_labels`).
- [x] 3.7 RED: HTTP test `label_write_denied_without_permission` — `403` for both attach and remove without `task:write` (Spec: "Label write denied without permission").
- [x] 3.8 GREEN: implement `POST /v1/tasks/:id/labels` and `DELETE /v1/tasks/:id/labels/:label` handlers in `apps/backend/src/api/tasks.rs`, gated `task:write`, loading parent task visibility first.
- [x] 3.9 RED: query test `create_task_with_parent_id_creates_subtask` — new task with `parent_id` set is created and appears in `list_subtasks(parent_id)` (Spec: "Create a subtask under a parent").
- [x] 3.10 GREEN: extend `create_task` to accept `parent_id` (already in `CreateTaskRequest` from PR0); implement `pub fn list_subtasks(conn, org_id, parent_id) -> Result<Vec<Task>>`.
- [x] 3.11 RED: query test `create_task_rejects_nesting_under_a_subtask` — creating task C with `parent_id` = an existing subtask's id (task B, itself a child of task A) fails with a 4xx-mappable error and no row is created (Spec: "Reject nesting a subtask under a subtask").
- [x] 3.12 GREEN: in `create_task` (or a pre-check helper `fn assert_not_nested_subtask(conn, parent_id) -> Result<()>`), reject when the target `parent_id` row itself has a non-null `parent_id`.
- [x] 3.13 RED: query test `create_task_rejects_cross_project_parent` — parent task in project X, create request specifies project Y with that `parent_id` -> 4xx-mappable error, no row created (Spec: "Reject cross-project parent/child").
- [x] 3.14 GREEN: extend the same pre-check helper to assert `parent.project == request.project` before insert.
- [x] 3.15 RED: HTTP test `get_subtasks_endpoint_returns_children` — `GET /v1/tasks/:id/subtasks` returns children ordered/filtered by `parent_id` (Spec: "Create a subtask under a parent" — endpoint-level coverage).
- [x] 3.16 GREEN: implement `GET /v1/tasks/:id/subtasks` handler in `apps/backend/src/api/tasks.rs`, gated `task:read`, loads parent visibility first, calls `queries::list_subtasks`.
- [x] 3.17 RED: query/HTTP test `soft_delete_parent_does_not_cascade_to_subtasks` — soft-deleting a parent with subtasks sets only the parent's `archived_at`; subtasks remain readable, non-archived, and still report the (now-archived) `parent_id` (Spec: "Soft-delete a parent with existing subtasks").
- [x] 3.18 GREEN: confirm `soft_delete_task` (PR1) never cascades to `tasks.parent_id` children (the FK is `ON DELETE CASCADE` only for hard delete, which v1 never performs — soft-delete is a plain `UPDATE`, so no code change should be needed; this task closes the loop with an explicit assertion and comment documenting the invariant).
- [x] 3.19 RED: query test `subtask_status_update_does_not_affect_parent` (Spec: "Subtask status is independent of parent status").
- [x] 3.20 GREEN: confirm `patch_task` never propagates a status write to `parent_id` (no code path currently does; add a regression test-only task if a gap is found).
- [x] 3.21 GREEN: mount `POST /v1/tasks/:id/labels`, `DELETE /v1/tasks/:id/labels/:label`, `GET /v1/tasks/:id/subtasks` routes in `apps/backend/src/api/router.rs`.

---

## PR4 — collaboration (team-tasks-collaboration)

**Goal**: threaded comments — create (write-gated), list (read-gated), delete (author-or-manage-gated).

**Satisfies**: `team-tasks-collaboration` — all 3 requirements.

**Est. changed lines**: ~230
**Depends on**: PR1

### Checklist

- [x] 4.1 RED: query test `add_task_comment_persists_author_body_timestamp` (Spec: "Add a comment to a task").
- [x] 4.2 GREEN: implement `pub fn add_task_comment(conn, task_id, user_id, body) -> Result<TaskComment>` in `queries.rs`.
- [x] 4.3 RED: query test `add_task_comment_rejects_empty_or_whitespace_body` (Spec: "Reject an empty comment body").
- [x] 4.4 GREEN: validate `body.trim().is_empty()` in the handler (or `add_task_comment`), returning a 4xx-mappable error before insert.
- [x] 4.5 RED: query test `list_task_comments_returns_chronological_order` (Spec: "List comments with read permission").
- [x] 4.6 GREEN: implement `pub fn list_task_comments(conn, task_id) -> Result<Vec<TaskComment>>` ordered by `created_at ASC`.
- [x] 4.7 RED: HTTP test `add_comment_denied_without_write_permission` — `403`, no comment row created (Spec: "Comment creation denied without write permission").
- [x] 4.8 GREEN: implement `POST /v1/tasks/:id/comments` handler in `apps/backend/src/api/tasks.rs`, gated `task:write`, loads parent task visibility first.
- [x] 4.9 RED: HTTP test `list_comments_non_member_returns_404` — non-member fetching the task or its comments gets `404`, no comment content leaked (Spec: "Non-member cannot read comments").
- [x] 4.10 GREEN: implement `GET /v1/tasks/:id/comments` handler, gated `task:read`, existence-leak rule applied via parent task visibility.
- [x] 4.11 RED: query test `delete_comment_by_author_succeeds` (Spec: "Author deletes their own comment").
- [x] 4.12 GREEN: implement `pub fn delete_task_comment(conn, comment_id) -> Result<bool>` and a `pub fn get_task_comment_author(conn, comment_id) -> Result<Option<String>>` lookup helper in `queries.rs`.
- [x] 4.13 RED: HTTP test `delete_comment_by_manager_succeeds_for_others_comment` (Spec: "Manager deletes another user's comment").
- [x] 4.14 GREEN: implement `DELETE /v1/tasks/:id/comments/:comment_id` handler — allow if `auth.user_id == comment.author_id` OR `require_permission(..., "task:manage")` passes.
- [x] 4.15 RED: HTTP test `delete_comment_denied_for_non_author_non_manager` — `403`, comment not removed (Spec: "Non-author, non-manager deletion is denied").
- [x] 4.16 GREEN: close any gap in the author-or-manage check found by 4.15.
- [x] 4.17 GREEN: mount `GET/POST /v1/tasks/:id/comments` and `DELETE /v1/tasks/:id/comments/:comment_id` routes in `apps/backend/src/api/router.rs`.

---

## PR5 — spec-links + auto-resolve (team-tasks-spec-links)

**Goal**: many-to-many task<->openspec-change-name links, filesystem validation at link time (advisory per the resolved decision above), and the `resolve-by-spec` endpoint that auto-transitions linked tasks to `done`, bypassing the transition matrix and per-project membership scoping.

**Satisfies**: `team-tasks-spec-links` — all 5 requirements.

**Est. changed lines**: ~370
**Depends on**: PR1

### Checklist

- [x] 5.1 RED: query test `link_task_spec_adds_to_list` (Spec: "Link a task to a spec change").
- [x] 5.2 GREEN: implement `pub fn link_task_spec(conn, task_id, linked_by, spec_change_name) -> Result<()>` in `queries.rs`.
- [x] 5.3 RED: query test `link_task_spec_supports_multiple_changes_per_task` and `multiple_tasks_per_change` (Spec: "Link one task to multiple changes", "Link multiple tasks to the same change").
- [x] 5.4 GREEN: confirm `link_task_spec` has no uniqueness constraint beyond `UNIQUE(task_id, spec_change_name)` from PR0 (already satisfies both scenarios; add regression coverage only).
- [x] 5.5 RED: unit test `spec_change_exists_matches_active_tree` — `fn spec_change_exists(root: &Path, name: &str) -> bool` returns true for a folder under `openspec/changes/<name>/` (Spec: "Link to an active change succeeds").
- [x] 5.6 GREEN: implement `spec_change_exists` helper in `apps/backend/src/api/tasks.rs` (or a small `spec_link.rs` submodule) checking the active tree.
- [x] 5.7 RED: unit test `spec_change_exists_matches_archived_tree` — matches `openspec/changes/archive/*-<name>/` via glob (Spec: "Link to an archived change succeeds").
- [x] 5.8 GREEN: extend `spec_change_exists` to glob the archive tree (date-prefixed folder names).
- [x] 5.9 RED: unit test `spec_change_exists_returns_false_for_unknown_name` and HTTP test `link_spec_rejects_unknown_change_name` — `POST /v1/tasks/:id/spec-links` with an unmatched name returns `422 { code: "unknown_spec" }`, no link created (Spec: "Reject linking to a non-existent change name").
- [x] 5.10 GREEN: implement `POST /v1/tasks/:id/spec-links` handler — loads parent task visibility, `require_permission(..., "task:write")`, calls `spec_change_exists`, rejects with 422 on no-match, else `queries::link_task_spec`.
- [x] 5.11 RED: unit test `spec_change_exists_treats_unreadable_root_as_advisory_pass` — when the openspec root path cannot be read (e.g. missing dir in test harness), `spec_change_exists` returns `true` (advisory decision documented above) rather than hard-failing every link (regression test for design risk R5).
- [x] 5.12 GREEN: implement the advisory fallback in `spec_change_exists` — on `std::io::Error` reading the root, log a warning and return `true` (do not block linking) instead of `false`.
- [x] 5.13 RED: HTTP test `get_spec_links_returns_list` (Spec: "Link a task to a spec change" — read-back coverage).
- [x] 5.14 GREEN: implement `GET /v1/tasks/:id/spec-links` handler + `pub fn list_task_spec_links(conn, task_id) -> Result<Vec<String>>` query.
- [x] 5.15 RED: query/HTTP test `read_task_with_dangling_spec_link_still_succeeds` — a link whose folder was renamed away still appears in `spec_links` on a normal `GET /v1/tasks/:id` (Spec: "Read a task with a dangling spec link").
- [x] 5.16 GREEN: confirm `get_task` (PR1) hydrates `spec_links` via `list_task_spec_links` with no filesystem re-validation on read (validation is link-time only).
- [x] 5.17 RED: HTTP test `remove_spec_link_denied_without_write_permission` (Spec: "Remove spec link denied without write permission").
- [x] 5.18 GREEN: implement `DELETE /v1/tasks/:id/spec-links/:name` handler, gated `task:write`, calls `pub fn unlink_task_spec(conn, task_id, spec_change_name) -> Result<bool>`.
- [x] 5.19 RED: query test `resolve_tasks_by_spec_transitions_all_linked_non_terminal_tasks` — 3 tasks linked to `"team-tasks"`, none `done`/`cancelled`, all transition to `done` and are returned as ids (Spec: "Resolve-by-spec transitions all linked tasks").
- [x] 5.20 GREEN: implement `pub fn resolve_tasks_by_spec(conn, org_id, spec_change_name) -> Result<Vec<String>>` in `queries.rs` — looks up `task_spec_links` by name, forces `status='done'` (bypassing `can_transition`) + bumps `updated_at` for each non-terminal task in the org, returns transitioned ids.
- [x] 5.21 RED: query test `resolve_tasks_by_spec_noop_for_unlinked_change` — zero transitioned, no task modified (Spec: "Resolve-by-spec is a no-op for an unlinked change name").
- [x] 5.22 GREEN: confirm `resolve_tasks_by_spec` returns an empty vec cleanly when no `task_spec_links` row matches.
- [x] 5.23 RED: query test `resolve_tasks_by_spec_skips_already_terminal_tasks` — a `cancelled` task linked to the change stays `cancelled` and is excluded from the transitioned list (Spec: "Resolve-by-spec skips already-terminal tasks").
- [x] 5.24 GREEN: add the `status NOT IN ('done','cancelled')` guard to the `UPDATE` in `resolve_tasks_by_spec`.
- [x] 5.25 RED: HTTP test `resolve_by_spec_transitions_across_projects_ignoring_membership` — tasks linked to the change span multiple projects; the endpoint transitions all of them for a caller holding org-level `task:write`, unblocked by `visibility_predicate`/project-membership (Spec: "Resolve-by-spec requires write authority, not caller project membership").
- [x] 5.26 GREEN: implement `POST /v1/tasks/resolve-by-spec` handler — `require_permission(&conn, &auth, None, "task:write")` (project `None` = org-level check, not per-project), calls `queries::resolve_tasks_by_spec(conn, auth.org_id, name)` (no `visibility_predicate` filter applied), returns `200 { resolved: Vec<String> }`.
- [x] 5.27 GREEN: mount `GET/POST /v1/tasks/:id/spec-links`, `DELETE /v1/tasks/:id/spec-links/:name`, `POST /v1/tasks/resolve-by-spec` routes in `apps/backend/src/api/router.rs` — register the literal `resolve-by-spec` path as its own route entry (Axum matches literal segments before `:id`, but keep it as a distinct builder entry per design section 3.3).

---

## PR6 — sprints (team-tasks-sprints)

**Goal**: sprint CRUD (`task:manage`-gated), task<->sprint single-assignment grouping, and the retrospective endpoint that `create_sprint_retrospective` will be repurposed to call in PR7.

**Satisfies**: `team-tasks-sprints` — all 4 requirements.

**Est. changed lines**: ~380
**Depends on**: PR1

### Checklist

- [x] 6.1 RED: query test `create_sprint_scoped_to_project` (Spec: "Create a sprint with manage permission").
- [x] 6.2 GREEN: implement `pub fn create_sprint(conn, org_id, created_by, &CreateSprintRequest) -> Result<Sprint>` in `queries.rs`.
- [x] 6.3 RED: HTTP test `create_sprint_denied_without_manage_permission` — caller with `task:write` but not `task:manage` gets `403`, no sprint row created (Spec: "Sprint creation denied without manage permission").
- [x] 6.4 GREEN: implement `POST /v1/sprints` handler in `apps/backend/src/api/tasks.rs` (or a `sprints.rs` submodule registered alongside), gated `require_permission(..., "task:manage")`.
- [x] 6.5 RED: query test `get_sprint`, `patch_sprint`, `soft_delete_sprint`, `list_sprints` basic round-trip tests.
- [x] 6.6 GREEN: implement `pub fn get_sprint`, `pub fn patch_sprint`, `pub fn soft_delete_sprint`, `pub fn list_sprints` in `queries.rs` per design's signature table.
- [x] 6.7 GREEN: implement `GET/PATCH/DELETE /v1/sprints/:id`, `GET /v1/sprints` handlers, `task:read` for reads, `task:manage` for patch/delete.
- [x] 6.8 RED: query test `assign_task_to_sprint_appears_in_sprint_task_list` (Spec: "Add a task to a sprint").
- [x] 6.9 GREEN: extend `patch_task` (PR1) to accept `sprint_id` and set `tasks.sprint_id`; add `pub fn list_tasks_in_sprint(conn, sprint_id) -> Result<Vec<Task>>` query for the sprint's task list.
- [x] 6.10 RED: query test `assign_task_to_sprint_rejects_cross_project` — sprint in project X, task in project Y -> 4xx-mappable error, `sprint_id` not set (Spec: "Reject adding a task to a sprint in a different project").
- [x] 6.11 GREEN: add a pre-check in the sprint-assignment path (`patch_task` or a dedicated `pub fn assign_task_to_sprint(conn, task_id, sprint_id) -> Result<()>`) asserting `task.project == sprint.project` before the `UPDATE`.
- [x] 6.12 RED: query test `moving_task_to_new_sprint_removes_from_prior` — task in sprint A, re-assign to sprint B, sprint A's task list no longer includes it, sprint B's does (Spec: "Moving a task to a new sprint removes it from the prior one").
- [x] 6.13 GREEN: confirm the single nullable `sprint_id` FK design makes this a plain overwrite (no explicit removal step needed); add regression test coverage only if a gap is found.
- [x] 6.14 RED: query test `create_retrospective_persists_and_associates_with_sprint` (Spec: "Create a retrospective for a closed sprint").
- [x] 6.15 GREEN: implement `pub fn create_retrospective(conn, sprint_id, org_id, created_by, &CreateRetrospectiveRequest) -> Result<SprintRetrospective>` in `queries.rs`.
- [x] 6.16 RED: HTTP test `create_retrospective_denied_without_manage_permission` (Spec: "Retrospective creation denied without manage permission").
- [x] 6.17 GREEN: implement `POST /v1/sprints/:id/retrospectives` handler, gated `task:manage`, loads parent sprint visibility first.
- [x] 6.18 RED: HTTP test `retrospective_retrievable_via_read_path` — a persisted retrospective is retrievable via `GET /v1/sprints/:id/retrospectives`, confirming no separate client-side-only record exists (Spec: "create_sprint_retrospective tool call persists through the backend" — backend half; MCP half covered in PR7).
- [x] 6.19 GREEN: implement `GET /v1/sprints/:id/retrospectives` handler + `pub fn list_retrospectives(conn, sprint_id) -> Result<Vec<SprintRetrospective>>` query.
- [x] 6.20 RED: HTTP test `sprint_and_retrospective_reads_scoped_to_membership` — non-member listing sprints or fetching a retrospective for that project gets `404` (Spec: "Non-member cannot read sprint or retrospective data").
- [x] 6.21 GREEN: apply `visibility_predicate`-backed scoping to `list_sprints`/`get_sprint`/`list_retrospectives` handlers, consistent with the task existence-leak rule.
- [x] 6.22 GREEN: mount `GET/POST /v1/sprints`, `GET/PATCH/DELETE /v1/sprints/:id`, `GET/POST /v1/sprints/:id/retrospectives` routes in `apps/backend/src/api/router.rs`.

---

## PR7 — mcp-client+tools (team-tasks-agent-tools: pull surface)

**Goal**: `client.ts` typed fns/types + 14 MCP tools (13 new + repurposed `create_sprint_retrospective`) + `delete_task` confirm guard + release hygiene. Requires all backend routes (PR1-PR6) mounted.

**Satisfies**: `team-tasks-agent-tools` — "Pull Tools Are Thin Permissioned Wrappers Over the Task API", "list_my_tasks Resolves the Caller's Identity Server-Side", "delete_task Requires an Explicit Confirmation Guard", "resolve_tasks_for_spec Wraps the Resolve-By-Spec Endpoint". Also closes `team-tasks-sprints` / "create_sprint_retrospective tool call persists through the backend" (MCP half).

**Est. changed lines**: ~400
**Depends on**: PR1, PR2, PR3, PR4, PR5, PR6

### Checklist

- [x] 7.1 RED: `nexusmind-mcp/src/tasks-client.test.ts` — mock `globalThis.fetch`, set env before `await import('./client.js')`; assert `createTask`, `listTasks`, `listMyTasks`, `getTask`, `updateTask`, `deleteTask`, `assignTask`, `addTaskComment`, `addTaskLabel`, `linkTaskSpec`, `resolveTasksForSpec`, `listSprints`, `createSprint`, `createSprintRetrospective` each call the correct verb/path/params.
- [x] 7.2 GREEN: add `Task`, `TaskAssignee`, `TaskComment`, `Sprint`, `SprintRetrospective`, `CreateTaskInput`, `UpdateTaskInput`, `AssignTaskInput`, `CreateSprintInput`, `CreateRetrospectiveInput` types + the 14 client fns to `nexusmind-mcp/src/client.ts` per design section 5.2, each a thin `request<T>()` wrapper.
- [x] 7.3 GREEN: add `src/tasks-client.test.ts` to the `test` script's `tsx --test` file list in `nexusmind-mcp/package.json`.
- [x] 7.4 RED: `nexusmind-mcp/src/tasks-tools.test.ts` — spawn `dist/index.js` over stdio (`StdioClientTransport`) against a fake in-process HTTP backend; assert `create_task` tool call with a key lacking `task:write` fails and makes no task on the fake backend (Spec: "create_task tool enforces task:write").
- [x] 7.5 GREEN: register `create_task` tool in `nexusmind-mcp/src/index.ts` under a new `// ── Tasks ──` section — zod shape `{ project, title, description?, priority?, due_date?, parent_id?, sprint_id? }`, calls `client.createTask`, formats via `formatTask`.
- [x] 7.6 RED: tool test `assign_task_tool_enforces_task_assign` — key with `task:write` but not `task:assign` fails, no assignee created (Spec: "assign_task tool enforces task:assign").
- [x] 7.7 GREEN: register `assign_task` tool — zod shape `{ task_id, user_ids: z.array(z.string()) }`, calls `client.assignTask`.
- [x] 7.8 RED: tool test `update_task_tool_returns_formatted_confirmation` — valid status change returns human-readable text reflecting the new status (Spec: "Tool call succeeds and returns human-readable confirmation").
- [x] 7.9 GREEN: register `update_task` tool — zod shape `{ task_id, title?, description?, status?, priority?, due_date?, sprint_id? }`, calls `client.updateTask`, `formatTask` includes status.
- [x] 7.10 RED: tool test `list_my_tasks_returns_only_callers_tasks` — fake backend seeded with tasks for user A and user B; calling with A's key returns only A's tasks (Spec: "list_my_tasks returns only the caller's assigned tasks").
- [x] 7.11 GREEN: register `list_my_tasks` tool — zod shape `{ project?, status? }`, calls `client.listMyTasks` (sets `assignee=me` server-side per PR2's filter), never accepts a `user_id` arg.
- [x] 7.12 RED: tool test `list_my_tasks_filtered_by_project_and_status` (Spec: "list_my_tasks filtered by project and status").
- [x] 7.13 GREEN: confirm `list_my_tasks` forwards `project`/`status` params to `client.listMyTasks`.
- [x] 7.14 RED: tool test `delete_task_without_confirm_makes_no_backend_call` — omitting `confirm` results in zero HTTP calls to the fake backend and a text response indicating confirmation is required (Spec: "delete_task without confirm makes no backend call").
- [x] 7.15 GREEN: register `delete_task` tool — zod shape `{ task_id, confirm: z.boolean() }`; handler returns early (no `client.deleteTask` call) when `confirm !== true`, copying the `delete_memory` guard pattern.
- [x] 7.16 RED: tool test `delete_task_with_confirm_true_proceeds` (Spec: "delete_task with confirm true proceeds").
- [x] 7.17 GREEN: confirm the `confirm === true` path calls `client.deleteTask` and formats a soft-delete confirmation.
- [x] 7.18 RED: tool test `resolve_tasks_for_spec_reports_transition_count` — 2 tasks linked to `"team-tasks"`; tool response states "2 tasks were resolved" (Spec: "resolve_tasks_for_spec reports transition count").
- [x] 7.19 GREEN: register `resolve_tasks_for_spec` tool — zod shape `{ spec_change_name: z.string() }`, calls `client.resolveTasksForSpec`, formats the count from the response's `resolved` array length.
- [x] 7.20 RED: tool tests for remaining thin wrappers — `list_tasks`, `get_task`, `add_task_comment`, `add_task_label`, `link_task_spec`, `list_sprints`, `create_sprint` each call their single backend route and format a text response.
- [x] 7.21 GREEN: register `list_tasks`, `get_task`, `add_task_comment`, `add_task_label`, `link_task_spec`, `list_sprints`, `create_sprint` tools per design section 5.1's zod shapes; add `formatTaskList`, `formatSprint` helpers.
- [x] 7.22 RED: tool test `create_sprint_retrospective_persists_via_backend_not_client_stub` — invoking with a valid sprint reference + retro content results in a retrospective retrievable via the fake backend's retrospective-read path; no memory-aggregation call is made (Spec: "create_sprint_retrospective tool call persists through the backend").
- [x] 7.23 GREEN: repurpose `create_sprint_retrospective` at `nexusmind-mcp/src/index.ts` (currently a client-side `listMemories` stub per design section 5.3) — new zod shape `{ sprint_id, went_well?, went_wrong?, action_items? }`, remove the old memory-aggregation implementation, call `client.createSprintRetrospective`, format via a `formatRetrospective` helper. Add an inline comment documenting the repurposing decision and reference to design section 5.3 / risk R2.
- [x] 7.24 GREEN: add `src/tasks-tools.test.ts` to the `test` script's `tsx --test` file list in `nexusmind-mcp/package.json`.
- [x] 7.25 GREEN: bump `nexusmind-mcp/package.json` `version` 0.7.1 -> 0.8.0.
- [x] 7.26 GREEN: add a `CHANGELOG.md` entry documenting the 13 new tools + the `create_sprint_retrospective` behavior change (breaking: old memory-aggregation output removed).
- [x] 7.27 GREEN: update `README.md` tool list (if present) with the new `// ── Tasks ──` tool set.

---

## PR8 — mcp-session-start-push (team-tasks-agent-tools: push surface)

**Goal**: extend the existing SessionStart hook to inject a pending-task reminder.

**Satisfies**: `team-tasks-agent-tools` — "SessionStart Hook Injects a Pending-Task Reminder", "Pending Count Excludes Terminal Statuses".

**Est. changed lines**: ~150
**Depends on**: PR7

### Checklist

- [x] 8.1 RED: `nexusmind-mcp/src/hooks/session-start.test.ts` (new file, following `pre-compact.test.ts`'s spawn-against-fake-backend pattern) — fake backend returns 3 non-terminal tasks for the current user/project; asserts the injected context includes "You have 3 pending tasks in <project>" (Spec: "Hook injects reminder when pending tasks exist").
- [x] 8.2 GREEN: extend `nexusmind-mcp/src/hooks/session-start.ts` — add a best-effort block after the memory blocks that calls `listMyTasks({ project, status: undefined })` wrapped in `withTimeout(..., FETCH_TIMEOUT_MS)` + try/catch, counts pending tasks (status not in `done`/`cancelled`), and appends the reminder block per design section 6's format (title line + up to 5 task bullets + "…and M more").
- [x] 8.3 RED: hook test `pending_count_excludes_done_and_cancelled` — fake backend returns 1 `done`, 1 `cancelled`, 1 `in_progress` task; injected count is 1 (Spec: "Done and cancelled tasks are excluded from the pending count").
- [x] 8.4 GREEN: implement the pending-status filter (`status !== 'done' && status !== 'cancelled'`) in the hook's local counting logic.
- [x] 8.5 RED: hook test `reminder_scoped_to_active_project_only` — user has 2 pending tasks in project A, 5 in project B; active project is A; injected reminder reflects only 2 (Spec: "Hook counts only the current user's tasks in the current project").
- [x] 8.6 GREEN: confirm the hook passes the active `project` as an explicit filter to `listMyTasks`, not aggregating across projects.
- [x] 8.7 RED: hook test `hook_silent_when_zero_pending_tasks` — fake backend returns `[]`; no pending-task block is present in the injected context (Spec: "Hook is silent when there are no pending tasks").
- [x] 8.8 GREEN: implement the "omit entirely when N=0" branch (matches how empty memory blocks are omitted, per design section 6's decision).
- [x] 8.9 RED: hook test `hook_failure_does_not_block_session_start` — fake backend errors/unreachable; the hook still exits `exitClean(0)` and session start succeeds, with the reminder block omitted (Spec: "Hook failure does not block session start").
- [x] 8.10 GREEN: confirm the try/catch around the tasks fetch swallows errors and omits the block, mirroring the existing memory-block failure handling exactly.
- [x] 8.11 GREEN: add `src/hooks/session-start.test.ts` to the `test` script's `tsx --test` file list in `nexusmind-mcp/package.json`.

---

## PR9 — admin-ui (team-tasks-admin-ui: list view)

**Goal**: Tasks list page — types, API client methods, routing, permission-gated nav, list/filter/create/edit/delete via modals, detail view (comments/labels/subtasks/spec-links).

**Satisfies**: `team-tasks-admin-ui` — all 5 requirements.

**Est. changed lines**: ~400
**Depends on**: PR1, PR2, PR3, PR4, PR5

### Checklist

- [x] 9.1 GREEN (types, no dedicated RED — covered transitively by 9.2+): add `Task`, `TaskAssignee`, `TaskComment`, `Sprint`, `TaskStatus` union, `TaskPriority` union to `apps/admin/src/types.ts`.
- [x] 9.2 RED: `apps/admin/src/pages/Tasks.test.tsx` (written FIRST per design section 7.4) — `vi.mock('../api/client', ...)` returning fixture task arrays; `renderWithProviders(<Tasks />)`; asserts each task's title/status/priority/assignees/due-date render (Spec: "List renders tasks for the selected project").
- [x] 9.3 GREEN: add `listTasks(params)`, `getTask(id)`, `createTask(input)`, `updateTask(id, input)`, `deleteTask(id)`, `assignTask(id, userIds)`, `addTaskComment(id, body)`, `linkTaskSpec(id, name)`, `listSprints(params)`, `createSprint(input)` methods to `NexusMindClient` in `apps/admin/src/api/client.ts`; implement `apps/admin/src/pages/Tasks.tsx` list view using `ui/Table`, `ui/Badge` (status/priority pills), `ui/Select` (filters), `ui/EmptyState`, `ui/Button`, reusing existing Tailwind classes (`bg-accent-blue`, `text-text-primary`, `rounded-[18px]`, `rounded-full` — no new hex values).
- [x] 9.4 RED: test `empty_state_when_no_tasks_match_filters` (Spec: "Empty state when no tasks match filters").
- [x] 9.5 GREEN: render `EmptyState` when the fetched list is empty.
- [x] 9.6 RED: test `filtering_by_status_updates_visible_list_and_refetches` — selecting a status filter updates the visible rows and the `['tasks', filters]` query key changes (Spec: "Filtering by status updates the visible list").
- [x] 9.7 GREEN: wire the status `Select` filter into `useQuery(['tasks', filters], ...)`.
- [x] 9.8 RED: test `create_task_via_modal_invalidates_list_and_shows_new_task` (Spec: "Create a task via the modal form").
- [x] 9.9 GREEN: implement the create-task `Modal` form (`Select` for status/priority, text fields for title/description/due_date) + `useMutation` calling `createTask`, `qc.invalidateQueries({ queryKey: ['tasks'] })` on success.
- [x] 9.10 RED: test `create_action_hidden_without_write_permission` (Spec: "Create action hidden without write permission").
- [x] 9.11 GREEN: gate the create-task button render on the mocked auth context's `task:write` permission.
- [x] 9.12 RED: test `edit_task_status_via_modal_updates_pill` (Spec: "Edit a task's status via the modal").
- [x] 9.13 GREEN: implement the edit-task modal (pre-filled `Select`s) + `useMutation` calling `updateTask`, invalidating `['tasks']` and `['task', id]`.
- [x] 9.14 RED: test `delete_confirmed_calls_api_and_invalidates` — stub `window.confirm` to return `true` (Spec: "Delete confirmed proceeds").
- [x] 9.15 GREEN: implement the delete action calling `window.confirm(...)` then `deleteTask` + `useMutation`/`invalidateQueries` on confirm.
- [x] 9.16 RED: test `delete_cancelled_makes_no_api_call` — stub `window.confirm` to return `false` (Spec: "Delete cancelled makes no API call").
- [x] 9.17 GREEN: confirm the delete handler short-circuits before calling `deleteTask` when `window.confirm` returns falsy.
- [x] 9.18 RED: test `delete_action_hidden_without_delete_permission` (Spec: "Delete action hidden without delete permission").
- [x] 9.19 GREEN: gate the delete action render on the mocked auth context's `task:delete` permission.
- [x] 9.20 RED: test `detail_view_renders_linked_spec_changes` (Spec: "Detail view renders linked spec changes").
- [x] 9.21 GREEN: implement `apps/admin/src/pages/tasks/TaskDetail.tsx` (or a `Modal`-based detail drawer per design section 7.1) rendering a spec-links section from `task.spec_links`.
- [x] 9.22 RED: test `detail_view_renders_subtasks_with_own_status_badges` (Spec: "Detail view renders subtasks with their own status").
- [x] 9.23 GREEN: render the subtask list in `TaskDetail` with independent `Badge` status pills, fetched via `['task-subtasks', id]`.
- [x] 9.24 RED: test `adding_comment_from_detail_view_refetches_thread` (Spec: "Adding a comment from the detail view").
- [x] 9.25 GREEN: implement the comment composer in `TaskDetail` calling `addTaskComment` + `useMutation` invalidating `['task-comments', id]`.
- [x] 9.26 RED: test `nav_item_visible_with_task_read` (Spec: "Nav item visible with task:read").
- [x] 9.27 GREEN: add the "Tasks" nav item with `requiredPermission: 'task:read'` in `apps/admin/src/components/Layout.tsx`.
- [x] 9.28 RED: test `nav_item_hidden_without_task_read` (Spec: "Nav item hidden without task:read").
- [x] 9.29 GREEN: confirm `Layout.tsx`'s existing permission-filtering logic hides the item when the mocked auth context lacks `task:read` (reuse the pattern already used by other gated nav items).
- [x] 9.30 RED: test/manual-check `direct_navigation_without_permission_denied` — navigating to `/tasks` without `task:read` redirects to the standard unauthorized/401 handling (Spec: "Direct navigation without permission is denied").
- [x] 9.31 GREEN: register the lazy route `const Tasks = lazy(() => import('./pages/Tasks'))` + `<Route path="/tasks" .../>` in `apps/admin/src/App.tsx`, wrapped in the same `<AdminRoute>`/permission-guard component used by other gated routes.

---

## PR10 (optional) — admin-ui-board (team-tasks-admin-ui: board view)

**Goal**: Kanban-style board view toggle, deferred from PR9 per design section 7.1's decision to ship list-only in v1's first slice.

**Satisfies**: `team-tasks-admin-ui` — enhances "Tasks Page Lists Tasks With Filtering" with an alternate view; no new spec requirements beyond what PR9 already covers (board is explicitly out of the locked v1 requirement scenarios). Ship only if the team wants it; otherwise the spec is already fully satisfied by PR9.

**Est. changed lines**: ~300
**Depends on**: PR9

### Checklist

- [x] 10.1 RED: test `board_view_toggle_switches_between_list_and_board`.
- [x] 10.2 GREEN: implement a view-toggle control on `Tasks.tsx` and a `apps/admin/src/pages/tasks/TaskBoard.tsx` column-per-status board, reusing card radius (`rounded-[18px]`) + surface-color elevation, no new shadow token.
- [x] 10.3 RED: test `board_columns_group_by_status_correctly`.
- [x] 10.4 GREEN: group the fetched task list into per-status columns matching the fixed `TaskStatus` set.
- [x] 10.5 RED: test `board_view_respects_same_filters_as_list`.
- [x] 10.6 GREEN: share the `['tasks', filters]` query and filter controls between list and board views.

---

## Coverage Checklist (every requirement/scenario mapped to a task)

### team-tasks-core (spec: `specs/team-tasks-core/spec.md`)
- Fixed Task Status Set — Create with valid status: 1.1/1.2 (create_task) + status parsing. Reject unrecognized status: 0.15/0.16 (enum), 1.3/1.4 (write-path validation).
- Task Creation Requires Write Permission — minimal fields: 1.1/1.2. Denied without permission: 1.15/1.16.
- Task Reads Are Scoped to Project Membership — member reads: 1.19/1.20. Non-member 404 not 403: 1.17/1.18.
- Task Updates Require Write Permission and Validate Status Transitions — update fields: 1.7/1.8. Denied without permission: 1.21/1.22. Invalid transition rejected: 0.19/0.20, 1.9/1.10.
- Task Deletion Is a Soft-Delete Gated by Delete Permission — soft-delete: 1.11/1.12. Denied without permission: 1.23/1.24. Non-existent/invisible: 1.25/1.26.
- Task Listing Supports Filtering and Pagination — filter by status: 1.13/1.14. Paginated total: 1.27/1.28.

### team-tasks-assignment (spec: `specs/team-tasks-assignment/spec.md`)
- A Task May Have Multiple Assignees — assign multiple: 2.1/2.2. Read requires only task:read: 2.19/2.20.
- Assigning a User Requires the Assign Permission — assign denied: 2.13/2.14. Unassign denied: 2.15/2.16. Assign succeeds: 2.17/2.18.
- Assignee Must Belong to the Task's Organization — reject other-org: 2.3/2.4. Reject non-existent user: 2.5/2.6.
- Duplicate Assignment Is Idempotent — re-assign: 2.7/2.8.

### team-tasks-organization (spec: `specs/team-tasks-organization/spec.md`)
- Labels Attach To and Detach From a Task — attach: 3.1/3.2. Remove: 3.3/3.4. Filter by label: 3.5/3.6. Denied without permission: 3.7/3.8.
- Subtasks Form a One-Level Parent/Child Hierarchy — create subtask: 3.9/3.10. Reject nesting under subtask: 3.11/3.12. Reject cross-project: 3.13/3.14.
- Deleting a Parent Task Preserves Subtask Integrity — soft-delete parent preserves subtasks: 3.17/3.18. Subtask status independent of parent: 3.19/3.20.

### team-tasks-collaboration (spec: `specs/team-tasks-collaboration/spec.md`)
- Comments Are Created Under Write Permission — add comment: 4.1/4.2. Denied without permission: 4.7/4.8. Reject empty body: 4.3/4.4.
- Comments Are Readable by Anyone Who Can Read the Task — list with read permission: 4.5/4.6. Non-member 404: 4.9/4.10.
- Only the Comment Author or a Manager May Delete a Comment — author deletes own: 4.11/4.12. Manager deletes another's: 4.13/4.14. Denied for non-author/non-manager: 4.15/4.16.

### team-tasks-spec-links (spec: `specs/team-tasks-spec-links/spec.md`)
- A Task Links to One or More Openspec Change Names — link to change: 5.1/5.2. Multiple changes per task / multiple tasks per change: 5.3/5.4.
- Link Creation Validates the Change Name Against the Openspec Trees — active tree match: 5.5/5.6. Archived tree match: 5.7/5.8. Reject unknown name: 5.9/5.10.
- Auto-Resolve Transitions Linked Tasks to Done on Trigger — transitions all linked: 5.19/5.20. No-op for unlinked: 5.21/5.22. Skips terminal: 5.23/5.24. Not blocked by project membership: 5.25/5.26.
- Dangling Links Are Tolerated and Surfaced, Not Blocked — read with dangling link: 5.15/5.16.
- Removing a Spec Link Requires Write Permission — remove with permission: 5.13/5.14, 5.17/5.18. Denied without permission: 5.17/5.18.

### team-tasks-sprints (spec: `specs/team-tasks-sprints/spec.md`)
- Sprint Administration Requires Manage Permission — create with permission: 6.1/6.2. Denied without permission: 6.3/6.4.
- Tasks Can Be Assigned to a Sprint — add to sprint: 6.8/6.9. Reject cross-project: 6.10/6.11. Move removes from prior: 6.12/6.13.
- Sprint Retrospectives Reconcile With the Existing MCP Tool — create retrospective: 6.14/6.15. Tool call persists through backend: 6.18/6.19 (backend half) + 7.22/7.23 (MCP half). Denied without manage: 6.16/6.17.
- Sprint Reads Are Scoped to Project Membership — non-member 404: 6.20/6.21.

### team-tasks-agent-tools (spec: `specs/team-tasks-agent-tools/spec.md`)
- Pull Tools Are Thin Permissioned Wrappers Over the Task API — create_task enforces task:write: 7.4/7.5. assign_task enforces task:assign: 7.6/7.7. Success returns formatted confirmation: 7.8/7.9.
- list_my_tasks Resolves the Caller's Identity Server-Side — only caller's tasks: 7.10/7.11 (backed by 2.21/2.22). Filtered by project/status: 7.12/7.13.
- delete_task Requires an Explicit Confirmation Guard — without confirm no call: 7.14/7.15. With confirm proceeds: 7.16/7.17.
- resolve_tasks_for_spec Wraps the Resolve-By-Spec Endpoint — reports transition count: 7.18/7.19.
- SessionStart Hook Injects a Pending-Task Reminder — injects when pending exist: 8.1/8.2. Scoped to current project: 8.5/8.6. Silent when zero: 8.7/8.8. Failure does not block: 8.9/8.10.
- Pending Count Excludes Terminal Statuses — excludes done/cancelled: 8.3/8.4.

### team-tasks-admin-ui (spec: `specs/team-tasks-admin-ui/spec.md`)
- Tasks Navigation Is Gated by task:read — visible with permission: 9.26/9.27. Hidden without: 9.28/9.29. Direct nav denied: 9.30/9.31.
- Tasks Page Lists Tasks With Filtering — renders list: 9.2/9.3. Empty state: 9.4/9.5. Filter updates list: 9.6/9.7.
- Task Creation and Editing Use Modal Forms — create via modal: 9.8/9.9. Create hidden without permission: 9.10/9.11. Edit status via modal: 9.12/9.13.
- Task Deletion Requires Confirmation — confirmed proceeds: 9.14/9.15. Cancelled makes no call: 9.16/9.17. Hidden without permission: 9.18/9.19.
- Task Detail Shows Comments, Labels, Subtasks, and Spec Links — spec links: 9.20/9.21. Subtasks with own status: 9.22/9.23. Adding comment: 9.24/9.25.

**All 8 capabilities, 35 requirements, and every listed scenario are mapped to at least one RED/GREEN task pair. No orphaned requirement found.**
