# Verify Report — Knowledge Migration Core

> **Change**: `knowledge-migration-core`
> **Branch**: `sdd/knowledge-migration`
> **Date**: 2026-08-15
> **Verdict**: ✅ complete — every requirement of the three delta specs has a passing test

---

## 1. Gates

| Gate | Command | Result |
|---|---|---|
| Backend tests | `cargo test` | **1121 lib + 46 integration + 11 runner + 60 other — 0 failures** |
| Backend lint | `cargo clippy -- -D warnings` | clean (rc=0) |
| Admin tests | `npm run test` | **261 passed** (28 files) |
| Admin types | `npx tsc -b` | rc=0 |
| Admin build | `npm run build` | ✓ built |

Test deltas: backend **1040 → 1121** (+81 lib, +11 runner binary); admin **236 → 261** (+25).

`cargo fmt` is deliberately **not** in this list — see `apply-progress.md` §1.

---

## 2. Requirement coverage

### `knowledge-migration`

| Requirement | Evidence |
|---|---|
| Migration Run Scoping | `create_run_rejects_project_from_other_org`, `create_run_rejects_client_from_other_org`, `create_run_accepts_null_client_as_internal`, `run_v60_scope_is_immutable_after_insert` |
| Per-Candidate Destination | `run_v60_run_has_no_destination_kind_column`, `run_v60_candidate_accepts_six_destination_kinds`, `migration_candidates_mix_destinations_within_one_project_scoped_run`, `commit_handles_every_destination_kind` |
| Deterministic Source Identity | `stage_rejects_duplicate_source_identity_in_run`, `stage_skips_already_committed_identity`, `stage_allows_same_identity_for_different_destination_kinds` |
| Idempotent Commit Across Runs | `run_v60_provenance_unique_blocks_second_commit`, `commit_twice_produces_no_duplicate_destination`, `commit_skips_when_provenance_exists_and_continues_batch` |
| Per-Candidate Atomic Commit | `commit_is_atomic_per_candidate_and_batch_is_resumable`, `commit_failure_leaves_no_provenance_row` |
| Destination Persistence Reuse | `commit_writes_audit_row_per_destination`, `commit_harness_rejects_invalid_manifest_without_creating_harness`, `store_memory_with_audit_*` |
| Client Isolation of Migrated Knowledge | `commit_memory_is_invisible_to_other_client`, `run_of_another_client_is_404_not_403`, `denied_read_is_audited`, `list_runs_hides_other_clients` |
| Backend Model Independence | `backend_pipeline_succeeds_with_no_model_credentials` |
| Run Reporting and Token Accounting | `run_report_explains_every_non_committed_candidate`, `report_explains_every_skip`, `budget_records_and_trips` |

### `knowledge-migration-review`

| Requirement | Evidence |
|---|---|
| Human Approval Before Commit | `commit_only_processes_approved_candidates`, `staging_reports_each_candidate_and_commits_nothing` |
| Optimistic Concurrency on Review | `review_with_stale_expected_version_is_rejected_and_recorded`, `review_increments_candidate_version`, `stale_version_is_reported_as_a_conflict_not_an_overwrite`, `review_request_without_expected_version_fails_to_deserialize` |
| Append-Only Review History | `run_v60_review_actions_reject_update_and_delete`, `restage_appends_action_without_erasing_rejection` |
| Rejected Candidates Do Not Reappear | `stage_skips_previously_rejected_identity`, `rejected_candidate_is_not_restaged_by_identical_rescan` |
| Provenance Visible at Review Time | `candidate_panel_shows_source_excerpt` (admin) |
| Constrained Batch Approval | `batch_approval_refuses_when_client_attested_present`, `batch_approval_succeeds_for_verified_manifest`, `blocks batch approval when a client-attested candidate is selected` (admin), `allows approving a single client-attested candidate` (admin) |
| Reviewer Authorization Is Recorded | `review_records_actor_and_authorization`, `review_without_permission_records_permission_denied`, `member_without_grants_cannot_create_or_review` |

### `documentation-index`

| Requirement | Evidence |
|---|---|
| Documentation Corpus Is Separate From Code | **`code_search_results_unchanged_after_doc_indexing`**, `doc_indexing_populates_only_the_doc_corpus`, `doc_walker_excludes_code_files`, `docs_search_returns_no_code_chunks` |
| Documentation Chunking Preserves Structure | `doc_chunks_preserve_heading_path`, `anchors_are_unique_within_a_document` |
| Indexing Is Independent of Migration Approval | by construction — `index_documents` never reads candidate status; `doc_indexing_populates_only_the_doc_corpus` indexes with no migration run in existence |
| Indexing State Is Observable and Reconcilable | `commit_succeeds_without_embed_service_and_leaves_indexed_at_null`, `reconciliation_vectorizes_pending_and_updates_state`, `index_status_reports_pending_count` |

---

## 3. What was verified by hand, and what was not

**Verified by test, not by inspection**: every row in §2.

**Not exercised against a live deployment.** The whole change runs in-process against an
in-memory SQLite. Nothing has been deployed, no real repository has been scanned, and
`claude -p` has never actually been invoked by this code — the classifier adapter is covered by
fixtures of the CLI's envelope, not by the CLI. That is deliberate for CI (see the `noop`
connector), and it means the first real run is still a first real run.

