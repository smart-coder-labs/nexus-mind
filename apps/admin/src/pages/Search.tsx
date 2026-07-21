import { useState, useEffect, useCallback } from 'react'
import { Search as SearchIcon, X, ChevronDown } from 'lucide-react'
import { createClient } from '../api/client'
import type { GlobalSearchResult } from '../types'
import { cn } from '@/lib/utils'
import { ResultRow } from './search/ResultRow'

const client = createClient()

type Tab = 'all' | 'memories' | 'users' | 'projects' | 'policies' | 'conventions' | 'sdd'

const TABS: { key: Tab; label: string }[] = [
  { key: 'all',         label: 'All types' },
  { key: 'memories',   label: 'Memories' },
  { key: 'users',      label: 'Users' },
  { key: 'projects',   label: 'Projects' },
  { key: 'policies',   label: 'Policies' },
  { key: 'conventions', label: 'Conventions' },
  { key: 'sdd',        label: 'SDD' },
]

function firstLine(content: string): string {
  return content.split('\n')[0].trim()
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function Search() {
  const [query, setQuery] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')
  const [activeTab, setActiveTab] = useState<Tab>('all')
  const [results, setResults] = useState<GlobalSearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Real count from the existing org-stats endpoint, used only for the hero subtitle.
  // Left `null` (and the number omitted from copy) if the call fails or hasn't resolved yet.
  const [totalMemories, setTotalMemories] = useState<number | null>(null)

  useEffect(() => {
    let cancelled = false
    // Guarded rather than called unconditionally: some test doubles for the API
    // client only stub the methods a given spec exercises (see Search.test.tsx),
    // so a missing `getStats` must degrade to "omit the number", not throw.
    if (typeof client.getStats === 'function') {
      client.getStats()
        .then(stats => { if (!cancelled) setTotalMemories(stats.total_memories) })
        .catch(() => { /* subtitle just omits the number */ })
    }
    return () => { cancelled = true }
  }, [])

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

  const memories    = results?.memories    ?? []
  const users       = results?.users       ?? []
  const projects    = results?.projects    ?? []
  const policies    = results?.policies    ?? []
  const conventions = results?.conventions ?? []
  // Additive facet: a backend that predates it omits the key entirely, and a
  // caller without `sdd:read` gets it back empty (A4). Both must be non-events.
  const sddChanges  = results?.sdd_changes ?? []
  const totalCount  = memories.length + users.length + projects.length + policies.length
    + conventions.length + sddChanges.length

  const tabCounts: Record<Tab, number> = {
    all:         totalCount,
    memories:    memories.length,
    users:       users.length,
    projects:    projects.length,
    policies:    policies.length,
    conventions: conventions.length,
    sdd:         sddChanges.length,
  }

  const hasResults = totalCount > 0
  const isTyping = query.trim().length > 0
  const hasSearched = debouncedQuery.trim().length > 0

  return (
    <div className="p-8 max-w-5xl mx-auto">
      {/* Hero — compresses toward the top once the user starts typing */}
      <div
        className={cn(
          'flex flex-col items-center gap-4 transition-all duration-300 ease-out',
          isTyping ? 'pt-4 pb-8' : 'pt-16 pb-10',
        )}
      >
        <h1
          className={cn(
            'font-extrabold text-text-primary tracking-tight text-center transition-all duration-300',
            isTyping ? 'text-2xl' : 'text-[32px]',
          )}
        >
          Search the organization&apos;s memory
        </h1>
        <p className="text-[13.5px] text-text-tertiary text-center">
          Hybrid semantic + keyword search across{' '}
          {totalMemories != null && `${totalMemories.toLocaleString()} `}
          memories, sessions, code and conventions.
        </p>

        <div className="w-full max-w-[720px] flex flex-col gap-3">
          {/* Search box */}
          <div className="relative flex items-center h-[54px] px-[18px] rounded-2xl border border-accent-blue/35 bg-white/[0.04] backdrop-blur-md">
            <SearchIcon className="w-[18px] h-[18px] text-accent-blue shrink-0" />
            <input
              autoFocus
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="Search everything…"
              className="flex-1 min-w-0 bg-transparent border-none outline-none text-[15px] text-text-primary placeholder:text-text-quaternary px-3"
            />
            {query ? (
              <button
                onClick={() => setQuery('')}
                className="text-text-quaternary hover:text-text-secondary transition-colors shrink-0"
                aria-label="Clear search"
              >
                <X className="w-4 h-4" />
              </button>
            ) : (
              <span className="shrink-0 text-[11px] text-text-quaternary font-mono border border-border-primary rounded-[5px] px-1.5 py-0.5">
                ⌘K
              </span>
            )}
          </div>

          {/*
            NOTE on parity with the design mockup: the Hybrid / Semantic / Keyword mode
            switch and the "All projects" filter are NOT implemented. GET /v1/search
            (apps/backend/src/api/search.rs) takes only `q` and `limit` — there is no
            `mode` param and no per-project facet — so either control would be inert,
            fake UI. The "All types" pill below is real: it drives the same activeTab
            state (and section grouping) the page already had.
          */}
          {(hasResults || loading) && hasSearched && (
            <div className="flex items-center justify-center">
              <div className="relative inline-flex items-center">
                <select
                  value={activeTab}
                  onChange={e => setActiveTab(e.target.value as Tab)}
                  aria-label="Filter by result type"
                  className="appearance-none h-[34px] pl-3.5 pr-8 rounded-[10px] border border-border-primary bg-white/[0.04] text-[12.5px] text-text-secondary cursor-pointer hover:border-border-primary/70 focus:outline-none focus:border-accent-blue/60 transition-colors"
                >
                  {TABS.map(t => (
                    <option key={t.key} value={t.key} className="bg-[#111319]">
                      {t.label}
                    </option>
                  ))}
                </select>
                <ChevronDown className="w-3 h-3 text-text-quaternary absolute right-2.5 pointer-events-none" />
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Results header */}
      {hasSearched && (hasResults || loading) && (
        <div className="flex items-center gap-2 mb-3.5">
          <span className="text-[12.5px] text-text-tertiary">
            <strong className="text-text-secondary font-bold">{tabCounts[activeTab]}</strong> results
            {/* Latency omitted: the client wraps GET /v1/search but neither the response
                body nor the client surfaces a timing figure, so there is nothing real to show. */}
          </span>
          <div className="flex-1" />
          <span className="text-[12px] text-text-quaternary">sorted by relevance</span>
        </div>
      )}

      {/* Loading */}
      {loading && (
        <div className="space-y-2.5">
          {[0, 1, 2].map(i => (
            <div key={i} className="h-20 animate-pulse bg-white/[0.04] rounded-[14px]" />
          ))}
        </div>
      )}

      {/* Error */}
      {error && !loading && (
        <p className="text-xs text-status-error text-center py-8">{error}</p>
      )}

      {/* Empty state — no query */}
      {!hasSearched && !loading && (
        <div className="text-center py-16">
          <SearchIcon className="w-8 h-8 text-text-quaternary mx-auto mb-3 opacity-50" />
          <p className="text-xs text-text-quaternary">
            Start typing to search everything in your organization
          </p>
        </div>
      )}

      {/* No results */}
      {hasSearched && !loading && !error && results && !hasResults && (
        <div className="text-center py-16">
          <p className="text-xs text-text-quaternary">No results for &quot;{debouncedQuery}&quot;</p>
        </div>
      )}

      {/* Results */}
      {!loading && hasResults && (
        <div className="space-y-6">
          {/* Memories */}
          {(activeTab === 'all' || activeTab === 'memories') && memories.length > 0 && (
            <section className="space-y-2.5">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Memories
                </p>
              )}
              {memories.map(m => (
                <ResultRow
                  key={m.id}
                  kind="memory"
                  title={m.title || (m.content.length > 80 ? `${m.content.slice(0, 80)}…` : m.content)}
                  excerpt={m.content}
                  query={debouncedQuery}
                  meta={[m.project, m.type, new Date(m.created_at).toLocaleDateString()].filter(Boolean) as string[]}
                  tags={m.tags}
                />
              ))}
            </section>
          )}

          {/* Users */}
          {(activeTab === 'all' || activeTab === 'users') && users.length > 0 && (
            <section className="space-y-2.5">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Users
                </p>
              )}
              {users.map(u => (
                <ResultRow
                  key={u.id}
                  kind="user"
                  title={u.name}
                  query={debouncedQuery}
                  meta={[u.email]}
                  extra={
                    <span className="shrink-0 text-[10px] font-semibold bg-white/[0.06] border border-border-secondary text-text-tertiary rounded-full px-2 py-0.5 capitalize">
                      {u.role}
                    </span>
                  }
                />
              ))}
            </section>
          )}

          {/* Projects */}
          {(activeTab === 'all' || activeTab === 'projects') && projects.length > 0 && (
            <section className="space-y-2.5">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Projects
                </p>
              )}
              {projects.map(p => (
                <ResultRow
                  key={p.id}
                  kind="project"
                  title={p.name}
                  excerpt={p.description ?? undefined}
                  query={debouncedQuery}
                  meta={[`Created ${new Date(p.created_at).toLocaleDateString()}`]}
                />
              ))}
            </section>
          )}

          {/* Policies */}
          {(activeTab === 'all' || activeTab === 'policies') && policies.length > 0 && (
            <section className="space-y-2.5">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Policies
                </p>
              )}
              {policies.map(p => (
                <ResultRow
                  key={p.id}
                  kind="policy"
                  title={p.name}
                  query={debouncedQuery}
                  meta={p.rule_type ? [p.rule_type] : []}
                  extra={
                    <span
                      className={cn(
                        'shrink-0 text-[10px] font-semibold rounded-full px-2 py-0.5',
                        p.enabled
                          ? 'bg-status-success/10 border border-status-success/20 text-status-success'
                          : 'bg-white/[0.06] border border-border-secondary text-text-quaternary',
                      )}
                    >
                      {p.enabled ? 'enabled' : 'disabled'}
                    </span>
                  }
                />
              ))}
            </section>
          )}

          {/* Conventions */}
          {(activeTab === 'all' || activeTab === 'conventions') && conventions.length > 0 && (
            <section className="space-y-2.5">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Conventions
                </p>
              )}
              {conventions.map(c => (
                <ResultRow
                  key={c.id}
                  kind="convention"
                  title={c.title}
                  excerpt={firstLine(c.content)}
                  query={debouncedQuery}
                  meta={c.category ? [c.category] : []}
                  tags={c.tags}
                />
              ))}
            </section>
          )}

          {/* SDD — the group is omitted, never rendered empty */}
          {(activeTab === 'all' || activeTab === 'sdd') && sddChanges.length > 0 && (
            <section data-testid="sdd-results" className="space-y-2.5">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  SDD
                </p>
              )}
              {sddChanges.map(c => (
                <ResultRow
                  key={c.id}
                  kind="sdd"
                  title={c.name}
                  excerpt={c.title ?? undefined}
                  query={debouncedQuery}
                  meta={[c.project]}
                  href={`/sdd?change=${encodeURIComponent(c.name)}`}
                  extra={
                    <span className="shrink-0 text-[10px] font-semibold bg-accent-blue/10 text-accent-blue border border-accent-blue/20 rounded-full px-2 py-0.5">
                      {c.phase}
                    </span>
                  }
                />
              ))}
            </section>
          )}
        </div>
      )}
    </div>
  )
}
