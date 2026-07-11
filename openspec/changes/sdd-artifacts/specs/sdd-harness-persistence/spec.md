# Delta for SDD Harness Persistence

## ADDED Requirements

### Requirement: The nexusmind Persistence Mode Writes to Both the Artifact Store and the Filesystem

The system MUST add a `nexusmind` mode to the SDD harness persistence contract. In this mode, every SDD phase MUST write its artifact to **both** the NexusMind artifact store (via `save_sdd_artifact`) **and** the project filesystem (per the openspec convention), and MUST read its dependency artifacts from the NexusMind store (via `get_sdd_artifact`). Neither store is a cache of the other: the filesystem write is what a reviewer diffs in a PR, and the store write is what a later session, a different machine, and the admin can query. A phase MUST NOT report success unless both writes succeeded, and a failure of either write MUST be surfaced to the user rather than silently swallowed.

#### Scenario: A phase writes the file and the artifact

- GIVEN a session running in `nexusmind` mode
- WHEN `/sdd-design` completes for change `"sdd-artifacts"`
- THEN `openspec/changes/sdd-artifacts/design.md` exists on disk
- AND a `design` artifact for that change exists in NexusMind with the same content

#### Scenario: A failed store write is reported, not hidden

- GIVEN a session in `nexusmind` mode and a backend that rejects the save
- WHEN a phase attempts to persist its artifact
- THEN the failure is surfaced to the user
- AND the phase MUST NOT be reported as successfully persisted

#### Scenario: The mode records the artifact's git provenance

- GIVEN a phase writes `design.md` inside a git checkout
- WHEN it calls `save_sdd_artifact`
- THEN it supplies the artifact's repository-relative path
- AND supplies the current commit when one is resolvable

#### Scenario: Spec artifacts are persisted per capability

- GIVEN `/sdd-spec` produces three capability specs for a change
- WHEN the phase persists them in `nexusmind` mode
- THEN three files are written under `specs/{capability}/spec.md`
- AND three `spec` artifacts are saved, one per capability

### Requirement: Re-Running a Phase Appends a Revision Instead of Overwriting

The system MUST preserve iteration history in `nexusmind` mode: re-running a phase with edited content MUST produce a new revision of the artifact while leaving the previous revision retrievable, and re-running it with unchanged content MUST produce no new revision. A skill MUST be permitted to call `save_sdd_artifact` unconditionally on every run without generating churn.

#### Scenario: Re-running a phase with edits produces revision 2

- GIVEN `/sdd-design` has been run once, producing revision 1 of `design.md`
- WHEN the design is edited and `/sdd-design` is run again
- THEN the artifact reaches revision 2
- AND revision 1 remains individually retrievable with its original content

#### Scenario: Re-running a phase with no edits produces no revision

- GIVEN an artifact is at revision 2
- WHEN a phase re-runs and persists byte-identical content
- THEN no revision 3 is created
- AND the artifact remains at revision 2

#### Scenario: History is not destroyed by a rerun

- GIVEN a change whose design has been revised three times
- WHEN the change's revision history is inspected
- THEN all three revisions are listed with their timestamps and authors
- AND no earlier revision has been overwritten

### Requirement: Cross-Phase Dependency Reads Return Full Documents

The system MUST have every `sdd-*` skill read its declared input artifacts through `get_sdd_artifact`, receiving the complete document. Dependency reads MUST NOT go through memory search or any preview-returning surface. A sub-agent MUST receive the full text of the artifacts it depends on, and this MUST hold with no local checkout present.

#### Scenario: sdd-design reads the full proposal

- GIVEN a change has a `proposal` artifact of 12 KB
- WHEN `/sdd-design` runs in `nexusmind` mode
- THEN the design sub-agent receives the complete proposal text
- AND it does not receive a truncated preview

#### Scenario: sdd-tasks reads both the spec and the design

- GIVEN a change has capability specs and a design artifact
- WHEN `/sdd-tasks` runs
- THEN it fetches each capability spec and the design via `get_sdd_artifact`
- AND all are received in full

#### Scenario: A fresh machine recovers a change with no checkout

- GIVEN a session starts on a machine with no clone of the repository
- WHEN it resumes a change via `/sdd-continue`
- THEN it recovers the change's phase, status, and artifact inventory from `get_sdd_change`
- AND it reads each artifact's full content from `get_sdd_artifact`
- AND it can continue the change without a checkout

#### Scenario: A missing dependency artifact stops the phase

- GIVEN a phase declares `design.md` as a required input
- AND no `design` artifact exists for the change
- WHEN the phase runs in `nexusmind` mode
- THEN the phase MUST report the missing dependency
- AND MUST NOT proceed with empty input

### Requirement: Phase and Status Transitions Are Pushed to the Change Record

