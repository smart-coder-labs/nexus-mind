# Delta for Harness Config Review

## MODIFIED Requirements

### Requirement: Redacted Config Snapshot Upload

The system MUST accept only user-reviewed, redacted configuration snapshots or config-derived harness examples with source tool, redaction report, content hash, and review status. Config-derived examples MUST NOT store raw secrets, tokens, private local paths, or unreviewed shell/hook arguments. When a config review is created from an agent session (for example via an MCP `create_harness_config_review` tool), the same local redaction and preview-before-upload rules apply before the agent session may submit the snapshot.
(Previously: did not address config reviews originating from an agent session as a distinct caller.)

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

#### Scenario: Agent-session config review requires local preview before upload

- GIVEN an agent session redacts a local Claude config snapshot via an MCP tool
- WHEN the agent session attempts to upload it as a config review
- THEN the redaction and preview MUST have occurred locally before the upload call
- AND the system MUST reject the upload if no local redaction report accompanies it

#### Scenario: Agent-session upload still enforces raw-content rejection

- GIVEN an agent session submits a config review upload containing unredacted secret indicators
- WHEN the upload reaches the backend
- THEN the system MUST reject the upload exactly as it would for a non-agent caller
- AND MUST NOT persist the raw content
