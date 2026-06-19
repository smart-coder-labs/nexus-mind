import { useMemo, useState, useCallback, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import ReactMarkdown from 'react-markdown'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { downloadExport, todayStamp } from '../lib/download'
import type { Memory } from '../types'
import { Search, X, Brain, Tag } from 'lucide-react'

function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delay)
    return () => clearTimeout(t)
  }, [value, delay])
  return debounced
}

const TYPE_META: Record<string, { label: string; cls: string }> = {
  decision:     { label: 'decision',     cls: 'text-blue-300 bg-blue-500/15 border-blue-500/30' },
  bugfix:       { label: 'bugfix',       cls: 'text-red-300 bg-red-500/15 border-red-500/30' },
  discovery:    { label: 'discovery',    cls: 'text-purple-300 bg-purple-500/15 border-purple-500/30' },
  convention:   { label: 'convention',   cls: 'text-green-300 bg-green-500/15 border-green-500/30' },
  architecture: { label: 'architecture', cls: 'text-indigo-300 bg-indigo-500/15 border-indigo-500/30' },
  config:       { label: 'config',       cls: 'text-yellow-300 bg-yellow-500/15 border-yellow-500/30' },
  preference:   { label: 'preference',   cls: 'text-pink-300 bg-pink-500/15 border-pink-500/30' },
  pattern:      { label: 'pattern',      cls: 'text-teal-300 bg-teal-500/15 border-teal-500/30' },
}

function TypeBadge({ type }: { type?: string }) {
  if (!type) return null
  const meta = TYPE_META[type]
  const cls = meta?.cls ?? 'text-text-tertiary bg-surface-secondary border-border-primary'
  return (
    <span className={`text-[11px] font-medium border rounded-md px-2 py-0.5 ${cls}`}>
      {meta?.label ?? type}
    </span>
  )
}

// ── Markdown renderer ─────────────────────────────────────────────────────────

function MemoryMarkdown({ content }: { content: string }) {
  return (
    <ReactMarkdown
      components={{
        h1: ({ children }) => (
          <h1 className="text-base font-semibold text-text-primary mt-6 mb-2 first:mt-0">{children}</h1>
        ),
        h2: ({ children }) => (
          <h2 className="text-sm font-semibold text-text-primary mt-5 mb-1.5 pb-1.5 border-b border-border-secondary first:mt-0">{children}</h2>
        ),
        h3: ({ children }) => (
          <h3 className="text-[13px] font-semibold text-accent-blue mt-4 mb-1 first:mt-0">{children}</h3>
        ),
        p: ({ children }) => (
          <p className="text-sm text-text-secondary leading-relaxed mb-3 last:mb-0">{children}</p>
        ),
        ul: ({ children }) => (
          <ul className="mb-3 ml-4 space-y-1 list-none last:mb-0">{children}</ul>
        ),
        ol: ({ children }) => (
          <ol className="mb-3 ml-4 space-y-1 list-decimal last:mb-0">{children}</ol>
        ),
        li: ({ children }) => (
          <li className="text-sm text-text-secondary leading-relaxed flex gap-2">
            <span className="text-accent-blue/50 mt-1.5 shrink-0 w-1 h-1 rounded-full bg-accent-blue/40 inline-block" />
            <span>{children}</span>
          </li>
        ),
        strong: ({ children }) => (
          <strong className="font-semibold text-text-primary">{children}</strong>
        ),
        em: ({ children }) => (
          <em className="italic text-text-secondary">{children}</em>
        ),
        a: ({ href, children }) => (
          <a href={href} target="_blank" rel="noopener noreferrer"
             className="text-accent-blue hover:text-accent-blue-hover underline decoration-accent-blue/30 transition-colors">
            {children}
          </a>
        ),
        blockquote: ({ children }) => (
          <blockquote className="border-l-2 border-accent-blue/30 pl-4 my-3 text-text-tertiary italic">
            {children}
          </blockquote>
        ),
        code: ({ children, className }) => {
          const isBlock = className?.startsWith('language-')
          if (isBlock) {
            return (
              <code className="block text-xs font-mono text-text-secondary leading-relaxed">
                {children}
              </code>
            )
          }
          return (
            <code className="text-[12px] font-mono text-accent-blue bg-accent-blue/8 rounded px-1.5 py-0.5">
              {children}
            </code>
          )
        },
        pre: ({ children }) => (
          <pre className="bg-bg-secondary border border-border-primary rounded-lg px-4 py-3 overflow-x-auto mb-3 last:mb-0">
            {children}
          </pre>
        ),
        hr: () => <hr className="border-border-primary my-4" />,
      }}
    >
      {content}
    </ReactMarkdown>
  )
}

