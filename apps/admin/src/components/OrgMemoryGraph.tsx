import { useEffect, useMemo, useState, useCallback, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Loader2, Share2, X } from 'lucide-react'
import { Link } from 'react-router-dom'
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
import { useGraphChrome } from './graph/useGraphChrome'
import {
  DetailField,
  FocusExitHint,
  FocusToggle,
  GRAPH_BG,
  GraphChipRow,
  GraphChipRows,
  GraphDetailPanel,
  GraphHint,
  GraphSearchBox,
  GraphSelect,
  GraphSettings,
  GraphStatsPill,
  GraphTopBar,
  ResetFiltersChip,
  SettingToggle,
  StatSeparator,
  StatValue,
  TypeChip,
  fmt,
  graphRootClass,
} from './graph/chrome'
import { escapeHtml } from '@/lib/utils'
import type { MemoryGraphResponse, Project, ProjectGraphInfo } from '../types'

const ALL_NODE_TYPES = ['Memory', 'Project', 'Session', 'User', 'Collection', 'Tag', 'AuditEvent']
const FALLBACK_PALETTE = [
  '#2997ff', '#34d399', '#fb923c', '#a78bfa',
  '#facc15', '#f472b6', '#22d3ee', '#fb7185',
]
// Matched search hits get the bright highlight fill from the design.
const HIGHLIGHT_COLOR = '#dbeafe'
// Nodes outside an isolated project are dimmed hard (design uses alpha 0.14).
const DIM_COLOR = 'rgba(148,163,184,0.14)'
// Chip text uses dark ink on the yellow Collection pill (design detail).
const DARK_INK_TYPES = new Set(['Collection'])

// Search activates at 2+ chars and caps the reported match count (design).
const MIN_QUERY = 2
const MAX_MATCHES = 500

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
  /** Optional: override the per-page empty title / description. */
  emptyTitle?: string
  emptyDescription?: string
  /** Page chrome (optional — omitted when embedded, e.g. in tests). Rendering
   *  the title + project dropdown here keeps the whole "graph experience"
   *  (incl. the immersive focus overlay) inside one component. */
  title?: string
  subtitle?: string
  projects?: Project[]
  selectedProjectId?: string
  onSelectProject?: (id: string) => void
  projectsLoading?: boolean
  /** Optional graph-source switcher rendered in the top bar's control
   *  cluster, so it fades with the rest of the chrome in focus mode. */
  tabs?: React.ReactNode
  /** Notifies the page when focus mode toggles so it can raise its own
   *  z-index above the app sidebar (child z-index is capped by the parent
   *  stacking context, so the overlay can't outrank the sidebar alone). */
  onFocusedChange?: (focused: boolean) => void
}

/**
 * Family-scoped memory knowledge graph — the full "Graph focus" experience.
 *
 * Faithful to the Graph Focus design:
 *   - Full-bleed 3D graph; every control floats over it on glass surfaces.
 *   - Search (2+ chars) highlights matches, shows a blue match count in the
 *     box, and flies the camera to the first hit.
 *   - Project chips isolate one project (colored border + camera flight);
 *     type chips use the design's smart toggle (isolate-on-first-click when
 *     everything is on, restore-all when the last one is clicked off).
 *   - Focus mode (button / `F` / double-click; `Esc` exits) fades the chrome
 *     out with slide transitions and covers the app shell. Auto-hide enters
 *     focus after 3.5s idle and ANY pointer movement exits it — that pairing
 *     is what keeps search/tags usable (the old auto-focus bug).
 *   - Auto-rotate pauses while dragging or hovering a node.
 *   - Auto-rotate and auto-hide are user-configurable (gear popover,
 *     persisted per browser).
 *
 * The shell (focus mode, auto-hide, glass chrome) lives in `./graph` and is
 * shared with the code graph so both render identically.
 */
