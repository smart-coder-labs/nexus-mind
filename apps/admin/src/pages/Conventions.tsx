import { useState, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { BookMarked, Plus, X, Pencil, Archive, RotateCcw, Trash2, Download, Upload, Layers } from 'lucide-react'
import ReactMarkdown from 'react-markdown'
import { createClient } from '../api/client'
import type { Convention, CreateConventionRequest } from '../types'

const client = createClient()

const CONVENTION_TEMPLATES = [
  {
    label: 'Clean Architecture',
    category: 'architecture',
    weight: 200,
    title: 'Clean Architecture layers',
    content: `Strict layer separation:
- Entities (innermost) — business rules, no framework deps
- Use Cases — application business logic
- Interface Adapters — controllers, presenters, gateways
- Frameworks & Drivers (outermost) — DB, UI, web

Rules:
- Dependencies point INWARD only
- Entities know nothing about use cases
- Use cases know nothing about controllers or DB
- Test from the inside out`,
    tags: ['architecture', 'clean-arch', 'layers'],
  },
  {
    label: 'Conventional Commits',
    category: 'workflow',
    weight: 150,
    title: 'Commit message format',
    content: `Use Conventional Commits format:

feat: new feature
fix: bug fix
docs: documentation only
style: formatting, no logic change
refactor: code change that neither fixes nor adds
test: adding tests
chore: build process, tooling

Breaking changes: add ! after type (feat!:) or BREAKING CHANGE in footer.
Scope is optional: feat(auth): add OAuth support`,
    tags: ['git', 'commits', 'workflow'],
  },
  {
    label: 'API Design REST',
    category: 'architecture',
    weight: 150,
    title: 'REST API naming conventions',
    content: `Resource naming:
- Use nouns, not verbs: /users not /getUsers
- Plural: /users/:id not /user/:id
- Nested: /projects/:id/members
- Actions: POST /users/:id/deactivate

HTTP methods:
- GET: read, idempotent, no body
- POST: create, non-idempotent
- PUT: full replace
- PATCH: partial update
- DELETE: remove

Status codes: 200 OK, 201 Created, 204 No Content, 400 Bad Request, 401 Unauth, 403 Forbidden, 404 Not Found, 409 Conflict, 422 Validation, 500 Server Error`,
    tags: ['api', 'rest', 'naming'],
  },
  {
    label: 'Testing pyramid',
    category: 'testing',
    weight: 120,
    title: 'Testing strategy',
    content: `Layer proportions:
- Unit tests: 70% — fast, isolated, mock externals
- Integration tests: 20% — test boundaries, real DB
- E2E tests: 10% — happy paths only, expensive

Rules:
- Test behavior, not implementation
- One assertion per test concept
- AAA: Arrange, Act, Assert
- No logic in tests
- Tests must be deterministic`,
    tags: ['testing', 'unit', 'integration'],
  },
  {
    label: 'Database naming',
    category: 'database',
    weight: 120,
    title: 'Database schema conventions',
    content: `Tables: snake_case, plural (users, api_keys, not ApiKey)
Columns: snake_case (created_at, user_id)
Primary keys: always "id" (not user_id on users table)
Foreign keys: {table_singular}_id (user_id, org_id)
Timestamps: created_at, updated_at (ISO text in SQLite)
Soft delete: deleted_at or archived_at (nullable text)
Booleans: store as INTEGER 0/1 in SQLite
JSON: store as TEXT, validate in application layer`,
    tags: ['database', 'naming', 'schema'],
  },
]

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

const CATEGORIES = [
  { value: 'all',           label: 'All' },
  { value: 'architecture',  label: 'Architecture' },
  { value: 'design-system', label: 'Design System' },
  { value: 'database',      label: 'Database' },
  { value: 'code-style',    label: 'Code Style' },
  { value: 'workflow',      label: 'Workflow' },
  { value: 'testing',       label: 'Testing' },
  { value: 'security',      label: 'Security' },
  { value: 'general',       label: 'General' },
]

// ── New/Edit Convention Modal ─────────────────────────────────────────────────

interface ConventionModalProps {
  open: boolean
  onClose: () => void
  onSave: (data: CreateConventionRequest) => Promise<void>
  saving: boolean
}

function ConventionModal({ open, onClose, onSave, saving }: ConventionModalProps) {
  const [title, setTitle] = useState('')
  const [content, setContent] = useState('')
  const [category, setCategory] = useState('general')
  const [weight, setWeight] = useState('100')
  const [tagsRaw, setTagsRaw] = useState('')
  const [templatesOpen, setTemplatesOpen] = useState(false)

  if (!open) return null

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const tags = tagsRaw.split(',').map(t => t.trim()).filter(Boolean)
    await onSave({ title, content, category, weight: Number(weight) || 100, tags })
    // Reset form after successful save
    setTitle(''); setContent(''); setCategory('general'); setWeight('100'); setTagsRaw('')
  }

  const applyTemplate = (tpl: typeof CONVENTION_TEMPLATES[number]) => {
    setTitle(tpl.title)
    setContent(tpl.content)
    setCategory(tpl.category)
    setWeight(String(tpl.weight))
    setTagsRaw(tpl.tags.join(', '))
    setTemplatesOpen(false)
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={onClose}
    >
      <div
        className="bg-[#272729] rounded-[18px] border border-border-primary p-6 max-w-lg w-full shadow-2xl mx-4 max-h-[90vh] overflow-y-auto"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-sm font-semibold text-text-primary">
            New Convention
          </h2>
          <div className="flex items-center gap-2">
            <div className="relative">
              <button
                type="button"
                onClick={() => setTemplatesOpen(v => !v)}
                className="border border-border-primary rounded-full px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
              >
                <Layers className="w-3 h-3" />
                Templates
              </button>
              {templatesOpen && (
                <div className="absolute right-0 top-full mt-1 bg-[#272729] border border-border-primary rounded-[11px] py-1 shadow-xl z-50 min-w-[200px]">
                  {CONVENTION_TEMPLATES.map(tpl => (
                    <button
                      key={tpl.label}
                      type="button"
                      onClick={() => applyTemplate(tpl)}
                      className="w-full text-left px-3 py-2 text-xs text-text-secondary hover:bg-white/[0.04] cursor-pointer"
                    >
                      {tpl.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <button onClick={onClose} className="text-text-quaternary hover:text-text-secondary transition-colors">
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div>
            <label className="block text-xs text-text-tertiary mb-1.5">Title</label>
            <input
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60"
              value={title}
              onChange={e => setTitle(e.target.value)}
              placeholder="Use snake_case for variable names"
              required
            />
          </div>

          <div>
            <label className="block text-xs text-text-tertiary mb-1.5">Category</label>
            <select
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 focus:outline-none focus:border-accent-blue/60"
              value={category}
              onChange={e => setCategory(e.target.value)}
            >
              {CATEGORIES.filter(c => c.value !== 'all').map(c => (
                <option key={c.value} value={c.value}>{c.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-xs text-text-tertiary mb-1.5">
              Weight <span className="text-text-quaternary">(higher = more priority)</span>
            </label>
            <input
              type="number"
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 focus:outline-none focus:border-accent-blue/60"
              value={weight}
              onChange={e => setWeight(e.target.value)}
              min={1}
              max={10000}
            />
          </div>

          <div>
            <label className="block text-xs text-text-tertiary mb-1.5">Content</label>
            <textarea
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 resize-y min-h-[120px]"
              value={content}
              onChange={e => setContent(e.target.value)}
              placeholder="Describe the convention in detail. Agents will receive this as an authoritative rule."
              required
            />
          </div>

          <div>
            <label className="block text-xs text-text-tertiary mb-1.5">
              Tags <span className="text-text-quaternary">(comma-separated)</span>
            </label>
            <input
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60"
              value={tagsRaw}
              onChange={e => setTagsRaw(e.target.value)}
              placeholder="frontend, typescript, naming"
            />
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
              disabled={saving}
              className="px-4 py-2 rounded-full bg-accent-blue text-white text-xs font-semibold hover:bg-accent-blue/90 disabled:opacity-50 transition-colors"
            >
              {saving ? 'Saving…' : 'Create convention'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Convention Card ───────────────────────────────────────────────────────────

function ConventionCard({
  conv,
  onArchive,
  onRestore,
  onDelete,
  onSaved,
}: {
  conv: Convention
  onArchive: (id: number) => void
  onRestore: (id: number) => void
  onDelete: (id: number) => void
  onSaved: () => void
}) {
  const isArchived = !!conv.archived_at
  const [viewMode, setViewMode] = useState<'raw' | 'preview'>('raw')
  const [editing, setEditing] = useState(false)
  const [saving, setSaving] = useState(false)

  // Edit state
  const [editTitle, setEditTitle] = useState(conv.title ?? '')
  const [editContent, setEditContent] = useState(conv.content)
  const [editCategory, setEditCategory] = useState(conv.category ?? 'general')
  const [editWeight, setEditWeight] = useState(String(conv.weight ?? 100))

  const enterEdit = () => {
    setEditTitle(conv.title ?? '')
    setEditContent(conv.content)
    setEditCategory(conv.category ?? 'general')
    setEditWeight(String(conv.weight ?? 100))
    setEditing(true)
  }

  const cancelEdit = () => setEditing(false)

  const handleSave = async () => {
    setSaving(true)
    try {
      await client.updateConvention(conv.id, {
        title: editTitle,
        content: editContent,
        category: editCategory,
        weight: Number(editWeight) || 100,
      })
      onSaved()
      setEditing(false)
    } finally {
      setSaving(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') cancelEdit()
  }

  return (
    <div
      className={`bg-[#272729] rounded-[18px] border border-border-primary p-5 group ${isArchived ? 'opacity-60' : ''}`}
      onKeyDown={handleKeyDown}
    >
      <div className="flex items-start justify-between mb-2 gap-3">
        {editing ? (
          <input
            autoFocus
            value={editTitle}
            onChange={e => setEditTitle(e.target.value)}
            className="bg-transparent border-b border-accent-blue/60 text-sm text-text-primary font-semibold focus:outline-none w-full"
            placeholder="Convention title"
          />
        ) : (
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm text-text-primary font-semibold">{conv.title}</span>
            <span className="text-[10px] bg-white/[0.06] text-text-quaternary rounded-[5px] px-1.5 py-0.5 capitalize">
              {conv.category}
            </span>
            {conv.weight > 100 && (
              <span className="text-[10px] bg-accent-blue/10 text-accent-blue rounded-[5px] px-1.5 py-0.5">
                weight {conv.weight}
              </span>
            )}
            {isArchived && (
              <span className="text-[10px] bg-white/[0.04] text-text-quaternary rounded-[5px] px-1.5 py-0.5">
                archived
              </span>
            )}
          </div>
        )}
        <div className="flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
          {!isArchived && !editing && (
            <button
              onClick={enterEdit}
              className="p-1.5 rounded-[6px] text-text-quaternary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
              title="Edit (E)"
            >
              <Pencil className="w-3.5 h-3.5" />
            </button>
          )}
          {!editing && (
            <>
              {!isArchived ? (
                <button
                  onClick={() => onArchive(conv.id)}
                  className="p-1.5 rounded-[6px] text-text-quaternary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
                  title="Archive"
                >
                  <Archive className="w-3.5 h-3.5" />
                </button>
              ) : (
                <button
                  onClick={() => onRestore(conv.id)}
                  className="p-1.5 rounded-[6px] text-text-quaternary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
                  title="Restore"
                >
                  <RotateCcw className="w-3.5 h-3.5" />
                </button>
              )}
              <button
                onClick={() => onDelete(conv.id)}
                className="p-1.5 rounded-[6px] text-text-quaternary hover:text-status-error hover:bg-status-error/10 transition-colors"
                title="Delete"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            </>
          )}
        </div>
      </div>

      {editing ? (
        <>
          {/* Category + Weight row */}
          <div className="flex items-center gap-2 mb-2">
            <select
              value={editCategory}
              onChange={e => setEditCategory(e.target.value)}
              className="rounded-[5px] border border-border-primary bg-white/[0.04] text-[10px] text-text-secondary px-1.5 py-1 focus:outline-none"
            >
              {CATEGORIES.filter(c => c.value !== 'all').map(c => (
                <option key={c.value} value={c.value}>{c.label}</option>
              ))}
            </select>
            <label className="text-[10px] text-text-quaternary">Weight</label>
            <input
              type="number"
              value={editWeight}
              onChange={e => setEditWeight(e.target.value)}
              min={1}
              max={10000}
              className="rounded-[5px] border border-border-primary bg-white/[0.04] text-[10px] text-text-secondary px-1.5 py-1 focus:outline-none w-16"
            />
          </div>

          {/* Content textarea */}
          <textarea
            value={editContent}
            onChange={e => setEditContent(e.target.value)}
            className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary p-2.5 focus:outline-none focus:border-accent-blue/60 resize-y min-h-[80px] w-full mt-2"
          />

          {/* Save / Cancel */}
          <div className="flex items-center gap-2 mt-3">
            <button
              onClick={handleSave}
              disabled={saving}
              className="bg-accent-blue text-white rounded-full px-3 py-1.5 text-xs font-semibold hover:bg-accent-blue/90 disabled:opacity-50 transition-colors"
            >
              {saving ? 'Saving…' : 'Save'}
            </button>
            <button
              onClick={cancelEdit}
              className="rounded-full px-3 py-1.5 text-xs text-text-secondary border border-border-primary hover:text-text-primary hover:bg-white/[0.04] transition-colors"
            >
              Cancel
            </button>
          </div>
        </>
      ) : (
        <>
          {/* Raw / Preview toggle */}
          <div className="bg-white/[0.04] rounded-full p-0.5 flex items-center mb-2 w-fit">
            <button
              onClick={() => setViewMode('raw')}
              className={`text-[10px] px-2 py-0.5 rounded-full transition-colors ${
                viewMode === 'raw'
                  ? 'bg-[#272729] text-text-primary font-semibold'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              Raw
            </button>
            <button
              onClick={() => setViewMode('preview')}
              className={`text-[10px] px-2 py-0.5 rounded-full transition-colors ${
                viewMode === 'preview'
                  ? 'bg-[#272729] text-text-primary font-semibold'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              Preview
            </button>
          </div>

          {viewMode === 'raw' ? (
            <p className="text-xs text-text-secondary whitespace-pre-wrap">{conv.content}</p>
          ) : (
            <div className="text-xs text-text-secondary prose-convention">
              <ReactMarkdown
                components={{
                  h1: ({ children }) => <h3 className="text-sm text-text-primary font-semibold mt-2 mb-1 first:mt-0">{children}</h3>,
                  h2: ({ children }) => <h3 className="text-sm text-text-primary font-semibold mt-2 mb-1 first:mt-0">{children}</h3>,
                  h3: ({ children }) => <h3 className="text-sm text-text-primary font-semibold mt-2 mb-1 first:mt-0">{children}</h3>,
                  p: ({ children }) => <p className="text-xs text-text-secondary mb-2 last:mb-0">{children}</p>,
                  ul: ({ children }) => <ul className="mb-2 space-y-0.5 list-none last:mb-0">{children}</ul>,
                  ol: ({ children }) => <ol className="mb-2 ml-4 space-y-0.5 list-decimal last:mb-0">{children}</ol>,
                  li: ({ children }) => (
                    <li className="text-xs text-text-secondary ml-3 flex gap-1.5">
                      <span className="text-text-quaternary shrink-0 mt-1">•</span>
                      <span>{children}</span>
                    </li>
                  ),
                  strong: ({ children }) => <strong className="font-semibold text-text-primary">{children}</strong>,
                  em: ({ children }) => <em className="italic text-text-tertiary">{children}</em>,
                  code: ({ children, className }) => {
                    const isBlock = className?.startsWith('language-')
                    if (isBlock) return <code className="block text-[11px] font-mono text-text-secondary leading-relaxed">{children}</code>
                    return <code className="bg-white/[0.06] rounded px-1 text-accent-blue font-mono text-[11px]">{children}</code>
                  },
                  pre: ({ children }) => (
                    <pre className="bg-white/[0.06] rounded-[8px] p-3 text-[11px] font-mono text-text-secondary overflow-x-auto mb-2 last:mb-0">
                      {children}
                    </pre>
                  ),
                }}
              >
                {conv.content}
              </ReactMarkdown>
            </div>
          )}

          {conv.tags.length > 0 && (
            <div className="flex gap-1.5 mt-3 flex-wrap">
              {conv.tags.map(t => (
                <span key={t} className="bg-white/[0.06] rounded-full px-2 py-0.5 text-[10px] text-text-quaternary">
                  {t}
                </span>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  )
}

// ── Main Page ─────────────────────────────────────────────────────────────────

export default function Conventions() {
  const [selectedCategory, setSelectedCategory] = useState('all')
  const [showArchived, setShowArchived] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [saving, setSaving] = useState(false)
  const [search, setSearch] = useState('')
  const [sortBy, setSortBy] = useState<'weight' | 'recent'>('weight')
  const [importProgress, setImportProgress] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const qc = useQueryClient()

  const { data: conventions = [], isLoading } = useQuery({
    queryKey: ['conventions', showArchived],
    queryFn: () => client.listConventions(undefined, showArchived),
  })

  const filtered = conventions
    .filter(c => selectedCategory === 'all' || c.category === selectedCategory)
    .filter(c => {
      if (!search) return true
      const q = search.toLowerCase()
      return (c.title ?? '').toLowerCase().includes(q) || c.content.toLowerCase().includes(q)
    })
    .sort((a, b) => {
      if (sortBy === 'recent') {
        return new Date(b.created_at ?? 0).getTime() - new Date(a.created_at ?? 0).getTime()
      }
      return (b.weight ?? 0) - (a.weight ?? 0)
    })

  const createMut = useMutation({
    mutationFn: (data: CreateConventionRequest) => client.createConvention(data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['conventions'] }); setModalOpen(false) },
  })

  const archiveMut = useMutation({
    mutationFn: (id: number) => client.archiveConvention(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['conventions'] }),
  })

  const restoreMut = useMutation({
    mutationFn: (id: number) => client.restoreConvention(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['conventions'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (id: number) => client.deleteConvention(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['conventions'] }),
  })

  const handleExport = async () => {
    const all = await client.listConventions(undefined, true)
    const payload = {
      exported_at: new Date().toISOString(),
      conventions: all.map(({ id, title, content, category, weight, tags }) => ({
        id, title, content, category, weight, tags,
      })),
    }
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' })
    downloadBlob(blob, 'conventions-export.json')
  }

  const handleImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    e.target.value = ''

    let parsed: { conventions?: CreateConventionRequest[] }
    try {
      parsed = JSON.parse(await file.text())
    } catch {
      alert('Invalid JSON file.')
      return
    }

    const items = parsed.conventions ?? []
    if (!Array.isArray(items) || items.length === 0) {
      alert('No conventions found in file.')
      return
    }

    let imported = 0
    for (const conv of items) {
      setImportProgress(`Imported ${imported}/${items.length} conventions…`)
      await client.createConvention({
        title:    conv.title,
        content:  conv.content,
        category: conv.category,
        weight:   conv.weight,
        tags:     conv.tags,
      })
      imported++
    }

    setImportProgress(null)
    qc.invalidateQueries({ queryKey: ['conventions'] })
  }

  const handleSave = async (data: CreateConventionRequest) => {
    setSaving(true)
    try {
      await createMut.mutateAsync(data as CreateConventionRequest)
    } finally {
      setSaving(false)
    }
  }

  const handleNewConvention = () => {
    setModalOpen(true)
  }

  const handleDelete = (id: number) => {
    if (confirm('Delete this convention permanently?')) {
      deleteMut.mutate(id)
    }
  }

  return (
    <div className="p-6 max-w-5xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-2.5">
          <BookMarked className="w-4 h-4 text-accent-blue" />
          <h1 className="text-base font-semibold text-text-primary">Conventions</h1>
          <span className="text-xs text-text-quaternary">
            Team-wide rules that agents must follow
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleExport}
            className="border border-border-primary rounded-full px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
          >
            <Download className="w-3 h-3" />
            Export
          </button>
          <button
            onClick={() => fileInputRef.current?.click()}
            className="border border-border-primary rounded-full px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
          >
            <Upload className="w-3 h-3" />
            {importProgress
              ? <span className="text-[10px] text-text-quaternary">{importProgress}</span>
              : 'Import'}
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".json"
            className="hidden"
            onChange={handleImportFile}
          />
          <button
            onClick={() => setShowArchived(v => !v)}
            className={`border border-border-primary rounded-full px-2.5 py-1 text-xs transition-colors ${
              showArchived
                ? 'text-text-primary bg-white/[0.06]'
                : 'text-text-quaternary hover:text-text-secondary'
            }`}
          >
            {showArchived ? 'Hide archived' : 'Show archived'}
          </button>
          <button
            onClick={handleNewConvention}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:bg-accent-blue/90 transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            New convention
          </button>
        </div>
      </div>

      <div className="flex gap-5">
        {/* Category sidebar */}
        <div className="w-44 flex-shrink-0">
          <div className="flex flex-col gap-0.5">
            {CATEGORIES.map(cat => (
              <button
                key={cat.value}
                onClick={() => setSelectedCategory(cat.value)}
                className={`text-left px-3 py-1.5 rounded-full text-xs transition-colors ${
                  selectedCategory === cat.value
                    ? 'bg-[#272729] text-text-primary font-semibold'
                    : 'text-text-secondary hover:text-text-primary'
                }`}
              >
                {cat.label}
                <span className="ml-1.5 text-[10px] text-text-quaternary font-normal">
                  {cat.value === 'all'
                    ? conventions.length || ''
                    : conventions.filter(c => c.category === cat.value && (showArchived || !c.archived_at)).length || ''}
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* Convention list */}
        <div className="flex-1 flex flex-col gap-3">
          {/* Search + Sort */}
          <div className="flex items-center gap-2 mb-1">
            <input
              placeholder="Search conventions..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 flex-1 placeholder:text-text-quaternary"
            />
            <div className="bg-white/[0.04] rounded-full p-0.5 flex shrink-0">
              <button
                onClick={() => setSortBy('weight')}
                className={sortBy === 'weight'
                  ? 'bg-[#272729] text-text-primary font-semibold rounded-full px-3 py-1 text-xs shadow-sm'
                  : 'text-text-quaternary px-3 py-1 text-xs rounded-full hover:text-text-secondary transition-colors'}
              >
                Weight ↓
              </button>
              <button
                onClick={() => setSortBy('recent')}
                className={sortBy === 'recent'
                  ? 'bg-[#272729] text-text-primary font-semibold rounded-full px-3 py-1 text-xs shadow-sm'
                  : 'text-text-quaternary px-3 py-1 text-xs rounded-full hover:text-text-secondary transition-colors'}
              >
                Recent
              </button>
            </div>
          </div>
          {isLoading && (
            <div className="animate-pulse h-24 bg-[#272729] rounded-[18px]" />
          )}
          {!isLoading && filtered.length === 0 && (
            <div className="flex flex-col items-center justify-center py-16 text-center">
              <BookMarked className="w-8 h-8 text-text-quaternary mb-3" />
              <p className="text-sm text-text-secondary font-semibold">No conventions yet</p>
              <p className="text-xs text-text-quaternary mt-1">
                Create your first convention to define team-wide rules for agents.
              </p>
            </div>
          )}
          {filtered.map(conv => (
            <ConventionCard
              key={conv.id}
              conv={conv}
              onArchive={id => archiveMut.mutate(id)}
              onRestore={id => restoreMut.mutate(id)}
              onDelete={handleDelete}
              onSaved={() => qc.invalidateQueries({ queryKey: ['conventions'] })}
            />
          ))}
        </div>
      </div>

      <ConventionModal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        onSave={handleSave}
        saving={saving}
      />
    </div>
  )
}
