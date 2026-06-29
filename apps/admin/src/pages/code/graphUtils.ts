import type { CodeGraph } from '../../types'

// ── Constants ─────────────────────────────────────────────────────────────────

/**
 * Node types visible by default (LOD filter).
 * Folder and External are hidden until the user explicitly enables them.
 */
export const DEFAULT_VISIBLE_TYPES = new Set<string>([
  'Project',
  'Folder',
  'File',
  'Module',
  'Function',
  'Method',
  'Class',
  'Struct',
  'Interface',
  'Type',
  'Enum',
])

/** Zoom level above which node labels are rendered to avoid label soup. */
export const LABEL_ZOOM_THRESHOLD = 1.5

/**
 * When the number of External nodes exceeds this value, they are collapsed
 * into a single aggregate node to prevent physics and visual overload.
 */
export const EXTERNAL_COLLAPSE_THRESHOLD = 150

// ── Color maps ────────────────────────────────────────────────────────────────

export const NODE_COLORS: Record<string, string> = {
  Project:   '#6366f1', // indigo
  Folder:    '#94a3b8', // slate-400
  File:      '#38bdf8', // sky-400
  Module:    '#a78bfa', // violet-400
  Function:  '#34d399', // emerald-400
  Method:    '#4ade80', // green-400
  Class:     '#fb923c', // orange-400
  Struct:    '#f97316', // orange-500
  Interface: '#facc15', // yellow-400
  Type:      '#e879f9', // fuchsia-400
  Enum:      '#c084fc', // purple-400
  External:  '#6b7280', // gray-500
}

export const EDGE_COLORS: Record<string, string> = {
  contains_folder:  '#64748b', // slate-500 (structural skeleton, visible on dark bg)
  contains_file:    '#475569', // slate-600
  defines:          '#3b82f6', // blue-500 (accent)
  defines_method:   '#6366f1', // indigo-500
  imports:          '#f59e0b', // amber-500 (distinct)
}

// ── Force-graph data shapes ───────────────────────────────────────────────────

export interface ForceGraphNode {
  id: number
  type: string
  name: string
  fp: string | null
  startLine?: number | null
  endLine?: number | null
  language?: string | null
  // ForceGraph injects x/y/z/vx/vy at runtime
  x?: number
  y?: number
}

export interface ForceGraphLink {
  source: number
  target: number
  type: string
}

export interface MappedGraph {
  nodes: ForceGraphNode[]
  links: ForceGraphLink[]
}

// ── Pure mapping functions (unit-tested seam) ─────────────────────────────────

/**
 * Maps the API response (from_id/to_id) to force-graph format (source/target).
 * This is the primary testable seam for the graph data transform.
 */
export function mapGraphData(graph: CodeGraph): MappedGraph {
  return {
    nodes: graph.nodes.map(n => ({
      id: n.id,
      type: n.type,
      name: n.name,
      fp: n.file_path,
      startLine: n.start_line ?? null,
      endLine: n.end_line ?? null,
      language: n.language ?? null,
    })),
    links: graph.edges.map(e => ({
      source: e.from_id,
      target: e.to_id,
      type: e.type,
    })),
  }
}

/**
 * Filters nodes to only those whose type is in visibleTypes.
 */
export function filterNodesByTypes(
  nodes: ForceGraphNode[],
  visibleTypes: Set<string>,
): ForceGraphNode[] {
  return nodes.filter(n => visibleTypes.has(n.type))
}

/**
 * Removes links where either endpoint is not in nodeIds.
 * Prevents dangling links after client-side LOD filtering.
 */
export function filterLinksByNodes(
  links: ForceGraphLink[],
  nodeIds: Set<number>,
): ForceGraphLink[] {
  return links.filter(
    l => nodeIds.has(Number(l.source)) && nodeIds.has(Number(l.target)),
  )
}

/**
 * Collapses External nodes into a single aggregate node when the count
 * exceeds EXTERNAL_COLLAPSE_THRESHOLD and expandExternals is false.
 * Keeps all other nodes and remaps import links to the aggregate.
 */
export function computeExternalAggregate(
  nodes: ForceGraphNode[],
  links: ForceGraphLink[],
  expandExternals: boolean,
): { nodes: ForceGraphNode[]; links: ForceGraphLink[] } {
  const externalNodes = nodes.filter(n => n.type === 'External')

  if (expandExternals || externalNodes.length <= EXTERNAL_COLLAPSE_THRESHOLD) {
    return { nodes, links }
  }

  const AGG_ID = -1
  const externalIds = new Set(externalNodes.map(n => n.id))
  const nonExternal = nodes.filter(n => n.type !== 'External')

  const aggNode: ForceGraphNode = {
    id: AGG_ID,
    type: 'External',
    name: `External dependencies (${externalNodes.length})`,
    fp: null,
    startLine: null,
    endLine: null,
    language: null,
  }

  // Remap links pointing to/from external nodes → aggregate
  const remappedLinks = links.map(l => {
    const targetIsExternal = externalIds.has(Number(l.target))
    const sourceIsExternal = externalIds.has(Number(l.source))
    if (targetIsExternal) return { ...l, target: AGG_ID }
    if (sourceIsExternal) return { ...l, source: AGG_ID }
    return l
  })

  // Deduplicate links to/from the aggregate (many externals → same source file)
  const seen = new Set<string>()
  const dedupedLinks = remappedLinks.filter(l => {
    const key = `${l.source}:${l.target}:${l.type}`
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })

  return { nodes: [...nonExternal, aggNode], links: dedupedLinks }
}
