// Pure, framework-free helpers for the immersive Graph page chrome
// (src/pages/Graph.tsx). Kept separate so the type-filter toggle logic and
// the node-type palette are unit-testable without mounting the page.

/** All node types the memory graph can return, in the order the design
 *  mock renders its filter pills. */
export const ALL_NODE_TYPES = ['Memory', 'Project', 'Session', 'User', 'Collection', 'Tag', 'AuditEvent']

/**
 * Per-type node colors, matched exactly to the dark-glass mockup's palette
 * (Graph-spec.md). Deliberately distinct from `memoryGraphUtils.MEM_NODE_COLORS`
 * — that map colors Memory/Project nodes by their *owning project* for the
 * boxed Memories-page graphs, while this redesign colors every node by its
 * *type* per the mock, and represents project identity via the legend-chip
 * camera-focus interaction instead.
 */
export const NODE_TYPE_COLORS: Record<string, string> = {
  Memory:     '#3b82f6',
  Project:    '#6366f1',
  Session:    '#a855f7',
  User:       '#f97316',
  Collection: '#eab308',
  Tag:        '#10b981',
  AuditEvent: '#94a3b8',
}

/**
 * Toggle logic for the node-type filter pills, matching the mock:
 *  - starting from "all visible" → clicking a pill isolates it (shows only
 *    that type)
 *  - clicking the sole remaining active type restores all types
 *  - otherwise, plain toggle (add/remove from the visible set)
 */
export function toggleNodeType(current: string[], all: string[], type: string): string[] {
  const currentSet = new Set(current)
  if (currentSet.size === all.length) {
    return [type]
  }
  if (currentSet.size === 1 && currentSet.has(type)) {
    return [...all]
  }
  if (currentSet.has(type)) {
    return current.filter(t => t !== type)
  }
  return [...current, type]
}

/** Converts a `#rrggbb`/`#rgb` hex color to an `rgba(...)` string at the
 *  given alpha — used to dim nodes that don't match the active search or
 *  project focus without losing their type-color identity entirely. */
export function hexToRgba(hex: string, alpha: number): string {
  const clean = hex.replace('#', '')
  const full = clean.length === 3 ? clean.split('').map(c => c + c).join('') : clean
  const int = parseInt(full, 16)
  const r = (int >> 16) & 255
  const g = (int >> 8) & 255
  const b = int & 255
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}
