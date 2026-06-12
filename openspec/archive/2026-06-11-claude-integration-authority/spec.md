# Spec: claude-integration-authority

## Scope

Delta for capabilities:

**New**: `mcp-tool-authority`, `mcp-memory-fetch`, `mcp-memory-delete`, `claude-protocol-doc`, `prompt-injection-protocol`, `session-bootstrap-protocol`, `subagent-capture-gate`, `mcp-config-portability`

**Modified**: None at the spec level (existing tool signatures, hook contracts, and HTTP API are preserved).

**Removed**: None.

---

# mcp-tool-authority (New)

## Purpose

Every MCP tool description shipped by `@smart-coder-labs/nexusmind-mcp` MUST be written as a behavioral mandate that Claude reads as part of its system prompt. Descriptions answer "WHEN should I call this?" and "WHAT must I always pass?" — not just "what does this do?".

## Requirements

### Requirement: store_memory Description as Mandate

The `store_memory` tool description MUST begin with an imperative verb (`ALWAYS`, `CALL`, etc.) and MUST contain:
- A proactive trigger clause ("immediately after ANY decision, bug fix, convention, or non-obvious discovery").
- An anti-procrastination clause ("do NOT wait to be asked").
- A mandatory-fields clause naming `title`, `type`, `project` as required-in-practice.
- A sequencing clause ("call this before moving to the next task").

The description MUST be 60 words or fewer.

#### Scenario: Tool description contains mandate keywords

- GIVEN the MCP server is built and registered with Claude
- WHEN Claude reads the `store_memory` tool description from the registry
- THEN the description contains the substring "ALWAYS"
- AND the description contains "do NOT wait"
- AND the description names `title`, `type`, `project` as mandatory in practice

### Requirement: search_memory Description as Mandate

The `search_memory` tool description MUST instruct Claude to call it:
- BEFORE starting any work that might have been done before.
- As the FIRST action when a user's message references a project, feature, bug, or module without prior context.
- When in doubt about prior work ("if unsure whether to search — search").

The description MUST be 60 words or fewer.

#### Scenario: First-message rule visible in description

- GIVEN the MCP server is registered
- WHEN Claude reads the `search_memory` description
- THEN the description states `search_memory` is the FIRST action on a user message that references a project/feature/bug
- AND the description states "if unsure whether to search — search"

### Requirement: get_context Description as Session-Start Mandate

The `get_context` tool description MUST instruct Claude to call it at the START of every session that involves significant work, and MUST state that it returns team knowledge grouped by type (architecture, decisions, patterns, bugs fixed).

#### Scenario: Session-start mandate visible

- GIVEN the MCP server is registered
- WHEN Claude reads the `get_context` description
- THEN the description states `get_context` is called at the START of every significant session
- AND the description names "architecture, decisions, patterns, bugs fixed" as the categories returned

### Requirement: list_memories Description as Utility

The `list_memories` tool description MUST be marked as a utility/browse tool and MUST NOT carry a mandatory call clause (it is the fallback for `search_memory`).

#### Scenario: list_memories framed as utility

- GIVEN the MCP server is registered
- WHEN Claude reads the `list_memories` description
- THEN the description does NOT contain "ALWAYS" or "MUST"
- AND the description references `search_memory` as the preferred targeted call

---

# mcp-memory-fetch (New)

## Purpose

Expose a tool that returns the FULL untruncated content of a single memory by ID. Search and list results are previews (often truncated to 120-300 chars); when Claude needs to act on the full record, this tool is the only path.

## Requirements

### Requirement: get_memory Tool Registered

The MCP server MUST register a tool named `get_memory` with the following signature:
- `id` (string, required): the memory ID.

The tool MUST call `getMemoryById(id)` against `GET /v1/memory/:id`. The tool MUST return the full memory record (title, content, type, project, tool, scope, tags, topic_key, created_at, revision_count) as a single formatted text block.

The tool description MUST instruct Claude to use this whenever a `search_memory` or `list_memories` preview is not enough to answer.

#### Scenario: Fetch full memory by id

- GIVEN a memory with `id = "m1"` exists for the authenticated tenant
- AND its content is 5000 characters long
- WHEN Claude calls `get_memory(id: "m1")`
- THEN the response contains the full 5000-character content
- AND the response includes title, type, project, scope, and created_at

#### Scenario: Unknown id returns error

