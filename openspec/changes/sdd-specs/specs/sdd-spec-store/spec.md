# Delta for SDD Spec Store

## ADDED Requirements

### Requirement: A Living Specification Is a Root Entity, Not an Artifact of a Change

The system MUST model `openspec/specs/{capability}/spec.md` as an entity owned by the **project**,
identified by `(org, project, capability)`, and MUST NOT model it as an artifact belonging to a change.
A living specification outlives the changes that amend it; saving one MUST NOT create a change.

#### Scenario: Saving a specification creates no change

- GIVEN an organization with no SDD changes
- WHEN a caller saves a living specification for capability `harness-library`
- THEN the specification is stored
- AND no change row is created

#### Scenario: One specification per capability per project

- GIVEN a living specification for capability `harness-library` in project `nexus-mind`
- WHEN a second specification for `harness-library` in `nexus-mind` is created
- THEN the write is rejected
- AND a specification for `harness-library` in a **different** project remains a distinct contract

#### Scenario: A specification survives the deletion of a change that shaped it

- GIVEN a specification revision recorded as merged from change `sdd-specs`
- WHEN the change `sdd-specs` is deleted
- THEN the specification and its revision still exist
- AND the revision's recorded change is cleared rather than the revision being removed

---

### Requirement: Saving a Specification Is Idempotent by Content Hash

The system MUST compare submitted content against the hash of the **latest revision only**, never
against the full history. Byte-identical content MUST create no revision, write no search-index entry,
and MUST NOT advance the specification's `updated_at`.

#### Scenario: Re-saving identical content creates no revision

- GIVEN a specification at revision 1
- WHEN the identical content is saved again
- THEN the response reports that no revision was created
- AND the specification remains at revision 1 with an unchanged `updated_at`
- AND the search index still holds exactly one entry for it

#### Scenario: A revert is an event and appends

- GIVEN a specification whose content has been A, then B
- WHEN content A is saved again
- THEN **revision 3** is appended
- AND revision 1 and revision 3 are two distinct revisions that happen to agree

---

### Requirement: The Size Cap Is Enforced Atomically

The system MUST reject content over 1 MB **before** any row is created, and MUST leave no specification
and no revision behind. A rejection MUST report an unprocessable-entity error, not a payload-too-large
error, and MUST NOT modify a pre-existing specification.

#### Scenario: An oversized first save leaves nothing behind

- WHEN a caller saves content exceeding 1 MB for a capability that has no specification
- THEN the save is rejected as unprocessable
- AND no specification row exists
- AND no revision row exists

#### Scenario: An oversized save does not disturb an existing contract

- GIVEN a specification at revision 1
- WHEN a caller saves content exceeding 1 MB for it
- THEN the save is rejected
- AND the specification is still at revision 1 with its original content and `updated_at`

---

### Requirement: Specification Revisions Are Immutable and Append-Only

The system MUST NOT modify or remove a specification revision once written. Revisions are produced by
the save path's insert and reclaimed only by cascade from the parent specification. No endpoint MUST
accept a write against an existing revision.

#### Scenario: A revision cannot be rewritten over HTTP

- GIVEN a specification with a revision 1
- WHEN a caller attempts to modify or delete revision 1 through the API
- THEN the request is rejected as a method that is not allowed
- AND revision 1's content is unchanged

---

### Requirement: A Revision Records the Change That Produced It

The system MUST record, on each specification revision, which change merged its delta specs to produce
that revision. A caller MUST be able to ask a change which specifications it has merged into, and a
specification which change last merged into it.

A submitted change name that resolves to no change visible to the caller MUST reject the save **whole**
— reporting not-found and writing nothing — rather than storing the content with the provenance
silently absent. A revision saved outside the change pipeline (an import, an administrative edit) MUST
be allowed to carry no provenance, and that is a distinct and legitimate state.

#### Scenario: The merge is traceable in both directions

- GIVEN a change named `sdd-specs` in project `nexus-mind`
- WHEN a specification for `sdd-spec-store` is saved naming `sdd-specs` as the change it merges
- THEN the specification reports `sdd-specs` as the change that last merged into it
- AND the change reports `sdd-spec-store` among the specifications it has merged into, with the
  revision that merge produced

#### Scenario: An unknown change name is refused and nothing is written

- GIVEN no change named `no-such-change`
- WHEN a caller saves a specification naming `no-such-change` as its merge source
- THEN the save is rejected as not-found
- AND no specification is created

#### Scenario: Another organization's change is not a resolvable provenance

- GIVEN a change named `a-change` in organization A
- WHEN a caller in organization B saves a specification naming `a-change` as its merge source
- THEN the save is rejected as not-found

#### Scenario: A revision may legitimately have no provenance

- WHEN a specification is imported from the filesystem, where the merging change is not recorded
- THEN the revision is stored with no change recorded against it
- AND this is not an error

---

