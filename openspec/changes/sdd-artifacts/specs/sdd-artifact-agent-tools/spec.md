# Delta for SDD Artifact Agent Tools

## ADDED Requirements

### Requirement: The SDD Tools Are Thin Permissioned Wrappers Over the SDD API

The system MUST expose exactly seven MCP tools — `save_sdd_artifact`, `get_sdd_artifact`, `list_sdd_changes`, `get_sdd_change`, `update_sdd_change`, `search_sdd_artifacts`, `link_sdd_change_memory` — as thin wrappers over the SDD backend routes. Each tool MUST inherit the permission gate of its underlying endpoint and MUST add no authority beyond the calling API key's existing `sdd:*` grants. Each tool MUST return a human-readable text confirmation of what happened, and MUST surface a backend rejection as a tool failure rather than reporting success.

#### Scenario: save_sdd_artifact enforces sdd:write

- GIVEN the calling API key resolves to a user who lacks `sdd:write`
- WHEN the agent calls `save_sdd_artifact`
- THEN the tool call fails
- AND no change, artifact, or revision is created on the backend

#### Scenario: search_sdd_artifacts enforces sdd:read

- GIVEN the calling API key resolves to a user who lacks `sdd:read`
- WHEN the agent calls `search_sdd_artifacts`
- THEN the tool call fails
- AND no artifact content or metadata is returned to the agent

#### Scenario: A read-only caller can read but not write

- GIVEN the calling API key holds `sdd:read` but not `sdd:write`
- WHEN the agent calls `get_sdd_artifact` and then `save_sdd_artifact`
- THEN the read succeeds
- AND the save fails with a permission error

#### Scenario: The tools grant no authority the API does not

- GIVEN an artifact belongs to an organization the calling key does not belong to
- WHEN the agent calls `get_sdd_artifact` for it
- THEN the tool reports not-found
- AND MUST NOT return the artifact, exactly as the REST endpoint would not

### Requirement: save_sdd_artifact Is Idempotent and Reports Whether a Revision Was Created

The system MUST make `save_sdd_artifact(project, change_name, kind, content, capability?, path?, git_commit?)` idempotent by content hash, so a skill MAY call it unconditionally on every phase run without generating revision churn. The tool MUST create the change if it does not exist, MUST report in its response whether a new revision was created and which revision number is now current, and MUST fail without writing anything when content exceeds the 1 MB cap.

#### Scenario: Re-saving identical content creates no revision

- GIVEN `design.md` was saved and the artifact is at revision 2
- WHEN a skill re-runs and calls `save_sdd_artifact` with byte-identical content
- THEN no new revision is created on the backend
- AND the tool response states that the content was unchanged and the artifact remains at revision 2

#### Scenario: Saving edited content appends a revision

- GIVEN `design.md` is at revision 2
- WHEN a skill calls `save_sdd_artifact` with edited content
- THEN revision 3 is created
- AND the tool response states that revision 3 was created

#### Scenario: Saving for an unknown change creates the change

- GIVEN no change named `"new-thing"` exists
- WHEN a skill calls `save_sdd_artifact` for `(nexus-mind, new-thing, proposal)`
- THEN the change and the artifact are created together
- AND the tool response identifies the created change

#### Scenario: Oversized content fails the tool call and writes nothing

- GIVEN a skill produces artifact content larger than 1 MB
- WHEN it calls `save_sdd_artifact`
- THEN the tool call fails with a size error
- AND no change, artifact, or revision is created on the backend

#### Scenario: A spec artifact is saved per capability

- GIVEN a skill writes two capability specs for one change
- WHEN it calls `save_sdd_artifact` twice with `kind: "spec"` and two distinct `capability` values
- THEN two distinct artifacts exist under that change
- AND neither save overwrites the other

### Requirement: get_sdd_artifact Returns the Full Document, Never a Preview

The system MUST have `get_sdd_artifact` return the complete, untruncated content of the requested artifact revision. It MUST NOT return a snippet, preview, summary, or otherwise elided form of the content. The tool MUST be addressable both by artifact id and by `(project, change_name, kind, capability?)`, MUST default to the latest revision, MUST accept an explicit revision number, and MUST return a structured not-found result — never an empty or partial document — when the artifact does not exist.

#### Scenario: A large design document is returned in full

- GIVEN a `design` artifact whose latest revision is 36 KB of markdown
- WHEN a sub-agent calls `get_sdd_artifact` for it
- THEN the returned content is byte-identical to the content that was saved
- AND it is not truncated, ellipsized, or summarized

#### Scenario: Addressable by change name and kind

- GIVEN the caller knows only the change name and the artifact kind
- WHEN it calls `get_sdd_artifact(project, change_name, kind)` with no artifact id
- THEN the artifact's latest revision content is returned

#### Scenario: An explicit revision returns that revision's full content

- GIVEN an artifact has revisions 1 through 3
- WHEN the caller requests revision 2 explicitly
- THEN the full content of revision 2 is returned
- AND revision 3's content is not returned

#### Scenario: A missing artifact reports not-found, not an empty document

- GIVEN a change has no `design` artifact
- WHEN a sub-agent calls `get_sdd_artifact` for `kind: "design"` on that change
- THEN the tool reports that the artifact does not exist
- AND MUST NOT return an empty string that a caller could mistake for an empty design

### Requirement: list_sdd_changes Reports Change Inventory Without Content

The system MUST have `list_sdd_changes` return change metadata — name, title, project, phase, status, sprint, timestamps — filterable by `project`, `status`, `phase`, and `sprint_id`, and MUST NOT return artifact content in the listing.

