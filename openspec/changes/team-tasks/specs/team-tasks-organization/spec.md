# Team Tasks Organization Specification

## Purpose

Let teams structure work with labels/tags for cross-cutting categorization and subtasks for parent/child decomposition within a task tree.

## Requirements

### Requirement: Labels Attach To and Detach From a Task

The system MUST allow attaching and removing free-text labels/tags on a task, gated by `task:write` for the task's project, and MUST support listing tasks filtered by one or more labels.

#### Scenario: Attach a label to a task

- GIVEN a caller holds `task:write` for a task's project
- WHEN they attach the label `"backend"` to the task
- THEN the task's label list includes `"backend"`

#### Scenario: Remove a label from a task

- GIVEN a task currently has the label `"backend"`
- WHEN a caller with `task:write` removes that label
- THEN the task's label list no longer includes `"backend"`

#### Scenario: Filter task list by label

- GIVEN a project has tasks with a mix of labels
- WHEN a member lists tasks filtered by label `"backend"`
- THEN only tasks carrying that label are returned

#### Scenario: Label write denied without permission

- GIVEN a caller lacks `task:write` for the task's project
- WHEN they attempt to attach or remove a label
- THEN the system MUST respond with 403 Forbidden

### Requirement: Subtasks Form a One-Level Parent/Child Hierarchy

The system MUST support marking a task as a subtask of another task via a `parent_id` reference, MUST restrict this hierarchy to a single level (a subtask cannot itself have subtasks), and MUST require both tasks to belong to the same project.

#### Scenario: Create a subtask under a parent

- GIVEN a parent task exists in a project
- WHEN a caller with `task:write` creates a new task with that task's id as `parent_id`
- THEN the new task is created as a subtask
- AND the parent task's subtask list includes the new task

#### Scenario: Reject nesting a subtask under a subtask

- GIVEN task B is already a subtask of task A
- WHEN a caller attempts to create task C with `parent_id` set to task B's id
- THEN the system MUST reject the request with a 4xx validation error
- AND MUST NOT create task C as a subtask of task B

#### Scenario: Reject cross-project parent/child

- GIVEN a parent task exists in project X
- WHEN a caller attempts to create a subtask referencing that parent while specifying a different project Y
- THEN the system MUST reject the request with a 4xx validation error

### Requirement: Deleting a Parent Task Preserves Subtask Integrity

The system MUST NOT cascade-delete subtasks when a parent task is soft-deleted, and MUST clear or surface the dangling `parent_id` reference so subtasks remain independently addressable.

#### Scenario: Soft-delete a parent with existing subtasks

- GIVEN a parent task has one or more subtasks
- WHEN a caller with `task:delete` soft-deletes the parent task
- THEN the parent task's `archived_at` is set
- AND the subtasks remain readable and are not themselves archived
- AND the subtasks' relationship to the archived parent remains visible in their data

#### Scenario: Subtask status is independent of parent status

- GIVEN a parent task has status `in_progress`
- WHEN a caller updates a subtask's status to `done`
- THEN the subtask's status changes to `done`
- AND the parent task's status is unaffected