- GIVEN no memory with `id = "missing"` exists for the tenant
- WHEN Claude calls `get_memory(id: "missing")`
- THEN the response is `isError: true`
- AND the response text contains the upstream error message (404 / not found)

#### Scenario: ID belongs to another tenant

- GIVEN org A has memory `id = "m_a"`
- WHEN org B's Claude calls `get_memory(id: "m_a")`
- THEN the response is `isError: true` (backend returns 404 for cross-tenant)
- AND no memory data is leaked in the response

---

# mcp-memory-delete (New)

## Purpose

Allow Claude to remove a memory that is stale, incorrect, or explicitly retired by the user. Because deletion is destructive and Claude is autonomous, the tool MUST require explicit confirmation.

## Requirements

### Requirement: delete_memory Tool Registered with Confirmation Gate

The MCP server MUST register a tool named `delete_memory` with the following signature:
- `id` (string, required): the memory ID to delete.
- `confirm` (boolean, required): MUST be `true` to perform the deletion.

The tool description MUST state:
- The USER must request deletion explicitly; Claude MUST NOT delete memories autonomously.
- `confirm: true` is mandatory; calling without it returns an error.
- Deletion is permanent (backend hard-deletes); no soft-delete or undo.

The tool MUST call `deleteMemory(id)` against `DELETE /v1/memory/:id` only when `confirm === true`.

#### Scenario: Delete with confirmation

- GIVEN a memory with `id = "m1"` exists
- WHEN Claude calls `delete_memory(id: "m1", confirm: true)`
- THEN the backend receives `DELETE /v1/memory/m1`
- AND the response is a success text block confirming deletion

#### Scenario: Refuse without confirmation

- GIVEN a memory with `id = "m1"` exists
- WHEN Claude calls `delete_memory(id: "m1", confirm: false)`
- THEN the tool returns `isError: true`
- AND no HTTP request is sent to the backend
- AND the response text explains that `confirm: true` is required

#### Scenario: Backend 404 surfaces as tool error

- GIVEN no memory with `id = "missing"` exists
- WHEN Claude calls `delete_memory(id: "missing", confirm: true)`
- THEN the response is `isError: true`
- AND the response text includes the backend error message

---

# claude-protocol-doc (New)

## Purpose

`nexus-mind/CLAUDE.md` MUST function as a **mandatory protocol document** that survives compaction and provides Claude with the same enforcement level the global Engram protocol provides for cross-session memory — but scoped to NexusMind tools and the `nexus-mind` project.

## Requirements

### Requirement: Proactive Save Triggers Section

`CLAUDE.md` MUST contain a section titled "PROACTIVE SAVE TRIGGERS" that lists conditions under which Claude MUST call `store_memory` without being asked. The list MUST include at minimum:
- Architecture or design decision made.
- Convention documented or established.
- Bug fix completed (with root cause).
- Feature implemented with non-obvious approach.
- Configuration or environment change.
- Non-obvious discovery about the codebase.
- Gotcha, edge case, or unexpected behavior.
- Pattern established (naming, structure).
- User preference or constraint learned.

The section MUST end with a self-check directive: "Did I make a decision, fix a bug, learn something non-obvious, or establish a convention? If yes, call store_memory NOW."

#### Scenario: Triggers section is present and complete

- GIVEN `nexus-mind/CLAUDE.md` is read
- WHEN the "PROACTIVE SAVE TRIGGERS" section is parsed
- THEN at least 9 trigger conditions are listed
- AND the self-check directive is present

### Requirement: First-Message Search Rule

`CLAUDE.md` MUST contain an instruction that, on the user's FIRST message of a session, Claude MUST call `search_memory` with keywords from the message before answering, when the message references a project, feature, bug, or module.

#### Scenario: First-message rule documented

- GIVEN `CLAUDE.md` is read
- WHEN the "WHEN TO SEARCH" section is parsed
- THEN a "first message" rule is explicitly listed
- AND the rule says "before responding"

### Requirement: Type Glossary

`CLAUDE.md` MUST contain a glossary of all 13 memory types with a one-line definition each:
`architecture`, `bugfix`, `decision`, `discovery`, `config`, `pattern`, `feedback`, `preference`, `project`, `session_summary`, `feature`, `refactoring`, `manual`.

#### Scenario: All 13 types documented

