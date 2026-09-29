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
- [ ] 6.1 Per-run cost and latency keyed by `task_id`

## 7. Jev spike
- [ ] 7.1 Validate API contract, typed output, latency and real price; write findings

## 8. Golden tasks
- [ ] 8.1 Harvest 50 merged PRs (nexusmind + kasymir) into a replayable format
