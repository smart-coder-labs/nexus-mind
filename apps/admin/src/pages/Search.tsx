import { useState, useEffect, useCallback } from 'react'
import { Search as SearchIcon, X } from 'lucide-react'
import { createClient } from '../api/client'
import type { GlobalSearchResult, Memory, UserSummary, Project } from '../types'
import { cn } from '@/lib/utils'

const client = createClient()

type Tab = 'all' | 'memories' | 'users' | 'projects'

const TABS: { key: Tab; label: string }[] = [
  { key: 'all',      label: 'All' },
  { key: 'memories', label: 'Memories' },
  { key: 'users',    label: 'Users' },
  { key: 'projects', label: 'Projects' },
]

// ── Result cards ──────────────────────────────────────────────────────────────

function MemoryCard({ memory }: { memory: Memory }) {
  return (
    <div className="bg-[#272729] rounded-[11px] border border-border-primary p-4 space-y-2">
      {/* Tags */}
      {memory.tags && memory.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {memory.tags.slice(0, 5).map(tag => (
            <span
              key={tag}
              className="text-[10px] font-semibold bg-accent-blue/10 text-accent-blue border border-accent-blue/20 rounded-[5px] px-1.5 py-0.5"
            >
              {tag}
            </span>
          ))}
        </div>
      )}
      {/* Content preview */}
      <p className="text-xs text-text-secondary line-clamp-2 leading-relaxed">{memory.content}</p>
      {/* Footer */}
      <div className="flex items-center gap-2 flex-wrap">
        {memory.project && (
          <span className="text-[10px] bg-white/[0.06] border border-border-secondary text-text-tertiary rounded-full px-2 py-0.5">
            {memory.project}
          </span>
        )}
        {memory.type && (
          <span className="text-[10px] text-text-quaternary">{memory.type}</span>
        )}
      </div>
    </div>
  )
}

function UserCard({ user }: { user: UserSummary }) {
  return (
    <div className="bg-[#272729] rounded-[11px] border border-border-primary p-4 flex items-center gap-3">
      <div className="w-8 h-8 rounded-full bg-accent-blue/10 border border-accent-blue/20 flex items-center justify-center shrink-0">
        <span className="text-xs font-semibold text-accent-blue">
          {user.name.charAt(0).toUpperCase()}
        </span>
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-xs font-semibold text-text-primary truncate">{user.name}</p>
        <p className="text-xs text-text-quaternary truncate">{user.email}</p>
      </div>
      <span className="text-[10px] font-semibold bg-white/[0.06] border border-border-secondary text-text-tertiary rounded-full px-2 py-0.5 shrink-0 capitalize">
        {user.role}
      </span>
    </div>
  )
}