- GIVEN `CLAUDE.md` is read
- WHEN the type glossary is parsed
- THEN all 13 type names appear
- AND each type has a one-line definition

### Requirement: topic_key Guidance

`CLAUDE.md` MUST document when to use `topic_key`:
- USE for evolving topics (architecture decisions that get revised, config that mutates, conventions that get refined).
- DO NOT use for one-shot records (a single bug fix, a session summary).
- Example keys: `architecture/auth-model`, `config/deploy-pipeline`, `pattern/repo-naming`.

#### Scenario: topic_key guidance present

- GIVEN `CLAUDE.md` is read
- WHEN the topic_key section is parsed
- THEN it includes both "USE" and "DO NOT use" guidance
- AND it includes at least 2 example keys

### Requirement: Session Close Mandatory Rule

`CLAUDE.md` MUST contain a "SESSION CLOSE (MANDATORY)" section that requires `store_memory(type: "session_summary")` BEFORE saying "done", "that's it", or the equivalent. The section MUST list the required content of a session summary: Goal, Accomplished, Discoveries, Next Steps, Relevant Files.

#### Scenario: Session close rule present

- GIVEN `CLAUDE.md` is read
- WHEN the session close section is parsed
- THEN the title is "SESSION CLOSE (MANDATORY)" or contains that phrase
- AND the section requires `type: "session_summary"`
- AND the section lists at least 4 required content fields

### Requirement: Post-Compaction Recovery Protocol

`CLAUDE.md` MUST contain a "AFTER COMPACTION" section that instructs Claude:
1. Immediately call `store_memory` with the compacted summary content (so it is preserved as a session_summary memory).
2. Call `search_memory` with project name to recover broader context.
3. Only THEN continue working.

#### Scenario: Post-compaction protocol present

- GIVEN `CLAUDE.md` is read
- WHEN the post-compaction section is parsed
- THEN the section lists 3 ordered steps
- AND the first step persists the compacted summary

### Requirement: Project Defaulting Rule

`CLAUDE.md` MUST state: "In this repo, every `store_memory` and `search_memory` call MUST pass `project='nexus-mind'` (or one of `nexusmind-backend`, `nexusmind-admin`, `nexusmind-mcp`, `nexusmind-landing` for cross-repo work)."

#### Scenario: Project default documented

- GIVEN `CLAUDE.md` is read
- WHEN the project rule is parsed
- THEN the doc names `nexus-mind` as the default project value
- AND lists the alternate per-app values

### Requirement: Single Source of Truth Statement

`CLAUDE.md` MUST contain the statement: "NexusMind is the single source of truth for this codebase. Before guessing, check it. Before finishing, save to it." This MUST appear in the document's opening section.

#### Scenario: Source-of-truth statement present

- GIVEN `CLAUDE.md` is read
- WHEN the opening section is parsed
- THEN the literal phrase "single source of truth" appears
- AND the phrase "Before guessing, check it. Before finishing, save to it." appears

---

# prompt-injection-protocol (New)

## Purpose

`user-prompt-submit.sh` in BOTH `nexusmind-mcp` and `nexusmind-claude-plugin` MUST inject a structured 5-part block on every user prompt (not only the first one and not only periodically). The injection is what Claude treats as instructions immediately above the user's message.

## Requirements

### Requirement: 5-Part Injection Block on Every Prompt

Each `user-prompt-submit.sh` hook MUST emit a single JSON object via stdout with a `systemMessage` (or equivalent additionalContext) on every invocation, formatted as a structured 5-part block in this order:

1. **Session context**: the last 5 memories for the user (recency order), as a fenced code block tagged `nexusmind-recent`.
2. **Project-specific context**: the last 5 memories matching the detected project name (via `POST /v1/memory/search { query: $PROJECT, limit: 5 }`), as a fenced code block tagged `nexusmind-project`.
3. **Behavioral mandate**: the literal text "MANDATORY: call `search_memory` with keywords from this message before responding if the message references existing work. Save any decision you make to NexusMind. Do not skip this."
4. **Save reminder**: "After completing any decision, bug fix, or non-obvious discovery, call `store_memory` BEFORE moving on."
5. **Format hint**: "When you call `store_memory`, always set `type`, always set `title`, always set `project`."

The hook MUST NOT use the previous "first-call only + 15-minute periodic" gate. The 5-part block runs on every prompt.

