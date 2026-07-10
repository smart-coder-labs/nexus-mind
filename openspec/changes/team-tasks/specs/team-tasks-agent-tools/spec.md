# Team Tasks Agent Tools Specification

## Purpose

Give agents a pull surface (MCP tools over the task API) and a push surface (a SessionStart reminder) so they can answer "what do I still owe on this project?" without leaving their session, while respecting the same permission model as the REST API.

## Requirements

### Requirement: Pull Tools Are Thin Permissioned Wrappers Over the Task API

The system MUST expose MCP tools (`list_my_tasks`, `list_tasks`, `create_task`, `update_task`, `assign_task`, `add_task_comment`, `link_task_spec`, `resolve_tasks_for_spec`, and sprint tools) as thin wrappers over the backend task routes, each inheriting the same permission gate as its underlying endpoint, adding no authority beyond the caller's existing `task:*` grants.

#### Scenario: create_task tool enforces task:write

- GIVEN an MCP caller's API key resolves to a user who lacks `task:write` for the target project
- WHEN the agent calls the `create_task` tool for that project
- THEN the tool call fails
- AND no task is created on the backend

#### Scenario: assign_task tool enforces task:assign

- GIVEN an MCP caller's API key resolves to a user who holds `task:write` but not `task:assign`
- WHEN the agent calls the `assign_task` tool
- THEN the tool call fails
- AND no assignee record is created

#### Scenario: Tool call succeeds and returns human-readable confirmation

- GIVEN an MCP caller holds `task:write` for a project
- WHEN the agent calls `update_task` with a valid status change
- THEN the backend task is updated
- AND the tool returns a formatted text confirmation reflecting the new status

### Requirement: list_my_tasks Resolves the Caller's Identity Server-Side

The system MUST have `list_my_tasks` resolve the assignee to the user identified by the `NEXUSMIND_API_KEY` used for the call (never a client-supplied user id), and MUST accept optional `project` and `status` filters as explicit tool arguments.

#### Scenario: list_my_tasks returns only the caller's assigned tasks

- GIVEN the API key belongs to user A
- AND user A is assigned to two tasks, while user B is assigned to a third
- WHEN the agent calls `list_my_tasks` with no filters
- THEN the response includes only the two tasks assigned to user A

#### Scenario: list_my_tasks filtered by project and status

- GIVEN user A has pending tasks across two projects
- WHEN the agent calls `list_my_tasks` with `project: "acme-backend"` and `status: "todo"`
- THEN only user A's `todo` tasks in `acme-backend` are returned

### Requirement: delete_task Requires an Explicit Confirmation Guard

The system MUST require an explicit `confirm: true` argument on the `delete_task` MCP tool, and MUST refuse to make any backend call — not merely refuse to delete — when `confirm` is absent or `false`.

#### Scenario: delete_task without confirm makes no backend call

- GIVEN an agent calls `delete_task` for a valid task id without a `confirm` argument
- WHEN the tool handler executes
- THEN no HTTP request is sent to the backend
- AND the tool response indicates confirmation is required

#### Scenario: delete_task with confirm true proceeds

- GIVEN an agent calls `delete_task` with `confirm: true` for a task the caller may delete
- WHEN the tool handler executes
- THEN the backend delete endpoint is called
- AND the task is soft-deleted

### Requirement: resolve_tasks_for_spec Wraps the Resolve-By-Spec Endpoint

The system MUST expose `resolve_tasks_for_spec(spec_change_name)` as an MCP tool that calls `POST /v1/tasks/resolve-by-spec`, intended for invocation by the sdd-verify/sdd-archive flow, and MUST return a summary of how many tasks were transitioned.

#### Scenario: resolve_tasks_for_spec reports transition count

- GIVEN two tasks are linked to change name `"team-tasks"`
- WHEN the agent calls `resolve_tasks_for_spec` with `spec_change_name: "team-tasks"`
- THEN both tasks transition to `done` on the backend
- AND the tool's text response states that 2 tasks were resolved

### Requirement: SessionStart Hook Injects a Pending-Task Reminder

The system MUST provide a SessionStart hook that fetches the current user's pending-task count for the active project and injects a text reminder ("You have N pending tasks in <project>") into the session context, and MUST fetch and format this text itself rather than relying on the agent to call a tool, since hooks cannot force tool calls.

#### Scenario: Hook injects reminder when pending tasks exist

- GIVEN the current API key's user has 3 tasks with a non-terminal status in the active project
- WHEN a new session starts
- THEN the hook fetches the pending count directly
- AND injects the text "You have 3 pending tasks in <project>" into session context

#### Scenario: Hook counts only the current user's tasks in the current project

- GIVEN the current user has 2 pending tasks in project A and 5 pending tasks in project B
- AND the active project for the session is project A
- WHEN the SessionStart hook runs
- THEN the injected reminder reflects only the 2 pending tasks in project A
- AND does not include the 5 pending tasks from project B

#### Scenario: Hook is silent when there are no pending tasks

- GIVEN the current user has zero tasks with a non-terminal status in the active project
- WHEN a new session starts
- THEN the hook does not inject a pending-task reminder line

#### Scenario: Hook failure does not block session start

- GIVEN the backend is unreachable when the SessionStart hook runs
- WHEN the hook attempts to fetch the pending-task count
- THEN the session still starts successfully
- AND the hook omits the reminder rather than failing session startup

### Requirement: Pending Count Excludes Terminal Statuses

The system MUST define "pending" for reminder and count purposes as any task status other than `done` or `cancelled`.

#### Scenario: Done and cancelled tasks are excluded from the pending count

- GIVEN the current user is assigned to one `done` task, one `cancelled` task, and one `in_progress` task in the active project
- WHEN the pending count is computed (by the hook or by `list_my_tasks`)
- THEN the count reflects only the `in_progress` task
