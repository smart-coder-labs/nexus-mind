# Fix harness targets and instruction validation

## Goal / What

Resolve the harness creation/agent/workflow regressions reported after release
`781ee89`: target selection absent, Markdown hidden by executor, uploads leaving
empty drafts on rejection, and legitimate code-review instructions rejected as
credentials or oversized text.

## Accomplished / Why

- Create/publish forms select Claude, Codex, Cursor and Pi targets. Selected
  targets, not a hardcoded Claude target, are used in the submitted manifest.
- Agent selectors list visible harnesses without an executor filter. Pure
  Markdown is delivered as non-authoritative guidance to every executor, including
  Nexus; no native plugin/tool permission is granted. Mixed/executable bundles
  retain target checks. Hash pins, per-executor approvals and visibility remain.
- `POST /v1/harnesses` accepts optional `initial_version` and creates metadata and
  its first version in one transaction. Validation/publish failures roll back
  both; existing create-without-version and standalone publishing remain supported.
  Initial publishing also retains the existing project/owner visibility check.
- The browser builds the upload before submitting, keeps it selected on rejection,
  and explains the failure without claiming a partially created harness.
- Workflow instructions and harness files use the same bounded content validator.
  Plain mentions of Bearer tokens/authorization and explicit placeholders are not
  credentials. Literal authorization headers, credential assignments and known
  key formats remain blocked. No contents or detected secret values are echoed.
- Workflow instructions now support up to 64 KiB (the existing manifest-file
  ceiling), rather than 16,000 bytes. Size and credential rejection have distinct
  error codes and messages; content is not truncated.

## Discoveries / Learned

- The old workflow predicate matched `bearer ` and `authorization:` anywhere, so
  ordinary security review guidance failed without containing a secret.
- The old content predicate also treated a lowercase API route `/users/...` as a
  private macOS home path. Canonical private `/Users/...` paths remain rejected.
- Explicit `Authorization: Bearer <token>.` documentation needs punctuation-aware
  placeholder recognition; real `Authorization: Bearer x` is still rejected.
- Workflow resource protocol v2 is required for inline Markdown guidance. New
  steps require `agent-runtime:v2`; updated runners advertise v1 and v2 so old
  runners cannot silently consume and ignore new Markdown plans.
- Previously created empty drafts are not deleted or overwritten automatically.
  Their owner can use Publish version to attach the corrected/revalidated upload.

## Validation

- 67 distinct targeted Rust tests passed (harness APIs, rollback, visibility,
  approvals, portability, workflow bindings/limits, scanner and redaction).
- 29 targeted UI tests passed; production admin build passed.
- Python suite: 96 tests run, 84 passed, 12 environment-dependent skips locally.
- The reported uploaded Markdown was not retained by the rejected publication;
  reproduction uses security-review text and secret fixtures, not a claim to have
  inspected that user's original file.

## Next Steps

These fixes are **not deployed** yet. Release backend/worker before admin, then
activate the v2-capable runner. No migration is required; do not mutate production
agents, credentials or existing empty drafts automatically. Existing isolated
runner MCP/proxy provisioning requirements still apply.

## Relevant Files / Where

- `apps/admin/src/pages/Harnesses.tsx` — target selection and atomic upload request.
- `apps/backend/src/api/harnesses.rs` — atomic initial publication.
- `apps/admin/src/pages/factory/AgentRuntimeEditor.tsx` — unfiltered Markdown selection.
- `apps/backend/src/automation/agent_runtime.rs` — portable data-only guidance.
- `apps/backend/src/models/types.rs` — shared bounded credential detection.
- `apps/backend/src/db/workflow_runtime.rs` — instruction validation and limits.
- `apps/backend/src/api/workflows.rs` — separate actionable error messages.
- `scripts/factory/execution.py`, `runner.py` — guidance transport and protocol v2.