function ProjectCard({ project }: { project: Project }) {
  return (
    <div className="bg-[#272729] rounded-[11px] border border-border-primary p-4 space-y-1.5">
      <p className="text-xs font-semibold text-text-primary">{project.name}</p>
      {project.description && (
        <p className="text-xs text-text-tertiary line-clamp-2">{project.description}</p>
      )}
      <p className="text-[10px] text-text-quaternary">
        Created {new Date(project.created_at).toLocaleDateString()}
      </p>
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function Search() {
  const [query, setQuery] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')
  const [activeTab, setActiveTab] = useState<Tab>('all')
  const [results, setResults] = useState<GlobalSearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Debounce
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), 300)
    return () => clearTimeout(timer)
  }, [query])

  const runSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults(null)
      return
    }
    setLoading(true)
    setError(null)
    try {
      const data = await client.globalSearch(q, 20)
      setResults(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Search failed')
      setResults(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    runSearch(debouncedQuery)
  }, [debouncedQuery, runSearch])

  const memories  = results?.memories  ?? []
  const users     = results?.users     ?? []
  const projects  = results?.projects  ?? []
  const totalCount = memories.length + users.length + projects.length

  const tabCounts: Record<Tab, number> = {
    all:      totalCount,
    memories: memories.length,
    users:    users.length,
    projects: projects.length,
  }

  const hasResults = totalCount > 0

  return (
    <div className="p-8 max-w-3xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Search</h1>
        <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">
          Search across memories, users, and projects
        </p>
      </div>

      {/* Search bar */}
      <div className="relative flex items-center">
        <SearchIcon className="absolute left-4 w-4 h-4 text-text-quaternary pointer-events-none" />
        <input
          autoFocus
          value={query}
          onChange={e => setQuery(e.target.value)}
          placeholder="Search everything…"
          className="w-full rounded-[11px] border border-border-primary bg-white/[0.04] text-xs text-text-primary pl-8 pr-16 py-3 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary transition-colors"
        />
        <div className="absolute right-4 flex items-center gap-2">
          {query && (
            <button
              onClick={() => setQuery('')}
              className="text-text-quaternary hover:text-text-secondary transition-colors"
              aria-label="Clear search"
            >
              <X className="w-4 h-4" />
            </button>
          )}
          {!query && (
            <span className="text-[10px] text-text-quaternary font-mono">⌘K</span>
          )}
        </div>
      </div>

      {/* Tabs — only visible when there are results or a query */}
      {(hasResults || loading) && debouncedQuery && (
        <div className="flex items-center gap-1.5">
          {TABS.map(({ key, label }) => (
            <button
              key={key}
              onClick={() => setActiveTab(key)}
              className={cn(
                'transition-colors',
                activeTab === key
                  ? 'bg-accent-blue/10 text-accent-blue border-accent-blue/40 rounded-full px-3 py-1 text-xs border'
                  : 'text-text-quaternary border-transparent rounded-full px-3 py-1 text-xs border hover:text-text-secondary',
              )}
            >
              {label}
              {results && tabCounts[key] > 0 && (
                <span className="ml-1.5 text-[10px] text-text-quaternary">
                  {tabCounts[key]}
                </span>
              )}
            </button>
          ))}
        </div>
      )}

      {/* Loading */}
      {loading && (
        <div className="space-y-3">
          {[0, 1, 2].map(i => (
            <div key={i} className="h-20 animate-pulse bg-[#272729] rounded-[11px]" />
          ))}
        </div>
      )}

      {/* Error */}
      {error && !loading && (
        <p className="text-sm text-status-error text-center">{error}</p>
      )}

      {/* Empty state — no query */}
      {!debouncedQuery && !loading && (
        <div className="text-center py-16">
          <SearchIcon className="w-8 h-8 text-text-quaternary mx-auto mb-3 opacity-50" />
          <p className="text-sm text-text-quaternary">
            Start typing to search everything in your organization
          </p>
        </div>
      )}

      {/* No results */}
      {debouncedQuery && !loading && !error && results && !hasResults && (
        <div className="text-center py-16">
          <p className="text-sm text-text-quaternary">No results for "{debouncedQuery}"</p>
        </div>
      )}

      {/* Results */}
      {!loading && hasResults && (
        <div className="space-y-6">
          {/* Memories */}
          {(activeTab === 'all' || activeTab === 'memories') && memories.length > 0 && (
            <section className="space-y-3">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Memories
                </p>
              )}
              {memories.map(m => (
                <MemoryCard key={m.id} memory={m} />
              ))}
            </section>
          )}

          {/* Users */}
          {(activeTab === 'all' || activeTab === 'users') && users.length > 0 && (
            <section className="space-y-3">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Users
                </p>
              )}
              {users.map(u => (
                <UserCard key={u.id} user={u} />
              ))}
            </section>
          )}

          {/* Projects */}
          {(activeTab === 'all' || activeTab === 'projects') && projects.length > 0 && (
            <section className="space-y-3">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Projects
                </p>
              )}
              {projects.map(p => (
                <ProjectCard key={p.id} project={p} />
              ))}
            </section>
          )}
        </div>
      )}
    </div>
  )
}
