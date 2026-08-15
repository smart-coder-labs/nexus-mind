# Tasks: u2s Client Model (consultancy grouping)

## Process deviation — strict TDD waived for this change

`openspec/config.yaml` declares `strict_tdd: true` with `tdd_scope: backend_and_admin`. **For this change the test-first ordering is waived by owner decision (2026-08-13).** Tests are written alongside or after the implementation of each task rather than before it.

What is **not** waived: the coverage itself. The isolation tests in Phase 2 are acceptance gates — the deliverable of this change is a security boundary, and the only evidence that client A cannot read client B is a test that tries. Ordering is a preference; proof of the boundary is not.

`config.yaml` still says `strict_tdd: true`. Either update it or leave this note as the recorded exception — but do not leave the two silently disagreeing across future changes.

---

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 1400–1550 |
| 400-line budget risk | Very high |
| Chained PRs recommended | Yes |
| Suggested split | PR 1 (migration + types + encryption fix) → PR 2 (visibility fragment + query rewrites) → PR 3 (handlers, inheritance, promotion) |
| Delivery strategy | auto-chain |
| Chain strategy | stacked-to-main |

Decision needed before apply: No
Chained PRs recommended: Yes
Chain strategy: stacked-to-main
400-line budget risk: Very high

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | `run_v58`, types, token-encryption fix | PR 1 | **Independently shippable and independently valuable** — closes the plaintext-token defect without waiting for the rest. No behaviour change otherwise. |
| 2 | `VISIBLE_PROJECT_IDS`, `user_can_view_client`, rewrite of existing visibility queries | PR 2 | The risky unit. Every existing test must stay green: with no client rows yet, visibility resolves exactly as before. |
| 3 | `api/clients.rs`, router, inheritance, promotion | PR 3 | Additive surface; nothing existing changes shape. |

---

## Phase 1: Foundation (PR 1)

- [x] T-01: Add `run_v58` + call from `run_all` in `apps/backend/src/db/migrations.rs` — **done 2026-08-13**
  - Files: `src/db/migrations.rs`, `tests/integration_test.rs`
  - Scope: `clients`, `client_members`, the five `ALTER TABLE … ADD COLUMN` statements, six indexes. Stage 1 only — the `github_connections` rebuild is T-03.
  - Tests added (8): `run_v58_creates_clients_and_members`, `run_v58_adds_columns`, `run_v58_creates_indexes`, `run_v58_is_idempotent`, `run_v58_rejects_invalid_status`, `run_v58_enforces_unique_slug_per_org`, `run_v58_allows_same_slug_across_orgs`, `run_v58_project_client_id_defaults_to_null`
  - Result: 1074 tests pass, 0 failures. New code is clippy- and rustfmt-clean.
  - **Deviations from the planned task, and why:**
    - **No `in_memory_db_v57()` helper.** It would have been 57 sequential `run_vNN` calls for no gain — `in_memory_db()` + `run_all()` reaches the same state. The helper pattern exists for old migrations that predate having a full chain.
    - **`run_v58_is_idempotent` forces the version guard open** (`PRAGMA user_version = 57`) before re-running. Calling `run_v58` twice without that returns early on the guard and proves nothing; the duplicate-column tolerance is only exercised once the guard is bypassed.
    - **26 hardcoded version assertions had to be bumped**, not 1. `migrations.rs` asserts the current `user_version` in 24 places and `tests/integration_test.rs:330` in one more. See the note under Gates.
    - Two extra tests beyond the plan: `allows_same_slug_across_orgs` (proves slug uniqueness is org-scoped, not global) and `project_client_id_defaults_to_null` (pins the "NULL means internal project" semantic so a future backfill to a sentinel row breaks a test).

- [ ] T-02: Add client types to `apps/backend/src/models/types.rs`
  - Files: `src/models/types.rs`
  - Scope: `Client`, `ClientMember`, `CreateClientRequest`, `UpdateClientRequest` (no `slug` field — immutable per spec §2.3), `PromoteMemoryRequest`, `ProjectResolutionReport`, `validate_slug`
  - Tests: `validate_slug_accepts_valid`, `validate_slug_rejects_uppercase_and_leading_dash`, `create_client_request_roundtrip`
  - Notes: place after the `Project`/`ProjectMember` block. Reuse the existing `default_active_status()`.

