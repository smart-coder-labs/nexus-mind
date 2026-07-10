# Team Tasks Core Specification

## Purpose

Provide the core task entity and its CRUD lifecycle — title, description, status, priority, due date, project scoping, soft-delete, and permission gating — as the foundation every other task capability builds on.

## Requirements

### Requirement: Fixed Task Status Set

The system MUST constrain every task's `status` field to exactly one of the following values: `backlog`, `todo`, `in_progress`, `in_review`, `done`, `cancelled`. Custom or per-organization statuses are not supported in v1.

#### Scenario: Create task with a valid status

- GIVEN a caller holds `task:write` for a project
- WHEN they create a task with `status: "todo"`
- THEN the task is created with `status` set to `"todo"`

#### Scenario: Reject an unrecognized status value

- GIVEN a caller holds `task:write` for a project
- WHEN they submit a task create or update request with a `status` value outside the fixed set
- THEN the system MUST reject the request with a 4xx validation error
- AND MUST NOT create or modify the task

### Requirement: Task Creation Requires Write Permission

The system MUST allow creating a task only when the caller holds `task:write` for the task's `project`, and MUST persist `title`, `description`, `status` (defaulting to `backlog` when omitted), `priority`, `due_date`, `project`, `created_by`, and creation/update timestamps.

#### Scenario: Create a task with minimal fields

- GIVEN a caller holds `task:write` for a project they belong to
- WHEN they submit a create-task request with only a `title` and `project`
- THEN the task is created with `status` defaulted to `backlog`
- AND `created_by` is set to the caller's user id
- AND creation and update timestamps are populated

#### Scenario: Create task denied without write permission

- GIVEN a caller does not hold `task:write` for the target project
- WHEN they attempt to create a task in that project
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT create the task

### Requirement: Task Reads Are Scoped to Project Membership

The system MUST scope task list and get operations to projects the caller belongs to (via `visibility_predicate`), and MUST return 404 Not Found — never 403 — when a non-member requests a task or task list outside their visibility, to avoid leaking project or task existence.

#### Scenario: Member reads tasks in their project

- GIVEN a caller is a member of a project with existing tasks
- WHEN they call get-task or list-tasks for that project
- THEN the system returns the task(s) belonging to that project

#### Scenario: Non-member read returns 404, not 403

- GIVEN a caller is not a member of a project that has a task
- WHEN they attempt to read that specific task by id, or list tasks scoped to that project
- THEN the system MUST respond with 404 Not Found
- AND MUST NOT reveal whether the task or project exists

### Requirement: Task Updates Require Write Permission and Validate Status Transitions

The system MUST allow updating a task's mutable fields (`title`, `description`, `status`, `priority`, `due_date`) only when the caller holds `task:write` for the task's project, and MUST reject updates to a `cancelled` or `done` task's status back to `backlog` or `todo` unless explicitly reopened via a supported transition.

#### Scenario: Update task fields with write permission

- GIVEN a caller holds `task:write` for the task's project
- WHEN they update the task's `priority` and `due_date`
- THEN the changes are persisted
- AND the update timestamp is refreshed

#### Scenario: Update denied without write permission

- GIVEN a caller lacks `task:write` for the task's project
- WHEN they attempt to update any field on a task in that project
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT modify the task

#### Scenario: Status transition to an invalid target state is rejected

- GIVEN a task currently has `status: "done"`
- WHEN a caller attempts to set `status` directly to a value not permitted as a transition from `done` (per the system's status-transition rules)
- THEN the system MUST reject the update with a 4xx validation error
- AND MUST leave the task's status unchanged

### Requirement: Task Deletion Is a Soft-Delete Gated by Delete Permission

The system MUST implement task deletion as a soft-delete (setting `archived_at`), MUST require `task:delete` for the task's project, and MUST exclude archived tasks from default list/get results while preserving them for audit and spec-link auto-resolve history.

#### Scenario: Soft-delete a task with delete permission

- GIVEN a caller holds `task:delete` for the task's project
- WHEN they delete the task
- THEN the task's `archived_at` is set to the current timestamp
- AND the task no longer appears in default list results

#### Scenario: Delete denied without delete permission

- GIVEN a caller holds `task:write` but not `task:delete` for the task's project
- WHEN they attempt to delete a task
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT set `archived_at`

#### Scenario: Delete a non-existent or already-archived task

- GIVEN a task id that does not exist, or belongs to a project the caller cannot see
- WHEN a caller attempts to delete it
- THEN the system MUST respond with 404 Not Found

### Requirement: Task Listing Supports Filtering and Pagination

The system MUST support filtering the task list by `project`, `status`, and `priority`, and MUST paginate results with a `count_*` total kept consistent with the filtered result set.

#### Scenario: List tasks filtered by status

- GIVEN a project has tasks in multiple statuses
- WHEN a member lists tasks filtered by `status: "in_progress"`
- THEN only tasks with `status: "in_progress"` in that project are returned

#### Scenario: Paginated list reports an accurate total

- GIVEN a project has more tasks than fit on one page
- WHEN a member lists tasks with a page size smaller than the total
- THEN the returned page contains at most the requested page size
- AND the reported total count matches the full filtered result set
