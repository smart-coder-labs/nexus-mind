import { useMemo, useState, useCallback, useRef, useEffect, type ReactNode } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Loader2, Search, ChevronDown, ChevronRight, Bookmark, BookmarkCheck, Trash2, X, RefreshCw, CheckCircle2, AlertCircle, Clock, RotateCcw, ArchiveX, Download, Copy, Check, Plus, FileText, Lock, Eye, EyeOff, Code2, GitBranch, MapPin } from 'lucide-react'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { CodeProject, CodeSearchResult, LocateResult } from '../types'
import { StatTile } from './dashboard/StatTile'
import { accentFor } from './dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'

// ── Saved searches ─────────────────────────────────────────────────────────────

const LS_KEY = 'nexusmind-code-searches'

interface SavedSearch {
  id: string
  name: string
  projectId: string
  query: string
}

function loadSaved(): SavedSearch[] {
  try {
    return JSON.parse(localStorage.getItem(LS_KEY) ?? '[]')
  } catch {
    return []
  }
}

function persistSaved(items: SavedSearch[]) {
  localStorage.setItem(LS_KEY, JSON.stringify(items))
}

function downloadBlob(data: object, filename: string) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url; a.download = filename; a.click()
  URL.revokeObjectURL(url)
}

const INPUT_CLS =
  'w-full bg-white/[0.04] border border-border-primary rounded-[11px] px-3 py-2.5 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Code page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// ── Private-repo detection ─────────────────────────────────────────────────────

/** Parses a GitHub URL and returns owner/repo or null if not a GitHub URL. */
function parseGitHubRepo(url: string): { owner: string; repo: string } | null {
  try {
    const u = new URL(url)
    if (u.hostname !== 'github.com') return null
    const parts = u.pathname.replace(/^\//, '').replace(/\.git$/, '').split('/')
    if (parts.length < 2 || !parts[0] || !parts[1]) return null
    return { owner: parts[0], repo: parts[1] }
  } catch {
    return null
  }
}

type RepoAccessState = 'idle' | 'checking' | 'accessible' | 'needs-token' | 'token-invalid'

/**
 * Check whether a GitHub repository is accessible with or without a PAT.
 * Calls the GitHub REST API directly from the browser (CORS is allowed for unauthenticated
 * and PAT-authenticated requests). Returns 'accessible', 'needs-token', or 'token-invalid'.
 */
async function checkGitHubAccess(
  url: string,
  token?: string,
): Promise<'accessible' | 'needs-token' | 'token-invalid'> {
  const parsed = parseGitHubRepo(url)
  if (!parsed) return 'accessible' // Non-GitHub URL — skip check

  const headers: Record<string, string> = {
    Accept: 'application/vnd.github.v3+json',
  }
  if (token) headers['Authorization'] = `Bearer ${token}`

  try {
    const res = await fetch(
      `https://api.github.com/repos/${parsed.owner}/${parsed.repo}`,
      { headers },
    )
    if (res.status === 200) return 'accessible'
    if (!token) return 'needs-token'
    return 'token-invalid'
  } catch {
    return 'accessible' // Network error — let the server handle it
  }
}

// ── Code highlight helper ──────────────────────────────────────────────────────

function highlightCode(text: string, query: string): ReactNode {
  if (!query || query.length < 2) return text
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const parts = text.split(new RegExp(`(${escaped})`, 'gi'))
  return parts.map((part, i) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark key={i} className="bg-accent-blue/20 text-accent-blue rounded-[2px] px-0.5 not-italic">{part}</mark>
    ) : part
  )
}

// ── Shared helpers ─────────────────────────────────────────────────────────────

function StatusChip({ project }: { project: CodeProject }) {
  const indexed = project.last_indexed != null
  if (indexed) {
    return (
      <span className="text-[10px] font-semibold border rounded-[5px] px-1.5 py-0.5 text-status-success bg-status-success/10 border-status-success/20">
        indexed
      </span>
    )
  }
  return (
    <span className="text-[10px] font-semibold border rounded-[5px] px-1.5 py-0.5 text-text-quaternary bg-white/[0.06] border-white/[0.09]">
      not indexed
    </span>
  )
}

function SkeletonRow() {
  return (
    <div className="px-5 py-4 animate-pulse">
      <div className="h-4 bg-white/[0.04] rounded-[5px] w-1/3 mb-2" />
      <div className="h-3 bg-white/[0.04] rounded-[5px] w-1/2 mb-2" />
      <div className="h-3 bg-white/[0.04] rounded-[5px] w-2/3" />
    </div>
  )
}

function formatDate(iso: string) {
  try {
    return new Date(iso).toLocaleString()
  } catch {
    return iso
  }
}

function relativeTime(iso: string): string {
  try {
    const diffMs = Date.now() - new Date(iso).getTime()
    const diffMin = Math.floor(diffMs / 60_000)
    if (diffMin < 60) return `${diffMin}m ago`
    const diffH = Math.floor(diffMin / 60)
    if (diffH < 24) return `${diffH}h ago`
    return `${Math.floor(diffH / 24)}d ago`
  } catch {
    return iso
  }
}

function StatusBadge({ project }: { project: CodeProject }) {
  if (project.index_status === 'indexing') {
    return (
      <span className="flex items-center gap-1 text-[10px] text-status-warning">
        <RefreshCw className="w-3 h-3 animate-spin" /> Indexing…
      </span>
    )
  }
  if (project.index_status === 'success') {
    return (
      <span className="flex items-center gap-1 text-[10px] text-status-success">
        <CheckCircle2 className="w-3 h-3" />
        {project.last_indexed_at ? relativeTime(project.last_indexed_at) : 'Synced'}
      </span>
    )
  }
  if (project.index_status === 'error') {
    return (
      <span className="flex items-center gap-1 text-[10px] text-status-error" title={project.last_index_error ?? ''}>
        <AlertCircle className="w-3 h-3" /> Error
      </span>
    )
  }
  return (
    <span className="flex items-center gap-1 text-[10px] text-text-quaternary">
      <Clock className="w-3 h-3" /> Pending
    </span>
  )
}

// ── Score badge ───────────────────────────────────────────────────────────────

function ScoreBadge({ score }: { score: number }) {
  const pct = Math.round(score * 100)
  const cls =
    pct >= 80
      ? 'text-status-success bg-status-success/10 border-status-success/20'
      : pct >= 50
      ? 'text-status-warning bg-status-warning/10 border-status-warning/20'
      : 'text-text-quaternary bg-white/[0.06] border-white/[0.09]'
  return (
    <span className={`text-[10px] font-semibold border rounded-[5px] px-2 py-0.5 shrink-0 ${cls}`}>
      {pct}%
    </span>
  )
}

// ── Search result row ─────────────────────────────────────────────────────────

