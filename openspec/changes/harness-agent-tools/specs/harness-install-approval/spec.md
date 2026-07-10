# Delta for Harness Install Approval

## MODIFIED Requirements

### Requirement: Explicit Approval Before Download or Install

The system MUST require explicit user approval, policy checks, executable/plugin warning acknowledgement, and immutable manifest-hash confirmation before any download intended for installation or any installation state transition. When the calling client is the `nexusmind-mcp` harness installer, the same approval, diff-before-write, and acknowledgement rules apply with no relaxed path.
(Previously: did not explicitly reaffirm the MCP installer as an enforcing client of the same rules.)

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

#### Scenario: MCP installer enforces diff-before-write

- GIVEN the `nexusmind-mcp` harness installer is the requesting client
- WHEN it calls `plan_harness_install` and then `apply_harness_install`
- THEN the diff MUST be produced and returned before any write occurs
- AND `apply_harness_install` MUST require the same manifest-hash confirmation and warning acknowledgement as any other client

#### Scenario: MCP installer cannot bypass acknowledgement for executable formats

- GIVEN a plan includes a `hook` or `claude_code_plugin` component
- WHEN the `nexusmind-mcp` installer calls `apply_harness_install` without the acknowledgement flag
- THEN the system MUST reject the install transition
- AND MUST NOT treat the MCP client as exempt from the executable/plugin warning requirement

### Requirement: Backend Must Not Mutate Local Config

The system MUST NOT write to Claude, Codex, Cursor, shell profiles, uploaded folders, plugin directories, theme files, or local project files. Local tools MUST perform diff preview and apply only after user confirmation.
(Previously: enumerated OpenCode instead of Cursor among the local surfaces the backend must not mutate.)

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

#### Scenario: Backend never writes Cursor files directly

- GIVEN an agent session requests installation targeting Cursor
- WHEN the request reaches the backend
- THEN the backend MUST return manifest and approval state only
- AND MUST NOT write to any `.cursor/` path itself
