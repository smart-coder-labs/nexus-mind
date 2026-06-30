import { useState, useEffect, useCallback } from 'react'
import { Search as SearchIcon, X } from 'lucide-react'
import { searchInternal } from '../api/client'
import type { OrgWithStats, User } from '../types'
import { cn } from '@/lib/utils'

type Tab = 'all' | 'orgs' | 'users'

const TABS: { key: Tab; label: string }[] = [
  { key: 'all',   label: 'All' },
  { key: 'orgs',  label: 'Organizations' },
  { key: 'users', label: 'Users' },
]

// ── Result cards ──────────────────────────────────────────────────────────────

function OrgCard({ org }: { org: OrgWithStats }) {
  return (
    <div className="bg-surface-secondary rounded-lg border border-border-primary p-4 space-y-1.5">
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm font-semibold text-text-primary truncate">{org.name}</p>
        <span className="text-[10px] text-text-quaternary font-mono shrink-0">{org.slug}</span>
      </div>
      <div className="flex items-center gap-4">
        <span className="text-[11px] text-text-tertiary">
          {org.user_count} {org.user_count === 1 ? 'user' : 'users'}
        </span>
        <span className="text-[11px] text-text-tertiary">
          {org.memory_count} {org.memory_count === 1 ? 'memory' : 'memories'}
        </span>
      </div>
    </div>
  )
}

function UserCard({ user }: { user: User }) {
  return (
    <div className="bg-surface-secondary rounded-lg border border-border-primary p-4 flex items-center gap-3">
      <div className="w-8 h-8 rounded-full bg-accent-blue/10 border border-accent-blue/20 flex items-center justify-center shrink-0">
        <span className="text-xs font-semibold text-accent-blue">
          {user.name.charAt(0).toUpperCase()}
        </span>
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold text-text-primary truncate">{user.name}</p>
        <p className="text-[11px] text-text-quaternary truncate">{user.email}</p>
      </div>
      <div className="shrink-0 flex flex-col items-end gap-1">
        <span className="text-[10px] font-semibold bg-white/[0.06] border border-border-secondary text-text-tertiary rounded-full px-2 py-0.5 capitalize">
          {user.role}
        </span>
        <span className={cn(
          'text-[10px] font-medium',
          user.status === 'active' ? 'text-green-400' : 'text-text-quaternary'
        )}>
          {user.status}
        </span>
      </div>
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function Search() {
  const [query, setQuery] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')
  const [activeTab, setActiveTab] = useState<Tab>('all')
  const [orgs, setOrgs] = useState<OrgWithStats[]>([])
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [hasSearched, setHasSearched] = useState(false)

  // 300ms debounce
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), 300)
    return () => clearTimeout(timer)
  }, [query])

  const runSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setOrgs([])
      setUsers([])
      setHasSearched(false)
      return
    }
    setLoading(true)
    setError(null)
    try {
      const data = await searchInternal(q, 20)
      setOrgs(data.orgs)
      setUsers(data.users)
      setHasSearched(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Search failed')
      setOrgs([])
      setUsers([])
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    runSearch(debouncedQuery)
  }, [debouncedQuery, runSearch])

  const totalCount = orgs.length + users.length

  const tabCounts: Record<Tab, number> = {
    all:   totalCount,
    orgs:  orgs.length,
    users: users.length,
  }

  const hasResults = totalCount > 0

  return (
    <div className="p-8 max-w-3xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-base font-semibold text-text-primary">Search</h1>
        <p className="text-xs text-text-quaternary mt-0.5">
          Search across all organizations and users
        </p>
      </div>

      {/* Search bar */}
      <div className="relative flex items-center">
        <SearchIcon className="absolute left-4 w-4 h-4 text-text-quaternary pointer-events-none" />
        <input
          autoFocus
          value={query}
          onChange={e => setQuery(e.target.value)}
          placeholder="Search organizations and users…"
          className="w-full rounded-lg border border-border-primary bg-surface-secondary text-sm text-text-primary pl-10 pr-16 py-2.5 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary transition-colors"
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
        </div>
      </div>

      {/* Tabs — only visible when there are results or loading */}
      {(hasResults || loading) && debouncedQuery && (
        <div className="flex items-center gap-1">
          {TABS.map(({ key, label }) => (
            <button
              key={key}
              onClick={() => setActiveTab(key)}
              className={cn(
                'text-xs px-3 py-1.5 rounded-md transition-colors',
                activeTab === key
                  ? 'bg-accent-blue/10 text-accent-blue font-medium'
                  : 'text-text-quaternary hover:text-text-secondary hover:bg-surface-secondary',
              )}
            >
              {label}
              {tabCounts[key] > 0 && (
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
            <div key={i} className="h-16 animate-pulse bg-surface-secondary rounded-lg" />
          ))}
        </div>
      )}

      {/* Error */}
      {error && !loading && (
        <p className="text-sm text-red-400 text-center">{error}</p>
      )}

      {/* Empty state — no query */}
      {!debouncedQuery && !loading && (
        <div className="text-center py-16">
          <SearchIcon className="w-8 h-8 text-text-quaternary mx-auto mb-3 opacity-40" />
          <p className="text-sm text-text-quaternary">
            Start typing to search all organizations and users
          </p>
        </div>
      )}

      {/* No results */}
      {debouncedQuery && !loading && !error && hasSearched && !hasResults && (
        <div className="text-center py-16">
          <p className="text-sm text-text-quaternary">No results for "{debouncedQuery}"</p>
        </div>
      )}

      {/* Results */}
      {!loading && hasResults && (
        <div className="space-y-6">
          {/* Organizations */}
          {(activeTab === 'all' || activeTab === 'orgs') && orgs.length > 0 && (
            <section className="space-y-3">
              {activeTab === 'all' && (
                <p className="text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">
                  Organizations
                </p>
              )}
              {orgs.map(org => (
                <OrgCard key={org.id} org={org} />
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
        </div>
      )}
    </div>
  )
}