function SearchResultRow({ result, searchQuery }: { result: CodeSearchResult; searchQuery: string }) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div className={`rounded-[18px] overflow-hidden ${GLASS_PANEL}`}>
      <button
        type="button"
        onClick={() => setExpanded(v => !v)}
        className="w-full flex items-start gap-3 px-4 py-3 hover:bg-accent-blue/[0.04] transition-colors text-left"
        aria-expanded={expanded}
        aria-label={expanded ? "Collapse code snippet" : "Expand code snippet"}
      >
        <span className="mt-0.5 shrink-0 text-text-quaternary">
          {expanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
        </span>
        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-xs font-semibold font-mono text-text-primary truncate">
              {highlightCode(result.file_path, searchQuery)}
            </span>
            {result.symbol && (
              <span className="text-[10px] font-mono text-accent-blue bg-accent-blue/8 rounded-[5px] px-1.5 py-0.5 shrink-0">
                {result.symbol}
              </span>
            )}
          </div>
          <p className="text-[11px] text-text-quaternary">
            lines {result.start_line}–{result.end_line}
          </p>
        </div>
        <ScoreBadge score={result.score} />
      </button>

      {expanded && (
        <div className="border-t border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] rounded-b-[18px]">
          <pre className="px-4 py-3 text-[10px] font-mono text-text-secondary leading-relaxed overflow-x-auto whitespace-pre-wrap break-words">
            {highlightCode(result.content, searchQuery)}
          </pre>
          {/* Indexed skeleton — the compact text (symbol + signature + doc) that
              was actually embedded. Hidden when the backend omits it. */}
          {result.skeleton && result.skeleton.trim() && (
            <div className="border-t border-white/[0.07] px-4 py-3">
              <p className="text-[9px] font-semibold tracking-[0.08em] uppercase text-text-quaternary mb-1.5 flex items-center gap-1">
                <Code2 className="w-3 h-3" aria-hidden="true" />
                indexed skeleton
              </p>
              <pre className="text-[10px] font-mono text-accent-blue/80 leading-relaxed overflow-x-auto whitespace-pre-wrap break-words">
                {result.skeleton}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

// ── Locate result row (paths-only) ────────────────────────────────────────────

function LocateResultRow({ result }: { result: LocateResult }) {
  const [copied, setCopied] = useState(false)

  const copyPath = useCallback(() => {
    navigator.clipboard.writeText(result.file_path).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    })
  }, [result.file_path])

  return (
    <button
      type="button"
      onClick={copyPath}
      title="Copy file path"
      className={`w-full flex items-center gap-3 px-4 py-2.5 rounded-[12px] text-left hover:bg-accent-blue/[0.04] transition-colors ${GLASS_PANEL}`}
    >
      <span className="shrink-0 text-text-quaternary">
        {copied ? <Check className="w-3.5 h-3.5 text-status-success" /> : <Copy className="w-3.5 h-3.5" />}
      </span>
      <span className="flex-1 min-w-0 text-xs font-mono text-text-primary truncate">
        {result.file_path}
      </span>
      {result.top_symbol && (
        <span className="text-[10px] font-mono text-accent-blue bg-accent-blue/8 rounded-[5px] px-1.5 py-0.5 shrink-0 truncate max-w-[38%]">
          {result.top_symbol}
        </span>
      )}
      <ScoreBadge score={result.score} />
    </button>
  )
}

// ── Search tab ────────────────────────────────────────────────────────────────

function CodeSearchTab({ projects }: { projects: CodeProject[] | undefined }) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const [selectedProject, setSelectedProject] = useState('')
  const [query, setQuery] = useState('')
  const [submittedQuery, setSubmittedQuery] = useState('')
  const [submittedProject, setSubmittedProject] = useState('')
  const [extensionFilter, setExtensionFilter] = useState('')
  const [submittedExtension, setSubmittedExtension] = useState('')
  const [copied, setCopied] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)

  // Search (full results) vs Locate (paths-only). `submittedMode` freezes the
  // mode the visible results belong to, so the header/branching never disagrees
  // with what was actually fetched.
  const [mode, setMode] = useState<'search' | 'locate'>('search')
  const [submittedMode, setSubmittedMode] = useState<'search' | 'locate'>('search')

  // Saved searches state
  const [savedSearches, setSavedSearches] = useState<SavedSearch[]>(loadSaved)
  const [showSavePopover, setShowSavePopover] = useState(false)
  const [saveName, setSaveName] = useState('')
  const [showSavedDropdown, setShowSavedDropdown] = useState(false)
  const savePopoverRef = useRef<HTMLDivElement>(null)
  const savedDropdownRef = useRef<HTMLDivElement>(null)

  // Close popovers on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (savePopoverRef.current && !savePopoverRef.current.contains(e.target as Node)) {
        setShowSavePopover(false)
      }
      if (savedDropdownRef.current && !savedDropdownRef.current.contains(e.target as Node)) {
        setShowSavedDropdown(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [])

  const indexedProjects = useMemo(
    () => projects?.filter(p => p.last_indexed != null) ?? [],
    [projects],
  )

  const hasQuery = submittedQuery.trim().length > 0 && submittedProject.trim().length > 0

  const { data: results, isLoading, isError, error } = useQuery({
    queryKey: ['code-search', submittedProject, submittedQuery, submittedExtension],
    queryFn: () => client.searchCode(submittedProject, submittedQuery, 10, submittedExtension || undefined),
    enabled: submittedMode === 'search' && hasQuery,
    retry: false,
  })

  const {
    data: locateResults,
    isLoading: locateLoading,
    isError: locateIsError,
    error: locateError,
  } = useQuery({
    queryKey: ['code-locate', submittedProject, submittedQuery],
    queryFn: () => client.locateCode(submittedProject, submittedQuery, 10),
    enabled: submittedMode === 'locate' && hasQuery,
    retry: false,
  })

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault()
      const q = query.trim()
      const p = selectedProject.trim()
      if (!q || !p) return
      setSubmittedQuery(q)
      setSubmittedProject(p)
      setSubmittedExtension(extensionFilter)
      setSubmittedMode(mode)
    },
    [query, selectedProject, extensionFilter, mode],
  )

  // Toggling the mode re-runs the already-submitted query in the new mode, so
  // the switch feels instant once a search exists; before any search it just
  // arms the button.
  const handleModeChange = useCallback(
    (m: 'search' | 'locate') => {
      setMode(m)
      if (submittedQuery.trim() && submittedProject.trim()) setSubmittedMode(m)
    },
    [submittedQuery, submittedProject],
  )

  const handleSaveSearch = useCallback(() => {
    const name = saveName.trim()
    if (!name || !query.trim() || !selectedProject) return
    const next: SavedSearch = {
      id: crypto.randomUUID(),
      name,
      projectId: selectedProject,
      query: query.trim(),
    }
    const updated = [...savedSearches, next]
    setSavedSearches(updated)
    persistSaved(updated)
    setSaveName('')
    setShowSavePopover(false)
  }, [saveName, query, selectedProject, savedSearches])

  const handleDeleteSaved = useCallback((id: string) => {
    const updated = savedSearches.filter(s => s.id !== id)
    setSavedSearches(updated)
    persistSaved(updated)
  }, [savedSearches])

  const handleLoadSaved = useCallback((s: SavedSearch) => {
    setSelectedProject(s.projectId)
    setQuery(s.query)
    setSubmittedQuery(s.query)
    setSubmittedProject(s.projectId)
    setShowSavedDropdown(false)
  }, [])

  const hasSearched = submittedQuery.length > 0 && submittedProject.length > 0
  const canSave = query.trim().length > 0 && selectedProject.length > 0
  const busy = isLoading || locateLoading

  if (indexedProjects.length === 0) {
    return (
      <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
        <p className="text-xs font-semibold text-text-primary">No indexed repositories yet.</p>
        <p className="text-xs text-text-quaternary">
          Index a repository in the Repositories tab to enable semantic search.
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-5">
      {/* Search form */}
      <form onSubmit={handleSubmit} className="border border-border-primary rounded-[18px] p-5 space-y-4">
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <p className="text-[12px] tracking-[-0.12px] text-text-tertiary">
            {mode === 'locate' ? 'Locate Files' : 'Semantic Code Search'}
          </p>
          {/* Search vs Locate segmented toggle */}
          <div className="bg-white/[0.04] border border-white/[0.09] rounded-[11px] p-0.5 flex items-center gap-0.5">
            <button
              type="button"
              onClick={() => handleModeChange('search')}
              aria-pressed={mode === 'search'}
              className={`flex items-center gap-1.5 px-2.5 py-1 text-[10px] rounded-[8px] transition-colors ${
                mode === 'search'
                  ? 'bg-accent-blue/15 text-accent-blue font-semibold'
                  : 'text-text-tertiary hover:text-text-secondary'
              }`}
            >
              <Search className="w-3 h-3" />
              Search
            </button>
            <button
              type="button"
              onClick={() => handleModeChange('locate')}
              aria-pressed={mode === 'locate'}
              className={`flex items-center gap-1.5 px-2.5 py-1 text-[10px] rounded-[8px] transition-colors ${
                mode === 'locate'
                  ? 'bg-accent-blue/15 text-accent-blue font-semibold'
                  : 'text-text-tertiary hover:text-text-secondary'
              }`}
            >
              <MapPin className="w-3 h-3" />
              Locate
            </button>
          </div>
        </div>

        <div className="flex gap-3 flex-col sm:flex-row">
          {/* Project selector */}
          <div className="sm:w-56 shrink-0">
            <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary mb-1.5">
              Project
            </label>
            <select
              value={selectedProject}
              onChange={e => setSelectedProject(e.target.value)}
              required
              className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors"
            >
              <option value="">Select a project…</option>
              {indexedProjects.map(p => (
                <option key={p.id} value={p.name}>{p.name}</option>
              ))}
            </select>
          </div>

          {/* Extension filter — only applies to full Search (Locate returns paths) */}
          {mode === 'search' && (
          <div className="sm:w-40 shrink-0">
            <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary mb-1.5">
              Extension
            </label>
            <select
              value={extensionFilter}
              onChange={e => setExtensionFilter(e.target.value)}
              className="w-full bg-transparent border border-border-secondary/40 rounded-[8px] text-xs text-text-secondary px-2 py-1.5 focus:border-accent-blue/60 focus:outline-none"
            >
              <option value="">All files</option>
              <option value="ts">TypeScript (.ts)</option>
              <option value="tsx">React (.tsx)</option>
              <option value="rs">Rust (.rs)</option>
              <option value="py">Python (.py)</option>
              <option value="js">JavaScript (.js)</option>
              <option value="go">Go (.go)</option>
              <option value="java">Java (.java)</option>
              <option value="md">Markdown (.md)</option>
            </select>
          </div>
          )}

          {/* Query input */}
          <div className="flex-1">
            <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary mb-1.5">
              Query
            </label>
            <input
              ref={inputRef}
              className={INPUT_CLS}
              placeholder="e.g. authentication middleware, JWT token refresh…"
              value={query}
              onChange={e => setQuery(e.target.value)}
              required
            />
          </div>
        </div>

        <div className="flex items-center justify-between gap-2 flex-wrap">
          {/* Saved searches pill */}
          <div className="relative" ref={savedDropdownRef}>
            {savedSearches.length > 0 && (
              <>
                <button
                  type="button"
                  onClick={() => setShowSavedDropdown(v => !v)}
                  className="flex items-center gap-1.5 border border-border-primary rounded-full px-2.5 py-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors"
                >
                  <BookmarkCheck className="w-3 h-3" />
                  Saved searches
                  <span className="bg-accent-blue text-white text-[10px] rounded-full w-4 h-4 flex items-center justify-center">
                    {savedSearches.length}
                  </span>
                </button>

                {showSavedDropdown && (
                  <div className="absolute left-0 top-full mt-1.5 z-20 border border-white/[0.10] bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] rounded-[12px] p-[5px] min-w-[220px]">
                    {savedSearches.map(s => (
                      <div
                        key={s.id}
                        className="flex items-center justify-between gap-[10px] px-[11px] py-[9px] rounded-[8px] hover:bg-white/[0.06] group cursor-pointer"
                        onClick={() => handleLoadSaved(s)}
                      >
                        <p className="text-[10px] text-text-quaternary uppercase tracking-wide shrink-0">{s.projectId}</p>
                        <p className="text-xs text-text-secondary truncate flex-1 ml-2">{s.query}</p>
                        <button
                          type="button"
                          onClick={e => { e.stopPropagation(); handleDeleteSaved(s.id) }}
                          className="shrink-0 opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-status-error transition-opacity ml-2"
                          aria-label={`Delete saved search "${s.name}"`}
                        >
                          <Trash2 className="w-3 h-3" />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
          </div>

          {/* Right-side actions */}
          <div className="flex items-center gap-2 ml-auto">
            {/* Save search button + inline popover */}
            {canSave && (
              <div className="relative" ref={savePopoverRef}>
                <button
                  type="button"
                  onClick={() => { setShowSavePopover(v => !v); setSaveName('') }}
                  className="border border-border-primary rounded-[8px] px-2.5 py-1.5 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
                >
                  <Bookmark className="w-3 h-3" />
                  Save
                </button>

                {showSavePopover && (
                  <div className="absolute right-0 top-full mt-1.5 z-20 border border-white/[0.10] bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] rounded-[11px] p-3 w-56 space-y-2">
                    <div className="flex items-center gap-1.5">
                      <input
                        autoFocus
                        type="text"
                        value={saveName}
                        onChange={e => setSaveName(e.target.value)}
                        onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); handleSaveSearch() } if (e.key === 'Escape') setShowSavePopover(false) }}
                        placeholder="Search name…"
                        className="flex-1 min-w-0 bg-white/[0.04] border border-border-primary rounded-[8px] px-2 py-1 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors"
                      />
                      <button
                        type="button"
                        onClick={() => setShowSavePopover(false)}
                        className="text-text-quaternary hover:text-text-secondary transition-colors shrink-0"
                        aria-label="Cancel"
                      >
                        <X className="w-3.5 h-3.5" />
                      </button>
                    </div>
                    <button
                      type="button"
                      onClick={handleSaveSearch}
                      disabled={!saveName.trim()}
                      className="w-full rounded-[8px] bg-accent-blue text-white text-xs font-semibold px-2.5 py-1.5 hover:opacity-90 transition-opacity disabled:opacity-50"
                    >
                      Save
                    </button>
                  </div>
                )}
              </div>
            )}

            <button
              type="submit"
              disabled={busy || !query.trim() || !selectedProject}
              className="flex items-center gap-1.5 bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold hover:opacity-90 transition-opacity disabled:opacity-50"
            >
              {busy && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
              {busy
                ? (mode === 'locate' ? 'Locating…' : 'Searching…')
                : (mode === 'locate' ? 'Locate' : 'Search')}
            </button>
          </div>
        </div>
      </form>

      {/* Results */}
      {hasSearched && submittedMode === 'search' && (
        <div className="space-y-3">
          {isError && (
            <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
              {(error as Error)?.message ?? 'Search failed.'}
            </div>
          )}

          {!isLoading && !isError && results !== undefined && (
            <>
              {results.length === 0 ? (
                <div className="border border-border-primary rounded-[18px] p-10 flex flex-col items-center gap-2 text-center">
                  <Search className="w-6 h-6 text-text-quaternary/50" />
                  <p className="text-xs font-semibold text-text-secondary">No results found</p>
                  <p className="text-xs text-text-quaternary max-w-xs">
                    Try a different query or check that the project is indexed.
                  </p>
                </div>
              ) : (
                <>
                  <div className="flex items-center gap-2 flex-wrap">
                    <p className="text-[12px] text-text-quaternary tracking-[-0.12px]">
                      {results.length} result{results.length === 1 ? '' : 's'} for &ldquo;{submittedQuery}&rdquo; in {submittedProject}
                    </p>
                    {submittedExtension && (
                      <span className="text-[10px] text-text-quaternary bg-white/[0.04] rounded-[5px] px-1.5 py-0.5 border border-border-secondary/50 flex items-center gap-1">
                        .{submittedExtension} <button onClick={() => { setExtensionFilter(''); setSubmittedExtension('') }} className="text-text-quaternary hover:text-text-primary">×</button>
                      </span>
                    )}
                    <div className="ml-auto flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => {
                          downloadBlob(
                            {
                              query: submittedQuery,
                              total: results.length,
                              exported_at: new Date().toISOString(),
                              results: results.map(r => ({
                                file: r.file_path,
                                line: r.start_line,
                                snippet: r.content,
                                score: r.score,
                                project: submittedProject,
                              })),
                            },
                            `code-search-${submittedQuery.replace(/\s+/g, '-').toLowerCase()}.json`,
                          )
                        }}
                        className="border border-border-primary rounded-full px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
                      >
                        <Download className="w-3 h-3" />
                        Export
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          const text = results
                            .map(r => `// ${r.file_path}:${r.start_line}\n${r.content}`)
                            .join('\n\n')
                          navigator.clipboard.writeText(text).then(() => {
                            setCopied(true)
                            setTimeout(() => setCopied(false), 2000)
                          })
                        }}
                        className={`border border-border-primary rounded-full px-2.5 py-1 text-xs flex items-center gap-1.5 transition-colors ${copied ? 'text-status-success' : 'text-text-secondary hover:text-text-primary'}`}
                      >
                        {copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                        {copied ? 'Copied!' : 'Copy snippets'}
                      </button>
                    </div>
                  </div>
                  <div className="space-y-2">
                    {results.map((r, i) => (
                      <SearchResultRow key={`${r.file_path}-${r.start_line}-${i}`} result={r} searchQuery={submittedQuery} />
                    ))}
                  </div>
                </>
              )}
            </>
          )}
        </div>
      )}

      {/* Locate results — paths-only ranked list */}
      {hasSearched && submittedMode === 'locate' && (
        <div className="space-y-3">
          {locateIsError && (
            <div className="border border-status-error/20 rounded-[11px] px-4 py-3 text-xs text-status-error/80">
              {(locateError as Error)?.message ?? 'Locate failed.'}
            </div>
          )}

          {!locateLoading && !locateIsError && locateResults !== undefined && (
            <>
              {locateResults.length === 0 ? (
                <div className="border border-border-primary rounded-[18px] p-10 flex flex-col items-center gap-2 text-center">
                  <MapPin className="w-6 h-6 text-text-quaternary/50" />
                  <p className="text-xs font-semibold text-text-secondary">No files found</p>
                  <p className="text-xs text-text-quaternary max-w-xs">
                    Try a different query or check that the project is indexed.
                  </p>
                </div>
              ) : (
                <>
                  <div className="flex items-center gap-2 flex-wrap">
                    <p className="text-[12px] text-text-quaternary tracking-[-0.12px]">
                      {locateResults.length} file{locateResults.length === 1 ? '' : 's'} for &ldquo;{submittedQuery}&rdquo; in {submittedProject}
                    </p>
                    <div className="ml-auto flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => {
                          const text = locateResults.map(r => r.file_path).join('\n')
                          navigator.clipboard.writeText(text).then(() => {
                            setCopied(true)
                            setTimeout(() => setCopied(false), 2000)
                          })
                        }}
                        className={`border border-border-primary rounded-full px-2.5 py-1 text-xs flex items-center gap-1.5 transition-colors ${copied ? 'text-status-success' : 'text-text-secondary hover:text-text-primary'}`}
                      >
                        {copied ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                        {copied ? 'Copied!' : 'Copy paths'}
                      </button>
                    </div>
                  </div>
                  <div className="space-y-2">
                    {locateResults.map((r, i) => (
                      <LocateResultRow key={`${r.file_path}-${i}`} result={r} />
                    ))}
                  </div>
                </>
              )}
            </>
          )}
        </div>
      )}
    </div>
  )
}