export default function OrgMemoryGraph({
  family,
  familyId,
  storageKey,
  emptyTitle = 'No data for this project',
  emptyDescription = 'This project family has no memories, code, or audit events yet.',
  title,
  subtitle,
  projects,
  selectedProjectId,
  onSelectProject,
  projectsLoading = false,
  tabs,
  onFocusedChange,
}: OrgMemoryGraphProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const typesKey = `nexusmind-org-graph-types-${storageKey}`

  const [visibleTypes, setVisibleTypes, resetVisibleTypes] = usePersistedGraphState<string[]>(
    typesKey,
    ALL_NODE_TYPES,
    { validate: Array.isArray },
  )
  const visibleTypeSet = useMemo(() => new Set(visibleTypes), [visibleTypes])

  const [selectedNode, setSelectedNode] = useState<MemForceNode | null>(null)
  const [query, setQuery] = useState('')
  const [focusProj, setFocusProj] = useState<string | null>(null)
  const [hoveredNode, setHoveredNode] = useState(false)

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

  const clearSelection = useCallback(() => setSelectedNode(null), [])

  const {
    containerRef,
    fgRef,
    size,
    focused,
    toggleFocus,
    autoRotate,
    setAutoRotate,
    autoHide,
    setAutoHide,
    settingsOpen,
    setSettingsOpen,
    flyTo,
    flyHome,
  } = useGraphChrome({
    storageKey,
    hasSelection: selectedNode != null,
    clearSelection,
    hoveredNode,
    graphReady: !!graph && graph.node_count > 0,
  })

  useEffect(() => { onFocusedChange?.(focused) }, [focused, onFocusedChange])

  // Clear detail panel + isolation + search when switching projects
  useEffect(() => { setSelectedNode(null); setFocusProj(null); setQuery('') }, [familyId])

  const { mergedNodes, mergedLinks } = useMemo(() => {
    const data = graph as MemoryGraphResponse | undefined
    if (!data) return { mergedNodes: [] as MemForceNode[], mergedLinks: [] as MemForceLink[] }
    const { nodes, links } = mapMemGraphData(data)
    return { mergedNodes: nodes, mergedLinks: links }
  }, [graph])

  const nodeOwningProjectId = useMemo(() => {
    const map = new Map<string, string>()
    if (!graph) return map
    for (const n of graph.nodes) {
      if (n.type === 'Project') {
        const id = n.id.startsWith('project:') ? n.id.slice('project:'.length) : n.id
        if (projectIdToColor.has(id)) map.set(n.id, id)
      }
    }
    for (const e of graph.edges) {
      if (e.type === 'belongs_to' && e.to_id.startsWith('project:')) {
        const pid = e.to_id.slice('project:'.length)
        if (projectIdToColor.has(pid)) map.set(e.from_id, pid)
      }
    }
    return map
  }, [graph, projectIdToColor])

  // Type filter removes nodes; search highlights matches; project isolation
  // dims non-members.
  const graphData = useMemo(() => {
    const filteredNodes = mergedNodes.filter(n => visibleTypeSet.has(n.type))
    const nodeIds = new Set(filteredNodes.map(n => n.id))
    const filteredLinks = filterMemLinksByNodes(mergedLinks, nodeIds)
    return { nodes: filteredNodes, links: filteredLinks }
  }, [mergedNodes, mergedLinks, visibleTypeSet])

  const normalizedQuery = query.trim().toLowerCase()
  const queryActive = normalizedQuery.length >= MIN_QUERY

  // Matching node ids for the active query (capped like the design).
  const matchInfo = useMemo(() => {
    if (!queryActive) return { ids: new Set<string>(), count: 0, first: null as MemForceNode | null }
    const ids = new Set<string>()
    let first: MemForceNode | null = null
    for (const n of graphData.nodes) {
      if ((n.label ?? '').toLowerCase().includes(normalizedQuery)) {
        ids.add(n.id)
        if (!first) first = n
        if (ids.size >= MAX_MATCHES) break
      }
    }
    return { ids, count: ids.size, first }
  }, [queryActive, normalizedQuery, graphData])

  // Fly to the first search match once matches settle (design: dist 620).
  const lastFlownRef = useRef<string | null>(null)
  useEffect(() => {
    if (!queryActive) {
      if (lastFlownRef.current !== null) {
        lastFlownRef.current = null
        if (!focusProj) flyHome()
      }
      return
    }
    const first = matchInfo.first as (MemForceNode & { x?: number; y?: number; z?: number }) | null
    if (!first || lastFlownRef.current === first.id) return
    if (typeof first.x !== 'number') return
    lastFlownRef.current = first.id
    flyTo(first.x, first.y!, first.z!, 620)
  }, [queryActive, matchInfo, focusProj, flyTo, flyHome])

  const handleTypeToggle = useCallback((type: string) => {
    setVisibleTypes(prev => {
      const allOn = prev.length === ALL_NODE_TYPES.length
      const isOn = prev.includes(type)
      // Design behavior: from "everything on", clicking a chip ISOLATES it;
      // clicking the last remaining chip restores everything.
      if (allOn) return [type]
      if (isOn && prev.length === 1) return ALL_NODE_TYPES
      return isOn ? prev.filter(t => t !== type) : [...prev, type]
    })
  }, [setVisibleTypes])

  const handleResetFilters = useCallback(() => {
    resetVisibleTypes()
    setFocusProj(null)
    flyHome()
  }, [resetVisibleTypes, flyHome])

  const handleNodeClick = useCallback((node: object) => {
    setSelectedNode(node as MemForceNode)
  }, [])

  const toggleProjectIsolation = useCallback((id: string) => {
    setFocusProj(prev => {
      const next = prev === id ? null : id
      if (next) {
        // Fly to the centroid of the isolated project's nodes.
        const members = graphData.nodes.filter(
          n => nodeOwningProjectId.get(n.id) === id,
        ) as Array<MemForceNode & { x?: number; y?: number; z?: number }>
        const placed = members.filter(m => typeof m.x === 'number')
        if (placed.length > 0) {
          const cx = placed.reduce((s, m) => s + m.x!, 0) / placed.length
          const cy = placed.reduce((s, m) => s + m.y!, 0) / placed.length
          const cz = placed.reduce((s, m) => s + m.z!, 0) / placed.length
          flyTo(cx, cy, cz, 650)
        }
      } else if (!queryActive) {
        flyHome()
      }
      return next
    })
  }, [graphData, nodeOwningProjectId, queryActive, flyTo, flyHome])

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

  // Tooltip — glass card with "Type · project" line, per the design.
  const nodeLabel = useCallback((node: object) => {
    const n = node as MemForceNode
    const pid = nodeOwningProjectId.get(n.id)
    const projName = pid ? family.find(p => p.id === pid)?.name : null
    const typeLine = projName ? `${n.type} · ${projName}` : n.type
    return `<div style="padding:10px 14px;background:rgba(17,19,25,0.95);border:1px solid rgba(255,255,255,0.12);border-radius:10px;box-shadow:0 10px 34px rgba(0,0,0,0.6);font-family:ui-sans-serif,system-ui;max-width:340px;">
      <div style="font-size:13.5px;font-weight:700;color:#f4f6fa;margin-bottom:2px;">${escapeHtml(n.label)}</div>
      <div style="font-size:12px;color:#98a0b1;margin-bottom:2px;">${escapeHtml(typeLine)}</div>
      <div style="font-size:11.5px;color:#5b6373;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;">${escapeHtml(n.id)}</div>
    </div>`
  }, [nodeOwningProjectId, family])

  const nodeColor = useCallback((node: object) => {
    const n = node as MemForceNode
    // Search matches take the bright highlight fill; everything else keeps
    // its normal color (the design highlights matches, it does not dim the rest).
    if (queryActive && matchInfo.ids.has(n.id)) return HIGHLIGHT_COLOR
    // Project isolation dims everything outside the isolated project.
    if (focusProj && nodeOwningProjectId.get(n.id) !== focusProj) return DIM_COLOR
    if (n.type === 'Memory' || n.type === 'Project') {
      const pid = nodeOwningProjectId.get(n.id)
      if (pid) return projectIdToColor.get(pid) ?? (MEM_NODE_COLORS[n.type] ?? '#94a3b8')
    }
    return MEM_NODE_COLORS[n.type] ?? '#94a3b8'
  }, [queryActive, matchInfo, focusProj, nodeOwningProjectId, projectIdToColor])

  // Node sizes by type (design: project hubs big, users medium, rest small).
  const nodeVal = useCallback((node: object) => {
    const n = node as MemForceNode
    if (queryActive && matchInfo.ids.has(n.id)) return 6
    if (n.type === 'Project') return 18
    if (n.type === 'User') return 4
    return 1.2
  }, [queryActive, matchInfo])

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return MEM_EDGE_COLORS[l.type] ?? '#475569'
  }, [])

  const legendSwatchColor = useCallback((p: ProjectGraphInfo, idx: number) => {
    if (p.color) return p.color
    return FALLBACK_PALETTE[idx % FALLBACK_PALETTE.length]
  }, [])

  const filtersDirty = visibleTypeSet.size !== ALL_NODE_TYPES.length || !!focusProj
  const isInitialLoading = isLoading && !graph
  const isFamilyExpanded = family.length > 1

  const showChrome = !!(title || (projects && onSelectProject))

  // ── Floating top bar: title + tabs + search + project select + settings ────
  const topBar = (
    <GraphTopBar title={title} subtitle={subtitle} focused={focused}>
      {tabs}
      <GraphSearchBox
        value={query}
        onChange={setQuery}
        active={queryActive}
        count={matchInfo.count}
        maxMatches={MAX_MATCHES}
      />
      {projects && onSelectProject && selectedProjectId !== undefined && (
        <GraphSelect
          value={selectedProjectId}
          onChange={onSelectProject}
          disabled={projectsLoading}
          ariaLabel="Select project"
          placeholder="Select a project…"
          options={projects.map(p => ({
            value: p.id,
            label: `${p.name}${p.parent_id ? '  (child)' : ''}`,
          }))}
        />
      )}
      <GraphSettings open={settingsOpen} onOpenChange={setSettingsOpen}>
        <SettingToggle
          label="Auto-rotate"
          description="Slowly orbit the graph"
          checked={autoRotate}
          onChange={setAutoRotate}
        />
        <SettingToggle
          label="Auto-hide UI"
          description="Enter focus after 3.5s idle"
          checked={autoHide}
          onChange={setAutoHide}
        />
      </GraphSettings>
    </GraphTopBar>
  )

  // ── Floating chip rows: project isolation + node-type filters ─────────────
  const chipRows = (
    <GraphChipRows focused={focused} offsetForChrome={showChrome}>
      {/* Per-project chips (click to isolate — active chip takes the project color) */}
      <GraphChipRow role="list" aria-label="Project family legend">
        {family.map((p, idx) => {
          const swatchColor = legendSwatchColor(p, idx)
          const isolated = focusProj === p.id
          return (
            <button
              key={p.id}
              role="listitem"
              type="button"
              onClick={() => toggleProjectIsolation(p.id)}
              className={`flex items-center gap-2 h-[32px] px-[13px] rounded-[16px] border backdrop-blur-[12px] transition-colors cursor-pointer hover:border-white/30 ${
                isolated ? 'bg-white/[0.08]' : 'bg-[#0d0f14]/[0.66]'
              }`}
              style={{ borderColor: isolated ? swatchColor : 'rgba(255,255,255,0.09)' }}
              title={`${p.name} · ${swatchColor}${isolated ? '' : ' — click to isolate'}`}
              aria-pressed={isolated}
            >
              <span
                className="w-[9px] h-[9px] rounded-full shrink-0"
                style={{ backgroundColor: swatchColor }}
                aria-hidden="true"
              />
              <span className={`text-[12.5px] ${isolated ? 'text-[#f2f4f8]' : 'text-[#cfd4de]'}`}>{p.name}</span>
            </button>
          )
        })}
      </GraphChipRow>

      {/* Node-type filter chips — always type-colored, opacity signals state */}
      <GraphChipRow>
        {ALL_NODE_TYPES.map(type => (
          <TypeChip
            key={type}
            type={type}
            color={MEM_NODE_COLORS[type] ?? '#94a3b8'}
            active={visibleTypeSet.has(type)}
            darkInk={DARK_INK_TYPES.has(type)}
            onClick={() => handleTypeToggle(type)}
          />
        ))}
        {filtersDirty && <ResetFiltersChip onClick={handleResetFilters} />}
      </GraphChipRow>
    </GraphChipRows>
  )

  const statsPill = graph && graph.node_count > 0 && (
    <GraphStatsPill focused={focused} offsetForChrome={showChrome}>
      <span><StatValue>{fmt(graphData.nodes.length)}</StatValue> nodes visible</span>
      <StatSeparator />
      <span><StatValue>{fmt(graph.node_count)}</StatValue> total</span>
      {isFamilyExpanded && (
        <>
          <StatSeparator />
          <Link to="/projects" className="text-[#7aa2ff] hover:text-[#a5c0ff] pointer-events-auto">
            {family.length} projects in family
          </Link>
        </>
      )}
    </GraphStatsPill>
  )

  // ── Render states ─────────────────────────────────────────────────────────
  let body: React.ReactNode
  if (isInitialLoading) {
    body = (
      <div className="absolute inset-0 flex items-center justify-center">
        <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
      </div>
    )
  } else if (isError) {
    body = (
      <div className="absolute inset-0 flex items-center justify-center p-6">
        <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
          {(error as Error)?.message ?? 'Failed to load memory graph.'}
        </div>
      </div>
    )
  } else if (!graph || graph.node_count === 0) {
    body = (
      <div className="absolute inset-0 flex items-center justify-center">
        <div className="text-center space-y-2">
          <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
          <p className="text-xs font-semibold text-text-secondary">{emptyTitle}</p>
          <p className="text-xs text-text-quaternary">{emptyDescription}</p>
        </div>
      </div>
    )
  } else if (graphData.nodes.length === 0) {
    body = (
      <div className="absolute inset-0 flex items-center justify-center">
        <div className="text-center space-y-2">
          <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
          <p className="text-xs text-text-quaternary">No nodes match the current filters.</p>
        </div>
      </div>
    )
  } else {
    body = (
      <div
        className="absolute inset-0 cursor-grab active:cursor-grabbing"
        onDoubleClick={toggleFocus}
      >
        <ForceGraph3D
          ref={fgRef as never}
          controlType="orbit"
          width={size.w || undefined}
          height={size.h || undefined}
          graphData={graphData}
          nodeColor={nodeColor}
          nodeVal={nodeVal}
          nodeLabel={nodeLabel}
          onNodeClick={handleNodeClick}
          onNodeHover={(n: object | null) => setHoveredNode(!!n)}
          nodeRelSize={2.6}
          nodeOpacity={0.9}
          linkColor={linkColor}
          linkWidth={0.5}
          linkOpacity={0.3}
          linkDirectionalArrowLength={2.5}
          linkDirectionalArrowRelPos={1}
          backgroundColor={GRAPH_BG}
          showNavInfo={false}
        />
      </div>
    )
  }

  return (
    <div
      ref={containerRef}
      className={graphRootClass(focused)}
      onPointerDown={() => settingsOpen && setSettingsOpen(false)}
    >
      {body}

      {/* Overlays — chrome fades/slides away in focus mode */}
      {showChrome && topBar}
      {graph && graph.node_count > 0 && (
        <>
          {chipRows}
          {statsPill}
          <GraphHint focused={focused} text="hover for info · click a node · drag to rotate" />
          {focused && <FocusExitHint />}
        </>
      )}
      <FocusToggle focused={focused} onToggle={toggleFocus} />

      {/* Node detail sheet — floating rounded glass panel (design spec) */}
      {selectedNode && (
        <GraphDetailPanel>
          <div className="flex items-start gap-3 px-5 pt-[18px] pb-3.5 border-b border-white/[0.06] shrink-0">
            <div className="flex flex-col gap-2 flex-1 min-w-0">
              <h2 className="m-0 text-[16px] font-bold text-[#f4f6fa] leading-[1.35] truncate">
                {selectedNode.type === 'Memory' && memoryDetail?.title
                  ? memoryDetail.title
                  : selectedNode.label}
              </h2>
              <div className="flex items-center gap-1.5 flex-wrap">
                <span
                  className="text-[11.5px] font-semibold px-[11px] py-[3px] rounded-[11px]"
                  style={{
                    backgroundColor: MEM_NODE_COLORS[selectedNode.type] ?? '#94a3b8',
                    color: DARK_INK_TYPES.has(selectedNode.type) ? '#1a1405' : '#ffffff',
                  }}
                >
                  {selectedNode.type}
                </span>
                {(() => {
                  const pid = nodeOwningProjectId.get(selectedNode.id)
                  if (!pid || selectedNode.type === 'Project') return null
                  const swatchColor = projectIdToColor.get(pid)
                  if (!swatchColor) return null
                  const project = family.find(p => p.id === pid)
                  return (
                    <span
                      className="text-[11.5px] font-semibold px-[11px] py-[3px] rounded-[11px] text-[#0b0c10]"
                      style={{ backgroundColor: swatchColor }}
                      title={`Project: ${project?.name ?? pid}`}
                    >
                      {project?.name ?? pid}
                    </span>
                  )
                })()}
              </div>
              <span className="text-[11.5px] text-[#5b6373] font-mono truncate">{selectedNode.id}</span>
            </div>
            <button
              onClick={clearSelection}
              className="shrink-0 w-7 h-7 rounded-[8px] flex items-center justify-center text-[#7c8496] hover:bg-white/[0.06] hover:text-[#e7eaf0] transition-colors"
              aria-label="Close detail panel"
            >
              <X className="w-[15px] h-[15px]" />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto px-5 py-[18px] flex flex-col gap-[18px]">
            {selectedNode.type === 'Memory' && memoryDetailLoading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="w-4 h-4 animate-spin text-text-quaternary" />
              </div>
            ) : (
              <>
                <DetailField label="TYPE" value={memoryDetail?.type ?? selectedNode.type} />
                <DetailField label="LABEL" value={selectedNode.label} />
                {(() => {
                  const pid = nodeOwningProjectId.get(selectedNode.id)
                  const projName = memoryDetail?.project
                    ?? (pid ? family.find(p => p.id === pid)?.name : undefined)
                  return projName ? <DetailField label="PROJECT" value={projName} /> : null
                })()}
                {selectedNode.type === 'Memory' && memoryDetail && (
                  <DetailField label="CREATED" value={new Date(memoryDetail.created_at).toLocaleString()} />
                )}
                {selectedNode.type === 'Memory' && memoryDetail && memoryDetail.tags.length > 0 && (
                  <div className="flex flex-col gap-[7px]">
                    <span className="text-[10.5px] font-bold tracking-[0.1em] text-[#5b6373]">TAGS</span>
                    <div className="flex gap-1.5 flex-wrap">
                      {memoryDetail.tags.map(t => (
                        <span key={t} className="text-[11.5px] px-2.5 py-[3px] rounded-[10px] bg-white/[0.06] text-[#b9c1d0]">
                          {t}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
                {selectedNode.type === 'Memory' && memoryDetail && (
                  <div className="flex flex-col gap-[7px]">
                    <span className="text-[10.5px] font-bold tracking-[0.1em] text-[#5b6373]">CONTENT</span>
                    <div className="px-4 py-3.5 rounded-[12px] border border-white/[0.06] bg-white/[0.02] text-[13px] text-[#b9c1d0] leading-[1.65]">
                      <Markdown content={memoryDetail.content} />
                    </div>
                  </div>
                )}
              </>
            )}
          </div>
        </GraphDetailPanel>
      )}
    </div>
  )
}
