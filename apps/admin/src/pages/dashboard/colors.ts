// Shared categorical accent palette for dashboard widgets (stat tiles, Memory
// Types, Top Projects, Memory Health legend, ...).
//
// The app's design tokens (src/index.css) only define two accent hues
// (accent-blue, accent-purple) plus the three status hues (success/warning/
// error). There is no accent-teal/orange/pink token. Per the task's hard
// constraint ("do not touch global CSS"), this palette reuses exactly those
// five existing CSS custom properties instead of inventing new colors, and
// cycles them in a fixed order across widgets so a given index always maps
// to the same hue (dataviz "assign categorical hues in fixed order" rule).
export const DASHBOARD_ACCENTS = [
  'var(--color-accent-blue)',
  'var(--color-status-success)',
  'var(--color-status-warning)',
  'var(--color-status-error)',
  'var(--color-accent-purple)',
] as const

export function accentFor(index: number): string {
  return DASHBOARD_ACCENTS[index % DASHBOARD_ACCENTS.length]
}
