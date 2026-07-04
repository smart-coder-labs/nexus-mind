# NexusMind Design Direction

Owner: nm-design-director. Single source of truth for UI work across `apps/admin`, `apps/backoffice`, `apps/landing`. Implementers follow this document over ad-hoc judgment; if a rule here conflicts with existing code, the code is the debt.

Version: 1.0 (2026-07-02)

## 1. Thesis

NexusMind is a memory system for engineering teams — its UI should feel like a **quiet, precise instrument**: dark, calm, and legible, where the team's knowledge (memories, code, activity) is the only thing that glows. We keep the existing Apple-derived language (single Action Blue accent, near-black surfaces, pill CTAs, no decorative shadows) but adapt it from a *marketing* spec (`apps/admin/DESIGN.md`) to a *data-dense product* spec. The core correction of this direction: **readability over miniature chrome** — the current UI sets nearly everything at 12px, which undermines the entire Apple restraint story.

One design language, three volumes:

| Surface | Volume | Identity |
|---|---|---|
| `apps/admin` | Product (dark) | Near-black gallery, Action Blue `#0066cc`, dense but legible |
| `apps/backoffice` | Internal ops (dark) | Same chassis and rules, **amber** `#f59e0b` accent to scream "you are superadmin" |
| `apps/landing` | Marketing (light-first) | Apple light canvas, iOS blue `#007aff`, full DESIGN.md typography (17px body) |

## 2. Color tokens

### Admin (`apps/admin/src/index.css`)

Keep the existing `:root` palette. Required additions/corrections:

```css
--color-accent-purple: #bf5af2;        /* NEW — referenced by Dashboard TYPE_COLORS but never defined */
--color-focus-ring: #0071e3;           /* Focus Blue, keyboard focus only */
```

Rules:
- Action Blue `#0066cc` is the only interactive accent. Status colors (`success/warning/error`) are for state, never decoration.
- No new hex literals in TSX. Every color goes through a token (`bg-accent-blue`, `text-text-secondary`, …). Existing inline hexes (`bg-[#1d1d1f]`, `bg-[#272729]`) are debt to be replaced with `bg-background-secondary` / `bg-background-tertiary` when a file is touched.
- Elevation = surface change (`#1d1d1f → #272729 → #2a2a2c`), not shadows. `shadow-2xl`/`shadow-xl` on modals/popovers are debt; replace with a 1px `border-border-primary` plus a surface step when touched.

### Backoffice (`apps/backoffice/src/index.css`)

Amber-on-dark identity stays (deliberate "different room" signal). Two corrections when touched:
- `--color-accent-blue: #f59e0b` is a semantic lie. Rename the family to `--color-accent-primary` (keep `--color-accent-blue` as an alias during migration; do not break pages in the same batch).
- Body text must be neutral, not violet: `--color-text-secondary` should be a neutral `#b5b0c4`-range gray, with violet reserved for links/emphasis (`--color-accent-violet`).

### Landing (`apps/landing/src/styles/apple-ds.css`)

Light-first with dark mode, iOS blue `#007aff`. Fine as-is. Remove the `VT323` font import (unused pixel font — dead weight on a marketing page) when the file is touched.

## 3. Typography

### Admin & backoffice — the dashboard scale

Font: `system-ui, -apple-system, "Inter"` (unchanged). Weight ladder 400 / 500 / 600 — no 300 in product UI.

| Role | Size / weight / tracking | Use |
|---|---|---|
| `page-title` | 22px / 600 / -0.3px | One per page, top of content |
| `page-subtitle` | 13px / 400 / normal, `text-secondary` | One line under the title |
| `section-title` | 15px / 600 / -0.2px | Card headers, panel titles |
| `body` | 13px / 400 | Table cells, form values, list items, descriptions |
| `label` | 12px / 500 | Form labels, column headers (sentence case; uppercase + `tracking-wide` only for column headers and tiny group labels) |
| `caption` | 12px / 400, `text-tertiary` | Metadata, timestamps, counts |
| `micro` | 11px / 400 | Badges, kbd hints — nothing below 11px |

