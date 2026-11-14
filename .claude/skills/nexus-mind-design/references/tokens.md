# Tokens

Read the sheet itself when in doubt; these lists are a map, not a copy.

## Admin — `apps/admin/src/styles/audit1-theme.css`

Theme: `.dark` on `<html>` (`@variant dark (&:where(.dark, .dark *))` in `index.css`). Toggle: `src/components/AdminThemeButton.tsx`, stored under `nexusmind-admin-theme`.

| Role | Tailwind / CSS | Use |
|---|---|---|
| Surfaces | `bg-background`, `bg-card`, `bg-popover`, `bg-muted`, `bg-sidebar` | page, cards, overlays, quiet fills |
| Text | `text-foreground`, `text-muted-foreground`, `text-card-foreground` | body, secondary/labels |
| Action | `bg-primary text-primary-foreground` (lime 400 + lime 950 ink) | filled primary action |
| Link / emphasis | `var(--brand-link)` (lime 700 light, lime 400 dark) | readable lime text on neutral |
| Lines | `border-border`, `border-input`, `ring-ring` | 1px borders, focus ring (3px at 50%) |
| Danger | `bg-destructive`, `text-destructive` | destructive actions/states |
| Categorical | `var(--data-lime/blue/teal/amber/rose)`, `var(--chart-1…5)` | chart series, stat accents — fixed order |
| Lime ramp | `var(--lime-50…950)` | only inside token/compat definitions |
| Radius | `rounded-sm` 6, `rounded-md` 8 (controls, badges), `rounded-lg` 10 (dialogs), `rounded-xl` 14 (cards) | |
| Fonts | `font-sans` = Geist Variable (self-hosted), `font-mono` = JetBrains Mono stack | code and identifiers in mono |

Compatibility aliases in `apps/admin/src/index.css` (`text-text-primary`, `bg-surface-primary`, `text-accent-blue`, `bg-action-primary`, `text-status-success|warning|error|info` …) keep older routes working on the same tokens. `accent-blue/purple/pink` are **not** blue/purple: they resolve to `brand-link`. In new code prefer the shadcn roles above; `status-*` is the only alias family with no shadcn equivalent (success/warning/info).

Status values: success `#27713f`/`#84e6b0`, warning `#866200`/`#e2c467`, info `#2563eb`/`#93c5fd` (light/dark), error = `--destructive`.

Motion: `--ease-apple` (`cubic-bezier(0.16,1,0.3,1)`); entrance 220ms, badges 160ms, sidebar 200ms, sheets 500/300ms. `index.css` has the global `prefers-reduced-motion` block; framer-motion components call `useReducedMotion()`.

## Backoffice — `apps/backoffice/src/index.css`

Dark only (the `.dark` block inherits `:root`). Fonts: Geist then Inter (Google Fonts import).

| Role | Class | Note |
|---|---|---|
| Canvas/surfaces | `bg-background-primary`, `bg-surface-primary`, `bg-surface-secondary` | near-black violet |
| Text | `text-text-primary/secondary/tertiary/quaternary` | quaternary must stay ≥ 4.5:1 on canvas and both surfaces — `npm test` asserts it |
| Accent | `*-accent-primary` (amber `#f59e0b`), `bg-accent-primary-tint` | `accent-blue*` are deprecated aliases of amber; prefer `accent-primary` |
| Secondary accent | `text-accent-violet`, `bg-accent-violet-tint` | links/emphasis only |
| Status | `text-status-success|warning|error|info` | |
| Borders | `border-border-primary`, `border-border-secondary` | violet at 12% / 6% |

## Landing — `apps/landing/src/styles/smartcoder.css`

Variables `--sc-ink, --sc-body, --sc-white, --sc-soft, --sc-panel, --sc-line, --sc-lime, --sc-green, --sc-pale, --sc-hero, --sc-shadow`; dark mode is `:root[data-theme=dark]`, which redefines the same names. Heebo typography and the canonical values are in `apps/landing/DESIGN.md` frontmatter. Use the variables; don't add Tailwind palette colours (the landing ships `tailwind.config.mjs` but its pages use `.sc-*` classes).
