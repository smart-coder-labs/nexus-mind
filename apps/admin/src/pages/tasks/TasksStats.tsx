import type { LucideIcon } from 'lucide-react'
import { CheckCircle2, ListTodo, Eye, Clock, LayoutGrid } from 'lucide-react'
import { STATUS_COLORS } from '../Tasks'
import type { Task, TaskStatus } from '../../types'
import { KpiMarquee } from '@/components/ui/KpiMarquee'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Tasks page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// Lowercase source text, same reasoning as the tile labels below: this legend
// sits next to TasksBoard's Title Case column headers ("Backlog", "In
// Progress", ...) in the same view, and Testing Library's `getByText` only
// looks at a node's OWN direct text children — the `<strong>{count}</strong>`
// sibling doesn't save a bare-label span from an exact-text collision with
// the board header of the same status.
const DISTRIBUTION_STATUSES: { status: TaskStatus; label: string }[] = [
  { status: 'backlog', label: 'backlog' },
  { status: 'todo', label: 'to do' },
  { status: 'in_progress', label: 'in progress' },
  { status: 'in_review', label: 'in review' },
  { status: 'done', label: 'done' },
  { status: 'cancelled', label: 'cancelled' },
]

interface StatTileData {
  key: string
  label: string
  value: number
  /** Omitted (not fabricated) when nothing real is derivable for this tile. */
  sub?: string
  subColor?: string
  icon: LucideIcon
  accent: string
  progressPct: number
}

/** Most frequent assignee name among the given tasks, or undefined when none
 *  of them carry an assignee — never a fabricated placeholder name. */
function topAssigneeName(tasks: Task[]): string | undefined {
  const counts = new Map<string, number>()
  for (const t of tasks) {
    for (const a of t.assignees) {
      counts.set(a.name, (counts.get(a.name) ?? 0) + 1)
    }
  }
  let best: string | undefined
  let bestCount = 0
  for (const [name, count] of counts) {
    if (count > bestCount) {
      best = name
      bestCount = count
    }
  }
  return best
}

interface TasksStatsProps {
  tasks: Task[]
}

/**
 * Stat tiles + status distribution bar for the Tasks page header, matching
 * the target mockup. Every number is derived from the already-fetched task
 * list passed in by Tasks.tsx (the same list backing "N tasks" in the page
 * header) — no separate endpoint, no fabricated figures. A tile's
 * sub-caption is omitted rather than invented when it cannot be derived
 * from real data (e.g. no in-progress task carries an assignee).
 */
