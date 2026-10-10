---
name: "NexusMind Admin — Audit1 Lime"
description: "Audit1 Admin’s shadcn New York system adapted to lime for NexusMind operational workflows."
colors:
  primary: "#c3d82e"
  primary-foreground: "#1d2409"
  action-hover: "#cde45a"
  action-active: "#a6bc1b"
  brand-link-light: "#5e6c16"
  brand-link-dark: "#c3d82e"
  data-blue-light: "#285f91"
  data-blue-dark: "#9bc8ed"
  data-teal-light: "#286f68"
  data-teal-dark: "#82cbbf"
  data-amber-light: "#805b0d"
  data-amber-dark: "#e6c16d"
  data-rose-light: "#984c5c"
  data-rose-dark: "#e0a0ad"
  background-light: "oklch(1 0 0)"
  foreground-light: "oklch(.145 0 0)"
  card-light: "oklch(1 0 0)"
  muted-light: "oklch(.97 0 0)"
  muted-foreground-light: "oklch(.556 0 0)"
  secondary-foreground-light: "oklch(.205 0 0)"
  border-light: "oklch(.922 0 0)"
  sidebar-light: "oklch(.985 0 0)"
  topbar-light: "oklch(.985 .002 247.839)"
  background-dark: "oklch(.145 0 0)"
  foreground-dark: "oklch(.985 0 0)"
  card-dark: "oklch(.205 0 0)"
  muted-dark: "oklch(.269 0 0)"
  muted-foreground-dark: "oklch(.708 0 0)"
  border-dark: "oklch(1 0 0 / 10%)"
  input-dark: "oklch(1 0 0 / 15%)"
  topbar-dark: "#252829"
  workspace-dark: "#1c1c1c"
  success-light: "#27713f"
  success-dark: "#84e6b0"
  warning-light: "#866200"
  warning-dark: "#e2c467"
  destructive-light: "oklch(.577 .245 27.325)"
  destructive-dark: "oklch(.704 .191 22.216)"
  info-light: "#2563eb"
  info-dark: "#93c5fd"
typography:
  headline:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.3px"
  body:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.47
    letterSpacing: "normal"
  card-title:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1
  control:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "14px"
    fontWeight: 500
    lineHeight: 1.428571
  data:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.333333
  metric:
    fontFamily: "Geist Variable, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.25
rounded:
  sm: "6px"
  control: "8px"
  base: "10px"
  card: "14px"
spacing:
  xs: "4px"
  sm: "8px"
  control: "12px"
  md: "16px"
  page: "20px"
  lg: "24px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.primary-foreground}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  button-outline:
    backgroundColor: "{colors.background-light}"
    textColor: "{colors.foreground-light}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  button-secondary:
    backgroundColor: "{colors.muted-light}"
    textColor: "{colors.secondary-foreground-light}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  button-ghost:
    textColor: "{colors.foreground-light}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  button-link:
    textColor: "{colors.brand-link-light}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  button-destructive:
    backgroundColor: "{colors.destructive-light}"
    textColor: "#ffffff"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  input:
    textColor: "{colors.foreground-light}"
    rounded: "{rounded.control}"
    padding: "4px 12px"
    height: "36px"
  navigation-active:
    textColor: "{colors.brand-link-light}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "8px 12px"
    height: "32px"
  badge:
    rounded: "{rounded.control}"
    padding: "2px 8px"
  card:
    backgroundColor: "{colors.card-light}"
    textColor: "{colors.foreground-light}"
    rounded: "{rounded.card}"
    padding: "24px"
---

# Design System: NexusMind Admin — Audit1 Lime

## Overview

**Creative North Star: "Audit1 operational clarity"**

NexusMind admin adopts the owner-selected Audit1 Admin visual system: Geist typography, neutral light and dark surfaces, an inset workspace, compact shadcn New York controls, and restrained lime actions. Audit1’s purple brand roles are translated to lime while its neutral material and component geometry remain the reference.

The source authority is Admin-portal-front e9e01b2a and its consumed UI Commons v2.0.670 (47968e54). Local implementation tokens live in src/styles/audit1-theme.css, with compatibility aliases and shell treatments in src/index.css. This system applies to the admin; the landing retains its separately approved identity.

**Key Characteristics:**
- Neutral inset shell with a collapsible sidebar and mobile sheet.
- Self-hosted Geist Variable and compact, readable operational hierarchy.
- Lime actions with dark ink; independently meaningful semantic statuses.
- Thin borders, modest shadows, and subtle workspace grid and radial washes.

## Colors

### Primary

Action Lime supplies primary and complementary filled actions. Deep Lime Ink supplies their foreground. Olive Link is the light-theme text and focus accent; dark surfaces use Action Lime for those roles. The full source lime ramp (50–950) is recorded in the sidecar. The frontmatter resolves semantic aliases to their source CSS color values; CSS remains the runtime theme authority.

The primary shadcn button hovers at 90% complementary opacity. Compatibility action-hover and action-active tokens remain available to older controls, so they are not a replacement for that button’s actual state treatment.

