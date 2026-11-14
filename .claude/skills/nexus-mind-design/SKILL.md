---
name: nexus-mind-design
description: NexusMind's visual systems as they exist in code — Signal with the SmartCoder palette in the admin and backoffice (shadcn New York, Geist, ink structure, forest-green actions, lime as accent only, glass levels, d1..d4 data series, Signal motion, phi spacing) and the landing's SmartCoder lime — with tokens, components to reuse, charts, motion, copy and the QA bar. Use before creating or restyling any page, card, table, chart, form, modal or empty state in apps/admin, apps/backoffice or apps/landing, and when reviewing a UI diff.
---

# NexusMind design

Design is scoped per app. The records and the token sheets are the authority; this skill is the working summary.

> **Status.** The Signal/SmartCoder tokens described here land with **PR #464** (`feat/signal-smartcoder`). Until it merges, `origin/main` still has the Audit1 Lime admin (`audit1-theme.css`) and the amber backoffice; if you are on a branch without `apps/admin/src/styles/signal.css`, follow the sheets you actually have.

| App | Record | Runtime tokens | Character |
|---|---|---|---|
| admin (org admins, daily ops) | `apps/admin/DESIGN.md`, `apps/admin/PRODUCT.md` | `apps/admin/src/styles/signal.css` (Signal layer) + `signal-theme.css` (light/dark roles) + compat aliases in `apps/admin/src/index.css` | Signal, SmartCoder palette: ink structure, forest-green actions, lime accent, glass, Geist, compact shadcn New York, light and dark |
| backoffice (superadmins only) | root `DESIGN.md` + comments in the token sheet | `apps/backoffice/src/styles/signal.css` (identical copy) + `apps/backoffice/src/index.css` | same Signal system, dark only, using the admin's dark values |
| landing (public marketing) | `apps/landing/DESIGN.md`, `apps/landing/PRODUCT.md` | `apps/landing/src/styles/smartcoder.css` (`--sc-*`) | SmartCoder lime, Heebo, light/dark via `data-theme` |

The root `DESIGN.md` only points at these. Do not restore the September 2026 blue/glass system (PR #271), the black-sidebar/Heebo admin, or retired landing components: the owner replaced them.

## What to read

| Need | Read |
|---|---|
| What Signal is and the palette roles (start here) | `references/principles.md` |
| Exact token and class names per app, and when to use each | `references/tokens.md` |
| Motion: curve, durations, `sig-*` classes, reduced motion | `references/motion.md` |
| Which component to reuse before writing a new one | `references/components.md` |
| The checklist a screen must pass (a11y, motion, charts, copy, layout) | `references/rules.md` |

## Workflow

1. Find the closest existing screen and the shared component (`references/components.md`). Reuse it. A second stat tile or empty state is a regression even if it looks the same.
2. Build with tokens only: Tailwind classes mapped to roles (`bg-primary text-primary-foreground`, `text-muted-foreground`, `glass-1`, `text-status-error`, `var(--d2)`, `p-phi-4`, `text-kpi`). Literal colours belong only in the token sheets listed above.
3. Run the static audit on what you touched (from the repo root):
   `node .claude/skills/nexus-mind-design/scripts/design-audit.mjs <files or dirs>`
   No args = web-app files changed vs `origin/main`. Exit 1 on a MUST. The `design-guard` hook already reports what each edit adds.
4. Look at it: screenshots at 1440 and 390 wide, light and dark (admin theme lives in `localStorage['nexusmind-admin-theme']` and the `.dark` class on `<html>`). Recipe in the `nexus-mind-qa` skill. Go through `references/rules.md` against the images. For a larger change, give the screenshots to the `design-reviewer` agent instead of judging your own work.
5. Put the screenshots in the PR. The admin owner reviews visible admin changes before merge (`apps/admin/PRODUCT.md`), so a UI PR without images is not reviewable.

## Decisions that come up often

- **Lime is the accent, never the base.** Lime (`--sig`, `#c3d82e`) is only for the focus ring (dark), a live dot (`sig-live`), a small highlight mark, the `sig-card` hover edge and gradient ends — about 5% of a screen. **Never a fill for large surfaces, never body text.** `design-audit.mjs` rule D-COL-01 flags both. Lime is 1.6:1 on white, so on light surfaces use `--sig-text-light` (`#677302`) when lime must be seen.
- **Green carries actions.** `bg-primary text-primary-foreground` is forest `#356b26` with white text in light and `#7fbe6f` with ink in dark — always use `primary-foreground`, never a hard `text-white`. Readable green text is `brand-link` / `text-primary-tint-foreground`.
- **Status ≠ category.** Success/warning/error/info are semantic and carry a text label. Categorical series use `--d1..--d4` (green, blue, teal, amber; aliases `--data-green/blue/teal/amber`, `--chart-1…4`) in that fixed order (`apps/admin/src/pages/dashboard/colors.ts`), never cycled and never status colours. `--data-rose`/`--chart-5` survives only for existing indicator dots.
- **Missing data is "—", never 0.** Real zeros stay visible. Never invent a "healthy %" or a metric the API does not return.
- **Charts** use the shadcn `ChartContainer` (`components/ui/shadcn/chart.tsx`) on Recharts 2.15.4, no entry animation, plus an accessible data table for analytical panels.
- **Counts** use `MetricSummary` — the Signal stat card (glass-1, `text-kpi` number, data-hue icon chip, `sig-card` hover, `sig-reveal`). Don't hand-roll a card.
- **Surfaces** are glass-1 (cards on the frame) or glass-2 (topbar, floating layers). Two levels only.
- **Nothing to show** → one `EmptyState` with a next action, not a grid of empty cards.
- **Graphs** (force-graph/WebGL) keep literal palettes in their `*graph*` utils because canvas can't read CSS variables; the audit only warns there.
- **New token?** Colour roles go in `signal-theme.css` in both `:root` and `:root.dark` (plus the `@theme inline` mapping) and, if the backoffice needs it, in `apps/backoffice/src/index.css`. Theme-independent Signal pieces go in `signal.css` and must be copied byte-for-byte to the backoffice (its `npm test` fails on drift). Check contrast for every new text pair (4.5:1) and UI/focus pair (3:1). No one-offs.
