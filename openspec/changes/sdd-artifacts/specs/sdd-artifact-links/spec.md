# Delta for SDD Artifact Links

## MODIFIED Requirements

### Requirement: Link Creation Validates the Change Name Against the Openspec Trees

The system MUST validate a spec-link's change name against the **SDD change store first** — a name matching an `sdd_changes` record in the caller's organization is valid — and MUST fall back to the filesystem check (active tree `openspec/changes/<name>/`, archived tree `openspec/changes/archive/<date>-<name>/`) only when the store has no match. The system MUST reject linking to a name that matches neither. The filesystem fallback retains its permissive behaviour when the repository root is unreadable, so a backend running inside a checkout keeps working; but a deployed backend with no checkout now has a real referent, and a name that exists in neither store nor filesystem MUST be rejected with 422 instead of being silently accepted.

(Previously: validation read the local filesystem only. On a deployed backend no `openspec/` directory exists, the root is unreadable, and the check returned `true` for every input — any string linked successfully. This requirement makes the check real in production. It is a behaviour change on the shipped `POST /v1/tasks/:id/spec-links` endpoint.)

#### Scenario: Link to a change known to the SDD store succeeds without touching the filesystem

- GIVEN an `sdd_changes` record named `"sdd-artifacts"` exists in the caller's organization
- AND no `openspec/` directory is present on the backend host
- WHEN a caller with `task:write` links a task to `"sdd-artifacts"`
- THEN the link is created
- AND validation is satisfied by the store, without requiring a filesystem read

#### Scenario: Link to an on-disk change not yet in the store succeeds via fallback

- GIVEN no `sdd_changes` record named `"local-only"` exists
- AND `openspec/changes/local-only/` exists on the backend host's filesystem
- WHEN a caller links a task to `"local-only"`
- THEN the filesystem fallback resolves the name
- AND the link is created

#### Scenario: Link to an archived on-disk change succeeds via fallback

- GIVEN no `sdd_changes` record named `"old-change"` exists
- AND `openspec/changes/archive/2026-01-15-old-change/` exists on the backend host
- WHEN a caller links a task to `"old-change"`
- THEN the fallback resolves the match against the archived tree
- AND the link is created

#### Scenario: A typo'd change name is now rejected in production

- GIVEN a deployed backend with no readable repository root
- AND no `sdd_changes` record named `"does-not-exist"` exists in the caller's organization
- WHEN a caller attempts to link a task to `"does-not-exist"`
- THEN the system MUST respond with 422
- AND MUST NOT create the link
- AND this MUST NOT return 201 as it did before this change

#### Scenario: Validation is org-scoped

- GIVEN an `sdd_changes` record named `"secret-change"` exists in organization B
- AND the caller belongs to organization A
- AND the backend has no readable repository root
- WHEN the caller attempts to link a task to `"secret-change"`
- THEN the store lookup MUST NOT match organization B's record
- AND the system MUST respond with 422
- AND MUST NOT reveal that the name exists in another organization

#### Scenario: Existing valid links keep working after the change

- GIVEN a task was linked to a change name that exists on disk in a local checkout
- WHEN validation runs after the DB-first change is deployed to that local backend
- THEN the same name still validates via the filesystem fallback
- AND no previously-working link becomes invalid

## ADDED Requirements

### Requirement: Tasks Join to Changes by Name, Not by a Foreign Key

The system MUST continue to key the task↔change edge on the change's kebab-case `spec_change_name` string in `task_spec_links`, and MUST NOT introduce a parallel `change_id` foreign key on tasks. A link created before the change exists in the SDD store MUST resolve automatically once a matching change record appears, with no re-linking required.

#### Scenario: A link created before the change existed resolves once the change appears

- GIVEN a task is linked to `"sdd-artifacts"` while no `sdd_changes` record of that name exists
- WHEN a change named `"sdd-artifacts"` is later created in the same organization and project
- THEN fetching that change's linked tasks returns the task
- AND the task's spec-link list is unchanged and required no re-linking

#### Scenario: The link survives with no duplicate source of truth

- GIVEN a task is linked to a change
- WHEN the task and the change are each read
- THEN the edge is represented only by the change name in `task_spec_links`
- AND no `change_id` field is present on the task

### Requirement: A Change Exposes the Tasks Linked to It

The system MUST expose the tasks linked to a change, joining `task_spec_links` on the change's `name` within the caller's organization. The read MUST require both `sdd:read` and `task:read`, MUST apply the task visibility rules (tasks in projects the caller cannot see are excluded), and MUST return an empty list — not an error — when the change has no linked tasks.

#### Scenario: Linked tasks are returned for a change

- GIVEN three tasks are linked to the change name `"sdd-artifacts"`
- AND the caller holds `sdd:read` and `task:read` and can see all three tasks' projects
- WHEN the caller fetches the change's linked tasks
- THEN all three tasks are returned with their status and title

#### Scenario: Tasks the caller cannot see are excluded

- GIVEN two tasks are linked to a change, one in a project the caller is not a member of
- WHEN the caller fetches the change's linked tasks
- THEN only the visible task is returned
- AND the response MUST NOT reveal that another linked task exists

