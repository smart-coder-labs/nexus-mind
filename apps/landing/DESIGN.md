---
name: "NexusMind · SmartCoder lime"
description: "Canonical landing system extracted from the owner-selected SmartCoder/Globant lime implementation."
colors:
  ink: "#111114"
  body: "#454547"
  white: "#fff"
  soft: "#f5f6f8"
  panel: "#fff"
  line: "#d9dcda"
  lime: "#c3d82e"
  green: "#356b26"
  pale: "#e7f4d8"
  charcoal: "#101114"
  cta-start: "#d2e322"
  cta-end: "#8ac83e"
  cta-text: "#182004"
  mint: "#84e6b0"
  dark-ink: "#f1f4e9"
  dark-body: "#c3c9bc"
  dark-white: "#101710"
  dark-soft: "#182019"
  dark-panel: "#242e23"
  dark-line: "#414c3e"
  dark-green: "#c3d82e"
  dark-pale: "#293e28"
typography:
  display:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "clamp(40px, 3.7vw, 56px)"
    fontWeight: 400
    lineHeight: 1.22
    letterSpacing: "-0.025em"
  headline:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "42px"
    fontWeight: 700
    lineHeight: 1.14
    letterSpacing: "-0.025em"
  title:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "28px"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.015em"
  body:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.5
  intro:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "21px"
    fontWeight: 300
    lineHeight: 1.5
  button:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "16px"
    fontWeight: 700
    lineHeight: 1.3
  link:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "14px"
    fontWeight: 700
    lineHeight: 1.5
  nav:
    fontFamily: "Heebo, Arial, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
rounded:
  button: "6px"
  panel: "8px"
  context-card: "16px"
  service-card: "24px"
  outline-button: "28px"
spacing:
  desktop-gutter: "40px"
  mobile-gutter: "22px"
  section: "90px"
  section-mobile: "65px"
  split-gap: "72px"
  card-gap: "28px"
components:
  button-primary:
    textColor: "{colors.cta-text}"
    typography: "{typography.button}"
    rounded: "{rounded.button}"
    padding: "12px 27px"
  button-outline:
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.outline-button}"
    padding: "12px 27px"
  button-outline-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.white}"
  link-text:
    textColor: "{colors.ink}"
    typography: "{typography.link}"
  service-card:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.service-card}"
    padding: "30px"
  context-card:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.context-card}"
    padding: "25px"
  header:
    backgroundColor: "{colors.charcoal}"
    textColor: "#fff"
    height: "68px"
    typography: "{typography.nav}"
  repository-panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "42px 38px"
---

# Design System: NexusMind · SmartCoder lime

## Overview

**Creative North Star: "SmartCoder lime, NexusMind control"**

The owner-selected SmartCoder/Globant lime identity is the canonical NexusMind landing: black navigation, white editorial space, a green atmospheric hero, Heebo typography and rounded conceptual cards. Dark mode translates the same hierarchy to deep green surfaces and light text. The earlier blue landing is superseded; this records the implemented adaptation, not a claim of pixel identity with the reference.

The implementation in `src/pages/index.astro`, `src/styles/smartcoder.css`, `src/scripts/smartcoder.ts` and `src/scripts/hero-mesh.ts` is the token authority. The pinned owner reference is `/Volumes/external/Documents/smartcoder/smartcoderlabs-design-pack/design/globant-inspired/DESIGN.md`; reference-only carousel, email-contact panel and alternate palettes are not landing components. Source inspection supports this document; it does not certify browser, device, contrast or accessibility readiness.

This document is scoped to `apps/landing`. The owner also authorized the same identity for the admin, but admin changes require owner review before any admin push. Landing publication is authorized separately. This document neither records an implemented admin system nor authorizes publishing admin changes.

**Key Characteristics:**
- Lime accents, black framing and light/dark green editorial surfaces.
- Heebo regular hero text, bold section headings and light introductory copy.
- Generous split sections, three-column capability groups and narrow conceptual images.
- Interactive mesh, pointer deformation and native-scroll motion with static reduced-motion content.
- Owner-supplied assets with provenance and font license retained in `public/smartcoder/`.

