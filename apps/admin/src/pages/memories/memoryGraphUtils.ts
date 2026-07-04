import type { MemoryGraphResponse } from '../../types'

// ── Color maps ────────────────────────────────────────────────────────────────

export const MEM_NODE_COLORS: Record<string, string> = {
  Memory:     '#2997ff',
  Project:    '#6366f1',
  Tag:        '#34d399',
  User:       '#fb923c',
  Session:    '#a78bfa',
  Collection: '#facc15',
  AuditEvent: '#94a3b8',
}

export const MEM_EDGE_COLORS: Record<string, string> = {
  belongs_to:    '#6366f1',
  in_session:    '#a78bfa',
  created_by:    '#fb923c',
  in_collection: '#facc15',
  tagged:        '#34d399',
  performed_by:  '#fb923c',
  targets:       '#2997ff',
}

// ── Force-graph data shapes ───────────────────────────────────────────────────

export interface MemForceNode {
  id: string
  type: string
  label: string
  // ForceGraph3D injects x/y/z at runtime
  x?: number
  y?: number
  z?: number
}

export interface MemForceLink {
  source: string
  target: string
  type: string
}

// ── Pure mapping functions ─────────────────────────────────────────────────────

export function mapMemGraphData(graph: MemoryGraphResponse): { nodes: MemForceNode[]; links: MemForceLink[] } {
  return {
    nodes: graph.nodes.map(n => ({ id: n.id, type: n.type, label: n.label })),
    links: graph.edges.map(e => ({ source: e.from_id, target: e.to_id, type: e.type })),
  }
}

export function filterMemNodesByTypes(nodes: MemForceNode[], visibleTypes: Set<string>): MemForceNode[] {
  return nodes.filter(n => visibleTypes.has(n.type))
}

export function filterMemLinksByNodes(links: MemForceLink[], nodeIds: Set<string>): MemForceLink[] {
  // ForceGraph3D mutates source/target from string → object after first render
  // so use String() to normalize
  return links.filter(
    l => nodeIds.has(String((l.source as unknown as { id?: string })?.id ?? l.source))
      && nodeIds.has(String((l.target as unknown as { id?: string })?.id ?? l.target)),
  )
}
