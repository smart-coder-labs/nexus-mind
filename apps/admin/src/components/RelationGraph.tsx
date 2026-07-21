import { Fragment, useCallback, useState } from 'react'
import ForceGraph3D from 'react-force-graph-3d'
import { X } from 'lucide-react'

export interface RelationGraphNode {
  id: string
  type: string
  label: string
  // ForceGraph3D injects x/y/z at runtime
  x?: number
  y?: number
  z?: number
}

export interface RelationGraphLink {
  source: string
  target: string
  type: string
}

export interface RelationGraphProps {
  nodes: RelationGraphNode[]
  links: RelationGraphLink[]
  nodeColors: Record<string, string>
  edgeColors?: Record<string, string>
  onNodeClick?: (node: RelationGraphNode) => void
  height?: number
  stats?: { label: string; value: string | number }[]
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, c => (
    c === '&' ? '&amp;' : c === '<' ? '&lt;' : c === '>' ? '&gt;' : '&quot;'
  ))
}

export function RelationGraph({
  nodes,
  links,
  nodeColors,
  edgeColors = {},
  onNodeClick,
  height = 500,
  stats = [],
}: RelationGraphProps) {
  const [selectedNode, setSelectedNode] = useState<RelationGraphNode | null>(null)

  const nodeLabel = useCallback((node: object) => {
    const n = node as RelationGraphNode
    const color = nodeColors[n.type] ?? '#94a3b8'
    return `<div style="padding:6px 9px;background:rgba(17,19,25,0.95);backdrop-filter:blur(14px);border:1px solid rgba(255,255,255,0.10);border-radius:8px;font-family:ui-sans-serif,system-ui;max-width:280px;">
      <div style="font-size:12px;font-weight:600;color:#e5e7eb;">${escapeHtml(n.label)}</div>
      <div style="font-size:10px;color:${color};margin-top:1px;">${escapeHtml(n.type)}</div>
    </div>`
  }, [nodeColors])

  const nodeColor = useCallback((node: object) => {
    const n = node as RelationGraphNode
    return nodeColors[n.type] ?? '#94a3b8'
  }, [nodeColors])

  const linkColor = useCallback((link: object) => {
    const l = link as { type: string }
    return edgeColors[l.type] ?? '#475569'
  }, [edgeColors])

  const handleNodeClick = useCallback((node: object) => {
    const n = node as RelationGraphNode
    setSelectedNode(n)
    onNodeClick?.(n)
  }, [onNodeClick])

  return (
    <div
      className="relative border border-border-primary rounded-[18px] overflow-hidden"
      style={{ height }}
    >
      {/* Stats bar */}
      {stats.length > 0 && (
        <div className="flex items-center gap-3 px-4 py-2 border-b border-border-primary bg-white/[0.02] text-[10px] text-text-quaternary">
          {stats.map((s, i) => (
            <Fragment key={s.label}>
              {i > 0 && <span>·</span>}
              <span>{s.value} {s.label}</span>
            </Fragment>
          ))}
          <span className="ml-auto text-text-quaternary/70">hover for info · click to select · drag to rotate</span>
        </div>
      )}

      <ForceGraph3D
        graphData={{ nodes, links }}
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
      {selectedNode && onNodeClick && (
        <div className="absolute top-0 right-0 h-full w-[380px] max-w-[70%] bg-[#0f1117]/[0.94] border-l border-white/10 backdrop-blur-[22px] flex flex-col">
          <div className="flex items-start gap-2 px-4 py-3 border-b border-border-primary">
            <div className="min-w-0 flex-1">
              <p className="text-sm font-semibold text-text-primary truncate">{selectedNode.label}</p>
              <span
                className="text-[10px] font-semibold px-1.5 py-0.5 rounded-full text-white mt-1 inline-block"
                style={{ backgroundColor: nodeColors[selectedNode.type] ?? '#94a3b8' }}
              >
                {selectedNode.type}
              </span>
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
          <div className="flex-1 overflow-auto p-4">
            <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1">ID</p>
            <p className="text-xs text-text-secondary font-mono break-all">{selectedNode.id}</p>
          </div>
        </div>
      )}
    </div>
  )
}
