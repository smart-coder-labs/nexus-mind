# Team Tasks Assignment Specification

## Purpose

Allow a task to carry multiple assignees via a join table, gate assignment changes behind a dedicated permission, and ensure only organization members can be assigned.

## Requirements

### Requirement: A Task May Have Multiple Assignees

The system MUST support assigning zero or more users to a task via a `task_assignees` join relationship, MUST allow listing all current assignees for a task, and MUST return denormalized assignee display data (id, name, email) rather than bare user ids.

#### Scenario: Assign multiple users to a task

- GIVEN a task exists in a project
- AND two users are members of that project's organization
- WHEN a caller with `task:assign` assigns both users to the task
- THEN the task's assignee list contains both users
- AND each entry includes the assignee's display name and email

#### Scenario: Read assignees requires only read permission

- GIVEN a task has one or more assignees
- WHEN a caller with `task:read` (but not `task:assign`) fetches the task
- THEN the response includes the current assignee list

### Requirement: Assigning a User Requires the Assign Permission

The system MUST require `task:assign` for the task's project to add or remove an assignee, independent of whether the caller holds `task:write`.

#### Scenario: Assign denied without task:assign

- GIVEN a caller holds `task:write` but not `task:assign` for the task's project
- WHEN they attempt to assign a user to the task
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT create the assignee record

#### Scenario: Unassign denied without task:assign

- GIVEN a caller holds `task:write` but not `task:assign` for the task's project
- WHEN they attempt to remove an existing assignee from the task
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT remove the assignee record

#### Scenario: Assign succeeds with task:assign

- GIVEN a caller holds `task:assign` for the task's project
- WHEN they assign a valid org member to the task
- THEN the assignee record is created
- AND the task's assignee list reflects the new assignee

### Requirement: Assignee Must Belong to the Task's Organization

The system MUST validate that any user being assigned to a task belongs to the same organization as the task (via `user_belongs_to_org`), and MUST reject assignment attempts for users outside that organization.

#### Scenario: Reject assigning a user from another organization

- GIVEN a task belongs to organization A
- AND a target user belongs only to organization B
- WHEN a caller with `task:assign` attempts to assign that user to the task
- THEN the system MUST reject the request with a 4xx validation error
- AND MUST NOT create the assignee record

#### Scenario: Reject assigning a non-existent user

- GIVEN a target user id does not correspond to any existing user
- WHEN a caller with `task:assign` attempts to assign that id to a task
- THEN the system MUST reject the request with a 4xx validation error
- AND MUST NOT create the assignee record

### Requirement: Duplicate Assignment Is Idempotent, Not Erroring

The system MUST treat assigning an already-assigned user as a no-op success rather than creating a duplicate join row or returning an error.

#### Scenario: Re-assign an already-assigned user

- GIVEN a user is already assigned to a task
- WHEN a caller with `task:assign` assigns that same user to the same task again
- THEN the system returns success
- AND the assignee list contains exactly one entry for that user
