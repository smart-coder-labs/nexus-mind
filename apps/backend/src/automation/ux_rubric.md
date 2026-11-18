# UX/UI review rubric

Sources: WCAG 2.2 AA, Nielsen's 10 heuristics + 0–4 severity, the DesignKits
reviewer rules (QA, rules and UX-learnings docs of the 24 kits), the Kasymir
design-reviewer rules, and WebAIM Million (the six most common failures first).
Every finding cites exactly one rule id below (or `axe:<rule>` / `DS-<id>` for a
project design-system rule). `[M]` = measure it in the DOM, never judge from
pixels. `[V]` = visual judgment from a screenshot.

## Severity (Nielsen 0–4 → finding severity)
- critical (4): blocks a task or excludes a user group (keyboard trap, unlabeled
  primary control, unreadable text on a key screen, data loss risk).
- high (3): major friction on a primary flow; WCAG A/AA failure on primary content.
- medium (2): noticeable friction or WCAG failure on secondary content.
- low (1): cosmetic or minor inconsistency.
- info (0): polish idea. Do NOT file info findings.

## Accessibility (WCAG 2.2 AA)
- A11Y-CONTRAST [M] text < 4.5:1 (large text ≥ 24px or ≥ 18.66px bold: < 3:1). WCAG 1.4.3. Check both themes.
- A11Y-NONTEXT-CONTRAST [M] UI component boundaries, focus rings and icons < 3:1. WCAG 1.4.11.
- A11Y-NAME [M] button, link, input, icon-only control without an accessible name. WCAG 4.1.2 / 2.4.4.
- A11Y-LABEL [M] form field without a programmatic label (placeholder is not a label). WCAG 1.3.1 / 3.3.2.
- A11Y-ALT [M] meaningful image without alt; decorative image/svg not hidden. WCAG 1.1.1.
- A11Y-LANG [M] `<html lang>` missing or wrong. WCAG 3.1.1.
- A11Y-HEADINGS [M] no h1, several h1, or skipped heading levels on a page. WCAG 1.3.1.
- A11Y-LANDMARKS [M] no `main` landmark or navigation not in `nav`. WCAG 1.3.1.
- A11Y-COLOR-ONLY [V] status/meaning conveyed by color alone. WCAG 1.4.1.
- A11Y-LIVE [M] async status, toasts or errors not announced (no aria-live / role=status|alert). WCAG 4.1.3.
- A11Y-ERRORS [M] form errors not shown as text tied to the field. WCAG 3.3.1 / 3.3.3.
- A11Y-REDUNDANT-ENTRY [V] user must re-type info already given in the same flow. WCAG 3.3.7.
- A11Y-AUTH [V] login requires a cognitive test (copying, puzzle) with no alternative. WCAG 3.3.8.

## Keyboard & focus
- KBD-REACH [M] an interactive element cannot be reached with Tab, or tab order is illogical. WCAG 2.1.1 / 2.4.3.
- KBD-TRAP [M] focus cannot leave a component. WCAG 2.1.2.
- KBD-FOCUS-VISIBLE [M] focused element has no visible indicator (outline removed). WCAG 2.4.7.
- KBD-FOCUS-OBSCURED [M] focused element hidden under a sticky header/footer or overlay. WCAG 2.4.11.
- KBD-ESC [M] dialog/menu/popover does not close on Escape or does not return focus to its trigger.
- KBD-DIALOG [M] modal does not move focus inside or lets Tab escape behind it.

## Targets & layout
- TGT-SIZE [M] pointer target < 24×24 CSS px without enough spacing (WCAG 2.5.8); flag < 44px for primary mobile actions as low.
- RSP-OVERFLOW [M] horizontal scroll at the page level at 390px (or 320px reflow). WCAG 1.4.10.
- RSP-TRUNCATION [V] text or numbers clipped, overlapping or cut without a way to read them (tooltip/expand).
- RSP-ZOOM [M] content or controls lost at 200% text size / `user-scalable=no`. WCAG 1.4.4.
- LAY-FIRST-VIEW [V] the key information and the primary action are not visible on the first screen (desktop and phone).
- LAY-ALIGNMENT [V] elements off the grid, inconsistent gutters, misaligned columns.
- LAY-SPACING [V] spacing not on a consistent scale; cramped or uneven groups (proximity broken).
- LAY-DENSITY [V] walls of controls; more than ~8 equal-weight choices in one group (Hick).

## Visual design & consistency
- VIS-HIERARCHY [V] no clear primary action, or several elements compete as primary (more than one primary button per group).
- VIS-CONSISTENCY [V] the same component looks or behaves differently across screens (buttons, tables, badges, dates). Nielsen H4.
- VIS-TOKENS [V] off-palette colors, mixed radii, one-off shadows, mismatched fonts against the rest of the app.
- VIS-DARK-MODE [V] white boxes, invisible borders, low contrast or unreadable charts in dark theme.
- VIS-NUMBERS [V] numeric columns not right-aligned / not tabular; inconsistent number, money or date formats.
- VIS-ICONS [V] ambiguous icon-only actions without a label or tooltip.

## Motion
- MOT-REDUCED [M] animation ignores `prefers-reduced-motion`. WCAG 2.3.3.
- MOT-PAUSE [M] auto-moving content longer than 5s without pause/stop. WCAG 2.2.2.
- MOT-FEEDBACK [V] no visible response within ~100ms after an action (Doherty); spinners with no context for long waits.

