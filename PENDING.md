# PENDING — issue #487 (session TTL, concurrent sessions, impersonation path)

## Done
- `config.rs`: `--session-ttl-hours` / `SESSION_TTL_HOURS`, default 168, documented.
- `db/queries.rs`:
  - `WEB_SESSION_LABEL`, `IMPERSONATION_LABEL`, `MAX_LIVE_WEB_SESSIONS = 5`,
    `IMPERSONATION_TTL_HOURS = 1`.
  - `insert_session_key` writes `expires_at` via SQLite `datetime('now', '+N hours')`
    so it string-orders correctly against `datetime('now')` in `validate_api_key`
    (chrono's `...T...Z` does not — `'T' > ' '` would let an expired key pass).
  - `sweep_dead_session_keys` deletes revoked/expired session rows.
  - `create_web_session_key(.., ttl_hours)`: sweeps, inserts with TTL, then caps
    live sessions at 5 by revoking the least recently used — no blanket revoke.
  - `get_org_admin_key` → `create_org_impersonation_key`: `impersonation` label,
    1h TTL, returns `(raw_key, user_id)`, never touches `web-session` rows.
- `api/auth.rs`: TTL threaded into both login paths; cookie `Max-Age` mirrors it.
- `api/internal.rs`: `impersonate_org` uses the new path and writes an
  `impersonate`/`org` audit entry (impersonated user, target org,
  `origin: "superadmin"` metadata); response gains `expires_in_hours`.
- `apps/backoffice`: modal copy now states the real TTL and that the admin's own
  sessions are untouched.
- Tests: 7 inline tests in `queries.rs` (TTL, enforced expiry, second login,
  LRU cap, sweep, impersonation TTL/labelling, cross-tenant) and 8 in
  `tests/http_auth_test.rs` (TTL row, cookie Max-Age, expired cookie → 401,
  two live sessions, logout scope, cap + sweep, impersonation + audit).
- `apps/backend/AGENTS.md`: new "Sessions" section documenting the default TTL.
- 9 test `Config` literals updated with `session_ttl_hours`.

## Left
- [ ] `cargo test --lib --test http_auth_test` green (compile running).
- [ ] BLOCKED: `.claude/skills/nexus-mind-business/references/gaps.md` entry 3
      still says sessions never expire — the write was denied by the sandbox.
      Needs a maintainer edit (text drafted in the PR summary).
- [ ] `apps/backoffice` has no `node_modules` here, so `tsc --noEmit` was not run.