#### Scenario: Injection on first prompt

- GIVEN a fresh session with `NEXUSMIND_API_KEY` set
- WHEN the first user prompt arrives
- THEN the hook emits a JSON object with a single field (systemMessage or additionalContext)
- AND that value contains the 5 numbered sections
- AND sections 1 and 2 are fenced code blocks

#### Scenario: Injection on every subsequent prompt

- GIVEN a session that has already had 10 prompts
- WHEN prompt 11 arrives
- THEN the hook emits the same 5-part structure (with refreshed memories)
- AND the block is not gated by the previous time-based logic

#### Scenario: Empty result handling

- GIVEN the backend returns 0 recent memories and 0 project-specific memories
- WHEN the hook runs
- THEN sections 1 and 2 contain the literal `(none)` placeholder
- AND sections 3, 4, 5 are still emitted verbatim

### Requirement: API Key Absence Skips Cleanly

If `NEXUSMIND_API_KEY` is unset, the hook MUST exit 0 with no stdout output (same as today). It MUST NOT emit a partially-formed JSON object.

#### Scenario: No API key, no output

- GIVEN `NEXUSMIND_API_KEY` is unset
- WHEN the hook runs
- THEN stdout is empty
- AND exit code is 0

---

# session-bootstrap-protocol (New)

## Purpose

`session-start.sh` in BOTH repos MUST emit additionalContext that is project-aware, type-labelled, and includes the full protocol body — not a one-line reminder.

## Requirements

### Requirement: Project-Specific Search at Session Start

`session-start.sh` MUST issue a `POST /v1/memory/search` request with `query=$PROJECT` and `limit=15` (in addition to the existing recency listing). The results MUST be merged with the recency listing such that:
- Project-specific memories appear FIRST under "### Project Memories — {PROJECT}".
- Recent memories (any project) appear SECOND under "### Recent Team Memories (last 10)".
- Duplicates (same id) appear only once, prioritized in the project section.

#### Scenario: Project memories returned

- GIVEN the detected project is `nexus-mind`
- AND there are 5 memories tagged `project: "nexus-mind"`
- WHEN `session-start.sh` runs
- THEN the output contains a section "Project Memories — nexus-mind"
- AND that section lists those 5 memories

#### Scenario: Project search empty

- GIVEN no memories match the detected project
- WHEN `session-start.sh` runs
- THEN the project section is omitted (no empty header)
- AND the recency section still appears if recency results exist

### Requirement: Type-Labelled Formatting

Each memory line in the output MUST use the format `- [{type}] {title or content snippet}` where `{type}` falls back to `general` when missing. Snippets MUST be truncated to 120 chars and newlines replaced with spaces.

#### Scenario: Format includes type label

- GIVEN a memory `{ type: "decision", title: "Use Rust 1.84" }`
- WHEN it is formatted by `session-start.sh`
- THEN the rendered line is `- [decision] Use Rust 1.84`

### Requirement: Full Protocol Body, Not One-Liner

`session-start.sh` MUST emit a full protocol block that includes ALL of:
- Project detection line.
- Core tools list (`store_memory`, `search_memory`, `list_memories`, `get_context`, `get_memory`, `delete_memory`).
- Proactive save rule (with type list).
- When-to-search rule.
- Session close rule.
- Post-compaction recovery rule.

The block MUST NOT be a single-line reminder. It MUST be self-contained so a Claude session that loses CLAUDE.md still has the protocol available.

#### Scenario: Full protocol block emitted

- GIVEN `session-start.sh` runs successfully
- WHEN its stdout is parsed
- THEN all 6 tools are named
- AND the proactive save rule contains "IMMEDIATELY"
- AND the post-compaction recovery rule is present

### Requirement: Health-Check Guard Preserved

The existing API-key-missing and backend-unreachable guards MUST remain. The new project search MUST NOT block the protocol output — if the search fails or returns an error, the protocol still emits and the project section is omitted.

#### Scenario: Backend unreachable

- GIVEN the backend health check fails
- WHEN `session-start.sh` runs
- THEN stdout contains the existing "NexusMind NOT CONNECTED" HTML comment
- AND no protocol body is emitted

#### Scenario: Project search fails but recency succeeds

- GIVEN the backend health check passes
- AND `POST /v1/memory/search` returns 500
- AND `GET /v1/memory?limit=15` returns 10 memories
- WHEN `session-start.sh` runs
- THEN the protocol body is emitted
- AND the project section is omitted
- AND the recency section is emitted

