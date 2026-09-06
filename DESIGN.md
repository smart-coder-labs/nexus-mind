---
name: NexusMind
description: A near-black instrument panel where frosted glass panels float over true black and a single Action Blue is the only thing that asks to be touched.
colors:
  action-blue: "#0066cc"
  action-blue-hover: "#0071e3"
  action-blue-active: "#0058a8"
  action-blue-tint: "rgba(0, 102, 204, 0.12)"
  sky-link-blue: "#2997ff"
  memory-purple: "#bf5af2"
  ops-amber: "#f59e0b"
  ops-amber-hover: "#d97706"
  ops-amber-active: "#b45309"
  trace-violet: "#a78bfa"
  instrument-black: "#000000"
  ink: "#1d1d1f"
  marketing-black: "#07080c"
  ops-black: "#060509"
  frosted-panel: "rgba(13, 15, 20, 0.60)"
  frosted-panel-elevated: "rgba(15, 17, 23, 0.94)"
  surface-tile-1: "#272729"
  surface-tile-2: "#2a2a2c"
  hairline: "rgba(255, 255, 255, 0.08)"
  hairline-faint: "rgba(255, 255, 255, 0.04)"
  hairline-bright: "rgba(255, 255, 255, 0.14)"
  text-primary: "#ffffff"
  text-secondary: "#cccccc"
  text-tertiary: "#7a7a7a"
  text-quaternary: "#555555"
  status-success: "#30d158"
  status-warning: "#ffd60a"
  status-error: "#ff453a"
typography:
  page-title:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, Inter, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.3px"
  section-title:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, Inter, sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.2px"
  body:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, Inter, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  label:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, Inter, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.5
    letterSpacing: "normal"
  caption:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, Inter, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  micro:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, Inter, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  metric:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, Inter, sans-serif"
    fontSize: "28px"
    fontWeight: 600
    lineHeight: 1.2
    fontFeature: "tabular-nums"
  mono:
    fontFamily: "JetBrains Mono, SF Mono, Monaco, Cascadia Code, monospace"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
rounded:
  control: "8px"
  field: "11px"
  panel: "18px"
  pill: "9999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  panel: "20px"
  gutter: "24px"
components:
  button-primary:
    backgroundColor: "{colors.action-blue}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.pill}"
    padding: "0 22px"
    height: "44px"
  button-primary-hover:
    backgroundColor: "{colors.action-blue-hover}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.pill}"
  button-primary-active:
    backgroundColor: "{colors.action-blue-active}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.pill}"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.pill}"
    padding: "0 22px"
    height: "44px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-tertiary}"
    rounded: "{rounded.pill}"
    padding: "0 22px"
    height: "44px"
  button-destructive:
    backgroundColor: "transparent"
    textColor: "{colors.status-error}"
    rounded: "{rounded.pill}"
    padding: "0 22px"
    height: "44px"
  input-default:
    backgroundColor: "rgba(255, 255, 255, 0.03)"
    textColor: "{colors.text-primary}"
    typography: "{typography.body}"
    rounded: "{rounded.field}"
    padding: "0 16px"
    height: "36px"
  badge-default:
    backgroundColor: "rgba(255, 255, 255, 0.06)"
    textColor: "{colors.text-secondary}"
    typography: "{typography.micro}"
    rounded: "{rounded.pill}"
    padding: "0 10px"
    height: "24px"
  badge-primary:
    backgroundColor: "{colors.action-blue-tint}"
    textColor: "{colors.action-blue}"
    typography: "{typography.micro}"
    rounded: "{rounded.pill}"
    padding: "0 10px"
    height: "24px"
  card-glass:
    backgroundColor: "{colors.frosted-panel}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.panel}"
    padding: "20px"
  modal:
    backgroundColor: "{colors.frosted-panel-elevated}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.panel}"
    padding: "24px"
  nav-item:
    backgroundColor: "transparent"
    textColor: "{colors.text-secondary}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "8px 12px"
  nav-item-active:
    backgroundColor: "rgba(255, 255, 255, 0.06)"
    textColor: "{colors.text-primary}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "8px 12px"
---

# Design System: NexusMind

## Overview

**Creative North Star: "The Quiet Instrument"**

NexusMind is where an engineering lead goes to see what their team's AI knows and decide what it is allowed to do. The interface has to hold thousands of memories, audit rows, code symbols and agent runs without ever competing with them. So the chassis is near-black and almost silent: true black behind, frosted glass panels floating on it, hairline borders drawn at 7–8% white, and exactly one blue that means "this responds to you." Everything that glows is the team's knowledge. Nothing else is allowed to.

