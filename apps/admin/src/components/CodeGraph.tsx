import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Loader2, Share2, X } from 'lucide-react'
import ForceGraph3D from 'react-force-graph-3d'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { usePersistedGraphState } from '../hooks/usePersistedGraphState'
import { useGraphChrome } from './graph/useGraphChrome'
import {
  DetailField,
  FocusExitHint,
  FocusToggle,
  GRAPH_BG,
  GlassChip,
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
import {
  DEFAULT_VISIBLE_TYPES,
  EDGE_COLORS,
  EXTERNAL_COLLAPSE_THRESHOLD,
  NODE_COLORS,
  computeExternalAggregate,
  filterLinksByNodes,
  filterNodesByTypes,
  mapGraphData,
  type ForceGraphNode,
} from '../pages/code/graphUtils'
import { escapeHtml } from '@/lib/utils'
import type { CodeProject } from '../types'

/** Every node type the code graph can return, in chip render order. */
const ALL_NODE_TYPES = [
  'Project', 'Folder', 'File', 'Module',
  'Function', 'Method', 'Class', 'Struct',
  'Interface', 'Type', 'Enum', 'External',
]

const DEFAULT_TYPES = ALL_NODE_TYPES.filter(t => DEFAULT_VISIBLE_TYPES.has(t))

// Matched search hits get the bright highlight fill (same as the memory graph).
const HIGHLIGHT_COLOR = '#dbeafe'
// Chip text uses dark ink on the yellow Interface pill.
const DARK_INK_TYPES = new Set(['Interface'])

// Search activates at 2+ chars and caps the reported match count.
const MIN_QUERY = 2
const MAX_MATCHES = 500

interface CodeGraphProps {
  /** All code repositories; only indexed ones can be graphed. */
  projects: CodeProject[] | undefined
  projectsLoading?: boolean
  /** True when the repository list itself failed to load — without it, a 500
   *  is indistinguishable from "this org has no indexed repositories". */
  projectsError?: boolean
  /** Selected repository NAME (`code_projects.name` — the graph API key). */
  selectedRepo: string
  onSelectRepo: (name: string) => void
  /** localStorage key suffix for the persisted filter + behavior state. */
  storageKey: string
  title?: string
  subtitle?: string
  /** Graph-source switcher rendered in the top bar's control cluster. */
  tabs?: React.ReactNode
  onFocusedChange?: (focused: boolean) => void
}

/**
 * Repository-scoped code graph, rendered with the SAME immersive shell as the
 * memory knowledge graph (`OrgMemoryGraph`): full-bleed 3D canvas, floating
 * glass chrome, focus mode, auto-hide and auto-rotate — all shared via
 * `./graph/chrome` + `./graph/useGraphChrome`.
 *
 * What differs from the memory graph is only the data: nodes are code symbols
 * (Project/Folder/File/Function/Class…), the selector picks a repository by
 * name instead of a project id, and the detail panel shows the node's source
 * instead of a memory record.
 */
export default function CodeGraph({
  projects,
  projectsLoading = false,
  projectsError = false,
  selectedRepo,
  onSelectRepo,
  storageKey,
  title,
  subtitle,
  tabs,
  onFocusedChange,
}: CodeGraphProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const indexedProjects = useMemo(
    () => projects?.filter(p => p.last_indexed != null && !p.archived_at) ?? [],
    [projects],
  )

  const [visibleTypes, setVisibleTypes, resetVisibleTypes] = usePersistedGraphState<string[]>(
    `nexusmind-code-graph-types-${storageKey}`,
    DEFAULT_TYPES,
    // A non-array survivor from a corrupted entry would crash the page with no
    // way out but clearing localStorage by hand.
    { validate: Array.isArray },
  )
  const visibleTypeSet = useMemo(() => new Set(visibleTypes), [visibleTypes])

  const [expandExternals, setExpandExternals] = useState(false)
  const [selectedNode, setSelectedNode] = useState<ForceGraphNode | null>(null)
  const [query, setQuery] = useState('')
  const [hoveredNode, setHoveredNode] = useState(false)

  const { data: graph, isLoading, isError, error } = useQuery({
    queryKey: ['code-graph', selectedRepo],
    queryFn: () => client.getCodeGraph(selectedRepo),
    enabled: selectedRepo.trim().length > 0,
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

  // Reset the transient view state when switching repositories.
  useEffect(() => { setSelectedNode(null); setQuery(''); setExpandExternals(false) }, [selectedRepo])

  // Source of the clicked symbol. Any node backed by a file has source:
  // symbols use their line range, a File node (no range) shows the whole file.
  // Folder/Project/External nodes have no file.
  const hasSource = !!selectedNode && selectedNode.fp != null
  const {
    data: snippet,
    isLoading: snippetLoading,
    isError: snippetError,
    error: snippetErr,
  } = useQuery({
    queryKey: ['code-snippet', selectedRepo, selectedNode?.fp, selectedNode?.startLine, selectedNode?.endLine],
    queryFn: () => client.getCodeSnippet(
      selectedRepo,
      selectedNode!.fp!,
      selectedNode!.startLine ?? undefined,
      selectedNode!.endLine ?? undefined,
    ),
    enabled: hasSource,
    retry: false,
  })

  const graphData = useMemo(() => {
    if (!graph) return { nodes: [] as ForceGraphNode[], links: [] }
    const mapped = mapGraphData(graph)
    const { nodes: withAgg, links: remappedLinks } = computeExternalAggregate(
      mapped.nodes,
      mapped.links,
      expandExternals,
    )
    const filteredNodes = filterNodesByTypes(withAgg, visibleTypeSet)
    const nodeIds = new Set(filteredNodes.map(n => n.id))
    return { nodes: filteredNodes, links: filterLinksByNodes(remappedLinks, nodeIds) }
  }, [graph, visibleTypeSet, expandExternals])

  const normalizedQuery = query.trim().toLowerCase()
  const queryActive = normalizedQuery.length >= MIN_QUERY

  const matchInfo = useMemo(() => {
    if (!queryActive) return { ids: new Set<number>(), count: 0, first: null as ForceGraphNode | null }
    const ids = new Set<number>()
    let first: ForceGraphNode | null = null
    for (const n of graphData.nodes) {
      if (n.name.toLowerCase().includes(normalizedQuery)) {
        ids.add(n.id)
        if (!first) first = n
        if (ids.size >= MAX_MATCHES) break
      }
    }
    return { ids, count: ids.size, first }
  }, [queryActive, normalizedQuery, graphData])

  // Fly to the first search match once matches settle.
  const lastFlownRef = useRef<number | null>(null)
  useEffect(() => {
    if (!queryActive) {
      if (lastFlownRef.current !== null) {
        lastFlownRef.current = null
        flyHome()
      }
      return
    }
    const first = matchInfo.first as (ForceGraphNode & { z?: number }) | null
    if (!first || lastFlownRef.current === first.id) return
    if (typeof first.x !== 'number') return
    lastFlownRef.current = first.id
    flyTo(first.x, first.y!, first.z!, 620)
  }, [queryActive, matchInfo, flyTo, flyHome])

  // Plain add/remove toggle — unlike the memory graph, the code graph does NOT
  // start with every type on (External is hidden by default), so the
  // "isolate on first click" rule has no sane starting point here.
  const handleTypeToggle = useCallback((type: string) => {
    setVisibleTypes(prev => (
      prev.includes(type) ? prev.filter(t => t !== type) : [...prev, type]
    ))
  }, [setVisibleTypes])

  const handleResetFilters = useCallback(() => {
    resetVisibleTypes()
    setExpandExternals(false)
    flyHome()
  }, [resetVisibleTypes, flyHome])

  const handleNodeClick = useCallback((node: object) => {
    setSelectedNode(node as ForceGraphNode)
  }, [])

  // Hover tooltip: name, type, location, language.
  const nodeLabel = useCallback((node: object) => {
    const n = node as ForceGraphNode
    const loc = n.startLine != null
      ? `${escapeHtml(n.fp ?? '')}:${n.startLine}${n.endLine != null ? `-${n.endLine}` : ''}`
      : escapeHtml(n.fp ?? n.name)
    const lang = n.language ? ` · ${escapeHtml(n.language)}` : ''
    const color = NODE_COLORS[n.type] ?? '#94a3b8'
    return `<div style="padding:10px 14px;background:rgba(17,19,25,0.95);border:1px solid rgba(255,255,255,0.12);border-radius:10px;box-shadow:0 10px 34px rgba(0,0,0,0.6);font-family:ui-sans-serif,system-ui;max-width:340px;">
      <div style="font-size:13.5px;font-weight:700;color:#f4f6fa;margin-bottom:2px;">${escapeHtml(n.name)}</div>
      <div style="font-size:12px;color:${color};margin-bottom:2px;">${escapeHtml(n.type)}</div>
      <div style="font-size:11.5px;color:#5b6373;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;">${loc}${lang}</div>
    </div>`
  }, [])

  const nodeColor = useCallback((node: object) => {
    const n = node as ForceGraphNode
    if (queryActive && matchInfo.ids.has(n.id)) return HIGHLIGHT_COLOR
    return NODE_COLORS[n.type] ?? '#94a3b8'
  }, [queryActive, matchInfo])

  // Node sizes by type — structure hubs big, symbols small (mirrors the
  // memory graph's hierarchy so both graphs read the same way).
  const nodeVal = useCallback((node: object) => {
    const n = node as ForceGraphNode
    if (queryActive && matchInfo.ids.has(n.id)) return 6
    if (n.type === 'Project') return 18
    if (n.type === 'Folder' || n.type === 'Module') return 5
    if (n.type === 'File') return 3
    return 1.2
  }, [queryActive, matchInfo])

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return EDGE_COLORS[l.type] ?? '#475569'
  }, [])

  const externalCount = graph?.nodes.filter(n => n.type === 'External').length ?? 0
  const showExternalToggle = visibleTypeSet.has('External') && externalCount > EXTERNAL_COLLAPSE_THRESHOLD
  const filtersDirty =
    visibleTypes.length !== DEFAULT_TYPES.length ||
    !DEFAULT_TYPES.every(t => visibleTypeSet.has(t)) ||
    expandExternals

  const isInitialLoading = isLoading && !graph
  const selectedRepoName = indexedProjects.find(p => p.name === selectedRepo)?.name

  // ── Floating top bar ──────────────────────────────────────────────────────
  const topBar = (
    <GraphTopBar title={title} subtitle={subtitle} focused={focused}>
      {tabs}
      <GraphSearchBox
        value={query}
        onChange={setQuery}
        active={queryActive}
        count={matchInfo.count}
        maxMatches={MAX_MATCHES}
        placeholder="Search symbols…"
      />
      <GraphSelect
        value={selectedRepo}
        onChange={onSelectRepo}
        disabled={projectsLoading || indexedProjects.length === 0}
        ariaLabel="Select repository"
        placeholder="Select a repository…"
        options={indexedProjects.map(p => ({ value: p.name, label: p.name }))}
      />
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

  // ── Floating chip rows: repository + node-type filters ────────────────────
  const chipRows = (
    <GraphChipRows focused={focused} offsetForChrome>
      {selectedRepoName && (
        <GraphChipRow role="list" aria-label="Repository legend">
          <span
            role="listitem"
            className="flex items-center gap-2 h-[32px] px-[13px] rounded-[16px] border border-white/[0.09] bg-[#0d0f14]/[0.66] backdrop-blur-[12px]"
            title={`Repository: ${selectedRepoName}`}
          >
            <span
              className="w-[9px] h-[9px] rounded-full shrink-0"
              style={{ backgroundColor: NODE_COLORS.Project }}
              aria-hidden="true"
            />
            <span className="text-[12.5px] text-[#cfd4de]">{selectedRepoName}</span>
          </span>
        </GraphChipRow>
      )}

      <GraphChipRow>
        {ALL_NODE_TYPES.map(type => (
          <TypeChip
            key={type}
            type={type}
            color={NODE_COLORS[type] ?? '#94a3b8'}
            active={visibleTypeSet.has(type)}
            darkInk={DARK_INK_TYPES.has(type)}
            onClick={() => handleTypeToggle(type)}
          />
        ))}
        {showExternalToggle && (
          <GlassChip
            onClick={() => setExpandExternals(v => !v)}
            ariaLabel={expandExternals ? 'Collapse external dependencies' : 'Expand external dependencies'}
          >
            {expandExternals ? 'Collapse externals' : `Expand ${fmt(externalCount)} externals`}
          </GlassChip>
        )}
        {filtersDirty && <ResetFiltersChip onClick={handleResetFilters} />}
      </GraphChipRow>
    </GraphChipRows>
  )

  const statsPill = graph && graph.node_count > 0 && (
    <GraphStatsPill focused={focused} offsetForChrome>
      <span><StatValue>{fmt(graphData.nodes.length)}</StatValue> nodes visible</span>
      <StatSeparator />
      <span><StatValue>{fmt(graphData.links.length)}</StatValue> edges visible</span>
      <StatSeparator />
      <span><StatValue>{fmt(graph.node_count)}</StatValue> total nodes</span>
      <StatSeparator />
      <span><StatValue>{fmt(graph.edge_count)}</StatValue> total edges</span>
    </GraphStatsPill>
  )

  // ── Render states ─────────────────────────────────────────────────────────
  const hasGraph = !!graph && graph.node_count > 0

  let body: React.ReactNode
  if (projectsError) {
    body = (
      <div className="absolute inset-0 flex items-center justify-center p-6">
        <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
          Failed to load repositories. Please refresh.
        </div>
      </div>
    )
  } else if (!projectsLoading && indexedProjects.length === 0) {
    body = (
      <EmptyState
        title="No indexed repositories yet"
        description="Index a repository from the Code page to explore its graph."
      />
    )
  } else if (!selectedRepo) {
    body = (
      <EmptyState
        title="Select a repository"
        description="Choose a repository from the dropdown to explore its code graph."
      />
    )
  } else if (isInitialLoading) {
    body = (
      <div className="absolute inset-0 flex items-center justify-center">
        <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
      </div>
    )
  } else if (isError) {
    body = (
      <div className="absolute inset-0 flex items-center justify-center p-6">
        <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
          {(error as Error)?.message ?? 'Failed to load code graph.'}
        </div>
      </div>
    )
  } else if (!hasGraph) {
    body = (
      <EmptyState
        title="No graph data"
        description="This repository has no indexed symbols yet. Try re-indexing it."
      />
    )
  } else if (graphData.nodes.length === 0) {
    body = (
      <EmptyState
        title="No nodes match the current filters"
        description="Re-enable a node type to bring the graph back."
      />
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

      {topBar}
      {hasGraph && (
        <>
          {chipRows}
          {statsPill}
          <GraphHint focused={focused} text="hover for info · click a node for source · drag to rotate" />
          {focused && <FocusExitHint />}
        </>
      )}
      <FocusToggle focused={focused} onToggle={toggleFocus} />

      {selectedNode && (
        <GraphDetailPanel>
          <div className="flex items-start gap-3 px-5 pt-[18px] pb-3.5 border-b border-white/[0.06] shrink-0">
            <div className="flex flex-col gap-2 flex-1 min-w-0">
              <h2 className="m-0 text-[16px] font-bold text-[#f4f6fa] leading-[1.35] truncate">
                {selectedNode.name}
              </h2>
              <div className="flex items-center gap-1.5 flex-wrap">
                <span
                  className="text-[11.5px] font-semibold px-[11px] py-[3px] rounded-[11px]"
                  style={{
                    backgroundColor: NODE_COLORS[selectedNode.type] ?? '#94a3b8',
                    color: DARK_INK_TYPES.has(selectedNode.type) ? '#1a1405' : '#ffffff',
                  }}
                >
                  {selectedNode.type}
                </span>
                {selectedNode.language && (
                  <span className="text-[11.5px] px-[11px] py-[3px] rounded-[11px] bg-white/[0.06] text-[#b9c1d0]">
                    {selectedNode.language}
                  </span>
                )}
              </div>
              {selectedNode.fp && (
                <span className="text-[11.5px] text-[#5b6373] font-mono truncate">
                  {selectedNode.fp}
                  {selectedNode.startLine != null && `:${selectedNode.startLine}`}
                  {selectedNode.endLine != null && `-${selectedNode.endLine}`}
                </span>
              )}
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
            <DetailField label="TYPE" value={selectedNode.type} />
            <DetailField label="NAME" value={selectedNode.name} />
            {selectedNode.fp && <DetailField label="FILE" value={selectedNode.fp} />}

            {!hasSource && (
              <p className="text-[13px] text-[#8b93a5] leading-[1.6]">
                {selectedNode.type} node — no file source. Click a File or a code symbol
                (Function, Method, Class…) to view code.
              </p>
            )}
            {hasSource && snippetLoading && (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="w-4 h-4 animate-spin text-text-quaternary" />
              </div>
            )}
            {hasSource && snippetError && (
              <p className="text-[13px] text-status-error/80">
                {(snippetErr as Error)?.message ?? 'No source found.'}
              </p>
            )}
            {hasSource && snippet && (
              <div className="flex flex-col gap-[7px]">
                <span className="text-[10.5px] font-bold tracking-[0.1em] text-[#5b6373]">SOURCE</span>
                <pre className="px-4 py-3.5 rounded-[12px] border border-white/[0.06] bg-white/[0.02] text-[11.5px] leading-[1.65] text-[#b9c1d0] font-mono overflow-x-auto">
                  <code>{snippet.content}</code>
                </pre>
              </div>
            )}
          </div>
        </GraphDetailPanel>
      )}
    </div>
  )
}

function EmptyState({ title, description }: { title: string; description: string }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center">
      <div className="text-center space-y-2">
        <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
        <p className="text-xs font-semibold text-text-secondary">{title}</p>
        <p className="text-xs text-text-quaternary">{description}</p>
      </div>
    </div>
  )
}
