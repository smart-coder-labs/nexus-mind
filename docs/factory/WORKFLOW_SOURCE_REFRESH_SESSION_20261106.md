# Session: refresh workflow sources before pinning snapshots

## Goal
User approved fixing stale sources automatically before new workflow executions.
Preserve original dirty clones and immutable commits/evidence for retries.
User subsequently requested deployment; preflight was performed but merge policy
and private-source access require operator authorization.

## Accomplished / What / Where
- Isolated worktree `/tmp/nexusmind-workflow-fresh-sources`, branch
  `fix/workflow-fresh-sources`, rebased on main `110e232`.
- Pushed commit `57566ce`; PR
  https://github.com/smart-coder-labs/nexus-mind/pull/416 .
- New ordinary/API/intake runs require confirmed branch metadata and fetch each
  authorized branch into disposable clones before other phases can start.
- Backend seals the initial snapshot once, under the fenced preparation lease and
  matching uploaded evidence. Retries/ambiguous completion reuse pinned commits.
- Explicit offline/exact-inventory opt-in `pinned_sources:true`; never a silent
  fallback. Existing snapshots remain untouched. A task-body field cannot change policy.
- Source read credentials are operator-provided private files scoped to a host and
  workspace, never inherited from model/backend/runner registration credentials.
- No new database migration, production deployment, credential copy, branch deletion
  or real workflow execution/retry was performed.

## Validation / Discoveries / Learned
- Python: 99 tests, OK, 12 skipped. Includes native bare Git, upstream updates,
  non-main/master branch, dirty originals intact, retry stability, fail-closed
  refresh and credential isolation.
- Admin: 19 focused tests and production build passed.
- Initial backend fenced-source regression passed. Full integrations then found
  an offline local-repository fixture now trying to refresh a missing remote;
  corrected via explicit pinned mode, with a regression prohibiting ordinary new
  runs from silently using a legacy inventory.
- Final local backend rebuild did not finish: first unlimited run stalled the
  host; later bounded runs used 5 GiB / no swap and were stopped. The final stop
  exited 137 (operator termination, not proof of a Rust failure). Final backend
  tests required CI validation; the subsequent full CI run is green.
- Removed only this worktree's stale index.lock after checking no holder with
  fuser; no global Git policy or unrelated worktree was changed.
- CI for PR416: https://github.com/smart-coder-labs/nexus-mind/actions/runs/42212909439 .
  **Backend build, full tests and Clippy passed** (12m19); Admin passed (1m14).
  This validates the latest commit, including the corrected offline fixture and
  ordinary-run fresh-default regression. **E2E smoke also passed** (7m53).
  All applicable checks are green for `57566cefb1abc34c2d5a5205f259d6f66cff589f`.
  PR416 was marked ready for review. It initially remained blocked by normal
  merge requirements; the subsequent explicitly authorized merge is recorded below.

## Deployment preflight (user requested deployment)

PR416 has all applicable CI checks green but `REVIEW_REQUIRED` and no approvals.
Ordinary `gh pr merge --merge --match-head-commit 57566ce...` was rejected by
branch policy. Administrator bypass was **not** used; ask for explicit approval
or obtain the required review. Branch auto-deletion is disabled, and no branch
was deleted. A merge to main triggers Deploy automatically; no dispatch is needed.

Oracle preflight: root filesystem about 89% used, 22 GiB free; runner service is
active on `runner-5b05955`. There is **no source_credentials profile** for the
Kasymir workspace. Do not silently copy credentials from backend/model/runner
registration. A read profile must be authorized/configured before claiming private
source refresh is operational. No deployment, migration runner, source switch,
private credential copy or workflow retry occurred during this attempt.

## Authorized deployment completed

User explicitly replied `dale` to the administrative-merge authorization question.
PR416 was merged administratively, without changing protections or deleting the
branch, as main **80e96d8a9de69e430ea09fe2e3b4ba55cf99aab4**, at
2026-11-06 14:07:13 UTC. Deploy started automatically:
https://github.com/smart-coder-labs/nexus-mind/actions/runs/42231093794 .