---

# subagent-capture-gate (New)

## Purpose

`subagent-stop.sh` in BOTH repos MUST be byte-identical and MUST apply a quality gate so only outputs that actually contain decision-like content are stored. This eliminates noise from agent runs that produce no learnings.

## Requirements

### Requirement: Decision-Keyword Quality Gate

Before storing a subagent output, the hook MUST check that the output (case-insensitive) contains at least one of:
`decided`, `decision`, `fixed`, `error`, `warning`, `convention`, `architecture`, `discovered`, `discovery`, `issue`, `solution`, `bug`, `gotcha`, `pattern`.

If none of those tokens appear, the hook MUST exit 0 without making a network call.

#### Scenario: Output with keyword is stored

- GIVEN subagent output contains the word "Fixed an N+1 query"
- WHEN the hook runs
- THEN a `POST /v1/memory/store` request is made
- AND the body contains the captured output

#### Scenario: Noise output is skipped

- GIVEN subagent output is "Done. See you next time."
- WHEN the hook runs
- THEN no HTTP request is made
- AND exit code is 0

#### Scenario: Very short output is skipped (existing behavior preserved)

- GIVEN subagent output is 20 characters long
- WHEN the hook runs
- THEN no HTTP request is made
- AND exit code is 0

### Requirement: Byte-Identical Across Both Repos

The `subagent-stop.sh` file in `nexusmind-mcp/plugin/scripts/` and the one in `nexusmind-claude-plugin/plugin/scripts/` MUST be byte-identical at all times.

#### Scenario: Files compare equal

- GIVEN both repos are at HEAD
- WHEN `diff` is run on the two `subagent-stop.sh` files
- THEN the output is empty
- AND the exit code is 0

### Requirement: Aligned Payload Shape

The stored memory MUST include `type: "discovery"`, `tool: "claude-code-subagent"`, the detected `project`, and the truncated content (max 2000 chars + `... [truncated]`). The previous divergence (one repo using `metadata.passive_capture`, the other omitting `type`) MUST be replaced by the same payload shape.

#### Scenario: Payload shape

- GIVEN a valid subagent output that passes the keyword gate
- WHEN the hook posts to `/v1/memory/store`
- THEN the JSON body contains `type: "discovery"`
- AND `tool: "claude-code-subagent"`
- AND `project: $PROJECT`
- AND `content` is the (possibly truncated) output

---

# mcp-config-portability (New)

## Purpose

`nexus-mind/.mcp.json` MUST be safe to commit, reproducible across machines, and free of secrets and machine-local paths.

## Requirements

### Requirement: No Hardcoded API Key

`.mcp.json` MUST reference the API key via `${NEXUSMIND_API_KEY}` placeholder. No literal `nm_*` value MAY appear in the file.

#### Scenario: No literal key in file

- GIVEN `nexus-mind/.mcp.json` at HEAD
- WHEN the file is scanned with `grep -E 'nm_[a-f0-9]{16,}'`
- THEN no match is found
- AND the file contains the literal `${NEXUSMIND_API_KEY}`

### Requirement: No Absolute Filesystem Path

`.mcp.json` MUST NOT contain absolute paths to a local checkout (e.g., `/Volumes/...`, `/Users/...`, `/home/...`). The MCP server MUST be launched via `npx -y @smart-coder-labs/nexusmind-mcp`.

#### Scenario: npx command used

- GIVEN `nexus-mind/.mcp.json`
- WHEN the `mcpServers.nexusmind` entry is parsed
- THEN `command` is `npx`
- AND `args` starts with `-y` and includes `@smart-coder-labs/nexusmind-mcp`
- AND no entry under that server starts with `/Volumes`, `/Users`, or `/home`

### Requirement: Documentation of Env Placeholder Convention

`nexus-mind/CLAUDE.md` MUST document that `.mcp.json` uses `${...}` env-var placeholders and that the user must set `NEXUSMIND_API_KEY` in their shell before launching Claude Code.

#### Scenario: Convention documented

- GIVEN `CLAUDE.md` at HEAD
- WHEN the "Configuration" section is parsed
- THEN it states `.mcp.json` uses env placeholders
- AND it names `NEXUSMIND_API_KEY` as the required env var
