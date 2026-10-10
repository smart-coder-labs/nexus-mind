import { Modal } from '../components/ui/Modal/Modal'
import { Button } from '../components/ui/Button'
import { useState, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { Collection, Memory } from '../types'
import { FolderOpen, Pencil, Trash2, X, Plus, Search, Layers, TrendingUp, Clock } from 'lucide-react'
import { StatTile } from './dashboard/StatTile'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { accentFor as metricAccentFor } from './dashboard/colors'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Collections page.
const GLASS_PANEL = 'border border-border-primary bg-surface-primary '

// Fixed-order accent cycle reusing the app's existing token set (no new
// colors introduced) so each collection card gets a stable tinted initial
// avatar + glow, matching the mockup's per-card palette.
const CARD_ACCENTS = [
  { bg: 'bg-status-warning/10', text: 'text-status-warning', glow: 'bg-status-warning/10' },
  { bg: 'bg-action-primary/10', text: 'text-accent-blue', glow: 'bg-action-primary/10' },
  { bg: 'bg-status-success/10', text: 'text-status-success', glow: 'bg-status-success/10' },
  { bg: 'bg-accent-purple/10', text: 'text-accent-purple', glow: 'bg-accent-purple/10' },
  { bg: 'bg-status-error/10', text: 'text-status-error', glow: 'bg-status-error/10' },
] as const

function accentFor(index: number) {
  return CARD_ACCENTS[index % CARD_ACCENTS.length]
}

function relativeDate(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const days = Math.floor(diff / 86_400_000)
  if (days <= 0) return 'today'
  if (days === 1) return '1 day ago'
  if (days < 30) return `${days} days ago`
  return new Date(iso).toLocaleDateString()
}

function CollectionIcon() {
  return (
    <svg
      className="w-10 h-10 text-text-quaternary"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={1.5}
      aria-hidden="true"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
      />
    </svg>
  )
}

interface CreateCollectionModalProps {
  onClose: () => void
  onCreated: () => void
}

function CreateCollectionModal({ onClose, onCreated }: CreateCollectionModalProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { setError('Name is required'); return }
    setError(null)
    setSubmitting(true)
    try {
      await client.createCollection({ name: name.trim(), description: description.trim() || undefined })
      onCreated()
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create collection')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal open onOpenChange={open => { if (!open) onClose() }} ariaLabel="New collection" size="md">
      <div className="min-w-0">
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-lg font-semibold text-text-primary">New collection</h2>
          <button
            onClick={onClose}
            className="text-text-quaternary hover:text-text-secondary transition-colors"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label htmlFor="create-collection-name" className="text-xs text-text-tertiary">Name</label>
            <input
              id="create-collection-name"
              type="text"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="My collection"
              autoFocus
              className="w-full rounded-md border border-input bg-foreground/[0.04] text-base md:text-sm text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
            />
          </div>

          <div className="space-y-1.5">
            <label htmlFor="create-collection-description" className="text-xs text-text-tertiary">
              Description <span className="text-text-quaternary">(optional)</span>
            </label>
            <textarea
              id="create-collection-description"
              value={description}
              onChange={e => setDescription(e.target.value)}
              placeholder="What this collection is for…"
              rows={3}
              className="w-full rounded-md border border-input bg-foreground/[0.04] text-base md:text-sm text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary resize-none shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
            />
          </div>

          {error && (
            <p className="text-xs text-status-error">{error}</p>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="border border-border-primary rounded-md px-4 py-1.5 text-xs text-text-secondary hover:bg-foreground/[0.04] transition-colors"
            >
              Cancel
            </button>
            <Button
              type="submit"
              disabled={submitting}

            >
              {submitting ? 'Creating…' : 'Create'}
            </Button>
          </div>
        </form>
      </div>
    </Modal>
  )
}

interface RenameModalProps {
  collection: Collection
  onClose: () => void
  onRenamed: () => void
}

function RenameModal({ collection, onClose, onRenamed }: RenameModalProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [name, setName] = useState(collection.name)
  const [description, setDescription] = useState(collection.description ?? '')
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { setError('Name is required'); return }
    setError(null)
    setSubmitting(true)
    try {
      await client.createCollection({ name: name.trim(), description: description.trim() || undefined })
      await client.deleteCollection(collection.id)
      onRenamed()
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to rename collection')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal open onOpenChange={open => { if (!open) onClose() }} ariaLabel="Rename collection" size="md">
      <div className="min-w-0">
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-lg font-semibold text-text-primary">Rename collection</h2>
          <button onClick={onClose} className="text-text-quaternary hover:text-text-secondary transition-colors" aria-label="Close">
            <X className="w-4 h-4" />
          </button>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label htmlFor="rename-collection-name" className="text-xs text-text-tertiary">Name</label>
            <input
              id="rename-collection-name"
              type="text"
              value={name}
              onChange={e => setName(e.target.value)}
              autoFocus
              className="w-full rounded-md border border-input bg-foreground/[0.04] text-base md:text-sm text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
            />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="rename-collection-description" className="text-xs text-text-tertiary">Description</label>
            <textarea
              id="rename-collection-description"
              value={description}
              onChange={e => setDescription(e.target.value)}
              rows={2}
              className="w-full rounded-md border border-input bg-foreground/[0.04] text-base md:text-sm text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 resize-none shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
            />
          </div>
          {error && <p className="text-xs text-status-error">{error}</p>}
          <div className="flex justify-end gap-2 pt-1">
            <button type="button" onClick={onClose} className="border border-border-primary rounded-md px-4 py-1.5 text-xs text-text-secondary hover:bg-foreground/[0.04] transition-colors">
              Cancel
            </button>
            <Button type="submit" disabled={submitting} >
              {submitting ? 'Saving…' : 'Save'}
            </Button>
          </div>
        </form>
      </div>
    </Modal>
  )
}

interface CollectionMemoriesProps {
  collection: Collection
  onClose: () => void
}

function CollectionMemories({ collection, onClose }: CollectionMemoriesProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const [searchQuery, setSearchQuery] = useState('')
  const [adding, setAdding] = useState(false)
  const [addError, setAddError] = useState<string | null>(null)

  const { data: searchResults, isLoading: searching } = useQuery({
    queryKey: ['memory-search-assign', searchQuery],
    queryFn: () => searchQuery.trim()
      ? client.searchMemories(searchQuery.trim(), 10, 'keyword')
      : client.listMemories({ limit: 10 } as Parameters<typeof client.listMemories>[0]),
    enabled: true,
    staleTime: 5000,
  })

  const handleAssign = async (memory: Memory) => {
    setAdding(true)
    setAddError(null)
    try {
      await client.assignMemoryToCollection(memory.id, { collection_id: collection.id })
      qc.invalidateQueries({ queryKey: ['collections'] })
    } catch (err) {
      setAddError(err instanceof Error ? err.message : 'Failed to assign memory')
    } finally {
      setAdding(false)
    }
  }

  return (
    <Modal open onOpenChange={open => { if (!open) onClose() }} ariaLabel="Collection memories" size="lg">
      <div className="min-w-0">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-xs font-semibold text-text-primary">{collection.name}</h2>
            {collection.description && (
              <p className="text-xs text-text-quaternary mt-0.5">{collection.description}</p>
            )}
          </div>
          <button onClick={onClose} className="text-text-quaternary hover:text-text-secondary transition-colors" aria-label="Close">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-3">
          <p className="text-xs text-text-secondary">Add memories to this collection</p>

          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-quaternary" />
            <input
              type="text"
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              aria-label="Search collection memories"
              placeholder="Search memories…"
              className="w-full rounded-md border border-input bg-foreground/[0.04] text-base md:text-sm text-text-primary pl-8 pr-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
            />
          </div>

          {addError && <p className="text-xs text-status-error">{addError}</p>}

          <div className="max-h-64 overflow-y-auto space-y-1">
            {searching && (
              <div className="space-y-2">
                {[1, 2, 3].map(i => (
                  <div key={i} className="animate-pulse h-10 bg-foreground/[0.04] rounded-md" />
                ))}
              </div>
            )}
            {!searching && (!searchResults || searchResults.length === 0) && (
              <p className="text-xs text-text-quaternary text-center py-4">No memories found</p>
            )}
            {searchResults?.map(memory => (
              <div
                key={memory.id}
                className="flex items-center gap-2 p-3 rounded-md bg-foreground/[0.04]"
              >
                <p className="text-xs text-text-secondary flex-1 truncate">{memory.content}</p>
                <button
                  onClick={() => handleAssign(memory)}
                  disabled={adding}
                  className="flex-shrink-0 flex items-center gap-1 text-xs px-2 py-1 rounded-sm border border-accent-blue/30 text-accent-blue hover:bg-action-primary/10 transition-colors disabled:opacity-50"
                >
                  <Plus className="w-3 h-3" />
                  Add
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>
    </Modal>
  )
}

interface CollectionCardProps {
  collection: Collection
  accentIndex: number
  onDeleted: () => void
  onRenamed: () => void
}

function CollectionCard({ collection, accentIndex, onDeleted, onRenamed }: CollectionCardProps) {
  const [showRename, setShowRename] = useState(false)
  const [showMemories, setShowMemories] = useState(false)
  const accent = accentFor(accentIndex)
  const initial = collection.name.trim().charAt(0).toUpperCase() || '#'

  return (
    <>
      <div
        onClick={() => setShowMemories(true)}
        className="group relative flex flex-col gap-3 rounded-xl border border-border-primary bg-surface-primary p-5 overflow-hidden cursor-pointer transition-colors hover:border-border-tertiary"
      >
        <div className="flex items-start gap-3 relative">
          <div className={`w-9 h-9 rounded-md flex items-center justify-center flex-shrink-0 text-sm font-bold ${accent.bg} ${accent.text}`}>
            {initial}
          </div>
          <div className="flex flex-col gap-0.5 flex-1 min-w-0">
            <span className="text-sm font-semibold text-text-primary truncate">{collection.name}</span>
            {/* Owner/contributor is not tracked on Collection — omitted rather
                than fabricated. Only the creation date is real data. */}
            <span className="text-xs text-text-quaternary">{relativeDate(collection.created_at)}</span>
          </div>
          {collection.memory_count != null && (
            <span className="flex-shrink-0 inline-flex items-center gap-1 text-[12px] font-medium px-2 py-0.5 rounded-full bg-foreground/[0.05] text-text-secondary">
              {collection.memory_count}
            </span>
          )}
          <div className="flex items-center gap-1 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100 transition-opacity flex-shrink-0">
            <button
              onClick={e => { e.stopPropagation(); setShowRename(true) }}
              className="p-1 rounded-sm text-text-quaternary hover:text-text-secondary hover:bg-foreground/[0.06] transition-colors"
              aria-label="Rename"
            >
              <Pencil className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={e => { e.stopPropagation(); onDeleted() }}
              className="p-1 rounded-sm text-text-quaternary hover:text-status-error hover:bg-status-error/10 transition-colors"
              aria-label="Delete"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {collection.description && (
          <p className="text-xs text-text-quaternary line-clamp-2 min-h-[32px] relative">{collection.description}</p>
        )}
        {/* Per-project chip row from the mockup isn't rendered: Collection
            has no linked-projects field in the API — nothing real to show. */}
      </div>

      {showRename && (
        <RenameModal
          collection={collection}
          onClose={() => setShowRename(false)}
          onRenamed={onRenamed}
        />
      )}

      {showMemories && (
        <CollectionMemories
          collection={collection}
          onClose={() => setShowMemories(false)}
        />
      )}
    </>
  )
}

export default function Collections() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const [showCreate, setShowCreate] = useState(false)

  const { data: collections, isLoading } = useQuery<Collection[]>({
    queryKey: ['collections'],
    queryFn: () => client.listCollections(),
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteCollection(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['collections'] }),
  })

  const handleDelete = (collection: Collection) => {
    if (!window.confirm(`Delete collection "${collection.name}"? This cannot be undone.`)) return
    deleteMut.mutate(collection.id)
  }

  // Stat tiles derived entirely from the already-fetched collections list —
  // no extra queries, no fabricated numbers. "Contributors" from the
  // mockup is omitted: Collection has no owner/contributor field in the API.
  const stats = useMemo(() => {
    if (!collections || collections.length === 0) return null
    const withCounts = collections.filter((c): c is Collection & { memory_count: number } => c.memory_count != null)
    const totalMemories = withCounts.reduce((sum, c) => sum + c.memory_count, 0)
    const largest = withCounts.length
      ? withCounts.reduce((a, b) => (b.memory_count > a.memory_count ? b : a))
      : null
    const newest = [...collections].sort(
      (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
    )[0]
    return { total: collections.length, totalMemories, hasCounts: withCounts.length > 0, largest, newest }
  }, [collections])

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-6">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 rounded-xl bg-status-warning/10 flex items-center justify-center flex-shrink-0">
            <FolderOpen className="w-[22px] h-[22px] text-status-warning" />
          </div>
          <div>
            <h1 className="text-[22px] font-semibold tracking-[-0.02em] text-text-primary">Collections</h1>
            <p className="mt-0.5 text-sm text-text-tertiary">
              Group memories into named collections for easier organization and filtering.
            </p>
          </div>
        </div>
        <Button
          onClick={() => setShowCreate(true)}
          className="flex-shrink-0"
        >
          <Plus className="w-4 h-4" />
          New collection
        </Button>
      </div>

      {stats && (
        <KpiMarquee compact role="list" aria-label="Collection statistics">
          <div key="collections" className="w-[232px] flex-none">
            <StatTile label="Collections" value={String(stats.total)} icon={FolderOpen} accent={metricAccentFor(0)} />
          </div>
          <div key="memories-curated" className="w-[232px] flex-none">
            <StatTile
              label="Memories curated"
              value={stats.hasCounts ? stats.totalMemories.toLocaleString() : '—'}
              sub={stats.hasCounts ? 'across all collections' : undefined}
              icon={Layers}
              accent={metricAccentFor(1)}
            />
          </div>
          <div key="largest" className="w-[232px] flex-none">
            <StatTile
              label="Largest"
              value={stats.largest ? stats.largest.name : '—'}
              sub={stats.largest ? `${stats.largest.memory_count} memories` : undefined}
              icon={TrendingUp}
              accent={metricAccentFor(2)}
            />
          </div>
          <div key="newest" className="w-[232px] flex-none">
            <StatTile
              label="Newest"
              value={stats.newest ? stats.newest.name : '—'}
              sub={stats.newest ? relativeDate(stats.newest.created_at) : undefined}
              icon={Clock}
              accent={metricAccentFor(3)}
            />
          </div>
        </KpiMarquee>
      )}

      {isLoading && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {[1, 2, 3, 4, 5, 6].map(i => (
            <div key={i} className={`animate-pulse rounded-xl p-5 h-28 ${GLASS_PANEL}`} />
          ))}
        </div>
      )}

      {!isLoading && (!collections || collections.length === 0) && (
        <div className="flex flex-col items-center gap-4 py-20">
          <CollectionIcon />
          <div className="text-center">
            <p className="text-sm font-semibold text-text-primary">No collections yet</p>
            <p className="text-xs text-text-quaternary mt-1">
              Create a collection to group related memories together.
            </p>
          </div>
          <button
            onClick={() => setShowCreate(true)}
            className="text-xs px-4 py-2 rounded-md border border-border-primary text-text-secondary hover:text-text-primary transition-colors"
          >
            New collection
          </button>
        </div>
      )}

      {collections && collections.length > 0 && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3.5">
          {collections.map((collection, i) => (
            <CollectionCard
              key={collection.id}
              collection={collection}
              accentIndex={i}
              onDeleted={() => handleDelete(collection)}
              onRenamed={() => qc.invalidateQueries({ queryKey: ['collections'] })}
            />
          ))}
        </div>
      )}

      {showCreate && (
        <CreateCollectionModal
          onClose={() => setShowCreate(false)}
          onCreated={() => qc.invalidateQueries({ queryKey: ['collections'] })}
        />
      )}
    </div>
  )
}
