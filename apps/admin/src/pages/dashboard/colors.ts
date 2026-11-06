// Categorical hues stay stable across pages. They are distinct from success,
// warning, and error so data categories never imply a status accidentally.
export const DASHBOARD_ACCENTS = [
  'var(--data-lime)',
  'var(--data-blue)',
  'var(--data-teal)',
  'var(--data-amber)',
  'var(--data-rose)',
] as const

export function accentFor(index: number): string {
  return DASHBOARD_ACCENTS[index % DASHBOARD_ACCENTS.length]
}
