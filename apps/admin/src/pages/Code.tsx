import { useMemo, useState, useCallback, useRef, useEffect, lazy, Suspense, type ReactNode } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Loader2, Search, ChevronDown, ChevronRight, Bookmark, BookmarkCheck, Trash2, X, RefreshCw, CheckCircle2, AlertCircle, Clock, RotateCcw, ArchiveX, Download, Copy, Check, Plus, FileText } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { CodeProject, CodeSearchResult } from '../types'

// Lazy-load the graph tab to avoid bundling react-force-graph-2d (~400 KB)
// into the initial admin chunk.
const GraphTab = lazy(() => import('./code/GraphTab'))

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
    <span className="text-[10px] font-semibold border rounded-[5px] px-1.5 py-0.5 text-text-quaternary bg-[#272729] border-border-secondary">
      not indexed
    </span>
  )
}

function SkeletonRow() {
  return (
    <div className="border border-border-primary rounded-[18px] p-5 animate-pulse">
      <div className="h-4 bg-[#272729] rounded-[5px] w-1/3 mb-2" />
      <div className="h-3 bg-[#272729] rounded-[5px] w-1/2 mb-2" />
      <div className="h-3 bg-[#272729] rounded-[5px] w-2/3" />
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
      : 'text-text-quaternary bg-[#272729] border-border-secondary'
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
    <div className="bg-[#272729] border border-border-primary rounded-[18px] overflow-hidden">
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
        <div className="border-t border-border-secondary bg-[#1d1d1f] rounded-b-[18px]">
          <pre className="px-4 py-3 text-[10px] font-mono text-text-secondary leading-relaxed overflow-x-auto whitespace-pre-wrap break-words">
            {highlightCode(result.content, searchQuery)}
          </pre>
        </div>
      )}
    </div>
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

  const { data: results, isLoading, isError, error } = useQuery({
    queryKey: ['code-search', submittedProject, submittedQuery, submittedExtension],
    queryFn: () => client.searchCode(submittedProject, submittedQuery, 10, submittedExtension || undefined),
    enabled: submittedQuery.trim().length > 0 && submittedProject.trim().length > 0,
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
    },
    [query, selectedProject, extensionFilter],
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
        <p className="text-[12px] tracking-[-0.12px] text-text-tertiary">Semantic Code Search</p>

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

          {/* Extension filter */}
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
                  <div className="absolute left-0 top-full mt-1.5 z-20 bg-[#272729] border border-border-primary rounded-[11px] py-1 min-w-[220px] shadow-xl">
                    {savedSearches.map(s => (
                      <div
                        key={s.id}
                        className="flex items-center justify-between px-3 py-2 hover:bg-white/[0.04] group cursor-pointer"
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
                  <div className="absolute right-0 top-full mt-1.5 z-20 bg-[#272729] border border-border-primary rounded-[11px] p-3 shadow-xl w-56 space-y-2">
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
              disabled={isLoading || !query.trim() || !selectedProject}
              className="flex items-center gap-1.5 bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold hover:opacity-90 transition-opacity disabled:opacity-50"
            >
              {isLoading && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
              {isLoading ? 'Searching…' : 'Search'}
            </button>
          </div>
        </div>
      </form>

      {/* Results */}
      {hasSearched && (
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
    mutationFn: (data: { project: string; repo_url?: string; root_path?: string; graph_only?: boolean }) => client.indexProject(data),
    onSuccess: () => {
      setRepoUrl('')
      setSelectedProject('')
      setNewProjectName('')
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

  const handleIndex = (e: React.FormEvent) => {
    e.preventDefault()
    setIndexError(null)
    const project = projectMode === 'existing' ? selectedProject : newProjectName.trim()
    indexMut.mutate({ project, repo_url: repoUrl.trim(), graph_only: graphOnly })
  }

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
                  ? 'bg-[#272729] text-text-primary font-semibold shadow-sm'
                  : 'text-text-quaternary'
              }`}
            >
              Active
            </button>
            <button
              onClick={() => !showArchived && onToggleArchived()}
              className={`px-3 py-1 text-xs rounded-full transition-colors ${
                showArchived
                  ? 'bg-[#272729] text-text-primary font-semibold shadow-sm'
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
              <div className="flex items-center bg-[#1d1d1f] border border-border-primary rounded-[11px] px-1 gap-0.5 w-fit mb-3">
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
              <input
                className={INPUT_CLS}
                placeholder="https://github.com/owner/repo"
                value={repoUrl}
                onChange={e => setRepoUrl(e.target.value)}
                disabled={indexMut.isPending}
                type="url"
                required
              />
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
                onClick={() => { setShowForm(false); setIndexError(null) }}
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

      {/* Projects list */}
      {isLoading ? (
        <div className="space-y-3">
          <SkeletonRow />
          <SkeletonRow />
          <SkeletonRow />
        </div>
      ) : !projects || projects.length === 0 ? (
        <div className="border border-border-primary rounded-[18px] p-10 text-center space-y-2">
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
        <div className="space-y-3">
          {projects.map(p => {
            const isReindexing = reindexMut.isPending && reindexMut.variables?.id === p.id
            return (
              <div key={p.id}>
              <div
                className="group bg-[#272729] border border-border-primary rounded-[18px] p-5 flex items-start justify-between gap-4"
              >
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex items-center gap-2 flex-wrap">
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
                <div className="mt-2 rounded-[11px] border border-border-primary bg-white/[0.02] p-3">
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
        </div>
      )}
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────────────────

type Tab = 'repositories' | 'search' | 'graph'

export default function Code() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [activeTab, setActiveTab] = useState<Tab>('repositories')
  const [showArchived, setShowArchived] = useState(false)

  const { data: projects, isLoading, isError: projectsError } = useQuery({
    queryKey: ['code-projects', showArchived],
    queryFn: () => client.listCodeProjects({ include_archived: showArchived }),
    refetchInterval: (query) =>
      (query.state.data as CodeProject[] | undefined)?.some(p => p.index_status === 'indexing') ? 5000 : false,
  })

  const TABS: { id: Tab; label: string; icon?: React.ReactNode }[] = [
    { id: 'repositories', label: 'Repositories' },
    { id: 'search', label: 'Search', icon: <Search className="w-3 h-3" /> },
    { id: 'graph', label: 'Graph' },
  ]

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-base font-semibold text-text-primary">
          Code Repositories
        </h1>
        <p className="text-xs text-text-quaternary mt-0.5">
          Connect and index codebases for AI-assisted search and context retrieval.
        </p>
      </div>

      {/* Tab bar */}
      <div className="flex items-center bg-[#1d1d1f] border border-border-primary rounded-[11px] px-1 gap-0.5 w-fit">
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
      {activeTab === 'graph' && (
        <Suspense
          fallback={
            <div className="flex items-center justify-center py-20">
              <Loader2 className="w-5 h-5 animate-spin text-text-quaternary" />
            </div>
          }
        >
          <GraphTab projects={projects} />
        </Suspense>
      )}
    </div>
  )
}
