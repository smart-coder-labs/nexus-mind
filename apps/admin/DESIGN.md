---
name: "NexusMind Admin — Signal (SmartCoder)"
description: "Signal design language on shadcn New York roles, with the SmartCoder palette: ink structure, forest-green actions, lime as accent only."
colors:
  ink: "#111114"
  charcoal: "#101114"
  primary-light: "#356b26"
  primary-foreground-light: "#ffffff"
  primary-hover-light: "#225811"
  primary-dark: "#7fbe6f"
  primary-foreground-dark: "#111114"
  primary-hover-dark: "#91d281"
  sig: "#c3d82e"
  sig-hi: "#d6e255"
  sig-2: "#67bd4b"
  sig-3: "#02ae63"
  on-sig: "#151800"
  sig-text-light: "#677302"
  pale: "#e7f4d8"
  mint: "#84e6b0"
  primary-tint-dark: "#293e28"
  brand-link-light: "#356b26"
  brand-link-dark: "#91d281"
  ring-light: "#677302"
  ring-dark: "#c3d82e"
  d1-light: "#356b26"
  d1-dark: "#91d281"
  d2-light: "#285f91"
  d2-dark: "#9bc8ed"
  d3-light: "#286f68"
  d3-dark: "#82cbbf"
  d4-light: "#805b0d"
  d4-dark: "#e6c16d"
  data-rose-light: "#984c5c"
  data-rose-dark: "#e0a0ad"
  background-light: "#ffffff"
  workspace-light: "#f4f5f7"
  card-light: "#ffffff"
  muted-light: "#f4f5f7"
  muted-foreground-light: "#60636a"
  secondary-foreground-light: "#26272c"
  border-light: "#e4e5e9"
  sidebar-light: "#fafafb"
  background-dark: "#101114"
  foreground-dark: "#f2f4f7"
  workspace-dark: "#0b0c0e"
  card-dark: "#18191c"
  muted-dark: "#222326"
  muted-foreground-dark: "#96989f"
  sidebar-dark: "#141518"
  border-dark: "oklch(1 0 0 / 10%)"
  input-dark: "oklch(1 0 0 / 15%)"
  success-light: "#27713f"
  success-dark: "#84e6b0"
  warning-light: "#866200"
  warning-dark: "#e2c467"
  destructive-light: "oklch(.577 .245 27.325)"
  destructive-dark: "oklch(.704 .191 22.216)"
  info-light: "#2563eb"
  info-dark: "#93c5fd"
typography:
  display:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "37px"
    fontWeight: 600
    lineHeight: 1.08
  kpi:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "29px"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "-0.03em"
  title:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "23px"
    fontWeight: 600
    lineHeight: 1.2
  lead:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "18px"
    fontWeight: 400
    lineHeight: 1.35
  body:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.47
  control:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "14px"
    fontWeight: 500
    lineHeight: 1.428571
  micro:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  sm: "6px"
  control: "8px"
  base: "10px"
  card: "14px"
  stat: "16px"
spacing:
  phi-1: "5px"
  phi-2: "8px"
  phi-3: "13px"
  phi-4: "21px"
  phi-5: "34px"
  phi-6: "55px"
  phi-7: "89px"
motion:
  ease-signal: "cubic-bezier(0.16, 1, 0.3, 1)"
  ease-signal-std: "cubic-bezier(0.2, 0.7, 0.2, 1)"
  sig-reveal: "1.1s ease-signal, 90ms sibling stagger, max 5"
  sig-bar: "0.8s ease-signal-std on width/height"
  sig-live: "1.6s ping, only while data streams"
components:
  button-primary:
    backgroundColor: "{colors.primary-light}"
    textColor: "{colors.primary-foreground-light}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  stat-card:
    backgroundColor: "glass-1"
    rounded: "{rounded.stat}"
    padding: "{spacing.phi-4}"
    typography: "{typography.kpi}"
  card:
    backgroundColor: "glass-1"
    rounded: "{rounded.card}"
    padding: "24px"
  navigation-active:
    backgroundColor: "{colors.pale}"
    textColor: "{colors.primary-light}"
    rounded: "{rounded.control}"
    height: "32px"
---

