# Team Tasks Spec Links Specification

## Purpose

Connect tasks to the openspec changes that resolve them via a many-to-many link keyed by the change's kebab-case folder name, validated against the active and archived openspec trees, and auto-resolve linked tasks to `done` when triggered by the openspec verify/archive flow.

## Requirements

### Requirement: A Task Links to One or More Openspec Change Names

The system MUST support a many-to-many relationship between tasks and openspec change folder names (no database representation of the spec itself — only the string is stored), MUST allow a task to link to multiple change names, and MUST allow a single change name to be linked from multiple tasks.

#### Scenario: Link a task to a spec change

- GIVEN a task exists
- AND an openspec change folder `team-tasks` exists under `openspec/changes/`
- WHEN a caller with `task:write` links the task to `"team-tasks"`
- THEN the task's spec-link list includes `"team-tasks"`

#### Scenario: Link one task to multiple changes

- GIVEN a task exists
- WHEN a caller links the task to two distinct valid change names
- THEN the task's spec-link list contains both change names

#### Scenario: Link multiple tasks to the same change

- GIVEN two separate tasks exist
- WHEN a caller links both tasks to the same change name
- THEN both tasks independently show that change name in their spec-link list

### Requirement: Link Creation Validates the Change Name Against the Openspec Trees

The system MUST validate a spec-link's change name against the active tree (`openspec/changes/<name>/`) and the archived tree (`openspec/changes/archive/<date>-<name>/`) at link-creation time, and MUST reject linking to a name that matches neither.

#### Scenario: Link to an active change succeeds

- GIVEN `openspec/changes/harness-agent-tools/` exists on disk
- WHEN a caller links a task to `"harness-agent-tools"`
- THEN the link is created

#### Scenario: Link to an archived change succeeds

- GIVEN `openspec/changes/archive/2026-01-15-old-change/` exists on disk
- WHEN a caller links a task to `"old-change"`
- THEN the system resolves the match against the archived tree
- AND the link is created

#### Scenario: Reject linking to a non-existent change name

- GIVEN no folder matching a given name exists in either the active or archived openspec tree
- WHEN a caller attempts to link a task to that name
- THEN the system MUST reject the request with a 4xx validation error
- AND MUST NOT create the link

### Requirement: Auto-Resolve Transitions Linked Tasks to Done on Trigger

The system MUST expose a `POST /v1/tasks/resolve-by-spec` endpoint accepting `{ spec_change_name }` that transitions every task linked to that change name to `status: "done"`, and MUST NOT resolve tasks automatically on any filesystem event — resolution only occurs when this endpoint (or the MCP tool wrapping it) is explicitly invoked, typically by the sdd-verify/sdd-archive flow.

#### Scenario: Resolve-by-spec transitions all linked tasks

- GIVEN three tasks are each linked to the change name `"team-tasks"`
- AND none of them currently has status `done` or `cancelled`
- WHEN `resolve-by-spec` is called with `spec_change_name: "team-tasks"`
- THEN all three tasks transition to `status: "done"`
- AND the response reports the count and ids of tasks transitioned

#### Scenario: Resolve-by-spec is a no-op for an unlinked change name

- GIVEN no task is linked to the change name `"unrelated-change"`
- WHEN `resolve-by-spec` is called with `spec_change_name: "unrelated-change"`
- THEN the system returns success with zero tasks transitioned
- AND no task is modified

#### Scenario: Resolve-by-spec skips already-terminal tasks

- GIVEN a task linked to `"team-tasks"` already has status `cancelled`
- WHEN `resolve-by-spec` is called with `spec_change_name: "team-tasks"`
- THEN that task's status remains `cancelled`
- AND it is not counted among the transitioned tasks

#### Scenario: Resolve-by-spec requires write authority, not caller project membership

- GIVEN the resolve-by-spec caller is an authenticated service/agent identity invoking on behalf of the openspec flow
- WHEN it calls `resolve-by-spec` for a change name whose linked tasks span multiple projects
- THEN tasks are transitioned across all applicable projects
- AND the operation is not blocked by per-project membership scoping

### Requirement: Dangling Links Are Tolerated and Surfaced, Not Blocked

The system MUST tolerate a spec-link whose change name no longer matches any folder (e.g., after a rename) without blocking reads of the task, and MUST allow the link to be surfaced as-is rather than silently dropped.

#### Scenario: Read a task with a dangling spec link

- GIVEN a task was linked to `"old-name"` before that change folder was renamed
- WHEN a caller reads the task
- THEN the response still includes `"old-name"` in the spec-link list
- AND the read succeeds without error

### Requirement: Removing a Spec Link Requires Write Permission

The system MUST require `task:write` for the task's project to remove a spec link, matching the permission required to add one.

#### Scenario: Remove a spec link with write permission

- GIVEN a task is linked to a change name
- WHEN a caller with `task:write` removes that link
- THEN the task's spec-link list no longer includes that change name

#### Scenario: Remove spec link denied without write permission

- GIVEN a caller lacks `task:write` for the task's project
- WHEN they attempt to remove a spec link from a task
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT remove the link
