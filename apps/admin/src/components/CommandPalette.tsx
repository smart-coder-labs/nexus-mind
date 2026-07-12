import { useEffect, useRef, useState, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { Brain, User, FolderOpen, FileStack, Search, Loader2, ChevronRight, Clock, X } from 'lucide-react'
import { createClient } from '../api/client'
import type { GlobalSearchResult } from '../types'

const HISTORY_KEY = 'nexusmind-search-history'
const MAX_HISTORY = 8

// A fully-empty result. Used as a fallback so a completed search always yields a
// non-null object — membership-filtered responses may omit domains entirely, and
// we must render a clear empty state rather than a blank/broken list.
const EMPTY_RESULT: GlobalSearchResult = {
  memories: [],
  users: [],
  projects: [],
  policies: [],
  conventions: [],
  sdd_changes: [],
  sdd_specs: [],
}

function saveToHistory(query: string) {
  if (!query || query.length < 2) return
  const prev: string[] = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]')
  const next = [query, ...prev.filter(q => q !== query)].slice(0, MAX_HISTORY)
  localStorage.setItem(HISTORY_KEY, JSON.stringify(next))
}

function loadHistory(): string[] {
  try { return JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]') }
  catch { return [] }
}

interface CommandPaletteProps {
  open: boolean
  onClose: () => void
}

type FlatResult = { path: string; primary: string; secondary: string; icon: 'memory' | 'user' | 'project' | 'sdd' }

export function flattenResults(results: GlobalSearchResult): FlatResult[] {
  const flat: FlatResult[] = []
  // Membership filtering can omit whole domains, so each array may be missing at
  // runtime even though the type says otherwise — guard every iteration.
  for (const m of results.memories ?? []) {
    flat.push({
      path: '/memories',
      primary: m.title ?? m.content.slice(0, 60),
      secondary: m.project,
      icon: 'memory',
    })
  }
  for (const u of results.users ?? []) {
    flat.push({ path: '/users', primary: u.name, secondary: u.email, icon: 'user' })
  }
  for (const p of results.projects ?? []) {
    flat.push({ path: '/projects', primary: p.name, secondary: p.description ?? p.id, icon: 'project' })
  }
  // Additive facet — absent on an older backend, empty for a caller without
  // `sdd:read` (A4). Both cases simply contribute nothing.
  for (const c of results.sdd_changes ?? []) {
    flat.push({
      path: `/sdd?change=${encodeURIComponent(c.name)}`,
      primary: c.name,
      secondary: c.phase,
      icon: 'sdd',
    })
  }
  return flat
}