### Neutral

Light mode uses white backgrounds/cards, pale neutral sidebar and topbar surfaces, dark text, and gray borders. Dark mode uses a charcoal background, slightly lifted cards/sidebar, a distinct charcoal topbar, and the workspace-dark canvas. Muted surfaces double as secondary and accent surfaces. Popovers use the card tone in each theme. Sidebar text inherits foreground; navigation is theme-aware.

### Semantic status

Success, warning, destructive, and information colors remain distinct. Compatibility names such as accent-blue, accent-purple, and accent-pink resolve to the brand-link role; information status remains blue. Preserve label meaning alongside color.

### Categorical accents

Lime remains the brand action color. Compact statistics and multi-series charts may use blue, teal, amber, and rose as supporting categorical hues. Keep their tints low on card surfaces, map them in a stable order across routes, and never reuse them to imply success, warning, or error. Each foreground has a theme-specific readable value.

**The Action Ink Rule.** Pair solid lime actions with primary foreground. Use the theme-aware brand-link role for readable text emphasis on neutral surfaces.

**The Semantic Pair Rule.** Choose surface, foreground, border, and focus roles together; theme changes must preserve their relationship.

## Typography

**Interface Font:** Self-hosted Geist Variable, loaded through @fontsource-variable/geist; sans-serif fallback.
**Code Font:** JetBrains Mono, SF Mono, Monaco, Cascadia Code, monospace.

The frontmatter captures the implemented dashboard headline, base body, card title, control, table data, small label, and metric roles. Primary counts use compact 12px semibold badges with tabular numerals, alongside 12px labels; counts no longer compete with page headings. Modal titles use an 18px semibold treatment with a tight line height. Fields use 16px text by default and 14px from the medium breakpoint, with 16px retained for coarse pointers. Domain code and graph labels may retain their context-specific compact typography; those exceptions do not define the shared hierarchy.

**The Geist Interface Rule.** Use the self-hosted Geist variable family for interface text. Keep the existing monospace stack for code and identifiers.

## Layout

The desktop shell uses a 288px expanded sidebar. Its icon-width token is 48px; the inset variant adds 16px to the reserved rail space and 18px to the fixed container width, so the entire collapsed shell must not be measured as a bare 48px rail. Desktop sidebar treatment begins at 768px. Below that, the sidebar is a 288px left sheet. The inset workspace has 8px desktop margins, a rounded outline, and shallow shadow; the Layout adds a 10px bottom margin.

The sidebar navigation keeps native scrolling. Its scrollbar is hidden in the collapsed icon rail; expanded and mobile navigation use a thin, low-contrast scrollbar that becomes clearer on hover or keyboard focus.

The topbar has a 56px minimum height and 16px horizontal padding. It may wrap on narrow widths. Main content owns vertical scrolling, with a 20px gutter reduced to 16px at widths up to 640px. Route content remains fluid and retains its responsive forms, grids, and tables.

Primary page statistics use a wrapping strip with 8px gaps. Each neutral summary is at least 36px high, with 12px horizontal and 8px vertical padding, an 8px radius and a small value badge. Summaries are static: label, value and supporting caption stay visible, with no chevron, disclosure or hidden detail. Real recent series, when supplied, occupy a compact 96×32px area alongside the caption. At widths up to 480px, summaries occupy full rows to preserve long labels and values. Legacy grids remain only for screens not yet migrated. Tables keep overflow within their own container.

## Elevation & Depth

Borders and shallow shadows separate neutral panels. Cards use small ambient shadows; compact metric summaries use borders without shadows; controls use a smaller shadow. Dialogs and sheets use stronger elevation above a half-black scrim. The workspace has low-opacity lime radial washes and a masked 64px grid. These are part of the Audit1 reference material, rather than a prohibition on gradients or grids. Compatibility glass utilities still exist, but shared card variants now resolve to the same ordinary card surface.

**The Quiet Depth Rule.** Use borders and shallow shadows for ordinary panels; stronger elevation and scrims belong to overlays.

## Shapes

The radius root is 10px. Small elements use 6px, controls/navigation/badges 8px, dialogs 10px, and cards/inset surfaces 14px. Badges are softly rounded rectangles; circular status dots and avatars retain their semantic shapes. Fields and cards use thin borders aligned with their theme roles.

## Components

### Buttons

Compact shadcn New York buttons use cva variants and optional Radix Slot composition. Default actions use complementary lime with dark ink, 36px height, and 16px horizontal padding; a direct SVG child reduces horizontal padding to 12px. Small, extra-small, large, and icon sizes are 32px, 20px, 40px, and 36px square respectively. Secondary, outline, ghost, link, and destructive variants retain distinct roles. Default hover reduces fill opacity; link hover underlines. Focus uses the ring color at 50% with a 3px ring. Disabled/loading states disable interaction. Legacy Button maps its existing API onto these variants.

### Inputs / Fields

