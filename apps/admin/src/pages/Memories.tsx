import React, { useMemo, useState, useCallback, useEffect, useRef } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import ReactMarkdown from 'react-markdown'
import { useAuth } from '../auth/AuthContext'
import { createClient, NexusMindClient } from '../api/client'
import { todayStamp } from '../lib/download'
import type { Memory, ImportMemory, ImportMemoriesResponse, Collection } from '../types'
import { TagAutocomplete } from '../components/TagAutocomplete'
import { Search, X, Brain, Tag, SlidersHorizontal, Trash2, Clock, Hash, ChevronDown, ChevronUp, CheckCircle2, Copy, Download, Upload, Loader2, Pencil, Check, Archive, RotateCcw, ArchiveX, Pin, Bookmark, BookmarkCheck, GitMerge, History, Folder, CalendarClock, Star, Plus } from 'lucide-react'
import { cn } from '@/lib/utils'

const FAV_KEY = 'nexusmind-memory-favorites'
function loadFavorites(): Set<string> {
  try { return new Set(JSON.parse(localStorage.getItem(FAV_KEY) ?? '[]')) }
  catch { return new Set() }
}
function saveFavorites(ids: Set<string>) {
  localStorage.setItem(FAV_KEY, JSON.stringify([...ids]))
}

function highlightMatch(text: string, query: string): React.ReactNode {
  if (!query || query.length < 2) return text
  const parts = text.split(new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi'))
  return parts.map((part, i) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark key={i} className="bg-accent-blue/20 text-accent-blue rounded-[2px] px-0.5">{part}</mark>
    ) : part
  )
}

function useDebounce<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delay)
    return () => clearTimeout(t)
  }, [value, delay])
  return debounced
}

const TYPE_META: Record<string, { label: string; cls: string }> = {
  decision:     { label: 'decision',     cls: 'text-accent-blue bg-accent-blue/10 border-accent-blue/25' },
  bugfix:       { label: 'bugfix',       cls: 'text-status-error bg-status-error/10 border-status-error/25' },
  discovery:    { label: 'discovery',    cls: 'text-text-secondary bg-white/[0.06] border-border-secondary/60' },
  convention:   { label: 'convention',   cls: 'text-status-success bg-status-success/10 border-status-success/25' },
  architecture: { label: 'architecture', cls: 'text-accent-blue bg-accent-blue/8 border-accent-blue/20' },
  config:       { label: 'config',       cls: 'text-status-warning bg-status-warning/10 border-status-warning/25' },
  preference:   { label: 'preference',   cls: 'text-text-tertiary bg-white/[0.04] border-border-secondary/50' },
  pattern:      { label: 'pattern',      cls: 'text-text-secondary bg-white/[0.05] border-border-secondary/50' },
}

