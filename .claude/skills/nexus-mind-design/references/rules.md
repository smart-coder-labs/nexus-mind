# Rules and QA checklist

Floor: WCAG 2.2 AA (root `DESIGN.md`). IDs match `scripts/design-audit.mjs` where a rule is automated.

## Tokens (automated)

- D-TOK-01 MUST — no literal colours outside the token sheets. Canvas/WebGL graph utils are the only exception (warned).
- D-TOK-02 MUST — no Tailwind palette colours (`text-red-400`, `bg-zinc-800`). Use a role.
- D-TOK-03 SHOULD — arbitrary px (`text-[11px]`, `p-[13px]`). The admin has ~880 legacy ones; don't add more. Use the scale.
- D-TOK-04 SHOULD — white text on a lime fill. Lime takes `primary-foreground`.

## Accessibility

- D-A11Y-01 MUST (automated) — every `<img>` has `alt` (`alt=""` if decorative).
- D-A11Y-02 MUST (automated) — icon-only buttons have `aria-label` (or `title`). Toggle buttons also set `aria-pressed`.
- Text clears 4.5:1 on its real background, in both themes. Muted text on a tinted card is the usual failure.
- Every input has a label; search fields carry `aria-label` (the backoffice test asserts the existing ones).
- Dialogs: accessible name, focus restored on close, Esc closes, 16px viewport margins, internal scroll.
- Keyboard: every action reachable, visible focus (`:focus-visible` ring), no keyboard traps in graphs.

## Motion

- D-MOT-01 MUST (automated per CSS file) — animation/transition CSS has a `prefers-reduced-motion` counterpart.
- framer-motion components read `useReducedMotion()`. No decorative chart entry animation.
- The landing mesh keeps its pause control and stops offscreen/hidden.

## Data honesty

- Missing values render "—"; zero renders 0. Never fill gaps with invented numbers or derived percentages the API doesn't return.
- Examples/sample data are labelled as illustrative (the landing hero already is). No invented customer metrics, pricing or waitlist on the landing.
- Don't compute in the UI what the backend owns (usage totals, factory verdicts, permissions). Show it.

## Copy

- English, sentence case for headings, labels and table headers ("Create organization", not "Create Organization" — the backoffice still has some Title Case; fix when you touch it).
- Name the object and the action on buttons ("Revoke key", not "OK"). Destructive confirmations say what is lost.
- Errors say what happened and what to do next; never raw JSON or a stack.
- Identifiers (ids, keys, hashes, paths) in `font-mono`, truncated with a `title` holding the full value.

## Layout

- No horizontal page scroll at 390px. Wide tables scroll inside their container; cards carry `min-w-0`.
- Summary strips wrap with 8px gaps; at ≤480px each summary takes a full row.
- Main gutter 20px (16px ≤640px). Topbar ≥56px.

## Visual QA (look at the screenshots)

1. 1440 and 390, light and dark (admin); backoffice dark only.
2. Lime appears only on actions, active nav and brand links.
3. Hierarchy: one page title, compact counts, nothing competing with the primary action.
4. Empty, loading (skeleton) and error states exist and look designed.
5. Permission: a role that can't use the screen doesn't see its nav entry or action.
6. Nothing overflows, truncates without a tooltip, or overlaps at 390.
