import { useState, useEffect, useCallback } from 'react'
import { ScrollText, RefreshCw, Search, X, AlertCircle } from 'lucide-react'
import { listAudit } from '../api/client'
import type { AuditEntry } from '../types'
import { cn } from '@/lib/utils'

const ACTION_COLORS: Record<string, string> = {
  store:    'bg-status-success/15 text-status-success',
  delete:   'bg-status-error/15 text-status-error',
  search:   'bg-status-info/15 text-status-info',
  invite:   'bg-accent-blue-tint text-accent-blue',
  revoke:   'bg-status-warning/15 text-status-warning',
  rotate:   'bg-accent-violet-tint text-accent-violet',
}

function ActionBadge({ action }: { action: string }) {
  return (
    <span className={cn('inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium', ACTION_COLORS[action] ?? 'bg-surface-secondary text-text-tertiary')}>
      {action}
    </span>
  )
}

const PAGE_SIZE = 50

export default function AuditLog() {
  const [entries, setEntries] = useState<AuditEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [loadingMore, setLoadingMore] = useState(false)
  const [hasMore, setHasMore] = useState(true)
  const [error, setError] = useState('')
  const [search, setSearch] = useState('')
  const [actionFilter, setActionFilter] = useState('')

  const fetchAudit = useCallback(() => {
    setLoading(true)
    setError('')
    setHasMore(true)
    listAudit({ limit: PAGE_SIZE, offset: 0 })
      .then(data => {
        setEntries(data)
        setHasMore(data.length === PAGE_SIZE)
      })
      .catch(err => setError(err.message ?? 'Failed to load audit log'))
      .finally(() => setLoading(false))
  }, [])

  const loadMore = () => {
    setLoadingMore(true)
    listAudit({ limit: PAGE_SIZE, offset: entries.length })
      .then(data => {
        setEntries(prev => [...prev, ...data])
        setHasMore(data.length === PAGE_SIZE)
      })
      .catch(err => setError(err.message ?? 'Failed to load more'))
      .finally(() => setLoadingMore(false))
  }

  useEffect(() => { fetchAudit() }, [fetchAudit])

  const filtered = entries.filter(e => {
    const matchSearch =
      !search ||
      e.action.includes(search.toLowerCase()) ||
      e.user_id.includes(search) ||
      e.resource_type.includes(search.toLowerCase())
    const matchAction = !actionFilter || e.action === actionFilter
    return matchSearch && matchAction
  })

  const uniqueActions = [...new Set(entries.map(e => e.action))].sort()

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6 animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-text-primary">Audit Log</h1>
          <p className="text-sm text-text-secondary mt-0.5">All events across all organizations</p>
        </div>
        <button
          id="audit-refresh"
          onClick={fetchAudit}
          disabled={loading}
          className={cn(
            'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors',
            loading && 'opacity-50 cursor-not-allowed',
          )}
        >
          <RefreshCw className={cn('w-3 h-3', loading && 'animate-spin')} />
          Refresh
        </button>
      </div>

      {error && (
        <div className="flex items-center gap-2 px-4 py-3 bg-status-error/10 border border-status-error/20 rounded-lg text-sm text-status-error">
          <AlertCircle className="w-4 h-4 flex-shrink-0" />
          {error}
        </div>
      )}

      {/* Filters */}
      <div className="flex items-center gap-3">
        <div className="relative flex-1">
          <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-text-quaternary" />
          <input
            type="text"
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder="Search by action, user, resource…"
            className="w-full bg-surface-primary border border-border-primary rounded-lg pl-10 pr-4 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
          />
          {search && (
            <button onClick={() => setSearch('')} className="absolute right-3 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-secondary">
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
        <select
          value={actionFilter}
          onChange={e => setActionFilter(e.target.value)}
          className="bg-surface-primary border border-border-primary rounded-lg px-3 py-2.5 text-sm text-text-primary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
        >
          <option value="">All actions</option>
          {uniqueActions.map(a => <option key={a} value={a}>{a}</option>)}
        </select>
      </div>

      {/* Table */}
      <div className="bg-surface-primary border border-border-primary rounded-xl overflow-hidden">
        <div className="grid grid-cols-[160px_100px_120px_90px_1fr] gap-4 px-5 py-3 border-b border-border-secondary">
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Timestamp</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Action</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Resource</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Org</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">User</span>
        </div>

        {loading ? (
          <div className="divide-y divide-border-secondary">
            {[...Array(8)].map((_, i) => (
              <div key={i} className="grid grid-cols-[160px_100px_120px_90px_1fr] gap-4 px-5 py-3.5 items-center">
                <div className="h-3 w-32 bg-surface-secondary animate-pulse rounded" />
                <div className="h-5 w-16 bg-surface-secondary animate-pulse rounded-full" />
                <div className="h-3 w-20 bg-surface-secondary animate-pulse rounded" />
                <div className="h-3 w-16 bg-surface-secondary animate-pulse rounded" />
                <div className="h-3 w-28 bg-surface-secondary animate-pulse rounded" />
              </div>
            ))}
          </div>
        ) : filtered.length === 0 ? (
          <div className="py-16 text-center">
            <ScrollText className="w-8 h-8 text-text-quaternary mx-auto mb-3" />
            <p className="text-sm text-text-tertiary">
              {search || actionFilter ? 'No events match your filters' : 'No audit events yet'}
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border-secondary">
            {filtered.map(entry => (
              <div key={entry.id} className="grid grid-cols-[160px_100px_120px_90px_1fr] gap-4 px-5 py-3 items-start text-xs hover:bg-surface-secondary/40 transition-colors">
                <span className="text-text-tertiary font-mono">
                  {new Date(entry.timestamp).toLocaleString()}
                </span>
                <ActionBadge action={entry.action} />
                <span className="text-text-secondary">
                  {entry.resource_type}
                  {entry.resource_id && (
                    <span className="block font-mono text-text-quaternary truncate">{entry.resource_id.slice(0, 8)}</span>
                  )}
                </span>
                <span className="font-mono text-text-quaternary truncate">{entry.org_id.slice(0, 8)}</span>
                <span className="text-text-tertiary font-mono truncate">{entry.user_id.slice(0, 8)}…</span>
              </div>
            ))}
          </div>
        )}
      </div>

      {!loading && filtered.length > 0 && (
        <div className="flex items-center justify-between">
          <p className="text-xs text-text-quaternary">
            Showing {filtered.length} of {entries.length} loaded events
          </p>
          {hasMore && (
            <button
              onClick={loadMore}
              disabled={loadingMore}
              className={cn(
                'text-xs px-3 py-1.5 rounded-lg border border-border-primary text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors',
                loadingMore && 'opacity-50 cursor-not-allowed',
              )}
            >
              {loadingMore ? 'Loading…' : 'Load more'}
            </button>
          )}
        </div>
      )}
    </div>
  )
}
