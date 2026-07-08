# Harness Library Specification

## Purpose

Provide an org-scoped, permissioned library for reusable AI tooling harness manifests with immutable versions, provenance, compatibility targets, downloads, and audit trails.

## Requirements

### Requirement: Harness Catalog Visibility

The system MUST store harnesses under an organization with optional project scope and MUST list only harnesses visible to the requesting user.

#### Scenario: List visible harnesses

- GIVEN a user has harness read access in an organization
- WHEN the user lists harnesses
- THEN org-wide harnesses are returned
- AND project-scoped harnesses are returned only for projects the user can access

#### Scenario: Hide inaccessible project harness

- GIVEN a harness is scoped to a project the user cannot access
- WHEN the user lists or inspects harnesses
- THEN that harness MUST NOT be returned

### Requirement: Versioned Immutable Manifests

The system MUST publish harness versions as immutable manifests with version, manifest hash, provenance, status, and compatibility targets for Claude, Codex, or OpenCode.

#### Scenario: Publish a harness version

- GIVEN an authorized user submits a valid manifest
- WHEN the version is published
- THEN the response includes the version identifier and manifest hash
- AND later downloads for that version return the same hash

#### Scenario: Reject invalid manifest

- GIVEN a manifest is missing required provenance or compatibility targets
- WHEN the user publishes it
- THEN the system MUST reject the request with validation details

### Requirement: Permissioned Manifest Download

The system MUST require download permission before returning a harness manifest and MUST audit publish, inspect, and download actions using safe metadata only.

#### Scenario: Download authorized version

- GIVEN a user has harness download access to a published version
- WHEN the user requests the manifest download
- THEN the system returns the immutable manifest and hash
- AND records an audit event without secrets or raw local config

#### Scenario: Deny unauthorized download

- GIVEN a user lacks harness download permission
- WHEN the user requests a manifest download
- THEN the system MUST deny the request
- AND MUST NOT expose manifest contents
