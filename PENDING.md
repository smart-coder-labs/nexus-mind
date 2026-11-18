# PENDING — issue #487 (web session expiry + concurrent sessions)

## DONE
- `Config::session_ttl_hours` (`--session-ttl-hours` / `SESSION_TTL_HOURS`, default 168) next to
  `cookie_secure`, clamped to 87_600h.
- `create_web_session_key` now: sweeps the user's dead `web-session` rows, inserts with
  `expires_at` in SQLite's `datetime()` format (so `validate_api_key`'s string comparison is
  correct), then revokes all but the 5 most recently used live sessions. No blanket revoke.
- `set_session_cookie` mirrors the TTL as the cookie `Max-Age`.
- New `create_impersonation_key` (label `impersonation`, `IMPERSONATION_TTL_HOURS = 1`, one live
  key per user, never touches `web-session` rows); `get_org_admin_key` returns `OrgImpersonation`.
- `internal::impersonate_org` writes one `org.impersonate` audit entry (impersonated user, target
  org, superadmin origin in metadata) and returns `label` + `expires_at` alongside `token`.
- Exchanging an `impersonation` key at `/v1/admin/auth/login` keeps the 1h TTL instead of
  minting a 168h web session.
- Migration v113 backfills `expires_at` onto legacy NULL session rows.
- Backoffice: `impersonateOrg` type widened, modal copy corrected.
- Tests: 6 inline in queries.rs, 1 in migrations.rs, 4 in http_auth_test.rs, 5 in the new
  tests/internal_impersonation_api.rs; `session_ttl_hours` added to all 9 `Config {}` literals.
- Docs: .env.example, apps/backend/AGENTS.md, docs/RUNNING.md.
- Three static review rounds (no Rust toolchain on this box); all findings fixed.

## LEFT
- Nothing in scope. CI must run build/test/clippy — nothing was compiled or executed here.

## Known limitations (carried into the PR body)
- Writes under `.claude/` are denied in this sandbox, so `references/gaps.md` entry 3 and
  `references/roles.md:27` could not be edited; replacement text is in the PR body.
- `apps/landing/.astro/*` deletions came from the earlier WIP checkpoint; those files are
  gitignored (force-added on main) and `git add -f` / `git checkout` are denied here.
