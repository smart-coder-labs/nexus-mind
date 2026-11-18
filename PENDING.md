# PENDING — Issue #487 (Expire web sessions; stop login/impersonation from revoking other sessions)

## DONE
- Investigated the issue; confirmed `expires_at` exists since migration v27, so no schema change is needed.
- Found and avoided a timestamp-format trap: `validate_api_key` compares against `datetime('now')` (space format) while this file elsewhere writes chrono ISO (`...T...Z`); because `'T' > ' '` lexically, an ISO `expires_at` would not expire for ~24h. All TTLs are now computed with SQL `datetime('now', '+N hours')`.
- `apps/backend/src/config.rs`: added `#[arg(long, env = "SESSION_TTL_HOURS", default_value_t = 168)] pub session_ttl_hours: i64`.
- `apps/backend/src/db/queries.rs`: `create_web_session_key` now takes a TTL and writes `expires_at`; blanket revoke removed; live `web-session` keys capped at `MAX_LIVE_WEB_SESSIONS = 5` (least-recently-used revoked); expired rows revoked and dead rows deleted after `SESSION_SWEEP_GRACE_HOURS = 24`, scoped to one `user_id` and one label. New `create_impersonation_key` (label `impersonation`, never touches `web-session` rows). `get_org_admin_key` returns the impersonated admin's `user_id` alongside the key.
- `apps/backend/src/api/auth.rs`: `set_session_cookie` sets `Max-Age` from the TTL; both login call sites pass `config.session_ttl_hours`. Logout untouched (still revokes only the presented key).
- `apps/backend/src/api/internal.rs`: `impersonate_org` mints a 1-hour `impersonation` key and writes a `log_audit` entry (impersonated admin's `user_id`, target `org_id`, superadmin origin in metadata, no key material); fails closed — on audit failure the key is revoked and the request 500s. Response change is additive (`token` kept, `label`/`expires_in` added).
- Added `session_ttl_hours` to all 9 exhaustive `Config` literals in `apps/backend/tests/`.
- Tests: 8 new integration tests in `tests/http_auth_test.rs` (TTL + cookie `Max-Age`, expired cookie → 401, two concurrent sessions, cap + sweep, impersonation leaves web sessions live, exactly one audit entry via `/internal/audit`, logout scoped to one session, cross-tenant impersonation key) and 5 inline tests in `queries.rs`.
- Static review pass over the whole diff (no compiler available): reverted a non-additive `list_all_org_keys` expiry filter that would have broken the admin UI's "Revoke N expired" action (`apps/admin/src/pages/ApiKeys.tsx`), and corrected a misleading comment.
- Docs: `apps/backend/AGENTS.md`, `deploy/u2s/README.md`, `deploy/oracle/README.md`, `apps/backoffice/CLAUDE.md`.

## LEFT
- [ ] Migration `run_v113` to revoke pre-existing `web-session` rows with `NULL expires_at` (in progress).
- [ ] **Blocked — needs a maintainer.** This sandbox denies writes under `.claude/**`, so three skill reference files could not be updated. The replacement text is in the PR description:
  - `.claude/skills/nexus-mind-business/references/gaps.md` — entry 3 (session gap) should be removed per the file's own "remove an entry when it is fixed" rule, entries 4-8 renumbered, and entry 4's impersonation clause trimmed since impersonation is now audited.
  - `.claude/skills/nexus-mind-business/references/roles.md:27` — "Each login revokes the user's previous web session" is now false.
  - `.claude/skills/nexus-mind-qa/SKILL.md:18` — should mention the TTL next to `COOKIE_SECURE`.
- [ ] **Verification: not run locally.** `cargo` is unavailable in this sandbox (every probe was refused), so nothing is compile-checked, `cargo fmt`/`clippy`/`test` were not executed, and formatting is hand-matched rather than rustfmt-applied. This must pass the CI Backend job before merge.
