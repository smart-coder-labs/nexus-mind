import React, { useMemo, useState, useCallback, useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { todayStamp } from '../lib/download'
import type { AuditFilters, AuditEntry } from '../types'
import { ChevronLeft, ChevronRight, ChevronDown, ChevronUp, Download, ScrollText, Layers, X } from 'lucide-react'

function highlightMatch(text: string, query: string): React.ReactNode {
  if (!query || query.length < 2) return text
  const parts = text.split(new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi'))
  return parts.map((part, i) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark key={i} className="bg-accent-blue/20 text-accent-blue rounded-[2px] px-0.5">{part}</mark>
    ) : part
  )
}

function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
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

const ACTION_COLORS: Record<string, string> = {
  store:  'text-accent-blue',
  search: 'text-text-tertiary',
  delete: 'text-status-error',
  invite: 'text-status-success',
  revoke: 'text-status-warning',
}

function actionClass(action: string) {
  return ACTION_COLORS[action] ?? 'text-text-tertiary'
}

export default function AuditLog() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const [page, setPage] = useState(0)
  const [filters, setFilters] = useState<Omit<AuditFilters, 'limit' | 'offset'>>({})
  const [draft, setDraft] = useState({ user_id: '', action: '', resource_type: '', from: '', to: '' })
  const [searchRaw, setSearchRaw] = useState('')
  const debouncedSearch = useDebounce(searchRaw, 300)
  const [groupBySessions, setGroupBySessions] = useState(false)
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())

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

  // Group entries by user_id + date for session grouping view
  const groupedEntries = useMemo(() => {
    if (!entries || !groupBySessions) return null
    const groups = new Map<string, { key: string; label: string; entries: AuditEntry[] }>()
    for (const entry of entries) {
      const date = entry.timestamp.slice(0, 10)
      const key = `${entry.user_id}::${date}`
      if (!groups.has(key)) {
        const userName = userMap.get(entry.user_id) ?? entry.user_id.slice(0, 8)
        groups.set(key, { key, label: `${userName} — ${date}`, entries: [] })
      }
      groups.get(key)!.entries.push(entry)
    }
    return Array.from(groups.values())
  }, [entries, groupBySessions, userMap])

  const toggleGroup = useCallback((key: string) => {
    setCollapsedGroups(prev => {
      const next = new Set(prev)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }, [])

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

  const [exporting, setExporting] = useState(false)
  const [exportingServer, setExportingServer] = useState(false)

  const handleExportCsv = useCallback(async () => {
    setExporting(true)
    try {
      const all = await client.getAuditLog({ ...filters, limit: 5000, offset: 0 })

      const escape = (val: unknown): string => {
        const s = typeof val === 'object' && val !== null ? JSON.stringify(val) : String(val ?? '')
        return s.includes(',') || s.includes('"') || s.includes('\n')
          ? `"${s.replace(/"/g, '""')}"`
          : s
      }

      const header = 'timestamp,user_id,action,resource_type,resource_id,details'
      const rows = all.map((e: AuditEntry) =>
        [
          escape(e.timestamp),
          escape(e.user_id),
          escape(e.action),
          escape(e.resource_type),
          escape(e.resource_id ?? ''),
          escape(e.metadata),
        ].join(',')
      )

      const csv = [header, ...rows].join('\n')
      const blob = new Blob([csv], { type: 'text/csv' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `audit-log-${todayStamp()}.csv`
      a.click()
      URL.revokeObjectURL(url)
    } finally {
      setExporting(false)
    }
  }, [filters, client])

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

  const inputCls = 'bg-white/[0.04] border border-border-secondary/40 rounded-[8px] px-3 py-2 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors'
  const selectCls = 'appearance-none bg-white/[0.04] border border-border-secondary/40 rounded-[8px] pl-3 pr-8 py-2 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer'

  const SelectWrapper = ({ children }: { children: React.ReactNode }) => (
    <div className="relative">
      {children}
      <svg className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-quaternary" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
      </svg>
    </div>
  )

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-[34px] font-semibold text-text-primary tracking-[-0.374px]">Audit Log</h1>
          <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">All actions performed in your organization</p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setGroupBySessions(g => !g)}
            className={`border border-border-primary rounded-full px-2.5 py-1.5 text-xs flex items-center gap-1.5 transition-colors ${
              groupBySessions
                ? 'bg-white/[0.06] text-text-primary'
                : 'text-text-secondary hover:text-text-primary'
            }`}
            aria-label={groupBySessions ? 'Disable session grouping' : 'Group by user and date'}
          >
            <Layers className="w-3.5 h-3.5" />
            {groupBySessions ? 'Grouped' : 'Group by session'}
          </button>
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
          className="bg-white/[0.04] border border-border-secondary/40 rounded-[8px] text-xs text-text-secondary placeholder:text-text-quaternary px-3 py-1.5 focus:border-accent-blue/60 focus:outline-none w-48"
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
          <span className="text-[10px] text-text-tertiary px-0.5">From</span>
          <input type="date" value={draft.from} onChange={setField('from')} className={inputCls} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[10px] text-text-tertiary px-0.5">To</span>
          <input type="date" value={draft.to}   onChange={setField('to')}   className={inputCls} />
        </label>
        <button
          onClick={applyFilters}
          className="px-3 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold transition-colors"
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
        <p className="text-sm text-status-error text-center py-8">Failed to load audit log. Please refresh.</p>
      )}

      {/* Table */}
      <div className="border border-border-primary rounded-[18px] overflow-hidden overflow-x-auto">
        <table className="w-full text-sm min-w-[620px]">
          <thead>
            <tr className="border-b border-border-secondary">
              {['Timestamp', 'User', 'Action', 'Resource', 'ID'].map(h => (
                <th key={h} className="text-left px-4 py-3 text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-border-secondary">
            {isLoading
              ? Array.from({ length: 8 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 5 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded-[5px] bg-[#272729] animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : groupBySessions && groupedEntries
              ? groupedEntries.flatMap(group => {
                  const isCollapsed = collapsedGroups.has(group.key)
                  return [
                    <tr
                      key={`group-${group.key}`}
                      className="bg-[#1d1d1f] border-t border-border-primary cursor-pointer hover:bg-[#272729] transition-colors"
                      onClick={() => toggleGroup(group.key)}
                    >
                      <td colSpan={5} className="px-4 py-2.5">
                        <div className="flex items-center gap-2">
                          {isCollapsed
                            ? <ChevronDown className="w-3.5 h-3.5 text-text-quaternary" />
                            : <ChevronUp className="w-3.5 h-3.5 text-text-quaternary" />
                          }
                          <span className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">{group.label}</span>
                          <span className="text-[10px] text-text-secondary bg-white/[0.06] border border-border-primary rounded-[5px] px-1.5 py-0.5">
                            {group.entries.length} events
                          </span>
                        </div>
                      </td>
                    </tr>,
                    ...(!isCollapsed ? group.entries.map(entry => (
                      <tr key={entry.id} className="hover:bg-white/[0.02] transition-colors duration-150">
                        <td className="px-4 py-3 text-[10px] text-text-quaternary whitespace-nowrap">
                          <div className="border-l-2 border-l-accent-blue/20 ml-4 pl-4">
                            {new Date(entry.timestamp).toLocaleString()}
                          </div>
                        </td>
                        <td className="px-4 py-3 text-xs text-text-secondary">
                          {userMap.get(entry.user_id) ?? '—'}
                        </td>
                        <td className={`px-4 py-3 text-xs font-semibold ${actionClass(entry.action)}`}>
                          {entry.action}
                        </td>
                        <td className="px-4 py-3 text-xs text-text-tertiary">{entry.resource_type}</td>
                        <td className="px-4 py-3 text-xs text-text-quaternary font-mono truncate max-w-[120px]">
                          {entry.resource_id ?? '—'}
                        </td>
                      </tr>
                    )) : []),
                  ]
                })
              : entries?.map(entry => (
                <tr key={entry.id} className="hover:bg-white/[0.02] transition-colors duration-150">
                  <td className="px-4 py-3 text-[10px] text-text-quaternary whitespace-nowrap">
                    {new Date(entry.timestamp).toLocaleString()}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {userMap.get(entry.user_id) ?? '—'}
                  </td>
                  <td className={`px-4 py-3 text-xs font-semibold ${actionClass(entry.action)}`}>
                    {highlightMatch(entry.action, debouncedSearch)}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-tertiary">
                    {highlightMatch(entry.resource_type, debouncedSearch)}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-quaternary font-mono truncate max-w-[120px]">
                    {entry.resource_id ?? '—'}
                  </td>
                </tr>
              ))
            }
          </tbody>
        </table>
        {!isLoading && !entriesError && (!entries || entries.length === 0) && (
          <div className="flex flex-col items-center gap-2 py-16 text-center">
            <ScrollText className="w-8 h-8 text-text-quaternary" />
            <p className="text-sm font-semibold text-text-primary">No audit events found</p>
            <p className="text-xs text-text-tertiary max-w-xs">
              {(Object.values(filters).some(Boolean) || debouncedSearch)
                ? 'No events match the current filters. Try adjusting or clearing them.'
                : 'Actions performed in your organization will appear here as they happen.'}
            </p>
          </div>
        )}
      </div>

      {/* Pagination */}
      {(entries?.length === PAGE_SIZE || page > 0) && (
        <div className="flex items-center justify-between">
          <p className="text-xs text-text-tertiary">Page {page + 1}</p>
          <div className="flex gap-2">
            <button
              onClick={() => setPage(p => Math.max(0, p - 1))}
              disabled={page === 0}
              className="p-1.5 rounded-[5px] text-text-tertiary hover:text-text-secondary hover:bg-[#272729] disabled:opacity-20 transition-colors"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
            <button
              onClick={() => setPage(p => p + 1)}
              disabled={!entries || entries.length < PAGE_SIZE}
              className="p-1.5 rounded-[5px] text-text-tertiary hover:text-text-secondary hover:bg-[#272729] disabled:opacity-20 transition-colors"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
