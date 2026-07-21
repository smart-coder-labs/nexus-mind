import { useState } from 'react'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../../components/ui/Select/Select'
import { Markdown } from '../../components/ui/Markdown'

/**
 * The document viewer shared by `ChangeDetail` (artifacts) and `SpecDetail` (the
 * living specification): the Raw/Preview toggle, the revision selector, and the
 * read-only content panel.
 *
 * It is EXTRACTED rather than duplicated on purpose. Both trees hold immutable,
 * append-only, markdown documents addressed by revision, and the admin is read-only
 * over the content of both (A7). Two copies of this would drift — one would grow a
 * Raw default, or a revision selector that silently shows the latest when an older
 * revision fails to load — and the two trees would start disagreeing about what
 * "showing a document" means.
 *
 * It owns the view mode (a purely presentational choice) and nothing else. Which
 * revision is selected belongs to the caller, because the caller is the one that has
 * to reset it when the user switches to a different document.
 */

/** The shape both `SddRevisionMeta` and `SddSpecRevisionMeta` already satisfy. */
export interface RevisionOption {
  id: string
  revision: number
  source: string
  created_at: string
}

const FOCUS =
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

export function defaultRevisionLabel(rev: RevisionOption): string {
  return `rev ${rev.revision} · ${rev.source} · ${new Date(rev.created_at).toLocaleDateString()}`
}

interface DocumentViewProps<R extends RevisionOption> {
  /** Text of the revision currently selected. Empty string while it loads. */
  content: string
  /** `false` renders `emptyMessage` and no toolbar — there is nothing to view. */
  hasDocument: boolean
  emptyMessage: string
  revisions: R[]
  latestRevision: number
  /** `null` means "the latest", which is what arrived inline with the detail read. */
  selectedRevision: number | null
  onSelectRevision: (revision: number) => void
  /** Overridable so the specs view can name the change each revision was merged from. */
  revisionLabel?: (rev: R) => string
  testId?: string
  rawTestId?: string
}

export default function DocumentView<R extends RevisionOption>({
  content,
  hasDocument,
  emptyMessage,
  revisions,
  latestRevision,
  selectedRevision,
  onSelectRevision,
  revisionLabel = defaultRevisionLabel,
  testId = 'artifact-panel',
  rawTestId = 'artifact-raw',
}: DocumentViewProps<R>) {
  const [viewMode, setViewMode] = useState<'raw' | 'preview'>('preview')

  return (
    <>
      {/* Toolbar: Raw/Preview + revision selector. Deliberately OUTSIDE the panel —
          the panel holds rendered content and nothing editable. */}
      {hasDocument && (
        <div className="flex items-center justify-between gap-3 mb-2">
          <div className="bg-white/[0.04] rounded-full p-0.5 flex items-center w-fit">
            <button
              onClick={() => setViewMode('raw')}
              className={`text-[10px] px-2 py-0.5 rounded-full transition-colors ${FOCUS} ${
                viewMode === 'raw'
                  ? 'bg-white/[0.08] text-text-primary font-semibold'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              Raw
            </button>
            <button
              onClick={() => setViewMode('preview')}
              className={`text-[10px] px-2 py-0.5 rounded-full transition-colors ${FOCUS} ${
                viewMode === 'preview'
                  ? 'bg-white/[0.08] text-text-primary font-semibold'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              Preview
            </button>
          </div>

          {revisions.length > 0 && (
            <Select
              value={String(selectedRevision ?? latestRevision)}
              onValueChange={v => onSelectRevision(Number(v))}
            >
              <SelectTrigger className="w-56 h-8 text-xs" aria-label="Revision">
                <SelectValue placeholder={`rev ${latestRevision}`} />
              </SelectTrigger>
              <SelectContent>
                {revisions.map(r => (
                  <SelectItem key={r.id} value={String(r.revision)}>
                    {revisionLabel(r)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        </div>
      )}

      {/* Content — READ-ONLY (A7). No editor, no save, no delete. */}
      <section
        data-testid={testId}
        role="tabpanel"
        className="mb-6 rounded-[11px] border border-border-secondary p-4 max-h-[50vh] overflow-y-auto"
      >
        {!hasDocument ? (
          <p className="text-xs text-text-quaternary">{emptyMessage}</p>
        ) : viewMode === 'raw' ? (
          <pre
            data-testid={rawTestId}
            className="text-xs text-text-secondary whitespace-pre-wrap font-mono"
          >{content}</pre>
        ) : (
          <div className="text-xs text-text-secondary">
            <Markdown content={content} />
          </div>
        )}
      </section>
    </>
  )
}
