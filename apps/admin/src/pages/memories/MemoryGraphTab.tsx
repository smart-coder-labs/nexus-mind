import { useMemo, useState, useCallback, useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Loader2, Share2, X } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { createClient } from '../../api/client'
import ForceGraph3D from 'react-force-graph-3d'
import {
  mapMemGraphData,
  filterMemNodesByTypes,
  filterMemLinksByNodes,
  MEM_NODE_COLORS,
  MEM_EDGE_COLORS,
  type MemForceNode,
} from './memoryGraphUtils'

// All possible memory graph node types
const ALL_NODE_TYPES = ['Memory', 'Project', 'Session', 'User', 'Collection', 'Tag', 'AuditEvent']
const DEFAULT_VISIBLE_TYPES = new Set<string>(ALL_NODE_TYPES)

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, c => (
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;'
  ))
}

export default function MemoryGraphTab() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const [selectedProject, setSelectedProject] = useState('')
  const [visibleTypes, setVisibleTypes] = useState<Set<string>>(new Set(DEFAULT_VISIBLE_TYPES))
  const [selectedNode, setSelectedNode] = useState<MemForceNode | null>(null)

  // Clear detail panel when switching projects
  useEffect(() => { setSelectedNode(null) }, [selectedProject])

  const { data: projects } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    staleTime: 60_000,
  })

  const { data: graph, isLoading, isError, error } = useQuery({
    queryKey: ['memory-graph', selectedProject],
    queryFn: () => client.getMemoryGraph(selectedProject),
    enabled: selectedProject.trim().length > 0,
    retry: false,
  })

  const graphData = useMemo(() => {
    if (!graph) return { nodes: [] as MemForceNode[], links: [] }
    const mapped = mapMemGraphData(graph)
    const filteredNodes = filterMemNodesByTypes(mapped.nodes, visibleTypes)
    const nodeIds = new Set(filteredNodes.map(n => n.id))
    const filteredLinks = filterMemLinksByNodes(mapped.links, nodeIds)
    return { nodes: filteredNodes, links: filteredLinks }
  }, [graph, visibleTypes])

  const handleTypeToggle = useCallback((type: string) => {
    setVisibleTypes(prev => {
      const next = new Set(prev)
      if (next.has(type)) next.delete(type)
      else next.add(type)
      return next
    })
  }, [])

  const nodeLabel = useCallback((node: object) => {
    const n = node as MemForceNode
    const color = MEM_NODE_COLORS[n.type] ?? '#94a3b8'
    return `<div style="padding:6px 9px;background:#16161a;border:1px solid #2a2a2e;border-radius:8px;font-family:ui-sans-serif,system-ui;max-width:320px;">
      <div style="font-size:12px;font-weight:600;color:#e5e7eb;">${escapeHtml(n.label)}</div>
      <div style="font-size:10px;color:${color};margin-top:1px;">${n.type}</div>
      <div style="font-size:10px;color:#9ca3af;margin-top:3px;font-family:ui-monospace,monospace;">${escapeHtml(n.id)}</div>
    </div>`
  }, [])

  const handleNodeClick = useCallback((node: object) => {
    setSelectedNode(node as MemForceNode)
  }, [])

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return MEM_EDGE_COLORS[l.type] ?? '#475569'
  }, [])

  const nodeColor = useCallback((node: object) => {
    const n = node as MemForceNode
    return MEM_NODE_COLORS[n.type] ?? '#94a3b8'
  }, [])

  const activeProjects = useMemo(() => projects?.filter(p => !p.archived_at) ?? [], [projects])

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

        {graph && (
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
        )}
      </div>

      {!selectedProject && (
        <div className="border border-border-primary rounded-[18px] p-10 text-center">
          <Share2 className="w-5 h-5 text-text-quaternary/40 mx-auto mb-2" />
          <p className="text-xs text-text-quaternary">Select a project to visualize its memory graph.</p>
        </div>
      )}

      {selectedProject && isLoading && (
        <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
          <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
        </div>
      )}

      {selectedProject && isError && (
        <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
          {(error as Error)?.message ?? 'Failed to load memory graph.'}
        </div>
      )}

      {selectedProject && !isLoading && !isError && graph && (
        graph.node_count === 0 ? (
          <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
            <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
            <p className="text-xs font-semibold text-text-secondary">No graph data</p>
            <p className="text-xs text-text-quaternary">
              No memory nodes found for this project.
            </p>
          </div>
        ) : (
          <div className="relative border border-border-primary rounded-[18px] overflow-hidden" style={{ height: 600 }}>
            {/* Stats */}
            <div className="flex items-center gap-3 px-4 py-2 border-b border-border-primary bg-white/[0.02] text-[10px] text-text-quaternary">
              <span>{graphData.nodes.length} nodes visible</span>
              <span>·</span>
              <span>{graphData.links.length} edges visible</span>
              <span>·</span>
              <span>{graph.node_count} total nodes</span>
              <span>·</span>
              <span>{graph.edge_count} total edges</span>
              <span className="ml-auto text-text-quaternary/70">hover for info · click a node for details · drag to rotate</span>
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

            {/* Detail panel */}
            {selectedNode && (
              <div className="absolute top-0 right-0 h-full w-[380px] max-w-[70%] bg-[#16161a]/95 border-l border-border-primary backdrop-blur-sm flex flex-col">
                <div className="flex items-start gap-2 px-4 py-3 border-b border-border-primary">
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-semibold text-text-primary truncate">{selectedNode.label}</p>
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
                    {selectedNode.type === 'Memory' && (
                      <p className="text-[10px] text-text-quaternary mt-2">
                        View this memory in the Memories tab to see full content.
                      </p>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => setSelectedNode(null)}
                    className="text-text-quaternary hover:text-text-primary transition-colors shrink-0"
                    aria-label="Close detail panel"
                  >
                    <X className="w-4 h-4" />
                  </button>
                </div>
                <div className="flex-1 overflow-auto p-4 space-y-3">
                  <div>
                    <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Type</p>
                    <p className="text-xs text-text-secondary">{selectedNode.type}</p>
                  </div>
                  <div>
                    <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">Label</p>
                    <p className="text-xs text-text-secondary">{selectedNode.label}</p>
                  </div>
                </div>
              </div>
            )}
          </div>
        )
      )}
    </div>
  )
}
