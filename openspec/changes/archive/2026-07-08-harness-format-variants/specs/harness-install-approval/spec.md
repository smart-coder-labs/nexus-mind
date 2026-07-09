# Delta for Harness Install Approval

## MODIFIED Requirements

### Requirement: Recommendation Without Installation

The system MAY recommend relevant harnesses to agents or users, but recommendations MUST expose metadata only until explicit approval is granted, including format, owner, executable/plugin warning metadata, and required permissions.
(Previously: Recommendations exposed metadata and permissions only, without format/owner/executable warning metadata.)

#### Scenario: Recommend matching harness

- GIVEN a relevant published harness exists
- WHEN an agent requests recommendations
- THEN the system returns harness metadata, owner, format, warning metadata, and required permissions
- AND does not return installable manifest content by default

#### Scenario: No accessible recommendation

- GIVEN matching harnesses exist outside the user's access scope
- WHEN recommendations are requested
- THEN inaccessible harnesses MUST NOT be recommended

### Requirement: Explicit Approval Before Download or Install

The system MUST require explicit user approval, policy checks, executable/plugin warning acknowledgement, and immutable manifest-hash confirmation before any download intended for installation or any installation state transition.
(Previously: Approval required user approval, policy checks, and manifest-hash confirmation, but not executable/plugin warning acknowledgement.)

#### Scenario: Approve installation candidate

- GIVEN a user can access a published harness version
- WHEN the user approves installation for a target tool and scope
- THEN the approval records the user, target tool, target scope, version, manifest hash, and warning acknowledgement when required
- AND the approval can be audited later

#### Scenario: Hash mismatch blocks install

- GIVEN an approval references one manifest hash
- WHEN a client reports a different manifest hash for installation
- THEN the system MUST reject the install transition
- AND require a new approval for the new hash

#### Scenario: Executable hook requires warning

- GIVEN a hook or plugin manifest is executable or high-trust
- WHEN approval is requested
- THEN the system MUST present an executable/plugin warning before download or install approval

### Requirement: Backend Must Not Mutate Local Config

The system MUST NOT write to Claude, Codex, OpenCode, shell profiles, uploaded folders, plugin directories, theme files, or local project files. Local tools MUST perform diff preview and apply only after user confirmation.
(Previously: The backend mutation prohibition listed tool/project files, but not uploaded folders, plugin directories, or theme files.)

#### Scenario: Request local mutation from backend

- GIVEN a caller asks NexusMind to apply local config changes
- WHEN the request reaches the backend
- THEN the system MUST reject or omit the mutation operation
- AND return only manifest and approval state information

#### Scenario: Record local install result

- GIVEN a local tool has applied changes after confirmation
- WHEN it reports installation status
- THEN the system records status for the approved version
- AND does not require or store raw local file contents
