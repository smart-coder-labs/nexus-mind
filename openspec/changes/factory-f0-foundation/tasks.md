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
- [ ] 2.1 `schemas/factory/*.schema.json` (6 contracts)
- [ ] 2.2 Rust types and schema round-trip tests

## 3. Policy storage and admin
- [ ] 3.1 Migration: `factory_action_policies` table (org-scoped) and permissions `factory_policy:read/write` granted to `super_user_template` only
- [ ] 3.2 Admin API gated by `require_permission`
- [ ] 3.3 Admin UI page

## 4. Policy evaluation
- [ ] 4.1 Evaluation order (policy → floors → Jev stub → merge checks → audit)
- [ ] 4.2 Soak window (10 min) via a delayed re-check
- [ ] 4.3 Decision audit log

## 5. Intake
- [ ] 5.1 `IntakeSource` trait, `TaskSpec` normalization
- [ ] 5.2 GitHub issues adapter, NexusMind tasks adapter

## 6. Telemetry
- [ ] 6.1 Per-run cost and latency keyed by `task_id`

## 7. Jev spike
- [ ] 7.1 Validate API contract, typed output, latency and real price; write findings

## 8. Golden tasks
- [ ] 8.1 Harvest 50 merged PRs (nexusmind + kasymir) into a replayable format