The system descends from Apple's marketing language — the single Action Blue, the pill CTA, the refusal to decorate — but it is no longer that system. It was rebuilt for a data-dense product. Depth is not the flat surface-stepping of the original: panels are genuinely translucent, `backdrop-filter: blur(12px)` over a dark canvas, so a card reads as a pane of smoked glass rather than a lighter rectangle. Density is high and deliberately so; the correction the system makes to its ancestor is that dense must still mean **legible** — a 13px body, not a 12px whisper.

One design language runs at three volumes. `apps/admin` is the product panel: Action Blue on near-black. `apps/backoffice` is the same chassis in Ops Amber, so an operator can never mistake a superadmin screen for a customer one. `apps/landing` is the same dark glass turned outward, with the marketing page's larger type and radial accent washes. Reading the three side by side, only the accent hue and the type scale change.

**Key Characteristics:**
- Near-black canvas; frosted translucent panels; hairline white borders
- Exactly one interactive accent per surface — blue in product, amber in internal ops
- Pill-shaped controls, 18px panels, 11px fields, 8px nav items — four radii, no others
- Color encodes state, never identity
- Dense but legible: 13px body, nothing below 11px
- Motion is short and eased (`cubic-bezier(0.16, 1, 0.3, 1)`), 220ms in, 150ms on state

## Colors

The palette is one accent against a long neutral ladder on black. Names in quotes below are the ones already written into the CSS comments; the rest name surfaces the code had no word for.

### Primary
- **Action Blue** (`#0066cc`): the single interactive color of the product and marketing surfaces. Every primary button, every link, every selected state. On a dark tile it brightens to **Sky Link Blue** (`#2997ff`) because Action Blue disappears against `#272729`. Hover lifts to `#0071e3`, press drops to `#0058a8`, and the 12% tint (`rgba(0, 102, 204, 0.12)`) fills badges and soft highlights.
- **Ops Amber** (`#f59e0b`): the same role, in `apps/backoffice` only. It exists to make an internal-operations screen unmistakable at a glance. Hover `#d97706`, press `#b45309`.

### Secondary
- **Trace Violet** (`#a78bfa`): backoffice links, emphasis and informational status. It is an emphasis color, never body text.
- **Memory Purple** (`#bf5af2`): reserved for memory-type encoding in the admin dashboard. It is a data category color, not a brand accent.

### Neutral
- **Instrument Black** (`#000000`): the admin page ground and the sidebar. **Marketing Black** (`#07080c`) and **Ops Black** (`#060509`) are the same role on landing and backoffice.
- **Ink** (`#1d1d1f`): the admin's main content canvas, one step up from true black.
- **Frosted Panel** (`rgba(13, 15, 20, 0.60)`): the system's real surface. Every card, sidebar and stat tile is this color over `backdrop-filter: blur(12px)`. **Frosted Panel Elevated** (`rgba(15, 17, 23, 0.94)` at `blur(22px)`) is the modal and popover variant.
- **Surface Tile 1 / 2** (`#272729` / `#2a2a2c`): the inherited opaque tile family. Still present, now the fallback for non-blurred contexts.
- **Hairline** (`rgba(255, 255, 255, 0.08)`): the only border in the system. **Hairline Faint** (`0.04`) divides table rows; **Hairline Bright** (`0.14`) is the hover state of an interactive panel.
- **Text ladder on dark**: `#ffffff` primary → `#cccccc` secondary → `#7a7a7a` tertiary → `#555555` quaternary. Landing and backoffice run tinted equivalents of the same four steps.

### Status
- **Success** `#30d158`, **Warning** `#ffd60a`, **Error** `#ff453a`. These carry state and nothing else.

### Named Rules
**The One Signal Rule.** A surface has exactly one interactive accent. Action Blue in the product and on marketing; Ops Amber in the backoffice. A second accent hue on the same screen is a defect, not a variation.

**The State-Not-Identity Rule.** Color on a metric, tile or row encodes *state* — an error count in `status-error`, a healthy run in `status-success`. It never encodes identity. Per-metric decorative tints read as arbitrary and are prohibited.

**The Hairline Rule.** Every border in the system is white at 4%, 8% or 14%. There are no colored borders except a status tint at 20% inside a badge, and no border thicker than 1px except the focus ring.

## Typography

**Display / Body Font:** system-ui, -apple-system, BlinkMacSystemFont, "Inter" — resolving to SF on Apple platforms and Inter elsewhere.
**Backoffice Font:** "Geist", with the same fallback chain. The face change is part of the "different room" signal.
**Mono Font:** "JetBrains Mono", "SF Mono", "Monaco" — code, symbols, identifiers and keys.