# Design System: NexusMind Admin — Signal (SmartCoder)

## Overview

**Creative North Star: "Signal over ink"**

The admin speaks the Signal design language (glass levels, a fixed data palette, expo-out motion, Fibonacci spacing and a golden type scale) on top of the shadcn New York roles it already used. The palette is SmartCoder's: ink and charcoal neutrals carry the structure, forest green carries primary actions, and lime is the **signal accent only**. Geist stays.

Runtime authority: `src/styles/signal.css` (theme-independent Signal layer, byte-identical in `apps/backoffice`), `src/styles/signal-theme.css` (light/dark colour roles) and the compatibility aliases and shell treatments in `src/index.css`. This is a re-skin through tokens and shared components; routes, permissions, data and contracts are unchanged.

### Palette roles

| Role | Light | Dark | Use |
|---|---|---|---|
| Structure | ink `#111114`, charcoal `#101114` | foreground `#f2f4f7` on charcoal | text, frame, neutrals |
| Primary action | forest `#356b26`, white text | `#7fbe6f`, ink text | filled primary buttons, checked controls |
| Signal accent | lime `#c3d82e` | same | focus ring (dark), live dots, current-page mark, hover edge, gradient ends. About 5% of a screen |
| Soft fills | pale `#e7f4d8`, mint `#84e6b0` | tint `#293e28` | selected nav, tinted chips |
| Data d1..d4 | `#356b26` `#285f91` `#286f68` `#805b0d` | `#91d281` `#9bc8ed` `#82cbbf` `#e6c16d` | chart and stat series, fixed order |

Dark values are OKLCH derivations: same hue as the light value, lightness raised into the 0.74–0.82 band the existing dark data set already used. The lime accent block (`--sig`, `--sig-hi`, `--sig-2`, `--sig-3`, `--on-sig`, `--sig-text-light`) is generated by the Signal kit's `rebrand.mjs '#c3d82e'`.

**The Lime Rule.** Lime is never a fill for large surfaces and never body text. It measures 1.6:1 on white, so on light surfaces any lime that must be read or seen as a ring uses `--sig-text-light` (`#677302`, 5.2:1). On dark surfaces lime is used as is (11:1+).

## Typography

**Interface Font:** Self-hosted Geist Variable, loaded through @fontsource-variable/geist; sans-serif fallback.
**Code Font:** JetBrains Mono, SF Mono, Monaco, Cascadia Code, monospace.

The frontmatter captures the implemented dashboard headline, base body, card title, control, table data, small label, and metric roles. Primary counts use compact 12px semibold badges with tabular numerals, alongside 12px labels; counts no longer compete with page headings. Modal titles use an 18px semibold treatment with a tight line height. Fields use 16px text by default and 14px from the medium breakpoint, with 16px retained for coarse pointers. Domain code and graph labels may retain their context-specific compact typography; those exceptions do not define the shared hierarchy.

**The Geist Interface Rule.** Use the self-hosted Geist variable family for interface text. Keep the existing monospace stack for code and identifiers.

**Golden scale.** New work uses the Tailwind utilities `text-micro` (11), `text-sm` (14, body), `text-lead` (18), `text-title` (23), `text-kpi` (29) and `text-display` (37): base 14px, each step ×√φ. Spacing uses `phi-1…phi-7` (5, 8, 13, 21, 34, 55, 89px). Existing route geometry is not rewritten.

## Layout

The desktop shell uses a 288px expanded sidebar. Its icon-width token is 48px; the inset variant adds 16px to the reserved rail space and 18px to the fixed container width, so the entire collapsed shell must not be measured as a bare 48px rail. Desktop sidebar treatment begins at 768px. Below that, the sidebar is a 288px left sheet. The inset workspace has 8px desktop margins, a rounded outline, and shallow shadow; the Layout adds a 10px bottom margin.

The sidebar navigation keeps native scrolling. Its scrollbar is hidden in the collapsed icon rail; expanded and mobile navigation use a thin, low-contrast scrollbar that becomes clearer on hover or keyboard focus.

The topbar has a 56px minimum height and 16px horizontal padding. It may wrap on narrow widths. Main content owns vertical scrolling, with a 20px gutter reduced to 16px at widths up to 640px. Route content remains fluid and retains its responsive forms, grids, and tables.