Verified private pre-deployment backup:
`/data/maintenance-backups/pre-source-refresh-80e96d8-20261106T140918Z.db`,
1,916,174,336 bytes, SHA256
`2adf348a01e7672a163592241fa1353f9362c84b611ab878479bb1cf47bfadd9`.
Schema is already 97; no migration is needed for this patch or was invoked.
No active workflow steps or autonomous-agent executions at initial preflight.
Node DiskPressure=false; about 20 GiB remained after the backup.

Staged exact merge scripts at `/home/ubuntu/.local/lib/nexusmind/runner-80e96d8`
with private root and group/other write bits removed, syntax checked, and all
10 runner tests passed on Oracle. Saved previous service/deployment manifests
privately under `/home/ubuntu/.local/state/nexusmind/deployments/80e96d8`.
Deploy **42231093794 completed successfully**, all seven jobs green.
Verified backend/worker live image matches the build digest exactly:
`ghcr.io/smart-coder-labs/nexusmind-backend@sha256:5c3c0202424d8545049ddcfd69fbc27b526a1d2d88a3b169ef0a6fb987fc4313`.
Backend pod `nexusmind-backend-79868f97f8-dxg2w` is 2/2 ready; egress proxy ready.
Public API health is HTTP 200, status/db OK. GitHub publication/backend model
organization grants and autonomous-agent enablement were verified preserved.

Activated the new runner after backend rollout, under its existing runner.lock,
using `95-source-refresh.conf` (ExecStart only; other service controls preserved).
Service active, source capability `workspace-sources:v1` verified in live runner
metadata, SDK recipe mode 0644, runtime revision 23 remains ready with no error.
No running workflow phase at final verification; no workflow was started/retried.

Public admin assets verified with curl: `/assets/index-Dplf8rux.js` references
`WorkflowsPage-Dnk2hamq.js`, whose served contents include the new source-branch UI.
Python urllib's default user-agent hit a Cloudflare 403; curl/browser user-agent
retrieval succeeded. This was not a failed Pages deployment.

Runner published inventory revision **2** with named branches; confirmation stays
at revision **1** pending user confirmation. Selection/write scope was not silently
changed. Final root headroom about 15 GiB (93% used), DiskPressure=false; monitor
space before large new source clones. No images/volumes/backups were pruned.

Read-only discovery found all 49 repositories on named branches: 15 master,
29 main, 5 develop. Source read profile remains absent; no credential was copied.
Do not claim private Git fetch works until an operator authorizes/configures it.

## Next Steps
1. Deployment and validation are complete. Do not rebuild/redeploy needlessly.
2. Confirm inventory revision 2 once, preserving the selected repository/write scope.
   New ordinary runs reject old branch-less inventory rather than recreating stale
   snapshots; the currently unconfirmed inventory deliberately prevents new runs.
3. Private repositories require an explicitly approved read credential profile;
   none has been provisioned in Oracle. Never borrow a registration/model token.
4. Create a new run after rollout; do not rewrite or blindly retry the old Accounting
   snapshot pinned to ac8c4f7. GitHub master already contains the corrected lockfile.
5. Separate pending tasks remain: automatic reviewer hook setup has unvalidated
   local changes in `/tmp/nexusmind-reviewer-auto-hooks`; reviewer main/master
   fallback remains uncorrected; full workflow observability is plan-only.

## NexusMind persistence

Saved through the official `POST /v1/memory/store` API with this session's existing
NexusMind API credential (MCP tools were not exposed), project `nexus-mind`:
- Bugfix: `9f649381-ec7c-46b8-979e-c1bed959adce` (HTTP 201).
- Session summary: `bcd11346-6a62-40d4-bf84-5b82ce80f3fe` (HTTP 201).
No credential value was printed or copied into Git source access.

## Relevant Files
- PR416: `scripts/factory/runner.py`, `test_runner.py`.
- Backend `factory/workflows.rs`, `db/workflows.rs`, `db/workflow_execution.rs`,
  `api/workflows.rs`, fixture updates and workflow integration tests.
- Admin `RepositoryInventory.tsx`, its test, workflow `types.ts`.
- PR documentation `docs/factory/WORKFLOW_SOURCE_REFRESH.md`.
- Root planning record `docs/factory/WORKFLOW_EXECUTION_OBSERVABILITY_PLAN.md`.
