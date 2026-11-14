---
name: nexus-mind-design
description: NexusMind's visual systems as they exist in code — the admin's Audit1 Lime (shadcn New York, Geist, lime actions with dark ink), the backoffice's amber internal-ops theme, and the landing's SmartCoder lime — with tokens, components to reuse, charts, motion, copy and the QA bar. Use before creating or restyling any page, card, table, chart, form, modal or empty state in apps/admin, apps/backoffice or apps/landing, and when reviewing a UI diff.
---

# NexusMind design

Design is scoped per app. The records and the token sheets are the authority; this skill is the working summary.

| App | Record | Runtime tokens | Character |
|---|---|---|---|
| admin (org admins, daily ops) | `apps/admin/DESIGN.md`, `apps/admin/PRODUCT.md` | `apps/admin/src/styles/audit1-theme.css` (+ compat aliases in `apps/admin/src/index.css`) | Audit1 Lime: neutral surfaces, Geist, compact shadcn New York, restrained lime |
| backoffice (superadmins only) | — (comments in the token sheet) | `apps/backoffice/src/index.css` | dark only, amber accent so staff never confuse it with a customer admin |
| landing (public marketing) | `apps/landing/DESIGN.md`, `apps/landing/PRODUCT.md` | `apps/landing/src/styles/smartcoder.css` (`--sc-*`) | SmartCoder lime, Heebo, light/dark via `data-theme` |

The root `DESIGN.md` only points at these. Do not restore the September 2026 blue/glass system (PR #271), the black-sidebar/Heebo admin, or retired landing components: the owner replaced them.

## What to read

| Need | Read |
|---|---|
| Exact token and class names per app, and when to use each | `references/tokens.md` |
| Which component to reuse before writing a new one | `references/components.md` |
| The checklist a screen must pass (a11y, motion, charts, copy, layout) | `references/rules.md` |

## Workflow

1. Find the closest existing screen and the shared component (`references/components.md`). Reuse it. A second stat tile or empty state is a regression even if it looks the same.
2. Build with tokens only: Tailwind classes mapped to roles (`bg-primary text-primary-foreground`, `text-muted-foreground`, `bg-card`, `text-status-error`, `var(--chart-2)`). Literal colours belong only in the token sheets listed above.
3. Run the static audit on what you touched (from the repo root):
   `node .claude/skills/nexus-mind-design/scripts/design-audit.mjs <files or dirs>`
   No args = web-app files changed vs `origin/main`. Exit 1 on a MUST. The `design-guard` hook already reports what each edit adds.
4. Look at it: screenshots at 1440 and 390 wide, light and dark (admin theme lives in `localStorage['nexusmind-admin-theme']` and the `.dark` class on `<html>`). Recipe in the `nexus-mind-qa` skill. Go through `references/rules.md` against the images. For a larger change, give the screenshots to the `design-reviewer` agent instead of judging your own work.
5. Put the screenshots in the PR. The admin owner reviews visible admin changes before merge (`apps/admin/PRODUCT.md`), so a UI PR without images is not reviewable.

## Decisions that come up often

- **Lime is for actions, not decoration.** Filled lime always pairs with `primary-foreground` (dark ink, never white). Readable lime text on neutral surfaces is `brand-link` (olive in light, lime in dark). Don't turn statuses, panels or icons lime.
- **Status ≠ category.** Success/warning/error/info are semantic and carry a text label. Categorical series use `--data-lime/blue/teal/amber/rose` (or `--chart-1…5`) in that fixed order (`apps/admin/src/pages/dashboard/colors.ts`), never status colours.
- **Missing data is "—", never 0.** Real zeros stay visible. Never invent a "healthy %" or a metric the API does not return.
- **Charts** use the shadcn `ChartContainer` (`components/ui/shadcn/chart.tsx`) on Recharts 2.15.4, no entry animation, plus an accessible data table for analytical panels.
- **Counts** use `MetricSummary` in the responsive summary strip, not big hero numbers.
- **Nothing to show** → one `EmptyState` with a next action, not a grid of empty cards.
- **Graphs** (force-graph/WebGL) keep literal palettes in their `*graph*` utils because canvas can't read CSS variables; the audit only warns there.
- **New token?** Add it to the app's token sheet in both `:root` and `:root.dark`, plus the `@theme inline` mapping, with a comment naming its role. No one-offs.