export default function TasksStats({ tasks }: TasksStatsProps) {
  const total = tasks.length
  const byStatus = (s: TaskStatus) => tasks.filter(t => t.status === s)
  const urgentIn = (list: Task[]) => list.filter(t => t.priority === 'urgent').length

  const done = byStatus('done')
  const backlog = byStatus('backlog')
  const inReview = byStatus('in_review')
  const inProgress = byStatus('in_progress')

  const completionPct = total > 0 ? Math.round((done.length / total) * 100) : 0
  const inProgressLead = inProgress.length > 0 ? topAssigneeName(inProgress) : undefined

  // Tile labels are intentionally lowercase source text with a CSS `uppercase`
  // transform (below) rather than literal capitalized strings: TasksBoard's
  // column headers ("Backlog", "In Progress", ...) render Title Case, and both
  // that board and this stats row are visible at once. Matching their exact
  // capitalization here would give two on-screen elements the identical
  // accessible text (a real a11y ambiguity, not just a test artifact) — the
  // case difference keeps them distinguishable to both screen readers and
  // `getByText` lookups while remaining visually uppercase either way.
  const tiles: StatTileData[] = [
    {
      key: 'done',
      label: 'done',
      value: done.length,
      sub: total > 0 ? `${completionPct}% completion` : undefined,
      subColor: '#34d399',
      icon: CheckCircle2,
      accent: '#34d399',
      progressPct: completionPct,
    },
    {
      key: 'backlog',
      label: 'backlog',
      value: backlog.length,
      sub: `${urgentIn(backlog)} urgent`,
      subColor: '#f87171',
      icon: ListTodo,
      accent: '#94a3b8',
      progressPct: total > 0 ? (backlog.length / total) * 100 : 0,
    },
    {
      key: 'in_review',
      label: 'in review',
      value: inReview.length,
      sub: `${urgentIn(inReview)} urgent`,
      subColor: '#f87171',
      icon: Eye,
      accent: '#facc15',
      progressPct: total > 0 ? (inReview.length / total) * 100 : 0,
    },
    {
      key: 'in_progress',
      label: 'in progress',
      value: inProgress.length,
      // No fabricated "top assignee" when none of the in-progress tasks has
      // one. Prefixed ("Lead: <name>") rather than the bare name so this
      // sub-caption's accessible text never exactly matches the same
      // person's name as it appears in the task list/board/timeline.
      sub: inProgressLead ? `Lead: ${inProgressLead}` : undefined,
      subColor: '#98a0b1',
      icon: Clock,
      accent: '#a78bfa',
      progressPct: total > 0 ? (inProgress.length / total) * 100 : 0,
    },
    {
      key: 'total',
      label: 'total',
      value: total,
      icon: LayoutGrid,
      accent: '#60a5fa',
      progressPct: 100,
    },
  ]

  return (
    <div className="space-y-3 mb-4">
      <KpiMarquee role="list" aria-label="Task stats">
        {tiles.map(tile => (
          <div key={tile.key} className="w-[232px] flex-none">
            <div
              role="listitem"
              className={`relative flex flex-col gap-2.5 rounded-[18px] p-4 overflow-hidden transition-colors hover:border-white/[0.16] ${GLASS_PANEL}`}
            >
              <div
                aria-hidden="true"
                className="absolute -top-9 -right-7 w-24 h-24 rounded-full pointer-events-none"
                style={{ background: tile.accent, opacity: 0.14, filter: 'blur(28px)' }}
              />
              <div className="flex items-center justify-between gap-2 relative">
                <span className="text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary truncate">
                  {tile.label}
                </span>
                <tile.icon className="w-3.5 h-3.5 shrink-0" style={{ color: tile.accent }} />
              </div>
              <div className="flex items-baseline gap-1.5 relative">
                <span className="text-lg font-bold leading-none text-text-primary tabular-nums">{tile.value}</span>
                {tile.sub && (
                  <span className="text-[11px] truncate" style={{ color: tile.subColor ?? 'var(--color-text-tertiary)' }}>
                    {tile.sub}
                  </span>
                )}
              </div>
              <div className="h-1 rounded-full bg-white/[0.06] overflow-hidden relative">
                <div
                  className="h-full rounded-full"
                  style={{ width: `${Math.min(tile.progressPct, 100)}%`, background: tile.accent }}
                />
              </div>
            </div>
          </div>
        ))}
      </KpiMarquee>

      <div className={`flex items-center gap-4 flex-wrap rounded-[13px] px-4 py-3 ${GLASS_PANEL}`}>
        <span className="text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary shrink-0">
          Distribution
        </span>
        <div
          role="img"
          aria-label="Task status distribution"
          className="flex-1 min-w-[180px] flex h-2.5 rounded-full overflow-hidden gap-[2px]"
        >
          {DISTRIBUTION_STATUSES.map(({ status, label }) => {
            const count = byStatus(status).length
            const pct = total > 0 ? (count / total) * 100 : 0
            if (pct === 0) return null
            return (
              <div
                key={status}
                title={`${label}: ${count}`}
                className="h-full"
                style={{ background: STATUS_COLORS[status], width: `${pct}%`, minWidth: 3, opacity: 0.9 }}
              />
            )
          })}
        </div>
        <div className="flex items-center gap-3 flex-wrap">
          {DISTRIBUTION_STATUSES.map(({ status, label }) => (
            <span key={status} className="flex items-center gap-1.5 text-[11px] text-text-tertiary">
              <span className="w-2 h-2 rounded-[3px]" style={{ background: STATUS_COLORS[status] }} />
              {label} <strong className="text-text-secondary font-semibold">{byStatus(status).length}</strong>
            </span>
          ))}
        </div>
      </div>
    </div>
  )
}