## Colors

Lime and green establish the accent family, neutral editorial surfaces carry content, and charcoal frames navigation and footer. Frontmatter owns exact reusable color primitives; gradients remain component effects in the sidecar. Synthesized OKLCH tonal ramps in the sidecar are inspection aids, not additional runtime tokens.

### Primary
- **Lime** supplies the brand mark, text selection and focus on dark framing.
- **Green** supplies light-mode emphasis, structure and focus; **Dark Green** is the lime-valued counterpart in dark mode.
- **CTA Start / CTA End** form the primary action gradient; **CTA Text** supplies its dark foreground. Gradients are not `backgroundColor` tokens.

### Secondary
- **Mint** marks the closing rule and visible focus in the dark closing section.

### Neutral
- **Ink / Body** separate headings and secondary copy; **White / Soft / Panel / Pale** distinguish page, tone band, raised surface and explanatory diagram.
- **Line** supplies separators. **Charcoal** remains the black navigation/footer surface in both themes.
- The `dark-*` primitives map to the matching `--sc-*` custom properties under `data-theme=dark`; lime stays unchanged. The legacy token name **White** denotes the page surface and becomes deep green in dark mode.

**The Source Authority Rule.** Use the canonical landing source for exact tokens; carry the chosen lime identity forward without reviving the discarded blue landing.

## Typography

**Display Font:** Heebo, with Arial and sans-serif fallbacks.
**Body Font:** the same family. Local files provide weights 300, 400, 500 and 700.

**Character:** an open editorial sans hierarchy, with regular hero text and stronger section/card headings. Introductory paragraphs are lighter than ordinary content.

### Hierarchy
- **Display:** frontmatter records the base hero treatment; Layout records responsive overrides. The emphasized line changes color rather than weight.
- **Headline:** recurring section headings. The centered problem heading and closing heading remain local variants rather than a fabricated uniform scale.
- **Title:** base service-card headings, with responsive adjustments.
- **Body:** page default. Explanatory split copy uses 18px and a 60ch maximum; service prose uses 16px at 1.6 line height.
- **Intro:** section leads. Hero lead copy uses the same size/weight with 1.55 line height and a 540px measure.
- **Button / Link / Nav:** distinguish bold actions from regular navigation.

**The Content Hierarchy Rule.** Keep the regular hero, bold section headings and readable body roles distinct. Small illustration metadata is not a functional text scale.

The 11px hero metadata is not canonized as a reusable typography role. Its rendered legibility remains a review concern; this document does not legitimize it by adding a token.

## Layout

Wide content uses `min(1220px, calc(100% - 80px))`; editorial content uses `min(1070px, calc(100% - 100px))`. The navigation container uses `min(1260px, calc(100% - 80px))`. Split stories use equal columns and a 72px gap; capability cards use three columns and a 28px gap. Main sections have 90px vertical padding. Tone bands and closing content have local spacing.

The sticky header is 68px high. The desktop hero uses a 690px minimum height, a 1.2fr/1fr split, a 35px gap and 90px/110px vertical padding. Content stays in native document flow; scroll motion does not pin sections or take over scrolling.

### Responsive behavior
- **1500px minimum:** the hero container expands to 1350px with 130px total gutters and an 80px gap; hero display is 56px.
- **1100px maximum:** hero display becomes 43px, hero columns become 1.15fr/1fr and the gap becomes 25px; split gap becomes 40px. Service cards use 24px padding and 26px titles.
- **800px maximum:** shared content gutters become 22px, header becomes 64px, desktop navigation becomes a native disclosure menu, main section padding becomes 65px and content grids stack. Hero display is `clamp(32px, 5.5vw, 44px)`; section headings become 36px. Final mesh-control styling sets hero bottom padding to 150px. Service cards use 28px padding and 29px titles.
- **520px maximum:** the two illustrative hero cards stack, lose their rotations and top offsets, and use normal-flow captions.
- **390px maximum:** hero display becomes 32px and the repository panel/action spacing tightens.

## Elevation & Depth

