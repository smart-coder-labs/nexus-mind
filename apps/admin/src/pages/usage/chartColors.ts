/**
 * Chart palette for the Usage panel — deliberately NOT the shared
 * `dashboard/colors.ts` categorical cycle.
 *
 * Why a separate palette:
 *
 * 1. The app's two accent hues fail colour-vision-deficiency separation against
 *    each other. Running the dataviz validator on `#2997ff` (accent-on-dark) vs
 *    `#bf5af2` (accent-purple) over this surface returns deutan ΔE 2.4 — far
 *    below the ΔE 8 floor. A deuteranope cannot tell those two series apart, so
 *    a categorical palette built from them is unusable for real data.
 * 2. `dashboard/colors.ts` also seats the three status hues (success/warning/
 *    error) as generic series colours. Status colours are reserved: reusing
 *    "error red" as "series 4" teaches the reader a meaning that isn't there.
 *
 * Every measure on this page — tokens, duration, event counts — is a magnitude,
 * and tokens-in vs tokens-out are two parts of one magnitude. None of that is
 * an *identity* encoding, so none of it needs categorical colour. Encoding by
 * lightness within a single hue is CVD-safe by construction: the channel that
 * separates the steps is the one channel every form of CVD preserves.
 *
 * Validated with the skill's `validate_palette.js` against surface `#12141a`
 * (the glass panel over the app's `--color-bg-secondary`):
 *
 *   #2a6dae ↔ #74bcff → CVD ΔE 25.1 (deutan) · normal ΔE 25.0 · contrast PASS
 *
 * The validator's "lightness band" check FAILs on this pair by design — that
 * check exists to stop categorical hues from having unequal visual weight, and
 * unequal weight is exactly the point of a sequential pair. The rule that does
 * apply to a sequential ramp, lightness monotonicity, holds.
 *
 * A dimmer fourth step (`#1e4976`) was measured and dropped: 1.99:1 against the
 * surface, under the 3:1 floor.
 */

/** Primary series hue — the app's dark-surface accent blue. */
export const CHART_PRIMARY = '#2997ff'

/** Dimmer step of the same hue. Carries `tokens_in` in the stacked trend. */
export const CHART_DIM = '#2a6dae'

/** Brighter step of the same hue. Carries `tokens_out` in the stacked trend. */
export const CHART_BRIGHT = '#74bcff'

/**
 * Surface colour the marks are separated *by* — the 2px gaps between stacked
 * segments and the rings on hover dots are painted in this, not in a stroke.
 */
export const CHART_SURFACE = '#12141a'

/** Recessive gridline / axis ink — one step off the surface, never dashed. */
export const CHART_GRID = 'rgba(255,255,255,0.07)'
