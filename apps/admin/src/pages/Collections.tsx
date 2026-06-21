import { useState, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { Collection, Memory } from '../types'
import { FolderOpen, Pencil, Trash2, X, Plus, Search } from 'lucide-react'

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
    <div
      className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-[#1d1d1f] rounded-[18px] border border-border-primary p-6 w-full max-w-md"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-sm font-semibold text-text-primary">New collection</h2>
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
            <label className="text-xs text-text-secondary">Name</label>
            <input
              type="text"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="My collection"
              autoFocus
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary"
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs text-text-secondary">
              Description <span className="text-text-quaternary">(optional)</span>
            </label>
            <textarea
              value={description}
              onChange={e => setDescription(e.target.value)}
              placeholder="What this collection is for…"
              rows={3}
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary resize-none"
            />
          </div>

          {error && (
            <p className="text-xs text-status-error">{error}</p>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="text-xs px-4 py-2 rounded-[8px] border border-border-primary text-text-secondary hover:text-text-primary transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="text-xs px-4 py-2 rounded-[8px] bg-accent-blue text-white hover:bg-accent-blue/90 transition-colors disabled:opacity-50"
            >
              {submitting ? 'Creating…' : 'Create'}
            </button>
          </div>
        </form>
      </div>
    </div>
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
    <div
      className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-[#1d1d1f] rounded-[18px] border border-border-primary p-6 w-full max-w-md"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-sm font-semibold text-text-primary">Rename collection</h2>
          <button onClick={onClose} className="text-text-quaternary hover:text-text-secondary transition-colors" aria-label="Close">
            <X className="w-4 h-4" />
          </button>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-xs text-text-secondary">Name</label>
            <input
              type="text"
              value={name}
              onChange={e => setName(e.target.value)}
              autoFocus
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60"
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs text-text-secondary">Description</label>
            <textarea
              value={description}
              onChange={e => setDescription(e.target.value)}
              rows={2}
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 resize-none"
            />
          </div>
          {error && <p className="text-xs text-status-error">{error}</p>}
          <div className="flex justify-end gap-2 pt-1">
            <button type="button" onClick={onClose} className="text-xs px-4 py-2 rounded-[8px] border border-border-primary text-text-secondary hover:text-text-primary transition-colors">
              Cancel
            </button>
            <button type="submit" disabled={submitting} className="text-xs px-4 py-2 rounded-[8px] bg-accent-blue text-white hover:bg-accent-blue/90 transition-colors disabled:opacity-50">
              {submitting ? 'Saving…' : 'Save'}
            </button>
          </div>
        </form>
      </div>
    </div>
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
    <div
      className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-[#1d1d1f] rounded-[18px] border border-border-primary p-6 w-full max-w-lg"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-sm font-semibold text-text-primary">{collection.name}</h2>
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
              placeholder="Search memories…"
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary pl-8 pr-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary"
            />
          </div>

          {addError && <p className="text-xs text-status-error">{addError}</p>}

          <div className="max-h-64 overflow-y-auto space-y-1">
            {searching && (
              <div className="space-y-2">
                {[1, 2, 3].map(i => (
                  <div key={i} className="animate-pulse h-10 bg-white/[0.04] rounded-[8px]" />
                ))}
              </div>
            )}
            {!searching && (!searchResults || searchResults.length === 0) && (
              <p className="text-xs text-text-quaternary text-center py-4">No memories found</p>
            )}
            {searchResults?.map(memory => (
              <div
                key={memory.id}
                className="flex items-center gap-2 px-3 py-2 rounded-[8px] bg-white/[0.02] border border-border-primary/50"
              >
                <p className="text-xs text-text-secondary flex-1 truncate">{memory.content}</p>
                <button
                  onClick={() => handleAssign(memory)}
                  disabled={adding}
                  className="flex-shrink-0 flex items-center gap-1 text-[10px] px-2 py-1 rounded-[5px] border border-accent-blue/30 text-accent-blue hover:bg-accent-blue/10 transition-colors disabled:opacity-50"
                >
                  <Plus className="w-3 h-3" />
                  Add
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}

interface CollectionCardProps {
  collection: Collection
  onDeleted: () => void
  onRenamed: () => void
}

function CollectionCard({ collection, onDeleted, onRenamed }: CollectionCardProps) {
  const [showRename, setShowRename] = useState(false)
  const [showMemories, setShowMemories] = useState(false)

  return (
    <>
      <div
        onClick={() => setShowMemories(true)}
        className="group bg-[#272729] rounded-[18px] border border-border-primary p-5 cursor-pointer hover:border-accent-blue/20 transition-colors"
      >
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <FolderOpen className="w-4 h-4 text-text-quaternary flex-shrink-0 group-hover:text-accent-blue transition-colors" />
            <span className="text-sm font-semibold text-text-primary truncate">{collection.name}</span>
          </div>
          <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
            <button
              onClick={e => { e.stopPropagation(); setShowRename(true) }}
              className="p-1 rounded-[5px] text-text-quaternary hover:text-text-secondary hover:bg-white/[0.06] transition-colors"
              aria-label="Rename"
            >
              <Pencil className="w-3 h-3" />
            </button>
            <button
              onClick={e => { e.stopPropagation(); onDeleted() }}
              className="p-1 rounded-[5px] text-text-quaternary hover:text-status-error hover:bg-status-error/10 transition-colors"
              aria-label="Delete"
            >
              <Trash2 className="w-3 h-3" />
            </button>
          </div>
        </div>

        {collection.memory_count != null && (
          <div className="mt-2">
            <span className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-text-secondary">
              {collection.memory_count} {collection.memory_count === 1 ? 'memory' : 'memories'}
            </span>
          </div>
        )}

        {collection.description && (
          <p className="text-xs text-text-quaternary mt-2 line-clamp-2">{collection.description}</p>
        )}
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

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-start justify-between">
        <div>
          <h1 className="text-[21px] font-semibold tracking-[0.231px] text-text-primary">Collections</h1>
          <p className="mt-1 text-[14px] text-text-tertiary tracking-[-0.224px]">
            Group memories into named collections for easier organization and filtering.
          </p>
        </div>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-1.5 text-xs px-4 py-2 rounded-full border border-border-primary text-text-secondary hover:text-text-primary hover:border-accent-blue/40 transition-colors flex-shrink-0"
        >
          <Plus className="w-3.5 h-3.5" />
          New collection
        </button>
      </div>

      {isLoading && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {[1, 2, 3, 4, 5, 6].map(i => (
            <div key={i} className="animate-pulse bg-[#272729] rounded-[18px] border border-border-primary p-5 h-28" />
          ))}
        </div>
      )}

      {!isLoading && (!collections || collections.length === 0) && (
        <div className="flex flex-col items-center gap-4 py-20">
          <CollectionIcon />
          <div className="text-center">
            <p className="text-sm font-semibold text-text-tertiary">No collections yet</p>
            <p className="text-xs text-text-quaternary mt-1">
              Create a collection to group related memories together.
            </p>
          </div>
          <button
            onClick={() => setShowCreate(true)}
            className="text-xs px-4 py-2 rounded-full border border-border-primary text-text-secondary hover:text-text-primary transition-colors"
          >
            New collection
          </button>
        </div>
      )}

      {collections && collections.length > 0 && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {collections.map(collection => (
            <CollectionCard
              key={collection.id}
              collection={collection}
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