**Character:** Neutral, system-native, and deliberately unbranded. The type carries no personality of its own so that the content can. Weights run 400 / 500 / 600; there is no 300 in product UI.

### Hierarchy
- **Page Title** (600, 22px, 1.2, -0.3px): one per page, at the top of the content column.
- **Section Title** (600, 15px, 1.2, -0.2px): card headers and panel titles.
- **Body** (400, 13px, 1.5): table cells, form values, list items, descriptions, nav items. This is the system's voice.
- **Label** (500, 12px): form labels and column headers. Sentence case; uppercase with wide tracking only for column headers and small group labels.
- **Caption** (400, 12px, `text-tertiary`): metadata, timestamps, counts.
- **Micro** (400, 11px): badges and keyboard hints. The floor.
- **Metric** (600, 28px, `tabular-nums`): the number in a stat tile. Its label is Caption.

### Named Rules
**The 13px Floor Rule.** Body is 13px and nothing renders below 11px. 12px is a label and caption size, not the reading voice. A page title set at 12px semibold is a defect.

**The Tabular Numbers Rule.** Any number a reader compares down a column — metrics, counts, durations, costs — is `tabular-nums`. Ragged digits in a dense table are unreadable at 13px.

## Layout

An 8px base grid governs everything. Page gutter is 24px; panel padding 20px; the gap between panels 16px.

The page skeleton is fixed across every product route: a header block (page title, one-line subtitle, primary action right-aligned), an optional filter row, then content. Content is capped at 1280px and centered wherever it contains prose or a form; data tables may run full-width, because truncating a table to a reading measure helps nobody.

The sidebar is a fixed 208px frosted panel against true black, grouped by domain rather than listed flat — Overview, Knowledge, Code, Access, System — with 11px uppercase group labels. With 29 routes, the grouping is what makes the nav scannable rather than a wall.

Density is high by intent. Table rows are at least 40px, hover at 3% white, divided by Hairline Faint, never zebra-striped.

## Elevation & Depth

**This system creates depth with translucency, not shadow.** A panel is a pane of smoked glass: `rgba(13, 15, 20, 0.60)` with `backdrop-filter: blur(12px)` and a 1px hairline. What sits behind it stays faintly visible, and that is the entire depth cue. Stacking is expressed by increasing opacity and blur — a modal is `rgba(15, 17, 23, 0.94)` at `blur(22px)` — not by casting a larger shadow.

There is exactly one exception, and it is a glow rather than a shadow: the primary button carries a soft Action Blue bloom beneath it (`shadow-md shadow-accent-blue/30`). It marks the single most important control on a screen. Nothing else in the product surface is allowed one.

The marketing and backoffice surfaces additionally define a neutral `--shadow-xs` … `--shadow-xl` scale. It is genuine, in use, and documented here as inherited vocabulary — but it belongs to those two surfaces and must not migrate into the product panel.

### Named Rules
**The Glass-Not-Shadow Rule.** Depth comes from `backdrop-filter` and opacity. To raise something, make it more opaque and blur harder — never add a drop shadow to a card, a row, a badge or a piece of text.

**The One Glow Rule.** The Action Blue bloom belongs to the primary button and to nothing else. A glowing card, tile or input is a defect.

## Shapes

Four radii, and no others:

- **8px — Control.** Nav items, small utility buttons, inline chips.
- **11px — Field.** Inputs, selects, textareas, inline alerts.
- **18px — Panel.** Cards, modals, popovers, the sidebar, drawers.
- **9999px — Pill.** Buttons and badges, without exception.

The pairing is the form language: everything a user *reads inside* is an 18px panel; everything a user *presses* is a pill; everything a user *types into* is an 11px field. A rectangular button or a pill-shaped card breaks the grammar.

Borders are always 1px hairline. There is no clipping, no asymmetric corner, and no decorative geometry.

## Components

**Character: refined and contained — the control yields to the data.** Chrome is quiet on purpose. A button, a field and a badge are all built from the same three moves: a hairline border, a translucent fill, and one accent when the element matters.

