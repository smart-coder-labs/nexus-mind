# Delta for Harness Library

## MODIFIED Requirements

### Requirement: Versioned Immutable Manifests

The system MUST publish harness versions as immutable typed manifests with version, manifest hash, provenance, status, supported format, components, security metadata, and compatibility targets for Claude, Codex, or Cursor.
(Previously: compatibility targets were Claude, Codex, or OpenCode.)

#### Scenario: Publish a harness version

- GIVEN an authorized user submits a valid typed manifest
- WHEN the version is published
- THEN the response includes the version identifier and manifest hash
- AND later downloads for that version return the same hash

#### Scenario: Reject invalid manifest

- GIVEN a manifest is missing required provenance, format, components, or compatibility targets
- WHEN the user publishes it
- THEN the system MUST reject the request with validation details

#### Scenario: Accept cursor as a valid target

- GIVEN an authorized user submits a manifest with `targets` including `cursor`
- WHEN the version is published
- THEN the system MUST accept `cursor` as a valid compatibility target
- AND the published version reflects `cursor` in its targets list

#### Scenario: Reject opencode as a target

- GIVEN an authorized user submits a manifest with `targets` including `opencode`
- WHEN the version is published
- THEN the system MUST reject the request with validation details naming `opencode` as an unsupported target
