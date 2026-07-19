import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useQueries, useQuery } from '@tanstack/react-query'
import ForceGraph3D, { type ForceGraphMethods } from 'react-force-graph-3d'
import { Maximize2, Minimize2, Search, X, RotateCcw } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { createClient } from '../../api/client'
import {
  mapMemGraphData,
  filterMemLinksByNodes,
  MEM_NODE_COLORS,
  type MemForceNode,
  type MemForceLink,
} from './memoryGraphUtils'
import { usePersistedGraphState } from '../../hooks/usePersistedGraphState'
import { escapeHtml } from '@/lib/utils'

// All possible memory graph node types (same vocabulary as OrgMemoryGraph /
// MemoryGraphTab — kept in sync with the backend's `/v1/memory/graph`).
const ALL_NODE_TYPES = ['Memory', 'Project', 'Session', 'User', 'Collection', 'Tag', 'AuditEvent']
const VISIBLE_TYPES_KEY = 'nexusmind-memories-bg-graph-types'
// How many active projects to merge into the ambient background scene. This
// is a decorative visualization, not the full Graph page, so we deliberately
// cap both project fan-out and per-project node count to stay smooth.
const MAX_PROJECTS = 8
const PER_PROJECT_LIMIT = 250

/**
 * Fixed, full-viewport ambient memory graph that lives behind the Memories
 * page content. Reuses the same data-mapping helpers and node/edge palette
 * as `OrgMemoryGraph`/`MemoryGraphTab` (feeds off the real
 * `GET /v1/memory/graph` endpoint, merged across the org's active projects)
 * instead of re-implementing a bespoke particle engine.
 *
 * Two modes, controlled by the parent (`focused`/`onToggleFocus`):
 *  - Background: low-opacity, `pointer-events: none`, camera auto-rotates
 *    slowly, no raycasting (cheap to keep running behind the page).
 *  - Focus: full-opacity, interactive (drag/zoom/click), legend + search +
 *    a compact node detail card, exit hint, "F" keyboard shortcut, and
 *    double-click-to-exit.
 */
