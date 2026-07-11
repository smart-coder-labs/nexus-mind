# Delta for SDD Artifact Store

## ADDED Requirements

### Requirement: SDD Changes Are Org-Scoped and Uniquely Keyed by Project and Name

The system MUST store an SDD change as a root entity scoped to an organization, identified by the tuple `(org_id, project, name)` where `name` is the kebab-case change folder name and `project` is a project **name string** (not a foreign key, matching `tasks.project`). The tuple MUST be unique. A change MUST carry a `status` (`active | archived | abandoned`), a `phase`, and creation/update timestamps, and MUST record the user who created it.

#### Scenario: Create a change

- GIVEN a caller holds `sdd:write`
- WHEN they create a change with `project: "nexus-mind"` and `name: "sdd-artifacts"`
- THEN the change is persisted with `status` defaulted to `active`
- AND `created_by` is set to the caller's user id
- AND creation and update timestamps are populated

#### Scenario: The same name in two projects is two changes

- GIVEN a change named `"team-tasks"` exists in project `"nexus-mind"`
- WHEN a caller creates a change named `"team-tasks"` in project `"kasymir"` within the same organization
- THEN two distinct changes exist
- AND neither overwrites the other

#### Scenario: Re-submitting the same project and name upserts, not duplicates

- GIVEN a change `(nexus-mind, sdd-artifacts)` already exists
- WHEN a caller submits a create request for the same `(project, name)` with a new `title`
- THEN the system MUST return the existing change with its `title` updated
- AND MUST NOT create a second row for that tuple

#### Scenario: An unregistered project name is accepted

- GIVEN the project name `"some-external-repo"` is not a registered project record
- WHEN a caller with `sdd:write` creates a change under that project name
- THEN the change is created
- AND the project name is stored verbatim

### Requirement: Artifact Identity Is (change, kind, capability) With an Empty-String Capability Sentinel

The system MUST identify an artifact within a change by the tuple `(change, kind, capability)`, and MUST enforce that tuple as unique. `kind` MUST be one of `exploration | proposal | spec | design | tasks | apply-progress | verify-report | archive-report | state`. `capability` MUST NOT be nullable: an omitted, null, or absent capability MUST be normalized to the empty string before the uniqueness check is applied, so that every non-`spec` kind can exist at most once per change. Only `kind: spec` is expected to carry a non-empty `capability`.

#### Scenario: Two saves of the same kind converge on one artifact

- GIVEN a caller saves `kind: "design"` for change `"sdd-artifacts"` with no `capability` supplied
- WHEN they save `kind: "design"` for the same change again with different content and again no `capability`
- THEN exactly one `design` artifact exists for that change
- AND the second save is recorded against that same artifact

#### Scenario: Omitted capability MUST NOT create a duplicate artifact row

- GIVEN a `design` artifact exists for a change, stored with the empty-string capability sentinel
- WHEN a caller saves `kind: "design"` for that change with `capability` explicitly null
- THEN the system MUST resolve to the existing artifact
- AND MUST NOT insert a second `(change, "design", ·)` row
- AND the artifact count for that change MUST remain unchanged

#### Scenario: Spec artifacts are discriminated by capability

- GIVEN a caller saves `kind: "spec", capability: "sdd-artifact-store"` for a change
- WHEN they also save `kind: "spec", capability: "sdd-artifact-links"` for the same change
- THEN two distinct `spec` artifacts exist under that change
- AND each has its own independent revision history

#### Scenario: Reject an unrecognized artifact kind

- GIVEN a caller holds `sdd:write`
- WHEN they save an artifact with a `kind` outside the fixed set
- THEN the system MUST reject the request with a 4xx validation error
- AND MUST NOT create an artifact or a change

### Requirement: Saving an Artifact Is Idempotent by Content Hash

The system MUST compute a content hash over the submitted artifact content and compare it against the hash of the artifact's **latest** revision. When the hashes match, the system MUST return the existing artifact untouched, MUST NOT create a revision, MUST NOT advance `latest_revision`, MUST NOT rewrite the search index, and MUST NOT bump the artifact's update timestamp. The save response MUST report whether a revision was created. Saving MUST always be a non-creating success (`200`), never a `201`, so the harness can call it unconditionally on every phase.

#### Scenario: First save creates revision 1

- GIVEN no `design` artifact exists for change `"sdd-artifacts"`
- WHEN a caller with `sdd:write` saves `design` content
- THEN the artifact is created with `latest_revision` = 1
- AND the response reports that a revision was created
- AND the response status is 200, not 201