Primary page statistics use the Signal stat card (`MetricSummary`): glass-1, 16px radius, phi-3/phi-4 padding, a 13px muted label, the value in `text-title` → `text-kpi` with tabular numerals, an icon chip tinted with the card's data hue, the lime hover edge (`sig-card`) and the `sig-reveal` entrance. Cards stay static: label, value and supporting caption stay visible, with no chevron, disclosure or hidden detail. Real recent series, when supplied, occupy a compact 96×32px area alongside the caption. At widths up to 480px, summaries occupy full rows to preserve long labels and values. Legacy grids remain only for screens not yet migrated. Tables keep overflow within their own container.

## Elevation & Depth

Glass has exactly two levels. **glass-1** is a card on the frame: a 76% card veil (dark: 5% white over 70% card), a hairline edge, a soft shadow and a 22px blur. **glass-2** is the frame itself (topbar) and floating layers: a 55–62% workspace veil and a 40px blur. Shared shadcn cards, the legacy `Card`, `.lima-panel`, KPI tracks and stat cards all use glass-1; the legacy `.glass` / `.glass-dark` utilities map to the two levels. What the glass refracts is the workspace: one faint green haze, one faint cool haze and a masked 55px grid. Dialogs and sheets keep stronger elevation above a half-black scrim. `.sig-hud` is the always-dark immersive surface for a single hero widget.

**The Quiet Depth Rule.** Two glass levels and overlays; no third level, no lime haze.

## Shapes

The radius root is 10px. Small elements use 6px, controls/navigation/badges 8px, dialogs 10px, and cards/inset surfaces 14px. Badges are softly rounded rectangles; circular status dots and avatars retain their semantic shapes. Fields and cards use thin borders aligned with their theme roles.

## Components

### Buttons

Compact shadcn New York buttons use cva variants and optional Radix Slot composition. Default actions use the forest-green primary with white text (dark: `#7fbe6f` with ink), 36px height, and 16px horizontal padding; a direct SVG child reduces horizontal padding to 12px. Small, extra-small, large, and icon sizes are 32px, 20px, 40px, and 36px square respectively. Secondary, outline, ghost, link, and destructive variants retain distinct roles. Default hover reduces fill opacity; link hover underlines. Focus uses a solid 3px ring in the ring colour (`#677302` light, lime dark), which clears 3:1 on every surface. Disabled/loading states disable interaction. Legacy Button maps its existing API onto these variants.

### Inputs / Fields

Fields share the 8px radius, thin input border, 36px standard height, and 12px horizontal padding. Small and large sizes are 32px and 40px. Light fields are transparent; dark fields use a 30% input tint. Focus pairs a ring-colored border with the 3px half-opacity ring. Invalid fields expose destructive border/ring states; disabled fields reduce opacity. Labels, helper text, and error text remain attached to existing form semantics.

### Navigation

The neutral sidebar groups permission-filtered routes. Rows use 32px height, 14px text, and SVG icons. Current-page links use the pale soft fill (dark: `#293e28`), green text/icons, a 35% primary border, medium weight and a 2px lime signal mark on the left edge (hidden in icon mode; decorative, the fill and weight carry the state). Icon mode shows tooltips. Group labels are sentence case at 12px/500. The sidebar trigger controls the desktop rail or mobile sheet.

### Chips / Badges

Custom badges use 12px medium text, 8px radius, and compact padding. Semantic variants use a 10% status tint and 20% border. Neutral badges use a foreground tint and neutral border. Existing optional dots remain; the purple compatibility variant uses the brand-link alias.

### Cards / Containers

Shared cards use card/foreground roles, a thin border, 14px corners, and shallow elevation. Standard content padding is 24px; compact compatibility cards use 16px. Native shadcn cards provide header/content/footer slots, 24px vertical padding and gaps, and 24px horizontal section padding. Compatibility card variants retain their API while sharing this surface grammar.

### Tables and statistics

Tables use 14px copy, a 40px header with sentence-case medium labels, 8px horizontal header padding, and 8px body-cell padding. Compact rows reduce vertical padding to 6px. Rows separate with borders and gain a half-muted hover fill. Summary counts use the compact metric role with small neutral inline icons. Secondary information stays visible below the label and value; zero counts remain visible and missing values stay “—”.

