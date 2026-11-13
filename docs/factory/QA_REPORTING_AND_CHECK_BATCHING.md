# QA reporting and verification batching

## Less repeated work

- Adjacent checks share a disposable container/checkout and dependency installation only when repository, immutable image, environment references and network mode match.
- Command order and each check's evidence/status are preserved. Different isolation profiles remain separate. Batches are capped at 32 checks to stay within result limits.
- Failed dependency preparation is attempted once per repository per batch, not once per check. Infrastructure failures still hold execution.
- Dockerfile validation reports all missing ARG defaults before resolving images. This avoids spending one repair plus a full verification cycle per missing default.
- No mutable `node_modules` cache is shared across runs. No results are reused across changed SHAs, and build/tests/typechecking are not silently removed.

## QA findings reach the PR

The dedicated read-only `isolated-functional-qa`, `isolated-design-qa` and `isolated-security-qa` steps are report-only when they require their corresponding QA check. Actual failed assertions complete the phase with **findings**, retaining `fail` evidence and a backend-issued `qa_report_only` marker. They do not trigger repair or block the evidence gate.

This does **not** waive:

- Build/tests or independent code review.
- Missing artifacts, missing check results, stale commits or blocked evidence.
- Infrastructure errors, dependency installation failures, holds or authorization/security floors.
- Merge or deployment approval.

PR descriptions receive a bounded **QA findings (non-blocking)** section naming each failed phase/check and its report artifact, with the run ID/revision for full evidence. Publication retries update only this managed section, preserving human text and preventing duplicates. Publication fails if required annotation delivery fails. The admin UI displays **Completada con hallazgos QA**, not a QA pass.

## Validation / rollout

Run `python3 -m unittest discover -s scripts/factory -p 'test_*.py'` and the workflow/admin and Rust regression tests. The optional existing multi-repository Docker integration verifies four checks execute in two containers, retaining all evidence.

These changes require a new approved backend/admin and runner rollout. They are not deployed by editing this worktree. The earlier Kasymir validation run `f69578c1-3b38-4485-a30c-2c26f9a23935` exhausted repairs on individually reported Dockerfile ARG defaults; its three native verification passes remain recorded, and the blocked history is preserved.
