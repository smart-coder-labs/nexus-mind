import { useEffect, useMemo, useState, useCallback, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ChevronDown, Loader2, RotateCcw, Search, Settings2, Share2, X } from 'lucide-react'
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

// Shared glass-surface styling for every floating control (design spec:
// rgba(13,15,20,0.72) + blur 14).
const GLASS = 'border border-white/[0.09] bg-[#0d0f14]/[0.72] backdrop-blur-[14px]'
const GLASS_SOFT = 'border border-white/[0.08] bg-[#0d0f14]/[0.66] backdrop-blur-[12px]'

// Keyboard focus indicator (matches the rest of the admin app).
const FOCUS_RING = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

// Idle time before auto-hide kicks in (design: 3.5s).
const AUTO_HIDE_MS = 3500
// Search activates at 2+ chars and caps the reported match count (design).
const MIN_QUERY = 2
const MAX_MATCHES = 500

const fmt = (n: number) => n.toLocaleString('en-US')

interface FgInstance {
  controls?: () => { autoRotate?: boolean; autoRotateSpeed?: number }
  cameraPosition?: (
    pos: { x?: number; y?: number; z?: number },
    lookAt?: { x: number; y: number; z: number },
    ms?: number,
  ) => void
  zoomToFit?: (ms?: number, padding?: number) => void
}

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
  onFocusedChange,
}: OrgMemoryGraphProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const typesKey = `nexusmind-org-graph-types-${storageKey}`

  const [visibleTypes, setVisibleTypes, resetVisibleTypes] = usePersistedGraphState<string[]>(
    typesKey,
    ALL_NODE_TYPES,
  )
  const visibleTypeSet = useMemo(() => new Set(visibleTypes), [visibleTypes])

  // User-configurable behavior (design props `autoRotate` / `autoHide`,
  // both default true). Persisted like the rest of the graph state.
  const [autoRotate, setAutoRotate] = usePersistedGraphState<boolean>(
    `nexusmind-graph-auto-rotate-${storageKey}`, true,
  )
  const [autoHide, setAutoHide] = usePersistedGraphState<boolean>(
    `nexusmind-graph-auto-hide-${storageKey}`, true,
  )
  const [settingsOpen, setSettingsOpen] = useState(false)

  const [selectedNode, setSelectedNode] = useState<MemForceNode | null>(null)
  const [focused, setFocused] = useState(false)
  const [query, setQuery] = useState('')
  const [focusProj, setFocusProj] = useState<string | null>(null)
  const [hoveredNode, setHoveredNode] = useState(false)

  // Whether the current focus state was entered automatically (idle) — those
  // exits on any pointer movement; manual focus does not.
  const autoFocusedRef = useRef(false)
  const idleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const containerRef = useRef<HTMLDivElement>(null)
  const fgRef = useRef<FgInstance | null>(null)
  const [size, setSize] = useState({ w: 0, h: 0 })

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

  // Clear detail panel + isolation + search when switching projects
  useEffect(() => { setSelectedNode(null); setFocusProj(null); setQuery('') }, [familyId])

  // Measure the container so the 3D graph fills it exactly in both modes.
  // Guarded for environments without ResizeObserver (jsdom in tests).
  useEffect(() => {
    const el = containerRef.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(entries => {
      const r = entries[0]?.contentRect
      if (r) setSize({ w: Math.floor(r.width), h: Math.floor(r.height) })
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // Keyboard: F toggles focus (ignored while typing in a field), Esc closes
  // the detail panel first, then exits focus — same order as the design.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null
      const typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)
      if (e.key === 'Escape') {
        if (selectedNode) { setSelectedNode(null); return }
        if (focused) { autoFocusedRef.current = false; setFocused(false) }
        return
      }
      if ((e.key === 'f' || e.key === 'F') && !typing) {
        e.preventDefault()
        autoFocusedRef.current = false
        setFocused(f => !f)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [focused, selectedNode])

  // Live mirrors of state for the idle timer (avoids stale closures without
  // re-registering the listener on every state change).
  const focusedRef = useRef(focused)
  const selectedRef = useRef(selectedNode)
  useEffect(() => { focusedRef.current = focused }, [focused])
  useEffect(() => { selectedRef.current = selectedNode }, [selectedNode])
  useEffect(() => { onFocusedChange?.(focused) }, [focused, onFocusedChange])

  // Auto-hide: after 3.5s of pointer inactivity (and nothing selected) enter
  // focus automatically; ANY pointer movement exits an auto-entered focus.
  useEffect(() => {
    if (!autoHide) return
    const onMove = () => {
      if (autoFocusedRef.current) {
        autoFocusedRef.current = false
        setFocused(false)
      }
      if (idleTimerRef.current) clearTimeout(idleTimerRef.current)
      idleTimerRef.current = setTimeout(() => {
        if (!focusedRef.current && !selectedRef.current) {
          autoFocusedRef.current = true
          setFocused(true)
        }
      }, AUTO_HIDE_MS)
    }
    window.addEventListener('pointermove', onMove)
    onMove()
    return () => {
      window.removeEventListener('pointermove', onMove)
      if (idleTimerRef.current) clearTimeout(idleTimerRef.current)
    }
  }, [autoHide])

  // Auto-rotate (OrbitControls via controlType="orbit"). Pauses while a node
  // is hovered — the design pauses rotation on hover so tooltips stay put.
  useEffect(() => {
    if (!graph || graph.node_count === 0) return
    let raf = 0
    let tries = 0
    const apply = () => {
      const controls = fgRef.current?.controls?.()
      if (controls) {
        controls.autoRotate = autoRotate && !hoveredNode
        controls.autoRotateSpeed = 0.6
        return
      }
      if (tries++ < 60) raf = requestAnimationFrame(apply)
    }
    apply()
    return () => cancelAnimationFrame(raf)
  }, [graph, autoRotate, hoveredNode])

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

  // Camera flights ------------------------------------------------------------

  const flyTo = useCallback((x: number, y: number, z: number, dist: number) => {
    const fg = fgRef.current
    if (!fg?.cameraPosition) return
    const len = Math.hypot(x, y, z) || 1
    const k = 1 + dist / len
    fg.cameraPosition({ x: x * k, y: y * k, z: z * k }, { x, y, z }, 900)
  }, [])

  const flyHome = useCallback(() => {
    fgRef.current?.zoomToFit?.(900, 60)
  }, [])

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

  // Chrome fade/slide transitions when entering/leaving focus (design curve).
  const EASE = '[transition-timing-function:cubic-bezier(0.32,0.72,0,1)]'
  const chromeCls = (slide: string) =>
    `transition-[opacity,transform] duration-[450ms] ${EASE} ${
      focused ? `opacity-0 pointer-events-none ${slide}` : 'opacity-100 translate-x-0 translate-y-0'
    }`

  // ── Floating top bar: title + search + project select + settings ──────────
  const topBar = (
    <div className={`absolute inset-x-0 top-0 z-20 flex items-start justify-between gap-4 px-6 lg:pl-[292px] pt-5 pr-[150px] pointer-events-none ${chromeCls('-translate-y-3.5')}`}>
      <div className="pointer-events-auto min-w-0 [text-shadow:0_2px_16px_rgba(0,0,0,0.8)]">
        {title && (
          <h1 className="text-[28px] font-extrabold tracking-[-0.02em] leading-[1.15] text-[#f4f6fa]">{title}</h1>
        )}
        {subtitle && (
          <p className="text-[13px] text-[#98a0b1] mt-1 max-w-[560px]">{subtitle}</p>
        )}
      </div>
      <div className="pointer-events-auto flex items-center gap-2 shrink-0 flex-wrap justify-end">
        {/* Node search — shows a blue match count when active */}
        <div className={`flex items-center gap-2 h-[42px] px-3.5 rounded-[11px] ${GLASS} min-w-[150px] max-w-[280px]`}>
          <Search className="w-[15px] h-[15px] text-[#5b6373] shrink-0" aria-hidden="true" />
          <input
            type="text"
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Search nodes…"
            aria-label="Search graph nodes"
            className="flex-1 min-w-0 bg-transparent border-none outline-none text-[13px] text-[#e7eaf0] placeholder:text-[#5b6373]"
          />
          {queryActive && (
            <span className="shrink-0 text-[11px] font-bold text-[#7aa2ff]" aria-label={`${matchInfo.count} matching nodes`}>
              {matchInfo.count >= MAX_MATCHES ? '500+' : matchInfo.count}
            </span>
          )}
        </div>
        {projects && onSelectProject && selectedProjectId !== undefined && (
          <ProjectSelect
            projects={projects}
            value={selectedProjectId}
            onChange={onSelectProject}
            disabled={projectsLoading}
          />
        )}
        {/* Settings (auto-rotate / auto-hide) */}
        <div className="relative">
          <button
            type="button"
            onClick={() => setSettingsOpen(o => !o)}
            className={`flex items-center justify-center w-[42px] h-[42px] rounded-[11px] ${GLASS} text-[#9aa2b2] hover:text-[#e7eaf0] hover:border-white/[0.18] transition-colors ${FOCUS_RING}`}
            aria-label="Graph settings"
            aria-expanded={settingsOpen}
          >
            <Settings2 className="w-4 h-4" />
          </button>
          {settingsOpen && (
            <div className={`absolute right-0 top-[48px] w-[220px] rounded-[12px] ${GLASS} shadow-[0_12px_40px_rgba(0,0,0,0.45)] p-3 space-y-1`}>
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
            </div>
          )}
        </div>
      </div>
    </div>
  )

  // ── Floating chip rows: project isolation + node-type filters ─────────────
  const chipRows = (
    <div className={`absolute left-6 z-20 flex flex-col gap-[9px] pointer-events-none ${showChrome ? 'top-[108px] lg:left-[292px]' : 'top-5'} ${chromeCls('-translate-y-3.5')}`}>
      {/* Per-project chips (click to isolate — active chip takes the project color) */}
      <div
        className="pointer-events-auto flex items-center gap-2 flex-wrap max-w-[72vw]"
        role="list"
        aria-label="Project family legend"
      >
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
      </div>

      {/* Node-type filter chips — always type-colored, opacity signals state */}
      <div className="pointer-events-auto flex items-center gap-2 flex-wrap max-w-[72vw]">
        {ALL_NODE_TYPES.map(type => {
          const on = visibleTypeSet.has(type)
          return (
            <button
              key={type}
              type="button"
              onClick={() => handleTypeToggle(type)}
              className="flex items-center h-[28px] px-[13px] rounded-[14px] text-[12px] font-semibold cursor-pointer transition-opacity hover:brightness-[1.15]"
              style={{
                backgroundColor: MEM_NODE_COLORS[type] ?? '#94a3b8',
                color: DARK_INK_TYPES.has(type) ? '#1a1405' : '#ffffff',
                opacity: on ? 1 : 0.28,
              }}
              aria-pressed={on}
              aria-label={`Toggle ${type} nodes`}
            >
              {type}
            </button>
          )
        })}
        {filtersDirty && (
          <button
            type="button"
            onClick={handleResetFilters}
            className={`flex items-center gap-[7px] h-[28px] px-[13px] rounded-[14px] ${GLASS_SOFT} text-[12px] font-semibold text-[#9aa2b2] hover:text-[#e7eaf0] hover:border-white/30 transition-colors`}
            aria-label="Reset graph filters"
          >
            <RotateCcw className="w-3 h-3" />
            Reset filters
          </button>
        )}
      </div>
    </div>
  )

  const statsPill = graph && graph.node_count > 0 && (
    <div className={`absolute bottom-5 left-6 ${showChrome ? 'lg:left-[292px]' : ''} z-20 flex items-center gap-3.5 h-[36px] px-4 rounded-[18px] ${GLASS_SOFT} text-[12.5px] text-[#8b93a5] whitespace-nowrap ${chromeCls('translate-y-3.5')}`}>
      <span><strong className="text-[#c9cfda] font-semibold">{fmt(graphData.nodes.length)}</strong> nodes visible</span>
      <span className="opacity-40">·</span>
      <span><strong className="text-[#c9cfda] font-semibold">{fmt(graph.node_count)}</strong> total</span>
      {isFamilyExpanded && (
        <>
          <span className="opacity-40">·</span>
          <Link to="/projects" className="text-[#7aa2ff] hover:text-[#a5c0ff] pointer-events-auto">
            {family.length} projects in family
          </Link>
        </>
      )}
    </div>
  )

  const hint = (
    <div className={`absolute bottom-6 right-6 z-10 text-[12px] text-[#646c7d] [text-shadow:0_1px_8px_rgba(0,0,0,0.8)] pointer-events-none whitespace-nowrap hidden md:block ${chromeCls('translate-y-3.5')}`}>
      hover for info · click a node · drag to rotate
    </div>
  )

  // Focus toggle — always visible; dims to 55% while focused (design).
  const focusToggle = (
    <button
      type="button"
      onClick={() => { autoFocusedRef.current = false; setFocused(f => !f) }}
      className={`absolute right-6 top-5 z-30 flex items-center gap-2 h-[42px] px-4 rounded-[11px] ${GLASS} border-white/[0.12] cursor-pointer select-none transition-opacity duration-300 hover:!opacity-100 hover:border-white/[0.28] ${FOCUS_RING}`}
      style={{ opacity: focused ? 0.55 : 1 }}
      title="Shortcut: F or double-click the graph"
      aria-pressed={focused}
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#dde1e9" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d={focused
          ? 'M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5'
          : 'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5'} />
      </svg>
      <span className="text-[13.5px] font-semibold text-[#dde1e9]">{focused ? 'Show UI' : 'Focus'}</span>
      <span className="text-[11px] text-[#6b7384] border border-white/[0.14] rounded-[5px] px-1.5 py-px">F</span>
    </button>
  )

  const exitHint = focused && (
    <div className="absolute bottom-6 left-1/2 -translate-x-1/2 z-20 h-[34px] flex items-center px-[18px] rounded-[17px] border border-white/[0.08] bg-[#0d0f14]/60 backdrop-blur-[12px] text-[12.5px] text-[#8b93a5] pointer-events-none whitespace-nowrap">
      Focus mode — press <span className="text-[#dde1e9] font-semibold mx-[5px]">F</span> or double-click to exit
    </div>
  )

  const rootClass = focused
    ? 'fixed inset-0 z-[100] bg-[#07080c] overflow-hidden'
    : 'absolute inset-0 bg-[#07080c] overflow-hidden'

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
        onDoubleClick={() => { autoFocusedRef.current = false; setFocused(f => !f) }}
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
          backgroundColor="#07080c"
          showNavInfo={false}
        />
      </div>
    )
  }

  return (
    <div ref={containerRef} className={rootClass} onPointerDown={() => settingsOpen && setSettingsOpen(false)}>
      {body}

      {/* Overlays — chrome fades/slides away in focus mode */}
      {showChrome && topBar}
      {graph && graph.node_count > 0 && (
        <>
          {chipRows}
          {statsPill}
          {hint}
          {exitHint}
        </>
      )}
      {focusToggle}

      {/* Node detail sheet — floating rounded glass panel (design spec) */}
      {selectedNode && (
        <div className="absolute right-3 top-[76px] bottom-3 w-[420px] max-w-[calc(100vw-320px)] z-[35] rounded-[16px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] shadow-[-16px_0_50px_rgba(0,0,0,0.55)] flex flex-col overflow-hidden">
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
              onClick={() => setSelectedNode(null)}
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
        </div>
      )}
    </div>
  )
}

