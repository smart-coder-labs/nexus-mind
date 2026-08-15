# Apply Progress — u2s Client Model

> **Change**: `u2s-client-model`
> **Date**: 2026-08-13
> **Result**: 1107 tests pass, 0 failures. Clippy clean on new code (`--all-targets`).

---

## Status by task

| Task | State | Notes |
|---|---|---|
| T-01 Migration `run_v58` (stage 1) | ✅ done | tables, 5 columns, 6 indexes |
| T-02 Rust types | ✅ done | `Client`, `ClientMember`, requests, `validate_slug` |
| T-03 `github_connections` rebuild + encryption | ✅ done | row-count assertion, aborts rather than copying plaintext |
| T-04 Encrypt on write path | ✅ done | plus decrypt on read |
| T-05 Document startup dependency | ✅ done | `docs/RUNNING.md` |
| T-06 Visibility helper | ✅ done | **as a SQL view, not a Rust constant** — see below |
| T-07 Route existing queries through it | ✅ done | 24 call sites migrated |
| T-08 `client:read` / `client:write` | ✅ done | added to both privileged lists |
| T-09 `api/clients.rs` | ✅ done | CRUD + members, 404-not-403 on hidden |
| T-10 Three-level inheritance | ✅ done | conventions + policies; uncovered a pre-existing bug — see below |
| T-11 `promote_memory` | ✅ done | query, handler, route, 4 tests |
| T-12 `client_id` on project create / repo link | ✅ done | cross-org client rejected; 1:1 repo link enforced |
| T-13 Router wiring | ✅ done | 6 client routes + promote |
| T-14 Project resolution report | ⚠️ partial | query + test done; no HTTP endpoint |

---

## Deviations from `design.md`

### 1. The visibility rule is a SQL view, not a Rust `&str` constant

`design.md` §3 specified a `VISIBLE_PROJECT_IDS` string constant. Implementing it
revealed that the ~19 call sites are not one shape but **three**:

- `JOIN project_members pm ON … AND pm.user_id = ?N` (filters rows via the join)
- `JOIN project_members pm ON …` with the user predicate in the `WHERE`
- `project_id IN (SELECT project_id FROM project_members WHERE user_id = ?N)`

A single string fragment cannot substitute into all three. A **view** can:

```sql
CREATE VIEW project_visibility AS
    SELECT p.id, p.org_id, p.name, pm.user_id
      FROM projects p JOIN project_members pm ON pm.project_id = p.id
    UNION
    SELECT p.id, p.org_id, p.name, cm.user_id
      FROM projects p JOIN client_members cm ON cm.client_id = p.client_id;
```

`UNION` rather than `UNION ALL` is load-bearing: a user who is both a project
member and a member of that project's client must appear once, or every JOIN
against the view would silently duplicate their rows. Covered by
`dual_membership_does_not_duplicate_rows`.

### 2. `token_cipher` moved out of `api/code.rs` into `crate::crypto`

It was a private module inside an HTTP handler file, unreachable from
`db::migrations` and `db::queries` which now both need it. Credentials are not
an HTTP concern. Added `is_configured()` so the migration can distinguish "no
key" from "cipher error" — `encrypt()` returning `None` cannot. Three unit tests
came with the move (roundtrip, fresh nonce per call, tamper rejection).

### 3. The migration does not require the key on a fresh install

`design.md` said the key must always be present or the migration fails. As
written that would block every new deployment, which holds no credentials to
protect. Implemented as: **required only when rows exist**. Covered by
`run_v58_succeeds_without_key_when_there_are_no_tokens`.

---

## Two bugs the tests caught during T-07

The mechanical rewrite of 24 call sites introduced two defects. Both were caught
by the existing suite, which is the entire reason T-07 was sequenced after a
green baseline:

1. **`over_enrolled_projects` was wrongly converted.** That query *counts
   members*; it is not a visibility filter. Pointing it at the view inflated the
   count with client members. Reverted to `project_members`.
2. **An orphaned alias in `list_sessions_visible`.** The `JOIN` became `pv` while
   the predicate still read `pm.user_id`, so the query failed to prepare.

Neither was findable by reading the diff — 24 near-identical hunks. The lesson
is the sequencing, not the bugs: a mechanical rewrite of a security predicate
needs a green suite before and after, and the suite has to be run, not assumed.

---

## Test coverage added

- **8** migration tests (`run_v58_*`) — schema, idempotency, constraints, the
  per-client GitHub key, the view, no-key-on-fresh-install
- **12** isolation tests (`client_isolation_tests`) — the acceptance gates
- **4** promotion tests (`promotion_tests`)
- **3** crypto tests (`crate::crypto`)
- **11** inheritance and wiring tests (`inheritance_tests`) — three-level
  stacking, the anti-override assertion, no cross-client leakage, internal
  projects, cross-org client rejection, 1:1 repo linking

Two isolation tests exist purely as traps and must not be "simplified" away:

- `admin_without_membership_does_not_see_client` — fails if anyone swaps
  `is_super_user()` for `is_privileged()` in the visibility path
- `user_can_view_client_returns_true_for_nonexistent_client` — fails if anyone
  "fixes" the existence-hiding branch, turning 404 into an existence oracle

---

## Bug found by T-10: project-scoped conventions never applied

Writing the inheritance test produced a foreign-key violation, which exposed a
defect that predates this change.

`conventions.project_id` is a real FK to `projects(id)` — it holds project
**ids**. But `api/context.rs` passed the URL path segment, which is a project
**name**, straight into that filter. A name compared against a UUID column
matches nothing, so **project-scoped conventions never reached
`GET /v1/context/:project`**. They were silently dropped, and no test noticed
because no test asserted they should be there.

Fixed by resolving name → id via the existing `get_project_id_by_name` before
filtering. Shipping "org → client → project" while the project level quietly did
nothing would have been delivering a lie, so this is in scope rather than a
follow-up.

---

## Remaining

**`client_id` in the conventions/policies request body.** The client level
resolves correctly, but the create/update API does not expose the field yet, so
client-level rules must be inserted by direct SQL.

**T-14 has no HTTP endpoint.** `report_project_resolution` is callable and
tested; it is not routed.

**Integration tests for the 404-not-403 path.** `require_visible_client` is
unit-covered through `user_can_view_client`; the handler-level assertions
(including that the denial writes a `resource.hidden_access_denied` audit row)
are not written.

---

## Verification

```
cargo test                                   → 1107 pass, 0 fail
cargo clippy --all-targets -- -D warnings    → no new warnings
```

Pre-existing baseline noise, not from this change: one lib warning in
`api/sdd.rs:86`, and a repo that is not `cargo fmt`-clean (~1,900 diffs). New
code is rustfmt-clean; see the Gates section of `tasks.md`.

Working tree is on `main`, uncommitted.
