# Workflow autonomy validation — 2026-11-13

## Production inventory

- `origin/main`: `f41acd4`; Deploy run `43318049304` succeeded for backend, verification image and all three frontends.
- Both user-level Oracle runner services are active, using `runner-82322f1`. There is no `scripts/factory` difference between that commit and `f41acd4` before this fix.
- Existing open PR: #248 (Context Fabric). It is not an approved production change.
- The original `fix/admin-panel-tests` worktree is old, dirty and contains 75 files different from current main, plus directories not counted as files. Its original PR #322 was merged. Differences are NOT automatically pending features: many are stale copies of since-merged workflows. No original changes or branches were removed or deployed. A complete semantic reconciliation of that WIP remains outstanding.

## Applied production policy configuration

Through the versioned policies API, not SQL:

- `always`: fix, publish, open_pr, reply, close, approve, notify, recover.
- `manual`: merge, deploy, including the existing project-scoped Kasymir merge override.
- Deterministic security floors remain enforced. These settings do not grant credentials or authorize migration execution.

## Reproduced failures and fixes

Using the runner-owned checkout of `github:kasymir/kasymir-app-ui#737`, run `bc12609d-8c27-48b4-9d56-2819b9153b06`, head `8e5ce320dccaaffa08e417318e571b873bf3a52a`:

1. Its locked dependency tarballs total **1,578,145,400 bytes**, already larger than `/tmp`'s 1 GiB tmpfs, before npm extraction. Use disposable, disk-backed `/scratch`, mounted separately from source/input/output and removed with the step.
2. Dependency-install failures were returned as ordinary check failures. The suite discarded the child outcome entirely. Preserve an infrastructure hold, stop repeated installation attempts in that suite, and classify `node_dependency_install_failed` as infrastructure before model recovery.
3. The 2 GiB container gave V8 only about 1 GiB of heap: build and TypeScript aborted. Node steps now default to 6144 MiB, with the operator-only `node_memory_mb` knob (512–32768). Other steps retain 2048 MiB.
4. Webpack exceeded the 256 MiB file-size ceiling. Node steps retain a bounded 1 GiB ceiling; core dumps are disabled.
5. Next page-data collection exceeded 1024 open files. Node steps use 8192; other steps retain 1024.

## Actual isolated-container results

No source code was run on the host. The tests used the production pinned Node image, no network, no injected credentials, read-only root, unprivileged UID, dropped capabilities and disposable source copies. No production service was changed or restarted.

- Jest: **188 suites, 1676 tests passed; zero skips**. Includes the purchase quantity regression.
- TypeScript: **exit 0** with the adjusted Node budget.
- Full Next production build: **exit 0**, 141/141 static pages, after resolving heap, file-size and open-file limits.
- Python regression command: `python3 -m unittest discover -s scripts/factory -p 'test_*.py'`.
- Earlier exploratory sandbox timed out; its remaining container was explicitly removed. Subsequent checks name and stop their own container before removing scratch.

These are real verification results, **not** a successful end-to-end workflow run. No fabricated evidence was uploaded to the run.

## Phase relevance

Existing backend validation planning decides each configured check independently, records `validation.planned`, and permits server-planned skips. Changed code and dependency consumers retain deterministic checks; uncertainty runs the check. Agent prompts now explicitly assess applicability and partial scope before work, without treating missing dependencies/services/credentials as a valid skip or skipping review of a changed diff.

## Approval boundary / next validation

1. Human approval to merge this change and deploy the backend plus runner bundle. The backend change adds an infrastructure error classification; no migration is introduced.
2. Re-run the real intake workflow after deployment. #737 has already exhausted its original repair budget; inspect retry controls instead of fabricating/resetting persisted evidence. It is still blocked in production.
3. Continue through integrated environment, functional/design/security QA, evidence gate, PR publication and independent review; none is claimed validated by the standalone build/test checks above.
4. Exercise another task type (e.g. docs and accounting refactor #42), including justified phase/check skips and genuine failure recovery. Full autonomous behavior across task types is not yet certified.
5. Merge/deploy actions must remain waiting for human approval.