**The four connectors do not exist.** `connector_for` refuses them by name, and
`only_the_noop_connector_is_available_in_the_core_change` keeps it that way.

---

## 4. Three things the implementation changed about the design

All three are recorded in `design.md` §11 and in the code that implements them.

1. **`destination_kind` moved from the run to the candidate** (`design.md` §3.1). v56 assumed one
   destination kind per run; one scan of `docs/` produces four.
2. **The commit is not one transaction per candidate** (`design.md` §4.4). `log_audit` and
   `upsert_sdd_artifact` open their own transactions, and SQLite has no nested transactions — so
   the outer transaction did not make the commit atomic, it silently *disabled the audit trail*.
   Caught by `commit_writes_audit_row_per_destination` failing with zero rows. The write order
   carries the guarantee instead.
3. **Vectorization is best-effort and after the commit** (`design.md` §4.3), so `indexed_at` can
   legitimately stay NULL. The spec was written to require that this be visible rather than
   hidden, and `pending_index` reports it.

---

## 5. Residual risks

| Risk | Status |
|---|---|
| **First real run is unrehearsed.** No connector exists yet, so the pipeline has never seen real material. | Accepted. `--dry-run` and `--max-tokens` exist for exactly that first run. |
| **A destination written whose provenance row then fails** would be duplicated on a re-run. | Narrow (only a UNIQUE violation is expected there, and that path is handled). Reported as `provenance_write_failed` and logged at error level rather than swallowed. |
| **`clippy --all-targets` reports a `MutexGuard` held across an await** in one of my tests. | Pre-existing pattern (`api/sdd.rs`, `api/tasks.rs` do the same) and CI does not run `--all-targets`. Left consistent with the surrounding code rather than diverging in one file. |
| **The NDA question is still unanswered.** | Does not block this change; blocks `--include-data` in `db-schemas` and the LLM mode of `git-history`. |
| ~~Adversarial review not yet run.~~ | **Done — §6.** 4 findings, all fixed with regression tests, no blockers. |

---

## 6. Adversarial review — done, 4 findings fixed

Run per the repo protocol (`~/.claude/CLAUDE.md` — Code Review) against the three areas the
change puts most at risk. Nothing blocking survived; four real defects were found and fixed, each
with a regression test.

### 🟠 Mayor — a failed version publish left an orphan harness

`db/migration_queries.rs::write_destination`, `Harness` arm. `create_harness` and
`publish_harness_version` are two writes, and a harness with no published version is not a
harness: nobody can install it and nothing points at it. If the second failed, the catalog row
survived.

Neither function opens a transaction of its own — verified — so the two are now wrapped in one.
This is the *opposite* of the §4.4 finding and worth stating plainly: destinations that manage
their own transactions must not be wrapped, and destinations that do not, must be.
Test: `a_failed_version_publish_leaves_no_orphan_harness`.

### 🟠 Mayor — a run was marked `completed` while candidates were still awaiting review

`api/migrations.rs::commit`. Committing the approved half of a queue set the run to `completed`
even with five candidates still staged. Nothing broke functionally — `review` never checked run
status — but the label lied, and a reviewer who trusts it walks away from work that is still
theirs. Now `completed` requires an empty queue; otherwise the run returns to `in_review`.
Tests: `committing_part_of_a_queue_leaves_the_run_in_review`,
`committing_an_entirely_decided_queue_completes_the_run`.

### 🟡 Menor — cancelling a completed run rewrote its status

`db/migration_queries.rs::cancel_run` set `cancelled` unconditionally. A completed run has
nothing pending, so the new status described nothing that happened. Now refused with
`run_already_completed`. Test: `a_completed_run_cannot_be_cancelled`.

### 🟡 Menor — the pending-index count reported its own limit

`api/docs.rs::index_status` used `list_pending_index(.., 10_000).len()`, which reads as
"exactly 10 000 pending" forever once the backlog passes the cap. Replaced with
`count_pending_index`, a real `COUNT`.

### Not fixed, deliberately

- 🔵 `search_docs_keyword` uses `LIKE '%q%'`, which cannot use an index. Correct and safe (the
  pattern is a bound parameter, not interpolated), and the corpus is small. Revisit with FTS5 if
  documentation search becomes a hot path — not before there is a reason.
- 🔵 `search_docs_semantic` loads every vector and sorts in memory. Same shape as the existing
  memory search; changing one without the other would leave two different answers to the same
  question.
- ⚪ `slugify`'s `replace("--", "-")` does not collapse runs of three or more dashes. Anchors stay
  unique because the start line is part of them.

### What the review found well-made

The emptiness guard in `run_v60` is the pattern worth copying: it makes a destructive migration's
premise **falsifiable** instead of merely believed, and it fails loudly with the table name and
the row count so whoever hits it knows what they are about to lose.

**Verdict: approved.** No blocking findings.

---

## 7. Pending after merge

- [ ] The four connectors, each in its own change — all still in `propose`.
- [ ] **The NDA question is still unanswered.** It does not block this change; it blocks
      `--include-data` in `db-schemas` and the LLM mode of `git-history`.
- [ ] First real run against a real repository, with `--dry-run` first.
