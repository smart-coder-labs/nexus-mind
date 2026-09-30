# Tasks — Factory F0: Foundation

## 1. Merge gate (design §1)
- [x] 1.1 RED: tests for `auto_merge_path_verdict` (docs, tests, never-eligible paths, removed, renamed)
- [x] 1.2 GREEN: implement `auto_merge_path_verdict`
- [x] 1.3 `merge_github_pull` takes `sha: Option<&str>`
- [x] 1.4 `list_github_pull_files` connector (paginated, capped at 300)
- [x] 1.5 `auto_merge_pull`: require reviewed SHA, head match, path verdict, CI on the reviewed SHA, pinned merge
- [x] 1.5b Review fixes: agent-instruction files, executable files under `docs/`, multi-ecosystem manifests, missing required checks
- [x] 1.5c Wizard copy states what auto-merge actually merges
- [x] 1.6 `cargo test` full suite green (1593 passed, 0 failed); clippy clean on touched files (21 pre-existing clippy errors elsewhere)

## 2. Contracts
- [x] 2.1 `schemas/factory/*.schema.json` (6 contracts) + 35 fixtures + `cases.json`
- [x] 2.2 Rust types (`src/factory/contracts.rs`) + conformance test (schema ⇔ serde agree on every fixture, valid ones round-trip) + cross-field reason tests
- [x] 2.3 Review fixes: plugin instruction dirs and `.mdx` not auto-mergeable; all check-run pages + commit statuses; integral-float parity

## 3. Policy storage and admin
- [x] 3.1 Migration v78: `factory_action_policies` table (org-scoped) and permissions `factory_policy:read/write` granted to `super_user_template` only
- [x] 3.2 Admin API gated by `require_explicit_permission` (no admin bypass); optimistic versioning; audit in the same transaction as the write
- [x] 3.3 Admin UI page `/factory-policies` (read-only without write; 409 and failed-delete states), nav, role catalog, hidden in only-context
- [x] 3.4 Review fixes: atomic audit (`append_audit_chained`), `/auth/me` reports template-governed grants (`autonomous_agent:*`, `factory_policy:*`) exactly as enforced

## 4. Policy evaluation
- [x] 4.1 Evaluation order in `factory/policy_engine.rs` (policy → floors → milestones → decision model; criteria holds while no model is configured)
- [x] 4.2 Soak window (10 min): `factory_merge_soaks` (v79) + `process_due_soaks` in the worker tick re-runs every gate and merges pinned to the SHA
- [x] 4.3 Decision audit log: `factory_decisions` (v79), one row per merge evaluation
- [x] 4.4 `auto_merge_pull` asks the engine; wizard copy explains policy + soak
- [x] 4.5 Review fixes: `project_unresolved` instead of falling back past project-scoped policies; any non-allow re-review cancels the soak; each allow restarts it with the latest run

## 5. Intake
- [x] 5.1 `IntakeSource` trait, `TaskSpec` normalization (`factory/intake.rs`): opt-in label, stable name-based task ids, sensitivity-ordered task class, checklist acceptance criteria
- [x] 5.2 GitHub issues adapter (`trusted` only for OWNER/MEMBER/COLLABORATOR authors), NexusMind tasks adapter (only `backlog`/`todo`, paginated)
- [x] 5.3 Review fixes: closed/started tasks excluded, case-insensitive label in both sources, UI warns that a project-scoped merge policy holds every autonomous merge

## 6. Telemetry
- [x] 6.1 `factory_run_metrics` (v80): cost, uncached/cached/cache-write/output tokens, latency, turns and outcome per finished run; unknowns stay NULL; `task_id` column ready for F3
- [x] 6.2 Review fixes: only the attempt that finished the run records telemetry; provider from the run's executor; subject prefers `config.repository`; fan-out runs sum every per-issue session (unknown if any part is unknown); a failed `run.finished` event append no longer reports a committed finish as failed

## 7. Jev spike
- [x] 7.1 API contract validated from public docs (typed questions choice/score/noul, not free-form JSON); spike script `scripts/factory/jev_spike.py` (+ unittest) over 12 synthetic tasks
- [x] 7.2 Spike run (36 calls, 0 errors, p50 341 ms, $0.000027/decision, 11/12 class accuracy); findings in `docs/factory/jev-spike.md` — `risk` score separates well, `needs_human` does not

## 8. Golden tasks
- [x] 8.1 Harvest script `scripts/factory/harvest_golden_tasks.py` (+ unittest), writes outside the repo, refuses in-repo output; validated end-to-end on 3 nexusmind PRs
- [x] 8.2 Full harvest run by the operator: 50 tasks (30 nexusmind, 20 kasymir-app-ui) in ~/.nexusmind/evals/golden/v1; all with SHAs and ticket text. Skew: kasymir-app-ui tasks are all `ui` — add a backend kasymir repo before calibrating
