# Harness Config Review Specification

## Purpose

Allow users to share redacted Claude configuration snapshots for review while preserving deterministic redaction, preview-before-upload, and strict boundaries against raw secret exposure.

## Requirements

### Requirement: Redacted Config Snapshot Upload

The system MUST accept only user-reviewed, redacted configuration snapshots with source tool, redaction report, content hash, and review status.

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

### Requirement: Deterministic Redaction Report

The system MUST require a redaction report that identifies redaction categories without exposing secret values, raw shell profile content, or sensitive hook arguments.

#### Scenario: Inspect redaction report

- GIVEN a stored config review exists
- WHEN an authorized reviewer inspects it
- THEN the reviewer sees redaction categories and counts
- AND does not see original secret values

#### Scenario: Missing deterministic hash

- GIVEN a redacted snapshot lacks a stable content hash
- WHEN the upload is submitted
- THEN the system MUST reject it with validation details

### Requirement: Permissioned Sharing Boundary

The system MUST require config review permission for upload, sharing, and inspection, and MUST audit those actions using safe metadata only.

#### Scenario: Share config review

- GIVEN a user has config review permission
- WHEN the user marks a redacted snapshot as shared
- THEN authorized reviewers can inspect the redacted snapshot
- AND the system records a safe audit event

#### Scenario: Unauthorized inspection denied

- GIVEN a user lacks config review permission
- WHEN the user attempts to inspect a shared snapshot
- THEN the system MUST deny access
- AND MUST NOT disclose redacted configuration content
