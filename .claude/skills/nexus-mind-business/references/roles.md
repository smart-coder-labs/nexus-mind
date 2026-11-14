# Actors, roles, permissions

## Roles (`apps/backend/src/models/types.rs`, `UserRole`)

| Role | Sees | Notes |
|---|---|---|
| `super_user` (custom string) | the whole org | the only role with org-wide visibility (`is_super_user`) |
| `admin` | projects it belongs to | "privileged": bypasses `require_permission`, but data reads stay project-scoped |
| `member` | its projects | `memory:read/write/delete/search`, `policy:read` |
| `viewer` | its projects | `memory:read/search` |
| custom (`security-officer`, `dev-senior`, `dev-junior`, `auditor`, or org-defined) | per `roles` row | resolved by name, org or global (`org_id` NULL) |

Resolution: `get_role_permissions` in `apps/backend/src/db/queries.rs`. Admin and super_user lists are hard-coded, except `autonomous_agent:*` and `factory_policy:*`, which come from the persisted `admin_template` / `super_user_template` rows. `factory_policy:read/write` is granted to `super_user_template` only.

Permission families: `memory:*`, `user:{invite,revoke}`, `audit:{read,write}`, `settings:write`, `policy:*`, `project:*`, `client:*`, `session:read`, `api_key:read`, `convention:*`, `webhook:read`, `code:{read,write,index}`, `collection:*`, `backup:*`, `graph:read`, `tag:*`, `harness:{read,write,download,install,review_config}`, `task:{read,write,assign,delete,manage}`, `sdd:{read,write,delete}`, `migration:{read,write,review}`, `autonomous_agent:{read,create,update,enable,run,cancel,manage_connectors}`, `factory_policy:{read,write}`.

## Where checks happen (`apps/backend/src/api/helpers.rs`)

- `require_permission` — privileged roles bypass; with a `project`, the `project_members` role overrides the global one; non-members of an existing project get 403.
- `require_explicit_permission` — no bypass; used for `autonomous_agent:*` and `factory_policy:*`, fails closed.
- `user_is_visible_to_actor`, `project_is_visible_to_actor` — admins only see users/projects they share.
- Some admin endpoints just check `auth.role.is_privileged()`.

## Authentication

- **API keys** `nm_` + 64 hex (`apps/backend/src/auth/api_keys.rs`); only the SHA-256 hash is stored. Middleware (`api/middleware.rs`) reads the `nexusmind_session` cookie first, then `Authorization: Bearer`.
- **Web login** (email + password, bcrypt, or API key) creates a `web-session` api_key set as an HttpOnly SameSite=Lax cookie; `Secure` follows `COOKIE_SECURE` (default true). Each login revokes the user's previous web session.
- **`/v1/admin/auth/me`** returns role + permission list; the admin UI gates on it.
- **GitHub OAuth** (`api/github_auth.rs`) connects repos for code/Factory; it is not a login.
- **Factory runners** authenticate with their own token hash (`factory_runners`), on routes outside the protected router.
- **Backoffice superadmin** — `/internal/*` requires `Authorization: Bearer <SUPERUSER_KEY>` (env), not an org role. The backoffice keeps it in `sessionStorage['bo_key']`. It is a different thing from `super_user`.

## Admin UI gating (`apps/admin/src`)

- `App.tsx`: `ProtectedRoute` (session), `AdminRoute` (**session only, no role check**), `SuperUserRoute` (`role === 'super_user'`, used by `/audit`).
- `components/Layout.tsx`: the sidebar hides items by `requiredPermission` or `adminOnly`. Pages also check `permissions.includes(...)`.
- `config/disabled-sections.ts`: `TEMPORARILY_DISABLED` (`/sessions`, `/api-keys`, `/agents`, `/policies`, `/webhooks`) and the `only-context` profile, which hides usage, collections, sessions, tasks, sdd, harnesses, factory, api-keys, agents, policies, webhooks, audit, backups. Disabled routes redirect to `/`.
- UI hiding is never security: the backend must enforce every rule on its own.
