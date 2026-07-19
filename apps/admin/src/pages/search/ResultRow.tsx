import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { cn } from '@/lib/utils'
import { highlightMatches } from './highlight'

export type ResultKind = 'memory' | 'user' | 'project' | 'policy' | 'convention' | 'sdd'

const KIND_STYLE: Record<ResultKind, { label: string; className: string }> = {
  memory:     { label: 'Memory',     className: 'bg-accent-blue/14 text-accent-blue' },
  convention: { label: 'Convention', className: 'bg-status-warning/14 text-status-warning' },
  sdd:        { label: 'SDD',        className: 'bg-accent-purple/14 text-accent-purple' },
  user:       { label: 'User',       className: 'bg-status-success/14 text-status-success' },
  project:    { label: 'Project',    className: 'bg-white/[0.06] text-text-tertiary' },
  policy:     { label: 'Policy',     className: 'bg-white/[0.06] text-text-tertiary' },
}

interface ResultRowProps {
  kind: ResultKind
  title: string
  /** Free-text body to run match-highlighting over. Omitted entirely when the
   *  entity has no natural excerpt field (e.g. a user has no "content"). */
  excerpt?: string
  /** The committed search query — used only for highlighting `excerpt`. */
  query: string
  /** Pre-filtered, already-joined meta fragments, e.g. ["kasymir", "7/14/2026"]. */
  meta?: string[]
  tags?: string[]
  /** When set, the whole row navigates here (existing SDD-only behavior, preserved). */
  href?: string
  /** Small trailing badge next to the title — role/enabled/phase, one per kind. */
  extra?: ReactNode
}

// Relevance score + response-latency columns from the design mockup are intentionally
// omitted here: GET /v1/search (apps/backend/src/api/search.rs) returns plain rows with
// no score and no timing field, so rendering either would be fabricated data.
export function ResultRow({ kind, title, excerpt, query, meta = [], tags = [], href, extra }: ResultRowProps) {
  const style = KIND_STYLE[kind]

  const body = (
    <div className="flex flex-col gap-2 p-4 rounded-[14px] border border-border-primary bg-white/[0.04] backdrop-blur-md transition-colors hover:border-accent-blue/40">
      <div className="flex items-center gap-2.5">
        <span className={cn('shrink-0 text-[11px] font-bold px-2.5 py-0.5 rounded-[10px]', style.className)}>
          {style.label}
        </span>
        <span className="text-[14px] font-bold text-text-primary truncate flex-1 min-w-0">{title}</span>
        {extra}
      </div>

      {excerpt && (
        <p className="text-[12.5px] text-text-secondary leading-relaxed line-clamp-2">
          {/* The full, unmodified excerpt as a single text node — the accessible
             content, and also what substring queries (incl. Testing Library's
             getByText, which only reads an element's *direct* text-node children)
             actually match against. The decorated copy below is presentation-only. */}
          <span className="sr-only">{excerpt}</span>
          <span aria-hidden="true">{highlightMatches(excerpt, query)}</span>
        </p>
      )}

      {(meta.length > 0 || tags.length > 0) && (
        <div className="flex items-center gap-2 flex-wrap">
          {meta.length > 0 && (
            <span className="text-[11px] text-text-quaternary shrink-0">{meta.join(' · ')}</span>
          )}
          <div className="flex-1" />
          {tags.length > 0 && (
            <div className="flex items-center gap-1.5 flex-wrap justify-end">
              {tags.slice(0, 6).map(tag => (
                <span
                  key={tag}
                  className="text-[10.5px] px-2 py-0.5 rounded-full bg-white/[0.05] text-text-tertiary"
                >
                  #{tag}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )

  if (href) {
    return (
      <Link to={href} className="block">
        {body}
      </Link>
    )
  }
  return body
}
