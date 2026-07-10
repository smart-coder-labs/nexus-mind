import { Badge } from '../../components/ui/Badge/Badge'
import { PRIORITY_BADGE_VARIANT } from '../Tasks'
import type { Task, TaskStatus } from '../../types'

const BOARD_COLUMNS: { status: TaskStatus; label: string }[] = [
  { status: 'backlog', label: 'Backlog' },
  { status: 'todo', label: 'To Do' },
  { status: 'in_progress', label: 'In Progress' },
  { status: 'in_review', label: 'In Review' },
  { status: 'done', label: 'Done' },
  { status: 'cancelled', label: 'Cancelled' },
]

interface TasksBoardProps {
  tasks: Task[]
  onTaskClick: (task: Task) => void
}

export default function TasksBoard({ tasks, onTaskClick }: TasksBoardProps) {
  return (
    <div className="grid grid-cols-6 gap-3">
      {BOARD_COLUMNS.map(col => {
        const columnTasks = tasks.filter(t => t.status === col.status)
        return (
          <div
            key={col.status}
            data-testid={`board-column-${col.status}`}
            className="rounded-[18px] bg-[#272729] border border-border-primary p-3 min-h-[240px]"
          >
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide">{col.label}</h3>
              <span className="text-[10px] text-text-quaternary">{columnTasks.length}</span>
            </div>
            {columnTasks.length === 0 ? (
              <p className="text-[10px] text-text-quaternary">No tasks</p>
            ) : (
              <div className="space-y-2">
                {columnTasks.map(task => (
                  <button
                    key={task.id}
                    onClick={() => onTaskClick(task)}
                    className="w-full text-left rounded-[11px] border border-border-secondary bg-background-tertiary/40 p-2.5 hover:border-accent-blue/40 transition-colors"
                  >
                    <p className="text-xs text-text-primary font-medium mb-1.5">{task.title}</p>
                    <div className="flex items-center justify-between">
                      <Badge variant={PRIORITY_BADGE_VARIANT[task.priority]} size="sm">{task.priority}</Badge>
                      {task.assignees.length > 0 && (
                        <span className="text-[10px] text-text-quaternary">
                          {task.assignees[0].name}{task.assignees.length > 1 ? ` +${task.assignees.length - 1}` : ''}
                        </span>
                      )}
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
