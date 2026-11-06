# Claude-style refinement — 2026-10-09

## Direction

Preserved the restored Claude design: dark surfaces, glass overlays, full sidebar, blue actions, existing typography and content. This is a component polish pass, not a new visual system.

## Changes

- Added missing admin semantic color tokens and replaced dynamic Tailwind classes that did not generate textarea resize and table width styles.
- Improved muted text and keyboard focus visibility in admin and landing.
- Connected shared input labels, descriptions and error states; added names and states to tag suggestions.
- Corrected modal initial focus, focus trapping, Escape restoration and accessible titles.
- Corrected empty table pagination, sortable header semantics and row checkbox propagation.
- Removed nested command palette buttons and duplicate project modal surfaces.
- Refined memory, project and role form typography and field labels.
- Contained the memory table horizontally on mobile and kept its header controls usable.
- Made KPI motion optional under reduced motion and manually scrollable on small screens.
- Refined landing waitlist spacing, selection states, labels and submission error feedback.
- Preserved the earlier landing native-scroll restoration and pre-existing Factory edits.

## Validation

- Admin full suite: 45 files, 364 tests passed, including seven new shared-component regression tests.
- After the final project/role label edits: admin production build passed and all six project tests passed.
- Landing production build passed.
- Browser checks: Memories desktop/mobile, new memory mobile dialog, create project and create role desktop dialogs, and landing waitlist desktop/mobile.
- At a 390px viewport, the memory main region measured 382px client/scroll width; only its table region scrolls horizontally (348px client, 720px content).
- Landing at 390px measured 382px document width and native scroll reached the waitlist. Interest toggle exposed its selected state.
- Project dialog Escape restored focus to its trigger.
- Screenshots in this directory record final representative states.

## Limits and preserved choices

Admin browser checks used illustrative, read-only fixtures. No production records, permissions or waitlist submissions were written. This pass does not certify every route or backend flow. Existing large graph-bundle warning remains in the admin build.

The Impeccable font hook flagged Inter against the generic admin reference. Kept Inter because the product-specific `docs/DESIGN_DIRECTION.md` explicitly specifies it and the user requested preserving Claude's design. A narrow `design-system-font` / `Inter` ignore records that decision in `.impeccable/config.json`.
