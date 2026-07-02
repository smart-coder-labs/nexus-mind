# NexusMind Backend API — QA Report

**Scope**: exhaustive functional test of every HTTP route in `apps/backend/src/api/router.rs` (Rust/axum, SQLite).
**Method**: live curl/HTTP requests against a locally running backend (`cargo run` → `./target/release/nexusmind`, port 8080), seeded via `./scripts/reset-demo.sh`, using the three demo API keys (`nm_demo_acme_admin`, `nm_demo_acme_sarah`, `nm_demo_techstartup_admin`). 303 individual request/response pairs were captured. Every list-shaped endpoint was checked for its actual JSON shape (not just HTTP 200), per the known `/v1/memory` → paginated `MemoryPage` change.
**Not covered here**: the admin dashboard UI (see `docs/QA_UI.md`) and the MCP tool layer (see `docs/MCP_TOOLS_AUDIT.md`) — this report is HTTP-API-only.

Mid-audit, the running server binary was found to be ~17 minutes stale relative to the checked-out source (`feat/scope-conventions-policies` @ `f992d39`). It was rebuilt (`cargo build --release`, incremental, 21s) and restarted against the same SQLite file before continuing, so all verdicts below reflect current `HEAD`.

## Summary

| Verdict | Count |
|---|---|
| WORKS | ~145 |
| WORKS (with notes/inconsistency) | ~9 |
| FAILS / BUG | 2 |
| N/A (correctly rejected: auth/validation edge cases) | ~145 |

## Summary table