function ResultIcon({ kind }: { kind: FlatResult['icon'] }) {
  const cls = 'w-3.5 h-3.5 text-text-quaternary'
  if (kind === 'memory') return <Brain className={cls} />
  if (kind === 'user') return <User className={cls} />
  if (kind === 'sdd') return <FileStack className={cls} />
  return <FolderOpen className={cls} />
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<GlobalSearchResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(-1)
  const [history, setHistory] = useState<string[]>(loadHistory)

  const inputRef = useRef<HTMLInputElement>(null)
  const selectedRef = useRef<HTMLButtonElement>(null)
  const modalRef = useRef<HTMLDivElement>(null)
  const navigate = useNavigate()
  const client = createClient()

  // Focus input when opened
  useEffect(() => {
    if (open) {
      setQuery('')
      setResults(null)
      setSelectedIndex(-1)
      setHistory(loadHistory())
      setTimeout(() => inputRef.current?.focus(), 10)
    }
  }, [open])

  // Focus trap
  useEffect(() => {
    if (!open) return
    const modal = modalRef.current
    if (!modal) return
    const focusable = modal.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    )
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    const trap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last?.focus() }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first?.focus() }
      }
    }
    document.addEventListener('keydown', trap)
    return () => document.removeEventListener('keydown', trap)
  }, [open])

  // Reset selected index when query changes
  useEffect(() => {
    setSelectedIndex(-1)
  }, [query])

  // Scroll selected row into view
  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  // Debounced search
  useEffect(() => {
    if (!open || !query.trim()) {
      setResults(null)
      setLoading(false)
      return
    }
    setLoading(true)
    const timer = setTimeout(async () => {
      try {
        const data = await client.globalSearch(query.trim())
        setResults(data ?? EMPTY_RESULT)
      } catch {
        setResults(EMPTY_RESULT)
      } finally {
        setLoading(false)
      }
    }, 300)
    return () => clearTimeout(timer)
  }, [query, open])

  const flat = results ? flattenResults(results) : []

  const goTo = useCallback(
    (path: string) => {
      if (query.trim().length >= 2) {
        saveToHistory(query.trim())
        setHistory(loadHistory())
      }
      navigate(path)
      onClose()
    },
    [navigate, onClose, query],
  )

  const removeFromHistory = useCallback((h: string) => {
    const prev: string[] = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]')
    const next = prev.filter(q => q !== h)
    localStorage.setItem(HISTORY_KEY, JSON.stringify(next))
    setHistory(next)
  }, [])

  const clearHistory = useCallback(() => {
    localStorage.removeItem(HISTORY_KEY)
    setHistory([])
  }, [])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Escape') {
        onClose()
        return
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setSelectedIndex((i) => Math.min(i + 1, flat.length - 1))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setSelectedIndex((i) => Math.max(i - 1, -1))
        return
      }
      if (e.key === 'Enter') {
        if (selectedIndex >= 0 && flat[selectedIndex]) {
          e.preventDefault()
          goTo(flat[selectedIndex].path)
        } else if (query.trim().length >= 2) {
          // Save on Enter even if no result selected
          saveToHistory(query.trim())
          setHistory(loadHistory())
        }
      }
    },
    [onClose, flat, selectedIndex, goTo, query],
  )

  if (!open) return null

  const hasResults = flat.length > 0
  const isEmpty = results && !hasResults

  // Build sections for display (preserving group headers)
  const memoriesFlat = flat.filter((r) => r.icon === 'memory')
  const usersFlat = flat.filter((r) => r.icon === 'user')
  const projectsFlat = flat.filter((r) => r.icon === 'project')
  const sddFlat = flat.filter((r) => r.icon === 'sdd')

  // Global index offset helpers
  const memoryOffset = 0
  const userOffset = memoriesFlat.length
  const projectOffset = memoriesFlat.length + usersFlat.length
  const sddOffset = memoriesFlat.length + usersFlat.length + projectsFlat.length

  return (
    <div
      className="fixed inset-0 bg-black/60 z-50 flex items-start justify-center pt-[18vh]"
      onClick={onClose}
      aria-modal="true"
      role="dialog"
      aria-label="Command palette"
    >
      <div
        ref={modalRef}
        className="bg-[#1d1d1f] border border-border-primary rounded-[18px] w-full max-w-lg shadow-2xl overflow-hidden mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input row */}
        <div className="relative flex items-center border-b border-border-secondary/60">
          <Search className="w-4 h-4 text-text-quaternary absolute left-4 top-1/2 -translate-y-1/2 pointer-events-none" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search memories, users, projects…"
            aria-label="Search memories, users, and projects"
            aria-autocomplete="list"
            aria-controls="command-palette-results"
            className="w-full bg-transparent pl-10 pr-16 py-3.5 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none"
          />
          <kbd className="absolute right-4 text-[10px] text-text-quaternary bg-[#1d1d1f] border border-border-secondary rounded-[5px] px-1.5 py-0.5 pointer-events-none">
            ESC
          </kbd>
        </div>

        {/* Results container */}
        <div id="command-palette-results" role="listbox" aria-label="Search results" className="max-h-80 overflow-y-auto">
          {/* Initial state — history or prompt */}
          {!query.trim() && !loading && history.length === 0 && (
            <p className="text-xs text-text-tertiary text-center py-8">
              Type to search memories, users, and projects…
            </p>
          )}
          {!query.trim() && !loading && history.length > 0 && (
            <div>
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide font-semibold px-3 pb-1 pt-2">Recent</p>
              {history.map((h, i) => (
                <button
                  key={i}
                  onClick={() => setQuery(h)}
                  className="flex items-center gap-2 w-full px-3 py-2 hover:bg-white/[0.04] text-xs text-text-secondary rounded-[8px] group"
                >
                  <Clock className="w-3 h-3 text-text-quaternary shrink-0" />
                  {h}
                  <button
                    onClick={e => { e.stopPropagation(); removeFromHistory(h) }}
                    className="ml-auto opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-text-primary transition-opacity"
                    aria-label={`Remove "${h}" from history`}
                  >
                    <X className="w-3 h-3" />
                  </button>
                </button>
              ))}
              <button
                onClick={clearHistory}
                className="text-[10px] text-text-quaternary hover:text-text-secondary px-3 py-1.5 transition-colors w-full text-left"
              >
                Clear recent searches
              </button>
            </div>
          )}

          {/* Loading */}
          {loading && (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="w-4 h-4 animate-spin text-text-quaternary" />
            </div>
          )}

          {/* No results — clear empty state (also covers membership-filtered
              searches where every accessible domain returned nothing) */}
          {!loading && isEmpty && (
            <div
              role="status"
              className="flex flex-col items-center justify-center gap-2 py-10 px-6 text-center"
            >
              <Search className="w-5 h-5 text-text-quaternary" />
              <p className="text-xs font-medium text-text-secondary">No results for "{query}"</p>
              <p className="text-[11px] text-text-tertiary">
                Try a different term — you only see items you have access to.
              </p>
            </div>
          )}

          {/* Memories section */}
          {!loading && hasResults && memoriesFlat.length > 0 && (
            <section>
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide font-semibold px-3 pb-1 pt-3">
                Memories
              </p>
              {memoriesFlat.map((r, i) => {
                const globalIdx = memoryOffset + i
                const isSelected = selectedIndex === globalIdx
                return (
                  <button
                    key={`memory-${i}`}
                    ref={isSelected ? selectedRef : null}
                    onClick={() => goTo(r.path)}
                    role="option"
                    aria-selected={isSelected}
                    className={`flex items-center gap-2 px-3 py-2 w-full cursor-pointer transition-colors text-left rounded-[8px] group ${
                      isSelected ? 'bg-white/[0.06]' : 'hover:bg-white/[0.04]'
                    }`}
                  >
                    <span className="w-7 h-7 rounded-[8px] bg-[#1d1d1f] flex items-center justify-center shrink-0">
                      <ResultIcon kind="memory" />
                    </span>
                    <span className="flex flex-col min-w-0 flex-1">
                      <span className="text-xs text-text-secondary truncate">{r.primary}</span>
                      {r.secondary && (
                        <span className="text-[10px] text-text-quaternary truncate">{r.secondary}</span>
                      )}
                    </span>
                    <ChevronRight className="w-3.5 h-3.5 text-text-quaternary shrink-0 ml-auto" />
                  </button>
                )
              })}
            </section>
          )}

          {/* Users section */}
          {!loading && hasResults && usersFlat.length > 0 && (
            <section>
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide font-semibold px-3 pb-1 pt-3">
                Users
              </p>
              {usersFlat.map((r, i) => {
                const globalIdx = userOffset + i
                const isSelected = selectedIndex === globalIdx
                return (
                  <button
                    key={`user-${i}`}
                    ref={isSelected ? selectedRef : null}
                    onClick={() => goTo(r.path)}
                    role="option"
                    aria-selected={isSelected}
                    className={`flex items-center gap-2 px-3 py-2 w-full cursor-pointer transition-colors text-left rounded-[8px] group ${
                      isSelected ? 'bg-white/[0.06]' : 'hover:bg-white/[0.04]'
                    }`}
                  >
                    <span className="w-7 h-7 rounded-[8px] bg-[#1d1d1f] flex items-center justify-center shrink-0">
                      <ResultIcon kind="user" />
                    </span>
                    <span className="flex flex-col min-w-0 flex-1">
                      <span className="text-xs text-text-secondary truncate">{r.primary}</span>
                      {r.secondary && (
                        <span className="text-[10px] text-text-quaternary truncate">{r.secondary}</span>
                      )}
                    </span>
                    <ChevronRight className="w-3.5 h-3.5 text-text-quaternary shrink-0 ml-auto" />
                  </button>
                )
              })}
            </section>
          )}

          {/* Projects section */}
          {!loading && hasResults && projectsFlat.length > 0 && (
            <section>
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide font-semibold px-3 pb-1 pt-3">
                Projects
              </p>
              {projectsFlat.map((r, i) => {
                const globalIdx = projectOffset + i
                const isSelected = selectedIndex === globalIdx
                return (
                  <button
                    key={`project-${i}`}
                    ref={isSelected ? selectedRef : null}
                    onClick={() => goTo(r.path)}
                    role="option"
                    aria-selected={isSelected}
                    className={`flex items-center gap-2 px-3 py-2 w-full cursor-pointer transition-colors text-left rounded-[8px] group ${
                      isSelected ? 'bg-white/[0.06]' : 'hover:bg-white/[0.04]'
                    }`}
                  >
                    <span className="w-7 h-7 rounded-[8px] bg-[#1d1d1f] flex items-center justify-center shrink-0">
                      <ResultIcon kind="project" />
                    </span>
                    <span className="flex flex-col min-w-0 flex-1">
                      <span className="text-xs text-text-secondary truncate">{r.primary}</span>
                      {r.secondary && (
                        <span className="text-[10px] text-text-quaternary truncate">{r.secondary}</span>
                      )}
                    </span>
                    <ChevronRight className="w-3.5 h-3.5 text-text-quaternary shrink-0 ml-auto" />
                  </button>
                )
              })}
            </section>
          )}

          {/* SDD section — omitted, never rendered empty */}
          {!loading && hasResults && sddFlat.length > 0 && (
            <section>
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide font-semibold px-3 pb-1 pt-3">
                SDD
              </p>
              {sddFlat.map((r, i) => {
                const globalIdx = sddOffset + i
                const isSelected = selectedIndex === globalIdx
                return (
                  <button
                    key={`sdd-${i}`}
                    ref={isSelected ? selectedRef : null}
                    onClick={() => goTo(r.path)}
                    role="option"
                    aria-selected={isSelected}
                    className={`flex items-center gap-2 px-3 py-2 w-full cursor-pointer transition-colors text-left rounded-[8px] group ${
                      isSelected ? 'bg-white/[0.06]' : 'hover:bg-white/[0.04]'
                    }`}
                  >
                    <span className="w-7 h-7 rounded-[8px] bg-[#1d1d1f] flex items-center justify-center shrink-0">
                      <ResultIcon kind="sdd" />
                    </span>
                    <span className="flex flex-col min-w-0 flex-1">
                      <span className="text-xs text-text-secondary truncate">{r.primary}</span>
                      {r.secondary && (
                        <span className="text-[10px] text-text-quaternary truncate">{r.secondary}</span>
                      )}
                    </span>
                    <ChevronRight className="w-3.5 h-3.5 text-text-quaternary shrink-0 ml-auto" />
                  </button>
                )
              })}
            </section>
          )}
        </div>
      </div>
    </div>
  )
}
