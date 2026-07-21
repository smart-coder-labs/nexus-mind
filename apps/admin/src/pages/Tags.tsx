import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Pencil, Trash2, Check, X, GitMerge, Hash, Layers, TrendingUp } from 'lucide-react'
import { createClient } from '../api/client'
import { cn } from '@/lib/utils'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { StatTile } from './dashboard/StatTile'
import { accentFor } from './dashboard/colors'
import type { NameCount } from '../types'

const client = createClient()

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Tags page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// Word-cloud size/weight/color scaled by usage relative to the top tag —
// mirrors the mockup's "Vocabulary" cloud using only real counts.
function cloudStyle(count: number, max: number) {
  const k = max > 0 ? count / max : 0
  const size = Math.round(11 + k * 13) // 11px..24px
  const weight = k > 0.5 ? 800 : k > 0.2 ? 700 : 500
  const color =
    k > 0.6 ? 'text-accent-blue' : k > 0.3 ? 'text-text-secondary' : k > 0.12 ? 'text-text-tertiary' : 'text-text-quaternary'
  return { size, weight, color }
}

export default function Tags() {
  const queryClient = useQueryClient()
  const [selectedTag, setSelectedTag] = useState<string | null>(null)
  const [renamingTag, setRenamingTag] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const [deletingTag, setDeletingTag] = useState<string | null>(null)
  const [mergingTag, setMergingTag] = useState<string | null>(null)
  const [mergeTarget, setMergeTarget] = useState('')

  const { data: tags = [], isLoading } = useQuery<NameCount[]>({
    queryKey: ['tag-stats'],
    queryFn: () => client.getTagStats(),
  })

  const mergeMut = useMutation({
    mutationFn: ({ source, target }: { source: string; target: string }) =>
      client.mergeTag(source, target),
    onSuccess: () => {
      setMergingTag(null)
      setMergeTarget('')
      queryClient.invalidateQueries({ queryKey: ['tag-stats'] })
    },
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

  // Stat tiles + word-cloud sizing derived purely from getTagStats() —
  // "last used" and "merge candidates" from the mockup have no backing
  // field/heuristic in NameCount, so they're omitted rather than fabricated.
  const maxCount = useMemo(() => tags.reduce((m, t) => Math.max(m, t.count), 0), [tags])
  const totalTaggings = useMemo(() => tags.reduce((s, t) => s + t.count, 0), [tags])
  const topTag = useMemo(
    () => (tags.length ? tags.reduce((a, b) => (b.count > a.count ? b : a)) : null),
    [tags],
  )

  if (isLoading) {
    return (
      <div className="flex-1 p-8">
        <div className="animate-pulse h-8 bg-white/[0.04] rounded-[11px] w-48 mb-4" />
        <div className={`animate-pulse h-40 rounded-[18px] w-full ${GLASS_PANEL}`} />
      </div>
    )
  }

  return (
    <div className="flex-1 p-8 max-w-5xl">
      {/* Header */}
      <div className="flex items-center gap-3.5 mb-6">
        <div className="w-11 h-11 rounded-[13px] bg-status-success/10 flex items-center justify-center flex-shrink-0">
          <Hash className="w-[22px] h-[22px] text-status-success" />
        </div>
        <div>
          <h1 className="text-[22px] font-semibold tracking-[-0.02em] text-text-primary">Tags</h1>
          <p className="text-[13px] text-text-tertiary mt-0.5">
            Manage memory tags across your organization
          </p>
        </div>
      </div>

      {/* Stat tiles — derived from getTagStats(), no fabricated numbers */}
      {tags.length > 0 && (
        <div className="mb-4">
          <KpiMarquee>
            <div key="tags" className="w-[232px] flex-none">
              <StatTile label="Tags" value={String(tags.length)} sub="vocabulary size" icon={Hash} accent={accentFor(0)} />
            </div>
            <div key="taggings" className="w-[232px] flex-none">
              <StatTile label="Taggings" value={totalTaggings.toLocaleString()} sub="total applications" icon={Layers} accent={accentFor(1)} />
            </div>
            <div key="top-tag" className="w-[232px] flex-none">
              <StatTile
                label="Top tag"
                value={topTag ? topTag.name : '—'}
                sub={topTag ? `${topTag.count.toLocaleString()} uses` : undefined}
                icon={TrendingUp}
                accent={accentFor(2)}
              />
            </div>
          </KpiMarquee>
        </div>
      )}

      {/* Tag cloud — size/weight/color scaled by relative usage */}
      <div className={`rounded-[18px] p-5 mb-4 ${GLASS_PANEL}`}>
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-[13px] font-semibold text-text-primary">Vocabulary</h2>
          <span className="text-[11px] text-text-quaternary">size = usage · click to filter</span>
        </div>
        {tags.length === 0 ? (
          <p className="text-xs text-text-quaternary">No tags found.</p>
        ) : (
          <div className="flex flex-wrap items-baseline gap-x-3.5 gap-y-2">
            {tags.map((t) => {
              const { size, weight, color } = cloudStyle(t.count, maxCount)
              return (
                <button
                  key={t.name}
                  onClick={() => setSelectedTag(selectedTag === t.name ? null : t.name)}
                  style={{ fontSize: `${size}px`, fontWeight: weight }}
                  className={cn(
                    'leading-tight transition-colors hover:text-accent-blue cursor-pointer',
                    selectedTag === t.name ? 'text-accent-blue' : color,
                  )}
                >
                  #{t.name}
                </button>
              )
            })}
          </div>
        )}
      </div>

      {/* Tag table */}
      <div className={`rounded-[18px] overflow-hidden overflow-x-auto ${GLASS_PANEL}`}>
        <table className="w-full">
          <thead>
            <tr className="border-b border-border-primary">
              <th className="text-left px-5 py-3 text-[10px] text-text-quaternary uppercase tracking-wide font-semibold">
                Tag
              </th>
              <th className="text-left px-5 py-3 text-[10px] text-text-quaternary uppercase tracking-wide font-semibold">
                Memories
              </th>
              <th className="text-left px-5 py-3 text-[10px] text-text-quaternary uppercase tracking-wide font-semibold">
                Distribution
              </th>
              <th className="px-5 py-3 w-20" />
            </tr>
          </thead>
          <tbody>
            {filteredTags.length === 0 && (
              <tr>
                <td colSpan={4} className="px-5 py-8 text-center text-xs text-text-quaternary">
                  {selectedTag ? `No tag matching "${selectedTag}"` : 'No tags found.'}
                </td>
              </tr>
            )}
            {filteredTags.map((t) => (
              <tr
                key={t.name}
                className="border-b border-border-secondary/20 last:border-b-0 group hover:bg-accent-blue/[0.05] transition-colors"
              >
                <td className="px-5 py-3">
                  {renamingTag === t.name ? (
                    <div className="flex items-center gap-2">
                      <input
                        autoFocus
                        value={renameValue}
                        onChange={(e) => setRenameValue(e.target.value)}
                        onKeyDown={handleRenameKeyDown}
                        className="bg-transparent border-b border-border-primary text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 min-w-0 w-32"
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
                    <span className="text-xs font-semibold text-text-primary font-mono">#{t.name}</span>
                  )}
                </td>
                <td className="px-5 py-3">
                  <span className="rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] text-text-quaternary tabular-nums">
                    {t.count}
                  </span>
                </td>
                <td className="px-5 py-3">
                  <div className="h-1 rounded-full bg-white/[0.06] overflow-hidden max-w-[140px]">
                    <div
                      className="h-full rounded-full bg-status-success"
                      style={{ width: `${maxCount > 0 ? Math.max(2, Math.round((t.count / maxCount) * 100)) : 0}%` }}
                    />
                  </div>
                </td>
                <td className="px-5 py-3">
                  <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity justify-end">
                    {renamingTag !== t.name && (
                      <>
                        <button
                          onClick={() => setMergingTag(t.name)}
                          className="text-text-quaternary hover:text-text-primary transition-colors"
                          aria-label={`Merge tag ${t.name}`}
                          title="Merge into another tag"
                        >
                          <GitMerge className="w-3.5 h-3.5" />
                        </button>
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
                              <X className="w-3.5 h-3.5" />
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

      {/* Merge modal */}
      {mergingTag && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] rounded-[18px] p-6 w-full max-w-md shadow-xl">
            <h2 className="text-xs font-semibold text-text-primary">Merge "{mergingTag}"</h2>
            <p className="text-xs text-text-quaternary mt-1">
              All memories tagged "{mergingTag}" will be retagged to the target.
              The original tag will be removed.
            </p>

            <div className="mt-4">
              <input
                autoFocus
                value={mergeTarget}
                onChange={(e) => setMergeTarget(e.target.value)}
                placeholder="Target tag name…"
                list="tag-list"
                className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-secondary px-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary"
              />
              <datalist id="tag-list">
                {tags.filter((t) => t.name !== mergingTag).map((t) => (
                  <option key={t.name} value={t.name} />
                ))}
              </datalist>
            </div>

            {mergeMut.isError && (
              <p className="text-[10px] text-status-error mt-2">
                {(mergeMut.error as Error)?.message ?? 'Merge failed'}
              </p>
            )}

            <div className="flex items-center justify-end gap-2 mt-4">
              <button
                onClick={() => {
                  setMergingTag(null)
                  setMergeTarget('')
                  mergeMut.reset()
                }}
                className="border border-border-primary rounded-full px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={() => mergeMut.mutate({ source: mergingTag, target: mergeTarget })}
                disabled={!mergeTarget || mergeTarget === mergingTag || mergeMut.isPending}
                className="bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold disabled:opacity-40 hover:bg-accent-blue/90 transition-colors"
              >
                {mergeMut.isPending ? 'Merging…' : 'Merge'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