| Endpoint | Method | Verdict |
|---|---|---|
| `/v1/health` | GET | WORKS |
| `/v1/orgs` | GET/POST | WORKS (superuser-gated, duplicates `/internal/orgs`) |
| `/v1/orgs/:id/users` | GET | WORKS (superuser-gated, duplicate) |
| `/v1/admin/auth/login` | POST | WORKS |
| `/v1/admin/auth/set-password` | POST | WORKS |
| `/v1/admin/auth/request-reset` | POST | WORKS (no user-enumeration leak) |
| `/v1/admin/auth/logout` | POST | WORKS |
| `/v1/auth/forgot-password` | POST | WORKS (alias) |
| `/v1/auth/reset-password/confirm` | POST | WORKS (alias) |
| `/v1/invites/:token` | GET | WORKS |
| `/v1/invites/:token/redeem` | POST | WORKS, single-use enforced |
| `/v1/admin/auth/me` | GET | WORKS |
| `/v1/admin/auth/change-password` / `/v1/auth/change-password` | POST | WORKS |
| `/v1/memory` | GET | WORKS — paginated `{memories,total,limit,offset}` |
| `/v1/memory/search` | POST | WORKS — same paginated shape (consistent) |
| `/v1/memory/store` | POST | WORKS |
| `/v1/memory/:id` | GET/PATCH/DELETE | WORKS, cross-org isolated (404) |
| `/v1/memory/:id/archive` `/restore` | POST | WORKS |
| `/v1/memory/:id/pin` | POST/DELETE | WORKS (2 unpin routes, see notes) |
| `/v1/memory/:id/unpin` | POST | WORKS (duplicate of DELETE .../pin) |
| `/v1/memory/bulk` | DELETE | WORKS |
| `/v1/memory/export` | GET | WORKS (CSV) |
| `/v1/sessions` | GET/POST | WORKS — plain array |
| `/v1/sessions/:id` | GET/PATCH | WORKS, cross-org isolated |
| `/v1/sessions/:id/memories` | GET | WORKS |
| `/v1/users` | GET | WORKS — plain array |
| `/v1/users/invite` | POST | WORKS (instant activation, see notes) |
| `/v1/users/:id` | DELETE | WORKS — **soft-delete (suspend), not removal** |
| `/v1/users/:id/rotate-key` | POST | WORKS |
| `/v1/users/:id/role` | PATCH | WORKS |
| `/v1/roles` | GET/POST | WORKS — plain array |
| `/v1/roles/:id` | DELETE | WORKS |
| `/v1/projects` | GET/POST | WORKS — plain array |
| `/v1/projects/:id` | GET/PATCH/DELETE | WORKS, cross-org isolated |
| `/v1/projects/:id/archive` `/restore` | POST | WORKS |
| `/v1/projects/:project_id/members` | GET/POST | WORKS |
| `/v1/projects/:project_id/members/:user_id` | DELETE | WORKS |
| `/v1/projects/:id/settings` | GET/PATCH | WORKS |
| `/v1/projects/:id/stats` | GET | WORKS |
| `/v1/policies` | GET/POST | WORKS — `{policies:[]}` wrapper |
| `/v1/policies/:id` | PATCH/DELETE | WORKS |
| `/v1/policy/check` | POST | WORKS |
| `/v1/conventions` | GET/POST | WORKS — plain array |
| `/v1/conventions/:id` | GET/PATCH/DELETE | WORKS |
| `/v1/conventions/:id/archive` `/restore` | POST | WORKS |
| `/v1/context` | GET | WORKS |
| `/v1/context/type/:type` | GET | WORKS |
| `/v1/context/session/:id` | GET | WORKS |
| `/v1/context/project/:project` | GET | WORKS |
| `/v1/code/index` | POST | WORKS but **silently "succeeds" on unreachable repo** (see Top Issues) |
| `/v1/code/search` | POST | WORKS |
| `/v1/code/status/:project` | GET | WORKS |
| `/v1/code/context` | GET | WORKS (requires `file_path`+`symbol` query params, undocumented in route comment) |
| `/v1/code/graph` | GET | WORKS |
| `/v1/code/snippet` | GET | WORKS (requires `file` not `path`) |
| `/v1/code/projects` | GET | WORKS — plain array |
| `/v1/code/projects/:id` | PATCH | WORKS (numeric id) |
| `/v1/code/projects/:id` | DELETE | **BUG — keyed by NAME, not id (see Top Issues)** |
| `/v1/code/projects/:id/schedule` | PATCH | WORKS |
| `/v1/code/projects/:id/files` | GET | WORKS, cross-org isolated |
| `/v1/code/projects/:id/reindex` | POST | WORKS |
| `/v1/code/projects/:id/archive` `/restore` | POST | WORKS |
| `/v1/audit` | GET | WORKS — plain array, role-gated (403 non-admin) |
| `/v1/audit/export` | GET | WORKS (CSV) |
| `/v1/audit/log` | POST | WORKS |
| `/v1/admin/stats*` (9 routes) | GET | WORKS, role-gated |
| `/v1/admin/onboarding` | GET | WORKS |
| `/v1/admin/org` | GET/PATCH | WORKS, role-gated |
| `/v1/admin/org/settings` | GET/PATCH | WORKS |
| `/v1/admin/settings/retention-preview` | GET | WORKS |
| `/v1/webhooks` | GET/POST | WORKS — `{webhooks:[]}` wrapper |
| `/v1/webhooks/:id` | PATCH/DELETE | WORKS |
| `/v1/webhooks/:id/test` | POST | WORKS (correctly rejects inactive webhook) |
| `/v1/webhooks/:id/deliveries` | GET | WORKS, cross-org isolated |
| `/v1/webhooks/deliveries/:delivery_id/retry` | POST | WORKS |
| `/v1/admin/keys` | GET/POST | WORKS — plain array; create needs `user_id` |
| `/v1/admin/keys/:key_id` | GET/PATCH/DELETE | WORKS |
| `/v1/admin/keys/:key_id/rotate` | POST | WORKS (returns new id, old id retired) |
| `/v1/admin/keys/:key_id/revoke` | POST | WORKS |
| `/v1/admin/users` | GET | WORKS — plain array |
| `/v1/admin/users/:user_id/reset-key` `/disable` `/enable` | POST | WORKS |
| `/v1/admin/users/:id/note` | PATCH | WORKS |
| `/v1/admin/memories/:id/note` | PATCH | WORKS |
| `/v1/admin/memories/:id/schedule-delete` | PATCH | WORKS |
| `/v1/admin/org/announcement` | PATCH | WORKS (field is `announcement`, not `message`) |
| `/v1/admin/org/logo` | PATCH | WORKS |
| `/v1/admin/memories/health` | GET | WORKS |
| `/v1/admin/memories/import` | POST | WORKS |
| `/v1/admin/memories/merge` | POST | WORKS (fields `target_id`/`source_id`) |
| `/v1/admin/memories/bulk-tag` | POST | WORKS |
| `/v1/admin/tags/rename` | POST | WORKS (fields `from`/`to`) |
| `/v1/admin/export` | GET | WORKS |
| `/v1/admin/import` | POST | WORKS |
| `/v1/search` | GET | WORKS — `{memories,users,projects,policies,conventions}` (was missing last 2 on stale binary) |
| `/v1/admin/notifications` | GET | WORKS — plain array |
| `/v1/admin/notifications/mark-all-read` | POST | WORKS |
| `/v1/admin/invites` | POST | WORKS (separate flow from `/v1/users/invite`) |
| `/v1/admin/collections` | GET/POST | WORKS — plain array |
| `/v1/admin/collections/:id` | DELETE | WORKS |
| `/v1/memories/:id/collection` | POST | WORKS |
| `/v1/agents` | GET/POST | **BUG — HTTP 500, table missing (see Top Issues)** |
| `/v1/agents/:id` | GET/PATCH | FAILS (same root cause) |
| `/v1/agents/:id/assignments` | GET | FAILS (same root cause) |
| `/v1/github/auth` | GET | WORKS |
| `/v1/github/callback` | POST | WORKS (rejects bogus code) |
| `/v1/github/status` | GET | WORKS |
| `/v1/github/connection` `/disconnect` | DELETE | WORKS (2 identical routes, see notes) |
| `/internal/metrics` | GET | WORKS, superuser-gated |
| `/internal/orgs` | GET/POST | WORKS, superuser-gated |
| `/internal/orgs/:id` | GET/PATCH/DELETE | WORKS, superuser-gated |
| `/internal/orgs/:id/users` | GET | WORKS, superuser-gated |
| `/internal/orgs/:id/impersonate` | POST | WORKS, superuser-gated |
| `/internal/users` | GET | WORKS, superuser-gated |
| `/internal/users/:id/suspend` | POST | WORKS, superuser-gated |
| `/internal/audit` | GET | WORKS, superuser-gated |
| `/internal/search` | GET | WORKS, superuser-gated |

