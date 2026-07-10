# Team Tasks Admin UI Specification

## Purpose

Give NexusMind admin users a Tasks page to list, filter, and manage tasks, wired to the task API client, routing, and permission-gated navigation, using the existing Apple design tokens and UI primitives.

## Requirements

### Requirement: Tasks Navigation Is Gated by task:read

The system MUST show the Tasks navigation entry only to users holding `task:read` for at least one project, and MUST route direct navigation to `/tasks` by an unauthorized user to the same access-denied handling used elsewhere in the admin app.

#### Scenario: Nav item visible with task:read

- GIVEN the current admin user holds `task:read` for at least one project
- WHEN they view the sidebar navigation
- THEN a "Tasks" nav item is visible

#### Scenario: Nav item hidden without task:read

- GIVEN the current admin user holds no `task:read` grant in any project
- WHEN they view the sidebar navigation
- THEN the "Tasks" nav item is not rendered

#### Scenario: Direct navigation without permission is denied

- GIVEN the current admin user lacks `task:read`
- WHEN they navigate directly to `/tasks`
- THEN they are redirected to the app's standard unauthorized/401 handling

### Requirement: Tasks Page Lists Tasks With Filtering

The system MUST render a Tasks list/board view showing title, status, priority, assignees, and due date, MUST support filtering by project, status, and label, and MUST use `useQuery` against the task API client with loading and empty states.

#### Scenario: List renders tasks for the selected project

- GIVEN the API returns a list of tasks for the selected project
- WHEN the Tasks page loads
- THEN each task's title, status, priority, assignees, and due date are displayed

#### Scenario: Empty state when no tasks match filters

- GIVEN the API returns zero tasks for the current filter selection
- WHEN the Tasks page renders
- THEN an `EmptyState` component is shown instead of an empty table/board

#### Scenario: Filtering by status updates the visible list

- GIVEN tasks exist in multiple statuses
- WHEN the user selects a status filter
- THEN only tasks matching that status are displayed
- AND the underlying query is invalidated/refetched with the new filter

### Requirement: Task Creation and Editing Use Modal Forms

The system MUST provide create and edit flows via `Modal`-based forms reusing `Select` for status/priority/assignee fields and `Badge` for status/priority pills, and MUST invalidate the task list query on successful create/update via `useMutation` + `qc.invalidateQueries`.

#### Scenario: Create a task via the modal form

- GIVEN the user has `task:write` for the target project
- WHEN they submit the create-task modal with a valid title and project
- THEN the API client's create method is called
- AND on success the task list query is invalidated and the new task appears

#### Scenario: Create action hidden without write permission

- GIVEN the user lacks `task:write` for any project they can view
- WHEN they view the Tasks page
- THEN no create-task action is rendered

#### Scenario: Edit a task's status via the modal

- GIVEN a task is displayed in the list
- AND the user has `task:write` for that task's project
- WHEN they open the edit modal and change the status
- THEN the API client's update method is called with the new status
- AND the list reflects the updated status pill after the mutation succeeds

### Requirement: Task Deletion Requires Confirmation

The system MUST require an explicit confirmation step (via `window.confirm` or an equivalent confirmation UI) before calling the delete API, MUST gate the delete action behind `task:delete`, and MUST NOT call the delete endpoint if the user cancels the confirmation.

#### Scenario: Delete confirmed proceeds

- GIVEN the user has `task:delete` for a task's project
- WHEN they click delete and confirm the prompt
- THEN the API client's delete method is called
- AND the task list query is invalidated on success

#### Scenario: Delete cancelled makes no API call

- GIVEN the user has `task:delete` for a task's project
- WHEN they click delete and cancel/dismiss the confirmation prompt
- THEN the API client's delete method is not called
- AND the task remains in the list

#### Scenario: Delete action hidden without delete permission

- GIVEN the user lacks `task:delete` for a task's project
- WHEN they view that task's row/detail
- THEN no delete action is rendered for that task

### Requirement: Task Detail Shows Comments, Labels, Subtasks, and Spec Links

The system MUST render a task detail view showing its comment thread, attached labels, subtask list, and linked openspec change names, each reusing the corresponding API client methods and existing UI primitives, consistent with the design tokens defined in `apps/admin/DESIGN.md`.

#### Scenario: Detail view renders linked spec changes

- GIVEN a task has two linked openspec change names
- WHEN the user opens that task's detail view
- THEN both change names are displayed in a spec-links section

#### Scenario: Detail view renders subtasks with their own status

- GIVEN a task has two subtasks with different statuses
- WHEN the user opens that task's detail view
- THEN each subtask is listed with its own status badge, distinct from the parent's status

#### Scenario: Adding a comment from the detail view

- GIVEN the user has `task:write` for the task's project
- WHEN they submit a non-empty comment in the detail view
- THEN the API client's add-comment method is called
- AND the comment thread is refetched/updated to include the new comment
