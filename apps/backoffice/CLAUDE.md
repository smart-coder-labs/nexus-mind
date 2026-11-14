# apps/backoffice

Internal panel for the platform superadmin across all orgs: orgs, users, search, audit and impersonation. React 19 + Vite + Tailwind 4, dark only, amber accent (`src/index.css`) so staff never mistake it for a customer's admin. It is not the org admin (`apps/admin`).

- **Auth is a single env key, not a user.** `src/api/client.ts` sends `Authorization: Bearer <SUPERUSER_KEY>` (kept in `sessionStorage['bo_key']`) to `/internal/*` (`apps/backend/src/api/internal.rs`). Those routes bypass the org auth and audit middleware. Treat every new internal action as high-risk, and audit it in the handler (today none are; see `nexus-mind-business` `references/gaps.md`).
- **No component library.** Pages in `src/pages/` use token classes (`bg-surface-secondary`, `text-text-tertiary`, `*-accent-primary`, `text-status-*`) and `components/Layout.tsx`. Copy the nearest page. `accent-blue*` are deprecated aliases of amber; use `accent-primary`.
- **`npm test` is an accessibility check** (`scripts/check-accessibility.mjs`): `text-quaternary` contrast ≥ 4.5:1 on three surfaces, a reduced-motion block, and five search `aria-label`s. If you rename one of those labels or change a text token, update the script in the same PR.
- `npm run dev` serves :3001. CI builds and tests with Node 20.
