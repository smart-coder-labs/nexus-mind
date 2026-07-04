import { useMemo, useState, useCallback } from 'react'
import { useQueries, useQuery } from '@tanstack/react-query'
import { Loader2, Share2, X } from 'lucide-react'
import ReactMarkdown from 'react-markdown'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import ForceGraph3D from 'react-force-graph-3d'
import {
  mapMemGraphData,
  filterMemLinksByNodes,
  MEM_NODE_COLORS,
  MEM_EDGE_COLORS,
  type MemForceNode,
  type MemForceLink,
} from '../pages/memories/memoryGraphUtils'
import { escapeHtml } from '@/lib/utils'

const PER_PROJECT_LIMIT = 1000
const ALL_NODE_TYPES = ['Memory', 'Project', 'Session', 'User', 'Collection', 'Tag', 'AuditEvent']

interface OrgMemoryGraphProps {
  /** localStorage key suffix — use a unique value per page (e.g. "dashboard", "audit") */
  storageKey: string
  height?: number
}

export default function OrgMemoryGraph({ storageKey, height = 500 }: OrgMemoryGraphProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const projectsKey = `nexusmind-org-graph-projects-${storageKey}`
  const typesKey = `nexusmind-org-graph-types-${storageKey}`

  const [hiddenProjects, setHiddenProjects] = useState<Set<string>>(() => {
    try {
      const stored = JSON.parse(localStorage.getItem(projectsKey) ?? '[]')
      return new Set<string>(Array.isArray(stored) ? stored : [])
    } catch {
      return new Set<string>()
    }
  })

  const [visibleTypes, setVisibleTypes] = useState<Set<string>>(() => {
    try {
      const stored = JSON.parse(localStorage.getItem(typesKey) ?? 'null')
      return stored && Array.isArray(stored)
        ? new Set<string>(stored)
        : new Set<string>(ALL_NODE_TYPES)
    } catch {
      return new Set<string>(ALL_NODE_TYPES)
    }
  })

  const [selectedNode, setSelectedNode] = useState<MemForceNode | null>(null)

  const { data: projects } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    staleTime: 60_000,
  })

  const activeProjects = useMemo(
    () => projects?.filter(p => !p.archived_at) ?? [],
    [projects],
  )

  const projectGraphQueries = useQueries({
    queries: activeProjects.map(p => ({
      queryKey: ['memory-graph-org', p.name],
      queryFn: () => client.getMemoryGraph(p.name, { limit: PER_PROJECT_LIMIT }),
      staleTime: 120_000,
      retry: false,
    })),
  })

  const someDataAvailable = projectGraphQueries.some(q => q.data != null)
  const isPartiallyLoading = projectGraphQueries.some(q => q.isLoading) && someDataAvailable
  const isInitialLoading = !someDataAvailable && projectGraphQueries.some(q => q.isLoading)

  // Merge all project graphs into one deduplicated scene, inject project nodes and
  // hierarchy edges derived from parent_id so the topology is always complete.
  const { mergedNodes, mergedLinks, nodeProjectMap, truncatedProjects } = useMemo(() => {
    const nodeMap = new Map<string, MemForceNode>()
    const nodeProjectMap = new Map<string, Set<string>>()
    // Dedupe links by stable key (normalize after ForceGraph mutation)
    const linkMap = new Map<string, MemForceLink>()
    const truncatedProjects: string[] = []

    // Step 1: Process API responses from each project graph
    activeProjects.forEach((p, i) => {
      const result = projectGraphQueries[i]?.data
      if (!result) return

      if (result.node_count > PER_PROJECT_LIMIT) {
        truncatedProjects.push(p.name)
      }

      const { nodes, links } = mapMemGraphData(result)

      for (const node of nodes) {
        if (!nodeMap.has(node.id)) {
          nodeMap.set(node.id, node)
        }
        const refs = nodeProjectMap.get(node.id) ?? new Set<string>()
        refs.add(p.name)
        nodeProjectMap.set(node.id, refs)
      }

      for (const link of links) {
        // Normalize IDs — ForceGraph3D mutates source/target from string → object
        const srcId = String((link.source as unknown as { id?: string })?.id ?? link.source)
        const tgtId = String((link.target as unknown as { id?: string })?.id ?? link.target)
        const key = `${srcId}|${link.type}|${tgtId}`
        if (!linkMap.has(key)) {
          linkMap.set(key, link)
        }
      }
    })

    // Step 2: Inject project nodes for ALL active projects (even with no memories yet),
    // so hierarchy edges always have endpoints to connect to.
    for (const p of activeProjects) {
      const id = `project:${p.id}`
      if (!nodeMap.has(id)) {
        nodeMap.set(id, { id, type: 'Project', label: p.name })
      }
      const refs = nodeProjectMap.get(id) ?? new Set<string>()
      refs.add(p.name)
      nodeProjectMap.set(id, refs)
    }

    // Step 3: Inject parent_id hierarchy edges (child_of type)
    for (const p of activeProjects) {
      if (!p.parent_id) continue
      const childId = `project:${p.id}`
      const parentId = `project:${p.parent_id}`
      // Only draw edge when both endpoints are present
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
      nodeProjectMap,
      truncatedProjects,
    }
  }, [projectGraphQueries, activeProjects])

  const visibleProjects = useMemo(
    () => new Set(activeProjects.map(p => p.name).filter(n => !hiddenProjects.has(n))),
    [activeProjects, hiddenProjects],
  )

  // Apply both project and type filters
  const graphData = useMemo(() => {
    const filteredNodes = mergedNodes.filter(n => {
      if (!visibleTypes.has(n.type)) return false
      const refs = nodeProjectMap.get(n.id)
      // Keep nodes not tracked to any project (safety) and nodes referenced by a visible project
      if (!refs || refs.size === 0) return true
      return [...refs].some(p => visibleProjects.has(p))
    })

    const nodeIds = new Set(filteredNodes.map(n => n.id))
    const filteredLinks = filterMemLinksByNodes(mergedLinks, nodeIds)

    return { nodes: filteredNodes, links: filteredLinks }
  }, [mergedNodes, mergedLinks, nodeProjectMap, visibleTypes, visibleProjects])

  const handleTypeToggle = useCallback((type: string) => {
    setVisibleTypes(prev => {
      const next = new Set(prev)
      if (next.has(type)) next.delete(type)
      else next.add(type)
      try { localStorage.setItem(typesKey, JSON.stringify([...next])) } catch { /* ignore */ }
      return next
    })
  }, [typesKey])

  const handleProjectToggle = useCallback((name: string) => {
    setHiddenProjects(prev => {
      const next = new Set(prev)
      if (next.has(name)) next.delete(name)
      else next.add(name)
      try { localStorage.setItem(projectsKey, JSON.stringify([...next])) } catch { /* ignore */ }
      return next
    })
  }, [projectsKey])

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

  const nodeColor = useCallback((node: object) => {
    const n = node as MemForceNode
    return MEM_NODE_COLORS[n.type] ?? '#94a3b8'
  }, [])

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return MEM_EDGE_COLORS[l.type] ?? '#475569'
  }, [])

  // Wait for projects list before rendering
  if (!projects) {
    return (
      <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
        <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
      </div>
    )
  }

  if (activeProjects.length === 0) {
    return (
      <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
        <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
        <p className="text-xs font-semibold text-text-secondary">No projects</p>
        <p className="text-xs text-text-quaternary">Create a project to visualize the organization memory graph.</p>
      </div>
    )
  }

  if (isInitialLoading) {
    return (
      <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
        <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
      </div>
    )
  }

  return (
    <div className="space-y-3">
      {/* Filter pills */}
      <div className="flex items-start gap-3 flex-wrap">
        {/* Per-project filter pills */}
        <div className="flex items-center gap-1.5 flex-wrap">
          {activeProjects.map(p => {
            const isVisible = !hiddenProjects.has(p.name)
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => handleProjectToggle(p.name)}
                className={`flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] border transition-colors cursor-pointer ${
                  isVisible
                    ? 'border-transparent text-white opacity-100'
                    : 'border-border-primary text-text-quaternary opacity-50'
                }`}
                style={isVisible ? { backgroundColor: '#6366f1' } : undefined}
                aria-pressed={isVisible}
                aria-label={`Toggle ${p.name} project`}
              >
                {p.name}
              </button>
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
                visibleTypes.has(type)
                  ? 'border-transparent text-white opacity-100'
                  : 'border-border-primary text-text-quaternary opacity-50'
              }`}
              style={visibleTypes.has(type) ? { backgroundColor: MEM_NODE_COLORS[type] ?? '#94a3b8' } : undefined}
              aria-pressed={visibleTypes.has(type)}
              aria-label={`Toggle ${type} nodes`}
            >
              {type}
            </button>
          ))}
        </div>
      </div>

      {/* Graph scene */}
      <div className="relative border border-border-primary rounded-[18px] overflow-hidden" style={{ height }}>
        {/* Stats bar */}
        <div className="flex items-center gap-3 px-4 py-2 border-b border-border-primary bg-white/[0.02] text-[10px] text-text-quaternary">
          <span>{graphData.nodes.length} nodes visible</span>
          <span>·</span>
          <span>{graphData.links.length} edges visible</span>
          <span>·</span>
          <span>{mergedNodes.length} total nodes</span>
          {truncatedProjects.length > 0 && (
            <>
              <span>·</span>
              <span className="text-status-warning">
                showing first {PER_PROJECT_LIMIT} per project: {truncatedProjects.join(', ')}
              </span>
            </>
          )}
          {isPartiallyLoading && (
            <>
              <span>·</span>
              <span className="flex items-center gap-1">
                <Loader2 className="w-2.5 h-2.5 animate-spin" />
                loading…
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
                      <ReactMarkdown
                        components={{
                          p: ({ children }) => (
                            <p className="text-xs text-text-secondary leading-relaxed mb-2 last:mb-0">{children}</p>
                          ),
                          h1: ({ children }) => (
                            <h1 className="text-sm font-semibold text-text-primary mt-4 mb-1.5 first:mt-0">{children}</h1>
                          ),
                          h2: ({ children }) => (
                            <h2 className="text-xs font-semibold text-text-primary mt-3 mb-1 first:mt-0">{children}</h2>
                          ),
                          h3: ({ children }) => (
                            <h3 className="text-xs font-semibold text-accent-blue mt-2 mb-0.5 first:mt-0">{children}</h3>
                          ),
                          ul: ({ children }) => <ul className="mb-2 ml-3 space-y-0.5 last:mb-0">{children}</ul>,
                          ol: ({ children }) => (
                            <ol className="mb-2 ml-3 space-y-0.5 list-decimal last:mb-0">{children}</ol>
                          ),
                          li: ({ children }) => (
                            <li className="text-xs text-text-secondary leading-relaxed list-disc">{children}</li>
                          ),
                          strong: ({ children }) => (
                            <strong className="font-semibold text-text-primary">{children}</strong>
                          ),
                          em: ({ children }) => (
                            <em className="italic text-text-secondary">{children}</em>
                          ),
                          code: ({ children }) => (
                            <code className="text-[10px] font-mono text-accent-blue bg-accent-blue/10 rounded px-1 py-0.5">
                              {children}
                            </code>
                          ),
                          pre: ({ children }) => (
                            <pre className="bg-[#1d1d1f] border border-border-primary rounded-[8px] px-3 py-2 overflow-x-auto mb-2 text-[10px] font-mono">
                              {children}
                            </pre>
                          ),
                        }}
                      >
                        {memoryDetail.content}
                      </ReactMarkdown>
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