Hard rules:
- **`text-xs` (12px) is no longer the default voice.** Page titles at `text-xs font-semibold` (current state in Layout and most pages) are wrong — retype to the scale above whenever a file is touched.
- Nav items: 13px. Sidebar app name: 13px/600. Table body: 13px, table headers 12px/500.
- Numbers in stat tiles: 28px/600 with `tabular-nums`; their labels 12px/400 `text-tertiary`.
- Line-height: 1.5 for body, 1.2 for titles.

### Landing — the marketing scale

Follow `apps/admin/DESIGN.md` typography verbatim (17px body, display at 40–56px/600 with negative tracking). It was written for this surface.

## 4. Spacing, radius, layout

- 8px base grid. Page gutter: 24px (`p-6`). Card padding: 20px (`p-5`). Gap between cards: 16px.
- Radius grammar (admin + backoffice): `8px` utility controls and nav items, `11px` inputs and inline alerts, `18px` cards, modals, popovers, `9999px` pill buttons and badges. Nothing else.
- Page skeleton (every admin/backoffice page): header block (page-title + subtitle + primary action right-aligned) → optional filter row → content. Max content width `1280px`, centered, on wide pages with prose or forms; data tables may go full-width.
- Sidebar: fixed 208px (current `w-52`), true black. Nav grouped by domain with 11px/500 uppercase group labels:
  - **Overview**: Search, Dashboard
  - **Knowledge**: Memories, Collections, Tags, Conventions, Sessions
  - **Code**: Projects, Code
  - **Access**: Users, Roles, API Keys, Agents, Policies
  - **System**: Webhooks, Audit Log, Settings

## 5. Component patterns

- **Buttons**: primary = Action Blue pill; secondary = ghost pill with 1px border; destructive = `status-error` pill, always paired with a confirm modal. Height 36px default (`h-9`), 44px only for hero/login CTAs. Press state `scale(0.95)` (already in `Button.tsx`) is the system micro-interaction — keep it, but wrap in reduced-motion respect.
- **Inputs**: 36px height, `radius 11px`, `bg-white/[0.04]`, 1px `border-border-primary`, focus = 2px `--color-focus-ring` ring. Label above at 12px/500. Error text 12px in `status-error` below, never placeholder-only errors.
- **Tables**: 13px body, 12px/500 headers, row height ≥ 40px, row hover `bg-white/[0.03]`, hairline row dividers `border-border-secondary`. No zebra striping.
- **Badges**: pill, 11px/500, tinted background at 10–12% of a status/accent color plus 20% border — the existing `BADGE_ACCENT` pattern is correct; systematize it.
- **Stat tiles**: one neutral treatment — `bg-white/[0.04]`, 1px `border-border-primary`, 18px radius, 28px/600 `tabular-nums` number, 12px `text-tertiary` label. No per-metric accent tints: color on a metric tile must encode state (an error count in `status-error`), never identity — per-metric decorative tints read as arbitrary (QA-confirmed on Dashboard).
- **Empty states**: card/page-level empties always use the `EmptyState` component — icon, one-line "what this is", one primary action. Small inline analytics sub-tiles (sparklines, mini-charts) may instead use a centered 13px `text-tertiary` caption — `EmptyState`'s footprint is too heavy there. Never a bare unstyled "No data" string either way.
- **Loading**: `Skeleton` shapes that mirror the final layout. Never spinners for full-page loads; spinners only inside buttons.
- **Errors**: inline alert (11px radius, `status-error` tint at 8%, icon + message + retry action). Never a toast-only failure for a page-level fetch.
- **Modals**: `18px` radius, surface `#1d1d1f`, 1px border, no shadow, `scaleIn` 220ms. One primary action, right-aligned.

## 6. Motion & accessibility floor