// ── Exclude patterns editor ───────────────────────────────────────────────────

function ExcludePatternsEditor({
  project,
  onSave,
  isSaving,
}: {
  project: CodeProject
  onSave: (patterns: string[]) => void
  isSaving: boolean
}) {
  const [patterns, setPatterns] = useState<string[]>(project.exclude_patterns ?? [])
  const [input, setInput] = useState('')
  const isDirty = JSON.stringify(patterns) !== JSON.stringify(project.exclude_patterns ?? [])

  const addPatterns = useCallback((raw: string) => {
    const incoming = raw
      .split(',')
      .map(s => s.trim())
      .filter(s => s.length > 0)
    setPatterns(prev => {
      const next = [...prev, ...incoming.filter(p => !prev.includes(p))].slice(0, 20)
      return next
    })
    setInput('')
  }, [])

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      addPatterns(input)
    }
  }

  const removePattern = (idx: number) => {
    setPatterns(prev => prev.filter((_, i) => i !== idx))
  }

  return (
    <div className="pt-2 space-y-2">
      <span className="font-semibold text-[10px] text-text-quaternary uppercase tracking-wide">Exclude patterns:</span>
      {/* Pill list */}
      {patterns.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {patterns.map((pat, idx) => (
            <span
              key={idx}
              className="bg-white/[0.06] rounded-full px-2 py-0.5 text-[10px] text-text-secondary flex items-center gap-1"
            >
              {pat}
              <button
                type="button"
                onClick={() => removePattern(idx)}
                aria-label={`Remove pattern ${pat}`}
                className="text-text-quaternary hover:text-status-error"
              >
                <X className="w-2.5 h-2.5" />
              </button>
            </span>
          ))}
        </div>
      )}
      {/* Input row */}
      <div className="flex items-center gap-2">
        <input
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="e.g. *.lock, node_modules/*"
          disabled={patterns.length >= 20}
          className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 focus:outline-none focus:border-accent-blue/60 flex-1 placeholder:text-text-quaternary disabled:opacity-40"
        />
        <button
          type="button"
          onClick={() => addPatterns(input)}
          disabled={!input.trim() || patterns.length >= 20}
          aria-label="Add pattern"
          className="border border-border-primary rounded-[8px] px-2 py-1.5 text-text-quaternary hover:text-text-primary disabled:opacity-40 transition-colors"
        >
          <Plus className="w-3 h-3" />
        </button>
        {isDirty && (
          <button
            type="button"
            onClick={() => onSave(patterns)}
            disabled={isSaving}
            className="border border-border-primary rounded-full px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50 flex items-center gap-1"
          >
            {isSaving && <Loader2 className="w-2.5 h-2.5 animate-spin" />}
            Save
          </button>
        )}
      </div>
      {patterns.length >= 20 && (
        <p className="text-[10px] text-status-error">Maximum 20 patterns reached.</p>
      )}
    </div>
  )
}