// ── Modal ─────────────────────────────────────────────────────────────────────

function MemoryDetailModal({ memory, onClose, onDelete, deleting }: {
  memory: Memory
  onClose: () => void
  onDelete: () => void
  deleting: boolean
}) {
  const { session } = useAuth()
  const canDelete =
    session?.user.role === 'admin' ||
    (session?.user.role === 'member' && memory.user_id === session.user.id)

  return (
    <div className="fixed inset-y-0 left-0 lg:left-52 right-0 z-50 flex items-center justify-center bg-black/60 p-6">
      <div className="bg-bg-secondary border border-border-primary rounded-[18px] w-full max-w-3xl flex flex-col max-h-full">

        {/* Header */}
        <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-4 shrink-0 border-b border-border-secondary">
          <div className="space-y-2 min-w-0">
            {memory.title && (
              <p className="text-sm font-semibold text-text-primary leading-snug">{memory.title}</p>
            )}
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-[11px] font-medium border border-border-primary rounded-md px-2 py-0.5 text-text-tertiary bg-surface-secondary">
                {memory.tool}
              </span>
              {memory.project && (
                <span className="text-[11px] text-text-tertiary font-medium">{memory.project}</span>
              )}
              <TypeBadge type={memory.type} />
              {memory.revision_count != null && memory.revision_count > 1 && (
                <span className="text-[11px] text-text-quaternary bg-surface-secondary border border-border-secondary rounded-md px-1.5 py-0.5">
                  rev {memory.revision_count}
                </span>
              )}
            </div>
            <p className="text-[11px] text-text-quaternary">
              {new Date(memory.created_at).toLocaleString()}
            </p>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-[11px] text-text-quaternary hover:text-text-primary hover:bg-surface-secondary transition-colors shrink-0"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Content */}
        <div className="overflow-y-auto flex-1 px-6 py-5">
          <MemoryMarkdown content={memory.content} />

          {memory.tags.length > 0 && (
            <div className="flex items-center gap-2 flex-wrap mt-5 pt-4 border-t border-border-secondary">
              <Tag className="w-3 h-3 text-text-quaternary shrink-0" />
              {memory.tags.map(tag => (
                <span key={tag} className="text-[11px] bg-surface-secondary text-text-tertiary border border-border-secondary rounded-md px-2 py-0.5">
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        {canDelete && (
          <div className="flex justify-end px-6 py-4 shrink-0 border-t border-border-secondary">
            <button
              onClick={onDelete}
              disabled={deleting}
              className="text-xs text-status-error/50 hover:text-status-error transition-colors disabled:opacity-40"
            >
              {deleting ? 'Deleting…' : 'Delete memory'}
            </button>
          </div>
        )}
      </div>
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function Memories() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [query, setQuery] = useState('')
  const [mode, setMode] = useState<'keyword' | 'hybrid'>('hybrid')
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
    queryKey: ['memories', 'search', debouncedQuery, mode],
    queryFn: () => client.searchMemories(debouncedQuery, 20, mode),
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

  const [exporting, setExporting] = useState<null | 'csv' | 'json'>(null)

  const handleExport = useCallback(async (format: 'csv' | 'json') => {
    setExporting(format)
    try {
      await downloadExport(
        `/v1/memory/export?format=${format}`,
        `memories-${todayStamp()}.${format}`,
      )
    } finally {
      setExporting(null)
    }
  }, [])

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">

      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-[11px] bg-accent-blue/10 border border-accent-blue/20">
            <Brain className="w-4 h-4 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-base font-semibold text-text-primary">Memories</h1>
            <p className="text-[12px] text-text-tertiary">
              {memories ? `${memories.length} entries` : 'Browse and search stored memories'}
            </p>
          </div>
        </div>
        <div className="flex gap-2">
          <button
            onClick={() => handleExport('csv')}
            disabled={exporting !== null}
            className="text-xs text-text-tertiary hover:text-text-secondary border border-border-primary rounded-lg px-3 py-1.5 hover:bg-surface-secondary transition-colors disabled:opacity-30"
            aria-label="Export memories as CSV"
          >
            {exporting === 'csv' ? 'Exporting…' : 'Export CSV'}
          </button>
          <button
            onClick={() => handleExport('json')}
            disabled={exporting !== null}
            className="text-xs text-text-tertiary hover:text-text-secondary border border-border-primary rounded-lg px-3 py-1.5 hover:bg-surface-secondary transition-colors disabled:opacity-30"
            aria-label="Export memories as JSON"
          >
            {exporting === 'json' ? 'Exporting…' : 'Export JSON'}
          </button>
        </div>
      </div>

      {/* Search */}
      <div className="flex gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-text-quaternary" />
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Search memories…"
            className="w-full bg-surface-primary border border-border-primary rounded-full pl-10 pr-4 py-3 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus transition-colors"
          />
          {query && (
            <button
              onClick={() => setQuery('')}
              className="absolute right-3.5 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-tertiary transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
        <div className="flex items-center bg-surface-primary border border-border-primary rounded-[11px] px-1 gap-0.5">
          {(['keyword', 'hybrid'] as const).map(m => (
            <button
              key={m}
              onClick={() => setMode(m)}
              className={`px-3 py-1.5 text-xs font-normal rounded-lg transition-colors ${
                mode === m
                  ? 'bg-accent-blue/15 text-accent-blue'
                  : 'text-text-quaternary hover:text-text-tertiary'
              }`}
            >
              {m}
            </button>
          ))}
        </div>
      </div>

      {/* Table */}
      <div className="border border-border-primary rounded-[18px] overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="bg-surface-secondary border-b border-border-primary">
              {['Date', 'User', 'Type', 'Memory'].map(h => (
                <th key={h} className="text-left px-4 py-3 text-[11px] font-semibold text-text-tertiary uppercase tracking-wider">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {isLoading
              ? Array.from({ length: 5 }).map((_, i) => (
                <tr key={i} className="border-t border-border-secondary">
                  {Array.from({ length: 4 }).map((_, j) => (
                    <td key={j} className="px-4 py-4">
                      <div className="h-3.5 rounded-md bg-surface-secondary animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : memories?.map((mem, idx) => (
                <tr
                  key={mem.id}
                  onClick={() => setSelected(mem)}
                  className={`border-t border-border-secondary hover:bg-accent-blue/[0.04] transition-colors cursor-pointer group ${idx === 0 ? 'border-t-0' : ''}`}
                >
                  <td className="px-4 py-3.5 whitespace-nowrap">
                    <p className="text-xs font-medium text-text-secondary">
                      {new Date(mem.created_at).toLocaleDateString()}
                    </p>
                    <p className="text-[11px] text-text-quaternary mt-0.5">
                      {new Date(mem.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                    </p>
                  </td>
                  <td className="px-4 py-3.5">
                    <div className="space-y-1">
                      <p className="text-xs text-text-secondary font-medium">
                        {userMap.get(mem.user_id) ?? '—'}
                      </p>
                      <span className="text-[10px] border border-border-primary rounded px-1.5 py-0.5 text-text-quaternary bg-surface-secondary/50 inline-block">
                        {mem.tool}
                      </span>
                    </div>
                  </td>
                  <td className="px-4 py-3.5">
                    <div className="space-y-1.5">
                      <TypeBadge type={mem.type} />
                      {mem.revision_count != null && mem.revision_count > 1 && (
                        <p className="text-[10px] text-text-quaternary">rev {mem.revision_count}</p>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-3.5 max-w-sm">
                    {mem.title && (
                      <p className="text-xs font-semibold text-text-primary truncate mb-0.5 group-hover:text-accent-blue transition-colors">
                        {mem.title}
                      </p>
                    )}
                    <p className="text-xs text-text-tertiary line-clamp-2 leading-relaxed">
                      {mem.content.replace(/#+\s/g, '').replace(/\*\*/g, '')}
                    </p>
                    {mem.tags.length > 0 && (
                      <div className="flex gap-1 flex-wrap mt-1.5">
                        {mem.tags.slice(0, 3).map(tag => (
                          <span key={tag} className="text-[10px] bg-surface-secondary text-text-quaternary rounded px-1.5 py-0.5">
                            {tag}
                          </span>
                        ))}
                      </div>
                    )}
                  </td>
                </tr>
              ))
            }
          </tbody>
        </table>

        {!isLoading && (!memories || memories.length === 0) && (
          <div className="flex flex-col items-center gap-2 py-16 text-center">
            <Brain className="w-6 h-6 text-text-quaternary/50" />
            <p className="text-sm text-text-quaternary">
              {isSearching ? 'No results found.' : 'No memories stored yet.'}
            </p>
          </div>
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
