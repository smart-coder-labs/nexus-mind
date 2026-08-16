# Delta for Knowledge Migration Review

## ADDED Requirements

### Requirement: Human Approval Before Commit

No candidate may reach a destination without an explicit human approval recorded against it. The system MUST NOT auto-approve a candidate on any signal, including a confidence score supplied by the classifier.

#### Scenario: Unapproved candidate is not committed

- GIVEN a run holds staged candidates
- WHEN a commit is requested for the run
- THEN only candidates in status `approved` are committed
- AND staged candidates remain staged

#### Scenario: Confidence does not grant approval

- GIVEN a candidate carries the highest possible classifier confidence
- WHEN no human approval has been recorded
- THEN the candidate MUST NOT be committed
- AND the attempt is recorded as `not_approved`

### Requirement: Optimistic Concurrency on Review

Every review action MUST carry the candidate version the reviewer acted on. An action whose `expected_version` does not match the candidate's current version MUST be rejected, and the rejection MUST be recorded.

#### Scenario: Stale approval is rejected

- GIVEN two reviewers load the same candidate at version 1
- WHEN the first approves it and the candidate moves to version 2
- AND the second submits an approval with `expected_version` 1
- THEN the second action MUST be rejected as stale
- AND a `stale_version` review action is recorded with both versions

#### Scenario: Current-version action succeeds

- GIVEN a candidate is at version 2
- WHEN a reviewer submits an action with `expected_version` 2
- THEN the action is applied
- AND the candidate version increments

### Requirement: Append-Only Review History

Review actions MUST be append-only. The system MUST reject any attempt to update or delete a recorded review action, including corrections.

#### Scenario: Updating a review action is refused

- GIVEN a review action was recorded
- WHEN any caller attempts to modify or delete it
- THEN the operation MUST be refused
- AND the original action remains intact

#### Scenario: A reversal is a new action

- GIVEN a candidate was rejected
- WHEN a reviewer re-stages it
- THEN a new `restaged` action is appended
- AND the earlier rejection remains visible in the history

### Requirement: Rejected Candidates Do Not Reappear

A rejected candidate MUST NOT be re-proposed by a later run of the same source at the same content. Re-proposing it MUST require an explicit re-staging action.

#### Scenario: Rescan does not resurrect a rejection

- GIVEN a candidate was rejected and its source is unchanged
- WHEN the same source is scanned again
- THEN no new candidate is staged for it
- AND the run report shows it as previously rejected

#### Scenario: Changed source after rejection is proposed again

- GIVEN a candidate was rejected
- WHEN its source content changes
- THEN a new candidate with a new source identity is staged
- AND it is presented as a fresh review, not as an override of the rejection

### Requirement: Provenance Visible at Review Time

A candidate MUST present, at review time, the source it came from and the literal source excerpt supporting it. A reviewer MUST NOT have to open the original source to judge the candidate.

#### Scenario: Candidate shows its origin

- GIVEN a candidate awaiting review
- WHEN a reviewer opens it
- THEN the candidate shows its source identity, its human-readable origin, and the verbatim excerpt it was derived from
- AND it shows the proposed destination and destination hint

### Requirement: Constrained Batch Approval

Batch approval MUST be available, and MUST be constrained by provenance. Candidates whose provenance is `client_attested` MUST be approved individually.

#### Scenario: Verified candidates approve in batch

- GIVEN a set of candidates all carrying `verified_manifest` provenance
- WHEN a reviewer approves them as a batch
- THEN all of them are approved in one action set
- AND one review action is recorded per candidate

#### Scenario: Attested candidate blocks batch approval

- GIVEN a batch containing at least one `client_attested` candidate
- WHEN a reviewer attempts a batch approval
- THEN the system MUST refuse the batch
- AND MUST identify the candidates requiring individual review

### Requirement: Reviewer Authorization Is Recorded

Every review action MUST record the acting user and the authorization under which they acted. An action by a user lacking permission on the run's scope MUST be refused and recorded.

#### Scenario: Unauthorized review is refused and recorded

- GIVEN a user without write access to the run's client
- WHEN they attempt to approve a candidate
- THEN the action MUST be refused
- AND a `permission_denied` review action is recorded