- Durations: 150ms hover/color, 220ms enter (`--ease-apple`). No exit animations longer than 150ms.
- `prefers-reduced-motion: reduce` must disable all `animate-*` keyframes and framer-motion springs — add a global CSS guard in each app's index.css.
- Every interactive element has a **visible** `:focus-visible` state: 2px solid `--color-focus-ring` with 2px offset, implemented as CSS `outline` (not Tailwind `ring`) — outline-offset's gap is transparent, so it renders correctly on any surface without ring-offset-color mismatches. `focus-visible:outline-none` without a replacement is a defect.
- Icon-only buttons require `aria-label` (Button.tsx already warns — fix the call sites, don't silence the warning).
- Text contrast: `text-quaternary` (#555) fails on `#1d1d1f` for body-size text — allowed only for decorative/disabled elements, never for the sole label of an action or information.

## 7. Known debt inventory (audit 2026-07-02)

1. Miniature typography: `text-xs` used as page-title/nav/body voice across all admin pages and Layout.
2. `var(--color-accent-purple)` referenced in `Dashboard.tsx` TYPE_COLORS but undefined → silently renders as nothing.
3. Inline hexes: `bg-[#1d1d1f]` (Layout modals/popovers), `bg-[#272729]`/`bg-[#1d1d1f]` (Button `subtle`), others.
4. No visible focus rings on admin sidebar nav, notification bell, sign-out.
5. No `prefers-reduced-motion` handling in any app.
6. Shadows on modals/popovers (`shadow-2xl`, `shadow-xl`) contradict the no-shadow elevation model.
7. 17-item flat sidebar nav — needs domain grouping (see §4).
8. Backoffice: amber tokens named `--color-accent-blue`; violet `text-secondary` used for body copy.
9. Landing: unused `VT323` font import; third accent blue (`#007aff`) — accepted as the light-surface variant, not debt.
10. Monolithic pages (`Memories.tsx` 3351 lines, `Settings.tsx` 1418, `Dashboard.tsx` 1291) — refactor only opportunistically, never as a pure-styling batch.

### QA baseline findings (nm-ux-qa, 2026-07-02, all 17 admin routes at 1440px/390px)

Functional bugs:
11. Relative time renders future tense for past events ("Last used: in about 5 hours") on `/agents` cards and `/api-keys` — likely date-fns formatDistance argument order / addSuffix.
12. A 429 during the auth/me bootstrap logs the admin out to `/login`. Transient rate limiting must never destroy a session (frontend: `auth/AuthContext.tsx` retry/backoff; backend rate limits are out of UI scope).

UX/visual:
13. `/conventions` broken at 390px (subtitle one-word-per-line overlapping title, header actions overflow, clipped empty state); desktop header layout is unique vs. every other page.
14. `/users`: duplicated "Active Active" status per row; actions column mixes plain links with a wrapping boxed button; native `<select>` role picker off-system.
15. Primary CTA inconsistency: gray ghost "New collection" / "New memory" vs. blue pill everywhere else. Rule: the page's single primary action is always the Action Blue pill (§5).
16. `/memories` at 390px: header actions and tab bar overflow (New memory/Export hidden, tabs cut off), table clips right with no scroll affordance.
17. Widespread sub-AA contrast: `/projects` chips and icon actions, `/code` segmented tabs, `/tags` count badges, timestamps, struck-through completed items on Getting Started. Enforce §6 contrast rule (`text-quaternary` never for sole labels).
18. `/settings`: three different Save button styles on one page, duplicated "My Profile" heading, inconsistent label casing, broken logo thumbnail.
19. `/agents` vs `/api-keys` duplicate the same data (cards vs table); three identical "Admin User" cards are indistinguishable. Needs a product decision on what Agents is for — raised to main; do not restyle into permanence meanwhile.
20. Minor: `/sessions` content max-width inconsistent; native date inputs on `/memories` and `/audit` filters; memories type-badge styling inconsistent (outline vs fill).
21. **CRITICAL (landing)**: every React island fails to hydrate (`react-dom/client` does not export `createRoot`) — NavbarCTA, CTAButtons, HeroBadge, WaitlistFormReact. The waitlist form falls back to a native submit with zero network requests: the page's only conversion silently loses leads. Likely react/react-dom version mismatch in `apps/landing`. Interest chips also show no selected state (recheck after the fix).
22. **CRITICAL (backoffice, backend-owned)**: `GET /internal/users` 500s — "Invalid column type Null at index: 2, name: email" for a seeded user with NULL email; backoffice Users page shows the raw error string. Backend must deserialize email as `Option` (the per-org endpoint already tolerates NULL). Escalated to main — outside UI-team scope. Admin-side symptom: empty email line in `/users`, indistinguishable "Admin User" cards in `/agents`.
23. Backoffice purple body links ("9 users · 21 memories") below AA contrast — fold into the backoffice cleanup batch.
24. **New (2026-07-03, from nm-authz-audit landing project-membership filtering at the DB layer)**: non-admin members now see fewer/empty results on screens that were previously unfiltered. Not a bug, but an empty-state UX gap — a member hitting a zero-result list must read as "you don't have access to items here," not as "this feature is broken" or "there is genuinely nothing." Affects: `pages/Memories.tsx` (list/search/export/session-memory expansion/collection view), `pages/Sessions.tsx`, `pages/Projects.tsx`, `pages/Collections.tsx`, `pages/Settings.tsx` (bulk export), `components/CommandPalette.tsx` (global search). Admin backoffice unaffected (admins see all). Not urgent.

### B2 component adoption findings (nm-ux-qa, 2026-07-03 — component primitives themselves APPROVED, these are consumer-side)

25. [MODERATE, a11y] `pages/Projects.tsx:360` overrides `SelectTrigger` with `focus:outline-none` — the only live `Select` consumer in the app has no visible Tab focus ring. One-line fix: drop the override.
26. [MODERATE] B2 `Table` component has zero live consumers — `/memories`, `/apikeys`, `/orgs`, `/conventions` all still hand-roll their own table markup (12px cells, bold non-uppercase headers, diverges from §5). QA verified `Table` at code level only; needs at least one adopting page before live sign-off.
27. [MINOR] The Add Policy modal uses a raw ~30px input instead of the B2 `Input` component — inconsistent chassis inside a flagship modal.
28. [MINOR, opportunistic] Backoffice: 67 `accent-blue` usages across 8 files still reference the pre-B4 name (now an alias to `accent-primary`) — migrate to `accent-primary` directly and drop the alias whenever those files are next touched, not a dedicated batch.

## 0. FREEZE — LIFTED (2026-07-03)

Freeze was in effect while main ran the full test suite + push. Main confirmed push complete, tests green, 2026-07-03 — normal dispatch resumed. Pre-lift stable-tree confirmations (kept for record): nm-ui-impl-b tsc clean/admin+landing build green, B3.1 already complete pre-freeze; nm-ui-impl-a tsc clean/21-21 page tests, Users.tsx+Dashboard addenda held untouched; nm-ux-qa held all verification, no dev-server touches. Post-lift: A2 resumed (impl-a), B4 dispatched (impl-b), QA re-verifies resumed (B1 drawer, B3 landing).

## 9. Prioritized backlog (post-A1/B1)

1. **B3 (CRITICAL, jumps queue after B2)**: landing repair — fix React island hydration (#21, react/react-dom mismatch), verify waitlist submits over the network end-to-end, give interest chips a visible selected state, remove VT323 import, add reduced-motion guard.
2. **A2**: fix relative-time bug (#11) + `/users` cleanup (#14) — `pages/Agents.tsx`, `pages/ApiKeys.tsx`, `pages/Users.tsx`.
3. **A3**: `/conventions` mobile + header normalization (#13); primary-CTA rule on `/collections` and `/memories` headers (#15).
4. **A4**: `/memories` responsive header/tabs/table (#16) — styling only, no refactor of the 3351-line file.
5. **B4**: backoffice cleanup — accent rename + neutral body text (#8), purple link contrast (#23).
6. **A5**: `/settings` consolidation (#18); contrast sweep (#17) across `/projects`, `/code`, `/tags`.
7. **A6**: membership-filtering empty states (#24) on `pages/Memories.tsx`, `pages/Sessions.tsx`, `pages/Projects.tsx`, `pages/Collections.tsx`, `pages/Settings.tsx` bulk export. Use the existing `EmptyState` component (§5) with copy specific to access scope ("No memories in projects you have access to" style — not a generic "No data" default); bulk export with zero exportable items must not silently produce an empty file, show an explicit notice instead.
8. **B5**: membership-filtering empty state (#24) on `components/CommandPalette.tsx` global search — zero cross-domain results must render a clear "no results" state, not a blank/broken-looking list.
9. **A5 addendum**: drop the `focus:outline-none` override on `Projects.tsx:360` `SelectTrigger` (#25) — natural fit since A5 already touches `/projects` for the contrast sweep.
10. **A4 addendum**: adopt B2 `Table` on `/memories` (#26) — natural fit since A4 is already the `/memories` responsive batch; gives QA a live page to sign off `Table` against. The other three hand-rolled tables (`/apikeys`, `/orgs`, `/conventions`) stay as opportunistic "when touched" debt, not a dedicated batch.
11. Unscheduled/opportunistic: Add Policy modal raw input → B2 `Input` (#27) — fix whenever `Policies.tsx`/its modal is next touched.
12. Session-resilience fix (#12) — assigned later with an explicit file grant on `auth/AuthContext.tsx` (outside both implementers' default zones).
13. Backend-owned, escalated to main: NULL-email 500 (#22), 429 rate-limit tuning (#12 backend half), `/agents` product decision (#19).

## 8. Working agreement (implementers)

- File ownership per batch is assigned explicitly by nm-design-director; never edit outside your assigned file list.
- nm-ui-impl-a owns `apps/admin/src/pages/**`. nm-ui-impl-b owns `apps/admin/src/components/**`, `apps/admin/src/index.css`, `apps/backoffice/**`, `apps/landing/**`.
- Apply the "when touched" rules (token hygiene, type scale, focus rings) to every file you edit, but do not expand a batch beyond its file list.
- No commits. Report back with what changed, per file, and any rule you could not satisfy.

## 10. Batch ledger (nm-design-director updates on every report — check before dispatching)

| Batch | Owner | Scope | Status |
|---|---|---|---|
| A1 | nm-ui-impl-a | `pages/Dashboard.tsx` retype + tokens + a11y | **complete** (approved; QA visual notes → A2) |
| B1 | nm-ui-impl-b | `index.css` tokens/reduced-motion, `Layout.tsx` nav/focus, `Button.tsx` | **complete** (QA PASS; 2 a11y follow-ups → B2 addendum) |
| B2 | nm-ui-impl-b | ui `Input`/`Select`/`Modal`/`Table`/`Badge` per §5 + Badge `purple` style key | **complete** (typecheck+build green; Table `striped` now defaults false; Badge `primary` solid→tint; Select chassis ready for A2's native-select swap) |
| A2 | nm-ui-impl-a | `Agents.tsx`/`ApiKeys.tsx` relative-time bug, `Users.tsx` cleanup, `Dashboard.tsx` focus-ring token swap + **addendum**: neutral stat tiles (§5) + Getting Started strikethrough contrast + 390px string-tile truncation + convert Dashboard FOCUS_CANVAS/FOCUS_TILE from Tailwind `ring` to CSS `outline` per §6 | **in flight — paused stable for freeze** (2026-07-03). DONE: Dashboard.tsx focus-ring swap; Agents.tsx + ApiKeys.tsx full retype+bugfix (root cause: backend writes zone-less UTC datetimes via SQLite `datetime('now')`, frontend `new Date()` parsed the space-form as local time, shifting past events into the future — fixed with an inline `toDate()` UTC normalizer, guarded against invalid zoned/date-only strings). PENDING (held, not started): `Users.tsx` (untouched), Dashboard addenda (tint removal, contrast, truncation, ring→outline). |
| A1-QA | nm-ux-qa | Formal A1 verification at 1440/390 | **complete** — PASS (type scale, tokens, mobile, console clean); one fix-forward finding folded into A2 addendum (ring→outline) |
| B3 | nm-ui-impl-b | CRITICAL landing repair (#21) + VT323 + reduced motion | **complete, QA re-verified PASS post-freeze** (fresh Vite cache, :4321): zero console/hydration errors, interest chips hydrate + toggle + register in form state, real POST to Supabase `/rest/v1/waitlist` with full payload + success toast. Note: impl-b's deletion of `apps/landing/pnpm-lock.yaml` as B3 hygiene was flagged by main pre-push and restored from HEAD — `apps/landing` diff is scoped to `astro.config.mjs` + `apple-ds.css` |
| B3.1 | nm-ui-impl-b | `Layout.tsx` drawer a11y from QA's B1 pass: `inert` on the mobile `<aside>` tied to `drawerOpen`; Escape closes the open drawer first in the global keydown (Layout.tsx:451); focus management (open→close button, Escape→hamburger) | **complete, QA re-verified PASS post-freeze** (390px, :3100): closed drawer inert+aria-hidden, 10 Tabs never land inside it, Escape closes + returns focus to "Open menu" trigger, bonus skip-to-content link verified working (also fixes the ~20-tab desktop sidebar gauntlet) |
| B2-QA | nm-ux-qa | Formal B2 verification (Input/Select/Modal/Table/Badge) | **complete** — primitives APPROVED (chassis, focus rings, reduced-motion, keyboard contract, badge grammar all match spec); 3 consumer-side adoption findings (#25-27) routed into A5/A4 addenda + opportunistic debt, not new batches |
| B1/B3-QA redrive | nm-ux-qa | Re-verify B3.1 drawer a11y (390px) + B3 landing post-cache-clear (:4321) | **complete** — both PASS, no findings. B1's original 2 drawer a11y bugs closed |
| A3 | nm-ui-impl-a | `/conventions` mobile/header (#13), primary-CTA rule (#15) | queued |
| A4 | nm-ui-impl-a | `/memories` responsive (#16) + **addendum**: adopt B2 `Table` (#26) | queued |
| B4 | nm-ui-impl-b | backoffice cleanup (#8, #23) | **complete** — `npm run build` green, backward-compatible (accent-blue kept as alias to new accent-primary, all 67 existing usages still compile). Contrast verified with a real WCAG script (not eyeballed): text-secondary 9.65:1, text-tertiary 5.77:1, text-quaternary 3.66:1 (intentionally dim/decorative-only). Dashboard.tsx + OrgDetail.tsx stat-link contrast fixed (was 2.86:1/3.57:1 FAIL → 5.77:1 PASS). Sent to QA for review |
| B5 | nm-ui-impl-b | membership-filtering empty state (#24): `CommandPalette.tsx` global search | **dispatched** 2026-07-03 |
| A5 | nm-ui-impl-a | `/settings` (#18), contrast sweep (#17) + **addendum**: fix `Projects.tsx:360` Select focus-ring override (#25) | queued |
| A6 | nm-ui-impl-a | membership-filtering empty states (#24): `Memories.tsx`, `Sessions.tsx`, `Projects.tsx`, `Collections.tsx`, `Settings.tsx` bulk export | queued (not urgent, from main 2026-07-03) |
| B5 | nm-ui-impl-b | membership-filtering empty state (#24): `CommandPalette.tsx` global search | queued (not urgent, from main 2026-07-03) |
| — | unassigned | `auth/AuthContext.tsx` 429 resilience (#12) | queued (explicit grant needed) |
| — | escalated to main | NULL-email 500 (#22), 429 backend tuning, `/agents` product decision (#19) | waiting on main/user |
