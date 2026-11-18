# PENDING — issue #487 (web session expiry + concurrent sessions)

## Done
- `config.rs`: `--session-ttl-hours` / `SESSION_TTL_HOURS`, default 168.
- `queries.rs`: `WEB_SESSION_LABEL`, `IMPERSONATION_LABEL`, `IMPERSONATION_TTL_HOURS`,
  `MAX_LIVE_WEB_SESSIONS`; `sweep_dead_session_keys`, `enforce_web_session_cap`;
  `create_web_session_key` now takes a TTL, sets `expires_at` and no longer
  blanket-revokes; new `create_impersonation_key`; `get_org_admin_key` returns
  `(user_id, raw_key)` and mints an impersonation key.
- `api/auth.rs`: TTL threaded into the session key and the cookie `Max-Age`.
- `api/internal.rs`: `impersonate_org` uses the impersonation path and writes a
  `org.impersonated` audit entry.
- Existing test `Config` literals updated with `session_ttl_hours`.

## Left
- Inline `queries.rs` tests (expiry, cap, sweep, impersonation isolation).
- `tests/http_auth_test.rs`: TTL/Max-Age, expired cookie → 401, two live logins,
  logout revokes only the presented key, impersonation (label, TTL, target
  sessions survive, audit entry, cross-tenant).
- Docs: backend AGENTS.md + `references/gaps.md` entry 3.
- `cargo check` / targeted `cargo test`.