export default function MemoryBackgroundGraph({
  focused,
  onToggleFocus,
}: {
  focused: boolean
  onToggleFocus: () => void
}) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const fgRef = useRef<ForceGraphMethods | undefined>(undefined)

  const [visibleTypes, setVisibleTypes, resetVisibleTypes] = usePersistedGraphState<string[]>(
    VISIBLE_TYPES_KEY,
    ALL_NODE_TYPES,
  )
  const visibleTypeSet = useMemo(() => new Set(visibleTypes), [visibleTypes])

  const [gQuery, setGQuery] = useState('')
  const [selectedNode, setSelectedNode] = useState<MemForceNode | null>(null)
  const [dims, setDims] = useState({ w: window.innerWidth, h: window.innerHeight })

  useEffect(() => {
    const onResize = () => setDims({ w: window.innerWidth, h: window.innerHeight })
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  // Close the detail card / exit focus on Escape; toggle focus on "f".
  // Ignores keystrokes while the user is typing in an input/textarea.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      if (target && /INPUT|TEXTAREA/.test(target.tagName)) return
      if (e.key === 'Escape') {
        if (selectedNode) { setSelectedNode(null); return }
        if (focused) onToggleFocus()
      }
      if ((e.key === 'f' || e.key === 'F') && !e.metaKey && !e.ctrlKey) {
        onToggleFocus()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [focused, selectedNode, onToggleFocus])

  useEffect(() => { setSelectedNode(null) }, [focused])

  // Gentle ambient auto-rotate — paused once the user starts dragging, and
  // fully disabled (no raycasting/controls) while in background mode.
  useEffect(() => {
    const controls = fgRef.current?.controls() as { autoRotate?: boolean; autoRotateSpeed?: number } | undefined
    if (!controls) return
    controls.autoRotate = true
    controls.autoRotateSpeed = focused ? 0.25 : 0.45
  })

  const { data: projects } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    staleTime: 60_000,
  })

  const activeProjects = useMemo(
    () => (projects ?? []).filter(p => !p.archived_at).slice(0, MAX_PROJECTS),
    [projects],
  )

  const graphQueries = useQueries({
    queries: activeProjects.map(p => ({
      queryKey: ['memory-graph-bg', p.name],
      queryFn: () => client.getMemoryGraph(p.name, { limit: PER_PROJECT_LIMIT }),
      enabled: p.name.trim().length > 0,
      retry: false,
      staleTime: 120_000,
    })),
  })

  const { mergedNodes, mergedLinks } = useMemo(() => {
    const nodeMap = new Map<string, MemForceNode>()
    const linkMap = new Map<string, MemForceLink>()
    activeProjects.forEach((_p, i) => {
      const result = graphQueries[i]?.data
      if (!result) return
      const { nodes, links } = mapMemGraphData(result)
      for (const node of nodes) if (!nodeMap.has(node.id)) nodeMap.set(node.id, node)
      for (const link of links) {
        const srcId = String((link.source as unknown as { id?: string })?.id ?? link.source)
        const tgtId = String((link.target as unknown as { id?: string })?.id ?? link.target)
        const key = `${srcId}|${link.type}|${tgtId}`
        if (!linkMap.has(key)) linkMap.set(key, link)
      }
    })
    return { mergedNodes: Array.from(nodeMap.values()), mergedLinks: Array.from(linkMap.values()) }
  }, [activeProjects, graphQueries])

  const graphData = useMemo(() => {
    const filteredNodes = mergedNodes.filter(n => visibleTypeSet.has(n.type))
    const nodeIds = new Set(filteredNodes.map(n => n.id))
    const filteredLinks = filterMemLinksByNodes(mergedLinks, nodeIds)
    return { nodes: filteredNodes, links: filteredLinks }
  }, [mergedNodes, mergedLinks, visibleTypeSet])

  const matchCount = useMemo(() => {
    const q = gQuery.trim().toLowerCase()
    if (q.length < 2) return 0
    return graphData.nodes.filter(n => n.label.toLowerCase().includes(q)).length
  }, [gQuery, graphData.nodes])

  const nodeColor = useCallback((node: object) => {
    const n = node as MemForceNode
    const base = MEM_NODE_COLORS[n.type] ?? '#94a3b8'
    const q = gQuery.trim().toLowerCase()
    if (q.length >= 2 && !n.label.toLowerCase().includes(q)) return 'rgba(148,163,184,0.15)'
    return base
  }, [gQuery])

  const nodeLabel = useCallback((node: object) => {
    const n = node as MemForceNode
    const color = MEM_NODE_COLORS[n.type] ?? '#94a3b8'
    return `<div style="padding:10px 14px;border-radius:10px;border:1px solid rgba(255,255,255,0.12);background:rgba(17,19,25,0.95);font-family:ui-sans-serif,system-ui;max-width:320px;box-shadow:0 10px 34px rgba(0,0,0,0.6);">
      <div style="font-size:13.5px;font-weight:700;color:#f4f6fa;margin-bottom:2px;">${escapeHtml(n.label)}</div>
      <div style="font-size:12px;color:${color};">${n.type === 'Tag' ? 'Shared tag' : 'Memory · ' + escapeHtml(n.type)}</div>
    </div>`
  }, [])

  const handleNodeClick = useCallback((node: object) => setSelectedNode(node as MemForceNode), [])

  const memoryUUID = useMemo(() => {
    if (!selectedNode || selectedNode.type !== 'Memory') return null
    const { id } = selectedNode
    return id.startsWith('memory:') ? id.slice('memory:'.length) : null
  }, [selectedNode])

  const { data: memoryDetail } = useQuery({
    queryKey: ['memory-detail', memoryUUID],
    queryFn: () => client.getMemory(memoryUUID!),
    enabled: memoryUUID != null,
    staleTime: 300_000,
  })

  const isFiltered = visibleTypeSet.size !== ALL_NODE_TYPES.length

  // Exit focus mode on double-click. The scene container only receives
  // pointer events while `focused` (see `pointerEvents` below), so this
  // handler is effectively a no-op in background mode without needing an
  // extra `focused` check.
  const handleDoubleClick = useCallback(() => {
    if (focused) onToggleFocus()
  }, [focused, onToggleFocus])

  return (
    <>
      {/* Background/foreground scene */}
      <div
        aria-hidden={!focused || undefined}
        className="fixed inset-0 z-0 transition-opacity duration-500"
        style={{ opacity: focused ? 1 : 0.35, pointerEvents: focused ? 'auto' : 'none' }}
        onDoubleClick={handleDoubleClick}
      >
        {graphData.nodes.length > 0 && (
          <ForceGraph3D
            ref={fgRef}
            width={dims.w}
            height={dims.h}
            graphData={graphData}
            nodeColor={nodeColor}
            nodeLabel={nodeLabel}
            onNodeClick={handleNodeClick}
            nodeRelSize={3.2}
            nodeOpacity={0.9}
            linkColor={() => 'rgba(148,163,184,0.10)'}
            linkWidth={0.5}
            linkOpacity={0.35}
            backgroundColor="rgba(0,0,0,0)"
            showNavInfo={false}
            enableNodeDrag={focused}
            enableNavigationControls={focused}
            enablePointerInteraction={focused}
          />
        )}
      </div>

      {/* Focus toggle */}
      <button
        type="button"
        onClick={onToggleFocus}
        title="Shortcut: F"
        aria-pressed={focused}
        className="fixed right-6 bottom-6 z-[55] flex items-center gap-2 h-[42px] px-4 rounded-[11px] border border-border-primary bg-surface-glass backdrop-blur-md hover:border-white/[0.28] transition-colors shadow-[0_8px_30px_rgba(0,0,0,0.45)]"
      >
        {focused ? <Minimize2 className="w-4 h-4 text-text-primary" /> : <Maximize2 className="w-4 h-4 text-text-primary" />}
        <span className="text-[13.5px] font-semibold text-text-primary">{focused ? 'Exit focus' : 'Focus'}</span>
        <span className="text-[11px] text-text-quaternary border border-border-primary rounded-[5px] px-1.5 py-px">F</span>
      </button>

      {focused && (
        <>
          {/* Exit hint */}
          <div className="fixed left-1/2 bottom-6 -translate-x-1/2 z-[54] h-[34px] flex items-center px-[18px] rounded-full border border-white/[0.08] bg-black/[0.4] backdrop-blur-md text-[12.5px] text-text-secondary pointer-events-none">
            Focus mode — memories and their relations · press <span className="text-text-primary font-semibold mx-1">F</span> or double-click to exit
          </div>

          {/* Legend + search header */}
          <div className="fixed left-4 lg:left-[232px] right-6 top-6 z-[54] flex flex-col gap-2.5">
            <div className="flex items-center justify-between gap-4 flex-wrap pr-2">
              <span className="text-[15px] font-extrabold tracking-tight text-text-primary" style={{ textShadow: '0 2px 14px rgba(0,0,0,0.8)' }}>
                Memory graph
              </span>
              <div className="flex items-center gap-2 h-10 w-[280px] max-w-full px-3.5 rounded-[11px] border border-border-primary bg-surface-glass backdrop-blur-md">
                <Search className="w-3.5 h-3.5 text-text-quaternary shrink-0" />
                <input
                  type="text"
                  value={gQuery}
                  onChange={e => setGQuery(e.target.value)}
                  placeholder="Search memories…"
                  className="flex-1 min-w-0 bg-transparent border-none outline-none text-text-primary text-xs"
                />
                {gQuery.trim().length >= 2 && (
                  <span className="shrink-0 text-[11px] font-bold" style={{ color: '#7aa2ff' }}>{matchCount}</span>
                )}
              </div>
            </div>
            <div className="flex gap-2 flex-wrap items-center">
              {ALL_NODE_TYPES.map(type => {
                const active = visibleTypeSet.has(type)
                const color = MEM_NODE_COLORS[type] ?? '#94a3b8'
                return (
                  <button
                    key={type}
                    type="button"
                    onClick={() => setVisibleTypes(prev => prev.includes(type) ? prev.filter(t => t !== type) : [...prev, type])}
                    aria-pressed={active}
                    className="flex items-center gap-1.5 h-7 px-3 rounded-full border backdrop-blur-md transition-colors"
                    style={{
                      borderColor: active ? 'rgba(255,255,255,0.14)' : 'rgba(255,255,255,0.08)',
                      background: active ? 'rgba(13,15,20,0.66)' : 'rgba(13,15,20,0.4)',
                    }}
                  >
                    <span className="w-2 h-2 rounded-full" style={{ background: color, opacity: active ? 1 : 0.35 }} />
                    <span className="text-xs" style={{ color: active ? '#e7eaf0' : '#6b7384' }}>{type}</span>
                  </button>
                )
              })}
              {isFiltered && (
                <button
                  type="button"
                  onClick={resetVisibleTypes}
                  className="flex items-center gap-1.5 h-7 px-3 rounded-full border border-border-primary bg-surface-glass text-text-secondary hover:text-text-primary transition-colors text-xs"
                >
                  <RotateCcw className="w-3 h-3" />
                  Reset
                </button>
              )}
            </div>
          </div>

          {/* Node detail card */}
          {selectedNode && (
            <div className="fixed right-6 top-[88px] z-[56] w-[340px] max-w-[calc(100vw-48px)] rounded-[14px] border border-border-primary bg-[#0f1117]/95 backdrop-blur-xl shadow-[0_16px_50px_rgba(0,0,0,0.55)] p-4 flex flex-col gap-2.5">
              <div className="flex items-start gap-2.5">
                <span
                  className="w-2.5 h-2.5 rounded-full shrink-0 mt-1"
                  style={{ background: MEM_NODE_COLORS[selectedNode.type] ?? '#94a3b8' }}
                />
                <span className="flex-1 min-w-0 text-sm font-bold text-text-primary leading-snug">
                  {selectedNode.type === 'Memory' && memoryDetail?.title ? memoryDetail.title : selectedNode.label}
                </span>
                <button
                  type="button"
                  onClick={() => setSelectedNode(null)}
                  aria-label="Close node detail"
                  className="shrink-0 w-6 h-6 rounded-[7px] flex items-center justify-center text-text-tertiary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              </div>
              <span className="self-start text-[11.5px] font-semibold px-2.5 py-0.5 rounded-full bg-white/[0.07] text-[#cfd4de]">
                {selectedNode.type}
              </span>
              {selectedNode.type === 'Memory' && memoryDetail && (
                <p className="text-xs text-text-tertiary leading-relaxed line-clamp-4">
                  {memoryDetail.content}
                </p>
              )}
            </div>
          )}
        </>
      )}
    </>
  )
}
