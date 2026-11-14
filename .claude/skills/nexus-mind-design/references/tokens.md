# Tokens

Read the sheet itself when in doubt; these lists are a map, not a copy.

## Admin — `apps/admin/src/styles/signal-theme.css` + `signal.css` (PR #464)

Theme: `.dark` on `<html>` (`@variant dark (&:where(.dark, .dark *))` in `index.css`). Toggle: `src/components/AdminThemeButton.tsx`, stored under `nexusmind-admin-theme`. `signal.css` holds what doesn't change with the theme (accent block, glass classes, motion, phi spacing, type scale); `signal-theme.css` holds the colour roles for `:root` and `:root.dark`.

| Role | Tailwind / CSS | Light / dark | Use |
|---|---|---|---|
| Surfaces | `bg-background`, `bg-card`, `bg-popover`, `bg-muted`, `bg-sidebar`; `var(--workspace)` | white, `#f4f5f7` / charcoal `#101114`, card `#18191c`, workspace `#0b0c0e` | page, cards, overlays, quiet fills |
| Glass | `glass-1`, `glass-2` (`--glass-1-bg`, `--glass-2-bg`, `--glass-edge`, `--glass-shadow`) | — | glass-1: cards, stat cards, panels. glass-2: topbar, floating layers |
| Text | `text-foreground`, `text-muted-foreground`, `text-secondary-foreground` | ink `#111114`, `#60636a` / `#f2f4f7`, `#96989f` | body, labels |
| Action | `bg-primary text-primary-foreground`, hover `var(--primary-hover)` | forest `#356b26` + white / `#7fbe6f` + ink | filled primary action, checked controls |
| Link / emphasis | `var(--brand-link)`, `text-accent-blue` (alias) | `#356b26` / `#91d281` | readable green text |
| Soft fill | `bg-primary-tint text-primary-tint-foreground`; `var(--pale)`, `var(--mint)` | pale `#e7f4d8` / `#293e28` | selected nav, tinted chips |
| Signal accent | `var(--sig)`, `bg-sig` (small marks only), `ring-ring` in dark, `var(--sig-text-light)` | lime `#c3d82e`; `#677302` for light surfaces | focus, live dot, highlight mark, hover edge, gradient ends |
| Accent block | `--sig --sig-hi --sig-2 --sig-3 --on-sig --sig-text-light` | from `rebrand.mjs '#c3d82e'` | gradients (`--grad-sig`), ink on a lime chip (`text-on-sig`) |
| Lines | `border-border`, `border-input`, `ring-ring` | `#e4e5e9` / white 10%; ring `#677302` / lime | 1px borders; focus is a **solid** 3px `ring-ring` (not `/50`) |
| Danger | `bg-destructive`, `text-destructive` | | destructive actions/states |
| Data series | `var(--d1…--d4)`, `bg-d1…`, aliases `--data-green/blue/teal/amber`, `--chart-1…4` | `#356b26 #285f91 #286f68 #805b0d` / `#91d281 #9bc8ed #82cbbf #e6c16d` | chart series, stat accents — fixed order, never cycled |
| Spacing | `p-phi-1…7`, `gap-phi-*` | 5 8 13 21 34 55 89 px | new layout work |
| Type | `text-micro` 11, `text-sm` 14, `text-lead` 18, `text-title` 23, `text-kpi` 29, `text-display` 37 | base 14 × √φ | new work; Geist only |
| Radius | `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14, `rounded-2xl` 16 (stat card) | | |
| Fonts | `font-sans` = Geist Variable (self-hosted), `font-mono` = JetBrains Mono stack | | code and identifiers in mono |

Compatibility aliases in `apps/admin/src/index.css` (`text-text-primary`, `bg-surface-primary`, `text-accent-blue`, `bg-action-primary`, `text-status-success|warning|error|info` …) keep older routes working on the same tokens. `accent-blue/purple/pink` are **not** blue/purple: they resolve to `brand-link` (green). In new code prefer the shadcn roles above; `status-*` is the only alias family with no shadcn equivalent.

Status values: success `#27713f`/`#84e6b0`, warning `#866200`/`#e2c467`, info `#2563eb`/`#93c5fd` (light/dark), error = `--destructive`. Status never borrows the brand green or lime.

Retired (do not reintroduce): the `--lime-50…950` ramp, `--data-lime`, lime `--primary`, `--complementary` as lime.

Motion: see `motion.md`.

## Backoffice — `apps/backoffice/src/index.css` + `src/styles/signal.css` (PR #464)

Dark only (`color-scheme: dark` on `:root`). Fonts: Geist then Inter (Google Fonts import). Values are the admin's dark set.

| Role | Class | Note |
|---|---|---|
| Canvas/surfaces | `bg-background-primary` `#0b0c0e`, `bg-bg-secondary` charcoal `#101114`, `bg-surface-primary` `#18191c`, `bg-surface-secondary` `#222326`; `glass-1`/`glass-2`; `.bo-workspace` (haze + 55px grid) | |
| Text | `text-text-primary/secondary/tertiary/quaternary` | quaternary must stay ≥ 4.5:1 on canvas and both surfaces — `npm test` asserts it |
| Accent | `*-accent-primary` (green `#7fbe6f`, hover `#91d281`), `bg-accent-primary-tint` (`#293e28`) | `accent-blue*` are deprecated aliases of the green accent. `npm test` asserts accent text ≥ 4.5:1 on the surfaces and button ink on the fill |
| Signal | `ring-sig`, `border-sig`, `bg-sig` (marks only), `:focus-visible` outline lime | focus and highlight only |
| Data | `--d1…--d4`, `text-data-blue`, `bg-data-blue-tint` | replaces the old violet accent |
| Status | `text-status-success|warning|error|info` | mint / `#e2c467` / `#f87171` / `#9bc8ed` |
| Borders | `border-border-primary`, `border-border-secondary` | white at 10% / 6% |

## Landing — `apps/landing/src/styles/smartcoder.css`

Variables `--sc-ink, --sc-body, --sc-white, --sc-soft, --sc-panel, --sc-line, --sc-lime, --sc-green, --sc-pale, --sc-hero, --sc-shadow`; dark mode is `:root[data-theme=dark]`, which redefines the same names. Heebo typography and the canonical values are in `apps/landing/DESIGN.md` frontmatter. Use the variables; don't add Tailwind palette colours (the landing ships `tailwind.config.mjs` but its pages use `.sc-*` classes).
