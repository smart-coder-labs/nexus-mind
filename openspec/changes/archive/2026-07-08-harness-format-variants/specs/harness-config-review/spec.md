# Delta for Harness Config Review

## MODIFIED Requirements

### Requirement: Redacted Config Snapshot Upload

The system MUST accept only user-reviewed, redacted configuration snapshots or config-derived harness examples with source tool, redaction report, content hash, and review status. Config-derived examples MUST NOT store raw secrets, tokens, private local paths, or unreviewed shell/hook arguments.
(Previously: The requirement covered redacted config snapshots, but not config-derived harness examples.)

#### Scenario: Upload reviewed redacted snapshot

- GIVEN a user previews a redacted Claude config snapshot locally
- WHEN the user uploads the approved snapshot
- THEN the system stores the redacted content, redaction report, and content hash
- AND associates it with the organization and user

#### Scenario: Reject raw secret-bearing snapshot

- GIVEN an upload contains unredacted secret indicators or lacks a redaction report
- WHEN the user submits the snapshot
- THEN the system MUST reject the upload
- AND MUST NOT persist the raw content

#### Scenario: Reject unsafe config-derived harness example

- GIVEN a harness example is derived from local config and contains raw secrets or sensitive local paths
- WHEN the user previews or publishes it
- THEN the system MUST reject it with validation details
- AND MUST NOT persist the unsafe values

### Requirement: Deterministic Redaction Report

The system MUST require a redaction report that identifies redaction categories without exposing secret values, raw shell profile content, sensitive hook arguments, or private local path values.
(Previously: The report excluded secret values, shell profile content, and hook arguments, but did not explicitly exclude private local paths.)

#### Scenario: Inspect redaction report

- GIVEN a stored config review exists
- WHEN an authorized reviewer inspects it
- THEN the reviewer sees redaction categories and counts
- AND does not see original secret values or private local paths

#### Scenario: Missing deterministic hash

- GIVEN a redacted snapshot lacks a stable content hash
- WHEN the upload is submitted
- THEN the system MUST reject it with validation details