## Per-endpoint detail (grouped)

### Auth & public routes
- `POST /v1/admin/auth/login` — wrong password → `401`; unknown email → `401` (no enumeration); empty body → `422`.
- `POST /v1/admin/auth/request-reset` / `/v1/auth/forgot-password` — both known and unknown emails return the same `200`-style response — correct, prevents email enumeration.
- `GET /v1/admin/auth/me` — no auth → `401`; bogus key → `401`; valid key → full `{org,user}`.
- `GET/POST /v1/invites/:token` — bogus token → `404`; valid token → redeemable once; **second redeem of the same token correctly rejected**.
- `POST /v1/admin/invites` then `GET /v1/invites/:token` then `POST /v1/invites/:token/redeem` all work end-to-end; redeem response is `{"api_key": "..."}` only (no `user` object — minor DX gap, a caller can't get the new user's id without a follow-up `GET /v1/users` scan).

### Memory (`/v1/memory*`)
- `GET /v1/memory` and `POST /v1/memory/search` **both** return the identical paginated shape `{memories, total, limit, offset}` — confirmed consistent, no regression found here.
- Full CRUD (`store`/`get`/`patch`/`delete`), `archive`/`restore`, `pin`/`unpin`, `bulk` delete, and `export` (CSV) all work correctly.
- Cross-org isolation confirmed: techstartup key reading/editing/deleting an acme memory id → `404` in all three cases (no leakage, no 403 fingerprinting difference — good).
- Missing `content` on store → `400`; malformed JSON body → `422 invalid_body` (clear error).
- **Minor**: both `POST /v1/memory/:id/unpin` and `DELETE /v1/memory/:id/pin` exist and do the same thing — harmless but redundant surface.

