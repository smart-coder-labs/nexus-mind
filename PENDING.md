# PENDING — issue #487 (web session expiry + concurrent sessions)

## DONE
- Checked repo state (clean at ed78ea6), no prior WIP.

## LEFT
- Investigate auth/session code (queries.rs, api/auth.rs, api/internal.rs, config.rs).
- Implement: session TTL config, expires_at on web-session, session cap + sweep, separate impersonation key + audit.
- Tests in apps/backend/tests/http_auth_test.rs + inline queries.rs tests.
- Docs: default TTL documented, references/gaps.md entry 3 updated.
- Verify: cargo fmt/clippy/test.