#### Scenario: Identical re-save creates NO revision

- GIVEN a `design` artifact is at revision 3
- WHEN a caller saves byte-identical content for that artifact
- THEN the system MUST NOT create revision 4
- AND `latest_revision` MUST remain 3
- AND the artifact's update timestamp MUST be unchanged
- AND the response reports that no revision was created

#### Scenario: Changed content appends a revision

- GIVEN a `design` artifact is at revision 3
- WHEN a caller saves content whose hash differs from revision 3's hash
- THEN revision 4 is appended
- AND `latest_revision` becomes 4
- AND revisions 1 through 3 remain individually retrievable and unmodified

#### Scenario: Reverting to earlier content appends a new revision

- GIVEN an artifact has revision 1 with content A and revision 2 with content B
- WHEN a caller saves content A again
- THEN the system MUST append revision 3 containing content A
- AND MUST NOT reuse or resurrect revision 1
- AND the comparison is made only against the latest revision, never against the full history

#### Scenario: Saving an artifact for an unknown change creates the change

- GIVEN no change named `"brand-new"` exists for project `"nexus-mind"`
- WHEN a caller with `sdd:write` saves a `proposal` artifact for `(nexus-mind, brand-new)`
- THEN the change is created
- AND the artifact and its revision 1 are created in the same atomic operation

### Requirement: Artifact Revisions Are Immutable and Append-Only

The system MUST treat every artifact revision as immutable once written: it MUST NOT expose any endpoint or tool that updates or deletes an individual revision's content. Revision numbers MUST be 1-based and monotonically increasing per artifact with no gaps and no reuse. Each revision MUST record its content, content hash, byte size, source (`agent | admin | import`), the user who created it, its creation timestamp, and — when supplied — its `git_path` and `git_commit` provenance.

#### Scenario: Revision content never changes after creation

- GIVEN revision 1 of an artifact was written with content A
- WHEN the artifact is subsequently saved twice with different content
- THEN fetching revision 1 still returns content A byte-for-byte
- AND revision 1's content hash and byte size are unchanged

#### Scenario: No API mutates an existing revision

- GIVEN an artifact with three revisions
- WHEN a caller attempts to modify or delete revision 2 through any exposed endpoint or tool
- THEN no such operation is available
- AND the revision remains intact

#### Scenario: Git provenance is recorded per revision

- GIVEN a caller saves an artifact supplying `path` and a git commit sha
- WHEN the revision is created
- THEN the revision records both the git path and the git commit
- AND a later revision saved without provenance does not overwrite the earlier revision's provenance

#### Scenario: Revision numbering is monotonic per artifact

- GIVEN two artifacts under the same change, each at revision 1
- WHEN one of them receives two further saves with changed content
- THEN that artifact reaches revision 3
- AND the other artifact remains at revision 1

### Requirement: Artifact Content Is Capped at 1 MB

The system MUST reject an artifact save whose content exceeds 1 MB with a 422 Unprocessable Entity response, and the rejection MUST be atomic: no change, no artifact, and no revision may be created or modified by a rejected save.

#### Scenario: Oversized content is rejected with 422

- GIVEN a caller holds `sdd:write`
- WHEN they save an artifact whose content exceeds 1 MB
- THEN the system MUST respond with 422
- AND MUST NOT create a revision

#### Scenario: A rejected oversized save leaves no partial state

- GIVEN no change named `"oversized"` exists
- WHEN a caller saves a 2 MB `design` artifact for `(nexus-mind, oversized)`
- THEN the system MUST respond with 422
- AND the change `"oversized"` MUST NOT exist afterwards
- AND no artifact row MUST have been created

#### Scenario: Content at or under the cap is accepted

- GIVEN a caller saves an artifact whose content is just under 1 MB
- WHEN the save is processed
- THEN the revision is created
- AND its recorded byte size matches the submitted content's byte length

### Requirement: SDD Operations Are Gated by sdd:read, sdd:write, and sdd:delete

The system MUST gate every SDD read (list changes, get change, list artifacts, get artifact, list revisions, get revision, search) behind `sdd:read`; every SDD write (create/patch a change, save an artifact) behind `sdd:write`; and change archival behind `sdd:delete`. A caller lacking the required permission MUST receive 403 Forbidden and MUST NOT cause any state change.

#### Scenario: Read denied without sdd:read

