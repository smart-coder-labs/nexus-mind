import React, { useMemo, useState, useCallback, lazy, Suspense } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { downloadExport, todayStamp } from '../lib/download'
import type { AuditFilters } from '../types'
import { ChevronLeft, ChevronRight, Download, X, Share2, List } from 'lucide-react'
import { ActivityTimeline } from '../components/ActivityTimeline'
import { usePersistedGraphState } from '../hooks/usePersistedGraphState'

const AUDIT_VIEW_KEY = 'nexusmind-audit-view'

const OrgMemoryGraph = lazy(() => import('../components/OrgMemoryGraph'))

function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  React.useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delay)
    return () => clearTimeout(t)
  }, [value, delay])
  return debounced
}

const PAGE_SIZE = 50

const ACTION_TYPES = [
  'memory.created', 'memory.updated', 'memory.deleted', 'memory.archived',
  'convention.created', 'convention.updated', 'convention.deleted',
  'user.invited', 'user.disabled', 'user.enabled',
  'api_key.created', 'api_key.revoked',
  'webhook.created', 'webhook.deleted',
  // legacy simple actions
  'store', 'search', 'delete', 'invite', 'revoke',
]

export default function AuditLog() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const [page, setPage] = useState(0)
  const [filters, setFilters] = useState<Omit<AuditFilters, 'limit' | 'offset'>>({})
  const [draft, setDraft] = useState({ user_id: '', action: '', resource_type: '', from: '', to: '' })
  const [searchRaw, setSearchRaw] = useState('')
  const debouncedSearch = useDebounce(searchRaw, 300)

  const { data: users } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    staleTime: 60_000,
  })

  const userMap = useMemo(() => {
    const m = new Map<string, string>()
    users?.forEach(u => m.set(u.id, u.name))
    return m
  }, [users])

  const { data: entries, isLoading, isError: entriesError } = useQuery({
    queryKey: ['audit', filters, debouncedSearch, page],
    queryFn: () => client.getAuditLog({
      ...filters,
      ...(debouncedSearch ? { search: debouncedSearch } : {}),
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
    }),
  })

  const applyFilters = () => {
    const clean: Omit<AuditFilters, 'limit' | 'offset'> = {}
    if (draft.user_id)       clean.user_id       = draft.user_id
    if (draft.action)        clean.action        = draft.action
    if (draft.resource_type) clean.resource_type = draft.resource_type
    if (draft.from)          clean.from          = draft.from
    if (draft.to)            clean.to            = draft.to
    setFilters(clean)
    setPage(0)
  }

  const clearFilters = () => {
    setDraft({ user_id: '', action: '', resource_type: '', from: '', to: '' })
    setFilters({})
    setSearchRaw('')
    setPage(0)
  }

  // Persist the list/graph view toggle so reloading keeps the user where
  // they were last. Shared hook handles localStorage + graceful fallback
  // for missing/corrupt values.
  const [viewMode, setViewMode] = usePersistedGraphState<'list' | 'graph'>(
    AUDIT_VIEW_KEY,
    'list',
    { validate: v => v === 'list' || v === 'graph' },
  )
  const [exporting, setExporting] = useState(false)
  const [exportingServer, setExportingServer] = useState(false)


  const handleExportCsv = useCallback(async () => {
    setExporting(true)
    try {
      const qs = new URLSearchParams({ format: 'csv' })
      Object.entries(filters).forEach(([k, v]) => v && qs.set(k, String(v)))
      if (debouncedSearch) qs.set('search', debouncedSearch)
      const base = import.meta.env.VITE_API_URL ?? ''
      await downloadExport(`${base}/v1/audit/export?${qs}`, `audit-${todayStamp()}.csv`)
    } finally {
      setExporting(false)
    }
  }, [filters, debouncedSearch])

  const handleExportServer = useCallback(async () => {
    setExportingServer(true)
    try {
      const blob = await client.exportAuditLog({
        ...filters,
        ...(debouncedSearch ? { search: debouncedSearch } : {}),
      })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `audit-log-${todayStamp()}.json`
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    } finally {
      setExportingServer(false)
    }
  }, [filters, debouncedSearch, client])

  const setField = (field: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setDraft(d => ({ ...d, [field]: e.target.value }))

  const inputCls = 'bg-white/[0.04] border border-border-primary rounded-[8px] px-3 py-2 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors'
  const selectCls = 'appearance-none bg-white/[0.04] border border-border-primary rounded-[8px] pl-3 pr-8 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer'

  const SelectWrapper = ({ children }: { children: React.ReactNode }) => (
    <div className="relative">
      {children}
      <svg className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-quaternary" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
      </svg>
    </div>
  )

  const pagination = (entries?.length === PAGE_SIZE || page > 0) ? (
    <div className="flex items-center justify-between pt-4 mt-1 border-t border-border-primary">
      <p className="text-xs text-text-tertiary">Page {page + 1}</p>
      <div className="flex gap-2">
        <button
          onClick={() => setPage(p => Math.max(0, p - 1))}
          disabled={page === 0}
          className="p-1.5 rounded-full text-text-tertiary hover:text-text-secondary hover:bg-white/[0.04] disabled:opacity-20 transition-colors"
          aria-label="Previous page"
        >
          <ChevronLeft className="w-4 h-4" />
        </button>
        <button
          onClick={() => setPage(p => p + 1)}
          disabled={!entries || entries.length < PAGE_SIZE}
          className="p-1.5 rounded-full text-text-tertiary hover:text-text-secondary hover:bg-white/[0.04] disabled:opacity-20 transition-colors"
          aria-label="Next page"
        >
          <ChevronRight className="w-4 h-4" />
        </button>
      </div>
    </div>
  ) : null

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-base font-semibold text-text-primary">Audit Log</h1>
          <p className="text-xs text-text-quaternary mt-0.5">All actions performed in your organization</p>
        </div>
        <div className="flex items-center gap-2">
          {/* View toggle */}
          <div className="flex items-center gap-0.5 bg-white/[0.04] rounded-full p-0.5">
            <button
              type="button"
              onClick={() => setViewMode('list')}
              className={`flex items-center gap-1 px-2.5 py-1 rounded-full text-xs transition-colors ${
                viewMode === 'list'
                  ? 'bg-white/[0.08] text-text-primary'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
              aria-label="List view"
            >
              <List className="w-3 h-3" /> List
            </button>
            <button
              type="button"
              onClick={() => setViewMode('graph')}
              className={`flex items-center gap-1 px-2.5 py-1 rounded-full text-xs transition-colors ${
                viewMode === 'graph'
                  ? 'bg-white/[0.08] text-text-primary'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
              aria-label="Graph view"
            >
              <Share2 className="w-3 h-3" /> Graph
            </button>
          </div>

          {session?.user.role === 'admin' && (
            <>
              <button
                onClick={handleExportCsv}
                disabled={exporting || exportingServer}
                className="border border-border-primary rounded-full px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors disabled:opacity-40"
                aria-label="Export audit log as CSV"
              >
                <Download className="w-3 h-3" />
                {exporting ? 'Exporting…' : 'Export CSV'}
              </button>
              <button
                onClick={handleExportServer}
                disabled={exporting || exportingServer}
                className="border border-border-primary rounded-full px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors disabled:opacity-40"
                aria-label="Export audit log via API"
              >
                <Download className="w-3 h-3" />
                {exportingServer ? 'Exporting…' : 'Export'}
              </button>
            </>
          )}
        </div>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap gap-2 items-end">
        <input
          type="text"
          placeholder="Search actions, resources…"
          value={searchRaw}
          onChange={e => setSearchRaw(e.target.value)}
          className="bg-white/[0.04] border border-border-primary rounded-[8px] text-xs text-text-primary placeholder:text-text-quaternary px-3 py-1.5 focus:border-accent-blue/60 focus:outline-none w-48"
        />
        <SelectWrapper>
          <select value={draft.user_id} onChange={setField('user_id')} className={selectCls}>
            <option value="">All users</option>
            {users?.map(u => <option key={u.id} value={u.id}>{u.name}</option>)}
          </select>
        </SelectWrapper>
        <SelectWrapper>
          <select value={draft.action} onChange={setField('action')} className={selectCls}>
            <option value="">All actions</option>
            {ACTION_TYPES.map(a => (
              <option key={a} value={a}>{a}</option>
            ))}
          </select>
        </SelectWrapper>
        <SelectWrapper>
          <select value={draft.resource_type} onChange={setField('resource_type')} className={selectCls}>
            <option value="">All resources</option>
            {['memory', 'user', 'org'].map(r => <option key={r} value={r}>{r}</option>)}
          </select>
        </SelectWrapper>
        <label className="flex flex-col gap-1">
          <span className="text-[10px] text-text-quaternary px-0.5">From</span>
          <input type="date" value={draft.from} onChange={setField('from')} className={inputCls} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[10px] text-text-quaternary px-0.5">To</span>
          <input type="date" value={draft.to}   onChange={setField('to')}   className={inputCls} />
        </label>
        <button
          onClick={applyFilters}
          className="px-3 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-semibold transition-colors"
        >
          Apply
        </button>
        {(Object.values(filters).some(Boolean) || searchRaw) && (
          <button
            onClick={clearFilters}
            className="flex items-center gap-1 border border-border-primary rounded-full px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary transition-colors"
          >
            <X className="w-3 h-3" />
            Clear
          </button>
        )}
      </div>

      {/* Query error */}
      {entriesError && (
        <p className="text-xs text-status-error text-center py-8">Failed to load audit log. Please refresh.</p>
      )}

      {/* Timeline (same design as the dashboard Recent Activity) */}
      {!entriesError && viewMode === 'list' && (
        <div className="bg-[#272729] border border-white/[0.06] rounded-[18px] p-5">
          <ActivityTimeline
            entries={entries}
            userMap={userMap}
            isLoading={isLoading}
            footer={pagination}
            emptyTitle="No audit events found"
            emptyDescription={(Object.values(filters).some(Boolean) || debouncedSearch)
              ? 'No events match the current filters. Try adjusting or clearing them.'
              : 'Actions performed in your organization will appear here as they happen.'}
          />
        </div>
      )}

      {/* Graph view — org-wide memory graph (all projects merged) */}
      {!entriesError && viewMode === 'graph' && (
        <Suspense fallback={
          <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
            <div className="w-5 h-5 animate-spin rounded-full border-2 border-text-quaternary border-t-transparent" />
          </div>
        }>
          <OrgMemoryGraph storageKey="audit" height={500} />
        </Suspense>
      )}
    </div>
  )
}
