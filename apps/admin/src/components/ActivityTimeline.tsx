import { useState } from 'react'
import { ChevronRight, FolderOpen } from 'lucide-react'
import { Badge } from '@/components/ui/Badge/Badge'
import { Skeleton } from '@/components/ui/Skeleton/Skeleton'
import { EmptyState } from '@/components/ui/EmptyState/EmptyState'
import { cn } from '@/lib/utils'
import type { AuditEntry } from '@/types'

// --- Timeline helpers (shared by the dashboard Recent Activity and the Audit Log) ---

function relativeTime(isoString: string): string {
  const diff = Date.now() - new Date(isoString).getTime()
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

function timelineActionVariant(action: string): 'primary' | 'success' | 'error' | 'warning' | 'default' {
  const a = action.split('.').pop() ?? action
  if (a === 'store' || a === 'create' || a === 'created' || a === 'invite') return 'success'
  if (a === 'search' || a === 'query') return 'primary'
  if (a === 'delete' || a === 'deleted' || a === 'revoke' || a === 'remove') return 'error'
  if (a === 'update' || a === 'updated' || a === 'edit') return 'warning'
  return 'default'
}

function timelineDotClass(action: string): string {
  const variant = timelineActionVariant(action)
  const map: Record<string, string> = {
    success: 'bg-status-success',
    primary: 'bg-accent-blue',
    error: 'bg-status-error',
    warning: 'bg-status-warning',
    default: 'bg-white/[0.20]',
  }
  return map[variant]
}

function formatAbsTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function dayLabel(iso: string): string {
  const d = new Date(iso)
  const today = new Date()
  const yesterday = new Date(today)
  yesterday.setDate(today.getDate() - 1)
  if (d.toDateString() === today.toDateString()) return 'Today'
  if (d.toDateString() === yesterday.toDateString()) return 'Yesterday'
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

interface ActivityTimelineProps {
  entries: AuditEntry[] | undefined
  userMap: Map<string, string>
  isLoading?: boolean
  /** Rendered below the timeline (e.g. a "Show more" / pagination control). */
  footer?: React.ReactNode
  emptyTitle?: string
  emptyDescription?: string
}

/**
 * Audit-event timeline shared by the dashboard "Recent Activity" card and the
 * Audit Log page. Groups entries by day, collapses bursts of identical events
 * into a single "× N" row, and expands rich search/store events into a tree.
 */
export function ActivityTimeline({
  entries,
  userMap,
  isLoading,
  footer,
  emptyTitle = 'No activity yet',
  emptyDescription = 'Actions performed by your team will appear here.',
}: ActivityTimelineProps) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const toggle = (id: string) =>
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  if (isLoading) {
    return (
      <div className="space-y-4">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="flex gap-3">
            <Skeleton className="w-[15px] h-[15px] rounded-full mt-0.5 shrink-0" />
            <Skeleton className="h-9 flex-1 rounded-[8px]" />
          </div>
        ))}
      </div>
    )
  }

  if (!entries || entries.length === 0) {
    return (
      <div className="py-8">
        <EmptyState title={emptyTitle} description={emptyDescription} />
      </div>
    )
  }

  // Group entries by calendar day
  const groups: { label: string; entries: AuditEntry[] }[] = []
  const seen = new Map<string, AuditEntry[]>()
  for (const entry of entries) {
    const label = dayLabel(entry.timestamp)
    if (!seen.has(label)) seen.set(label, [])
    seen.get(label)!.push(entry)
  }
  for (const [label, dayEntries] of seen) {
    groups.push({ label, entries: dayEntries })
  }

  type E = AuditEntry
  const meta = (e: E) => (e.metadata ?? {}) as Record<string, unknown>
  const isRich = (e: E) => {
    const a = e.action.toLowerCase()
    if (a.includes('search')) return Array.isArray(meta(e).results) || typeof meta(e).query === 'string'
    if (a.includes('store')) return typeof meta(e).preview === 'string' || typeof meta(e).title === 'string'
    return false
  }

  return (
    <>
      <div className="space-y-5">
        {groups.map(({ label, entries: dayEntries }) => (
          <div key={label}>
            <p className="text-[10px] font-semibold text-text-quaternary uppercase tracking-wider mb-3 pl-5">
              {label}
            </p>
            <div className="relative">
              {/* Vertical connector line */}
              <div className="absolute left-[7px] top-2 bottom-2 w-px bg-border-primary" aria-hidden="true" />
              <ul className="space-y-2.5">
                {(() => {
                  type Item =
                    | { kind: 'run'; entry: E; count: number; lastTimestamp: string }
                    | { kind: 'detail'; entry: E }
                  const items: Item[] = []
                  for (const entry of dayEntries) {
                    if (isRich(entry)) {
                      items.push({ kind: 'detail', entry })
                      continue
                    }
                    const last = items[items.length - 1]
                    if (
                      last && last.kind === 'run' &&
                      last.entry.user_id === entry.user_id &&
                      last.entry.action === entry.action &&
                      last.entry.resource_type === entry.resource_type &&
                      typeof (entry.metadata as Record<string, unknown>)?.description !== 'string'
                    ) {
                      last.count += 1
                      last.lastTimestamp = entry.timestamp
                    } else {
                      items.push({ kind: 'run', entry, count: 1, lastTimestamp: entry.timestamp })
                    }
                  }
                  return items.map((item) => {
                    const entry = item.entry
                    const displayName =
                      userMap.get(entry.user_id) ??
                      (entry.metadata?.user_email as string | undefined) ??
                      'System'
                    const variant = timelineActionVariant(entry.action)
                    const actionLabel = entry.action.split('.').pop() ?? entry.action
                    const dot = (
                      <div
                        className={cn(
                          'w-[15px] h-[15px] rounded-full shrink-0 mt-0.5 ring-2 ring-[#272729] relative z-10',
                          timelineDotClass(entry.action)
                        )}
                        aria-hidden="true"
                      />
                    )

                    if (item.kind === 'detail') {
                      const md = (entry.metadata ?? {}) as Record<string, unknown>
                      const isSearch = entry.action.toLowerCase().includes('search')
                      const query = typeof md.query === 'string' ? md.query : null
                      const resultCount = typeof md.result_count === 'number' ? md.result_count : null
                      const results = (Array.isArray(md.results) ? md.results : []) as {
                        id?: string; title?: string; project?: string; type?: string
                      }[]
                      const byProject = new Map<string, typeof results>()
                      for (const r of results) {
                        const p = r.project || 'unknown'
                        if (!byProject.has(p)) byProject.set(p, [])
                        byProject.get(p)!.push(r)
                      }
                      const project = typeof md.project === 'string' ? md.project : null
                      const title = typeof md.title === 'string' ? md.title : null
                      const memType = typeof md.type === 'string' ? md.type : null
                      const tags = Array.isArray(md.tags) ? (md.tags as string[]) : []
                      const preview = typeof md.preview === 'string' ? md.preview : null
                      const open = expanded.has(entry.id)
                      return (
                        <li key={entry.id} className="flex items-start gap-3">
                          {dot}
                          <div className="flex-1 min-w-0 max-w-2xl">
                            <button
                              type="button"
                              onClick={() => toggle(entry.id)}
                              aria-expanded={open}
                              className="w-full flex items-baseline justify-between gap-3 text-left"
                            >
                              <span className="text-xs text-text-primary leading-snug flex items-center flex-wrap gap-1 min-w-0">
                                <ChevronRight className={cn('w-3 h-3 text-text-quaternary transition-transform shrink-0', open && 'rotate-90')} />
                                {displayName !== 'System' && <span className="font-semibold">{displayName}</span>}
                                <Badge variant={variant} size="sm">{actionLabel}</Badge>
                                <span className="text-text-secondary">{entry.resource_type}</span>
                                {isSearch && query && <span className="text-text-primary truncate">“{query}”</span>}
                                {isSearch && resultCount != null && (
                                  <span className="text-[10px] text-text-quaternary tabular-nums">· {resultCount} result{resultCount === 1 ? '' : 's'}</span>
                                )}
                                {!isSearch && project && (
                                  <span className="text-text-quaternary">in <span className="text-text-secondary">{project}</span></span>
                                )}
                                {!isSearch && title && <span className="text-text-primary truncate">— {title}</span>}
                              </span>
                              <time dateTime={entry.timestamp} className="shrink-0 text-[10px] text-text-quaternary tabular-nums" title={formatAbsTime(entry.timestamp)}>
                                {relativeTime(entry.timestamp)}
                              </time>
                            </button>
                            {open && (
                              <div className="mt-2 ml-1.5 pl-3 border-l border-border-primary space-y-1.5 text-[11px]">
                                {isSearch ? (
                                  <>
                                    {query && <p className="text-text-quaternary">query: <span className="text-text-secondary">“{query}”</span></p>}
                                    {results.length > 0 ? (
                                      <div className="space-y-1.5">
                                        <p className="text-text-quaternary">returned:</p>
                                        {[...byProject.entries()].map(([proj, rs]) => (
                                          <div key={proj} className="ml-1">
                                            <p className="text-text-secondary flex items-center gap-1">
                                              <FolderOpen className="w-3 h-3 shrink-0" />
                                              <span className="truncate">{proj}</span>
                                              <span className="text-text-quaternary">({rs.length})</span>
                                            </p>
                                            <ul className="ml-4 mt-0.5 space-y-0.5">
                                              {rs.map((r, i) => (
                                                <li key={r.id ?? i} className="text-text-quaternary truncate">• {r.title || r.id}</li>
                                              ))}
                                            </ul>
                                          </div>
                                        ))}
                                      </div>
                                    ) : (
                                      <p className="text-text-quaternary">no results captured for this search</p>
                                    )}
                                  </>
                                ) : (
                                  <>
                                    {project && (
                                      <p className="text-text-quaternary flex items-center gap-1">
                                        <FolderOpen className="w-3 h-3 shrink-0" />
                                        <span className="text-text-secondary truncate">{project}</span>
                                      </p>
                                    )}
                                    {title && <p className="text-text-quaternary">title: <span className="text-text-secondary">{title}</span></p>}
                                    {memType && <p className="text-text-quaternary">type: <span className="text-text-secondary">{memType}</span></p>}
                                    {tags.length > 0 && (
                                      <div className="flex flex-wrap gap-1 items-center">
                                        <span className="text-text-quaternary">tags:</span>
                                        {tags.map(t => (
                                          <span key={t} className="px-1.5 py-0.5 rounded-full bg-white/[0.06] text-text-secondary">{t}</span>
                                        ))}
                                      </div>
                                    )}
                                    {preview && (
                                      <p className="text-text-quaternary line-clamp-3 whitespace-pre-wrap">{preview}</p>
                                    )}
                                  </>
                                )}
                              </div>
                            )}
                          </div>
                        </li>
                      )
                    }

                    const { count, lastTimestamp } = item
                    return (
                      <li key={entry.id} className="flex items-start gap-3 group">
                        {dot}
                        <div className="flex-1 min-w-0 flex items-baseline justify-between gap-3 max-w-2xl">
                          <p className="text-xs text-text-primary leading-snug flex items-center flex-wrap gap-1 min-w-0">
                            {displayName !== 'System' && <span className="font-semibold">{displayName}</span>}
                            <Badge variant={variant} size="sm">{actionLabel}</Badge>
                            {entry.resource_type && <span className="text-text-secondary">{entry.resource_type}</span>}
                            {count > 1 && (
                              <span className="text-[10px] font-semibold text-text-quaternary tabular-nums">×{count}</span>
                            )}
                            {typeof entry.metadata?.description === 'string' && (
                              <span className="text-text-quaternary truncate">— {entry.metadata.description}</span>
                            )}
                          </p>
                          <time dateTime={entry.timestamp} className="shrink-0 text-[10px] text-text-quaternary tabular-nums" title={formatAbsTime(entry.timestamp)}>
                            {count > 1
                              ? `${relativeTime(lastTimestamp)} – ${relativeTime(entry.timestamp)}`
                              : relativeTime(entry.timestamp)}
                          </time>
                        </div>
                      </li>
                    )
                  })
                })()}
              </ul>
            </div>
          </div>
        ))}
      </div>
      {footer}
    </>
  )
}
