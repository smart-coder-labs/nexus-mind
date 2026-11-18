# PENDING — issue #487 (web session expiry + concurrent sessions)

## DONE
- Explored: queries.rs (`create_web_session_key`, `get_org_admin_key`, `validate_api_key`,
  `list_all_org_keys`, `log_audit`), api/auth.rs, api/internal.rs, config.rs, tests/http_auth_test.rs.

## LEFT
- [ ] config: `--session-ttl-hours` / `SESSION_TTL_HOURS` (default 168)
- [ ] queries: session TTL + per-user live-session cap + sweep of dead session rows
- [ ] queries: `create_impersonation_key` / org impersonation path (label `impersonation`, 1h TTL)
- [ ] auth.rs: pass TTL, mirror it as cookie `Max-Age`
- [ ] internal.rs: impersonation uses the new path and writes an audit entry
- [ ] tests: inline queries.rs tests + http_auth_test.rs HTTP tests
- [ ] docs: .env.example / AGENTS.md / gaps.md entry 3
- [ ] `cargo check` + targeted `cargo test`
