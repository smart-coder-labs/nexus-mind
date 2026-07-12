import { useEffect, useMemo, useState, useCallback } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Loader2, RotateCcw, Share2, X } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { Markdown } from './ui/Markdown'
import ForceGraph3D from 'react-force-graph-3d'
import {
  mapMemGraphData,
  filterMemLinksByNodes,
  MEM_NODE_COLORS,
  MEM_EDGE_COLORS,
  type MemForceNode,
  type MemForceLink,
} from '../pages/memories/memoryGraphUtils'
import { usePersistedGraphState } from '../hooks/usePersistedGraphState'
import { escapeHtml } from '@/lib/utils'
import type { MemoryGraphResponse, ProjectGraphInfo } from '../types'

const ALL_NODE_TYPES = ['Memory', 'Project', 'Session', 'User', 'Collection', 'Tag', 'AuditEvent']
const FALLBACK_PALETTE = [
  '#2997ff', '#34d399', '#fb923c', '#a78bfa',
  '#facc15', '#f472b6', '#22d3ee', '#fb7185',
]

interface OrgMemoryGraphProps {
  /** The resolved project family — drives the legend, the per-node colors,
   *  and the API call (via `familyId`). Caller (the Graph page) computes the
   *  family via a BFS walk over `parent_id` so this component stays pure. */
  family: ProjectGraphInfo[]
  /** The root project id used in the `project_id=` query param. */
  familyId: string
  /** localStorage key suffix for the per-page filter state. */
  storageKey: string
  height?: number
  /** Optional: override the per-page header. Defaults to a generic
   *  "Knowledge graph" label. */
  emptyTitle?: string
  emptyDescription?: string
}

/**
 * Family-scoped memory knowledge graph. The caller (the new Graph page)
 * resolves a root project to its full family (parent + descendants via
 * `parent_id`) and passes the result here. This component then:
 *
 *   1. Fetches `GET /v1/memory/graph?project_id=<root>` once. The backend
 *      returns the merged graph for the family plus a `projects` array with
 *      stable per-project colors.
 *   2. Renders a legend at the top: one swatch per project in the family,
 *      using the colors the backend shipped.
 *   3. Colors Memory/Project nodes by their owning project's color.
 *   4. Persists filter state (hidden types) across reloads via the shared
 *      `usePersistedGraphState` hook.
 */
