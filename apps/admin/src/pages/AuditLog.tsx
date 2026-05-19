import { useMemo, useState, useCallback } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { AuditFilters } from '../types'
import { ChevronLeft, ChevronRight } from 'lucide-react'

const PAGE_SIZE = 50

const ACTION_COLORS: Record<string, string> = {
  store:  'text-blue-400/70',
  search: 'text-white/40',
  delete: 'text-red-400/70',
  invite: 'text-green-400/70',
  revoke: 'text-orange-400/70',
}

function actionClass(action: string) {
  return ACTION_COLORS[action] ?? 'text-white/40'
}

export default function AuditLog() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(session!.apiKey), [session])

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

  const inputCls = 'bg-transparent border border-white/8 rounded-lg px-3 py-2 text-sm text-white/70 placeholder:text-white/15 focus:outline-none focus:border-white/20 transition-colors'
  const selectCls = 'bg-[#0c0c0e] border border-white/8 rounded-lg px-3 py-2 text-sm text-white/70 focus:outline-none focus:border-white/20 transition-colors'

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-white">Audit Log</h1>
          <p className="text-[12px] text-white/30 mt-0.5">All actions performed in your organization</p>
        </div>
        <button
          onClick={handleExportCsv}
          disabled={!entries?.length}
          className="text-xs text-white/30 hover:text-white/60 transition-colors disabled:opacity-30"
        >
          Export CSV
        </button>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap gap-2 items-end">
        <select value={draft.user_id} onChange={setField('user_id')} className={selectCls}>
          <option value="">All users</option>
          {users?.map(u => <option key={u.id} value={u.id}>{u.name}</option>)}
        </select>
        <select value={draft.action} onChange={setField('action')} className={selectCls}>
          <option value="">All actions</option>
          {['store', 'search', 'delete', 'invite', 'revoke'].map(a => (
            <option key={a} value={a}>{a}</option>
          ))}
        </select>
        <select value={draft.resource_type} onChange={setField('resource_type')} className={selectCls}>
          <option value="">All resources</option>
          {['memory', 'user', 'org'].map(r => <option key={r} value={r}>{r}</option>)}
        </select>
        <label className="flex flex-col gap-1">
          <span className="text-[10px] text-white/25 uppercase tracking-wide px-0.5">From</span>
          <input type="date" value={draft.from} onChange={setField('from')} className={inputCls} />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-[10px] text-white/25 uppercase tracking-wide px-0.5">To</span>
          <input type="date" value={draft.to}   onChange={setField('to')}   className={inputCls} />
        </label>
        <button
          onClick={applyFilters}
          className="px-3 py-2 rounded-lg bg-white text-[#0c0c0e] text-sm font-medium hover:bg-white/90 transition-colors"
        >
          Apply
        </button>
        {Object.values(filters).some(Boolean) && (
          <button onClick={clearFilters} className="text-xs text-white/30 hover:text-white/60 transition-colors">
            Clear
          </button>
        )}
      </div>

      {/* Table */}
      <div className="border border-white/8 rounded-xl overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-white/5">
              {['Timestamp', 'User', 'Action', 'Resource', 'ID'].map(h => (
                <th key={h} className="text-left px-4 py-3 text-[11px] text-white/30 uppercase tracking-wide font-normal">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-white/5">
            {isLoading
              ? Array.from({ length: 8 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 5 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded bg-white/5 animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : entries?.map(entry => (
                <tr key={entry.id} className="hover:bg-white/[0.02] transition-colors">
                  <td className="px-4 py-3 text-xs text-white/25 whitespace-nowrap">
                    {new Date(entry.timestamp).toLocaleString()}
                  </td>
                  <td className="px-4 py-3 text-xs text-white/50">
                    {userMap.get(entry.user_id) ?? '—'}
                  </td>
                  <td className={`px-4 py-3 text-xs font-medium ${actionClass(entry.action)}`}>
                    {entry.action}
                  </td>
                  <td className="px-4 py-3 text-xs text-white/40">{entry.resource_type}</td>
                  <td className="px-4 py-3 text-xs text-white/20 font-mono truncate max-w-[120px]">
                    {entry.resource_id ?? '—'}
                  </td>
                </tr>
              ))
            }
          </tbody>
        </table>
        {!isLoading && (!entries || entries.length === 0) && (
          <p className="text-center text-white/20 text-sm py-10">No audit entries found.</p>
        )}
      </div>

      {/* Pagination */}
      {(entries?.length === PAGE_SIZE || page > 0) && (
        <div className="flex items-center justify-between">
          <p className="text-xs text-white/25">Page {page + 1}</p>
          <div className="flex gap-2">
            <button
              onClick={() => setPage(p => Math.max(0, p - 1))}
              disabled={page === 0}
              className="p-1.5 rounded text-white/30 hover:text-white/60 disabled:opacity-20 transition-colors"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
            <button
              onClick={() => setPage(p => p + 1)}
              disabled={!entries || entries.length < PAGE_SIZE}
              className="p-1.5 rounded text-white/30 hover:text-white/60 disabled:opacity-20 transition-colors"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
