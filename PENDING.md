# PENDING — issue #487 (web session expiry + concurrent sessions)

## Done
- Investigated: `create_web_session_key` (queries.rs:3712), `get_org_admin_key` (:5154),
  `validate_api_key` (:47), `auth.rs` login/logout/cookie, `internal::impersonate_org`,
  `list_all_org_keys` (:6904), `log_audit` (:1466). Only 3 call sites to change.

- [x] config.rs: `--session-ttl-hours` / `SESSION_TTL_HOURS` (default 168) + clamp helper + tests
- [x] queries.rs: session TTL on insert, per-user cap (5) + sweep, `create_impersonation_key`
      (1h, audited), `get_org_admin_key` -> impersonation key, expiry comparison normalised
      through `datetime()` (`KEY_NOT_EXPIRED`)
- [x] auth.rs: cookie `Max-Age` = TTL, TTL passed to `create_web_session_key`
- [x] internal.rs: impersonate returns label + expires_at (backoffice client type updated)
- [x] tests: 7 inline in queries.rs (`web_session_tests`) + 5 in http_auth_test.rs; all 8
      test `Config` literals updated — `cargo test --lib web_session_tests` 7/7,
      `cargo test --test http_auth_test` 25/25
- [x] docs: config doc comment + apps/backend/AGENTS.md "Run" section

## Left
- [ ] waiting on: `cargo test --lib config::tests`, `cargo test --lib key`, `cargo clippy --lib`
- [ ] NOT DONE (blocked): `.claude/skills/nexus-mind-business/references/gaps.md` entry 3 and
      `references/roles.md` line 27 — writes under `.claude/` are denied in this sandbox.
      A maintainer should mark gaps entry 3 fixed (sessions expire after
      `SESSION_TTL_HOURS`, 5 live sessions per user, impersonation is its own audited 1h key;
      residual gap: absolute TTL only, no idle timeout, no "sign out everywhere") and drop
      "impersonation" from the unaudited list in entry 4.