Fields share the 8px radius, thin input border, 36px standard height, and 12px horizontal padding. Small and large sizes are 32px and 40px. Light fields are transparent; dark fields use a 30% input tint. Focus pairs a ring-colored border with the 3px half-opacity ring. Invalid fields expose destructive border/ring states; disabled fields reduce opacity. Labels, helper text, and error text remain attached to existing form semantics.

### Navigation

The neutral sidebar groups permission-filtered routes. Rows use 32px height, 14px text, and SVG icons. Current-page links use a 5% primary tint, 35% primary border, brand-link text/icons, medium weight, and a subtle 1.05 scale in expanded mode. Icon mode removes that scale and shows tooltips. Group labels are sentence case at 12px/500. The sidebar trigger controls the desktop rail or mobile sheet.

### Chips / Badges

Custom badges use 12px medium text, 8px radius, and compact padding. Semantic variants use a 10% status tint and 20% border. Neutral badges use a foreground tint and neutral border. Existing optional dots remain; the purple compatibility variant uses the brand-link alias.

### Cards / Containers

Shared cards use card/foreground roles, a thin border, 14px corners, and shallow elevation. Standard content padding is 24px; compact compatibility cards use 16px. Native shadcn cards provide header/content/footer slots, 24px vertical padding and gaps, and 24px horizontal section padding. Compatibility card variants retain their API while sharing this surface grammar.

### Tables and statistics

Tables use 14px copy, a 40px header with sentence-case medium labels, 8px horizontal header padding, and 8px body-cell padding. Compact rows reduce vertical padding to 6px. Rows separate with borders and gain a half-muted hover fill. Summary counts use the compact metric role with small neutral inline icons. Secondary information stays visible below the label and value; zero counts remain visible and missing values stay “—”.

Compact summaries share a responsive grid measured against the available content width, with equal-height cells and 8px gaps. Six summaries use three columns from 640px of content width and six from 1200px; narrower layouts fit columns of at least 160px. This avoids isolated dashboard counts and excessive vertical stacking on phones. Statistic icons and value badges can use a restrained categorical tint while keeping copy legible. Disabled accounts retain readable table text and actions; their status badge carries the state without reducing opacity across the entire row.

### Charts

Analytical charts use Audit1 UI Commons’ shadcn ChartContainer, ChartTooltipContent and ChartLegendContent on Recharts 2.15.4, matching the consumed reference version. Use theme variables instead of fixed dark-surface colors: chart-1 is lime in both themes, followed by blue, teal, amber, and rose for distinct series. Categorical bars within a single ranked list share one hue rather than assigning status colors to unrelated categories.

Dashboard memory trends, type/project breakdowns, health indicators, usage trends, ranked usage bars and compact metric series share this chart system. Axes use muted labels and quiet gridlines, with shadcn tooltips and keyboard chart navigation. Include an accessible data table for analytical panels. Plot actual zeros without a decorative minimum bar height. Disable decorative chart entry animation. Dashboard charts load behind individual skeleton fallbacks; count sparklines load independently in a compact, always-visible space, so the chart dependency does not block summary navigation. Missing data is unavailable, never invented as zero.

Health categories can overlap; show duplicates, stale and untagged counts independently, without deriving an unsupported “healthy percentage.” The task status strip, calendar heatmap and interactive relationship graphs retain their specialized representations for now; they are not certified as shadcn chart migrations.

### Overlays and selection controls

Select, switch, checkbox, popover, tooltip, sidebar sheet, and shared modal use Radix primitives or the shadcn wrappers present in the source. Centered modals keep viewport margins, 24px padding, focus restoration, keyboard dismissal, and accessible names. Domain-specific controls can remain native or custom. Component previews in the sidecar illustrate appearance and are not replacements for these runtime behaviors.

The invitation form retains its existing workflow and native role selector. Its overlay keeps 16px viewport margins, caps the panel to the dynamic viewport height with internal scrolling, and uses readable 12px labels with 36px fields (16px input text on mobile, 14px on desktop).

### Theme and motion

The persistent topbar theme control is 36px high and follows the outlined control grammar. Theme choice remains stored under nexusmind-admin-theme. Sidebar geometry transitions over 200ms; existing entrance utilities use 220ms easing, badges 160ms, and sheets 500ms opening/300ms closing. The reduced-motion media query minimizes CSS animation and transition durations, and participating Framer Motion components read the same preference. This does not imply that every third-party animation has been visually validated.

## Do's and Don'ts

### Do:
- Do use primary with primary-foreground for filled lime actions.
- Do preserve separate success, warning, error, and information colors with text labels.
- Do extend shared shadcn primitives and compatibility wrappers before adding route-specific visual rules.
- Do preserve keyboard focus, labels, dialog semantics, permission filtering, and reduced-motion support.
- Do preserve real chart data and existing workflow semantics.

### Don't:
- Don't restore the obsolete Heebo, black-sidebar, or landing-derived admin system.
- Don't use white text or icons on the lime primary fill.
- Don't turn every status, panel, or icon into a lime accent.
- Don't assume every domain control is Radix or promote an isolated legacy measurement into a global rule.