Compact summaries share a responsive grid measured against the available content width, with equal-height cells and 8px gaps. Six summaries use three columns from 640px of content width and six from 1200px; narrower layouts fit columns of at least 160px. This avoids isolated dashboard counts and excessive vertical stacking on phones. Statistic icons and value badges can use a restrained categorical tint while keeping copy legible. Disabled accounts retain readable table text and actions; their status badge carries the state without reducing opacity across the entire row.

### Charts

Analytical charts use Audit1 UI Commons’ shadcn ChartContainer, ChartTooltipContent and ChartLegendContent on Recharts 2.15.4, matching the consumed reference version. Use theme variables instead of fixed colors: `--chart-1..4` are `--d1..--d4` (green, blue, teal, amber) in a fixed order that never cycles; `--chart-5` (rose) stays only for existing categorical indicators. A bar whose length is the datum may use `.sig-bar` (green into the lime end). Categorical bars within a single ranked list share one hue rather than assigning status colors to unrelated categories.

Dashboard memory trends, type/project breakdowns, health indicators, usage trends, ranked usage bars and compact metric series share this chart system. Axes use muted labels and quiet gridlines, with shadcn tooltips and keyboard chart navigation. Include an accessible data table for analytical panels. Plot actual zeros without a decorative minimum bar height. Disable decorative chart entry animation. Dashboard charts load behind individual skeleton fallbacks; count sparklines load independently in a compact, always-visible space, so the chart dependency does not block summary navigation. Missing data is unavailable, never invented as zero.

Health categories can overlap; show duplicates, stale and untagged counts independently, without deriving an unsupported “healthy percentage.” The task status strip, calendar heatmap and interactive relationship graphs retain their specialized representations for now; they are not certified as shadcn chart migrations.

### Overlays and selection controls

Select, switch, checkbox, popover, tooltip, sidebar sheet, and shared modal use Radix primitives or the shadcn wrappers present in the source. Centered modals keep viewport margins, 24px padding, focus restoration, keyboard dismissal, and accessible names. Domain-specific controls can remain native or custom. Component previews in the sidecar illustrate appearance and are not replacements for these runtime behaviors.

The invitation form retains its existing workflow and native role selector. Its overlay keeps 16px viewport margins, caps the panel to the dynamic viewport height with internal scrolling, and uses readable 12px labels with 36px fields (16px input text on mobile, 14px on desktop).

### Theme and motion

The persistent topbar theme control is 36px high and follows the outlined control grammar. Theme choice remains stored under nexusmind-admin-theme. Signal motion uses one curve, `--ease-signal` (expo-out, the same curve as the legacy `--ease-apple`): `sig-reveal` (1.1s, 90ms stagger capped at five), `sig-bar` (0.8s data easing), `sig-live` (1.6s ping, only while a run is actually streaming; first use: the Factory step console's "Seguir en vivo") and `sig-card` (0.2s border). Existing utilities keep 220ms entrances, sidebar 200ms, sheets 500/300ms. Under `prefers-reduced-motion: reduce` the Signal animations and transitions are removed and the global guard minimizes the rest; participating Framer Motion components read the same preference. This does not imply that every third-party animation has been visually validated.

## Do's and Don'ts

### Do:
- Do use primary with primary-foreground for filled green actions.
- Do keep lime to accent roles: focus, live data, current-page mark, hover edge, gradient ends.
- Do preserve separate success, warning, error, and information colors with text labels.
- Do extend shared shadcn primitives and compatibility wrappers before adding route-specific visual rules.
- Do preserve keyboard focus, labels, dialog semantics, permission filtering, and reduced-motion support.
- Do preserve real chart data and existing workflow semantics.

### Don't:
- Don't restore the obsolete Heebo, black-sidebar, or landing-derived admin system.
- Don't fill large surfaces with lime or set body text in lime; on light surfaces use `--sig-text-light` when lime must be legible.
- Don't add a second brand colour, cycle d1..d4, or reuse a data hue as a status.
- Don't assume every domain control is Radix or promote an isolated legacy measurement into a global rule.