- [ ] T-03: `github_connections` rebuild + token encryption in `run_v58`
  - Files: `src/db/migrations.rs`
  - Scope: create `github_connections_new` with `PRIMARY KEY (org_id, client_id, github_login)`; copy rows **in Rust** through `token_cipher::encrypt`; count-match assertion; drop + rename. All inside one transaction.
  - Tests: `run_v58_rebuilds_github_connections_preserving_row_count`, `run_v58_encrypts_existing_tokens` (assert no plaintext remains in the raw column), `run_v58_aborts_when_encryption_fails` (assert original table intact), `two_clients_with_distinct_logins_coexist`
  - Notes: abort the whole migration if `token_cipher::encrypt` returns `None` for any row — never copy a plaintext token forward.

- [ ] T-04: Encrypt on the `github_connections` write path
  - Files: `src/db/queries.rs` (~line 14860)
  - Scope: the `INSERT OR REPLACE INTO github_connections` currently stores the raw token; route it through `token_cipher::encrypt` first, and decrypt on read.
  - Tests: `github_connection_roundtrip_stores_ciphertext`
  - Notes: **T-03 without T-04 is a regression** — history gets encrypted while every new write re-introduces plaintext. They ship in the same PR.

- [ ] T-05: Document `NEXUSMIND_TOKEN_ENCRYPTION_KEY` as a startup dependency
  - Files: `docs/RUNNING.md`, `.env.example` if present
  - Scope: state that the key must be set before first boot and that `run_v58` fails loudly without it.
  - Notes: this is the hard dependency the AWS deployment module must satisfy from Parameter Store.

---

## Phase 2: Visibility (PR 2) — the isolation boundary

- [ ] T-06: Add `VISIBLE_PROJECT_IDS` and `user_can_view_client` to `apps/backend/src/db/queries.rs`
  - Files: `src/db/queries.rs`
  - Scope: the canonical fragment (design §3) and the client visibility helper, including its `NOT EXISTS … THEN 1` existence-hiding branch.
  - Tests: `client_member_sees_all_projects_of_that_client`, `project_member_sees_only_that_project`, `non_member_cannot_see_other_client_projects`, `super_user_sees_every_client`, `admin_without_membership_does_not_see_client`, `user_can_view_client_returns_true_for_nonexistent_client`, `internal_project_visible_only_via_project_membership`
  - Notes: two of these tests exist to guard specific traps — `admin_without_membership_…` guards against reaching for `is_privileged()` instead of `is_super_user()`, and `…returns_true_for_nonexistent_client` guards against turning 404 into an existence oracle. Do not "simplify" either away.

- [ ] T-07: Route existing visibility queries through the fragment
  - Files: `src/db/queries.rs`
  - Scope: rewrite `user_can_view_project_name`, `list_sessions_visible`, and the memory/convention/policy/project/code list paths to consume `VISIBLE_PROJECT_IDS` instead of their own hand-written `EXISTS` clauses.
  - Tests: **the entire existing suite must stay green.** With no client rows, every project resolves through project membership exactly as before — that is the regression check.
  - Notes: this is the highest-risk task in the change. Any list endpoint left on its old clause is a silent isolation hole.

- [ ] T-08: Add `client:read` / `client:write` to `get_role_permissions`
  - Files: `src/db/queries.rs`
  - Tests: `default_roles_include_client_permissions`
  - Notes: mirror the existing `project:*` entries. `require_permission` keeps using `is_privileged()` — permissions and visibility stay separate axes.

---

## Phase 3: Surface (PR 3)

- [ ] T-09: New module `apps/backend/src/api/clients.rs`
  - Files: `src/api/clients.rs` (new)
  - Scope: list, create, update, archive, list/add/remove members. Every read gated by `user_can_view_client`; denials via `hidden_resource_not_found` with `resource_type = "client"`.
  - Tests: `create_rejects_duplicate_slug_409`, `patch_rejects_slug_field_400`, `archive_is_idempotent`, `delete_client_with_projects_returns_422`
  - Notes: `get_client_member_role` mirrors `get_project_member_role` in shape and return type.

