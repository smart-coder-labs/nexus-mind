# Delta for Knowledge Migration — Claude Code Connector

## ADDED Requirements

### Requirement: Local Memory Files Are Read With Their Declared Type

The connector MUST parse the local memory format — frontmatter carrying a name, a description and a type, followed by the body — and MUST use the declared type as the primary signal for the proposed destination rather than re-deriving it.

#### Scenario: A typed memory keeps its type

- GIVEN a local memory file whose frontmatter declares a type
- WHEN the connector scans it
- THEN the proposed destination reflects that declared type
- AND the body becomes the candidate's content

#### Scenario: A memory with no frontmatter is still scanned

- GIVEN a memory file with no frontmatter
- WHEN the connector scans it
- THEN it still produces a candidate
- AND the proposal falls back to a generic knowledge destination

### Requirement: Personal Preferences Do Not Become Team Conventions

A memory declared as a personal preference MUST be proposed as a personal-scoped memory and MUST NOT be proposed as a team convention. Promoting it MUST require an explicit human action.

#### Scenario: A personal preference stays personal

- GIVEN a memory whose declared type marks it as a user preference
- WHEN the connector proposes a destination
- THEN the destination is a memory scoped to the individual
- AND it is not a convention

### Requirement: Agent Instruction Files Become Conventions

Files that instruct an agent how to behave — the project and global instruction files, and equivalent rule files for other tools — MUST be proposed as conventions rather than memories. They are rules the team follows, not observations about the work.

#### Scenario: An instruction file proposes a convention

- GIVEN a repository-level agent instruction file
- WHEN the connector scans it
- THEN each of its sections is proposed as a convention

### Requirement: Executable And Installable Assets Become Typed Harnesses

Skills, agents, commands, hooks, output styles, plugins and themes MUST be proposed as harness candidates carrying a manifest whose format matches the asset, and MUST NOT be proposed as memories.

A manifest the system would reject MUST fail its own candidate and MUST NOT be submitted.

#### Scenario: Each asset maps to its own format

- GIVEN a local agent definition, a skill directory, a command, a hook script and an output style
- WHEN the connector scans them
- THEN each produces a harness candidate whose declared format matches the asset kind
- AND none of them produces a memory

#### Scenario: An executable asset is marked as such

- GIVEN a hook script
- WHEN a harness candidate is produced for it
- THEN the manifest marks the asset as executable and as requiring approval

#### Scenario: A manifest that would be rejected fails locally

- GIVEN an asset whose manifest cannot satisfy the harness validator
- WHEN the connector produces candidates
- THEN that asset's candidate is reported as failed with its reason
- AND the rest of the scan continues

### Requirement: Paths In Manifests Are Relative To The Asset Root

Every path a manifest carries MUST be relative. An absolute path, or one carrying a user's home directory, MUST NOT reach a manifest.

#### Scenario: A home directory never reaches a manifest

- GIVEN assets discovered under a user's home directory
- WHEN manifests are produced
- THEN every component path is relative to the asset root
- AND no path contains the user's home directory

### Requirement: Content Is Redacted Before It Leaves The Machine

The connector MUST redact machine- and user-identifying material — home directory paths, credentials, connection strings — from every candidate before it is submitted, and MUST report what was redacted.

Redaction MUST happen before staging, not before commit: sensitive material must never reach the review queue.

Redaction MUST NOT alter any part of the content it does not replace: whitespace, indentation and line endings survive byte for byte.

#### Scenario: A home path is redacted, not shipped

- GIVEN content containing an absolute path under a user's home directory
- WHEN a candidate is produced
- THEN the submitted content carries a redacted placeholder instead
- AND the redaction is reported

#### Scenario: A credential is redacted

- GIVEN content containing a credential-shaped token
- WHEN a candidate is produced
- THEN the token does not appear in the submitted content

#### Scenario: Untouched content survives exactly

- GIVEN a shell script with indentation and a trailing newline and nothing to redact
- WHEN it passes through redaction
- THEN the result is byte-for-byte identical to the input

### Requirement: Configuration Carrying Secrets Goes To Config Review

Tool configuration files that hold credentials MUST be proposed as redacted configuration reviews carrying a redaction report, and MUST NOT be proposed as harness versions.

#### Scenario: Settings become a config review

- GIVEN a tool settings file containing an environment variable that holds a credential
- WHEN the connector scans it
- THEN it proposes a configuration review carrying the redacted configuration and a report of what was removed
- AND it does not propose a harness version

### Requirement: Third-Party Assets Are Never Republished

Assets obtained from a marketplace or a plugin cache MUST NOT produce candidates of any kind. This exclusion MUST NOT be overridable.

#### Scenario: Cached third-party plugins are skipped

- GIVEN skills and agents inside a plugin cache directory
- WHEN the connector scans
- THEN no candidate is produced from them
- AND they are reported as excluded with the reason

#### Scenario: The exclusion cannot be switched off

- GIVEN an operator supplying options that would widen the scan
- WHEN the connector scans
- THEN plugin-cache assets remain excluded

### Requirement: Links Between Memories Are Preserved

Where the local format links one memory to another, the connector MUST record those links on the candidate so they can be materialised later.

#### Scenario: A link is carried on the candidate

- GIVEN a memory whose body links to another memory by name
- WHEN a candidate is produced
- THEN the candidate records the linked name

### Requirement: Session Transcripts Are Out Of Scope

The connector MUST NOT read session transcripts.

#### Scenario: Transcripts are not scanned

- GIVEN transcript files alongside the memory files
- WHEN the connector scans
- THEN no candidate is produced from them
- AND they are reported as excluded rather than omitted silently
