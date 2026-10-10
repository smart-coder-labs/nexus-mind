import type { LucideIcon } from 'lucide-react'
import { CheckCircle2, ListTodo, Eye, Clock, LayoutGrid } from 'lucide-react'
import { STATUS_COLORS } from '../Tasks'
import type { Task, TaskStatus } from '../../types'
import { StatTile } from '../dashboard/StatTile'
import { accentFor } from '../dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Tasks page.
const GLASS_PANEL = 'border border-border-primary bg-surface-primary '

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
  icon: LucideIcon
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
 * Compact metric disclosures and status distribution for the Tasks page. Every number is derived from the already-fetched task
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

  const tiles: StatTileData[] = [
    {
      key: 'done',
      label: 'done',
      value: done.length,
      sub: total > 0 ? `${completionPct}% completion` : undefined,
      icon: CheckCircle2,
    },
    {
      key: 'backlog',
      label: 'backlog',
      value: backlog.length,
      sub: `${urgentIn(backlog)} urgent`,
      icon: ListTodo,
    },
    {
      key: 'in_review',
      label: 'in review',
      value: inReview.length,
      sub: `${urgentIn(inReview)} urgent`,
      icon: Eye,
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
      icon: Clock,
    },
    {
      key: 'total',
      label: 'total',
      value: total,
      icon: LayoutGrid,
    },
  ]

  return (
    <div className="space-y-3 mb-4">
      <KpiMarquee compact role="list" aria-label="Task stats">
        {tiles.map((tile, index) => <StatTile key={tile.key} label={tile.label} value={tile.value.toLocaleString()} sub={tile.sub} icon={tile.icon} accent={accentFor(index)} />)}
      </KpiMarquee>

      <div className={`flex items-center gap-4 flex-wrap rounded-xl px-4 py-3 ${GLASS_PANEL}`}>
        <span className="text-xs font-medium text-muted-foreground shrink-0">
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