- [ ] T-10: Three-level inheritance in conventions and policies
  - Files: `src/db/queries.rs` (`list_conventions_visible`), `src/api/context.rs`, `src/api/policy.rs`
  - Scope: add the `client: Option<&str>` parameter and the additive three-branch `WHERE` (design §6).
  - Tests: `org_convention_applies_to_every_client_project`, `client_convention_adds_to_org_convention` (assert **both** present — this is the anti-override test), `internal_project_resolves_org_then_project`
  - Notes: the `MAX_CONTEXT_CONVENTIONS` cap of 50 applies to the merged result, unchanged.

- [ ] T-11: `promote_memory`
  - Files: `src/db/queries.rs`, `src/api/memory.rs`
  - Scope: new org-scoped memory with `promoted_from` lineage; source untouched; audit row `memory.promoted`; widen `scope` validation to `org | client | project | personal`.
  - Tests: `promote_creates_org_scoped_copy_with_lineage`, `promote_leaves_source_unchanged`, `promote_rejects_org_scoped_source`, `scope_validation_rejects_unknown_value`
  - Notes: no content rewriting. Sanitization is out of scope by decision.

- [ ] T-12: Accept `client_id` on project create; link `code_projects.project_id`
  - Files: `src/api/projects.rs`, `src/api/code.rs`
  - Scope: optional `client_id` on create (omitted ⇒ internal u2s project); 1:1 repo↔project enforced at handler level with 409 on a second link.
  - Tests: `create_project_without_client_id_is_internal`, `second_repo_link_to_same_project_returns_409`

- [ ] T-13: Router wiring
  - Files: `src/api/router.rs`
  - Scope: mount the six `/v1/clients*` routes and `POST /v1/memories/:id/promote`.
  - Tests: `cross_client_read_returns_404_not_403`, `cross_client_read_writes_hidden_access_denied_audit_row` (in `tests/integration_test.rs`)

- [ ] T-14: Project resolution report (read-only)
  - Files: `src/db/queries.rs`
  - Scope: report how `memories.project` values map to `projects.name` — exact match only. Resolved count, unresolved count, distinct unresolved values with row counts.
  - Tests: `resolution_report_counts_without_mutating`
  - Notes: **it must not write.** Assigning `project_id`/`client_id` to legacy memories is a separate operator action, out of scope.

---

## Gates

Every PR must pass before merge:

```
cargo test   --manifest-path apps/backend/Cargo.toml
cargo clippy --manifest-path apps/backend/Cargo.toml --all-targets -- -D warnings
```

**`cargo fmt -- --check` is NOT a usable gate on this repo.** Measured at T-01: it reports ~1,900 diffs across the existing codebase (214 in `migrations.rs` alone, 167 in `queries.rs`, 143 in `api/memory.rs`). Running `cargo fmt` to satisfy the gate would produce a repo-wide reformat that buries the actual change. The workable rule is **new and modified lines must be rustfmt-clean**, verified by diffing the `--check` output against the line ranges the PR touches:

```sh
cargo fmt -- --check 2>&1 | grep '^Diff in.*<file>' | sed 's/.*<file>://;s/:$//' | awk '$1 >= <first-new-line>'
```

Repo-wide `cargo fmt` is worth doing — as its own commit, on its own, never inside a feature PR.

Clippy note: CI runs it **without** `--all-targets` (per `config.yaml`), so lint failures that only appear in test code are not caught there. Run it locally with `--all-targets`, as the gate above does. Baseline at T-01 carries one pre-existing lib warning (`api/sdd.rs:86`, useless conversion) that is not from this change.

**Version-assertion churn.** `user_version` is hardcoded in 25 test assertions across `migrations.rs` and `tests/integration_test.rs`, so every migration costs a mechanical 25-line diff. Out of scope here, but worth a follow-up: a single `const CURRENT_SCHEMA_VERSION` asserted once would remove the churn and the risk of a half-updated bump.

---

## Rollout order (design §11)

1. PR 1 merges — schema lands, security defect closed, no behavioural change.
2. PR 2 merges — visibility rewritten; existing suite green proves no regression.
3. PR 3 merges — client surface available; create the first client.
4. Assign existing projects to clients one at a time via `PATCH`, verifying context resolution after each. Reversible (`client_id` back to `NULL`).

---

## Process note

`openspec/config.yaml` sets `artifact_store: nexusmind` (dual-write to filesystem **and** the NexusMind artifact store). **The NexusMind backend is unreachable in this session**, so all four artifacts of this change exist on the filesystem only and must be replayed into the artifact store once it is up.
