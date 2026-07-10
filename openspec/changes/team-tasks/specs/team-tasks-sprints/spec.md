# Team Tasks Sprints Specification

## Purpose

Group tasks into time-boxed sprints and capture end-of-sprint retrospectives, gated behind `task:manage`, reconciled with the existing `create_sprint_retrospective` MCP tool rather than duplicating it.

## Requirements

### Requirement: Sprint Administration Requires Manage Permission

The system MUST require `task:manage` for the project to create, update, or close a sprint, and MUST scope sprints to a single project.

#### Scenario: Create a sprint with manage permission

- GIVEN a caller holds `task:manage` for a project
- WHEN they create a sprint with a name, start date, and end date
- THEN the sprint is created and scoped to that project

#### Scenario: Sprint creation denied without manage permission

- GIVEN a caller holds `task:write` but not `task:manage` for a project
- WHEN they attempt to create a sprint in that project
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT create the sprint

### Requirement: Tasks Can Be Assigned to a Sprint

The system MUST allow adding a task to a sprint (a simple grouping association, no burndown or velocity computation in v1), MUST require the task and sprint to belong to the same project, and MUST allow a task to belong to at most one active sprint at a time.

#### Scenario: Add a task to a sprint

- GIVEN a sprint and a task exist in the same project
- WHEN a caller with `task:write` assigns the task to the sprint
- THEN the sprint's task list includes that task

#### Scenario: Reject adding a task to a sprint in a different project

- GIVEN a sprint belongs to project X
- AND a task belongs to project Y
- WHEN a caller attempts to assign that task to that sprint
- THEN the system MUST reject the request with a 4xx validation error

#### Scenario: Moving a task to a new sprint removes it from the prior one

- GIVEN a task is currently assigned to sprint A
- WHEN a caller assigns that task to sprint B
- THEN the task no longer appears in sprint A's task list
- AND the task appears in sprint B's task list

### Requirement: Sprint Retrospectives Reconcile With the Existing MCP Tool

The system MUST provide a backend-backed sprint retrospective capability (capturing what went well, what didn't, and action items per sprint) that the existing `create_sprint_retrospective` MCP tool is wired to, MUST NOT introduce a second, parallel retrospective code path, and MUST require `task:manage` to create or edit a retrospective.

#### Scenario: Create a retrospective for a closed sprint

- GIVEN a sprint exists and has ended
- WHEN a caller with `task:manage` creates a retrospective with went-well, went-poorly, and action-item entries
- THEN the retrospective is persisted and associated with that sprint

#### Scenario: create_sprint_retrospective tool call persists through the backend

- GIVEN the MCP tool `create_sprint_retrospective` is invoked with a valid sprint reference and retrospective content
- WHEN the call completes
- THEN the retrospective is retrievable via the backend sprint-retrospective read path
- AND no separate, unbacked client-side-only retrospective record is created

#### Scenario: Retrospective creation denied without manage permission

- GIVEN a caller holds `task:write` but not `task:manage`
- WHEN they attempt to create a retrospective for a sprint
- THEN the system MUST respond with 403 Forbidden

### Requirement: Sprint Reads Are Scoped to Project Membership

The system MUST scope sprint and retrospective reads to project members, returning 404 for non-members, consistent with the task existence-leak rule.

#### Scenario: Non-member cannot read sprint or retrospective data

- GIVEN a caller is not a member of a project that has sprints
- WHEN they attempt to list sprints or fetch a retrospective for that project
- THEN the system MUST respond with 404 Not Found
