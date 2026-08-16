# Delta for Knowledge Migration

## ADDED Requirements

### Requirement: Migration Run Scoping

A migration run MUST be scoped to exactly one organization, MUST declare its `source_kind`, and MAY be attributed to one client and one project. `client_id` and `project_id` MUST be immutable after the run is created, and any referenced project MUST belong to the run's organization.

A run with `client_id IS NULL` denotes internal work and MUST NOT be treated as belonging to any client.

#### Scenario: Run created for a client project

- GIVEN a client and a project of that client exist in the organization
- WHEN an operator creates a migration run naming both
- THEN the run is created with status `staging`
- AND every candidate staged under it inherits that client and project

#### Scenario: Project from another organization is rejected

- GIVEN a project id that belongs to a different organization
- WHEN a run is created referencing it
- THEN the system MUST reject the run
- AND MUST NOT create any partial run record

#### Scenario: Client cannot be reassigned after creation

- GIVEN a run exists with a client
- WHEN any caller attempts to change the run's `client_id`
- THEN the system MUST reject the change
- AND the run keeps its original client

### Requirement: Per-Candidate Destination

Each candidate MUST declare its own `destination_kind`, one of `memory`, `convention`, `task`, `sdd_artifact`, `harness`, or `harness_config_review`. A single run MUST be able to hold candidates of different destination kinds.

A candidate MAY carry a `destination_hint` holding the fields its destination requires and the generic candidate shape does not express.

#### Scenario: One scan produces several destination kinds

- GIVEN a source scan yields prose, a team rule, an unchecked task item, and a spec document
- WHEN the connector stages them under one run
- THEN each candidate carries its own destination kind
- AND the run accepts all of them without a separate run per kind

#### Scenario: Unknown destination kind is rejected

- GIVEN a candidate declaring a destination kind outside the accepted set
- WHEN it is submitted for staging
- THEN the system MUST reject that candidate
- AND MUST report which value was rejected

### Requirement: Deterministic Source Identity

Every candidate MUST carry a `source_identity` computed by the connector from its provenance, and that identity MUST include a hash of the source content. Candidate identity MUST be unique within a run.

#### Scenario: Unchanged source produces no new candidate

- GIVEN a source was already staged and committed in an earlier run
- WHEN the same source is scanned again without modification
- THEN the connector produces the same `source_identity`
- AND the system reports it as skipped rather than staging a duplicate

#### Scenario: Edited source is proposed again

- GIVEN a source was committed in an earlier run
- WHEN the source content changes and is scanned again
- THEN the computed `source_identity` differs
- AND a new candidate is staged for human review

#### Scenario: Duplicate identity within a run is rejected

- GIVEN a candidate with a given `source_identity` is already staged in a run
- WHEN a second candidate with the same identity is submitted to that run
- THEN the system MUST reject the second submission

### Requirement: Idempotent Commit Across Runs

The system MUST NOT commit the same `source_identity` to the same destination kind twice within an organization. A repeated commit MUST be recorded as skipped, and MUST NOT fail the enclosing batch.

#### Scenario: Second commit of the same source is skipped

- GIVEN a source identity was already committed to `memory` in this organization
- WHEN a candidate with that identity and destination is approved and committed in a later run
- THEN the system records the outcome as `skipped`
- AND no second destination record is created
- AND the remaining candidates in the batch continue to commit

#### Scenario: Same source to a different destination kind is allowed

- GIVEN a source identity was committed to `memory`
- WHEN a candidate with the same identity and destination `convention` is committed
- THEN the commit succeeds
- AND both provenance records coexist

### Requirement: Per-Candidate Atomic Commit

Committing one candidate MUST write the destination record, its provenance record, and its outcome record atomically. A failure on one candidate MUST NOT roll back candidates already committed in the same batch, and the batch MUST be resumable.

#### Scenario: One candidate fails mid-batch

- GIVEN a batch of approved candidates is committed
- WHEN one candidate fails to write its destination
- THEN that candidate is recorded with outcome `failed` and an error code
- AND candidates committed before it remain committed
- AND candidates after it are still processed

#### Scenario: Failed candidate leaves no partial record

- GIVEN a candidate fails while writing its destination
- WHEN the failure is recorded
- THEN no provenance record exists for that candidate
- AND re-running the commit retries it

### Requirement: Destination Persistence Reuse

Committing a candidate MUST reuse the persistence path that the corresponding first-class API already uses for that destination. The migration MUST NOT write destination records through a parallel code path that bypasses the destination's own scoping, audit, or indexing behaviour.

#### Scenario: Committed memory is audited like any other

- GIVEN a candidate with destination `memory` is committed
- WHEN the commit succeeds
- THEN an audit record exists for the memory creation
- AND the memory is subject to the same visibility rules as a memory stored through the memory API

#### Scenario: Committed harness goes through harness publication

- GIVEN a candidate with destination `harness` carrying a typed manifest
- WHEN the commit succeeds
- THEN a harness and a published harness version exist
- AND the manifest was validated by the harness manifest validator
- AND an invalid manifest fails the candidate instead of creating a harness

### Requirement: Client Isolation of Migrated Knowledge

An artifact committed by a migration MUST inherit the run's client attribution, and MUST NOT be readable by a user who cannot access that client. A denied read MUST be audited.

#### Scenario: Cross-client read is denied

- GIVEN knowledge was migrated under client A
- WHEN a user with access only to client B searches, lists, or loads context
- THEN the migrated artifact MUST NOT appear in any result
- AND the denied attempt is recorded in the audit log

#### Scenario: Internal run is not attributed to a client

- GIVEN a run created with no client
- WHEN its candidates are committed
- THEN the resulting artifacts have no client attribution
- AND they behave as internal organization knowledge

### Requirement: Backend Model Independence

The backend MUST NOT invoke a language model at any point in the migration pipeline. Candidates MUST arrive already classified, and the system MUST remain fully operational when no model credentials are configured anywhere in the deployment.

#### Scenario: Migration works without model credentials

- GIVEN the backend is deployed with no language-model credentials
- WHEN candidates are staged, reviewed, and committed
- THEN every step succeeds
- AND no outbound model request is made

### Requirement: Run Reporting and Token Accounting

A run MUST expose a report of what was staged, approved, rejected, committed, skipped, and failed, with a reason for every non-committed outcome. A run that used a language model outside the backend MUST report its token consumption to the usage subsystem.

#### Scenario: Report explains every skip

- GIVEN a run completes with some candidates skipped
- WHEN the run report is requested
- THEN each skipped candidate carries the reason it was skipped
- AND the counts reconcile with the number of candidates staged

#### Scenario: Runner reports its token spend

- GIVEN a runner classified sources with an external model
- WHEN the run completes
- THEN token usage is recorded against the run's organization, client, and project