// ── Repositories tab ──────────────────────────────────────────────────────────

function RepositoriesTab({
  projects,
  isLoading,
  showArchived,
  onToggleArchived,
}: {
  projects: CodeProject[] | undefined
  isLoading: boolean
  showArchived: boolean
  onToggleArchived: () => void
}) {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [showForm, setShowForm] = useState(false)
  const [repoUrl, setRepoUrl] = useState('')
  const [graphOnly, setGraphOnly] = useState(false)
  const [selectedProject, setSelectedProject] = useState('')
  const [projectMode, setProjectMode] = useState<'existing' | 'new'>('existing')
  const [newProjectName, setNewProjectName] = useState('')
  const [indexError, setIndexError] = useState<string | null>(null)
  const [expandedFiles, setExpandedFiles] = useState<string | null>(null)

  // Private-repo PAT state
  const [repoAccess, setRepoAccess] = useState<RepoAccessState>('idle')
  const [githubToken, setGithubToken] = useState('')
  const [showToken, setShowToken] = useState(false)
  const [tokenValidating, setTokenValidating] = useState(false)

  const { data: files, isLoading: filesLoading } = useQuery({
    queryKey: ['code-project-files', expandedFiles],
    queryFn: () => client.getCodeProjectFiles(expandedFiles!),
    enabled: !!expandedFiles,
  })

  const { data: memProjects } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    enabled: showForm,
  })

  const indexMut = useMutation({
    mutationFn: (data: { project: string; repo_url?: string; root_path?: string; github_token?: string; graph_only?: boolean }) => client.indexProject(data),
    onSuccess: () => {
      setRepoUrl('')
      setSelectedProject('')
      setNewProjectName('')
      setGithubToken('')
      setShowToken(false)
      setRepoAccess('idle')
      setShowForm(false)
      setIndexError(null)
      qc.invalidateQueries({ queryKey: ['code-projects'] })
    },
    onError: (err: Error) => setIndexError(err.message),
  })

  const reindexMut = useMutation({
    mutationFn: (p: CodeProject) => client.reindexCodeProject(p.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (name: string) => client.deleteCodeProject(name),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const scheduleMut = useMutation({
    mutationFn: ({ id, interval_hours }: { id: string; interval_hours: number | null }) =>
      client.updateCodeProjectSchedule(id, interval_hours),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const archiveMut = useMutation({
    mutationFn: (p: CodeProject) => client.archiveCodeProject(p.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const restoreMut = useMutation({
    mutationFn: (p: CodeProject) => client.restoreCodeProject(p.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const updateProjectMut = useMutation({
    mutationFn: ({ id, exclude_patterns }: { id: string; exclude_patterns: string[] }) =>
      client.updateCodeProject(id, { exclude_patterns }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  // Check repo accessibility on URL blur; if inaccessible, reveal the PAT field.
  const handleRepoUrlBlur = useCallback(async () => {
    const url = repoUrl.trim()
    if (!url || !parseGitHubRepo(url)) {
      setRepoAccess('idle')
      return
    }
    setRepoAccess('checking')
    const result = await checkGitHubAccess(url)
    setRepoAccess(result === 'accessible' ? 'accessible' : 'needs-token')
  }, [repoUrl])

  // Validate the token against the repo when user changes the token field.
  const handleTokenBlur = useCallback(async () => {
    const url = repoUrl.trim()
    const tok = githubToken.trim()
    if (!url || !tok || repoAccess !== 'needs-token') return
    setTokenValidating(true)
    const result = await checkGitHubAccess(url, tok)
    if (result === 'accessible') {
      setRepoAccess('accessible')
    } else {
      setRepoAccess('token-invalid')
    }
    setTokenValidating(false)
  }, [repoUrl, githubToken, repoAccess])

  const handleIndex = (e: React.FormEvent) => {
    e.preventDefault()
    setIndexError(null)
    const project = projectMode === 'existing' ? selectedProject : newProjectName.trim()
    const tokenToSend = githubToken.trim() || undefined
    indexMut.mutate({ project, repo_url: repoUrl.trim(), github_token: tokenToSend, graph_only: graphOnly })
  }

  // Clear PAT state when the form is reset
  const resetForm = useCallback(() => {
    setShowForm(false)
    setIndexError(null)
    setRepoUrl('')
    setGithubToken('')
    setShowToken(false)
    setRepoAccess('idle')
  }, [])

  const handleDelete = (p: CodeProject) => {
    if (!window.confirm(`Delete "${p.name}"? This removes all indexed chunks.`)) return
    deleteMut.mutate(p.name)
  }

  return (
    <div className="space-y-5">
      {/* Toolbar: toggle + add button */}
      {!showForm && (
        <div className="flex items-center justify-between gap-3 flex-wrap">
          <div className="bg-white/[0.04] rounded-full p-0.5 flex items-center">
            <button
              onClick={() => showArchived && onToggleArchived()}
              className={`px-3 py-1 text-xs rounded-full transition-colors ${
                !showArchived
                  ? 'bg-white/[0.10] text-text-primary font-semibold shadow-sm'
                  : 'text-text-quaternary'
              }`}
            >
              Active
            </button>
            <button
              onClick={() => !showArchived && onToggleArchived()}
              className={`px-3 py-1 text-xs rounded-full transition-colors ${
                showArchived
                  ? 'bg-white/[0.10] text-text-primary font-semibold shadow-sm'
                  : 'text-text-quaternary'
              }`}
            >
              Archived
            </button>
          </div>
          <button
            onClick={() => setShowForm(true)}
            className="shrink-0 bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold hover:opacity-90 transition-opacity"
          >
            Add Repository
          </button>
        </div>
      )}

      {/* Add form */}
      {showForm && (
        <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
          <p className="text-[12px] tracking-[-0.12px] text-text-tertiary">Add Repository</p>
          <form onSubmit={handleIndex} className="space-y-4">
            <div>
              <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary mb-2">
                Project
              </label>
              <div className="flex items-center bg-white/[0.04] border border-white/[0.09] rounded-[11px] px-1 gap-0.5 w-fit mb-3">
                {(['existing', 'new'] as const).map(mode => (
                  <button
                    key={mode}
                    type="button"
                    onClick={() => setProjectMode(mode)}
                    className={`px-3 py-1 rounded-[8px] text-xs transition-colors ${
                      projectMode === mode
                        ? 'bg-accent-blue/15 text-accent-blue font-semibold'
                        : 'text-text-tertiary hover:text-text-secondary'
                    }`}
                  >
                    {mode === 'existing' ? 'Existing project' : 'New project'}
                  </button>
                ))}
              </div>
              {projectMode === 'existing' ? (
                <select
                  value={selectedProject}
                  onChange={e => setSelectedProject(e.target.value)}
                  disabled={indexMut.isPending}
                  required
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors"
                >
                  <option value="">Select a project…</option>
                  {memProjects?.map(p => (
                    <option key={p.id} value={p.name}>{p.name}</option>
                  ))}
                </select>
              ) : (
                <input
                  className={INPUT_CLS}
                  placeholder="my-repo"
                  value={newProjectName}
                  onChange={e => setNewProjectName(e.target.value)}
                  disabled={indexMut.isPending}
                  required
                />
              )}
            </div>

            <div>
              <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary mb-1.5">
                GitHub repository URL
              </label>
              <div className="relative">
                <input
                  className={INPUT_CLS}
                  placeholder="https://github.com/owner/repo"
                  value={repoUrl}
                  onChange={e => { setRepoUrl(e.target.value); setRepoAccess('idle') }}
                  onBlur={handleRepoUrlBlur}
                  disabled={indexMut.isPending}
                  type="url"
                  required
                />
                {repoAccess === 'checking' && (
                  <Loader2 className="absolute right-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 animate-spin text-text-quaternary" />
                )}
                {repoAccess === 'accessible' && (
                  <CheckCircle2 className="absolute right-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-status-success" />
                )}
                {(repoAccess === 'needs-token' || repoAccess === 'token-invalid') && (
                  <Lock className="absolute right-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-status-warning" />
                )}
              </div>

              {/* PAT field — revealed when the repo isn't publicly accessible */}
              {(repoAccess === 'needs-token' || repoAccess === 'token-invalid') && (
                <div className="mt-3 space-y-1.5">
                  <label className="block text-[12px] tracking-[-0.12px] text-text-tertiary">
                    GitHub Personal Access Token
                  </label>
                  <div className="relative">
                    <input
                      className={INPUT_CLS}
                      placeholder="ghp_…"
                      value={githubToken}
                      onChange={e => { setGithubToken(e.target.value); setRepoAccess('needs-token') }}
                      onBlur={handleTokenBlur}
                      type={showToken ? 'text' : 'password'}
                      autoComplete="off"
                      disabled={indexMut.isPending || tokenValidating}
                    />
                    <button
                      type="button"
                      onClick={() => setShowToken(v => !v)}
                      className="absolute right-3 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-secondary transition-colors"
                      aria-label={showToken ? 'Hide token' : 'Show token'}
                    >
                      {showToken ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                    </button>
                  </div>
                  {tokenValidating && (
                    <p className="text-[10px] text-text-quaternary flex items-center gap-1">
                      <Loader2 className="w-3 h-3 animate-spin" /> Validating token…
                    </p>
                  )}
                  {repoAccess === 'token-invalid' && !tokenValidating && (
                    <p className="text-[10px] text-status-error">
                      Token cannot access this repository. Verify it has the{' '}
                      <code className="font-mono">repo</code> scope and access to this repo.
                    </p>
                  )}
                  {repoAccess === 'needs-token' && !tokenValidating && (
                    <p className="text-[10px] text-text-quaternary">
                      This repository isn't publicly accessible. Provide a GitHub PAT with{' '}
                      <code className="font-mono">repo</code> (read) scope. The token is
                      stored encrypted and never returned in API responses.
                    </p>
                  )}
                </div>
              )}
            </div>

            <label className="flex items-start gap-2 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={graphOnly}
                onChange={e => setGraphOnly(e.target.checked)}
                disabled={indexMut.isPending}
                className="mt-0.5 accent-accent-blue"
              />
              <span className="text-[11px] text-text-tertiary leading-snug">
                Graph only — build the code structure graph fast, skip semantic-search embeddings.
                Much faster on large repos; you can run a full index later for search.
              </span>
            </label>

            {indexError && <p className="text-xs text-status-error/80">{indexError}</p>}

            <div className="flex gap-2 pt-1">
              <button
                type="button"
                onClick={resetForm}
                disabled={indexMut.isPending}
                className="rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={indexMut.isPending}
                className="flex items-center gap-1.5 bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold hover:opacity-90 transition-opacity disabled:opacity-60"
              >
                {indexMut.isPending && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                {indexMut.isPending ? 'Cloning & indexing…' : 'Index'}
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Projects list — established card+divider-row idiom (matches Projects.tsx) */}
      <div className={`rounded-[18px] overflow-hidden ${GLASS_PANEL}`}>
        <div className="px-5 py-4 border-b border-border-secondary flex items-center justify-between gap-3 flex-wrap">
          <span className="text-sm font-semibold text-text-primary">Indexed repositories</span>
          <span className="text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">
            {(projects ?? []).length} repo{(projects ?? []).length === 1 ? '' : 's'}
          </span>
        </div>
        <div className="divide-y divide-border-secondary">
      {isLoading ? (
        <>
          <SkeletonRow />
          <SkeletonRow />
          <SkeletonRow />
        </>
      ) : !projects || projects.length === 0 ? (
        <div className="p-10 text-center space-y-2">
          <p className="text-xs font-semibold text-text-primary">No repositories indexed yet.</p>
          <p className="text-xs text-text-quaternary">
            Add a repository to enable semantic code search and context retrieval.
          </p>
          {!showForm && (
            <button
              onClick={() => setShowForm(true)}
              className="mt-3 bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold hover:opacity-90 transition-opacity"
            >
              Add Repository
            </button>
          )}
        </div>
      ) : (
        <>
          {projects.map(p => {
            const isReindexing = reindexMut.isPending && reindexMut.variables?.id === p.id
            return (
              <div key={p.id}>
              <div
                className="group px-5 py-4 hover:bg-accent-blue/[0.04] transition-colors flex items-start justify-between gap-4"
              >
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <GitBranch className="w-3.5 h-3.5 text-text-quaternary shrink-0" aria-hidden="true" />
                    <span className="text-xs font-semibold text-text-primary">{p.name}</span>
                    <StatusChip project={p} />
                    {p.archived_at && (
                      <span className="bg-status-warning/10 text-status-warning text-[10px] rounded-[5px] px-1.5 py-0.5">
                        archived
                      </span>
                    )}
                    {p.reindex_interval_hours != null && (
                      <span className="rounded-[5px] text-[10px] bg-white/[0.04] text-text-quaternary border border-border-secondary/50 px-1.5 py-0.5">
                        auto {p.reindex_interval_hours}h
                      </span>
                    )}
                    {/* Sync status indicator */}
                    <StatusBadge project={p} />
                    {/* Indexed files count chip */}
                    {(p.indexed_files_count ?? 0) > 0 && (
                      <span className="text-[10px] text-text-secondary bg-white/[0.06] rounded-[5px] px-1.5 py-0.5 border border-border-secondary/50">
                        {p.indexed_files_count} files
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-text-secondary font-mono truncate">{p.repo_url ?? p.root_path}</p>
                  <p className="text-xs text-text-tertiary">
                    {p.file_count.toLocaleString()} files
                    {' · '}
                    {p.chunk_count.toLocaleString()} chunks
                    {p.last_indexed
                      ? ` · Last indexed: ${formatDate(p.last_indexed)}`
                      : ' · Never indexed'}
                  </p>
                  {/* Schedule selector */}
                  <div className="flex items-center gap-2 pt-1">
                    <span className="text-[10px] text-text-quaternary">Auto re-index:</span>
                    <select
                      value={p.reindex_interval_hours ?? ''}
                      onChange={e => {
                        const val = e.target.value
                        scheduleMut.mutate({ id: p.id, interval_hours: val === '' ? null : Number(val) })
                      }}
                      disabled={scheduleMut.isPending && scheduleMut.variables?.id === p.id}
                      className="rounded-[11px] bg-transparent border border-border-primary text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 px-2 py-0.5 cursor-pointer disabled:opacity-50 transition-opacity"
                    >
                      <option value="">No schedule</option>
                      <option value="6">Every 6h</option>
                      <option value="12">Every 12h</option>
                      <option value="24">Every 24h</option>
                      <option value="168">Every week</option>
                    </select>
                    {scheduleMut.isPending && scheduleMut.variables?.id === p.id && (
                      <Loader2 className="w-3 h-3 animate-spin text-text-quaternary shrink-0" />
                    )}
                    {scheduleMut.isError && scheduleMut.variables?.id === p.id && (
                      <p className="text-xs text-status-error mt-1">
                        Failed. {scheduleMut.error instanceof Error ? scheduleMut.error.message : 'Please try again.'}
                      </p>
                    )}
                  </div>
                  {/* Exclude patterns editor */}
                  <ExcludePatternsEditor
                    project={p}
                    isSaving={updateProjectMut.isPending && updateProjectMut.variables?.id === p.id}
                    onSave={patterns => updateProjectMut.mutate({ id: p.id, exclude_patterns: patterns })}
                  />
                </div>
                <div className="flex flex-col items-end gap-1 shrink-0 opacity-0 group-hover:opacity-100 sm:opacity-100 transition-opacity">
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => setExpandedFiles(expandedFiles === p.id ? null : p.id)}
                      className="border border-border-primary rounded-full px-2.5 py-1 text-[10px] text-text-quaternary hover:text-text-primary transition-colors flex items-center gap-1"
                    >
                      <FileText className="w-3 h-3" />
                      Files
                    </button>
                    {!p.archived_at && (
                      <button
                        onClick={() => reindexMut.mutate(p)}
                        disabled={isReindexing || p.index_status === 'indexing'}
                        title={(isReindexing || p.index_status === 'indexing') ? 'Syncing…' : 'Sync now'}
                        className="border border-border-primary rounded-full w-7 h-7 flex items-center justify-center text-text-quaternary hover:text-text-primary hover:border-border-primary transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        {(isReindexing || p.index_status === 'indexing')
                          ? <Loader2 className="w-3 h-3 animate-spin" />
                          : <RotateCcw className="w-3 h-3" />}
                      </button>
                    )}
                    {p.archived_at ? (
                      <button
                        onClick={() => restoreMut.mutate(p)}
                        disabled={restoreMut.isPending && restoreMut.variables?.id === p.id}
                        aria-label="Restore project"
                        className="opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-status-success transition-opacity disabled:opacity-50"
                      >
                        {restoreMut.isPending && restoreMut.variables?.id === p.id
                          ? <Loader2 className="w-3.5 h-3.5 animate-spin" />
                          : <RotateCcw className="w-3.5 h-3.5" />}
                      </button>
                    ) : (
                      <button
                        onClick={() => archiveMut.mutate(p)}
                        disabled={archiveMut.isPending && archiveMut.variables?.id === p.id}
                        aria-label="Archive project"
                        className="opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-status-warning transition-opacity disabled:opacity-50"
                      >
                        {archiveMut.isPending && archiveMut.variables?.id === p.id
                          ? <Loader2 className="w-3.5 h-3.5 animate-spin" />
                          : <ArchiveX className="w-3.5 h-3.5" />}
                      </button>
                    )}
                    {!p.archived_at && (
                      <button
                        onClick={() => handleDelete(p)}
                        disabled={deleteMut.isPending}
                        className="text-xs border border-status-error/20 rounded-full px-3 py-1 text-status-error/60 hover:text-status-error transition-colors disabled:opacity-50"
                      >
                        Delete
                      </button>
                    )}
                  </div>
                  {reindexMut.isError && reindexMut.variables?.id === p.id && (
                    <p className="text-xs text-status-error">
                      {reindexMut.error instanceof Error ? reindexMut.error.message : 'Re-index failed. Please try again.'}
                    </p>
                  )}
                  {deleteMut.isError && deleteMut.variables === p.name && (
                    <p className="text-xs text-status-error">
                      {deleteMut.error instanceof Error ? deleteMut.error.message : 'Delete failed. Please try again.'}
                    </p>
                  )}
                  {archiveMut.isError && archiveMut.variables?.id === p.id && (
                    <p className="text-xs text-status-error">
                      {archiveMut.error instanceof Error ? archiveMut.error.message : 'Archive failed.'}
                    </p>
                  )}
                  {restoreMut.isError && restoreMut.variables?.id === p.id && (
                    <p className="text-xs text-status-error">
                      {restoreMut.error instanceof Error ? restoreMut.error.message : 'Restore failed.'}
                    </p>
                  )}
                </div>
              </div>
              {expandedFiles === p.id && (
                <div className="mx-5 mb-3 rounded-[11px] border border-border-primary bg-white/[0.02] p-3">
                  {filesLoading ? (
                    <p className="text-[10px] text-text-quaternary text-center py-2">Loading…</p>
                  ) : (
                    <ul className="space-y-0.5 max-h-48 overflow-y-auto">
                      {(files ?? []).map((f: string) => (
                        <li key={f} className="text-[10px] text-text-secondary font-mono py-0.5 px-1 rounded hover:bg-white/[0.04]">
                          {f}
                        </li>
                      ))}
                    </ul>
                  )}
                  {(files ?? []).length === 0 && !filesLoading && (
                    <p className="text-[10px] text-text-quaternary text-center py-2">No indexed files yet</p>
                  )}
                </div>
              )}
              </div>
            )
          })}
        </>
      )}
        </div>
      </div>
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────────────────

type Tab = 'repositories' | 'search'

export default function Code() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const isAdmin = isPrivileged(session?.user.role)
  const [activeTab, setActiveTab] = useState<Tab>('repositories')
  const [showArchived, setShowArchived] = useState(false)

  const { data: projects, isLoading, isError: projectsError } = useQuery({
    queryKey: ['code-projects', showArchived],
    queryFn: () => client.listCodeProjects({ include_archived: showArchived }),
    enabled: isAdmin,
    refetchInterval: (query) =>
      (query.state.data as CodeProject[] | undefined)?.some(p => p.index_status === 'indexing') ? 5000 : false,
  })

  // Reindexes every active (non-archived) repository by looping the existing
  // per-project reindex call — there is no bulk endpoint, so this reuses
  // client.reindexCodeProject rather than fabricating a new API.
  const reindexAllMut = useMutation({
    mutationFn: async () => {
      const active = (projects ?? []).filter(p => !p.archived_at)
      await Promise.allSettled(active.map(p => client.reindexCodeProject(p.id)))
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['code-projects'] }),
  })

  const activeProjects = useMemo(() => (projects ?? []).filter(p => !p.archived_at), [projects])
  const totalFiles = useMemo(() => activeProjects.reduce((sum, p) => sum + (p.file_count ?? 0), 0), [activeProjects])
  const indexingCount = useMemo(() => activeProjects.filter(p => p.index_status === 'indexing').length, [activeProjects])
  const mostRecentIndexed = useMemo(
    () =>
      activeProjects.reduce<CodeProject | null>((latest, p) => {
        if (!p.last_indexed) return latest
        if (!latest || !latest.last_indexed) return p
        return new Date(p.last_indexed) > new Date(latest.last_indexed) ? p : latest
      }, null),
    [activeProjects],
  )

  const statTiles = [
    {
      label: 'Repos indexed',
      value: String(activeProjects.length),
      sub: indexingCount > 0 ? `${indexingCount} indexing now` : activeProjects.length > 0 ? 'all synced' : undefined,
      icon: GitBranch,
    },
    {
      label: 'Files',
      value: totalFiles.toLocaleString(),
      sub: 'across repos',
      icon: FileText,
    },
    {
      label: 'Last index',
      value: mostRecentIndexed?.last_indexed ? relativeTime(mostRecentIndexed.last_indexed) : '—',
      sub: mostRecentIndexed?.name,
      icon: Clock,
    },
    // "Symbols" and "Searches (7d)" tiles from the mockup would need a
    // symbol-count aggregate and a search-history aggregate this page
    // doesn't fetch (CodeProject carries no symbol count; CodeSearchResult
    // results aren't persisted across searches) — omitted rather than
    // fabricated.
  ]

  const TABS: { id: Tab; label: string; icon?: React.ReactNode }[] = [
    { id: 'repositories', label: 'Repositories' },
    { id: 'search', label: 'Search', icon: <Search className="w-3 h-3" /> },
  ]

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 rounded-[13px] bg-accent-blue/12 flex items-center justify-center shrink-0">
            <Code2 className="w-5 h-5 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-base font-semibold text-text-primary">
              Code Repositories
            </h1>
            <p className="text-xs text-text-quaternary mt-0.5">
              Connect and index codebases for AI-assisted search and context retrieval.
            </p>
          </div>
        </div>
        {isAdmin && (
          <button
            onClick={() => reindexAllMut.mutate()}
            disabled={reindexAllMut.isPending || activeProjects.length === 0}
            className="flex items-center gap-1.5 bg-accent-blue text-white rounded-full px-3.5 py-1.5 text-xs font-semibold hover:opacity-90 transition-opacity disabled:opacity-50 shrink-0"
          >
            {reindexAllMut.isPending
              ? <Loader2 className="w-3.5 h-3.5 animate-spin" />
              : <RefreshCw className="w-3.5 h-3.5" />}
            {reindexAllMut.isPending ? 'Reindexing…' : 'Reindex all'}
          </button>
        )}
      </div>

      {/* Stats */}
      <KpiMarquee role="list" aria-label="Code repository statistics">
        {statTiles.map((tile, i) => (
          <div key={tile.label} className="w-[232px] flex-none">
            <StatTile label={tile.label} value={tile.value} sub={tile.sub} icon={tile.icon} accent={accentFor(i)} />
          </div>
        ))}
      </KpiMarquee>

      {/* Tab bar */}
      <div className="flex items-center bg-white/[0.04] border border-white/[0.09] rounded-[11px] px-1 gap-0.5 w-fit">
        {TABS.map(tab => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`flex items-center gap-1.5 px-2 py-0.5 text-[10px] rounded-[5px] transition-colors ${
              activeTab === tab.id
                ? 'bg-accent-blue/10 text-accent-blue'
                : 'text-text-quaternary hover:text-text-secondary'
            }`}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {projectsError && (
        <p className="text-xs text-status-error text-center py-8">Failed to load repositories. Please refresh.</p>
      )}
      {activeTab === 'repositories' && !projectsError && (
        <RepositoriesTab
          projects={projects}
          isLoading={isLoading}
          showArchived={showArchived}
          onToggleArchived={() => setShowArchived(v => !v)}
        />
      )}
      {activeTab === 'search' && (
        <CodeSearchTab projects={projects} />
      )}
    </div>
  )
}