// ── Detail field (uppercase small label + value, per the design) ─────────────

function DetailField({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-[5px]">
      <span className="text-[10.5px] font-bold tracking-[0.1em] text-[#5b6373]">{label}</span>
      <span className="text-[13px] text-[#cfd4de] leading-[1.6]">{value}</span>
    </div>
  )
}

// ── Setting toggle row ───────────────────────────────────────────────────────

function SettingToggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string
  description: string
  checked: boolean
  onChange: (v: boolean) => void
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="w-full flex items-center justify-between gap-3 px-2 py-2 rounded-[8px] hover:bg-white/[0.05] transition-colors text-left"
    >
      <span className="min-w-0">
        <span className="block text-[12.5px] font-semibold text-[#e7eaf0]">{label}</span>
        <span className="block text-[11px] text-[#7c8496]">{description}</span>
      </span>
      <span
        className={`shrink-0 w-[34px] h-[20px] rounded-full p-[2px] transition-colors ${checked ? 'bg-accent-blue' : 'bg-white/[0.12]'}`}
        aria-hidden="true"
      >
        <span
          className={`block w-4 h-4 rounded-full bg-white transition-transform ${checked ? 'translate-x-[14px]' : 'translate-x-0'}`}
        />
      </span>
    </button>
  )
}

// ── Project selector ─────────────────────────────────────────────────────────

function ProjectSelect({
  projects,
  value,
  onChange,
  disabled,
}: {
  projects: Project[]
  value: string
  onChange: (id: string) => void
  disabled: boolean
}) {
  return (
    <div className="relative">
      <select
        value={value}
        onChange={e => onChange(e.target.value)}
        disabled={disabled}
        aria-label="Select project"
        className={`appearance-none h-[42px] ${GLASS} rounded-[11px] pl-3.5 pr-9 text-[13.5px] text-[#dde1e9] focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer disabled:opacity-50 ${FOCUS_RING}`}
      >
        <option value="">Select a project…</option>
        {projects.map(p => (
          <option key={p.id} value={p.id}>
            {p.name}
            {p.parent_id ? '  (child)' : ''}
          </option>
        ))}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-[#7c8496]" />
    </div>
  )
}
