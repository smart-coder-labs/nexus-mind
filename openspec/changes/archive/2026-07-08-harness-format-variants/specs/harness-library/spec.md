# Delta for Harness Library

## ADDED Requirements

### Requirement: Typed Harness Format Manifests

Published harness versions MUST declare a supported format and components matching that format.

#### Scenario: Build markdown-based harness

- GIVEN an authorized user selects agent, skill, command, hook, or output-style format
- WHEN the user previews the manifest
- THEN the manifest MUST include `schema_version`, `format`, `targets`, `components`, `provenance`, and `security`
- AND the template MUST match the selected Claude-family structure

#### Scenario: Reject mismatched format structure

- GIVEN a manifest declares one format but contains components for another format
- WHEN the user publishes it
- THEN the system MUST reject the request with validation details

### Requirement: Uploaded File and Folder Components

The system MUST allow file or folder upload intent as manifest components using safe relative entries only.

#### Scenario: Package folder entries

- GIVEN a user selects multiple files with relative paths
- WHEN the manifest preview is generated
- THEN each entry MUST include normalized relative path, media type, size, and hash metadata

#### Scenario: Reject unsafe upload path

- GIVEN an uploaded entry has an absolute path, parent traversal, or sensitive local path indicator
- WHEN the user publishes it
- THEN the system MUST reject it and MUST NOT persist unsafe local path data

### Requirement: Plugin and Theme JSON Handling

Plugin and theme formats MUST preserve expected JSON/metadata semantics while remaining approval-gated.

#### Scenario: Publish plugin metadata

- GIVEN a user selects Claude Code plugin format
- WHEN the manifest is previewed
- THEN plugin JSON metadata MUST be represented as a plugin component
- AND security MUST indicate approval is required

#### Scenario: Publish theme JSON

- GIVEN a user selects theme format
- WHEN the manifest is previewed
- THEN theme JSON MUST be represented as a theme component
- AND invalid JSON MUST be rejected before publish

## MODIFIED Requirements

### Requirement: Harness Catalog Visibility

The system MUST store harnesses under an organization with optional project scope and first-class owner user metadata, and MUST list only harnesses visible to the requesting user with owner display and filter support.
(Previously: Harness listing only required org/project visibility and did not require owner metadata.)

#### Scenario: List visible harnesses

- GIVEN a user has harness read access in an organization
- WHEN the user lists harnesses
- THEN org-wide harnesses are returned with owner metadata
- AND project-scoped harnesses are returned only for projects the user can access

#### Scenario: Hide inaccessible project harness

- GIVEN a harness is scoped to a project the user cannot access
- WHEN the user lists or inspects harnesses
- THEN that harness MUST NOT be returned

#### Scenario: Filter by owner

- GIVEN visible harnesses have different owner users
- WHEN the user filters by owner
- THEN only visible harnesses for that owner MUST be returned

### Requirement: Versioned Immutable Manifests

The system MUST publish harness versions as immutable typed manifests with version, manifest hash, provenance, status, supported format, components, security metadata, and compatibility targets for Claude, Codex, or OpenCode.
(Previously: Versions required generic manifest metadata without typed format/component structure.)

#### Scenario: Publish a harness version

- GIVEN an authorized user submits a valid typed manifest
- WHEN the version is published
- THEN the response includes the version identifier and manifest hash
- AND later downloads for that version return the same hash

#### Scenario: Reject invalid manifest

- GIVEN a manifest is missing required provenance, format, components, or compatibility targets
- WHEN the user publishes it
- THEN the system MUST reject the request with validation details