export default function OrgMemoryGraph({
  family,
  familyId,
  storageKey,
  height = 600,
  emptyTitle = 'No data for this project',
  emptyDescription = 'This project family has no memories, code, or audit events yet.',
}: OrgMemoryGraphProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const typesKey = `nexusmind-org-graph-types-${storageKey}`

  // Persist node-type filter state across reloads. Storage format stays as
  // a plain JSON array of strings for back-compat with the previous manual
  // implementation and the OrgMemoryGraph tests.
  const [visibleTypes, setVisibleTypes, resetVisibleTypes] = usePersistedGraphState<string[]>(
    typesKey,
    ALL_NODE_TYPES,
  )

  const visibleTypeSet = useMemo(() => new Set(visibleTypes), [visibleTypes])

  const [selectedNode, setSelectedNode] = useState<MemForceNode | null>(null)

  const projectIdToColor = useMemo(() => {
    const m = new Map<string, string>()
    family.forEach(p => m.set(p.id, p.color))
    return m
  }, [family])

  const { data: graph, isLoading, isError, error } = useQuery({
    queryKey: ['memory-graph-family', familyId],
    queryFn: () => client.getMemoryGraphForFamily(familyId),
    enabled: familyId.trim().length > 0,
    retry: false,
    staleTime: 120_000,
  })

  // Clear detail panel when switching projects
  useEffect(() => { setSelectedNode(null) }, [familyId])

  // Map nodes to (color-by-project) using the family palette. Memory nodes
  // carry the `project:UUID` reference (via their `belongs_to` edge target
  // or their `project_id` field on the underlying memory); Project nodes
  // match their own id.
  const { mergedNodes, mergedLinks } = useMemo(() => {
    const data = graph as MemoryGraphResponse | undefined
    if (!data) return { mergedNodes: [] as MemForceNode[], mergedLinks: [] as MemForceLink[] }
    const { nodes, links } = mapMemGraphData(data)
    return { mergedNodes: nodes, mergedLinks: links }
  }, [graph])

  // Color for a memory node = the color of its project. We resolve it
  // client-side from the `belongs_to` edge target (`project:UUID`) or, for
  // project nodes, directly from the id.
  const nodeOwningProjectId = useMemo(() => {
    const map = new Map<string, string>()
    if (!graph) return map
    for (const n of graph.nodes) {
      if (n.type === 'Project') {
        // node id format: "project:UUID"
        const id = n.id.startsWith('project:') ? n.id.slice('project:'.length) : n.id
        if (projectIdToColor.has(id)) map.set(n.id, id)
      }
    }
    for (const e of graph.edges) {
      if (e.type === 'belongs_to' && e.to_id.startsWith('project:')) {
        const pid = e.to_id.slice('project:'.length)
        if (projectIdToColor.has(pid)) {
          map.set(e.from_id, pid)
        }
      }
    }
    return map
  }, [graph, projectIdToColor])

  // Apply type filter only (project visibility is no longer a thing — the
  // whole family is always visible).
  const graphData = useMemo(() => {
    const filteredNodes = mergedNodes.filter(n => visibleTypeSet.has(n.type))
    const nodeIds = new Set(filteredNodes.map(n => n.id))
    const filteredLinks = filterMemLinksByNodes(mergedLinks, nodeIds)
    return { nodes: filteredNodes, links: filteredLinks }
  }, [mergedNodes, mergedLinks, visibleTypeSet])

  const handleTypeToggle = useCallback((type: string) => {
    setVisibleTypes(prev =>
      prev.includes(type) ? prev.filter(t => t !== type) : [...prev, type],
    )
  }, [setVisibleTypes])

  const handleResetFilters = useCallback(() => {
    resetVisibleTypes()
  }, [resetVisibleTypes])

  const handleNodeClick = useCallback((node: object) => {
    setSelectedNode(node as MemForceNode)
  }, [])

  // Extract UUID from namespaced memory id ("memory:uuid") for detail fetch
  const memoryUUID = useMemo(() => {
    if (!selectedNode || selectedNode.type !== 'Memory') return null
    const { id } = selectedNode
    return id.startsWith('memory:') ? id.slice('memory:'.length) : null
  }, [selectedNode])

  const { data: memoryDetail, isLoading: memoryDetailLoading } = useQuery({
    queryKey: ['memory-detail', memoryUUID],
    queryFn: () => client.getMemory(memoryUUID!),
    enabled: memoryUUID != null,
    staleTime: 300_000,
  })

  const nodeLabel = useCallback((node: object) => {
    const n = node as MemForceNode
    const color = MEM_NODE_COLORS[n.type] ?? '#94a3b8'
    return `<div style="padding:6px 9px;background:#16161a;border:1px solid #2a2a2e;border-radius:8px;font-family:ui-sans-serif,system-ui;max-width:320px;">
      <div style="font-size:12px;font-weight:600;color:#e5e7eb;">${escapeHtml(n.label)}</div>
      <div style="font-size:10px;color:${color};margin-top:1px;">${escapeHtml(n.type)}</div>
      <div style="font-size:10px;color:#9ca3af;margin-top:3px;font-family:ui-monospace,monospace;">${escapeHtml(n.id)}</div>
    </div>`
  }, [])

  // Per-node color: project color for Memory/Project nodes, type color
  // otherwise. Falls back to a stable palette color for nodes whose owning
  // project we couldn't resolve.
  const nodeColor = useCallback((node: object) => {
    const n = node as MemForceNode
    if (n.type === 'Memory' || n.type === 'Project') {
      const pid = nodeOwningProjectId.get(n.id)
      if (pid) return projectIdToColor.get(pid) ?? (MEM_NODE_COLORS[n.type] ?? '#94a3b8')
    }
    return MEM_NODE_COLORS[n.type] ?? '#94a3b8'
  }, [nodeOwningProjectId, projectIdToColor])

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return MEM_EDGE_COLORS[l.type] ?? '#475569'
  }, [])

  // Legend swatches in family order. Falls back to a stable palette color
  // if the backend didn't ship one (shouldn't happen, but defensive — keeps
  // every swatch distinct).
  const legendSwatchColor = useCallback((p: ProjectGraphInfo, idx: number) => {
    if (p.color) return p.color
    return FALLBACK_PALETTE[idx % FALLBACK_PALETTE.length]
  }, [])

  const visibleTypesCount = visibleTypeSet.size
  const isInitialLoading = isLoading && !graph
  const isFamilyExpanded = family.length > 1

  if (isInitialLoading) {
    return (
      <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
        <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
      </div>
    )
  }

  if (isError) {
    return (
      <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
        {(error as Error)?.message ?? 'Failed to load memory graph.'}
      </div>
    )
  }

  if (!graph || graph.node_count === 0) {
    return (
      <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
        <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
        <p className="text-xs font-semibold text-text-secondary">{emptyTitle}</p>
        <p className="text-xs text-text-quaternary">{emptyDescription}</p>
      </div>
    )
  }

  return (
    <div className="space-y-3">
      {/* Legend + filter pills */}
      <div className="flex items-start gap-3 flex-wrap">
        {/* Per-project legend swatches */}
        <div
          className="flex items-center gap-1.5 flex-wrap"
          role="list"
          aria-label="Project family legend"
        >
          {family.map((p, idx) => {
            const swatchColor = legendSwatchColor(p, idx)
            return (
              <div
                key={p.id}
                role="listitem"
                className="flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-white/[0.04] border border-border-primary"
                title={`${p.name} · ${swatchColor}`}
              >
                <span
                  className="w-2.5 h-2.5 rounded-full shrink-0"
                  style={{ backgroundColor: swatchColor, boxShadow: `0 0 6px ${swatchColor}40` }}
                  aria-hidden="true"
                />
                <span className="text-[10px] text-text-secondary">{p.name}</span>
              </div>
            )
          })}
        </div>

        {/* Node type filter pills */}
        <div className="flex items-center gap-1.5 flex-wrap">
          {ALL_NODE_TYPES.map(type => (
            <button
              key={type}
              type="button"
              onClick={() => handleTypeToggle(type)}
              className={`flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] border transition-colors cursor-pointer ${
                visibleTypeSet.has(type)
                  ? 'border-transparent text-white opacity-100'
                  : 'border-border-primary text-text-quaternary opacity-50'
              }`}
              style={visibleTypeSet.has(type) ? { backgroundColor: MEM_NODE_COLORS[type] ?? '#94a3b8' } : undefined}
              aria-pressed={visibleTypeSet.has(type)}
              aria-label={`Toggle ${type} nodes`}
            >
              {type}
            </button>
          ))}
        </div>

        {/* Reset filters */}
        {visibleTypesCount !== ALL_NODE_TYPES.length && (
          <button
            type="button"
            onClick={handleResetFilters}
            className="flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] border border-border-primary text-text-quaternary hover:text-text-secondary transition-colors"
            aria-label="Reset graph filters"
          >
            <RotateCcw className="w-2.5 h-2.5" />
            Reset filters
          </button>
        )}
      </div>

      {/* Graph scene */}
      <div className="relative border border-border-primary rounded-[18px] overflow-hidden" style={{ height }}>
        {/* Stats bar */}
        <div className="flex items-center gap-3 px-4 py-2 border-b border-border-primary bg-white/[0.02] text-[10px] text-text-quaternary">
          <span>{graphData.nodes.length} nodes visible</span>
          <span>·</span>
          <span>{graphData.links.length} edges visible</span>
          <span>·</span>
          <span>{graph.node_count} total nodes</span>
          <span>·</span>
          <span>{graph.edge_count} total edges</span>
          {isFamilyExpanded && (
            <>
              <span>·</span>
              <span className="text-accent-blue/70">
                {family.length} projects in family
              </span>
            </>
          )}
          <span className="ml-auto text-text-quaternary/70">hover for info · click a node · drag to rotate</span>
        </div>

        {graphData.nodes.length === 0 ? (
          <div className="flex items-center justify-center" style={{ height: height - 36 }}>
            <div className="text-center space-y-2">
              <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
              <p className="text-xs text-text-quaternary">No nodes match the current filters.</p>
            </div>
          </div>
        ) : (
          <ForceGraph3D
            graphData={graphData}
            nodeColor={nodeColor}
            nodeLabel={nodeLabel}
            onNodeClick={handleNodeClick}
            nodeRelSize={4}
            nodeOpacity={0.9}
            linkColor={linkColor}
            linkWidth={0.5}
            linkOpacity={0.5}
            linkDirectionalArrowLength={2.5}
            linkDirectionalArrowRelPos={1}
            backgroundColor="#111113"
            showNavInfo={false}
          />
        )}

        {/* Detail panel — slides in over the right side when a node is selected */}
        {selectedNode && (
          <div className="absolute top-0 right-0 h-full w-[380px] max-w-[70%] bg-[#16161a]/95 border-l border-border-primary backdrop-blur-sm flex flex-col overflow-hidden">
            {/* Panel header */}
            <div className="flex items-start gap-2 px-4 py-3 border-b border-border-primary shrink-0">
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold text-text-primary truncate">
                  {selectedNode.type === 'Memory' && memoryDetail?.title
                    ? memoryDetail.title
                    : selectedNode.label}
                </p>
                <div className="flex items-center gap-2 mt-0.5">
                  <span
                    className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full text-white"
                    style={{ backgroundColor: MEM_NODE_COLORS[selectedNode.type] ?? '#94a3b8' }}
                  >
                    {selectedNode.type}
                  </span>
                  {(() => {
                    const pid = nodeOwningProjectId.get(selectedNode.id)
                    if (!pid) return null
                    const swatchColor = projectIdToColor.get(pid)
                    if (!swatchColor) return null
                    const project = family.find(p => p.id === pid)
                    return (
                      <span
                        className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full text-white"
                        style={{ backgroundColor: swatchColor }}
                        title={`Project: ${project?.name ?? pid}`}
                      >
                        {project?.name ?? pid}
                      </span>
                    )
                  })()}
                </div>
                <p className="text-[10px] text-text-quaternary font-mono mt-1 break-all">
                  {selectedNode.id}
                </p>
              </div>
              <button
                onClick={() => setSelectedNode(null)}
                className="text-text-quaternary hover:text-text-primary transition-colors shrink-0 mt-0.5"
                aria-label="Close detail panel"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* Panel body */}
            <div className="flex-1 overflow-auto p-4">
              {selectedNode.type === 'Memory' ? (
                memoryDetailLoading ? (
                  <div className="flex items-center justify-center py-8">
                    <Loader2 className="w-4 h-4 animate-spin text-text-quaternary" />
                  </div>
                ) : memoryDetail ? (
                  <div className="space-y-4">
                    {memoryDetail.type && (
                      <div>
                        <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Type</p>
                        <span className="text-[10px] font-medium px-2 py-0.5 rounded-full bg-white/[0.06] text-text-secondary">
                          {memoryDetail.type}
                        </span>
                      </div>
                    )}
                    {memoryDetail.tags.length > 0 && (
                      <div>
                        <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Tags</p>
                        <div className="flex flex-wrap gap-1">
                          {memoryDetail.tags.map(t => (
                            <span key={t} className="text-[10px] px-1.5 py-0.5 rounded-full bg-white/[0.06] text-text-tertiary">
                              {t}
                            </span>
                          ))}
                        </div>
                      </div>
                    )}
                    <div>
                      <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Project</p>
                      <p className="text-xs text-text-secondary">{memoryDetail.project}</p>
                    </div>
                    {memoryDetail.session_id && (
                      <div>
                        <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Session</p>
                        <p className="text-[10px] text-text-tertiary font-mono break-all">{memoryDetail.session_id}</p>
                      </div>
                    )}
                    <div>
                      <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Created</p>
                      <p className="text-xs text-text-secondary">
                        {new Date(memoryDetail.created_at).toLocaleString()}
                      </p>
                    </div>
                    <div>
                      <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-2">Content</p>
                      <Markdown content={memoryDetail.content} />
                    </div>
                  </div>
                ) : (
                  <p className="text-xs text-text-quaternary">Failed to load memory details.</p>
                )
              ) : (
                <div className="space-y-3">
                  <div>
                    <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Type</p>
                    <p className="text-xs text-text-secondary">{selectedNode.type}</p>
                  </div>
                  <div>
                    <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Label</p>
                    <p className="text-xs text-text-secondary">{selectedNode.label}</p>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
