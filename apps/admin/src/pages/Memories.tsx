import { useMemo, useState, useCallback, useEffect } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { Memory } from '../types'
import { Search, X } from 'lucide-react'

function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delay)
    return () => clearTimeout(t)
  }, [value, delay])
  return debounced
}

function MemoryDetailModal({ memory, onClose, onDelete, deleting }: {
  memory: Memory
  onClose: () => void
  onDelete: () => void
  deleting: boolean
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="bg-[#161618] border border-white/8 rounded-xl w-full max-w-lg space-y-4 p-6">
        <div className="flex items-start justify-between gap-4">
          <div className="space-y-0.5">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-[11px] border border-white/10 rounded px-1.5 py-0.5 text-white/40">{memory.tool}</span>
              {memory.project && (
                <span className="text-[11px] text-white/25">{memory.project}</span>
              )}
            </div>
            <p className="text-[11px] text-white/20">
              {new Date(memory.created_at).toLocaleString()}
            </p>
          </div>
          <button onClick={onClose} className="text-white/30 hover:text-white/60 transition-colors shrink-0">
            <X className="w-4 h-4" />
          </button>
        </div>

        <p className="text-sm text-white/70 leading-relaxed whitespace-pre-wrap">{memory.content}</p>

        {memory.tags.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {memory.tags.map(tag => (
              <span key={tag} className="text-[11px] bg-white/5 text-white/40 rounded px-2 py-0.5">
                {tag}
              </span>
            ))}
          </div>
        )}

        <div className="flex justify-end pt-1">
          <button
            onClick={onDelete}
            disabled={deleting}
            className="text-xs text-red-400/60 hover:text-red-400 transition-colors disabled:opacity-40"
          >
            {deleting ? 'Deleting…' : 'Delete memory'}
          </button>
        </div>
      </div>
    </div>
  )
}

export default function Memories() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(session!.apiKey), [session])

  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<Memory | null>(null)
  const debouncedQuery = useDebounce(query, 300)

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

  const isSearching = debouncedQuery.trim().length > 0

  const { data: listData, isLoading: listLoading } = useQuery({
    queryKey: ['memories', 'list'],
    queryFn: () => client.listMemories({ limit: 50 }),
    enabled: !isSearching,
  })

  const { data: searchData, isLoading: searchLoading } = useQuery({
    queryKey: ['memories', 'search', debouncedQuery],
    queryFn: () => client.searchMemories(debouncedQuery),
    enabled: isSearching,
  })

  const memories = isSearching ? searchData : listData
  const isLoading = isSearching ? searchLoading : listLoading

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteMemory(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setSelected(null)
    },
  })

  const handleExportCsv = useCallback(() => {
    if (!memories) return
    const rows = [
      ['id', 'user', 'tool', 'project', 'content', 'tags', 'created_at'],
      ...memories.map(m => [
        m.id,
        userMap.get(m.user_id) ?? m.user_id,
        m.tool,
        m.project,
        `"${m.content.replace(/"/g, '""')}"`,
        m.tags.join(';'),
        m.created_at,
      ]),
    ]
    const csv = rows.map(r => r.join(',')).join('\n')
    const a = document.createElement('a')
    a.href = URL.createObjectURL(new Blob([csv], { type: 'text/csv' }))
    a.download = 'memories.csv'
    a.click()
  }, [memories, userMap])

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-white">Memories</h1>
          <p className="text-[12px] text-white/30 mt-0.5">Browse and search stored memories</p>
        </div>
        <button
          onClick={handleExportCsv}
          disabled={!memories?.length}
          className="text-xs text-white/30 hover:text-white/60 transition-colors disabled:opacity-30"
        >
          Export CSV
        </button>
      </div>

      {/* Search */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-white/20" />
        <input
          value={query}
          onChange={e => setQuery(e.target.value)}
          placeholder="Full-text search…"
          className="w-full bg-transparent border border-white/8 rounded-lg pl-9 pr-4 py-2.5 text-sm text-white placeholder:text-white/15 focus:outline-none focus:border-white/20 transition-colors"
        />
        {query && (
          <button
            onClick={() => setQuery('')}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-white/20 hover:text-white/40 transition-colors"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {/* Table */}
      <div className="border border-white/8 rounded-xl overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-white/5">
              {['Date', 'User', 'Tool', 'Content', 'Tags'].map(h => (
                <th key={h} className="text-left px-4 py-3 text-[11px] text-white/30 uppercase tracking-wide font-normal">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-white/5">
            {isLoading
              ? Array.from({ length: 5 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 5 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded bg-white/5 animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : memories?.map(mem => (
                <tr
                  key={mem.id}
                  onClick={() => setSelected(mem)}
                  className="hover:bg-white/[0.02] transition-colors cursor-pointer"
                >
                  <td className="px-4 py-3 text-xs text-white/25 whitespace-nowrap">
                    {new Date(mem.created_at).toLocaleDateString()}
                  </td>
                  <td className="px-4 py-3 text-xs text-white/50">
                    {userMap.get(mem.user_id) ?? '—'}
                  </td>
                  <td className="px-4 py-3">
                    <span className="text-[11px] border border-white/10 rounded px-1.5 py-0.5 text-white/40">
                      {mem.tool}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-xs text-white/60 max-w-xs truncate">
                    {mem.content}
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex gap-1 flex-wrap">
                      {mem.tags.slice(0, 3).map(tag => (
                        <span key={tag} className="text-[11px] bg-white/5 text-white/30 rounded px-1.5 py-0.5">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </td>
                </tr>
              ))
            }
          </tbody>
        </table>
        {!isLoading && (!memories || memories.length === 0) && (
          <p className="text-center text-white/20 text-sm py-10">
            {isSearching ? 'No results found.' : 'No memories yet.'}
          </p>
        )}
      </div>

      {selected && (
        <MemoryDetailModal
          memory={selected}
          onClose={() => setSelected(null)}
          onDelete={() => deleteMut.mutate(selected.id)}
          deleting={deleteMut.isPending}
        />
      )}
    </div>
  )
}
