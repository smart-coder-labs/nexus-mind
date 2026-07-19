import { cn } from '../../lib/utils'
import { EmptyState } from '../../components/ui/EmptyState/EmptyState'
import { parseDateOnly, PriorityPill, STATUS_COLORS, StatusPill } from '../Tasks'
import type { Task } from '../../types'
import { CalendarClock } from 'lucide-react'

const NO_DUE_DATE_KEY = '__no-due-date__'
const FOCUS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

function dueDateKey(task: Task): string {
  return task.due_date ?? NO_DUE_DATE_KEY
}

function formatGroupLabel(key: string): string {
  if (key === NO_DUE_DATE_KEY) return 'No due date'
  const d = parseDateOnly(key)
  if (Number.isNaN(d.getTime())) return key
  return d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric', year: 'numeric' })
}

interface TasksTimelineProps {
  tasks: Task[]
  onTaskClick: (task: Task) => void
}

export default function TasksTimeline({ tasks, onTaskClick }: TasksTimelineProps) {
  if (tasks.length === 0) {
    return (
      <EmptyState
        icon={<CalendarClock />}
        title="No tasks"
        description="No tasks match the current filters. Try adjusting the filters or create a new task."
      />
    )
  }

  // Group by due date (falling back to updated_at is not needed — tasks with
  // no due_date are grouped under a dedicated "No due date" bucket, always
  // rendered last). Dated groups are sorted chronologically, earliest first.
  const byKey = new Map<string, Task[]>()
  for (const task of tasks) {
    const key = dueDateKey(task)
    if (!byKey.has(key)) byKey.set(key, [])
    byKey.get(key)!.push(task)
  }

  const dated = [...byKey.entries()]
    .filter(([key]) => key !== NO_DUE_DATE_KEY)
    .sort((a, b) => parseDateOnly(a[0]).getTime() - parseDateOnly(b[0]).getTime())
  const undated = byKey.get(NO_DUE_DATE_KEY)
  const groups = undated ? [...dated, [NO_DUE_DATE_KEY, undated] as const] : dated

  return (
    <div className="space-y-6">
      {groups.map(([key, groupTasks]) => (
        <div key={key}>
          <p className="text-[11px] font-semibold uppercase tracking-wider mb-3 pl-5 text-text-quaternary">
            {formatGroupLabel(key)}
          </p>
          <div className="relative">
            <div
              className="absolute left-[7px] top-2 bottom-2 w-px"
              style={{ background: 'rgba(255,255,255,0.08)' }}
              aria-hidden="true"
            />
            <ul className="space-y-2.5">
              {groupTasks.map(task => (
                <li key={task.id} className="flex items-start gap-3">
                  <span
                    className="w-[15px] h-[15px] rounded-full shrink-0 mt-0.5 relative z-10"
                    style={{ background: STATUS_COLORS[task.status], boxShadow: '0 0 0 2px rgba(13,15,20,0.6)' }}
                    aria-hidden="true"
                  />
                  <button
                    type="button"
                    onClick={() => onTaskClick(task)}
                    className={cn(
                      'flex-1 min-w-0 text-left rounded-[11px] border border-border-secondary bg-white/[0.02] hover:border-accent-blue/40 transition-colors px-3 py-2.5',
                      FOCUS,
                    )}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <span className="text-[13px] font-semibold text-text-primary truncate">{task.title}</span>
                      <div className="flex items-center gap-1.5 shrink-0">
                        <StatusPill status={task.status} />
                        <PriorityPill priority={task.priority} />
                      </div>
                    </div>
                    <p className="mt-1.5 text-[11.5px] text-text-quaternary truncate">
                      {task.assignees.length === 0 ? 'Unassigned' : task.assignees.map(a => a.name).join(', ')}
                    </p>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        </div>
      ))}
    </div>
  )
}