#### Scenario: Linked-tasks read denied without task:read

- GIVEN a caller holds `sdd:read` but no `task:read` grant
- WHEN they fetch a change's linked tasks
- THEN the system MUST respond with 403 Forbidden

#### Scenario: A change with no linked tasks returns an empty list

- GIVEN a change has no `task_spec_links` entries matching its name
- WHEN a caller fetches its linked tasks
- THEN the system returns success with an empty list

### Requirement: Changes Link to Memories Many-to-Many With a Relation

The system MUST support a many-to-many link between a change and memories, carrying a `relation` of `produced` or `informed`, recording the linking user. The pair `(change, memory)` MUST be unique: re-linking the same pair MUST NOT create a duplicate row. Creating and removing a link MUST require `sdd:write`. A memory outside the caller's organization MUST NOT be linkable, and the attempt MUST return 404 rather than reveal the memory's existence. Deleting a change or a memory MUST remove the link, not orphan it.

#### Scenario: Link a memory produced by a change

- GIVEN a caller with `sdd:write` has a change and a memory in the same organization
- WHEN they link the memory to the change with `relation: "produced"`
- THEN the link is created
- AND fetching the change returns that memory among its linked memories

#### Scenario: Re-linking the same pair creates no duplicate

- GIVEN a change is already linked to a memory
- WHEN a caller links the same change and memory again
- THEN the system MUST NOT create a second link row
- AND the change still reports exactly one link to that memory

#### Scenario: Cross-org memory link returns 404

- GIVEN a memory exists in organization B
- AND the caller belongs to organization A with `sdd:write`
- WHEN they attempt to link that memory to a change in organization A
- THEN the system MUST respond with 404 Not Found
- AND MUST NOT create the link
- AND MUST NOT reveal whether the memory id is valid

#### Scenario: Memory link denied without sdd:write

- GIVEN a caller holds `sdd:read` only
- WHEN they attempt to link or unlink a memory on a change
- THEN the system MUST respond with 403 Forbidden
- AND the change's memory links MUST be unchanged

#### Scenario: Deleting the memory removes the link

- GIVEN a change is linked to a memory
- WHEN that memory is deleted
- THEN the link no longer appears among the change's linked memories
- AND fetching the change still succeeds

#### Scenario: Unlink a memory

- GIVEN a change is linked to a memory
- WHEN a caller with `sdd:write` removes that link
- THEN the memory no longer appears among the change's linked memories
- AND the memory itself is not deleted

### Requirement: A Change Belongs to One Project and Optionally One Sprint

The system MUST associate every change with exactly one project (by name string) and MUST allow associating a change with at most one sprint. The sprint association MUST be optional and MUST be filterable, so that "what are we speccing this sprint" is answerable. Deleting a sprint MUST clear the association without deleting the change.

#### Scenario: Assign a change to a sprint

- GIVEN a change and a sprint exist in the caller's organization
- WHEN a caller with `sdd:write` patches the change's `sprint_id` to that sprint
- THEN the change reports that sprint

#### Scenario: List the changes in a sprint

- GIVEN three changes are assigned to a sprint and two are not
- WHEN a caller lists changes filtered by that `sprint_id`
- THEN exactly the three assigned changes are returned

#### Scenario: Deleting the sprint leaves the change intact

- GIVEN a change is assigned to a sprint
- WHEN that sprint is deleted
- THEN the change still exists
- AND its sprint association is cleared rather than the change being removed

#### Scenario: A change without a sprint is valid

- GIVEN a change is created with no `sprint_id`
- WHEN it is read
- THEN it is returned with no sprint association
- AND it appears in an unfiltered change list

### Requirement: Global Search Includes an SDD Facet

The system MUST add an SDD facet to the global search result, returning matching changes alongside the existing facets. The field MUST be additive — no existing global-search field may be removed or renamed. The facet MUST be populated only for callers holding `sdd:read`; a caller without that grant MUST still receive a successful global-search response containing the facets they may see, with the SDD facet empty, rather than a 403 on the whole search.

#### Scenario: A matching change appears in the SDD facet

- GIVEN a change titled "SDD Artifacts" exists in the caller's organization
- AND the caller holds `sdd:read`
- WHEN they run a global search for "sdd artifacts"
- THEN the result's SDD facet includes that change with its name, title, project, and phase

#### Scenario: The SDD facet is empty without sdd:read

- GIVEN a caller holds no `sdd:read` grant
- WHEN they run a global search matching an existing change
- THEN the search returns 200 with its other facets populated
- AND the SDD facet is empty
- AND the response MUST NOT be a 403

#### Scenario: Existing global-search facets are unaffected

- GIVEN a global-search query that previously matched memories and tasks
- WHEN the same query is run after the SDD facet is added
- THEN the memory and task facets contain the same results as before
- AND no previously-present field is removed from the response shape

#### Scenario: The SDD facet is org-scoped

- GIVEN a matching change exists in organization B
- WHEN a caller in organization A with `sdd:read` runs global search
- THEN organization B's change MUST NOT appear in the SDD facet