- GIVEN a caller holds no `sdd:read` grant
- WHEN they list changes, fetch an artifact, or call search
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT return any artifact content or metadata

#### Scenario: Save denied without sdd:write

- GIVEN a caller holds `sdd:read` but not `sdd:write`
- WHEN they attempt to save an artifact
- THEN the system MUST respond with 403 Forbidden
- AND MUST NOT create a change, artifact, or revision

#### Scenario: Archive denied without sdd:delete

- GIVEN a caller holds `sdd:read` and `sdd:write` but not `sdd:delete`
- WHEN they attempt to archive a change
- THEN the system MUST respond with 403 Forbidden
- AND the change's `archived_at` MUST remain unset

#### Scenario: Privileged roles bypass the permission check

- GIVEN a caller is an organization admin or super user
- WHEN they perform any SDD read, write, or delete operation
- THEN the operation is permitted without an explicit `sdd:*` grant

### Requirement: SDD Data Is Isolated Per Organization and Never Leaks Existence

The system MUST scope every SDD read, write, search, and archive operation to the caller's organization. A change, artifact, or revision belonging to another organization MUST be indistinguishable from one that does not exist: the system MUST respond with 404 Not Found — never 403 — and MUST NOT reveal whether the resource exists.

#### Scenario: Cross-org artifact fetch returns 404, not 403

- GIVEN an artifact exists in organization B
- AND the caller belongs to organization A and holds `sdd:read`
- WHEN they fetch that artifact by id
- THEN the system MUST respond with 404 Not Found
- AND MUST NOT return content, metadata, or any signal that the id is valid

#### Scenario: Cross-org save does not hijack another org's change

- GIVEN organization B has a change `(nexus-mind, team-tasks)`
- WHEN a caller in organization A saves an artifact for `(nexus-mind, team-tasks)`
- THEN a separate change is created under organization A
- AND organization B's change and its revisions are unmodified

#### Scenario: Search never crosses the organization boundary

- GIVEN organization B has an artifact containing the term "rate limiting"
- WHEN a caller in organization A with `sdd:read` searches for "rate limiting"
- THEN organization B's artifact MUST NOT appear in the results

#### Scenario: Unknown artifact id returns 404

- GIVEN an artifact id that has never existed
- WHEN a caller with `sdd:read` fetches it
- THEN the system MUST respond with 404 Not Found

### Requirement: List Endpoints Return Metadata Only, Never Artifact Content

