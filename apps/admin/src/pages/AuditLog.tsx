import { useMemo, useState, useCallback } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { AuditFilters } from '../types'
import { ChevronLeft, ChevronRight } from 'lucide-react'

const PAGE_SIZE = 50

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

  const { data: entries, isLoading } = useQuery({
    queryKey: ['audit', filters, page],
    queryFn: () => client.getAuditLog({
      ...filters,
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
    setPage(0)
  }

  const handleExportCsv = useCallback(() => {
    if (!entries) return
    const rows = [
      ['timestamp', 'user', 'action', 'resource_type', 'resource_id'],
      ...entries.map(e => [
        e.timestamp,
        userMap.get(e.user_id) ?? e.user_id,
        e.action,
        e.resource_type,
        e.resource_id ?? '',
      ]),
    ]
    const csv = rows.map(r => r.join(',')).join('\n')
    const a = document.createElement('a')
    a.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv' }))
    a.download = 'audit-log.csv'
    a.click()
  }, [entries, userMap])

  const setField = (field: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setDraft(d => ({ ...d, [field]: e.target.value }))

  const inputCls = 'bg-surface-primary border border-border-primary rounded-lg px-3 py-2 text-sm text-text-secondary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus transition-colors'
  const selectCls = 'appearance-none bg-surface-secondary border border-border-primary rounded-lg pl-3 pr-8 py-2 text-sm text-text-secondary focus:outline-none focus:border-border-focus transition-colors cursor-pointer'

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
          <h1 className="text-lg font-semibold text-text-primary">Audit Log</h1>
          <p className="text-[12px] text-text-tertiary mt-0.5">All actions performed in your organization</p>
        </div>
        <button
          onClick={handleExportCsv}
          disabled={!entries?.length}
          className="text-xs text-text-tertiary hover:text-text-secondary border border-border-primary rounded-lg px-3 py-1.5 hover:bg-surface-secondary transition-colors disabled:opacity-30"
        >
          Export CSV
        </button>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap gap-2 items-end">
        <SelectWrapper>
          <select value={draft.user_id} onChange={setField('user_id')} className={selectCls}>
            <option value="">All users</option>
            {users?.map(u => <option key={u.id} value={u.id}>{u.name}</option>)}
          </select>
        </SelectWrapper>
        <SelectWrapper>
          <select value={draft.action} onChange={setField('action')} className={selectCls}>
            <option value="">All actions</option>
            {['store', 'search', 'delete', 'invite', 'revoke'].map(a => (
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
          <span className="text-[10px] text-text-tertiary uppercase tracking-wide px-0.5">From</span>
          <input type="date" value={draft.from} onChange={setField('from')} className={inputCls} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[10px] text-text-tertiary uppercase tracking-wide px-0.5">To</span>
          <input type="date" value={draft.to}   onChange={setField('to')}   className={inputCls} />
        </label>
        <button
          onClick={applyFilters}
          className="px-3 py-2 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-medium transition-colors"
        >
          Apply
        </button>
        {Object.values(filters).some(Boolean) && (
          <button onClick={clearFilters} className="text-xs text-text-tertiary hover:text-text-secondary transition-colors">
            Clear
          </button>
        )}
      </div>

      {/* Table */}
      <div className="border border-border-primary rounded-xl overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border-secondary">
              {['Timestamp', 'User', 'Action', 'Resource', 'ID'].map(h => (
                <th key={h} className="text-left px-4 py-3 text-[11px] text-text-tertiary uppercase tracking-wide font-normal">
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
                      <div className="h-4 rounded bg-surface-secondary animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : entries?.map(entry => (
                <tr key={entry.id} className="hover:bg-surface-secondary/40 transition-colors">
                  <td className="px-4 py-3 text-xs text-text-quaternary whitespace-nowrap">
                    {new Date(entry.timestamp).toLocaleString()}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {userMap.get(entry.user_id) ?? '—'}
                  </td>
                  <td className={`px-4 py-3 text-xs font-medium ${actionClass(entry.action)}`}>
                    {entry.action}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-tertiary">{entry.resource_type}</td>
                  <td className="px-4 py-3 text-xs text-text-quaternary font-mono truncate max-w-[120px]">
                    {entry.resource_id ?? '—'}
                  </td>
                </tr>
              ))
            }
          </tbody>
        </table>
        {!isLoading && (!entries || entries.length === 0) && (
          <p className="text-center text-text-quaternary text-sm py-10">No audit entries found.</p>
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
              className="p-1.5 rounded text-text-tertiary hover:text-text-secondary hover:bg-surface-secondary disabled:opacity-20 transition-colors"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
            <button
              onClick={() => setPage(p => p + 1)}
              disabled={!entries || entries.length < PAGE_SIZE}
              className="p-1.5 rounded text-text-tertiary hover:text-text-secondary hover:bg-surface-secondary disabled:opacity-20 transition-colors"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