Depth combines a pale radial hero gradient, soft tinted shadows and layered cards. Dark mode changes surfaces and shadow tone without adding a separate glow vocabulary. The repeated capability shadow is `0 6px 28px #1a251e0f`; hero and architecture surfaces use the theme-aware `--sc-shadow`. Exact gradients and shadow values are in the sidecar.

**The Soft Depth Rule.** Use tonal gradients and diffuse shadows for explanatory surfaces; retain the implemented green family in both themes.

The hero mesh uses local canvas geometry, not an external scene or video. Fine-pointer movement deforms its surface, decorative clicks send a wave, and a ring cursor follows within the hero. It pauses through a visible control and suspends when hidden or offscreen. Reduced motion keeps a static mesh and disables scroll entrances. GSAP enhances native scrolling with short entrances, desktop card parallax and image scale changes; content is rendered before enhancement.

## Shapes

Primary actions use modest corners; outline actions use a rounded pill. Context cards use medium rounding, capability cards use the largest recurring rounding, and repository/navigation panels use compact rounding. Architecture nodes retain their own smaller corners. Hairline dividers separate details and footer content. Tilted explanatory cards are decorative surfaces, not a shape requirement for controls. Icons are inline SVG. There is no input or form-field system on this landing.

## Components

### Buttons and text links

Primary actions use the lime/green gradient with dark bold text, lightening on hover. Outline actions use an ink border and switch to ink fill/page-surface text on hover. Buttons have a 46px minimum height and lift 2px over 0.2 seconds, except under reduced motion. Text links have a 44px minimum height, pair text with inline SVG and underline on hover. Ordinary visible focus uses a 3px green outline offset by 5px; header/footer focus uses lime and the dark closing section uses mint. Repository-panel focus returns to the theme green.

### Capability and context cards

Capability cards have a conceptual landscape image, a title, introductory feature copy and native disclosures. Desktop images use a 2.7 aspect ratio; mobile uses 3. Context cards illustrate shared cross-tool memory, with distinct regular/medium type roles and an explicit illustrative caption. Their rotations are removed on narrow screens. Neither pattern implies live customer activity.

### Capability disclosures

Native `details`/`summary` provides an accessible browser baseline. Separators use the theme line color. Summaries have a 58px minimum height, medium 15px text and an inline SVG plus that rotates when open. Expanded copy adds bottom space; toggle events refresh later scroll-trigger positions.

### Navigation and theme

A persistent black sticky band combines the brand, anchors, GitHub action and a circular theme button. At the mobile breakpoint, native details navigation appears with a compact dark dropdown. Enhancement closes it on outside click, selected link or Escape; Escape returns focus to its summary. Theme selection persists under `nexusmind-comparison-theme`, is applied before paint and is reported through the button's pressed state. These are source behaviors, not a keyboard or device certification.

### Mesh controls

The rounded background-pause control is visible only after canvas enhancement and outside reduced-motion preference. Its pressed state represents paused motion. The cursor enhancement is restricted to fine-pointer, hover-capable environments without reduced motion; leaving the hero restores normal cursor behavior. The canvas is decorative and excluded from accessibility content.

### Repository panel

A light/dark theme surface sits against an image-backed green closing band. It combines the repository identity, a real repository action and license/pull-request links. It is not a contact or signup form.

## Do's and Don'ts

### Do:
- Do preserve Heebo, lime accents, black navigation and the paired light/dark theme hierarchy.
- Do retain owner-supplied asset provenance and the Heebo license.
- Do keep native scrolling, visible keyboard focus, mesh pause and reduced-motion paths.
- Do label illustrative content honestly and source product claims from PRODUCT.md and repository content.
- Do review admin changes with the owner before any admin push; publish the authorized landing independently.

### Don't:
- Don't revive the discarded blue landing or copy reference-only carousel/contact components into this system.
- Don't treat small hero metadata as the standard for functional interface text.
- Don't replace SVG icons with Unicode arrows or other text glyph icons.
- Don't claim screenshot fidelity, visual approval or accessibility certification from this source extraction.
- Don't introduce invented customers, metrics, live status or fake form submissions.
