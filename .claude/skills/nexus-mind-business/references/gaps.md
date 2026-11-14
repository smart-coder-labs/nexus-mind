# Known gaps (2026-11-14, origin/main 5dc4664)

Re-check on `origin/main` before filing; remove an entry when it is fixed.

## Security / correctness

1. **`AdminRoute` checks only the session, not the role** (`apps/admin/src/App.tsx`). A member who types `/users` gets the page; only backend 403s protect the data.
2. **`topic_key` upsert is org-wide** (`db/queries.rs`, `WHERE org_id = ?1 AND topic_key = ?2`). Two projects using the same key overwrite each other and the row moves project.
3. **Web sessions never expire and each login revokes the previous one** (`create_web_session_key`). One browser per user; backoffice impersonation uses the same function, logging the impersonated user out.
4. **Backoffice `/internal/*`**: superuser key compared with `!=` (not constant-time, `api/internal.rs`), stored in `sessionStorage`, and none of its actions (org changes, impersonation, suspension) are audited.
5. **Admin permission list** lacks `project:read`, `code:read`, `convention:read`, `tag:read`, `collection:read`, so the sidebar hides those sections from `admin` even though the backend allows them via the privileged bypass. Intent unclear — ask before "fixing".
6. Factory action policy mode `always` (v111) is flagged dangerous in code; UI should make that obvious.
7. Memory `type` isn't validated server-side; `normalized_hash` doesn't dedupe.
8. `deploy.yml` also deploys production from branch `agent/autonomous-agents-mvp`.

## Design debt (from `design-audit.mjs`)

- `apps/admin/src/pages/Settings.tsx` hard-codes status colours (`#34d399`, `rgba(...)`) for automation types and log levels.
- `apps/admin/src/components/ui/Sparkline/Sparkline.tsx` uses literal rgb for trend colours instead of `status-*`.
- `apps/backoffice/src/pages/Search.tsx` uses `text-green-400` / `text-red-400`; `apps/backoffice/src/pages/Orgs.tsx` has icon-only close buttons without `aria-label`.
- ~880 arbitrary px classes in the admin.

## Doc / harness drift

- `Makefile` `mcp-build`/`mcp-inspect` and `docs/RUNNING.md` point at `apps/mcp/`, which doesn't exist (the MCP is the external npm package).
- `docs/AUTH_SPEC.md`, `docs/ARCHITECTURE.md`, `docs/ENGINEERING_PROCESS.md` describe JWT, SSO/SAML/SCIM, ABAC, role inheritance, Redis and sqlite-vss that the code doesn't have (`extends_json` is stored but never resolved).
- `infrastructure/terraform` and `firebase.json` describe a GCP/Firebase deploy no workflow uses.
- Only backend TODO: FTS tokenizer recall (`db/migrations.rs`).