### Sessions / Users / Roles
- `GET /v1/sessions`, `GET /v1/users`, `GET /v1/roles` all return **plain arrays**, not paginated wrappers (contrast with `/v1/memory`'s paginated shape — see Top Issues for the broader shape-inconsistency pattern).
- `POST /v1/users/invite` creates a fully active user **immediately** (returns `api_key` + user in one call) — this is a *different* invite mechanism from `POST /v1/admin/invites` → `/v1/invites/:token/redeem` (email-first, token-based, requires the invitee to set their own password). Two independent user-onboarding flows exist; not documented which one the admin UI actually uses.
- `DELETE /v1/users/:id` returns `204` but the user is **not removed** — `users::remove` calls `suspend_user` internally, sets `status: "suspended"`, and the record still appears in `GET /v1/users`. There is also a separate `POST /v1/admin/users/:user_id/disable`. Both effectively deactivate a user; DELETE semantics are misleading (see Top Issues).
- Role CRUD works; deleting a built-in template role (`tmpl_security_officer`) is correctly rejected.

### Projects
- Full CRUD + archive/restore + members + settings + stats all work.
- `GET /v1/projects/:id/settings` on a freshly created project returns `{}` (no defaults populated) — harmless but means clients must handle every field as optional.
- Cross-org `GET` on another org's project → `404`, correct.

### Policies / Conventions
- `POST /v1/policies` requires `{name, rule_type, config}` — `rule_type` restricted to `model_whitelist|budget_limit|pii_redact`. First test attempt with an intuitive-but-wrong payload (`action`/`pattern` fields) correctly failed with `422 invalid_json`; the valid schema works and round-trips through PATCH/DELETE.
- `GET /v1/policies` wraps in `{"policies":[...]}`; `GET /v1/conventions` is a bare array — inconsistent sibling shapes.
- Convention CRUD + archive/restore all work; conventions use small integer ids (e.g. `3`) rather than UUIDs, unlike almost everything else in the API (memories, sessions, projects, policies, webhooks, keys, collections are all UUIDs).

### Context
- `GET /v1/context` → `{conventions, last_activity, recent_memories, scope, tools}`.
- `GET /v1/context/type/:type` → different, narrower shape `{last_activity, recent_memories, tools, type}` (no `conventions`/`scope`) — expected given it's type-scoped, but worth documenting since it's not a strict subset naming pattern.
- Unknown `:type` and unknown `:project` degrade gracefully (empty results, no error).

### Code (`/v1/code*`)
- `GET /v1/code/projects` → plain array with **string ids that are actually small integers** (`"7"`, `"10"`) — a third id style alongside UUIDs (memories etc.) and small-int (conventions).
- `GET /v1/code/graph`, `POST /v1/code/search` work; empty-corpus search correctly returns `[]` rather than erroring.
- `GET /v1/code/snippet` requires query param `file` (not `path` as the name might suggest); `GET /v1/code/context` requires `file_path` **and** `symbol`. Both return axum's auto-generated `"missing field 'x'"` message on omission, which is genuinely helpful for API discovery — no complaint, just noting the exact required names for the report record.
- **`POST /v1/code/index` on an unreachable/nonexistent repo URL returns `200 {"status":"indexing_started"}` and the project later settles into `status:"indexed", file_count:0, chunk_count:0` with no error surfaced anywhere** — see Top Issues.
- **`DELETE /v1/code/projects/:id` is keyed by project NAME, while every sibling route on the same path family (`archive`, `restore`, `schedule`, `files`, `reindex`, and `PATCH .../:id`) is keyed by the numeric `id`** — see Top Issues, this is the highest-priority bug found.

### Audit
- `GET /v1/audit` role-gated: Sarah (non-admin) → `403`, admin → `200` array of 50 most-recent entries.
- `GET /v1/audit/export` → CSV with a hash chain (`previous_hash`/`current_hash`) — tamper-evident audit log, nice.
- `POST /v1/audit/log` requires both `action` and `resource_type` (first attempt without `resource_type` correctly `400`'d).

### Admin stats / org settings
- All 9 `/v1/admin/stats/*` sub-routes return `200` with sensibly-shaped, endpoint-specific bodies (some dict, some array — intentional, each is a different aggregation).
- `PATCH /v1/admin/org/announcement` expects `{announcement, announcement_type}` — a payload guess using `{message, type}` (reasonable field names) fails with `422`. Clearing by sending `announcement: ""` makes the `announcement` key **disappear entirely** from the subsequent `GET /v1/admin/org/settings` response rather than appearing as `""` or `null` — minor shape wobble.
- `PATCH /v1/admin/org/settings` and `GET .../retention-preview` both work.

### Webhooks
- Full CRUD works. `POST /v1/webhooks` requires `{name, target_url, events}` (not `url`).
- `POST /v1/webhooks/:id/test` correctly refuses to fire on an `active:false` webhook (`400 webhook_not_active`) — good guardrail.
- Cross-org `GET .../deliveries` → `404`, correct.

### Admin keys
- `POST /v1/admin/keys` requires `{user_id, label}` (not just `label`).
- Response is nested: `{"key": {...}, "raw_key": "..."}` — the raw key is shown once at creation, as expected for a secret-reveal pattern.
- `POST .../rotate` returns a **new** key id and retires the old one (the old id then correctly 404s on subsequent `revoke`/`delete` — this is expected replace-not-mutate behavior, not a bug, but worth documenting since it surprised the first test pass).

### Admin memories / tags / org
- `merge` needs `{target_id, source_id}` (singular, not `source_ids` array); on success it deletes both originals and returns a **new** memory with concatenated content — good, no orphaned duplicates.
- `bulk-tag`, `import`, `rename` (fields `from`/`to`) all work once given the correct schema.

### Search, notifications, invites, collections
- `GET /v1/search?q=` → `{memories, users, projects, policies, conventions}`. This is the endpoint called out in the task brief — verified consistent and complete after rebuild (see Top Issues for the stale-binary caveat found mid-run).
- `GET /v1/admin/notifications` + `mark-all-read` work.
- Collection CRUD + `POST /v1/memories/:id/collection` assignment all work.

### Agents
- **Every `/v1/agents*` route returns `500 {"error":"no such table: agents","code":"internal_error"}`** — see Top Issues, #1 priority.

### GitHub integration
- `GET /v1/github/auth`, `/status` work; `POST /v1/github/callback` correctly rejects a bogus OAuth code.
- `DELETE /v1/github/connection` and `DELETE /v1/github/disconnect` are two separate routes doing the same thing (both map effectively to disconnect) — harmless duplication, same class of issue as the memory pin/unpin duplication.

### Internal / superuser routes
- Every `/internal/*` route correctly returns `401` for both no-auth and a regular org-scoped demo key. Verified specifically for the two most sensitive routes (`POST /internal/orgs/:id/impersonate`, `POST /internal/users/:id/suspend`) — both properly gated, no privilege escalation found.
- One test artifact worth recording: `POST /internal/orgs` with an incomplete body returned `422` (body-shape rejection) *before* the auth check ran, because axum's JSON extractor runs before the handler body executes. Re-tested with a **well-formed** body and the same non-superuser key → correctly `401 "Valid superuser key required"`. Confirmed not an auth bypass.
- `/v1/orgs`, `/v1/orgs/:id/users` (public-prefixed but manually superuser-gated) duplicate `/internal/orgs`, `/internal/orgs/:id/users` — see Top Issues for the redundancy note.

### Rate limiting
- 8 rapid-fire `GET /v1/memory` calls on the same key all returned `200` — free-tier bucket (100 req/min capacity) was not exhausted by this test run; rate limiting exists (`src/api/rate_limit.rs`, token-bucket, tier-based quotas) but wasn't tripped under normal QA load. Not re-tested to exhaustion to avoid disrupting the shared demo environment.

## Top issues to fix (ranked)

1. **`GET/POST/PATCH /v1/agents*` → HTTP 500, feature completely unusable.**
   Root cause: `apps/backend/src/db/migrations.rs`, `run_all()` (lines 43–48) calls `run_v38(conn)?;` then `run_v40(conn)?;` — **`run_v39(conn)?;` is never called**, even though `run_v39` (fully implemented, ~line 181) creates the `agents` and `agent_assignments` tables. Every database, old or brand new, is missing these tables. Fix is a one-line addition (`run_v39(conn)?;` between the v38 and v40 calls). Confirmed reproducible on a freshly rebuilt binary from current `HEAD`, not stale state.

2. **`DELETE /v1/code/projects/:id` uses the project's `name`, not its numeric `id` — inconsistent with all 6 sibling routes on the same path family.**
   `archive_project`, `restore_project`, `update_schedule`, `get_project_files`, `post_reindex` all take `Path<i64>`; `update_code_project` (PATCH) parses the path segment as `i64`. Only `delete_project` takes `Path<String>` and looks the project up by name (`src/api/code.rs`). Any client that deletes using the `id` field returned by `GET /v1/code/projects` (the obvious, consistent choice) gets a false `404` and cannot delete the project via id at all — they'd have to know to pass the name instead. Fix: change `delete_project` to take `Path<i64>` and delete by id, matching its siblings.

3. **`POST /v1/code/index` silently "succeeds" when the target repo is unreachable/nonexistent.**
   A clone against a bogus GitHub URL returns `200 {"status":"indexing_started"}`, and the subsequent `GET /v1/code/status/:project` reports `status:"indexed", file_count:0, chunk_count:0` — indistinguishable from successfully indexing a genuinely empty repo. No error state, message, or field anywhere. A client (or the admin UI) has no way to tell "clone failed" from "repo happens to be empty." Recommend adding a `status:"failed"` (or similar) outcome with an error reason surfaced via `GET /v1/code/status/:project`.

4. **`DELETE /v1/users/:id` doesn't delete — it soft-suspends, and the user still shows up in `GET /v1/users`.**
   `users::remove` (`src/api/users.rs`) calls `suspend_user`, sets `status:"suspended"`, returns `204`. The record remains in the default `GET /v1/users` listing with no way to filter it out. This overlaps functionally with the separate `POST /v1/admin/users/:user_id/disable` endpoint — two routes doing effectively the same thing under different names/verbs, one of which lies about its HTTP semantics (DELETE implies removal). Recommend either (a) making DELETE actually remove/hide the user from default listings, or (b) renaming/documenting it clearly as suspend and deprecating the redundant `disable` route.

5. **List-endpoint response shape is inconsistent across the API** (not a regression, but worth a deliberate convention pass): `/v1/memory` and `/v1/memory/search` → paginated `{memories,total,limit,offset}` object; `/v1/policies` and `/v1/webhooks` → `{policies:[...]}` / `{webhooks:[...]}` wrapper with no pagination; `/v1/sessions`, `/v1/users`, `/v1/roles`, `/v1/projects`, `/v1/conventions`, `/v1/code/projects`, `/v1/admin/collections`, `/v1/admin/keys` → bare arrays with no pagination or total count at all. A frontend that assumes any one of these shapes for "the other" list endpoint will break exactly like the `/v1/memory` MemoryPage regression this audit was asked to watch for — this class of bug will keep recurring until list endpoints share one convention.

6. **Duplicate/dead API surface** (low severity, cleanup candidates):
   - `/v1/orgs`, `POST /v1/orgs`, `/v1/orgs/:id/users` (public path, manually superuser-gated) duplicate `/internal/orgs`, `/internal/orgs/:id/users` almost exactly, but without the update/delete/impersonate capabilities `/internal/orgs` has.
   - `POST /v1/memory/:id/unpin` and `DELETE /v1/memory/:id/pin` do the same thing.
   - `DELETE /v1/github/connection` and `DELETE /v1/github/disconnect` do the same thing.
   - Id style is inconsistent: UUIDs (memories, sessions, projects, policies, webhooks, keys, collections), small ints as strings (conventions, code projects). Not urgent, but confusing for SDK/client generation.

7. **Minor: stale long-running dev server served an outdated API shape.**
   Not a code bug, but worth a process note: the local backend process was running a binary ~17 minutes older than `HEAD`, and `/v1/search` was silently missing the `policies`/`conventions` fields that shipped in commit `4266d84` until the process was restarted. Long-lived local dev servers can mask landed fixes; consider a `make backend-restart` helper or a startup log line printing the git SHA the binary was built from, so it's obvious when a running instance is stale.

## Notes on QA data hygiene

All disposable test resources created during this audit (memories, sessions, projects, policies, conventions, webhooks, roles, api keys, collections, code project, invited users) were cleaned up afterward via their respective DELETE/archive routes, except where the API itself only supports soft-delete (e.g. the two disposable invited users end up `status:"suspended"`, consistent with finding #4 above — this is expected product behavior, not leftover QA debris).
