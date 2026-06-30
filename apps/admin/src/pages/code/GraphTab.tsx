import { useMemo, useState, useCallback, useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Loader2, Share2, X } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { createClient } from '../../api/client'
import type { CodeProject } from '../../types'
import ForceGraph3D from 'react-force-graph-3d'
import {
  mapGraphData,
  filterNodesByTypes,
  filterLinksByNodes,
  computeExternalAggregate,
  DEFAULT_VISIBLE_TYPES,
  EXTERNAL_COLLAPSE_THRESHOLD,
  NODE_COLORS,
  EDGE_COLORS,
  type ForceGraphNode,
} from './graphUtils'

interface GraphTabProps {
  projects: CodeProject[] | undefined
}

// All possible node types that can appear in the graph
const ALL_NODE_TYPES = [
  'Project', 'Folder', 'File', 'Module',
  'Function', 'Method', 'Class', 'Struct',
  'Interface', 'Type', 'Enum', 'External',
]

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, c => (
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;'
  ))
}

export default function GraphTab({ projects }: GraphTabProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const indexedProjects = useMemo(
    () => projects?.filter(p => p.last_indexed != null) ?? [],
    [projects],
  )

  const [selectedProject, setSelectedProject] = useState('')
  const [visibleTypes, setVisibleTypes] = useState<Set<string>>(new Set(DEFAULT_VISIBLE_TYPES))
  const [expandExternals, setExpandExternals] = useState(false)
  const [selectedNode, setSelectedNode] = useState<ForceGraphNode | null>(null)

  // Clear any open detail panel when switching projects
  useEffect(() => { setSelectedNode(null) }, [selectedProject])

  const { data: graph, isLoading, isError, error } = useQuery({
    queryKey: ['code-graph', selectedProject],
    queryFn: () => client.getCodeGraph(selectedProject),
    enabled: selectedProject.trim().length > 0,
    retry: false,
  })

  // Source of the clicked symbol (only for nodes that map to code, i.e. have a line)
  // Any node backed by a file has source: symbols use their line range; a File
  // node (no line range) shows the whole file. Folder/Project have no file.
  const hasSource = !!selectedNode && selectedNode.fp != null
  const { data: snippet, isLoading: snippetLoading, isError: snippetError, error: snippetErr } = useQuery({
    queryKey: ['code-snippet', selectedProject, selectedNode?.fp, selectedNode?.startLine, selectedNode?.endLine],
    queryFn: () => client.getCodeSnippet(
      selectedProject,
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

    const filteredNodes = filterNodesByTypes(withAgg, visibleTypes)
    const nodeIds = new Set(filteredNodes.map(n => n.id))
    const filteredLinks = filterLinksByNodes(remappedLinks, nodeIds)

    return { nodes: filteredNodes, links: filteredLinks }
  }, [graph, visibleTypes, expandExternals])

  const handleTypeToggle = useCallback((type: string) => {
    setVisibleTypes(prev => {
      const next = new Set(prev)
      if (next.has(type)) next.delete(type)
      else next.add(type)
      return next
    })
  }, [])

  // Hover tooltip: name, type, location, language
  const nodeLabel = useCallback((node: object) => {
    const n = node as ForceGraphNode
    const loc = n.startLine != null
      ? `${escapeHtml(n.fp ?? '')}:${n.startLine}${n.endLine != null ? `-${n.endLine}` : ''}`
      : escapeHtml(n.fp ?? n.name)
    const lang = n.language ? ` · ${escapeHtml(n.language)}` : ''
    const color = NODE_COLORS[n.type] ?? '#94a3b8'
    return `<div style="padding:6px 9px;background:#16161a;border:1px solid #2a2a2e;border-radius:8px;font-family:ui-sans-serif,system-ui;max-width:320px;">
      <div style="font-size:12px;font-weight:600;color:#e5e7eb;">${escapeHtml(n.name)}</div>
      <div style="font-size:10px;color:${color};margin-top:1px;">${n.type}</div>
      <div style="font-size:10px;color:#9ca3af;margin-top:3px;font-family:ui-monospace,monospace;">${loc}${lang}</div>
    </div>`
  }, [])

  const handleNodeClick = useCallback((node: object) => {
    setSelectedNode(node as ForceGraphNode)
  }, [])

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return EDGE_COLORS[l.type] ?? '#475569'
  }, [])

  const nodeColor = useCallback((node: object) => {
    const n = node as ForceGraphNode
    return NODE_COLORS[n.type] ?? '#94a3b8'
  }, [])

  if (indexedProjects.length === 0) {
    return (
      <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
        <Share2 className="w-6 h-6 text-text-quaternary/50 mx-auto" />
        <p className="text-xs font-semibold text-text-primary">No indexed repositories yet.</p>
        <p className="text-xs text-text-quaternary">
          Index a repository in the Repositories tab to explore its code graph.
        </p>
      </div>
    )
  }

  const externalCount = graph?.nodes.filter(n => n.type === 'External').length ?? 0
  const showExternalToggle = externalCount > 0

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
          {indexedProjects.map(p => (
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
                style={visibleTypes.has(type) ? { backgroundColor: NODE_COLORS[type] ?? '#94a3b8' } : undefined}
                aria-pressed={visibleTypes.has(type)}
                aria-label={`Toggle ${type} nodes`}
              >
                {type}
              </button>
            ))}

            {showExternalToggle && visibleTypes.has('External') && externalCount > EXTERNAL_COLLAPSE_THRESHOLD && (
              <button
                type="button"
                onClick={() => setExpandExternals(v => !v)}
                className="px-2 py-0.5 rounded-full text-[10px] border border-border-primary text-text-secondary hover:text-text-primary transition-colors"
              >
                {expandExternals ? 'Collapse externals' : `Expand ${externalCount} externals`}
              </button>
            )}
          </div>
        )}
      </div>

      {!selectedProject && (
        <div className="border border-border-primary rounded-[18px] p-10 text-center">
          <Share2 className="w-5 h-5 text-text-quaternary/40 mx-auto mb-2" />
          <p className="text-xs text-text-quaternary">Select a project to visualize its code graph.</p>
        </div>
      )}

      {selectedProject && isLoading && (
        <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
          <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
        </div>
      )}

      {selectedProject && isError && (
        <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
          {(error as Error)?.message ?? 'Failed to load graph.'}
        </div>
      )}

      {selectedProject && !isLoading && !isError && graph && (
        graph.node_count === 0 ? (
          <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
            <Share2 className="w-6 h-6 text-text-quaternary/40 mx-auto" />
            <p className="text-xs font-semibold text-text-secondary">No graph data</p>
            <p className="text-xs text-text-quaternary">
              This project has no indexed symbols yet. Try re-indexing it.
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
              <span className="ml-auto text-text-quaternary/70">hover for info · click a node for source · drag to rotate</span>
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
                    <p className="text-sm font-semibold text-text-primary truncate">{selectedNode.name}</p>
                    <div className="flex items-center gap-2 mt-0.5">
                      <span
                        className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full text-white"
                        style={{ backgroundColor: NODE_COLORS[selectedNode.type] ?? '#94a3b8' }}
                      >
                        {selectedNode.type}
                      </span>
                      {selectedNode.language && (
                        <span className="text-[10px] text-text-quaternary">{selectedNode.language}</span>
                      )}
                    </div>
                    {selectedNode.fp && (
                      <p className="text-[10px] text-text-quaternary font-mono mt-1 truncate">
                        {selectedNode.fp}
                        {selectedNode.startLine != null && `:${selectedNode.startLine}`}
                        {selectedNode.endLine != null && `-${selectedNode.endLine}`}
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

                <div className="flex-1 overflow-auto">
                  {!hasSource && (
                    <p className="text-xs text-text-quaternary p-4">
                      {selectedNode.type} node — no file source. Click a File or a code symbol
                      (Function, Method, Class…) to view code.
                    </p>
                  )}
                  {hasSource && snippetLoading && (
                    <div className="flex items-center justify-center py-10">
                      <Loader2 className="w-4 h-4 animate-spin text-text-quaternary" />
                    </div>
                  )}
                  {hasSource && snippetError && (
                    <p className="text-xs text-status-error/80 p-4">
                      {(snippetErr as Error)?.message ?? 'No source found.'}
                    </p>
                  )}
                  {hasSource && snippet && (
                    <pre className="text-[11px] leading-relaxed text-text-secondary font-mono p-4 whitespace-pre">
                      <code>{snippet.content}</code>
                    </pre>
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