The system MUST exclude artifact content from every list response. Listing changes MUST return change metadata only; listing a change's artifacts MUST return the artifact inventory (kind, capability, path, latest revision, timestamps) without content; listing an artifact's revisions MUST return revision metadata (revision number, hash, byte size, source, provenance, author, timestamp) without content. Content MUST be obtainable only by fetching a specific artifact (which returns its latest revision's content) or a specific revision.

#### Scenario: Change list carries no content

- GIVEN a change has a 36 KB `design` artifact
- WHEN a caller lists changes
- THEN the response contains the change's metadata
- AND MUST NOT contain the design's markdown content

#### Scenario: Revision list carries metadata but no content

- GIVEN an artifact has three revisions
- WHEN a caller lists that artifact's revisions
- THEN the response contains three entries with revision number, byte size, content hash, source, and author
- AND MUST NOT contain the content of any revision

#### Scenario: Fetching an artifact returns its latest revision's full content

- GIVEN an artifact is at revision 3
- WHEN a caller with `sdd:read` fetches the artifact by id
- THEN the response includes the complete, untruncated content of revision 3
- AND identifies which revision the content belongs to

#### Scenario: Fetching a specific revision returns that revision's full content

- GIVEN an artifact has revisions 1 through 3
- WHEN a caller fetches revision 2 explicitly
- THEN the response includes the complete, untruncated content of revision 2

### Requirement: Artifacts Are Full-Text Searchable Over Their Latest Revision Only

The system MUST maintain a full-text index over the latest revision of every artifact, keyed so that each artifact contributes exactly one indexed document. Search MUST accept a query and a result limit, MUST return matching artifacts with a text snippet and enough identity to fetch the artifact (change name, kind, capability), MUST be scoped to the caller's organization, and MUST require `sdd:read`. Superseded revisions MUST NOT be searchable.

#### Scenario: A term in the latest revision is findable

- GIVEN a `design` artifact whose latest revision mentions "content-hash de-duplication"
- WHEN a caller with `sdd:read` searches for "content-hash de-duplication"
- THEN the artifact is returned
- AND the result includes a snippet and the artifact's change name, kind, and capability

#### Scenario: A term removed by a newer revision stops matching

- GIVEN revision 1 of an artifact contained the term "sharding"
- AND revision 2 removed that term
- WHEN a caller searches for "sharding"
- THEN that artifact MUST NOT be returned
- AND the search index MUST reflect only revision 2

#### Scenario: An artifact contributes at most one search hit

- GIVEN an artifact has five revisions, all mentioning "idempotent"
- WHEN a caller searches for "idempotent"
- THEN the artifact appears exactly once in the results

#### Scenario: An idempotent re-save does not disturb the index

- GIVEN an artifact is indexed at revision 3
- WHEN a caller re-saves byte-identical content
- THEN the index entry is unchanged
- AND search results for terms in that artifact are unaffected

#### Scenario: Search denied without sdd:read

- GIVEN a caller holds no `sdd:read` grant
- WHEN they call the SDD search endpoint
- THEN the system MUST respond with 403 Forbidden

### Requirement: Changes Are Soft-Archived, Never Hard-Deleted

The system MUST implement change deletion as a soft archive that sets `archived_at`, MUST require `sdd:delete`, and MUST exclude archived changes from list results by default while making them retrievable via an explicit `include_archived` filter or by id. Archiving a change MUST NOT delete its artifacts or revisions.

#### Scenario: Soft-archive a change

- GIVEN a caller holds `sdd:delete`
- WHEN they delete a change
- THEN the change's `archived_at` is set
- AND the change no longer appears in default list results

#### Scenario: Archived change's artifacts survive

- GIVEN a change with two artifacts and five revisions is archived
- WHEN a caller fetches the change by id
- THEN the change is returned with its full artifact inventory
- AND every revision remains retrievable

#### Scenario: Archived changes are listable on request

- GIVEN an archived change exists
- WHEN a caller lists changes with `include_archived` set
- THEN the archived change is included in the results

#### Scenario: Archiving an unknown or cross-org change returns 404

- GIVEN a change id that does not exist, or belongs to another organization
- WHEN a caller with `sdd:delete` attempts to archive it
- THEN the system MUST respond with 404 Not Found

### Requirement: Change Listing Supports Filtering, and Change Metadata Is Patchable

The system MUST support filtering the change list by `project`, `status`, `phase`, `sprint_id`, and `include_archived`. The system MUST allow patching a change's `title`, `status`, `phase`, and `sprint_id` under `sdd:write`, and MUST NOT allow patching a change's `project` or `name` (its identity tuple).

#### Scenario: Filter changes by project and phase

- GIVEN changes exist across two projects and several phases
- WHEN a caller lists changes filtered by `project: "nexus-mind"` and `phase: "design"`
- THEN only changes in that project and phase are returned

#### Scenario: Patch a change's phase

- GIVEN a change is in phase `spec`
- WHEN a caller with `sdd:write` patches `phase` to `design`
- THEN the change's phase is `design`
- AND its update timestamp is refreshed

#### Scenario: Patch denied without sdd:write

- GIVEN a caller holds `sdd:read` only
- WHEN they attempt to patch a change's phase
- THEN the system MUST respond with 403 Forbidden
- AND the change MUST be unmodified

#### Scenario: Identity fields are not patchable

- GIVEN a change `(nexus-mind, sdd-artifacts)` exists
- WHEN a caller attempts to patch its `project` or `name`
- THEN the system MUST reject the request with a 4xx error
- AND the change's identity tuple MUST be unchanged

### Requirement: Phase Is Advisory Metadata, Not a Write Gate

The system MUST treat `phase` as advisory positional metadata for the change, and MUST NOT use it to gate artifact saves. The artifact inventory is the ground truth for what exists; the system MUST accept an artifact of any valid kind regardless of the change's current phase, and MUST NOT silently mutate `phase` as a side effect of saving an artifact.

#### Scenario: Saving an out-of-order artifact is accepted

- GIVEN a change is in phase `propose`
- WHEN a caller saves a `verify-report` artifact for that change
- THEN the save succeeds
- AND the system MUST NOT reject it for being out of phase order

#### Scenario: Saving an artifact does not change the phase

- GIVEN a change is in phase `spec`
- WHEN a caller saves a `design` artifact for that change
- THEN the change's `phase` remains `spec`
- AND advancing the phase requires an explicit patch