function TypeBadge({ type }: { type?: string }) {
  if (!type) return null
  const meta = TYPE_META[type]
  const cls = meta?.cls ?? 'text-text-tertiary bg-[#272729] border-border-primary'
  return (
    <span className={`text-[10px] font-semibold border rounded-[5px] px-2 py-0.5 ${cls}`}>
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
          <pre className="bg-[#1d1d1f] border border-border-primary rounded-[11px] px-4 py-3 overflow-x-auto mb-3 last:mb-0">
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

// ── Create Memory Modal ───────────────────────────────────────────────────────

function CreateMemoryModal({
  open,
  onClose,
  onCreated,
}: {
  open: boolean
  onClose: () => void
  onCreated: () => void
}) {
  const { session } = useAuth()
  const client = React.useMemo(() => createClient(), [session])
  const qc = useQueryClient()

  const [content, setContent] = useState('')
  const [tagInput, setTagInput] = useState('')
  const [tags, setTags] = useState<string[]>([])
  const [projectId, setProjectId] = useState<string>('')
  const [saving, setSaving] = useState(false)
  const [flash, setFlash] = useState(false)

  const { data: projects = [] } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    enabled: open,
  })

  useEffect(() => {
    if (!open) {
      setContent('')
      setTagInput('')
      setTags([])
      setProjectId('')
      setSaving(false)
      setFlash(false)
    }
  }, [open])

  useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [open, onClose])

  const handleTagSelect = (tag: string) => {
    if (!tags.includes(tag)) {
      setTags(prev => [...prev, tag])
    }
    setTagInput('')
  }

  const handleTagKeyDown = (e: React.KeyboardEvent) => {
    if ((e.key === 'Enter' || e.key === ',') && tagInput.trim()) {
      e.preventDefault()
      const newTag = tagInput.trim().replace(/,$/, '')
      if (newTag && !tags.includes(newTag)) {
        setTags(prev => [...prev, newTag])
      }
      setTagInput('')
    } else if (e.key === 'Backspace' && !tagInput && tags.length > 0) {
      setTags(prev => prev.slice(0, -1))
    }
  }

  const removeTag = (tag: string) => {
    setTags(prev => prev.filter(t => t !== tag))
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!content.trim()) return
    setSaving(true)
    try {
      await client.storeMemory({
        content: content.trim(),
        tags: tags.length > 0 ? tags : undefined,
        project_id: projectId || undefined,
      })
      qc.invalidateQueries({ queryKey: ['memories'] })
      setFlash(true)
      setTimeout(() => {
        onCreated()
        onClose()
      }, 800)
    } finally {
      setSaving(false)
    }
  }

  if (!open) return null

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={onClose}
    >
      <div
        className="bg-[#1d1d1f] rounded-[18px] border border-border-primary p-6 max-w-lg w-full shadow-2xl mx-4 max-h-[90vh] overflow-y-auto"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-sm font-semibold text-text-primary">New memory</h2>
          <button onClick={onClose} className="text-text-quaternary hover:text-text-secondary transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>

        {flash ? (
          <div className="flex items-center gap-2 py-6 justify-center text-status-success">
            <CheckCircle2 className="w-5 h-5" />
            <span className="text-sm font-semibold">Memory created</span>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            {/* Content */}
            <div>
              <label className="block text-xs text-text-tertiary mb-1.5">Content</label>
              <textarea
                placeholder="Memory content..."
                value={content}
                onChange={e => setContent(e.target.value)}
                className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary resize-none min-h-[160px] p-3 focus:outline-none focus:border-accent-blue/60 w-full placeholder:text-text-quaternary"
                maxLength={10000}
                required
              />
              <p className="text-[10px] text-text-quaternary text-right mt-0.5">{content.length} / 10000</p>
            </div>

            {/* Tags */}
            <div>
              <label className="block text-xs text-text-tertiary mb-1.5">Tags</label>
              <div className="rounded-[8px] border border-border-primary bg-white/[0.04] px-2 py-1.5 flex flex-wrap gap-1.5 focus-within:border-accent-blue/60 transition-colors">
                {tags.map(tag => (
                  <span key={tag} className="bg-white/[0.06] rounded-full px-2 py-0.5 text-[10px] text-text-secondary flex items-center gap-1">
                    {tag}
                    <button type="button" onClick={() => removeTag(tag)} className="hover:text-text-primary transition-colors">
                      <X className="w-2.5 h-2.5" />
                    </button>
                  </span>
                ))}
                <TagAutocomplete
                  value={tagInput}
                  onChange={setTagInput}
                  onSelect={handleTagSelect}
                  onKeyDown={handleTagKeyDown}
                  existingTags={tags}
                  placeholder={tags.length === 0 ? 'Add tags…' : ''}
                  className="flex-1 min-w-[100px] bg-transparent text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none"
                />
              </div>
            </div>

            {/* Project */}
            <div>
              <label className="block text-xs text-text-tertiary mb-1.5">Project <span className="text-text-quaternary">(optional)</span></label>
              <select
                value={projectId}
                onChange={e => setProjectId(e.target.value)}
                className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs focus:outline-none focus:border-accent-blue/60 text-text-primary px-2 py-1.5"
              >
                <option value="">No project</option>
                {projects.filter(p => !p.archived_at).map(p => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
            </div>

            <div className="flex gap-2 justify-end pt-1">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 rounded-[8px] text-xs text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={saving || !content.trim()}
                className="px-4 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:bg-accent-blue/90 disabled:opacity-50 transition-colors"
              >
                {saving ? 'Creating…' : 'Create memory'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}

// ── Modal ─────────────────────────────────────────────────────────────────────

function MemoryDetailModal({ memory, onClose, onDelete, deleting, deleteError }: {
  memory: Memory
  onClose: () => void
  onDelete: () => void
  deleting: boolean
  deleteError?: string
}) {
  const { session } = useAuth()
  const canDelete =
    session?.user.role === 'admin' ||
    (session?.user.role === 'member' && memory.user_id === session.user.id)

  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    document.body.style.overflow = 'hidden'
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handleEscape)
    return () => {
      document.body.style.overflow = ''
      document.removeEventListener('keydown', handleEscape)
    }
  }, [onClose])

  return (
    <div
      className="fixed inset-y-0 left-0 lg:left-52 right-0 z-50 flex items-center justify-center bg-black/60 p-6"
      onClick={onClose}
    >
      <div
        ref={panelRef}
        className="bg-[#272729] border border-white/[0.08] rounded-[18px] w-full max-w-3xl flex flex-col max-h-full"
        onClick={e => e.stopPropagation()}
      >

        {/* Header */}
        <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-4 shrink-0 border-b border-border-secondary">
          <div className="space-y-2 min-w-0">
            {memory.title && (
              <p className="text-sm font-semibold text-text-primary leading-snug">{memory.title}</p>
            )}
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-[11px] font-semibold border border-border-primary rounded-[5px] px-2 py-0.5 text-text-tertiary bg-[#272729]">
                {memory.tool}
              </span>
              {memory.project && (
                <span className="text-[11px] text-text-tertiary font-semibold">{memory.project}</span>
              )}
              <TypeBadge type={memory.type} />
              {memory.revision_count != null && memory.revision_count > 1 && (
                <span className="text-[11px] text-text-quaternary bg-[#272729] border border-border-secondary rounded-[5px] px-1.5 py-0.5">
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
            aria-label="Close memory detail"
            className="p-1.5 rounded-[11px] text-text-quaternary hover:text-text-primary hover:bg-[#272729] transition-colors shrink-0"
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
                <span key={tag} className="text-[11px] bg-[#272729] text-text-tertiary border border-border-secondary rounded-[5px] px-2 py-0.5">
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        {canDelete && (
          <div className="flex items-center justify-between px-6 py-4 shrink-0 border-t border-border-secondary">
            {deleteError
              ? <p className="text-xs text-status-error/80">{deleteError}</p>
              : <div />
            }
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

// ── Facet select ──────────────────────────────────────────────────────────────

function FacetSelect({
  label,
  value,
  onChange,
  options,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  options: { value: string; count: number }[]
}) {
  return (
    <div className="relative">
      <select
        value={value}
        onChange={e => onChange(e.target.value)}
        className="appearance-none bg-transparent border border-border-secondary/40 rounded-[8px] pl-3 pr-7 py-1.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer"
      >
        <option value="">{label}</option>
        {options.map(o => (
          <option key={o.value} value={o.value}>
            {o.value} ({o.count})
          </option>
        ))}
      </select>
      <svg
        className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 w-3 h-3 text-text-quaternary"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={2}
      >
        <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
      </svg>
    </div>
  )
}

// ── Bulk action bar ───────────────────────────────────────────────────────────

function BulkActionBar({
  count,
  onDelete,
  onArchive,
  onClear,
  deleting,
  archiving,
  tagAction,
  setTagAction,
  tagInput,
  setTagInput,
  onBulkTag,
  bulkTagPending,
}: {
  count: number
  onDelete: () => void
  onArchive: () => void
  onClear: () => void
  deleting: boolean
  archiving: boolean
  tagAction: 'add' | 'remove' | null
  setTagAction: (a: 'add' | 'remove' | null) => void
  tagInput: string
  setTagInput: (v: string) => void
  onBulkTag: () => void
  bulkTagPending: boolean
}) {
  if (count === 0) return null
  return (
    <div
      className="fixed bottom-6 left-1/2 -translate-x-1/2 z-30 flex items-center gap-3 bg-[#272729] border border-border-primary rounded-full px-4 py-2.5 shadow-xl"
      role="toolbar"
      aria-label="Bulk actions"
    >
      <span className="bg-accent-blue/10 text-accent-blue rounded-full px-2 py-0.5 text-xs font-semibold">
        {count}
      </span>
      <div className="w-px h-4 bg-border-primary" />
      {tagAction ? (
        <div className="flex items-center gap-2">
          <TagAutocomplete
            value={tagInput}
            onChange={setTagInput}
            onSelect={tag => { setTagInput(tag); onBulkTag() }}
            onKeyDown={e => {
              if (e.key === 'Enter') onBulkTag()
              if (e.key === 'Escape') { setTagAction(null); setTagInput('') }
            }}
            placeholder={tagAction === 'add' ? 'Tag to add…' : 'Tag to remove…'}
            className="bg-transparent border border-border-primary rounded-[8px] px-2.5 py-1 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 w-36"
          />
          <button
            onClick={onBulkTag}
            disabled={!tagInput.trim() || bulkTagPending}
            className="px-3 py-1 rounded-[8px] bg-accent-blue text-white text-xs font-semibold disabled:opacity-50"
          >
            {bulkTagPending ? '…' : 'Apply'}
          </button>
          <button
            onClick={() => { setTagAction(null); setTagInput('') }}
            className="text-text-quaternary hover:text-text-secondary"
            aria-label="Cancel tag action"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      ) : (
        <>
          <button
            onClick={onArchive}
            disabled={archiving}
            className="text-xs text-text-secondary hover:text-text-primary transition-colors disabled:opacity-40"
            aria-label={`Archive ${count} selected memories`}
          >
            {archiving ? 'Archiving…' : 'Archive'}
          </button>
          <button
            onClick={() => setTagAction('add')}
            className="text-xs text-text-secondary hover:text-text-primary transition-colors"
          >
            Add tag
          </button>
          <button
            onClick={() => setTagAction('remove')}
            className="text-xs text-text-secondary hover:text-text-primary transition-colors"
          >
            Remove tag
          </button>
          <button
            onClick={onDelete}
            disabled={deleting}
            className="text-xs text-status-error hover:text-status-error/80 transition-colors disabled:opacity-40"
            aria-label={`Delete ${count} selected memories`}
          >
            {deleting ? 'Deleting…' : 'Delete'}
          </button>
        </>
      )}
      <div className="w-px h-4 bg-border-primary" />
      <button
        onClick={onClear}
        disabled={deleting || archiving}
        className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-quaternary hover:text-text-tertiary transition-colors disabled:opacity-40"
        aria-label="Clear selection"
      >
        Clear
      </button>
    </div>
  )
}

// ── Memory Detail Slide-over ──────────────────────────────────────────────────

function MemorySlideOver({
  memoryId,
  onClose,
  client,
}: {
  memoryId: string | null
  onClose: () => void
  client: NexusMindClient
}) {
  const qc = useQueryClient()
  const [deleteConfirm, setDeleteConfirm] = useState(false)

  const { data: memory, isLoading } = useQuery({
    queryKey: ['memory', memoryId],
    queryFn: () => client.getMemory(memoryId!),
    enabled: !!memoryId,
  })

  const firstWords = memory?.content?.split(/\s+/).slice(0, 8).join(' ') ?? ''

  const { data: related, isLoading: relatedLoading } = useQuery({
    queryKey: ['memory-related', memoryId, firstWords],
    queryFn: () => client.searchMemory({ query: firstWords, limit: 3 }),
    enabled: !!memoryId && !!firstWords,
    staleTime: 60_000,
  })

  const pinMut = useMutation({
    mutationFn: (id: string) => client.pinMemory(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['memories'] }); qc.invalidateQueries({ queryKey: ['memory', memoryId] }) },
  })

  const unpinMut = useMutation({
    mutationFn: (id: string) => client.unpinMemory(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['memories'] }); qc.invalidateQueries({ queryKey: ['memory', memoryId] }) },
  })

  const archiveMut = useMutation({
    mutationFn: (id: string) => client.archiveMemory(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['memories'] }); qc.invalidateQueries({ queryKey: ['memory', memoryId] }) },
  })

  const restoreMut = useMutation({
    mutationFn: (id: string) => client.restoreMemory(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['memories'] }); qc.invalidateQueries({ queryKey: ['memory', memoryId] }) },
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteMemory(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      onClose()
    },
  })

  // Escape key to close
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [onClose])

  const isOpen = !!memoryId

  function fmt(ts?: string) {
    if (!ts) return '—'
    return new Date(ts).toLocaleString()
  }

  return (
    <>
      {/* Backdrop */}
      {isOpen && (
        <div
          className="fixed inset-0 bg-black/30 z-30"
          onClick={onClose}
        />
      )}

      {/* Slide-over panel */}
      <div
        className={`fixed right-0 top-0 h-full w-[420px] bg-[#1d1d1f] border-l border-border-primary shadow-2xl z-50 flex flex-col transform transition-transform duration-200 ${isOpen ? 'translate-x-0' : 'translate-x-full'}`}
      >
        {/* Header */}
        <div className="px-5 py-4 border-b border-border-primary flex items-start justify-between gap-3 shrink-0">
          <div className="flex items-center gap-2 flex-wrap min-w-0">
            {memory && <TypeBadge type={memory.type} />}
            <p className="text-sm font-semibold text-text-primary leading-snug truncate">
              {memory?.content?.slice(0, 60) ?? (isLoading ? 'Loading…' : '—')}
            </p>
          </div>
          <button
            onClick={onClose}
            aria-label="Close detail panel"
            className="text-text-quaternary hover:text-text-primary transition-colors shrink-0"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="overflow-y-auto flex-1 p-6">
        {isLoading ? (
          <div className="space-y-3">
            {[1, 2, 3].map(i => (
              <div key={i} className="h-4 rounded-[5px] bg-white/[0.06] animate-pulse" />
            ))}
          </div>
        ) : memory ? (
          <div className="space-y-5">
            {/* Full content */}
            <div>
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-1.5">Content</p>
              <p className="whitespace-pre-wrap text-xs text-text-secondary leading-relaxed">
                {memory.content}
              </p>
            </div>

            {/* Metadata grid */}
            <div className="grid grid-cols-2 gap-3 pt-4 border-t border-border-primary">
              {([
                ['Created',    fmt(memory.created_at)],
                ['Updated',    fmt((memory as any).updated_at)],
                ['Type',       memory.type ?? '—'],
                ['Project',    memory.project || '—'],
                ['Session',    (memory as any).session_id ? ((memory as any).session_id as string).slice(0, 12) + '…' : '—'],
                ['Pinned',     (memory as any).pinned ? 'Yes' : 'No'],
                ['Archived',   (memory as any).archived_at ? 'Yes' : 'No'],
                ['Revisions',  String(memory.revision_count ?? 1)],
              ] as [string, string][]).map(([label, val]) => (
                <div key={label}>
                  <p className="text-[10px] text-text-quaternary uppercase tracking-wide">{label}</p>
                  <p className="text-xs text-text-secondary mt-0.5">{val}</p>
                </div>
              ))}
            </div>

            {/* Tags */}
            {memory.tags && memory.tags.length > 0 && (
              <div className="pt-4 border-t border-border-primary">
                <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-2">Tags</p>
                <div className="flex flex-wrap gap-1.5">
                  {memory.tags.map(tag => (
                    <span key={tag} className="rounded-full px-2 py-0.5 text-[10px] bg-white/[0.06] text-text-secondary">
                      {tag}
                    </span>
                  ))}
                </div>
              </div>
            )}

            {/* Actions */}
            <div className="flex flex-wrap gap-2 pt-4 border-t border-border-primary">
              {(memory as any).pinned ? (
                <button
                  onClick={() => unpinMut.mutate(memory.id)}
                  disabled={unpinMut.isPending}
                  className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary hover:border-border-secondary transition-colors disabled:opacity-40 flex items-center gap-1.5"
                >
                  <Pin className="w-3 h-3 fill-current text-accent-blue" />
                  Unpin
                </button>
              ) : (
                <button
                  onClick={() => pinMut.mutate(memory.id)}
                  disabled={pinMut.isPending}
                  className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary hover:border-border-secondary transition-colors disabled:opacity-40 flex items-center gap-1.5"
                >
                  <Pin className="w-3 h-3" />
                  Pin
                </button>
              )}
              {(memory as any).archived_at ? (
                <button
                  onClick={() => restoreMut.mutate(memory.id)}
                  disabled={restoreMut.isPending}
                  className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary hover:border-border-secondary transition-colors disabled:opacity-40 flex items-center gap-1.5"
                >
                  <RotateCcw className="w-3 h-3" />
                  Restore
                </button>
              ) : (
                <button
                  onClick={() => archiveMut.mutate(memory.id)}
                  disabled={archiveMut.isPending}
                  className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-text-primary hover:border-border-secondary transition-colors disabled:opacity-40 flex items-center gap-1.5"
                >
                  <Archive className="w-3 h-3" />
                  Archive
                </button>
              )}
              {!deleteConfirm ? (
                <button
                  onClick={() => setDeleteConfirm(true)}
                  className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-secondary hover:text-status-error hover:border-status-error/30 transition-colors flex items-center gap-1.5"
                >
                  <Trash2 className="w-3 h-3" />
                  Delete
                </button>
              ) : (
                <div className="flex items-center gap-2">
                  <span className="text-xs text-status-error">Confirm?</span>
                  <button
                    onClick={() => deleteMut.mutate(memory.id)}
                    disabled={deleteMut.isPending}
                    className="rounded-full bg-status-error/10 border border-status-error/30 px-3 py-1 text-xs text-status-error hover:bg-status-error/20 transition-colors disabled:opacity-40"
                  >
                    {deleteMut.isPending ? 'Deleting…' : 'Yes, delete'}
                  </button>
                  <button
                    onClick={() => setDeleteConfirm(false)}
                    className="rounded-full border border-border-primary px-3 py-1 text-xs text-text-quaternary hover:text-text-secondary transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              )}
            </div>

            {/* Related memories */}
            <div className="pt-4 border-t border-border-primary">
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide mb-2">Related memories</p>
              {relatedLoading ? (
                <div className="space-y-2">
                  {[1, 2, 3].map(i => (
                    <div key={i} className="h-10 rounded-[8px] bg-white/[0.04] animate-pulse" />
                  ))}
                </div>
              ) : (
                <div className="space-y-2">
                  {(related ?? [])
                    .filter(r => r.id !== memory.id)
                    .slice(0, 3)
                    .map(r => (
                      <div key={r.id} className="rounded-[11px] bg-white/[0.04] border border-border-primary/50 px-3 py-2">
                        <div className="flex items-center gap-2 mb-0.5">
                          {r.type && <TypeBadge type={r.type} />}
                        </div>
                        <p className="text-xs text-text-secondary line-clamp-2 leading-relaxed">
                          {r.content?.slice(0, 100)}
                        </p>
                      </div>
                    ))}
                  {(related ?? []).filter(r => r.id !== memory.id).length === 0 && (
                    <p className="text-xs text-text-quaternary">No related memories found.</p>
                  )}
                </div>
              )}
            </div>
          </div>
        ) : null}
        </div>
      </div>
    </>
  )
}

// ── Page ──────────────────────────────────────────────────────────────────────

// ── History panel ─────────────────────────────────────────────────────────────

function HistoryPanel({
  memoryId,
  onClose,
  client,
}: {
  memoryId: string
  onClose: () => void
  client: NexusMindClient
}) {
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [onClose])

  const { data, isLoading } = useQuery({
    queryKey: ['audit', 'memory-history', memoryId],
    queryFn: () => client.getAuditLog({ resource_id: memoryId, action: 'memory.updated' }),
    staleTime: 30_000,
  })

  function relativeTs(ts: string) {
    const ms = Date.now() - new Date(ts).getTime()
    const min = Math.floor(ms / 60000)
    if (min < 1) return 'just now'
    if (min < 60) return `${min}m ago`
    const h = Math.floor(min / 60)
    if (h < 24) return `${h}h ago`
    return `${Math.floor(h / 24)}d ago`
  }

  return (
    <div
      ref={panelRef}
      className="absolute right-0 top-6 z-50 bg-[#272729] border border-border-primary rounded-[11px] p-3 shadow-xl w-64"
    >
      <p className="text-[11px] font-semibold text-text-quaternary mb-2">Edit History</p>
      {isLoading ? (
        <div className="space-y-2">
          {[1, 2].map(i => (
            <div key={i} className="h-3 rounded-[4px] bg-[#1d1d1f] animate-pulse" />
          ))}
        </div>
      ) : !data || data.length === 0 ? (
        <p className="text-xs text-text-quaternary text-center py-2">No edits recorded</p>
      ) : (
        <div className="space-y-2 max-h-48 overflow-y-auto">
          {data.map(entry => (
            <div key={entry.id} className="flex items-start gap-2">
              <span className="text-xs text-text-secondary shrink-0 mt-0.5">Edited</span>
              <div className="min-w-0">
                <p className="text-[10px] text-text-quaternary">{relativeTs(entry.timestamp)}</p>
                {entry.user_id && (
                  <p className="text-[10px] text-text-quaternary truncate">by {entry.user_id.slice(0, 8)}</p>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function downloadBlob(data: object, filename: string) {
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url; a.download = filename; a.click()
  URL.revokeObjectURL(url)
}

// ── CSV parser ────────────────────────────────────────────────────────────────

function parseCSV(text: string): ImportMemory[] {
  const lines = text.split('\n').filter(l => l.trim())
  if (lines.length < 2) return []
  const headers = lines[0].split(',')
  return lines.slice(1).map(line => {
    const vals = line.match(/(".*?"|[^,]+)/g) ?? []
    const obj: Record<string, string> = {}
    headers.forEach((h, i) => obj[h.trim()] = (vals[i] ?? '').replace(/^"|"$/g, '').trim())
    return {
      content: obj.content,
      tags: obj.tags ? obj.tags.split(/[;|]/).map(t => t.trim()).filter(Boolean) : [],
    }
  }).filter(m => m.content)
}

export default function Memories() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [searchParams, setSearchParams] = useSearchParams()

  const [query, setQuery] = useState('')
  const [mode, setMode] = useState<'keyword' | 'hybrid'>('hybrid')
  const [selected, setSelected] = useState<Memory | null>(null)
  const [selectedMemoryId, setSelectedMemoryId] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'memories' | 'sessions' | 'tags' | 'duplicates' | 'collections'>('memories')
  const [createMemoryOpen, setCreateMemoryOpen] = useState(false)

  // Open create modal when navigated here with ?new=1 (e.g. via Cmd+N from Layout)
  useEffect(() => {
    if (searchParams.get('new') === '1') {
      setCreateMemoryOpen(true)
      setSearchParams({}, { replace: true })
    }
  }, [searchParams, setSearchParams])

  const [expandedGroups, setExpandedGroups] = useState<Set<number>>(new Set())
  const [expandedSessionId, setExpandedSessionId] = useState<string | null>(null)
  const [showArchived, setShowArchived] = useState(false)
  const debouncedQuery = useDebounce(query, 300)

  // Favorites
  const [favorites, setFavorites] = useState<Set<string>>(() => loadFavorites())
  const [showFavoritesOnly, setShowFavoritesOnly] = useState(false)
  const toggleFavorite = (id: string) => {
    setFavorites(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      saveFavorites(next)
      return next
    })
  }

  // Bulk selection
  const [selectMode, setSelectMode] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const toggleRow = useCallback((id: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }, [])
  const clearSelection = useCallback(() => {
    setSelectedIds(new Set())
    setSelectMode(false)
  }, [])
  const isAdmin = session?.user.role === 'admin'

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

  // Facet filters
  const [filterType, setFilterType] = useState('')
  const [filterScope, setFilterScope] = useState('')
  const [filterProject, setFilterProject] = useState('')
  // Date range filters
  const [fromDate, setFromDate] = useState('')
  const [toDate, setToDate] = useState('')
  // Pinned-only toggle
  const [pinnedOnly, setPinnedOnly] = useState(false)
  // Sort
  type SortBy = 'newest' | 'oldest' | 'most-used'
  const [sortBy, setSortBy] = useState<SortBy>('newest')
  // Collections
  const [filterCollection, setFilterCollection] = useState('')
  const [newCollectionName, setNewCollectionName] = useState('')
  const [newCollectionDesc, setNewCollectionDesc] = useState('')
  const [collectionError, setCollectionError] = useState<string | null>(null)
  const [assigningMemory, setAssigningMemory] = useState<string | null>(null)

  const hasFilters = filterType !== '' || filterScope !== '' || filterProject !== '' || fromDate !== '' || toDate !== '' || filterCollection !== '' || pinnedOnly

  const activeFilterCount = [filterType, filterScope, filterProject, fromDate, toDate, filterCollection, pinnedOnly, query].filter(Boolean).length

  const { data: facets } = useQuery({
    queryKey: ['memory-facets'],
    queryFn: () => client.getMemoryFacets(),
    staleTime: 60_000,
    enabled: isAdmin,
  })

  const isSearching = debouncedQuery.trim().length > 0

  const { data: listData, isLoading: listLoading } = useQuery({
    queryKey: ['memories', 'list', filterType, filterScope, filterProject, showArchived, fromDate, toDate, filterCollection],
    queryFn: () => client.listMemories({
      limit: 50,
      type: filterType || undefined,
      scope: filterScope || undefined,
      project: filterProject || undefined,
      include_archived: showArchived || undefined,
      from_date: fromDate || undefined,
      to_date: toDate || undefined,
      collection_id: filterCollection || undefined,
    }),
    enabled: !isSearching,
  })

  const { data: searchData, isLoading: searchLoading } = useQuery({
    queryKey: ['memories', 'search', debouncedQuery, mode],
    queryFn: () => client.searchMemories(debouncedQuery, 20, mode),
    enabled: isSearching,
  })

  const memoriesRaw = isSearching ? searchData : listData
  const memories = useMemo(() => {
    if (!memoriesRaw) return memoriesRaw
    let result = [...memoriesRaw]
    if (showFavoritesOnly) result = result.filter(m => favorites.has(m.id))
    if (pinnedOnly) result = result.filter(m => (m as any).pinned)
    if (sortBy === 'oldest') result = result.sort((a, b) => a.created_at.localeCompare(b.created_at))
    else if (sortBy === 'newest') result = result.sort((a, b) => b.created_at.localeCompare(a.created_at))
    else if (sortBy === 'most-used') result = result.sort((a, b) => (b.revision_count ?? 0) - (a.revision_count ?? 0))
    return result
  }, [memoriesRaw, showFavoritesOnly, favorites, pinnedOnly, sortBy])
  const isLoading = isSearching ? searchLoading : listLoading

  const { data: sessions, isLoading: sessionsLoading } = useQuery({
    queryKey: ['sessions'],
    queryFn: () => client.listSessions(),
    enabled: activeTab === 'sessions',
  })

  const { data: sessionMemories, isLoading: sessionMemoriesLoading } = useQuery({
    queryKey: ['session-memories', expandedSessionId],
    queryFn: () => client.listMemories({ session_id: expandedSessionId!, limit: 100 }),
    enabled: expandedSessionId !== null,
  })

  const { data: tagStats, isLoading: tagsLoading } = useQuery({
    queryKey: ['tag-stats'],
    queryFn: () => client.getTagStats(),
    enabled: activeTab === 'tags',
    staleTime: 60_000,
  })

  const { data: duplicateGroups, isLoading: duplicatesLoading } = useQuery({
    queryKey: ['memory-duplicates'],
    queryFn: () => client.getDuplicates(),
    enabled: activeTab === 'duplicates',
  })

  const { data: collections, isLoading: collectionsLoading } = useQuery({
    queryKey: ['collections'],
    queryFn: () => client.listCollections(),
    staleTime: 30_000,
  })

  const createCollectionMut = useMutation({
    mutationFn: (data: { name: string; description?: string }) => client.createCollection(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['collections'] })
      setNewCollectionName('')
      setNewCollectionDesc('')
      setCollectionError(null)
    },
    onError: (e: Error) => setCollectionError(e.message),
  })

  const deleteCollectionMut = useMutation({
    mutationFn: (id: string) => client.deleteCollection(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['collections'] }),
  })

  const assignCollectionMut = useMutation({
    mutationFn: ({ memoryId, collectionId }: { memoryId: string; collectionId: string | null }) =>
      client.assignMemoryToCollection(memoryId, { collection_id: collectionId }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setAssigningMemory(null)
    },
  })

  const exportCollection = async (col: Collection) => {
    const result = await client.listMemories({ collection_id: col.id, limit: 500 })
    downloadBlob(
      { collection: col, memories: result, exported_at: new Date().toISOString() },
      `collection-${col.name.toLowerCase().replace(/\s+/g, '-')}.json`
    )
  }

  // Inline edit state
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editContent, setEditContent] = useState('')
  const [editFlash, setEditFlash] = useState<string | null>(null)

  // Session inline rename state
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null)
  const [editSessionSummary, setEditSessionSummary] = useState('')

  const updateSessionMut = useMutation({
    mutationFn: ({ id, summary }: { id: string; summary: string }) =>
      client.updateSession(id, { summary }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sessions'] })
      setEditingSessionId(null)
    },
  })

  const [deleteConfirmSessionId, setDeleteConfirmSessionId] = useState<string | null>(null)

  const deleteSessionMut = useMutation({
    mutationFn: (id: string) => client.deleteSession(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sessions'] })
      setDeleteConfirmSessionId(null)
    },
  })

  // History panel state
  const [historyMemoryId, setHistoryMemoryId] = useState<string | null>(null)

  // Admin note state
  const [editingNoteId, setEditingNoteId] = useState<string | null>(null)
  const [noteInput, setNoteInput] = useState('')

  const noteMut = useMutation({
    mutationFn: ({ id, note }: { id: string; note: string }) =>
      client.updateMemoryNote(id, note),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setEditingNoteId(null)
    },
  })

  // Scheduled deletion state
  const [schedulePopoverId, setSchedulePopoverId] = useState<string | null>(null)

  const scheduleDeleteMut = useMutation({
    mutationFn: ({ id, deleteAfter }: { id: string; deleteAfter: string | null }) =>
      client.scheduleMemoryDelete(id, deleteAfter),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setSchedulePopoverId(null)
    },
  })

  const updateMut = useMutation({
    mutationFn: ({ id, content }: { id: string; content: string }) =>
      client.updateMemory(id, content),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setEditingId(null)
      setEditFlash(updateMut.variables?.id ?? null)
      setTimeout(() => setEditFlash(null), 2000)
    },
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteMemory(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setSelected(null)
    },
    onError: () => {},
  })

  const bulkDeleteMut = useMutation({
    mutationFn: (ids: string[]) => client.bulkDeleteMemories(ids),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      qc.invalidateQueries({ queryKey: ['memory-facets'] })
      clearSelection()
    },
  })

  const bulkArchiveMut = useMutation({
    mutationFn: (ids: string[]) => Promise.all(ids.map(id => client.archiveMemory(id))),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      clearSelection()
    },
  })

  const mergeMut = useMutation({
    mutationFn: ({ keepId, mergeId }: { keepId: string; mergeId: string }) =>
      client.mergeMemories(keepId, mergeId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memory-duplicates'] })
      qc.invalidateQueries({ queryKey: ['memories'] })
    },
    onError: () => {},
  })

  const [archiveError, setArchiveError] = useState<string | null>(null)
  const [restoreError, setRestoreError] = useState<string | null>(null)
  const [pinError, setPinError] = useState<string | null>(null)

  const archiveMut = useMutation({
    mutationFn: (id: string) => client.archiveMemory(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setArchiveError(null)
    },
    onError: (err: Error) => setArchiveError(err.message ?? 'Failed to archive. Please try again.'),
  })

  const restoreMut = useMutation({
    mutationFn: (id: string) => client.restoreMemory(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setRestoreError(null)
    },
    onError: (err: Error) => setRestoreError(err.message ?? 'Failed to restore. Please try again.'),
  })

  const pinMut = useMutation({
    mutationFn: (id: string) => client.pinMemory(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setPinError(null)
    },
    onError: (err: Error) => setPinError(err.message ?? 'Failed to pin. Please try again.'),
  })

  const unpinMut = useMutation({
    mutationFn: (id: string) => client.unpinMemory(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      setPinError(null)
    },
    onError: (err: Error) => setPinError(err.message ?? 'Failed to unpin. Please try again.'),
  })

  const handleBulkDelete = useCallback(() => {
    if (selectedIds.size === 0) return
    bulkDeleteMut.mutate(Array.from(selectedIds))
  }, [selectedIds, bulkDeleteMut])

  const handleBulkArchive = useCallback(() => {
    if (selectedIds.size === 0) return
    bulkArchiveMut.mutate(Array.from(selectedIds))
  }, [selectedIds, bulkArchiveMut])

  // Bulk tag state
  const [tagAction, setTagAction] = useState<'add' | 'remove' | null>(null)
  const [tagInput, setTagInput] = useState('')
  const [tagFlash, setTagFlash] = useState(false)

  // Inline tag rename state
  const [renamingTag, setRenamingTag] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const [renameFlash, setRenameFlash] = useState<string | null>(null)

  const renameTagMut = useMutation({
    mutationFn: ({ from, to }: { from: string; to: string }) => client.renameTag(from, to),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['tag-stats'] })
      qc.invalidateQueries({ queryKey: ['memories'] })
      setRenamingTag(null)
      setRenameValue('')
      setRenameFlash(vars.to)
      setTimeout(() => setRenameFlash(null), 1500)
    },
  })

  const bulkTagMut = useMutation({
    mutationFn: ({ ids, action, tag }: { ids: string[]; action: 'add' | 'remove'; tag: string }) =>
      client.bulkTagMemories(ids, action, tag),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['memories'] })
      qc.invalidateQueries({ queryKey: ['tag-stats'] })
      setSelectedIds(new Set())
      setTagAction(null)
      setTagInput('')
      setTagFlash(true)
      setTimeout(() => setTagFlash(false), 1500)
    },
  })

  const handleBulkTag = useCallback(() => {
    if (!tagAction || !tagInput.trim() || selectedIds.size === 0) return
    bulkTagMut.mutate({ ids: Array.from(selectedIds), action: tagAction, tag: tagInput.trim() })
  }, [tagAction, tagInput, selectedIds, bulkTagMut])

  const [exporting, setExporting] = useState<null | 'csv' | 'json'>(null)
  const [exportOpen, setExportOpen] = useState(false)
  const exportRef = useRef<HTMLDivElement>(null)

  // ── Import state ──────────────────────────────────────────────────────────
  const importFileRef = useRef<HTMLInputElement>(null)
  const [importPending, setImportPending] = useState<ImportMemory[] | null>(null)
  type ImportState = 'idle' | 'loading' | 'success' | 'error'
  const [importState, setImportState] = useState<ImportState>('idle')
  const [importResult, setImportResult] = useState<ImportMemoriesResponse | null>(null)
  const [importError, setImportError] = useState<string | null>(null)
  const [importing, setImporting] = useState(false)
  const [importToast, setImportToast] = useState<{ imported: number; failed: number } | null>(null)

  // ── Drag-and-drop state ───────────────────────────────────────────────────
  const [isDragOver, setIsDragOver] = useState(false)

  // Close export dropdown when clicking outside
  useEffect(() => {
    if (!exportOpen) return
    const handler = (e: MouseEvent) => {
      if (exportRef.current && !exportRef.current.contains(e.target as Node)) {
        setExportOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [exportOpen])

  // ── Memory Search Presets ─────────────────────────────────────────────────
  type FilterPreset = {
    id: string
    name: string
    filters: { q: string; type: string; scope: string; project: string; mode: 'keyword' | 'hybrid'; fromDate: string; toDate: string }
  }

  const PRESETS_KEY = 'nexusmind-memory-presets'

  const loadPresets = (): FilterPreset[] => {
    try { return JSON.parse(localStorage.getItem(PRESETS_KEY) ?? '[]') } catch { return [] }
  }

  const [presets, setPresets] = useState<FilterPreset[]>(loadPresets)
  const [savePresetOpen, setSavePresetOpen] = useState(false)
  const [presetsOpen, setPresetsOpen] = useState(false)
  const [presetName, setPresetName] = useState('')
  const [presetSavedFlash, setPresetSavedFlash] = useState(false)
  const savePresetRef = useRef<HTMLDivElement>(null)
  const presetsRef = useRef<HTMLDivElement>(null)

  // Close popovers when clicking outside
  useEffect(() => {
    if (!savePresetOpen && !presetsOpen) return
    const handler = (e: MouseEvent) => {
      if (savePresetOpen && savePresetRef.current && !savePresetRef.current.contains(e.target as Node)) {
        setSavePresetOpen(false)
      }
      if (presetsOpen && presetsRef.current && !presetsRef.current.contains(e.target as Node)) {
        setPresetsOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [savePresetOpen, presetsOpen])

  const handleSavePreset = () => {
    const name = presetName.trim()
    if (!name) return
    const preset: FilterPreset = {
      id: crypto.randomUUID(),
      name,
      filters: { q: query, type: filterType, scope: filterScope, project: filterProject, mode, fromDate, toDate },
    }
    const updated = [...presets, preset]
    setPresets(updated)
    localStorage.setItem(PRESETS_KEY, JSON.stringify(updated))
    setPresetName('')
    setSavePresetOpen(false)
    setPresetSavedFlash(true)
    setTimeout(() => setPresetSavedFlash(false), 1500)
  }

  const handleApplyPreset = (preset: FilterPreset) => {
    setQuery(preset.filters.q)
    setFilterType(preset.filters.type)
    setFilterScope(preset.filters.scope)
    setFilterProject(preset.filters.project)
    setMode(preset.filters.mode)
    setFromDate(preset.filters.fromDate ?? '')
    setToDate(preset.filters.toDate ?? '')
    setPresetsOpen(false)
  }

  const handleDeletePreset = (id: string) => {
    const updated = presets.filter(p => p.id !== id)
    setPresets(updated)
    localStorage.setItem(PRESETS_KEY, JSON.stringify(updated))
  }

  const handleExport = useCallback(async (format: 'csv' | 'json') => {
    setExportOpen(false)
    setExporting(format)
    try {
      // Fetch all matching memories client-side (up to 5000)
      const all = isSearching
        ? await client.searchMemories(debouncedQuery, 5000, mode)
        : await client.listMemories({
            type: filterType || undefined,
            scope: filterScope || undefined,
            project: filterProject || undefined,
            limit: 5000,
            offset: 0,
          })

      const filename = `memories-${todayStamp()}`
      let blob: Blob

      if (format === 'json') {
        blob = new Blob([JSON.stringify(all, null, 2)], { type: 'application/json' })
      } else {
        const header = 'id,project,tool,type,scope,content,tags,created_at'
        const rows = all.map((m: Memory) => [
          m.id,
          m.project,
          m.tool,
          m.type ?? '',
          m.scope ?? '',
          `"${(m.content ?? '').replace(/"/g, '""')}"`,
          `"${(m.tags ?? []).join(', ')}"`,
          m.created_at,
        ].join(','))
        blob = new Blob([[header, ...rows].join('\n')], { type: 'text/csv' })
      }

      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `${filename}.${format}`
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    } finally {
      setExporting(null)
    }
  }, [client, isSearching, debouncedQuery, mode, filterType, filterScope, filterProject])

  const handleExportServer = useCallback(async () => {
    setExportOpen(false)
    setExporting('json')
    try {
      const blob = await client.exportMemories({
        q: debouncedQuery || undefined,
        collection_id: filterCollection || undefined,
      })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `memories-export-${todayStamp()}.json`
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    } finally {
      setExporting(null)
    }
  }, [client, debouncedQuery, filterCollection])

  const parseAndStageFile = useCallback((file: File) => {
    const reader = new FileReader()
    reader.onload = (ev) => {
      try {
        const text = ev.target?.result as string
        const isCSV = file.name.toLowerCase().endsWith('.csv')

        let memories: ImportMemory[] | null = null

        if (isCSV) {
          memories = parseCSV(text)
          if (memories.length === 0) {
            setImportPending(null)
            setImportError('CSV file is empty or has no valid rows')
            setImportState('error')
            return
          }
        } else {
          const parsed = JSON.parse(text)
          memories = Array.isArray(parsed)
            ? parsed
            : Array.isArray(parsed?.memories)
              ? parsed.memories
              : null

          if (!memories) {
            setImportPending(null)
            setImportError('Invalid JSON: expected { memories: [...] } or [...]')
            setImportState('error')
            return
          }
        }

        setImportPending(memories)
        setImportResult(null)
        setImportError(null)
        setImportState('idle')
      } catch {
        setImportPending(null)
        setImportError('Could not parse file — make sure it is valid JSON or CSV')
        setImportState('error')
      }
    }
    reader.readAsText(file)
  }, [])

  const handleImportFileChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!importFileRef.current) return
    importFileRef.current.value = ''
    if (!file) return
    parseAndStageFile(file)
  }, [parseAndStageFile])

  const handleImportConfirm = useCallback(async () => {
    if (!importPending) return
    setImportState('loading')
    setImporting(true)
    try {
      const result = await client.importMemories(importPending)
      setImportResult(result)
      setImportState('success')
      qc.invalidateQueries({ queryKey: ['memories'] })
      // Show toast summary and auto-close modal after a moment
      setImportToast({ imported: result.imported, failed: result.errors?.length ?? 0 })
      setTimeout(() => setImportToast(null), 4000)
      setTimeout(() => handleImportClose(), 1500)
    } catch (err) {
      setImportError((err as Error)?.message ?? 'Import failed')
      setImportState('error')
    } finally {
      setImporting(false)
    }
  }, [importPending, client, qc])

  const handleImportClose = useCallback(() => {
    setImportPending(null)
    setImportResult(null)
    setImportError(null)
    setImportState('idle')
  }, [])

  // ── Drag-and-drop handlers ─────────────────────────────────────────────────
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setIsDragOver(true)
  }, [])

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    if (e.currentTarget.contains(e.relatedTarget as Node)) return
    setIsDragOver(false)
  }, [])

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setIsDragOver(false)
    if (!isAdmin) return
    const file = e.dataTransfer.files?.[0]
    if (!file) return
    const name = file.name.toLowerCase()
    if (!name.endsWith('.json') && !name.endsWith('.csv')) return
    parseAndStageFile(file)
  }, [isAdmin, parseAndStageFile])

  return (
    <div
      className="p-8 max-w-5xl mx-auto space-y-6"
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {/* Drag-and-drop overlay */}
      {isDragOver && (
        <div className="fixed inset-0 bg-accent-blue/5 border-2 border-dashed border-accent-blue/40 z-20 flex items-center justify-center pointer-events-none">
          <p className="text-sm text-accent-blue font-semibold">Drop JSON or CSV to import memories</p>
        </div>
      )}

      {/* Import result toast */}
      {importToast && (
        <div className="fixed bottom-6 right-6 z-50 bg-[#272729] border border-border-primary rounded-[11px] px-4 py-3 shadow-xl flex items-center gap-3">
          <CheckCircle2 className="w-4 h-4 text-status-success shrink-0" />
          <span className="text-sm text-text-primary">
            Imported {importToast.imported} {importToast.imported === 1 ? 'memory' : 'memories'}
            {importToast.failed > 0 ? ` (${importToast.failed} failed)` : ''}
          </span>
        </div>
      )}

      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-[11px] bg-accent-blue/10 border border-accent-blue/20">
            <Brain className="w-4 h-4 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Memories</h1>
            <p className="text-[12px] text-text-tertiary">
              {memories ? `${memories.length} entries` : 'Browse and search stored memories'}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {/* Select mode controls — admin only, memories tab only */}
          {isAdmin && activeTab === 'memories' && (
            selectMode ? (
              <>
                <button
                  onClick={() => {
                    if (memories) setSelectedIds(new Set(memories.map(m => m.id)))
                  }}
                  className="border border-border-primary rounded-full px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
                >
                  Select all ({memories?.length ?? 0})
                </button>
                <button
                  onClick={clearSelection}
                  className="border border-border-primary rounded-full px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
                >
                  Cancel
                </button>
              </>
            ) : (
              <button
                onClick={() => setSelectMode(true)}
                className="border border-border-primary rounded-full px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
              >
                Select
              </button>
            )
          )}

          {/* Save Filter preset */}
          <div ref={savePresetRef} className="relative">
            <button
              onClick={() => { setSavePresetOpen(prev => !prev); setPresetsOpen(false) }}
              aria-label="Save current filter as preset"
              className="border border-border-primary rounded-full px-3 py-1.5 text-sm text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
            >
              {presetSavedFlash
                ? <><Check className="w-3.5 h-3.5 text-status-success" /><span className="text-status-success">Saved</span></>
                : <><Bookmark className="w-3.5 h-3.5" />Save filter</>
              }
            </button>
            {savePresetOpen && (
              <div className="absolute right-0 top-full mt-1 bg-[#272729] border border-border-primary rounded-[11px] p-3 z-20 min-w-[200px] space-y-2">
                <p className="text-xs text-text-tertiary">Name this filter preset</p>
                <input
                  autoFocus
                  value={presetName}
                  onChange={e => setPresetName(e.target.value)}
                  onKeyDown={e => { if (e.key === 'Enter') handleSavePreset(); if (e.key === 'Escape') setSavePresetOpen(false) }}
                  placeholder="e.g. My bugfixes"
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60"
                />
                <button
                  onClick={handleSavePreset}
                  disabled={!presetName.trim()}
                  className="w-full rounded-full bg-accent-blue text-white text-xs font-semibold py-1.5 hover:opacity-90 disabled:opacity-40 transition-opacity"
                >
                  Save
                </button>
              </div>
            )}
          </div>

          {/* Presets picker — only visible when presets exist */}
          {presets.length > 0 && (
            <div ref={presetsRef} className="relative">
              <button
                onClick={() => { setPresetsOpen(prev => !prev); setSavePresetOpen(false) }}
                aria-label="Load a filter preset"
                className="border border-border-primary rounded-full px-3 py-1.5 text-sm text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
              >
                <BookmarkCheck className="w-3.5 h-3.5" />
                Presets
              </button>
              {presetsOpen && (
                <div className="absolute right-0 top-full mt-1 bg-[#272729] border border-border-primary rounded-[11px] py-1 z-20 min-w-[180px]">
                  {presets.map(preset => (
                    <div
                      key={preset.id}
                      className="flex items-center justify-between gap-2 px-3 py-2 hover:bg-white/[0.04] transition-colors group"
                    >
                      <button
                        onClick={() => handleApplyPreset(preset)}
                        className="flex-1 text-left text-sm text-text-secondary hover:text-text-primary transition-colors truncate"
                      >
                        {preset.name}
                      </button>
                      <button
                        onClick={() => handleDeletePreset(preset.id)}
                        aria-label={`Delete preset ${preset.name}`}
                        className="text-text-quaternary hover:text-status-error transition-colors opacity-0 group-hover:opacity-100 shrink-0"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Import */}
          {isAdmin && (
            <>
              <input
                ref={importFileRef}
                type="file"
                accept=".json,.csv"
                className="hidden"
                onChange={handleImportFileChange}
                aria-label="Select JSON or CSV file to import"
              />
              <button
                onClick={() => importFileRef.current?.click()}
                aria-label="Import memories from JSON or CSV"
                className="border border-border-primary rounded-full px-3 py-2 text-sm text-text-secondary hover:text-text-primary flex items-center gap-2 transition-colors"
              >
                {importing ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Upload className="w-3.5 h-3.5" />}
                {importing ? 'Importing…' : 'Import'}
              </button>
            </>
          )}

          {/* New memory */}
          <button
            onClick={() => setCreateMemoryOpen(true)}
            className="border border-border-primary rounded-full px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
          >
            <Plus className="w-3 h-3" />
            New memory
          </button>

          {/* Export */}
          <div ref={exportRef} className="relative">
            <button
              onClick={() => setExportOpen(prev => !prev)}
              disabled={exporting !== null}
              aria-label="Export memories"
              aria-expanded={exportOpen}
              aria-haspopup="menu"
              className="border border-border-primary rounded-full px-3 py-1.5 text-sm text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors disabled:opacity-30"
            >
              {exporting !== null
                ? <><span className="w-3.5 h-3.5 border-2 border-current border-t-transparent rounded-full animate-spin" />Exporting…</>
                : <><Download className="w-3.5 h-3.5" />Export</>
              }
            </button>
            {exportOpen && (
              <div
                role="menu"
                className="absolute right-0 top-full mt-1 bg-[#272729] border border-border-primary rounded-[11px] py-1 z-10 min-w-[160px]"
              >
                <button
                  role="menuitem"
                  onClick={() => handleExport('json')}
                  className="block w-full text-left px-4 py-2 text-sm text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors"
                >
                  Export JSON
                </button>
                <button
                  role="menuitem"
                  onClick={() => handleExport('csv')}
                  className="block w-full text-left px-4 py-2 text-sm text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors"
                >
                  Export CSV
                </button>
                <div className="border-t border-border-secondary/40 my-1" />
                <button
                  role="menuitem"
                  onClick={handleExportServer}
                  className="block w-full text-left px-4 py-2 text-sm text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors"
                >
                  Export via API
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Import confirmation modal */}
      {(importPending !== null || importState === 'error') && (
        <div
          className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center"
          onClick={() => importState !== 'loading' && handleImportClose()}
          role="dialog"
          aria-modal="true"
          aria-label="Import memories"
        >
          <div
            className="bg-[#272729] border border-border-primary rounded-[18px] p-6 max-w-md w-full mx-4 space-y-4"
            onClick={e => e.stopPropagation()}
          >
            {importState === 'success' && importResult ? (
              <>
                <p className="text-sm font-semibold text-status-success">
                  ✓ Imported {importResult.imported} {importResult.imported === 1 ? 'memory' : 'memories'}.
                  {importResult.skipped > 0 && ` ${importResult.skipped} skipped (empty content).`}
                </p>
                {importResult.errors.length > 0 && (
                  <div className="space-y-1">
                    <p className="text-xs text-text-quaternary">Errors:</p>
                    {importResult.errors.slice(0, 5).map((e, i) => (
                      <p key={i} className="text-xs text-status-error/80 font-mono">{e}</p>
                    ))}
                  </div>
                )}
                <div className="flex justify-end">
                  <button
                    onClick={handleImportClose}
                    className="rounded-full border border-border-primary text-text-secondary font-semibold px-4 py-2 text-sm hover:text-text-primary transition-colors"
                  >
                    Done
                  </button>
                </div>
              </>
            ) : importState === 'error' ? (
              <>
                <p className="text-sm font-semibold text-text-primary">Import failed</p>
                <p className="text-sm text-status-error/80">{importError}</p>
                <div className="flex justify-end">
                  <button
                    onClick={handleImportClose}
                    className="rounded-full border border-border-primary text-text-secondary font-semibold px-4 py-2 text-sm hover:text-text-primary transition-colors"
                  >
                    Close
                  </button>
                </div>
              </>
            ) : (
              <>
                <p className="text-sm font-semibold text-text-primary">
                  Import {importPending?.length ?? 0} {(importPending?.length ?? 0) === 1 ? 'memory' : 'memories'}?
                </p>
                {importPending && importPending.length > 0 && (
                  <div className="space-y-1">
                    <p className="text-xs text-text-quaternary">Preview:</p>
                    {importPending.slice(0, 3).map((m, i) => (
                      <p key={i} className="text-xs text-text-tertiary font-mono bg-[#1d1d1f] rounded-[8px] px-3 py-1.5 truncate">
                        {m.content.slice(0, 50)}{m.content.length > 50 ? '…' : ''}
                      </p>
                    ))}
                  </div>
                )}
                <div className="flex gap-3 justify-end pt-1">
                  <button
                    onClick={handleImportClose}
                    disabled={importState === 'loading'}
                    className="rounded-full border border-border-primary text-text-secondary font-semibold px-4 py-2 text-sm hover:text-text-primary transition-colors disabled:opacity-40"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleImportConfirm}
                    disabled={importState === 'loading'}
                    className="rounded-full bg-accent-blue text-white font-semibold px-4 py-2 text-sm hover:bg-accent-blue/90 transition-colors disabled:opacity-40 flex items-center gap-2"
                  >
                    {importState === 'loading'
                      ? <><Loader2 className="w-3.5 h-3.5 animate-spin" />Importing…</>
                      : 'Import'
                    }
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* Tabs */}
      <div className="bg-[#1d1d1f] border border-border-primary rounded-[11px] px-1 flex w-fit">
        {(['memories', 'sessions', 'tags', 'duplicates', 'collections'] as const).map(tab => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-full transition-colors ${
              activeTab === tab
                ? 'bg-white/[0.08] text-text-primary font-semibold'
                : 'text-text-secondary hover:text-text-primary font-normal'
            }`}
          >
            {tab === 'memories'
              ? <><Brain className="w-3.5 h-3.5" /> Memories</>
              : tab === 'sessions'
              ? <><Clock className="w-3.5 h-3.5" /> Sessions</>
              : tab === 'tags'
              ? <><Hash className="w-3.5 h-3.5" /> Tags</>
              : tab === 'duplicates'
              ? <>
                  <Copy className="w-3.5 h-3.5" />
                  Duplicates
                  {duplicateGroups && duplicateGroups.length > 0 && (
                    <span className="ml-1 rounded-full bg-status-error/10 border border-status-error/20 text-status-error text-[10px] px-1.5">
                      {duplicateGroups.length}
                    </span>
                  )}
                </>
              : <>
                  <Folder className="w-3.5 h-3.5" />
                  Collections
                  {collections && collections.length > 0 && (
                    <span className="ml-1 rounded-full bg-[#272729] border border-border-primary text-text-quaternary text-[10px] px-1.5">
                      {collections.length}
                    </span>
                  )}
                </>
            }
          </button>
        ))}
      </div>

      {activeTab === 'memories' && <>
      {/* Search */}
      <div className="flex gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-text-quaternary" />
          <input
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Search memories…"
            aria-label="Search memories"
            className="w-full bg-transparent border border-border-primary rounded-full pl-10 pr-4 py-3 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors"
          />
          {query && (
            <button
              onClick={() => setQuery('')}
              aria-label="Clear search"
              className="absolute right-3.5 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-tertiary transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
        <div className="flex items-center bg-transparent border border-border-primary rounded-[11px] px-1 gap-0.5">
          {(['keyword', 'hybrid'] as const).map(m => (
            <button
              key={m}
              onClick={() => setMode(m)}
              className={`px-3 py-1.5 text-xs font-normal rounded-[8px] transition-colors ${
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

      {/* Facet filters — admin only, only when facets loaded */}
      {isAdmin && facets && (facets.types.length > 0 || facets.projects.length > 0) && (
        <div className="flex items-center gap-2 flex-wrap">
          <SlidersHorizontal className="w-3.5 h-3.5 text-text-quaternary shrink-0" />
          {facets.types.length > 0 && (
            <FacetSelect
              label="All types"
              value={filterType}
              onChange={setFilterType}
              options={facets.types}
            />
          )}
          {facets.scopes.length > 0 && (
            <FacetSelect
              label="All scopes"
              value={filterScope}
              onChange={setFilterScope}
              options={facets.scopes}
            />
          )}
          {facets.projects.length > 0 && (
            <FacetSelect
              label="All projects"
              value={filterProject}
              onChange={setFilterProject}
              options={facets.projects}
            />
          )}
          <input
            type="date"
            value={fromDate}
            onChange={e => setFromDate(e.target.value)}
            className="bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 [color-scheme:dark]"
            aria-label="From date"
          />
          <span className="text-xs text-text-quaternary">–</span>
          <input
            type="date"
            value={toDate}
            onChange={e => setToDate(e.target.value)}
            className="bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 [color-scheme:dark]"
            aria-label="To date"
          />
          {collections && collections.length > 0 && (
            <div className="relative">
              <select
                value={filterCollection}
                onChange={e => setFilterCollection(e.target.value)}
                className="appearance-none bg-transparent border border-border-secondary/40 rounded-[8px] pl-3 pr-7 py-1.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer"
                aria-label="Filter by collection"
              >
                <option value="">All collections</option>
                {collections.map(col => (
                  <option key={col.id} value={col.id}>{col.name}</option>
                ))}
              </select>
              <svg className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 w-3 h-3 text-text-quaternary" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
              </svg>
            </div>
          )}
          {/* Pinned only toggle */}
          <button
            onClick={() => setPinnedOnly(v => !v)}
            className={`rounded-full border px-3 py-1.5 text-xs font-semibold transition-colors flex items-center gap-1.5 ${
              pinnedOnly
                ? 'bg-accent-blue/10 text-accent-blue border-accent-blue/40'
                : 'border-border-primary text-text-quaternary hover:text-text-secondary'
            }`}
          >
            <Pin className="w-3 h-3" />
            Pinned
          </button>

          {/* Sort dropdown */}
          <div className="relative">
            <select
              value={sortBy}
              onChange={e => setSortBy(e.target.value as SortBy)}
              className="appearance-none bg-white/[0.04] border border-border-primary rounded-[8px] pl-3 pr-7 py-1.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors cursor-pointer"
              aria-label="Sort memories"
            >
              <option value="newest">Newest first</option>
              <option value="oldest">Oldest first</option>
              <option value="most-used">Most revised</option>
            </select>
            <svg className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 w-3 h-3 text-text-quaternary" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
            </svg>
          </div>

          {/* Active filter badge + clear all */}
          {activeFilterCount > 0 && (
            <button
              onClick={() => { setFilterType(''); setFilterScope(''); setFilterProject(''); setFromDate(''); setToDate(''); setFilterCollection(''); setPinnedOnly(false); setQuery('') }}
              className="rounded-full border border-border-primary px-3 py-1.5 text-[10px] text-accent-blue hover:text-accent-blue/80 transition-colors flex items-center gap-1.5"
            >
              <span className="bg-accent-blue/10 text-accent-blue rounded-full px-1.5 py-0.5 text-[10px] font-semibold">{activeFilterCount}</span>
              filters active · clear all
            </button>
          )}

          {hasFilters && !activeFilterCount && (
            <button
              onClick={() => { setFilterType(''); setFilterScope(''); setFilterProject(''); setFromDate(''); setToDate(''); setFilterCollection(''); setPinnedOnly(false) }}
              className="text-[11px] text-text-quaternary hover:text-text-tertiary transition-colors flex items-center gap-1"
            >
              <X className="w-3 h-3" /> Clear filters
            </button>
          )}
          <button
            onClick={() => setShowArchived(prev => !prev)}
            className={`rounded-full border px-3 py-1.5 text-xs font-semibold transition-colors ${
              showArchived
                ? 'border-accent-blue/40 text-accent-blue bg-accent-blue/10'
                : 'border-border-primary text-text-quaternary hover:text-text-secondary'
            }`}
          >
            {showArchived ? 'Showing archived' : 'Show archived'}
          </button>
          <button
            onClick={() => setShowFavoritesOnly(v => !v)}
            className={showFavoritesOnly
              ? "bg-[#272729] text-text-primary rounded-full px-2.5 py-1 text-xs flex items-center gap-1.5"
              : "text-text-quaternary hover:text-text-secondary rounded-full px-2.5 py-1 text-xs flex items-center gap-1.5 transition-colors"
            }
          >
            <Star className="w-3 h-3" />
            Favorites
          </button>
        </div>
      )}

      {/* Mutation error banners */}
      {archiveError && (
        <p className="text-xs text-status-error mt-1">Archive failed: {archiveError}</p>
      )}
      {restoreError && (
        <p className="text-xs text-status-error mt-1">Restore failed: {restoreError}</p>
      )}
      {pinError && (
        <p className="text-xs text-status-error mt-1">Pin failed: {pinError}</p>
      )}
      {tagFlash && (
        <p className="text-xs text-status-success mt-1">Tags updated</p>
      )}

      {/* Table */}
      <div className="border border-border-primary rounded-[18px] overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="bg-[#272729] border-b border-border-primary">
              {/* Select-all checkbox — admin only, select mode only */}
              <th className="w-10 px-4 py-3">
                {isAdmin && selectMode && memories && memories.length > 0 && (
                  <input
                    type="checkbox"
                    aria-label="Select all memories"
                    checked={memories.length > 0 && memories.every(m => selectedIds.has(m.id))}
                    onChange={e => {
                      if (e.target.checked) {
                        setSelectedIds(new Set(memories.map(m => m.id)))
                      } else {
                        setSelectedIds(new Set())
                      }
                    }}
                    className="rounded border-border-primary accent-accent-blue cursor-pointer"
                  />
                )}
              </th>
              {['Date', 'User', 'Type', 'Memory', '', '', '', '', '', '', ''].map((h, i) => (
                <th key={`h-${i}`} className="text-left px-4 py-3 text-[11px] font-semibold text-text-tertiary tracking-[-0.12px]">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {isLoading
              ? Array.from({ length: 5 }).map((_, i) => (
                <tr key={i} className="border-t border-border-secondary">
                  {Array.from({ length: 6 }).map((_, j) => (
                    <td key={j} className="px-4 py-4">
                      <div className="h-3.5 rounded-[5px] bg-[#272729] animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : memories?.map((mem, idx) => {
                const isChecked = selectedIds.has(mem.id)
                const isEditing = editingId === mem.id
                const isSaving = updateMut.isPending && updateMut.variables?.id === mem.id
                const didSave = editFlash === mem.id
                return (
                <tr
                  key={mem.id}
                  onClick={() => {
                    if (isEditing) return
                    if (selectMode) { toggleRow(mem.id) }
                    else setSelectedMemoryId(mem.id)
                  }}
                  onKeyDown={e => {
                    if (!isEditing && (e.key === 'Enter' || e.key === ' ')) {
                      e.preventDefault()
                      if (selectMode) toggleRow(mem.id)
                      else setSelectedMemoryId(mem.id)
                    }
                  }}
                  role="button"
                  tabIndex={0}
                  aria-label={`View memory: ${mem.title ?? 'untitled'}`}
                  className={`border-t border-border-secondary transition-colors cursor-pointer group focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent-blue/40 ${idx === 0 ? 'border-t-0' : ''} ${isChecked ? 'bg-accent-blue/[0.06] ring-1 ring-accent-blue/60' : ''} ${isEditing ? 'bg-[#1d1d1f]' : 'hover:bg-accent-blue/[0.04]'} ${didSave ? 'bg-status-success/5' : ''} ${mem.pinned ? 'border-l-2 border-l-accent-blue/40' : ''}`}
                >
                  {/* Row checkbox — only shown in selectMode */}
                  <td className="w-10 px-4 py-3.5" onClick={e => e.stopPropagation()}>
                    {isAdmin && selectMode && (
                      <input
                        type="checkbox"
                        aria-label={`Select memory ${mem.id}`}
                        checked={isChecked}
                        onChange={() => toggleRow(mem.id)}
                        className="absolute top-3 left-3 w-4 h-4 rounded border border-border-primary bg-white/[0.04] accent-accent-blue cursor-pointer"
                      />
                    )}
                  </td>
                  <td className="px-4 py-3.5 whitespace-nowrap">
                    <p className="text-xs font-semibold text-text-secondary">
                      {new Date(mem.created_at).toLocaleDateString()}
                    </p>
                    <p className="text-[11px] text-text-quaternary mt-0.5">
                      {new Date(mem.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                    </p>
                  </td>
                  <td className="px-4 py-3.5">
                    <div className="space-y-1">
                      <p className="text-xs text-text-secondary font-semibold">
                        {userMap.get(mem.user_id) ?? '—'}
                      </p>
                      <span className="text-[10px] border border-border-primary rounded-[5px] px-1.5 py-0.5 text-text-quaternary bg-[#272729]/50 inline-block">
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
                  <td className="px-4 py-3.5 max-w-sm" onClick={e => { if (isEditing) e.stopPropagation() }}>
                    {isEditing ? (
                      <div onClick={e => e.stopPropagation()}>
                        <textarea
                          autoFocus
                          value={editContent}
                          onChange={e => setEditContent(e.target.value)}
                          onKeyDown={e => {
                            if (e.key === 'Escape') { setEditingId(null) }
                          }}
                          rows={3}
                          className="w-full text-xs text-text-secondary bg-[#1d1d1f] border border-accent-blue/40 rounded-[8px] p-2 resize-none focus:outline-none focus:border-accent-blue"
                        />
                        <div className="flex items-center justify-between mt-1">
                          <span className="text-[10px] text-text-quaternary">
                            {editContent.trim().split(/\s+/).filter(Boolean).length} words · {editContent.length} chars
                          </span>
                          <div className="flex items-center gap-2">
                            <button
                              onClick={() => updateMut.mutate({ id: mem.id, content: editContent })}
                              disabled={isSaving || editContent.trim() === ''}
                              aria-label="Save edit"
                              className="flex items-center gap-1 text-[11px] text-status-success hover:text-status-success/80 disabled:opacity-40 transition-colors"
                            >
                              <Check className="w-3 h-3" />
                              Save
                            </button>
                            <button
                              onClick={() => setEditingId(null)}
                              disabled={isSaving}
                              aria-label="Cancel edit"
                              className="flex items-center gap-1 text-[11px] text-text-quaternary hover:text-text-tertiary disabled:opacity-40 transition-colors"
                            >
                              <X className="w-3 h-3" />
                              Cancel
                            </button>
                            {updateMut.isError && updateMut.variables?.id === mem.id && (
                              <span className="text-[11px] text-status-error/80">
                                {(updateMut.error as Error)?.message ?? 'Failed to save'}
                              </span>
                            )}
                          </div>
                        </div>
                      </div>
                    ) : (
                      <>
                        <div className="flex items-center gap-2 flex-wrap mb-0.5">
                          {mem.title && (
                            <p className="text-xs font-semibold text-text-primary truncate group-hover:text-accent-blue transition-colors">
                              {mem.title}
                            </p>
                          )}
                          {mem.archived_at && (
                            <span className="text-[10px] bg-[#272729] text-text-quaternary border border-border-primary rounded-[5px] px-1.5 py-0.5 shrink-0">
                              archived
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-text-tertiary line-clamp-2 leading-relaxed">
                          {isSearching && debouncedQuery.length >= 2
                            ? highlightMatch((mem.content ?? '').replace(/#+\s/g, '').replace(/\*\*/g, ''), debouncedQuery)
                            : (mem.content ?? '').replace(/#+\s/g, '').replace(/\*\*/g, '')}
                        </p>
                        {mem.tags.length > 0 && (
                          <div className="flex gap-1 flex-wrap mt-1.5">
                            {mem.tags.slice(0, 3).map(tag => (
                              <span key={tag} className="text-[10px] bg-[#272729] text-text-quaternary rounded-[5px] px-1.5 py-0.5">
                                {tag}
                              </span>
                            ))}
                          </div>
                        )}
                        {/* Scheduled deletion chip */}
                        {mem.delete_after && (
                          <div className="mt-1.5 flex items-center gap-1" onClick={e => e.stopPropagation()}>
                            <span className="text-[10px] text-status-warning bg-status-warning/10 rounded-[5px] px-1.5 py-0.5 border border-status-warning/30 flex items-center gap-1">
                              <CalendarClock className="w-2.5 h-2.5" />
                              Deletes {mem.delete_after}
                            </span>
                            {isAdmin && (
                              <button
                                onClick={() => scheduleDeleteMut.mutate({ id: mem.id, deleteAfter: null })}
                                aria-label="Cancel scheduled deletion"
                                className="p-0.5 text-status-warning/60 hover:text-status-warning transition-colors"
                              >
                                <X className="w-2.5 h-2.5" />
                              </button>
                            )}
                          </div>
                        )}

                        {/* Admin note section — admin only */}
                        {isAdmin && (
                          <div className="mt-2" onClick={e => e.stopPropagation()}>
                            {editingNoteId === mem.id ? (
                              <div className="flex flex-col gap-1.5">
                                <textarea
                                  autoFocus
                                  value={noteInput}
                                  onChange={e => setNoteInput(e.target.value)}
                                  onBlur={() => noteMut.mutate({ id: mem.id, note: noteInput })}
                                  onKeyDown={e => {
                                    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                                      noteMut.mutate({ id: mem.id, note: noteInput })
                                    }
                                    if (e.key === 'Escape') {
                                      setEditingNoteId(null)
                                    }
                                  }}
                                  placeholder="Add admin note…"
                                  maxLength={500}
                                  className="bg-white/[0.04] border border-border-primary rounded-[8px] px-3 py-2 text-xs text-text-secondary resize-none w-full h-16 focus:border-accent-blue/60 focus:outline-none"
                                />
                                <span className="text-[10px] text-text-quaternary mt-0.5">
                                  {noteInput.length} / 500
                                </span>
                              </div>
                            ) : mem.admin_note ? (
                              <div className="flex items-start gap-1.5">
                                <span className="text-[10px] bg-status-warning/10 text-status-warning rounded-[5px] px-1.5 py-0.5 border border-status-warning/30 shrink-0 mt-0.5">
                                  Note
                                </span>
                                <span className="text-xs text-status-warning italic flex-1 min-w-0 break-words">{mem.admin_note}</span>
                                <button
                                  onClick={() => { setEditingNoteId(mem.id); setNoteInput(mem.admin_note ?? '') }}
                                  aria-label="Edit note"
                                  className="p-0.5 text-text-quaternary hover:text-text-primary transition-colors shrink-0"
                                >
                                  <Pencil className="w-3 h-3" />
                                </button>
                                <button
                                  onClick={() => noteMut.mutate({ id: mem.id, note: '' })}
                                  aria-label="Clear note"
                                  className="p-0.5 text-text-quaternary hover:text-status-error transition-colors shrink-0"
                                >
                                  <X className="w-3 h-3" />
                                </button>
                              </div>
                            ) : (
                              <button
                                onClick={() => { setEditingNoteId(mem.id); setNoteInput('') }}
                                className="text-[10px] text-text-quaternary hover:text-text-secondary transition-colors"
                              >
                                + Add note
                              </button>
                            )}
                          </div>
                        )}
                      </>
                    )}
                  </td>
                  {/* Assign-to-collection action cell */}
                  <td className="px-4 py-3.5 w-8 relative" onClick={e => e.stopPropagation()}>
                    {isAdmin && (
                      <div className="relative">
                        <button
                          onClick={() => setAssigningMemory(assigningMemory === mem.id ? null : mem.id)}
                          aria-label={`Assign memory ${mem.id} to collection`}
                          className={`p-1 rounded-[5px] transition-all ${mem.collection_id ? 'text-accent-blue' : 'text-text-quaternary opacity-0 group-hover:opacity-100 hover:text-accent-blue hover:bg-accent-blue/10'}`}
                        >
                          <Folder className="w-3 h-3" />
                        </button>
                        {assigningMemory === mem.id && (
                          <div className="absolute right-0 top-6 z-20 bg-[#272729] border border-border-primary rounded-[11px] py-1 min-w-[160px] shadow-xl">
                            <button
                              className="w-full text-left px-3 py-2 text-xs text-text-quaternary hover:bg-white/[0.04] transition-colors"
                              onClick={() => assignCollectionMut.mutate({ memoryId: mem.id, collectionId: null })}
                            >
                              None
                            </button>
                            {collections?.map(col => (
                              <button
                                key={col.id}
                                className={`w-full text-left px-3 py-2 text-xs transition-colors ${mem.collection_id === col.id ? 'text-accent-blue bg-accent-blue/10' : 'text-text-secondary hover:bg-white/[0.04]'}`}
                                onClick={() => assignCollectionMut.mutate({ memoryId: mem.id, collectionId: col.id })}
                              >
                                {col.name}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </td>
                  {/* Pin action cell */}
                  <td className="px-4 py-3.5 w-8" onClick={e => e.stopPropagation()}>
                    {isAdmin && (
                      <button
                        onClick={() => mem.pinned ? unpinMut.mutate(mem.id) : pinMut.mutate(mem.id)}
                        disabled={(pinMut.isPending && pinMut.variables === mem.id) || (unpinMut.isPending && unpinMut.variables === mem.id)}
                        aria-label={mem.pinned ? `Unpin memory ${mem.id}` : `Pin memory ${mem.id}`}
                        className={`p-1 rounded-[5px] transition-all ${mem.pinned ? 'text-accent-blue' : 'text-text-quaternary opacity-0 group-hover:opacity-100 hover:text-accent-blue hover:bg-accent-blue/10'}`}
                      >
                        <Pin className={`w-3 h-3 ${mem.pinned ? 'fill-current' : ''}`} />
                      </button>
                    )}
                  </td>
                  {/* Favorite action cell */}
                  <td className="px-4 py-3.5 w-8" onClick={e => e.stopPropagation()}>
                    <button
                      onClick={() => toggleFavorite(mem.id)}
                      aria-label="Toggle favorite"
                      className="p-1 rounded-[5px] transition-colors"
                    >
                      <Star
                        className={favorites.has(mem.id)
                          ? "text-status-warning fill-current w-3.5 h-3.5"
                          : "opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-status-warning w-3.5 h-3.5"
                        }
                      />
                    </button>
                  </td>
                  {/* Edit + History action cell */}
                  <td className="px-4 py-3.5 w-8 relative" onClick={e => e.stopPropagation()}>
                    <div className="flex items-center gap-1">
                      {didSave ? (
                        <span className="flex items-center gap-1 text-[10px] text-status-success animate-pulse">
                          <History className="w-3 h-3" />
                          Edited
                        </span>
                      ) : (!isEditing && isAdmin && (
                        <button
                          onClick={() => { setEditingId(mem.id); setEditContent(mem.content) }}
                          aria-label={`Edit memory ${mem.id}`}
                          className="p-1 rounded-[7px] text-text-quaternary hover:text-accent-blue hover:bg-accent-blue/10 opacity-0 group-hover:opacity-100 transition-all"
                        >
                          <Pencil className="w-3 h-3" />
                        </button>
                      ))}
                      {isAdmin && !isEditing && (
                        <button
                          onClick={() => setHistoryMemoryId(historyMemoryId === mem.id ? null : mem.id)}
                          aria-label={`View edit history for memory ${mem.id}`}
                          className="p-1 rounded-[7px] text-text-quaternary hover:text-text-primary hover:bg-[#272729] opacity-0 group-hover:opacity-100 transition-opacity"
                        >
                          <History className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>
                    {historyMemoryId === mem.id && (
                      <HistoryPanel
                        memoryId={mem.id}
                        client={client}
                        onClose={() => setHistoryMemoryId(null)}
                      />
                    )}
                  </td>
                  {/* Schedule delete action cell */}
                  <td className="px-4 py-3.5 w-8 relative" onClick={e => e.stopPropagation()}>
                    {isAdmin && (
                      <div className="relative">
                        <button
                          onClick={() => setSchedulePopoverId(schedulePopoverId === mem.id ? null : mem.id)}
                          aria-label={`Schedule deletion for memory ${mem.id}`}
                          className={cn(
                            'p-1 rounded-[5px] transition-all',
                            mem.delete_after
                              ? 'text-status-warning'
                              : 'text-text-quaternary opacity-0 group-hover:opacity-100 hover:text-text-primary hover:bg-status-warning/10',
                          )}
                        >
                          <CalendarClock className="w-3.5 h-3.5" />
                        </button>
                        {schedulePopoverId === mem.id && (
                          <div className="bg-[#272729] border border-border-primary rounded-[11px] p-3 shadow-xl absolute right-0 top-6 z-50 min-w-[200px]" onClick={e => e.stopPropagation()}>
                            <p className="text-[10px] text-text-tertiary mb-2 font-semibold">Schedule deletion</p>
                            <input
                              type="date"
                              defaultValue={mem.delete_after ?? ''}
                              min={new Date().toISOString().slice(0, 10)}
                              onChange={e => {
                                const val = e.target.value || null
                                scheduleDeleteMut.mutate({ id: mem.id, deleteAfter: val })
                              }}
                              className="w-full rounded-[8px] bg-white/[0.04] border border-border-primary text-xs text-text-primary px-2 py-1.5 focus:border-accent-blue/60 focus:outline-none [color-scheme:dark]"
                            />
                            {mem.delete_after && (
                              <button
                                onClick={() => scheduleDeleteMut.mutate({ id: mem.id, deleteAfter: null })}
                                className="mt-2 text-[10px] text-status-error hover:underline w-full text-left"
                              >
                                Clear scheduled deletion
                              </button>
                            )}
                          </div>
                        )}
                      </div>
                    )}
                  </td>
                  {/* Archive / Restore + Delete action cell */}
                  <td className="px-4 py-3.5" onClick={e => e.stopPropagation()}>
                    {isAdmin && (
                      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                        {mem.archived_at ? (
                          <button
                            onClick={() => restoreMut.mutate(mem.id)}
                            disabled={restoreMut.isPending && restoreMut.variables === mem.id}
                            aria-label={`Restore memory ${mem.id}`}
                            className="flex items-center gap-1 text-[11px] rounded-[7px] px-2 py-1 text-text-quaternary hover:text-accent-blue hover:bg-accent-blue/10 transition-colors"
                          >
                            <RotateCcw className="w-3 h-3" />
                            Restore
                          </button>
                        ) : (
                          <button
                            onClick={() => archiveMut.mutate(mem.id)}
                            disabled={archiveMut.isPending && archiveMut.variables === mem.id}
                            aria-label={`Archive memory ${mem.id}`}
                            className="flex items-center gap-1 text-[11px] rounded-[7px] px-2 py-1 text-text-quaternary hover:text-text-secondary hover:bg-[#272729] transition-colors"
                          >
                            <Archive className="w-3 h-3" />
                            Archive
                          </button>
                        )}
                        <button
                          onClick={() => deleteMut.mutate(mem.id)}
                          disabled={deleteMut.isPending && deleteMut.variables === mem.id}
                          aria-label={`Delete memory ${mem.id}`}
                          className="flex items-center gap-1 text-[11px] rounded-[7px] px-2 py-1 text-text-quaternary hover:text-status-error hover:bg-status-error/10 transition-colors"
                        >
                          <Trash2 className="w-3 h-3" />
                          Delete
                        </button>
                      </div>
                    )}
                  </td>
                </tr>
                )
              })
            }
          </tbody>
        </table>

        {!isLoading && (!memories || memories.length === 0) && (
          <div className="flex flex-col items-center gap-2 py-16 text-center">
            <Brain className="w-6 h-6 text-text-quaternary/50" />
            <p className="text-sm font-semibold text-text-secondary">
              {isSearching ? 'No results found' : 'No memories stored yet'}
            </p>
            <p className="text-xs text-text-quaternary max-w-xs">
              {isSearching
                ? 'Try adjusting your search query or switching between keyword and hybrid mode.'
                : 'Memories will appear here once the AI agent starts storing decisions, discoveries, and conventions.'}
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
          deleteError={deleteMut.isError ? ((deleteMut.error as Error)?.message ?? 'Failed to delete memory') : undefined}
        />
      )}

      <MemorySlideOver
        memoryId={selectedMemoryId}
        onClose={() => setSelectedMemoryId(null)}
        client={client}
      />

      <BulkActionBar
        count={selectedIds.size}
        onDelete={handleBulkDelete}
        onArchive={handleBulkArchive}
        onClear={clearSelection}
        deleting={bulkDeleteMut.isPending}
        archiving={bulkArchiveMut.isPending}
        tagAction={tagAction}
        setTagAction={setTagAction}
        tagInput={tagInput}
        setTagInput={setTagInput}
        onBulkTag={handleBulkTag}
        bulkTagPending={bulkTagMut.isPending}
      />
      </>}

      {/* Tags Tab */}
      {activeTab === 'tags' && (
        <div>
          {tagsLoading ? (
            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
              {Array.from({ length: 8 }).map((_, i) => (
                <div
                  key={i}
                  className="border border-border-primary rounded-[18px] p-4 animate-pulse"
                >
                  <div className="h-3.5 w-1/2 rounded bg-[#272729] mb-2" />
                  <div className="h-5 w-16 rounded bg-[#272729]" />
                </div>
              ))}
            </div>
          ) : !tagStats?.length ? (
            <div className="flex flex-col items-center gap-2 py-16 text-center border border-border-primary rounded-[18px]">
              <Tag className="w-6 h-6 text-text-quaternary/50" />
              <p className="text-sm font-semibold text-text-secondary">No tags found</p>
              <p className="text-xs text-text-quaternary max-w-xs">
                Tags will appear here once memories with tags are stored by the AI agent.
              </p>
            </div>
          ) : (
            <div className="border border-border-primary rounded-[18px] overflow-hidden">
              {tagStats.map(tag => {
                const isRenaming = renamingTag === tag.name
                const didRename = renameFlash === tag.name || renameFlash === renameValue
                return (
                  <div
                    key={tag.name}
                    className="group flex items-center justify-between px-4 py-2 border-b border-border-secondary/30 last:border-b-0 hover:bg-white/[0.02] transition-colors"
                  >
                    {isRenaming ? (
                      <div className="flex-1 flex items-center gap-2" onClick={e => e.stopPropagation()}>
                        <input
                          autoFocus
                          value={renameValue}
                          onChange={e => setRenameValue(e.target.value)}
                          onKeyDown={e => {
                            if (e.key === 'Enter') {
                              const to = renameValue.trim()
                              if (to && to !== tag.name) {
                                renameTagMut.mutate({ from: tag.name, to })
                              } else {
                                setRenamingTag(null)
                              }
                            }
                            if (e.key === 'Escape') {
                              setRenamingTag(null)
                              setRenameValue('')
                            }
                          }}
                          className="bg-white/[0.04] border border-border-primary rounded-[8px] px-2 py-0.5 text-xs text-text-primary focus:border-accent-blue/60 focus:outline-none w-full"
                          aria-label={`Rename tag ${tag.name}`}
                        />
                        <button
                          onClick={() => {
                            const to = renameValue.trim()
                            if (to && to !== tag.name) {
                              renameTagMut.mutate({ from: tag.name, to })
                            } else {
                              setRenamingTag(null)
                            }
                          }}
                          disabled={renameTagMut.isPending || !renameValue.trim()}
                          className="flex items-center gap-1 text-[11px] text-status-success hover:text-status-success/80 disabled:opacity-40 transition-colors shrink-0"
                        >
                          <Check className="w-3 h-3" />
                          Save
                        </button>
                        <button
                          onClick={() => { setRenamingTag(null); setRenameValue('') }}
                          className="flex items-center gap-1 text-[11px] text-text-quaternary hover:text-text-tertiary transition-colors shrink-0"
                        >
                          <X className="w-3 h-3" />
                          Cancel
                        </button>
                      </div>
                    ) : (
                      <>
                        <button
                          onClick={() => {
                            setActiveTab('memories')
                            setQuery(tag.name)
                          }}
                          className="flex-1 text-left flex items-center gap-3 min-w-0"
                        >
                          <span className="text-xs font-semibold text-text-primary truncate">
                            <span className="text-accent-blue/60 font-normal mr-0.5">#</span>
                            {tag.name}
                          </span>
                          <span className={`shrink-0 rounded-[5px] px-1.5 py-0.5 text-[10px] font-semibold border ${didRename ? 'bg-status-success/10 text-status-success border-status-success/20' : 'bg-accent-blue/10 text-accent-blue border-accent-blue/20'}`}>
                            {didRename ? 'Renamed' : `${tag.count} ${tag.count === 1 ? 'memory' : 'memories'}`}
                          </span>
                        </button>
                        {/* Pencil icon — only shown to admins, on hover, when not renaming */}
                        {isAdmin && (
                          <button
                            onClick={e => {
                              e.stopPropagation()
                              setRenamingTag(tag.name)
                              setRenameValue(tag.name)
                            }}
                            aria-label={`Rename tag ${tag.name}`}
                            className="ml-2 shrink-0 text-text-quaternary opacity-0 group-hover:opacity-100 hover:text-text-primary transition-opacity"
                          >
                            <Pencil className="w-3 h-3" />
                          </button>
                        )}
                      </>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </div>
      )}

      {/* Collections Tab */}
      {activeTab === 'collections' && (
        <div className="space-y-4">
          {/* Create collection form */}
          {isAdmin && (
            <div className="border border-border-primary rounded-[18px] p-5 bg-[#1d1d1f] space-y-3">
              <p className="text-xs font-semibold text-text-secondary">New collection</p>
              <div className="flex gap-2">
                <input
                  value={newCollectionName}
                  onChange={e => setNewCollectionName(e.target.value)}
                  placeholder="Collection name"
                  aria-label="Collection name"
                  className="flex-1 bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60"
                />
                <input
                  value={newCollectionDesc}
                  onChange={e => setNewCollectionDesc(e.target.value)}
                  placeholder="Description (optional)"
                  aria-label="Collection description"
                  className="flex-1 bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60"
                />
                <button
                  onClick={() => {
                    if (!newCollectionName.trim()) return
                    createCollectionMut.mutate({ name: newCollectionName.trim(), description: newCollectionDesc.trim() || undefined })
                  }}
                  disabled={createCollectionMut.isPending || !newCollectionName.trim()}
                  className="px-3 py-1.5 bg-accent-blue text-white text-xs font-semibold rounded-[8px] disabled:opacity-50 hover:bg-accent-blue/90 transition-colors"
                >
                  {createCollectionMut.isPending ? 'Creating…' : 'Create'}
                </button>
              </div>
              {collectionError && <p className="text-xs text-status-error">{collectionError}</p>}
            </div>
          )}

          {/* Collections list */}
          {collectionsLoading ? (
            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
              {Array.from({ length: 4 }).map((_, i) => (
                <div key={i} className="border border-border-primary rounded-[18px] p-4 animate-pulse">
                  <div className="h-3.5 w-1/2 rounded bg-[#272729] mb-2" />
                  <div className="h-5 w-16 rounded bg-[#272729]" />
                </div>
              ))}
            </div>
          ) : !collections?.length ? (
            <div className="flex flex-col items-center gap-2 py-16 text-center border border-border-primary rounded-[18px]">
              <Folder className="w-6 h-6 text-text-quaternary/50" />
              <p className="text-sm font-semibold text-text-secondary">No collections yet</p>
              <p className="text-xs text-text-quaternary max-w-xs">
                Create a collection to organize memories into named groups.
              </p>
            </div>
          ) : (
            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
              {collections.map((col: Collection) => (
                <div
                  key={col.id}
                  className="relative group border border-border-primary rounded-[18px] p-5 bg-[#272729] hover:border-border-focus transition-colors"
                >
                  <div className="flex items-start justify-between gap-2 mb-2">
                    <div className="flex items-center gap-2">
                      <Folder className="w-4 h-4 text-accent-blue shrink-0" />
                      <p className="text-sm font-semibold text-text-primary truncate">{col.name}</p>
                    </div>
                    <div className="flex items-center gap-1">
                      <button
                        onClick={() => exportCollection(col)}
                        aria-label="Export collection"
                        className="opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-text-primary transition-opacity"
                      >
                        <Download className="w-3.5 h-3.5" />
                      </button>
                      {isAdmin && (
                        <button
                          onClick={() => { if (window.confirm(`Delete collection "${col.name}"?`)) deleteCollectionMut.mutate(col.id) }}
                          aria-label={`Delete collection ${col.name}`}
                          className="opacity-0 group-hover:opacity-100 p-1 rounded-[5px] text-text-quaternary hover:text-status-error hover:bg-status-error/10 transition-all"
                        >
                          <Trash2 className="w-3 h-3" />
                        </button>
                      )}
                    </div>
                  </div>
                  {col.description && (
                    <p className="text-xs text-text-tertiary truncate mb-2">{col.description}</p>
                  )}
                  <div className="flex items-center justify-between">
                    <span className="text-[10px] rounded-[5px] bg-white/[0.04] text-text-quaternary border border-border-secondary/50 px-1.5 py-0.5">
                      {col.memory_count ?? 0} {(col.memory_count ?? 0) === 1 ? 'memory' : 'memories'}
                    </span>
                    <button
                      onClick={() => { setFilterCollection(col.id); setActiveTab('memories') }}
                      className="text-[11px] text-accent-blue hover:underline"
                    >
                      View
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Duplicates Tab */}
      {activeTab === 'duplicates' && (
        <div className="space-y-3">
          {duplicatesLoading ? (
            Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="border border-border-primary rounded-[18px] p-5 animate-pulse space-y-3">
                <div className="h-3.5 w-1/3 rounded bg-[#272729]" />
                <div className="h-2.5 w-2/3 rounded bg-[#272729]" />
              </div>
            ))
          ) : !duplicateGroups?.length ? (
            <div className="flex flex-col items-center gap-2 py-16 text-center border border-border-primary rounded-[18px]">
              <CheckCircle2 className="w-6 h-6 text-status-success/60" />
              <p className="text-sm font-semibold text-text-secondary">No duplicate memories found</p>
              <p className="text-xs text-text-quaternary max-w-xs">
                All memories in this organization have unique content. Nothing to clean up.
              </p>
            </div>
          ) : (
            duplicateGroups.map((group, groupIdx) => {
              const isExpanded = expandedGroups.has(groupIdx)
              // Group is already sorted newest-first from the backend; keep [0], delete the rest
              const toDelete = group.slice(1)

              const newest = group[0]

              const handleDeleteAllButNewest = async () => {
                for (const mem of toDelete) {
                  await client.deleteMemory(mem.id)
                }
                qc.invalidateQueries({ queryKey: ['memory-duplicates'] })
                qc.invalidateQueries({ queryKey: ['memories'] })
              }

              const handleMergeAllIntoNewest = async () => {
                if (!window.confirm('Merge all memories in this group into the newest one? Content will be combined.')) return
                // Merge sequentially: merge group[1] into newest, then group[2], etc.
                for (const mem of toDelete) {
                  await client.mergeMemories(newest.id, mem.id)
                }
                qc.invalidateQueries({ queryKey: ['memory-duplicates'] })
                qc.invalidateQueries({ queryKey: ['memories'] })
              }

              return (
                <div
                  key={groupIdx}
                  className="rounded-[18px] border border-status-error/20 bg-status-error/5"
                >
                  {/* Group header */}
                  <div className="flex items-center justify-between gap-3 px-5 py-4">
                    <button
                      onClick={() => setExpandedGroups(prev => {
                        const next = new Set(prev)
                        if (next.has(groupIdx)) next.delete(groupIdx)
                        else next.add(groupIdx)
                        return next
                      })}
                      className="flex items-center gap-2 text-sm font-semibold text-text-primary hover:text-text-secondary transition-colors"
                    >
                      {isExpanded
                        ? <ChevronUp className="w-4 h-4 text-text-quaternary" />
                        : <ChevronDown className="w-4 h-4 text-text-quaternary" />
                      }
                      Group of {group.length} duplicates
                    </button>
                    <div className="flex items-center gap-2">
                      <button
                        onClick={handleMergeAllIntoNewest}
                        aria-label={`Merge all into newest in duplicate group ${groupIdx + 1}`}
                        className="border border-border-primary rounded-[8px] px-2.5 py-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors"
                      >
                        Merge all into newest
                      </button>
                      <button
                        onClick={handleDeleteAllButNewest}
                        aria-label={`Delete all but newest in duplicate group ${groupIdx + 1}`}
                        className="text-xs text-status-error/60 hover:text-status-error transition-colors border border-status-error/20 rounded-full px-3 py-1 hover:bg-status-error/10"
                      >
                        Delete all but newest
                      </button>
                    </div>
                  </div>

                  {/* Expanded list */}
                  {isExpanded && (
                    <div className="border-t border-status-error/10 divide-y divide-status-error/10">
                      {group.map((mem, memIdx) => (
                        <div key={mem.id} className="flex items-start justify-between gap-3 px-5 py-3">
                          <div className="min-w-0 flex-1 space-y-0.5">
                            <div className="flex items-center gap-2 flex-wrap">
                              <span className="text-[11px] font-semibold text-text-tertiary">
                                {new Date(mem.created_at).toLocaleString()}
                              </span>
                              <span className="text-[10px] border border-border-primary rounded-[5px] px-1.5 py-0.5 text-text-quaternary bg-[#272729]/50">
                                {mem.project}
                              </span>
                              {memIdx === 0 && (
                                <span className="text-[10px] bg-status-success/10 text-status-success border border-status-success/20 px-1.5 py-0.5 rounded-[5px]">
                                  newest
                                </span>
                              )}
                            </div>
                            <p className="text-xs text-text-tertiary line-clamp-2 leading-relaxed">
                              {mem.content.slice(0, 100)}{mem.content.length > 100 ? '…' : ''}
                            </p>
                          </div>
                          <div className="flex items-center gap-2 shrink-0">
                            {group.length >= 2 && memIdx !== 0 && (
                              <>
                                <button
                                  onClick={() => mergeMut.mutate({ keepId: newest.id, mergeId: mem.id })}
                                  disabled={mergeMut.isPending}
                                  aria-label="Merge into newest"
                                  className="p-1 rounded text-text-quaternary hover:text-accent-blue hover:bg-accent-blue/10 transition-colors disabled:opacity-40"
                                >
                                  <GitMerge className="w-3.5 h-3.5" />
                                </button>
                                {mergeMut.isError && mergeMut.variables?.mergeId === mem.id && (
                                  <p className="text-xs text-status-error">
                                    {mergeMut.error instanceof Error ? mergeMut.error.message : 'Merge failed.'}
                                  </p>
                                )}
                              </>
                            )}
                            <button
                              onClick={async () => {
                                await client.deleteMemory(mem.id)
                                qc.invalidateQueries({ queryKey: ['memory-duplicates'] })
                                qc.invalidateQueries({ queryKey: ['memories'] })
                              }}
                              className="text-xs text-status-error/50 hover:text-status-error transition-colors"
                            >
                              Delete
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )
            })
          )}
        </div>
      )}

      {/* Sessions Tab */}
      {activeTab === 'sessions' && (
        <div className="space-y-3">
          {sessionsLoading ? (
            Array.from({ length: 4 }).map((_, i) => (
              <div key={i} className="border border-border-primary rounded-[18px] p-4 bg-[#272729] space-y-2 animate-pulse">
                <div className="h-3.5 w-1/3 rounded bg-[#1d1d1f]" />
                <div className="h-2.5 w-2/3 rounded bg-[#1d1d1f]" />
              </div>
            ))
          ) : !sessions?.length ? (
            <div className="flex flex-col items-center gap-2 py-16 text-center border border-border-primary rounded-[18px]">
              <Clock className="w-6 h-6 text-text-quaternary/50" />
              <p className="text-sm font-semibold text-text-secondary">No sessions yet</p>
              <p className="text-xs text-text-quaternary max-w-xs">
                Sessions are created automatically when an AI agent starts working. Each session groups a set of memories.
              </p>
            </div>
          ) : (
            sessions.map(session => {
              const isExpanded = expandedSessionId === session.id
              return (
              <div
                key={session.id}
                className="group border border-border-primary rounded-[18px] bg-[#272729] overflow-hidden transition-colors hover:border-border-focus"
              >
                {/* Card header — clickable to expand */}
                <div
                  role="button"
                  tabIndex={0}
                  aria-expanded={isExpanded}
                  aria-label={`Expand session ${session.id}`}
                  onClick={() => setExpandedSessionId(isExpanded ? null : session.id)}
                  onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setExpandedSessionId(isExpanded ? null : session.id) } }}
                  className="flex items-start justify-between gap-3 p-5 cursor-pointer select-none"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-mono text-xs text-text-tertiary truncate">
                        {session.project}
                      </span>
                      <span className="text-[10px] bg-accent-blue/10 text-accent-blue px-2 py-0.5 rounded-[5px]">
                        {session.memory_count} {session.memory_count === 1 ? 'memory' : 'memories'}
                      </span>
                      {session.ended_at ? (
                        <span className="text-[10px] bg-[#1d1d1f] text-text-quaternary border border-border-secondary px-2 py-0.5 rounded-[5px]">
                          ended
                        </span>
                      ) : (
                        <span className="text-[10px] bg-status-success/10 text-status-success border border-status-success/20 px-2 py-0.5 rounded-[5px]">
                          active
                        </span>
                      )}
                    </div>
                    {session.directory && (
                      <p className="text-[11px] text-text-quaternary font-mono mt-0.5 truncate">
                        {session.directory}
                      </p>
                    )}
                    {editingSessionId === session.id ? (
                      <div className="mt-1" onClick={e => e.stopPropagation()}>
                        <input
                          autoFocus
                          value={editSessionSummary}
                          onChange={e => setEditSessionSummary(e.target.value)}
                          onBlur={() => {
                            if (editSessionSummary.trim() !== (session.summary ?? '')) {
                              updateSessionMut.mutate({ id: session.id, summary: editSessionSummary.trim() })
                            } else {
                              setEditingSessionId(null)
                            }
                          }}
                          onKeyDown={e => {
                            if (e.key === 'Enter') {
                              updateSessionMut.mutate({ id: session.id, summary: editSessionSummary.trim() })
                            }
                            if (e.key === 'Escape') {
                              setEditingSessionId(null)
                            }
                          }}
                          className="w-full bg-[#1d1d1f] border border-accent-blue/40 rounded-[8px] px-2 py-1 text-[13px] text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue"
                          placeholder="Session summary…"
                        />
                      </div>
                    ) : (
                      <div
                        className="flex items-center gap-1.5 mt-1 group/summary"
                        onClick={e => e.stopPropagation()}
                      >
                        {session.summary ? (
                          <p className="text-[13px] text-text-tertiary line-clamp-2">{session.summary}</p>
                        ) : (
                          <p className="text-[13px] text-text-quaternary italic opacity-0 group-hover:opacity-60 transition-opacity">Add summary…</p>
                        )}
                        <button
                          onClick={e => {
                            e.stopPropagation()
                            setEditingSessionId(session.id)
                            setEditSessionSummary(session.summary ?? '')
                          }}
                          aria-label="Rename session summary"
                          className="opacity-0 group-hover/summary:opacity-100 transition-opacity text-text-quaternary hover:text-text-primary shrink-0"
                        >
                          <Pencil className="w-3 h-3" />
                        </button>
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-3 shrink-0">
                    <div className="text-right">
                      <p className="text-[11px] text-text-tertiary">
                        {new Date(session.started_at).toLocaleDateString()}
                      </p>
                      <p className="text-[10px] text-text-quaternary">
                        {new Date(session.started_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                      </p>
                    </div>
                    {/* Delete session */}
                    <div className="relative" onClick={e => e.stopPropagation()}>
                      <button
                        onClick={e => { e.stopPropagation(); setDeleteConfirmSessionId(deleteConfirmSessionId === session.id ? null : session.id) }}
                        aria-label={`Delete session ${session.id}`}
                        className="p-1 rounded-[5px] text-text-quaternary opacity-0 group-hover:opacity-100 hover:text-status-error hover:bg-status-error/10 transition-all"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                      {deleteConfirmSessionId === session.id && (
                        <div className="absolute right-0 top-7 z-30 bg-[#272729] border border-border-primary rounded-[11px] p-3 shadow-xl min-w-[180px]">
                          <p className="text-[11px] text-text-secondary mb-2">Delete this session?</p>
                          <div className="flex gap-2">
                            <button
                              onClick={() => deleteSessionMut.mutate(session.id)}
                              disabled={deleteSessionMut.isPending}
                              className="flex-1 rounded-full bg-status-error/10 border border-status-error/20 text-[11px] text-status-error hover:bg-status-error/20 py-1 transition-colors disabled:opacity-40"
                            >
                              {deleteSessionMut.isPending ? 'Deleting…' : 'Delete'}
                            </button>
                            <button
                              onClick={() => setDeleteConfirmSessionId(null)}
                              className="flex-1 rounded-full border border-border-primary text-[11px] text-text-quaternary hover:text-text-secondary py-1 transition-colors"
                            >
                              Cancel
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                    {isExpanded
                      ? <ChevronUp className="w-3.5 h-3.5 text-text-quaternary" />
                      : <ChevronDown className="w-3.5 h-3.5 text-text-quaternary" />
                    }
                  </div>
                </div>

                {/* Expanded memory list */}
                <div
                  className={`overflow-y-auto transition-all duration-200 ${
                    isExpanded ? 'max-h-[400px]' : 'max-h-0'
                  }`}
                >
                  {isExpanded && (
                    <div className="border-t border-border-secondary/50 px-5">
                      {sessionMemoriesLoading ? (
                        Array.from({ length: 3 }).map((_, i) => (
                          <div key={i} className="flex items-start gap-3 py-2.5 border-b border-border-secondary/30 last:border-b-0 animate-pulse">
                            <div className="h-4 w-16 rounded-[5px] bg-[#1d1d1f] shrink-0" />
                            <div className="flex-1 space-y-1">
                              <div className="h-3 w-full rounded bg-[#1d1d1f]" />
                              <div className="h-3 w-2/3 rounded bg-[#1d1d1f]" />
                            </div>
                          </div>
                        ))
                      ) : !sessionMemories?.length ? (
                        <p className="text-xs text-text-quaternary py-4 text-center">No memories in this session</p>
                      ) : (
                        sessionMemories.map(mem => (
                          <div key={mem.id} className="flex items-start gap-3 py-2.5 border-b border-border-secondary/20 last:border-b-0">
                            <span className="rounded-[5px] px-1.5 py-0.5 text-[10px] font-semibold bg-white/[0.04] text-text-quaternary border border-border-secondary/50 shrink-0 mt-0.5">
                              {mem.type ?? mem.tool}
                            </span>
                            <p className="text-xs text-text-secondary line-clamp-2 flex-1 leading-relaxed">
                              {mem.title ? <span className="font-semibold text-text-primary">{mem.title} — </span> : null}
                              {mem.content.replace(/#+\s/g, '').replace(/\*\*/g, '')}
                            </p>
                            <div className="flex items-center gap-2 shrink-0">
                              <span className="text-[10px] text-text-tertiary whitespace-nowrap">
                                {new Date(mem.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                              </span>
                              {isAdmin && !mem.archived_at && (
                                <button
                                  onClick={e => { e.stopPropagation(); archiveMut.mutate(mem.id) }}
                                  disabled={archiveMut.isPending && archiveMut.variables === mem.id}
                                  aria-label={`Archive memory ${mem.id}`}
                                  className="p-0.5 rounded text-text-quaternary hover:text-text-secondary hover:bg-white/[0.04] transition-colors disabled:opacity-40"
                                >
                                  <ArchiveX className="w-3 h-3" />
                                </button>
                              )}
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                  )}
                </div>
              </div>
              )
            })
          )}
        </div>
      )}

      {/* Create Memory Modal */}
      <CreateMemoryModal
        open={createMemoryOpen}
        onClose={() => setCreateMemoryOpen(false)}
        onCreated={() => qc.invalidateQueries({ queryKey: ['memories'] })}
      />
    </div>
  )
}