### Buttons
- **Shape:** fully rounded pill (`9999px`). Heights 32px (sm), 44px (md, default), 52px (lg).
- **Primary:** Action Blue fill, white text, the single Action Blue bloom beneath. Hover `#0071e3`, press `#0058a8`.
- **Secondary:** transparent, `text-secondary`, 1px hairline. Hover fills to 5% white and brightens the border.
- **Ghost:** transparent and borderless, `text-tertiary`, resolving to `text-primary` on hover.
- **Subtle:** 6% white fill, no border — for dense toolbars where a border would add noise.
- **Destructive:** transparent with a `status-error` border at 35% and error-colored text. Always paired with a confirm modal; never a bare click.
- **Focus:** 2px `#0071e3` outline at 2px offset, implemented as CSS `outline`. Offset renders transparently, so it is correct on any surface — unlike a ring, which needs a matching offset color.
- **Press:** `scale(0.95)` spring. This is the system's signature micro-interaction, and it is suppressed under `prefers-reduced-motion`.

### Inputs / Fields
- **Style:** 36px tall (md), `11px` radius, 3% white fill, 1px border at 9% white, 13px text.
- **Label:** 12px medium above the field, at 6px distance. Never a placeholder standing in for a label.
- **Focus:** the same 2px `#0071e3` outline at 2px offset.
- **Error:** border swaps to `status-error`, with a 12px error message below. Never a placeholder-only error.
- **Disabled:** 40% opacity, `not-allowed` cursor.

### Badges
- **Style:** pill, 11px medium, height 24px. Background is a status or accent color at 10–12%, bordered by the same color at 20%.
- **Use:** state and category. A badge is never a button.

### Cards / Containers
- **Corner:** 18px.
- **Background:** Frosted Panel over `backdrop-filter: blur(12px)`.
- **Border:** 1px hairline at 7%.
- **Padding:** 20px (24px on marketing).
- **Hover (when interactive):** `translateY(-2px)` and the border brightens to 14%. Never a shadow.

### Navigation
- **Sidebar:** 208px frosted panel on true black, 18px radius, grouped by domain with 11px uppercase labels.
- **Items:** 13px, 8px radius, 8×12px padding. Active fills to 6% white with `text-primary`; hover sits between.
- **Focus:** visible 2px outline, same as every other control.

### Stat Tiles
One neutral treatment, always: 4% white fill, 1px hairline, 18px radius, a 28px semibold `tabular-nums` number, and a 12px `text-tertiary` label. A tile may carry a small blurred accent glow in its corner (`.tile-glow`, 7rem, 18% opacity, 30px blur) only when the tint encodes state.

### Empty & Loading States
- **Empty:** the `EmptyState` component — icon, a one-line "what this is", one primary action. Small inline sub-tiles (sparklines, mini-charts) may instead use a centered 13px `text-tertiary` caption, because `EmptyState`'s footprint is too heavy there. Never a bare unstyled "No data".
- **Loading:** `Skeleton` shapes that mirror the final layout. Spinners live inside buttons and nowhere else.
- **Errors:** an inline alert — 11px radius, `status-error` at 8%, icon plus message plus a retry action. Never a toast alone for a page-level fetch failure.

## Do's and Don'ts

### Do:
- **Do** build every surface from the three moves: hairline border, translucent fill, one accent where it matters.
- **Do** reach for `backdrop-filter` and opacity when you need depth — `rgba(13, 15, 20, 0.60)` at `blur(12px)` for a panel, `rgba(15, 17, 23, 0.94)` at `blur(22px)` for a modal.
- **Do** set body text at 13px and stop at 11px. Page titles are 22px/600.
- **Do** route every color through a token (`bg-accent-blue`, `text-text-secondary`). Inline hexes in TSX are debt.
- **Do** give every interactive element a visible `:focus-visible` outline — 2px `#0071e3` at 2px offset, as CSS `outline`, not a Tailwind ring.
- **Do** use `tabular-nums` on any number a reader compares down a column.
- **Do** pick from the four radii — 8px control, 11px field, 18px panel, 9999px pill.
- **Do** swap the accent to Ops Amber, and only the accent, when the surface is `apps/backoffice`.

### Don't:
- **Don't** add a drop shadow to a card, row, badge or text. The one bloom in the product surface belongs to the primary button.
- **Don't** introduce a second interactive accent on a surface. Status colors are state, not decoration.
- **Don't** tint a metric tile to encode identity. Color on a metric means state or it means nothing.
- **Don't** build a generic multicolor SaaS dashboard — a palette per metric, rainbow charts, decorative per-card hues. This is the system's confirmed anti-reference.
- **Don't** set a page title, nav item or table cell at 12px. That is the label size.
- **Don't** write `focus-visible:outline-none` without an immediate replacement.
- **Don't** use `text-quaternary` (`#555555`) as the only label of an action or a piece of information — it fails contrast at body size on `#1d1d1f`. Decorative and disabled only.
- **Don't** invent a fifth radius, a colored border, or a border thicker than 1px.
