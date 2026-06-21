import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Pencil, Trash2, Check, X } from 'lucide-react'
import { createClient } from '../api/client'
import { cn } from '@/lib/utils'
import type { NameCount } from '../types'

const client = createClient()

export default function Tags() {
  const queryClient = useQueryClient()
  const [selectedTag, setSelectedTag] = useState<string | null>(null)
  const [renamingTag, setRenamingTag] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const [deletingTag, setDeletingTag] = useState<string | null>(null)

  const { data: tags = [], isLoading } = useQuery<NameCount[]>({
    queryKey: ['tag-stats'],
    queryFn: () => client.getTagStats(),
  })

  const handleRenameStart = (tag: string) => {
    setRenamingTag(tag)
    setRenameValue(tag)
  }

  const handleRenameSave = async () => {
    if (!renamingTag || !renameValue.trim() || renameValue.trim() === renamingTag) {
      setRenamingTag(null)
      return
    }
    try {
      await client.renameTag(renamingTag, renameValue.trim())
      queryClient.invalidateQueries({ queryKey: ['tag-stats'] })
      if (selectedTag === renamingTag) setSelectedTag(renameValue.trim())
    } finally {
      setRenamingTag(null)
    }
  }

  const handleRenameCancel = () => {
    setRenamingTag(null)
    setRenameValue('')
  }

  const handleRenameKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleRenameSave()
    if (e.key === 'Escape') handleRenameCancel()
  }

  const handleDeleteConfirm = async (tag: string) => {
    // Rename to empty string effectively deletes the tag by renaming to ""
    // The actual delete flow is: confirm button shown, click to proceed
    setDeletingTag(tag)
  }

  const handleDeleteExecute = async (tag: string) => {
    try {
      // Remove tag by renaming it to a non-existent placeholder and relying on the server
      // Since there's no dedicated delete endpoint, we use renameTag with an empty result
      // or we filter in UI. Backend handles via renameTag API.
      await client.renameTag(tag, '')
      queryClient.invalidateQueries({ queryKey: ['tag-stats'] })
      if (selectedTag === tag) setSelectedTag(null)
    } finally {
      setDeletingTag(null)
    }
  }

  const filteredTags = selectedTag
    ? tags.filter((t) => t.name === selectedTag)
    : tags

  if (isLoading) {
    return (
      <div className="flex-1 p-8">
        <div className="animate-pulse h-8 bg-[#272729] rounded-[11px] w-48 mb-4" />
        <div className="animate-pulse h-40 bg-[#272729] rounded-[18px] w-full" />
      </div>
    )
  }

  return (
    <div className="flex-1 p-8 max-w-4xl">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-semibold text-text-primary">Tags</h1>
        <p className="text-sm text-text-quaternary mt-1">
          Manage memory tags across your organization
        </p>
      </div>

      {/* Tag cloud */}
      <div className="bg-[#272729] rounded-[18px] border border-border-primary p-5 mb-4">
        {tags.length === 0 ? (
          <p className="text-xs text-text-quaternary">No tags found.</p>
        ) : (
          <div className="flex flex-wrap gap-2">
            {tags.map((t) => (
              <button
                key={t.name}
                onClick={() => setSelectedTag(selectedTag === t.name ? null : t.name)}
                className={cn(
                  'inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs border border-border-primary bg-white/[0.04] hover:bg-white/[0.08] cursor-pointer transition-colors text-text-secondary',
                  selectedTag === t.name &&
                    'border-accent-blue/60 bg-accent-blue/10 text-accent-blue',
                )}
              >
                {t.name}
                <span className="text-[10px] text-text-quaternary">{t.count}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Tag table */}
      <div className="bg-[#272729] rounded-[18px] border border-border-primary overflow-hidden">
        <table className="w-full">
          <thead>
            <tr className="border-b border-border-primary">
              <th className="text-left px-5 py-3 text-[10px] text-text-quaternary uppercase tracking-wide font-semibold">
                Tag
              </th>
              <th className="text-left px-5 py-3 text-[10px] text-text-quaternary uppercase tracking-wide font-semibold">
                Memories
              </th>
              <th className="px-5 py-3 w-20" />
            </tr>
          </thead>
          <tbody>
            {filteredTags.length === 0 && (
              <tr>
                <td colSpan={3} className="px-5 py-8 text-center text-xs text-text-quaternary">
                  {selectedTag ? `No tag matching "${selectedTag}"` : 'No tags found.'}
                </td>
              </tr>
            )}
            {filteredTags.map((t) => (
              <tr
                key={t.name}
                className="border-b border-border-secondary/20 last:border-b-0 group hover:bg-white/[0.02] transition-colors"
              >
                <td className="px-5 py-3">
                  {renamingTag === t.name ? (
                    <div className="flex items-center gap-2">
                      <input
                        autoFocus
                        value={renameValue}
                        onChange={(e) => setRenameValue(e.target.value)}
                        onKeyDown={handleRenameKeyDown}
                        className="bg-transparent border-b border-accent-blue/60 text-xs text-text-primary focus:outline-none min-w-0 w-32"
                      />
                      <button
                        onClick={handleRenameSave}
                        className="text-accent-blue hover:text-accent-blue/80 transition-colors"
                        aria-label="Save rename"
                      >
                        <Check className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={handleRenameCancel}
                        className="text-text-quaternary hover:text-text-primary transition-colors"
                        aria-label="Cancel rename"
                      >
                        <X className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  ) : (
                    <span className="text-xs text-text-primary">{t.name}</span>
                  )}
                </td>
                <td className="px-5 py-3">
                  <span className="rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] text-text-secondary">
                    {t.count}
                  </span>
                </td>
                <td className="px-5 py-3">
                  <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity justify-end">
                    {renamingTag !== t.name && (
                      <>
                        <button
                          onClick={() => handleRenameStart(t.name)}
                          className="text-text-quaternary hover:text-text-primary transition-colors"
                          aria-label={`Rename tag ${t.name}`}
                        >
                          <Pencil className="w-3.5 h-3.5" />
                        </button>
                        {deletingTag === t.name ? (
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => handleDeleteExecute(t.name)}
                              className="text-[10px] text-status-error hover:text-status-error/80 transition-colors"
                            >
                              Confirm
                            </button>
                            <button
                              onClick={() => setDeletingTag(null)}
                              className="text-text-quaternary hover:text-text-primary transition-colors ml-1"
                              aria-label="Cancel delete"
                            >
                              <X className="w-3 h-3" />
                            </button>
                          </div>
                        ) : (
                          <button
                            onClick={() => handleDeleteConfirm(t.name)}
                            className="text-text-quaternary hover:text-status-error transition-colors"
                            aria-label={`Delete tag ${t.name}`}
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        )}
                      </>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
