# Session: diagnose workflow using stale Accounting inventory

## Goal
Explain why the latest workflow failed after the user integrated the Accounting
lockfile correction into master. Read-only investigation; no retry authorized.

## Accomplished / What
Latest run `48948dd7-d2dd-46c7-a0a5-aa2583c35777`, created 2026-11-06 09:58:49 UTC,
blocked at `verify`, attempt 1, candidate revision 1, at 10:19:57 UTC with the same
workspace-installation error. Its Accounting snapshot and execution heads still
pin `ac8c4f70689a7387ee7ba2f54e9b1f6a39580c11`.

## Discoveries / Why / Where
- GitHub master is now `55845405ef63b11ca6e42bd14e764c3497647b32`.
- Read that commit's package-lock via GitHub: 954 entries, **zero** unsupported
  local/link entries. The user's master correction is present; do not tell them
  it still needs merging. A cherry-pick can diverge from e8c77a6 ancestry while
  containing the actual fix, so compare content, not ancestry alone.
- Workspace `f0bec300-6964-4e81-aaa7-bed0653621e6` still uses inventory revision 1,
  confirmed revision 1. Its Accounting head is the old `ac8c4f7`.
- Oracle's actual source checkout is on master at `ac8c4f7`, with zero dirty files.
  Creating another run from that inventory continues pinning the old lockfile.
- The new run's implementation selected GraphQL and changed its SHA, not Accounting.
  Verification failed during npm dependency preparation, before the checks ran.

## Next Steps / Learned
Synchronize the authorized source checkout to the corrected upstream commit using
a non-destructive fast-forward, rediscover and confirm the updated inventory, then
create an execution that actually pins the corrected SHA. Inspect existing work
and concurrent runs before doing so. Do not force-reset local changes, rewrite
an existing immutable snapshot, carry approvals across revisions or restart paid
agent work automatically. No checkout, inventory, approval or run was changed.

## Relevant Files
- This document records the investigation.
- `scripts/factory/runner.py` and backend workspace inventory/snapshot handling:
  distinguish local source refresh from retrying an already pinned run.
- Existing Accounting `package-lock.json` fix on GitHub master is valid for the
  runner's unsupported-path check; no further lockfile edit is demonstrated here.
