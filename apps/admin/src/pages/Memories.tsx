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

const TYPE_STYLES: Record<string, string> = {
  decision:     'text-blue-400 bg-blue-400/10 border-blue-400/20',
  bugfix:       'text-red-400 bg-red-400/10 border-red-400/20',
  discovery:    'text-purple-400 bg-purple-400/10 border-purple-400/20',
  convention:   'text-green-400 bg-green-400/10 border-green-400/20',
  architecture: 'text-indigo-400 bg-indigo-400/10 border-indigo-400/20',
  config:       'text-yellow-400 bg-yellow-400/10 border-yellow-400/20',
  preference:   'text-pink-400 bg-pink-400/10 border-pink-400/20',
  pattern:      'text-teal-400 bg-teal-400/10 border-teal-400/20',
}

function TypeBadge({ type }: { type?: string }) {
  if (!type) return null
  const cls = TYPE_STYLES[type] ?? 'text-text-tertiary bg-surface-secondary border-border-primary'
  return (
    <span className={`text-[11px] border rounded px-1.5 py-0.5 ${cls}`}>
      {type}
    </span>
  )
}

function MemoryDetailModal({ memory, onClose, onDelete, deleting }: {
  memory: Memory
  onClose: () => void
  onDelete: () => void
  deleting: boolean
}) {
  return (
    <div className="fixed inset-y-0 left-0 lg:left-52 right-0 z-50 flex items-center justify-center bg-black/60 p-6">
      <div className="bg-surface-primary border border-border-primary rounded-xl w-full max-w-4xl flex flex-col max-h-full">
        <div className="flex items-start justify-between gap-4 p-6 pb-4 shrink-0">
          <div className="space-y-1 min-w-0">
            {memory.title && (
              <p className="text-sm font-medium text-text-primary">{memory.title}</p>
            )}
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-[11px] border border-border-primary rounded px-1.5 py-0.5 text-text-tertiary">{memory.tool}</span>
              {memory.project && (
                <span className="text-[11px] text-text-tertiary">{memory.project}</span>
              )}
              <TypeBadge type={memory.type} />
              {memory.scope && memory.scope !== 'project' && (
                <span className="text-[11px] text-text-quaternary">{memory.scope}</span>
              )}
              {memory.revision_count != null && memory.revision_count > 1 && (
                <span className="text-[11px] text-text-quaternary">rev {memory.revision_count}</span>
              )}
            </div>
            <p className="text-[11px] text-text-quaternary">
              {new Date(memory.created_at).toLocaleString()}
            </p>
          </div>
          <button onClick={onClose} className="text-text-tertiary hover:text-text-primary transition-colors shrink-0">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="overflow-y-auto flex-1 px-6 pb-2">
          <p className="text-sm text-text-secondary leading-relaxed whitespace-pre-wrap">{memory.content}</p>

          {memory.tags.length > 0 && (
            <div className="flex flex-wrap gap-1.5 mt-4">
              {memory.tags.map(tag => (
                <span key={tag} className="text-[11px] bg-surface-secondary text-text-tertiary rounded px-2 py-0.5">
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>

        <div className="flex justify-end p-6 pt-4 shrink-0 border-t border-border-primary">
          <button
            onClick={onDelete}
            disabled={deleting}
            className="text-xs text-status-error/60 hover:text-status-error transition-colors disabled:opacity-40"
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
      ['id', 'user', 'tool', 'type', 'scope', 'title', 'project', 'content', 'tags', 'revision_count', 'created_at'],
      ...memories.map(m => [
        m.id,
        userMap.get(m.user_id) ?? m.user_id,
        m.tool,
        m.type ?? '',
        m.scope ?? '',
        m.title ? `"${m.title.replace(/"/g, '""')}"` : '',
        m.project,
        `"${m.content.replace(/"/g, '""')}"`,
        m.tags.join(';'),
        String(m.revision_count ?? 1),
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
          <h1 className="text-lg font-semibold text-text-primary">Memories</h1>
          <p className="text-[12px] text-text-tertiary mt-0.5">Browse and search stored memories</p>
        </div>
        <button
          onClick={handleExportCsv}
          disabled={!memories?.length}
          className="text-xs text-text-tertiary hover:text-text-secondary border border-border-primary rounded-lg px-3 py-1.5 hover:bg-surface-secondary transition-colors disabled:opacity-30"
        >
          Export CSV
        </button>
      </div>

      {/* Search */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-quaternary" />
        <input
          value={query}
          onChange={e => setQuery(e.target.value)}
          placeholder="Full-text search…"
          className="w-full bg-transparent border border-border-primary rounded-lg pl-9 pr-4 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus transition-colors"
        />
        {query && (
          <button
            onClick={() => setQuery('')}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-tertiary transition-colors"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {/* Table */}
      <div className="border border-border-primary rounded-xl overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border-secondary">
              {['Date', 'User', 'Tool', 'Type', 'Content', 'Tags'].map(h => (
                <th key={h} className="text-left px-4 py-3 text-[11px] text-text-tertiary uppercase tracking-wide font-normal">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-border-secondary">
            {isLoading
              ? Array.from({ length: 5 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 6 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded bg-surface-secondary animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : memories?.map(mem => (
                <tr
                  key={mem.id}
                  onClick={() => setSelected(mem)}
                  className="hover:bg-surface-secondary/40 transition-colors cursor-pointer"
                >
                  <td className="px-4 py-3 text-xs text-text-quaternary whitespace-nowrap">
                    {new Date(mem.created_at).toLocaleDateString()}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {userMap.get(mem.user_id) ?? '—'}
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex flex-col gap-1">
                      <span className="text-[11px] border border-border-primary rounded px-1.5 py-0.5 text-text-tertiary w-fit">
                        {mem.tool}
                      </span>
                      {mem.revision_count != null && mem.revision_count > 1 && (
                        <span className="text-[10px] text-text-quaternary">rev {mem.revision_count}</span>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-3">
                    <TypeBadge type={mem.type} />
                  </td>
                  <td className="px-4 py-3 max-w-xs">
                    {mem.title && (
                      <p className="text-xs font-medium text-text-primary truncate">{mem.title}</p>
                    )}
                    <p className="text-xs text-text-tertiary truncate">{mem.content}</p>
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex gap-1 flex-wrap">
                      {mem.tags.slice(0, 3).map(tag => (
                        <span key={tag} className="text-[11px] bg-surface-secondary text-text-tertiary rounded px-1.5 py-0.5">
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
          <p className="text-center text-text-quaternary text-sm py-10">
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
