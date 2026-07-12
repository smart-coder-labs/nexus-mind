import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { Link } from 'react-router-dom'
import { createClient } from '../../api/client'
import { useAuth, isPrivileged } from '../../auth/AuthContext'
import { Badge } from '../../components/ui/Badge/Badge'
import DocumentView from './DocumentView'
import type { SddSpecRevisionMeta } from '../../types'

const client = createClient()

interface SpecDetailProps {
  specId: string
  onClose: () => void
}

const FOCUS =
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

/**
 * One living specification — `openspec/specs/{capability}/spec.md`.
 *
 * There are no tabs and no capability sub-list, because a spec IS one document.
 * Everything else — the Raw/Preview toggle, the revision selector, the read-only
 * markdown panel — is `DocumentView`, shared with `ChangeDetail` rather than forked
 * from it.
 *
 * The one thing this view has that the artifact view does not is provenance: each
 * revision names the CHANGE whose deltas produced it. That is the whole reason the
 * spec store exists as its own entity, so the revision selector says it out loud.
 */
export function specRevisionLabel(rev: SddSpecRevisionMeta): string {
  const base = `rev ${rev.revision} · ${rev.source} · ${new Date(rev.created_at).toLocaleDateString()}`
  return rev.merged_from_change_name ? `${base} · ← ${rev.merged_from_change_name}` : base
}

export default function SpecDetail({ specId, onClose }: SpecDetailProps) {
  const { session } = useAuth()
  const isAdmin = isPrivileged(session?.user.role)
  const permissions = session?.user.permissions ?? []
  // Every query below states its grant. An ungated 403 trips the client's global
  // handler and redirects the whole app to /401.
  const canRead = isAdmin || permissions.includes('sdd:read')

  const [selectedRevision, setSelectedRevision] = useState<number | null>(null)

  const { data: spec } = useQuery({
    queryKey: ['sdd-spec', specId],
    queryFn: () => client.getSddSpec(specId),
    enabled: canRead,
  })

  const { data: revisions = [] } = useQuery({
    queryKey: ['sdd-spec-revisions', specId],
    queryFn: () => client.listSddSpecRevisions(specId),
    enabled: canRead,
  })

  // Only fetched when the user asks for a revision other than the latest — the latest
  // already arrived inline with the detail read.
  const wantsOlderRevision =
    selectedRevision != null && selectedRevision !== spec?.latest_revision

  const { data: revisionDetail } = useQuery({
    queryKey: ['sdd-spec-revision', specId, selectedRevision],
    queryFn: () => client.getSddSpecRevision(specId, selectedRevision!),
    enabled: canRead && wantsOlderRevision,
  })

  const content = wantsOlderRevision ? revisionDetail?.content ?? '' : spec?.content ?? ''

  return (
    <div className="relative w-full">
      <button
        onClick={onClose}
        aria-label="Close"
        className={`absolute top-0 right-0 w-8 h-8 flex items-center justify-center rounded-full bg-background-tertiary text-text-secondary hover:text-text-primary transition-colors ${FOCUS}`}
      >
        <X className="w-3.5 h-3.5" />
      </button>

      <header className="mb-5 pr-10">
        <h2 className="text-sm font-semibold text-text-primary">{spec?.capability ?? '…'}</h2>
        {spec?.title && <p className="text-xs text-text-tertiary mt-0.5">{spec.title}</p>}
        {spec && (
          <div className="flex items-center gap-2 mt-2 flex-wrap">
            <Badge variant="default" size="sm">{spec.project}</Badge>
            <Badge variant="primary" size="sm">rev {spec.latest_revision}</Badge>
            {spec.path && (
              <span className="text-[10px] text-text-quaternary font-mono">{spec.path}</span>
            )}
          </div>
        )}
      </header>

      {/* Provenance — the payoff. Which change last merged its deltas into this
          contract? A spec with no answer is not an error: it may have been imported
          from disk, where that fact simply is not recorded. */}
      <section data-testid="spec-provenance" className="mb-4">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">
          Last Merged From
        </h3>
        {spec?.last_merged_from_change_name ? (
          <Link
            to={`/sdd?change=${encodeURIComponent(spec.last_merged_from_change_name)}`}
            className={`text-xs text-text-primary hover:text-accent-blue transition-colors ${FOCUS}`}
          >
            {spec.last_merged_from_change_name}
          </Link>
        ) : (
          <p className="text-xs text-text-quaternary">
            No change recorded — this revision was imported or written outside the change pipeline.
          </p>
        )}
      </section>

      {/* The contract — READ-ONLY (A7), same machinery as the artifact view. */}
      <DocumentView
        content={content}
        hasDocument={!!spec}
        emptyMessage="This capability has no specification yet."
        revisions={revisions}
        latestRevision={spec?.latest_revision ?? 0}
        selectedRevision={selectedRevision}
        onSelectRevision={setSelectedRevision}
        revisionLabel={specRevisionLabel}
        testId="spec-panel"
        rawTestId="spec-raw"
      />
    </div>
  )
}