The system MUST have the `sdd-*` skills keep the NexusMind change record current: `sdd-apply` MUST advance the change to the `apply` phase and MUST link the decisions, bugfixes, and discoveries it records back to the change via `link_sdd_change_memory`; `sdd-verify` and `sdd-archive` MUST update the change's phase and status. The transitions MUST be made through `update_sdd_change`, not by editing artifact content.

#### Scenario: sdd-apply advances the phase

- GIVEN a change is in phase `tasks`
- WHEN `/sdd-apply` starts in `nexusmind` mode
- THEN it calls `update_sdd_change` with `phase: "apply"`
- AND the change's phase is `apply`

#### Scenario: sdd-apply links the memories it produced

- GIVEN `/sdd-apply` records two decision memories while implementing a change
- WHEN the phase completes
- THEN both memories are linked to the change with relation `produced`
- AND they appear among the change's linked memories

#### Scenario: sdd-archive marks the change archived

- GIVEN a change has passed verification
- WHEN `/sdd-archive` completes
- THEN it sets the change's phase to `archive` and its status to `archived`
- AND the change's artifacts and revisions remain retrievable

#### Scenario: The orchestrator's state survives as an artifact

- GIVEN the orchestrator persists its DAG state in `nexusmind` mode
- WHEN it writes `state.yaml`
- THEN a `state` artifact is also saved for the change
- AND a later session can recover it without a checkout

### Requirement: The Harness Is Published to the NexusMind Harness Library

The system MUST publish the updated `sdd-*` skills as a harness version in the NexusMind harness library, so that consuming repositories install one shared SDD harness rather than each drifting a private copy.

#### Scenario: The updated skills are published as a version

- GIVEN the `sdd-*` skills have been updated to the `nexusmind` mode
- WHEN they are packaged and published to the harness library
- THEN an immutable harness version exists containing the updated skills
- AND another repository can install that version and obtain the same skills

## MODIFIED Requirements

### Requirement: Artifact Store Mode Resolution

The persistence contract's mode set MUST be `nexusmind | openspec | hybrid | engram | none`. `nexusmind` MUST be documented as the recommended default and MUST be selected by default when the NexusMind SDD tools are available. `engram` MUST be marked **deprecated** in the contract, with its overwrite-on-rerun limitation stated explicitly and `nexusmind` named as its replacement. `engram` MUST NOT be removed: repositories still running the old contract MUST continue to work when they select it.

(Previously: the mode set was `engram | openspec | hybrid | none`, and the default was `engram` whenever the memory store was available — which routed every SDD artifact into the memory table, where upsert-by-`topic_key` overwrote the previous run and left no iteration history.)

#### Scenario: nexusmind is the default when the SDD tools are available

- GIVEN a session starts a change and the NexusMind SDD tools are available
- WHEN the orchestrator resolves the artifact store mode without an explicit user choice
- THEN it selects `nexusmind`
- AND it does not select `engram`

#### Scenario: engram remains selectable and functional

- GIVEN a user explicitly selects `engram` mode
- WHEN a phase persists an artifact
- THEN the artifact is written to the memory store as before
- AND the mode is not rejected or removed

#### Scenario: The contract states engram's limitation

- GIVEN a reader consults the persistence contract's mode comparison
- WHEN they read the `engram` row
- THEN it is marked deprecated
- AND it states that re-running a phase overwrites the previous artifact with no revision history
- AND it points to `nexusmind` as the replacement

#### Scenario: The mode comparison covers all five modes

- GIVEN the contract's mode-comparison table
- WHEN it is read
- THEN `nexusmind` is listed with read via `get_sdd_artifact`, write to both `save_sdd_artifact` and the filesystem, project files yes, and history via revisions plus git

## REMOVED Requirements

### Requirement: SDD Artifacts Are Saved as Memories With capture_prompt Disabled

**Reason:** SDD artifacts no longer share the `memories` table with human decisions, so there is nothing left to disambiguate. The `capture_prompt: false` instruction existed only because an automated pipeline artifact and a real human architecture decision were both stored as `type: architecture` memories, and the prompt-capture default had to be suppressed per call. With a dedicated artifact store, the entire workaround — the mandatory `capture_prompt: false` on every SDD `mem_save`, the "do not infer this from `type`" caveat, and the older-schema fallback — is deleted from the persistence contract and from every `sdd-*` sub-agent prompt block.

**Migration:** Skills in `nexusmind` mode call `save_sdd_artifact` instead of `mem_save` with a `sdd/{change}/{artifact}` topic key; no prompt-capture flag is involved. Skills still running the deprecated `engram` mode retain their existing behaviour unchanged. Legacy `sdd/*` memories already in the memory table are imported into the artifact store and tagged, not deleted; whether to archive them is an explicit user decision.
