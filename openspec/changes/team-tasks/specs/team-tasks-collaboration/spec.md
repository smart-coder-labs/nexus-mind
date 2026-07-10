# Team Tasks Collaboration Specification

## Purpose

Allow project members to discuss a task through threaded comments, visible to anyone who can read the task and writable by anyone with task write access.

## Requirements

### Requirement: Comments Are Created Under Write Permission

The system MUST allow adding a comment to a task when the caller holds `task:write` for the task's project, MUST persist the comment's author, body, and creation timestamp, and MUST associate the comment with exactly one task.

#### Scenario: Add a comment to a task

- GIVEN a caller holds `task:write` for a task's project
- WHEN they add a comment with a non-empty body to the task
- THEN the comment is persisted with the caller as author and a creation timestamp
- AND the task's comment list includes the new comment

#### Scenario: Comment creation denied without write permission

- GIVEN a caller lacks `task:write` for the task's project
- WHEN they attempt to add a comment to a task in that project
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT create the comment

#### Scenario: Reject an empty comment body

- GIVEN a caller holds `task:write` for a task's project
- WHEN they attempt to add a comment with an empty or whitespace-only body
- THEN the system MUST reject the request with a 4xx validation error

### Requirement: Comments Are Readable by Anyone Who Can Read the Task

The system MUST return a task's comments (ordered oldest-first) to any caller who can read the task itself, without requiring a separate comment-specific permission.

#### Scenario: List comments with read permission

- GIVEN a task has multiple comments
- WHEN a caller with `task:read` for the task's project fetches the task
- THEN the response includes the comment list in chronological order

#### Scenario: Non-member cannot read comments

- GIVEN a caller is not a member of the task's project
- WHEN they attempt to fetch the task or its comments
- THEN the system MUST respond with 404 Not Found
- AND MUST NOT return any comment content

### Requirement: Only the Comment Author or a Manager May Delete a Comment

The system MUST allow deleting a comment when the caller is the comment's original author, or when the caller holds `task:manage` for the project, and MUST reject deletion by any other caller.

#### Scenario: Author deletes their own comment

- GIVEN a caller authored a comment on a task
- WHEN they delete that comment
- THEN the comment is removed from the task's comment list

#### Scenario: Manager deletes another user's comment

- GIVEN a comment was authored by user A
- WHEN a caller holding `task:manage` for the project deletes that comment
- THEN the comment is removed from the task's comment list

#### Scenario: Non-author, non-manager deletion is denied

- GIVEN a comment was authored by user A
- WHEN user B, who is neither the author nor holds `task:manage`, attempts to delete the comment
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT remove the comment
