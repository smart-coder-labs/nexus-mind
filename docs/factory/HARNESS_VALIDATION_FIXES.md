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

Deployed on 2026-11-06 UTC: application commit
`5b0595599c99f1995772eecf121708d7529a032e`, successful release
https://github.com/smart-coder-labs/nexus-mind/actions/runs/42184341900.
Backend/worker and egress image:
`ghcr.io/smart-coder-labs/nexusmind-backend@sha256:65bff536852c5ea1f722c6b736ea52cc49dcc2263faa5da05d39f517dcf47d59`.

- All seven jobs passed: 1,870 backend unit tests (4 ignored), 27 integration
  tests, 96 Python tests without skips, 146 focused admin tests and production build.
- Backend/worker 2/2 ready, zero restarts; egress proxy 1 ready.
  Public health is HTTP 200, status/db OK.
- Admin entrypoint `/assets/index-CMZOMO5X.js`; feature bundles match the release
  artifact byte-for-byte:
  - `Harnesses-D66rlm4L.js`: SHA-256 `522392600d68cca179fc8f7b5ffd2093ef94758f3fb1b4f5b2d9949fe90cc655`.
  - `AgentRuntimeEditor-DbB3n0Qo.js`: SHA-256 `2b3c51b972eca9a8594b8d3899f4dd6dae8d535eb48b58099f722a734a567f31`.
- Immutable Pages deployment: https://c1bb8be5.nexusmind-admin.pages.dev.
- Runner activated after backend/admin, under its `runner.lock` (no active phase
  killed): `/home/ubuntu/.local/lib/nexusmind/runner-5b05955/runner.py`, existing
  `90-agent-runtime.conf` drop-in. Service active, zero restarts. Core Python
  module hashes match Git source. Read-only backend verification confirms this
  runner advertises both `agent-runtime:v1` and `agent-runtime:v2`.
- Verified online SQLite backup before rollout:
  `/data/pre-workflows-20261106T080952Z.db`. No new schema migration or production
  migration job was required. Existing agents, credentials and drafts were not modified.

Refresh the admin to load the new client. Previously orphaned empty harnesses
can use Publish version to attach the original/corrected upload; do not recreate
or delete them automatically. The isolated runner still has no `nexusmind_mcp`
transport configured; steps requiring it remain explicitly blocked until scoped
operator provisioning. No live paid model execution was used as a smoke test.

Rollback source: application `781ee89`, image
`sha256:218644a02086c36a301544f88b421bf53a38edc31608cd50d326403b81cc487d`,
runner `/home/ubuntu/.local/lib/nexusmind/runner-781ee89/runner.py`. Do not restore
an old database over newer writes without an explicit recovery plan.

## Relevant Files / Where

- `apps/admin/src/pages/Harnesses.tsx` — target selection and atomic upload request.
- `apps/backend/src/api/harnesses.rs` — atomic initial publication.
- `apps/admin/src/pages/factory/AgentRuntimeEditor.tsx` — unfiltered Markdown selection.
- `apps/backend/src/automation/agent_runtime.rs` — portable data-only guidance.
- `apps/backend/src/models/types.rs` — shared bounded credential detection.
- `apps/backend/src/db/workflow_runtime.rs` — instruction validation and limits.
- `apps/backend/src/api/workflows.rs` — separate actionable error messages.
- `scripts/factory/execution.py`, `runner.py` — guidance transport and protocol v2.