## States & feedback (Nielsen H1, H9)
- STA-EMPTY [V] empty state is blank or a bare "No data" with no explanation or next action.
- STA-LOADING [V] layout jumps or blank screen while loading; no skeleton/progress for slow content.
- STA-ERROR [V] errors are technical (codes, stack traces, raw ids), blame the user, or offer no recovery/retry.
- STA-SUCCESS [V] action completes with no confirmation, or the confirmation shows internal data (UUIDs, enum keys).
- STA-DISABLED [V] disabled controls with no reason; unavailable options hidden where showing them disabled would orient the user.
- STA-DESTRUCTIVE [V] destructive action without confirmation or undo; confirmation that does not name the object. Nielsen H3/H5.

## Forms (Postel)
- FRM-INPUT-TYPE [M] wrong `type`/`inputmode`/`autocomplete` (email, tel, numeric, one-time-code, current-password).
- FRM-VALIDATION [V] validation only on submit for long forms, or errors that clear the user's input.
- FRM-REQUIRED [V] required fields not indicated; format expectations not stated before the error.
- FRM-DEFAULTS [V] missing sensible defaults; the user must enter data the app already knows.

## Navigation & information architecture (Nielsen H2, H6, H7)
- NAV-LOCATION [V] the user cannot tell where they are (no active nav state, no page title, no breadcrumb in deep pages).
- NAV-LABELS [V] jargon, internal names or inconsistent terms for the same thing. Nielsen H2/H4.
- NAV-DEAD-END [V] a screen with no way back or forward other than the browser.
- NAV-SEARCH [V] long lists/tables with no search, filter or sort.

## Copy & content
- CPY-CLARITY [V] vague button labels ("Submit", "OK") where a verb + object would be clear; mixed languages in one UI.
- CPY-I18N [V] untranslated strings, broken pluralization, placeholders like `{count}` or `undefined` shown to users.
- CPY-TONE [V] error and empty-state copy that is robotic, blaming or alarming.

## Honesty (dark patterns)
- HON-FAKE-DATA [V] sample or placeholder data presented as real; fake "live" badges; invented numbers.
- HON-URGENCY [V] fake scarcity or countdowns that reset.
- HON-OBSTRUCTION [V] cancel/unsubscribe/delete made harder than sign-up; confirmshaming; preselected opt-ins.

## Nielsen heuristics (use only when no specific rule above fits)
H1 visibility of status · H2 match with the real world · H3 user control and freedom · H4 consistency and
standards · H5 error prevention · H6 recognition over recall · H7 flexibility and efficiency ·
H8 aesthetic and minimalist design · H9 error recovery · H10 help and documentation.

## Domain packs (apply only the configured `domain_pack`)
- saas_admin: tables have sort/filter/pagination and a visible row count; bulk actions show the selection count and offer undo; settings save state is explicit; ⌘K or search reaches main entities; role-restricted actions explained, not silently missing.
- fintech: money right-aligned, tabular, with currency; red only for real failures, not normal outflows; money movements have a review step before confirm and a clear receipt; balances can be hidden; legal notes linked, not buried.
- marketplace: total price including fees visible before checkout; unavailable dates/variants shown disabled before selection; "you won't be charged yet" where true; filters show result counts.
- ecommerce: product page shows price, availability and delivery before the fold; cart edits inline; checkout as guest allowed; coupon conditions visible; sold-out variants disabled, not hidden.
- ai_chat: composer has a label; thread is `role=log` with aria-live for new replies; stop generation without losing the draft; retry in place on errors; auto-scroll only when already at the bottom; AI output labelled as possibly wrong.
- content: readable line length (≤ ~75ch), sufficient body size (≥ 16px), clear article hierarchy, images with captions/alt.

## Measured checks (run with `browser_evaluate`)
1. axe (when `typeof axe !== 'undefined'`):
   `() => axe.run(document,{runOnly:{type:'tag',values:['wcag2a','wcag2aa','wcag21a','wcag21aa','wcag22aa']},resultTypes:['violations','incomplete']}).then(r=>({violations:r.violations.map(v=>({id:v.id,impact:v.impact,help:v.help,wcag:v.tags.filter(t=>/^wcag\d/.test(t)),nodes:v.nodes.slice(0,5).map(n=>({target:n.target.join(' '),summary:n.failureSummary}))})),incomplete:r.incomplete.map(v=>({id:v.id,nodes:v.nodes.slice(0,3).map(n=>n.target.join(' '))}))}))`
   Each violation is evidence for the matching rule; cite it as `axe:<id>`. `incomplete` items need your own check before reporting.
2. Overflow and small targets:
   `() => ({overflow: document.documentElement.scrollWidth > innerWidth + 1, small: [...document.querySelectorAll('a[href],button,input,select,textarea,[role=button],[role=link],[tabindex]:not([tabindex="-1"])')].filter(e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0&&(r.width<24||r.height<24)}).slice(0,10).map(e=>(e.id?'#'+e.id:e.tagName.toLowerCase())+' '+(e.getAttribute('aria-label')||e.textContent||'').trim().slice(0,40))})`
3. Reduced motion: check whether the stylesheets contain a `prefers-reduced-motion` rule when the page animates.
4. Console: read browser console errors once per page; uncaught errors that break UI are STA-ERROR evidence.
