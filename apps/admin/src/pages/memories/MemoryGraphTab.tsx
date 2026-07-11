import { useEffect, useMemo, useState, useCallback } from 'react'
import { useQueries, useQuery } from '@tanstack/react-query'
import { Loader2, RotateCcw, Share2, X } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { createClient } from '../../api/client'
import { Markdown } from '../../components/ui/Markdown'
import ForceGraph3D from 'react-force-graph-3d'
import {
  mapMemGraphData,
  filterMemLinksByNodes,
  MEM_NODE_COLORS,
  MEM_EDGE_COLORS,
  type MemForceNode,
  type MemForceLink,
} from './memoryGraphUtils'
import { usePersistedGraphState } from '../../hooks/usePersistedGraphState'
import { escapeHtml } from '@/lib/utils'
import type { Project } from '../../types'

// All possible memory graph node types
const ALL_NODE_TYPES = ['Memory', 'Project', 'Session', 'User', 'Collection', 'Tag', 'AuditEvent']
const PER_PROJECT_LIMIT = 1000

// localStorage keys (versioned to allow future schema migration)
const SELECTED_PROJECT_KEY = 'nexusmind-memory-graph-project'
const VISIBLE_TYPES_KEY = 'nexusmind-memory-graph-types'
const STORAGE_VERSION = 1

/**
 * BFS walk that collects the full connected family of a project:
 * ancestors (via parent_id) AND descendants (via children).
 * Returns project names (not IDs). Guarded against parent_id cycles
 * via a visited set on project IDs.
 */
function buildProjectFamily(selectedName: string, allProjects: Project[]): string[] {
  const byId = new Map<string, Project>()
  const byName = new Map<string, Project>()
  allProjects.forEach(p => {
    byId.set(p.id, p)
    byName.set(p.name, p)
  })

  const selected = byName.get(selectedName)
  if (!selected) return [selectedName]

  const family = new Set<string>()
  const queue = [selected.id]
  const visitedIds = new Set<string>()

  while (queue.length > 0) {
    const id = queue.shift()!
    if (visitedIds.has(id)) continue
    visitedIds.add(id)

    const p = byId.get(id)
    if (!p) continue
    family.add(p.name)

    // Walk up to parent
    if (p.parent_id && !visitedIds.has(p.parent_id)) {
      queue.push(p.parent_id)
    }

    // Walk down to children
    for (const child of allProjects) {
      if (child.parent_id === id && !visitedIds.has(child.id)) {
        queue.push(child.id)
      }
    }
  }

  return Array.from(family)
}

