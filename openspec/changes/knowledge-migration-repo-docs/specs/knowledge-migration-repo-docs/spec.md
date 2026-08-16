# Delta for Knowledge Migration — Repo Docs Connector

## ADDED Requirements

### Requirement: Section-Level Scanning

The connector MUST scan documentation at the level of document sections, not whole files. Each section MUST carry the path of its document and the heading it falls under, and a document with no headings MUST still produce one unit.

#### Scenario: A multi-section document yields several units

- GIVEN a document with three headed sections
- WHEN the connector scans it
- THEN it produces one source unit per section
- AND each unit records the document path and its heading

#### Scenario: A document without headings still scans

- GIVEN a document consisting only of prose with no heading
- WHEN the connector scans it
- THEN it produces exactly one source unit for the whole document

### Requirement: Deterministic Identity Per Section

A unit's `source_identity` MUST be derived from the repository, the document path, the section anchor, and a hash of the section's content. Re-scanning an unchanged section MUST produce the same identity; editing one section MUST change only that section's identity.

#### Scenario: Editing one section leaves the others alone

- GIVEN a document with three sections that was scanned before
- WHEN one section's text changes and the document is re-scanned
- THEN only that section's identity differs
- AND the other two produce the identities they produced before

#### Scenario: Moving a document changes its units' identities

- GIVEN a document that was scanned at one path
- WHEN the same content is scanned at a different path
- THEN the identities differ, because provenance is part of identity

### Requirement: Default Mapping From Source To Destination

The connector MUST propose a destination for every unit using deterministic rules over the document's path and the section's shape. A classifier MAY override the proposal; a human MUST always be able to.

The default rules are:

| Source | Proposed destination |
|---|---|
| A file under an `adr/` directory | memory of type `decision` |
| A section stating rules, principles, or prohibitions | convention |
| An unchecked task-list item | task |
| A file under `openspec/changes/` | sdd_artifact |
| Any other prose | memory of type `architecture` |

#### Scenario: An ADR becomes a decision

- GIVEN a file at `docs/adr/ADR-001.md`
- WHEN it is scanned
- THEN its units are proposed as memories of type `decision`

#### Scenario: An unchecked checklist item becomes a task

- GIVEN a section containing an unchecked task-list item
- WHEN it is scanned
- THEN a task candidate is proposed carrying that item's text
- AND the proposed task status is the backlog

#### Scenario: A checked item is not proposed as work

- GIVEN a section whose task-list items are all checked
- WHEN it is scanned
- THEN no task candidate is proposed from it

### Requirement: Source Excerpt Accompanies Every Candidate

Every candidate the connector produces MUST carry a verbatim excerpt of the source text it was derived from, so a reviewer can judge the proposal without opening the original file.

#### Scenario: The excerpt is the source, not a paraphrase

- GIVEN a section of documentation
- WHEN a candidate is produced from it
- THEN the candidate's excerpt appears verbatim in the source document

### Requirement: Scanning Without A Classifier

The connector MUST produce candidates with no language model available, using its deterministic rules alone. The result MAY be less well titled, and MUST be complete: no unit is dropped for lack of a classifier.

#### Scenario: Every unit still yields a candidate

- GIVEN a documentation tree and no classifier
- WHEN the connector runs
- THEN every scanned unit produces a candidate
- AND each carries a destination derived from the default rules

### Requirement: Cost Is Estimable Before It Is Spent

The connector MUST support a dry run that scans completely, performs no classification, and reports the number of documents, the number of units, and an estimate of the tokens a real run would consume.

#### Scenario: A dry run classifies nothing

- GIVEN a documentation tree
- WHEN the connector runs in dry-run mode
- THEN it reports document, unit and estimated-token counts
- AND no classification is performed
- AND no candidate is submitted

### Requirement: Noise Is Excluded By Default

The connector MUST exclude, by default, documentation that is not engineering knowledge: marketing and research material, and the living specification under `openspec/specs/`. An operator MUST be able to override any exclusion.

#### Scenario: Marketing material is skipped

- GIVEN documents under a marketing directory
- WHEN the connector scans with default settings
- THEN no unit is produced from them
- AND the run report accounts for them as excluded rather than omitting them silently

#### Scenario: The living specification is not migrated

- GIVEN documents under `openspec/specs/`
- WHEN the connector scans
- THEN no candidate is produced from them, because that tree is maintained by the archive flow rather than imported