### Requirement: Reads Are Organization-Scoped, and Not-Found and Not-Visible Are Both 404

The system MUST scope every specification read to the caller's organization, and MUST answer a request
for a specification that does not exist, or that belongs to another organization, with the same
not-found response. A caller MUST NOT be able to distinguish the two.

A capability with no specification MUST report not-found, and MUST NOT return a success carrying an
empty document: "this capability has no contract yet" and "its contract is empty" are different facts.

#### Scenario: Another organization's specification is not found

- GIVEN a specification in organization A
- WHEN a caller in organization B requests it by id, lists its revisions, or reads one of its revisions
- THEN each request reports not-found, not forbidden

#### Scenario: A capability with no specification is not found

- WHEN a caller requests the specification for a capability that has none
- THEN the request reports not-found
- AND no empty document is returned

---

### Requirement: Lists Never Carry Content

The system MUST NOT include specification content in the specification list, in the revision list, or in
the specifications reported for a change. Content MUST be fetched explicitly, by id or by natural key,
or per revision.

#### Scenario: The specification list is metadata only

- GIVEN two specifications with substantial content
- WHEN a caller lists the specifications for a project
- THEN each entry carries its capability, title, latest revision and the change that last merged into it
- AND no entry carries any specification content

#### Scenario: The revision list is metadata only

- GIVEN a specification with two revisions
- WHEN a caller lists its revisions
- THEN each entry carries its revision number, source, size, hash and merging change
- AND no entry carries content

---

### Requirement: A Specification Read Returns the Full Document

The system MUST return the complete text of a specification — never a preview, a truncation, or a
summary. A caller MUST be able to read the latest revision or any explicit earlier revision in full.

#### Scenario: The full contract is returned

- GIVEN a specification of two hundred requirements
- WHEN a caller reads it
- THEN the first requirement and the last requirement are both present in the response

#### Scenario: An older revision is readable in full

- GIVEN a specification whose content has been amended
- WHEN a caller reads revision 1 explicitly
- THEN the original content is returned in full

---

### Requirement: Search Spans Both Trees and Labels Every Hit

The system MUST search the living specifications as well as the artifacts of changes, and every hit
MUST state which of the two it came from. A caller MUST be able to distinguish a requirement the
capability has agreed from a change someone is proposing.

The full-text index MUST track the latest revision of each specification only, so that a specification
contributes exactly one hit however long its history, and a requirement struck from the contract stops
matching.

#### Scenario: A search finds the contract, not only the drafts

- GIVEN a change whose design document proposes rate limiting
- AND a living specification for `gateway` that requires rate limiting
- WHEN a caller searches for "rate limiting"
- THEN both are returned
- AND the specification hit is labelled as a specification and carries no change
- AND the artifact hit is labelled as an artifact and carries no specification id

#### Scenario: A struck requirement stops matching

- GIVEN a specification whose latest revision no longer mentions "leaky buckets"
- WHEN a caller searches for "leaky"
- THEN no specification is returned

---

### Requirement: Global Search Exposes Specifications Without Gating Itself on Them

The system MUST include a specifications facet in global search for callers holding `sdd:read`, and MUST
return that facet **empty** — never an authorization failure — for callers without it. Global search
MUST NOT begin failing for users who lack the SDD grants.

#### Scenario: The facet is empty, not forbidden

- GIVEN a caller who may search memories but holds no `sdd:read`
- WHEN they perform a global search
- THEN the request succeeds
- AND the specifications facet is empty

---

### Requirement: Specification Content Is Read-Only Outside the Harness

The system MUST NOT offer any means of authoring or editing specification content from the
administrative interface, and MUST NOT offer an agent tool that deletes a specification. The contract is
written by the harness and by git; the platform stores and serves it.

#### Scenario: The administrative specification view offers no editor

- WHEN an administrator opens a specification
- THEN its content is rendered read-only
- AND no control is offered to edit, save or delete it

---

### Requirement: The Importer Walks the Specifications Tree Idempotently

The system's importer MUST walk `openspec/specs/*/spec.md`, importing each as a living specification
with an import provenance and its repository-relative path, over both the database and the HTTP sink. A
second run MUST create zero revisions. The importer MUST NOT invent a merging change for a specification
read from disk, where that fact is not recorded.

#### Scenario: A second import creates no revision

- GIVEN a specifications tree already imported
- WHEN the importer runs again over unchanged files
- THEN no revision is created
- AND the run reports the files as skipped

#### Scenario: The importer records no provenance it does not have

- WHEN a specification is imported from the filesystem
- THEN the revision is stored with an import source and no merging change

#### Scenario: A delta spec and the living spec for one capability remain two documents

- GIVEN a change containing a delta spec for capability `cap`
- AND a living specification for capability `cap`
- WHEN both trees are imported
- THEN the delta spec is an artifact of the change
- AND the living specification is a separate entity
- AND the delta spec does not appear among the living specifications
