# apps/admin

The org admin: memories, projects, users and roles, code, tasks, SDD, harnesses, usage, audit and the Factory UI. React 19, Vite 5, Tailwind 4, TanStack Query 5, React Router 6, Radix/shadcn New York, Recharts 2.15, Vitest 2 + Testing Library. Design record: `DESIGN.md` (Audit1 Lime); product record: `PRODUCT.md`. Load the `nexus-mind-design` skill before touching UI.

## Rules

- **The owner reviews visible admin changes before they ship** (`PRODUCT.md`). Every UI PR includes 1440/390 light/dark screenshots (`nexus-mind-qa`).
- **All NexusMind API calls go through `src/api/client.ts`.** It handles the session cookie (`credentials: 'include'`), errors and the `VITE_API_URL` base. Don't `fetch` `/v1` from components (direct calls to third parties, like the GitHub check in `pages/Code.tsx`, are the exception).
- **Route guards don't secure anything.** `AdminRoute` checks only that a session exists. Gate UI with `session.user.permissions.includes(...)` from `useAuth()` (pattern: `pages/Tasks.tsx`) and the nav's `requiredPermission`; the backend still has to enforce the rule.
- **New route:** add it to `App.tsx`, the nav in `components/Layout.tsx`, and `config/disabled-sections.ts` if the `only-context` profile must hide it. `vite.config.ts` rejects unknown `VITE_ADMIN_PROFILE` values at build time.
- **Missing data is "—", never 0.** Charts plot only real values (`DESIGN.md` §Charts).
- **Escape untrusted text in raw-HTML contexts** (force-graph labels) with `escapeHtml` from `src/lib/utils.ts`.
- **Shared components keep their props.** `components/ui/<Capitalized>` adapters are used across many routes; extend them, don't change their API.

## Test

`npm run test` (vitest, jsdom, setup `src/test/setup.ts`, render helper `src/test/render.tsx`). Tests sit next to the code as `*.test.ts(x)`. `npm run build` runs `tsc -b`, the only typecheck. CI uses Node 20; run `mise exec node@20 -- npm run test` here if results look odd.

## Run

- `npm run dev` serves :3000 and proxies `/v1` to a local backend on :8080.
- Live read-only preview against production is described in `dev/README.md` (port 3005; writes are blocked by the proxy). Never put credentials in `.env.development.local`.
- `/design-system` is a dev-only token and component preview.