#### Scenario: Listing changes for a project

- GIVEN five changes exist across two projects
- WHEN an agent calls `list_sdd_changes` with `project: "nexus-mind"`
- THEN only that project's changes are returned, each with its phase and status

#### Scenario: The listing contains no artifact content

- GIVEN a listed change has a 36 KB design artifact
- WHEN the agent calls `list_sdd_changes`
- THEN the tool response MUST NOT include the design's markdown

#### Scenario: Filtering by phase

- GIVEN changes exist in phases `spec`, `design`, and `apply`
- WHEN an agent calls `list_sdd_changes` with `phase: "design"`
- THEN only changes in the `design` phase are returned

### Requirement: get_sdd_change Returns the Artifact Inventory as Recoverable State

The system MUST have `get_sdd_change` return the change together with its artifact inventory (each artifact's kind, capability, path, and latest revision number), its linked tasks, and its linked memories — sufficient for an agent to resume a change with no local checkout. The inventory MUST reflect the artifacts that actually exist, independently of the change's advisory `phase` value.

#### Scenario: A fresh session recovers a change with no checkout

- GIVEN an agent starts on a machine with no clone of the repository
- WHEN it calls `get_sdd_change` for a known change
- THEN it receives the change's phase, status, artifact inventory, linked tasks, and linked memories
- AND it can then fetch any artifact's full content via `get_sdd_artifact`

#### Scenario: The inventory contradicts a stale phase and the inventory wins

- GIVEN a change's `phase` is `spec` but a `design` artifact exists
- WHEN an agent calls `get_sdd_change`
- THEN the inventory lists the `design` artifact
- AND an agent resuming the change can determine that the design step already produced an artifact

#### Scenario: The inventory omits content

- GIVEN a change has four artifacts totalling 90 KB
- WHEN an agent calls `get_sdd_change`
- THEN the response lists all four artifacts with their kinds and latest revisions
- AND MUST NOT inline their content

### Requirement: update_sdd_change Performs Phase and Status Transitions

The system MUST have `update_sdd_change` patch a change's `phase`, `status`, `title`, and `sprint_id` under `sdd:write`, MUST reject an invalid `phase` or `status` value without applying any part of the update, and MUST report not-found for a change the caller cannot see.

#### Scenario: Advance a change to the apply phase

- GIVEN a change is in phase `tasks` and the caller holds `sdd:write`
- WHEN the agent calls `update_sdd_change` with `phase: "apply"`
- THEN the change's phase becomes `apply`
- AND the tool confirms the transition

#### Scenario: Transition denied without sdd:write

- GIVEN the calling key holds `sdd:read` only
- WHEN the agent calls `update_sdd_change`
- THEN the tool call fails
- AND the change is unmodified

#### Scenario: An invalid phase value is rejected atomically

- GIVEN the agent calls `update_sdd_change` with `phase: "shipped"` and a valid new `title`
- WHEN the tool handler executes
- THEN the call fails with a validation error
- AND neither the phase nor the title is changed

#### Scenario: Unknown change reports not-found

- GIVEN a change id or name that does not exist in the caller's organization
- WHEN the agent calls `update_sdd_change` for it
- THEN the tool reports not-found
- AND no change is created as a side effect

### Requirement: search_sdd_artifacts Searches Every Change in the Organization

The system MUST have `search_sdd_artifacts(query, limit?)` run a full-text search across the latest revision of every artifact in the caller's organization, returning snippets plus the change name, kind, and capability needed to fetch the full document. Results MUST be scoped to the caller's organization and gated by `sdd:read`.

#### Scenario: Find the spec that covers a topic

- GIVEN a capability spec mentions rate limiting
- WHEN an agent calls `search_sdd_artifacts` with `query: "rate limiting"`
- THEN the spec artifact is returned with a snippet and its change name, kind, and capability
- AND the agent can pass those identifiers to `get_sdd_artifact` to obtain the full text

#### Scenario: Search spans changes, not just the current one

- GIVEN matching artifacts exist under three different changes
- WHEN an agent searches for a term common to all three
- THEN artifacts from all three changes are returned

#### Scenario: Results honour the limit

- GIVEN twenty artifacts match the query
- WHEN an agent calls `search_sdd_artifacts` with `limit: 5`
- THEN at most five results are returned

### Requirement: link_sdd_change_memory Ties Decisions Back to the Change

The system MUST have `link_sdd_change_memory(change, memory_id, relation?)` create the change↔memory link under `sdd:write`, so that `sdd-apply` and `sdd-verify` can attach the decisions, bugfixes, and discoveries they record back to the change that produced them. The call MUST be idempotent for a `(change, memory)` pair and MUST fail without writing when the memory is not visible to the caller.

#### Scenario: sdd-apply links a decision it recorded

- GIVEN `sdd-apply` stored a decision memory while implementing a change
- WHEN it calls `link_sdd_change_memory` with that memory and `relation: "produced"`
- THEN the link is created
- AND the memory appears among the change's linked memories

#### Scenario: Re-linking the same memory is a no-op

- GIVEN a change is already linked to a memory
- WHEN the agent calls `link_sdd_change_memory` for the same pair again
- THEN the call succeeds
- AND no duplicate link is created

#### Scenario: Linking an invisible memory fails without writing

- GIVEN a memory id that does not exist in the caller's organization
- WHEN the agent calls `link_sdd_change_memory` with it
- THEN the tool reports not-found
- AND no link is created