export default function MemoryGraphTab() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  // Persist the user's selection across reloads so the graph returns to the
  // same project and node-type visibility they last viewed.
  const [selectedProject, setSelectedProject, resetSelectedProject] = usePersistedGraphState<string>(
    SELECTED_PROJECT_KEY,
    '',
    { version: STORAGE_VERSION },
  )
  const [visibleTypes, setVisibleTypes, resetVisibleTypes] = usePersistedGraphState<string[]>(
    VISIBLE_TYPES_KEY,
    ALL_NODE_TYPES,
    { version: STORAGE_VERSION },
  )

  const visibleTypeSet = useMemo(() => new Set(visibleTypes), [visibleTypes])

  const [selectedNode, setSelectedNode] = useState<MemForceNode | null>(null)

  // Clear detail panel when switching projects
  useEffect(() => { setSelectedNode(null) }, [selectedProject])

  const { data: projects } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    staleTime: 60_000,
  })

  const activeProjects = useMemo(() => projects?.filter(p => !p.archived_at) ?? [], [projects])

  // Compute the connected family (selected + ancestors + descendants)
  const familyProjects = useMemo(() => {
    if (!selectedProject || activeProjects.length === 0) return []
    return buildProjectFamily(selectedProject, activeProjects)
  }, [selectedProject, activeProjects])

  // Fetch every project in the family in parallel
  const familyGraphQueries = useQueries({
    queries: familyProjects.map(projectName => ({
      queryKey: ['memory-graph', projectName],
      queryFn: () => client.getMemoryGraph(projectName, { limit: PER_PROJECT_LIMIT }),
      enabled: projectName.trim().length > 0,
      retry: false,
      staleTime: 120_000,
    })),
  })

  const someDataAvailable = familyGraphQueries.some(q => q.data != null)
  const isLoading = familyProjects.length > 0 && familyGraphQueries.some(q => q.isLoading)
  const isInitialLoading = isLoading && !someDataAvailable
  const isError = !someDataAvailable && familyGraphQueries.length > 0 && familyGraphQueries[0]?.isError
  const primaryError = familyGraphQueries[0]?.error

  // Merge all family graphs into one deduplicated scene and inject hierarchy edges
  const { mergedNodes, mergedLinks, truncatedProjects } = useMemo(() => {
    const nodeMap = new Map<string, MemForceNode>()
    const linkMap = new Map<string, MemForceLink>()
    const truncatedProjects: string[] = []

    // Step 1: Merge API responses
    familyProjects.forEach((projectName, i) => {
      const result = familyGraphQueries[i]?.data
      if (!result) return

      if (result.node_count > PER_PROJECT_LIMIT) {
        truncatedProjects.push(projectName)
      }

      const { nodes, links } = mapMemGraphData(result)

      for (const node of nodes) {
        if (!nodeMap.has(node.id)) {
          nodeMap.set(node.id, node)
        }
      }

      for (const link of links) {
        const srcId = String((link.source as unknown as { id?: string })?.id ?? link.source)
        const tgtId = String((link.target as unknown as { id?: string })?.id ?? link.target)
        const key = `${srcId}|${link.type}|${tgtId}`
        if (!linkMap.has(key)) {
          linkMap.set(key, link)
        }
      }
    })

    // Step 2: Inject project nodes for ALL active projects (ensures hierarchy edges connect)
    for (const p of activeProjects) {
      const id = `project:${p.id}`
      if (!nodeMap.has(id)) {
        nodeMap.set(id, { id, type: 'Project', label: p.name })
      }
    }

    // Step 3: Inject parent_id hierarchy edges (child_of) for all active projects
    for (const p of activeProjects) {
      if (!p.parent_id) continue
      const childId = `project:${p.id}`
      const parentId = `project:${p.parent_id}`
      if (nodeMap.has(childId) && nodeMap.has(parentId)) {
        const key = `${childId}|child_of|${parentId}`
        if (!linkMap.has(key)) {
          linkMap.set(key, { source: childId, target: parentId, type: 'child_of' })
        }
      }
    }

    return {
      mergedNodes: Array.from(nodeMap.values()),
      mergedLinks: Array.from(linkMap.values()),
      truncatedProjects,
    }
  }, [familyGraphQueries, familyProjects, activeProjects])

  // Apply type filter
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
    resetSelectedProject()
    resetVisibleTypes()
  }, [resetSelectedProject, resetVisibleTypes])

  const handleNodeClick = useCallback((node: object) => {
    setSelectedNode(node as MemForceNode)
  }, [])

  // Extract memory UUID from namespaced id "memory:uuid"
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

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return MEM_EDGE_COLORS[l.type] ?? '#475569'
  }, [])

  const nodeColor = useCallback((node: object) => {
    const n = node as MemForceNode
    return MEM_NODE_COLORS[n.type] ?? '#94a3b8'
  }, [])

  const totalNodeCount = familyGraphQueries.reduce((sum, q) => sum + (q.data?.node_count ?? 0), 0)
  const totalEdgeCount = familyGraphQueries.reduce((sum, q) => sum + (q.data?.edge_count ?? 0), 0)

  const isFamilyExpanded = familyProjects.length > 1

  return (
    <div className="space-y-4">
      {/* Controls bar */}
      <div className="flex items-start gap-3 flex-wrap">
        <select
          value={selectedProject}
          onChange={e => setSelectedProject(e.target.value)}
          className="bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors"
          aria-label="Select project"
        >
          <option value="">Select project…</option>
          {activeProjects.map(p => (
            <option key={p.id} value={p.name}>{p.name}</option>
          ))}
        </select>

        {someDataAvailable && (
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
        )}

        {/* Reset filters — clears persisted project + type selection */}
        {(selectedProject || visibleTypeSet.size !== ALL_NODE_TYPES.length) && (
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

      {!selectedProject && (
        <div className="border border-border-primary rounded-[18px] p-10 text-center">
          <Share2 className="w-5 h-5 text-text-quaternary/40 mx-auto mb-2" />
          <p className="text-xs text-text-quaternary">Select a project to visualize its memory graph.</p>
        </div>
      )}

      {selectedProject && isInitialLoading && (
        <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
          <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
        </div>
      )}

      {selectedProject && isError && (
        <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
          {(primaryError as Error)?.message ?? 'Failed to load memory graph.'}
        </div>
      )}

      {selectedProject && !isInitialLoading && !isError && someDataAvailable && (
        mergedNodes.length === 0 ? (
          <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
            <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
            <p className="text-xs font-semibold text-text-secondary">No graph data</p>
            <p className="text-xs text-text-quaternary">
              No memory nodes found for this project.
            </p>
          </div>
        ) : (
          <div className="relative border border-border-primary rounded-[18px] overflow-hidden" style={{ height: 600 }}>
            {/* Stats bar */}
            <div className="flex items-center gap-3 px-4 py-2 border-b border-border-primary bg-white/[0.02] text-[10px] text-text-quaternary">
              <span>{graphData.nodes.length} nodes visible</span>
              <span>·</span>
              <span>{graphData.links.length} edges visible</span>
              <span>·</span>
              <span>{totalNodeCount} total nodes</span>
              <span>·</span>
              <span>{totalEdgeCount} total edges</span>
              {isFamilyExpanded && (
                <>
                  <span>·</span>
                  <span className="text-accent-blue/70">
                    {familyProjects.length} projects in family
                  </span>
                </>
              )}
              {truncatedProjects.length > 0 && (
                <>
                  <span>·</span>
                  <span className="text-status-warning">
                    capped at {PER_PROJECT_LIMIT}: {truncatedProjects.join(', ')}
                  </span>
                </>
              )}
              {isLoading && someDataAvailable && (
                <>
                  <span>·</span>
                  <span className="flex items-center gap-1">
                    <Loader2 className="w-2.5 h-2.5 animate-spin" />
                    loading family…
                  </span>
                </>
              )}
              <span className="ml-auto text-text-quaternary/70">hover for info · click a node · drag to rotate</span>
            </div>

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
                    </div>
                    <p className="text-[10px] text-text-quaternary font-mono mt-1 break-all">
                      {selectedNode.id}
                    </p>
                  </div>
                  <button
                    type="button"
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
        )
      )}
    </div>
  )
}
