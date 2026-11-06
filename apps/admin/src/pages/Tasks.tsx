import { Button } from '../components/ui/Button'
import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Pencil, Trash2, ListTodo, List, LayoutGrid, ChartGantt, ListChecks, Circle, CircleDashed, Eye, CheckCircle2, Ban, ArrowDown, Equal, ArrowUp, Siren, Users, UserRound, UserRoundCheck, FolderKanban } from 'lucide-react'
import { Navigate } from 'react-router-dom'
import { createClient } from '../api/client'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import { Modal, ModalCloseButton } from '../components/ui/Modal/Modal'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import { Badge } from '../components/ui/Badge/Badge'
import { EmptyState } from '../components/ui/EmptyState/EmptyState'
import { SegmentedControl } from '../components/ui/SegmentedControl'
import TaskDetail from './tasks/TaskDetail'
import TasksBoard from './tasks/TasksBoard'
import TasksTimeline from './tasks/TasksTimeline'
import TasksStats from './tasks/TasksStats'
import type { Task, TaskStatus, TaskPriority } from '../types'

type TasksView = 'list' | 'board' | 'timeline'

export const STATUS_OPTIONS: TaskStatus[] = ['backlog', 'todo', 'in_progress', 'in_review', 'done', 'cancelled']
export const PRIORITY_OPTIONS: TaskPriority[] = ['low', 'medium', 'high', 'urgent']

export const STATUS_BADGE_VARIANT: Record<TaskStatus, 'default' | 'primary' | 'success' | 'warning' | 'error' | 'info'> = {
  backlog: 'default',
  todo: 'info',
  in_progress: 'primary',
  in_review: 'warning',
  done: 'success',
  cancelled: 'error',
}

export const PRIORITY_BADGE_VARIANT: Record<TaskPriority, 'default' | 'primary' | 'warning' | 'error'> = {
  low: 'default',
  medium: 'primary',
  high: 'warning',
  urgent: 'error',
}

// Theme-aware semantic tones shared by task chips and the distribution strip.
export const STATUS_COLORS: Record<TaskStatus, string> = {
  backlog: 'var(--muted-foreground)',
  todo: 'var(--color-status-info)',
  in_progress: 'var(--brand-link)',
  in_review: 'var(--color-status-warning)',
  done: 'var(--color-status-success)',
  cancelled: 'var(--color-status-error)',
}

export const PRIORITY_COLORS: Record<TaskPriority, string> = {
  low: 'var(--muted-foreground)',
  medium: 'var(--color-status-info)',
  high: 'var(--color-status-warning)',
  urgent: 'var(--color-status-error)',
}

const TASK_STATUS_PRESENTATION: Record<TaskStatus, { description: string; icon: React.ReactNode }> = {
  backlog: { description: 'Captured for later; work has not started.', icon: <ListTodo /> },
  todo: { description: 'Ready to be picked up.', icon: <Circle /> },
  in_progress: { description: 'Someone is actively working on it.', icon: <CircleDashed /> },
  in_review: { description: 'Waiting for review or approval.', icon: <Eye /> },
  done: { description: 'Completed and verified.', icon: <CheckCircle2 /> },
  cancelled: { description: 'Stopped and will not be completed.', icon: <Ban /> },
}

const TASK_PRIORITY_PRESENTATION: Record<TaskPriority, { description: string; icon: React.ReactNode }> = {
  low: { description: 'Can wait until higher-priority work is done.', icon: <ArrowDown /> },
  medium: { description: 'Normal priority for planned work.', icon: <Equal /> },
  high: { description: 'Should be addressed soon.', icon: <ArrowUp /> },
  urgent: { description: 'Needs immediate attention.', icon: <Siren /> },
}

export function TaskStatusOption({ status }: { status: TaskStatus }) {
  const color = STATUS_COLORS[status]
  return <SelectItem value={status} icon={TASK_STATUS_PRESENTATION[status].icon} indicatorColor={color} description={TASK_STATUS_PRESENTATION[status].description}>{status.replace('_', ' ')}</SelectItem>
}

export function TaskPriorityOption({ priority }: { priority: TaskPriority }) {
  const color = PRIORITY_COLORS[priority]
  return <SelectItem value={priority} icon={TASK_PRIORITY_PRESENTATION[priority].icon} indicatorColor={color} description={TASK_PRIORITY_PRESENTATION[priority].description}>{priority}</SelectItem>
}

/** Subtle status chip: tinted background at ~14% of the status color. */
export function StatusPill({ status }: { status: TaskStatus }) {
  const color = STATUS_COLORS[status]
  return (
    <span
      className="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap"
      style={{ backgroundColor: `color-mix(in srgb, ${color} 14%, transparent)`, color }}
    >
      {status.replace(/_/g, ' ')}
    </span>
  )
}

/** Colored priority chip — urgent red-ish, high yellow, medium blue, low gray. */
export function PriorityPill({ priority }: { priority: TaskPriority }) {
  const color = PRIORITY_COLORS[priority]
  return (
    <span
      className="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-bold whitespace-nowrap"
      style={{ backgroundColor: `color-mix(in srgb, ${color} 14%, transparent)`, color }}
    >
      {priority}
    </span>
  )
}

/** Parses a `YYYY-MM-DD` date-only string (e.g. `task.due_date`) as a LOCAL
 *  date rather than UTC midnight — `new Date('2026-07-15')` shifts a day
 *  backward in any timezone west of UTC, which would misfile a task into the
 *  wrong timeline group. */
export function parseDateOnly(dateStr: string): Date {
  const [y, m, d] = dateStr.split('-').map(Number)
  return new Date(y, (m ?? 1) - 1, d ?? 1)
}

interface TaskFormState {
  title: string
  description: string
  project: string
  status: TaskStatus
  priority: TaskPriority
  due_date: string
}

const EMPTY_FORM: TaskFormState = {
  title: '',
  description: '',
  project: '',
  status: 'backlog',
  priority: 'medium',
  due_date: '',
}

const client = createClient()

export default function Tasks() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const isAdmin = isPrivileged(session?.user.role)
  const permissions = session?.user.permissions ?? []
  const canWrite = isAdmin || permissions.includes('task:write')
  const canDelete = isAdmin || permissions.includes('task:delete')
  const canRead = isAdmin || permissions.includes('task:read')

  const [projectFilter, setProjectFilter] = useState<string>('')
  const [statusFilter, setStatusFilter] = useState<string>('')
  /// Holds a user id, or the literal `me`, which the backend resolves from the
  /// caller's API key (api/tasks.rs). Empty string means "no filter".
  const [assigneeFilter, setAssigneeFilter] = useState<string>('')
  const [showArchived, setShowArchived] = useState(false)

  const [creating, setCreating] = useState(false)
  const [createForm, setCreateForm] = useState<TaskFormState>(EMPTY_FORM)

  const [detailTask, setDetailTask] = useState<Task | null>(null)
  const [view, setView] = useState<TasksView>('list')
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())

  const filters = useMemo(
    () => ({
      project: projectFilter || undefined,
      status: statusFilter ? (statusFilter as TaskStatus) : undefined,
      // `undefined`, never `''` — the client serializes every non-null value, so an
      // empty string would go out as `?assignee=` and match no one, rendering an
      // empty list that reads as "there are no tasks".
      assignee: assigneeFilter || undefined,
      // Same reasoning: `undefined` when off, so the param is omitted entirely.
      include_archived: showArchived || undefined,
    }),
    [projectFilter, statusFilter, assigneeFilter, showArchived],
  )

  const { data: tasks = [], isLoading } = useQuery({
    queryKey: ['tasks', filters],
    queryFn: () => client.listTasks(filters),
    enabled: canRead,
  })

  const { data: projects = [] } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
  })

  // Populates the assignee filter. Gated on the ROLE, not on task:read — because
  // `GET /v1/users` (api/users.rs) gates on `auth.role.is_privileged()` and not on any
  // permission string. Gated on canRead, as it was, a plain member holding task:read
  // fired this, took a 403, and the client's global handler ran
  // window.location.replace('/401') — ejecting them from the entire admin for opening
  // the Tasks page. The filter degrades gracefully without it: "All assignees" and
  // "Assigned to me" both still work, the latter because the backend resolves the `me`
  // sentinel from the caller's API key rather than from this list.
  const { data: users = [] } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    enabled: isAdmin,
  })

  const createMut = useMutation({
    mutationFn: () =>
      client.createTask({
        project: createForm.project || projects[0]?.name || '',
        title: createForm.title,
        description: createForm.description || undefined,
        status: createForm.status,
        priority: createForm.priority,
        due_date: createForm.due_date || undefined,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      setCreating(false)
      setCreateForm(EMPTY_FORM)
    },
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteTask(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['tasks'] }),
  })

  const bulkDeleteMut = useMutation({
    mutationFn: (ids: string[]) => Promise.all(ids.map(id => client.deleteTask(id))),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      setSelectedIds(new Set())
    },
  })

  // Archiving what is already archived is a no-op (soft_delete_task only matches rows
  // with `archived_at IS NULL`), so archived rows are not selectable.
  const selectableTasks = useMemo(() => tasks.filter(t => !t.archived_at), [tasks])
  const selectedTasks = useMemo(
    () => selectableTasks.filter(t => selectedIds.has(t.id)),
    [selectableTasks, selectedIds],
  )
  const allSelected = selectableTasks.length > 0 && selectedTasks.length === selectableTasks.length

  const handleCreateSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!createForm.title.trim()) return
    createMut.mutate()
  }

  const toggleOne = (id: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const toggleAll = () => {
    setSelectedIds(prev =>
      prev.size === selectableTasks.length ? new Set() : new Set(selectableTasks.map(t => t.id)),
    )
  }

  /** The FK `tasks.parent_id` is ON DELETE CASCADE, but that only fires on a hard
   *  DELETE and the API never issues one — `soft_delete_task` is a plain
   *  `UPDATE tasks SET archived_at = …`. So subtasks are NOT archived along with their
   *  parent; they survive, still pointing at an archived task. The backend pins this
   *  down in `soft_delete_parent_does_not_cascade_to_subtasks`. Warn about what will
   *  really happen rather than promising a cascade that does not occur. */
  const subtaskNote = (list: Task[]): string => {
    const n = list.reduce((sum, t) => sum + (t.subtask_count ?? 0), 0)
    if (n === 0) return ''
    return ` ${n} subtask${n === 1 ? '' : 's'} ${n === 1 ? 'is' : 'are'} NOT archived with ${list.length === 1 ? 'it' : 'them'} — ${n === 1 ? 'it remains' : 'they remain'} in the list.`
  }

  /** It is a SOFT delete — the backend sets `archived_at` and the row survives — so
   *  "this cannot be undone" would be a lie. But the API also exposes no task-restore
   *  endpoint, so "can be restored" is a promise the admin cannot keep. Both halves of
   *  the truth, or the user learns to distrust every warning you give them. */
  const survivesNote = ' The row survives and stays visible under "Show archived", but the API has no restore endpoint.'

  const handleDelete = (task: Task) => {
    if (!window.confirm(
      `Archive task "${task.title}"?${subtaskNote([task])} It is removed from the list.${survivesNote}`,
    )) return
    deleteMut.mutate(task.id)
  }

  const handleBulkDelete = () => {
    const count = selectedTasks.length
    if (count === 0) return
    // ONE confirmation for the batch, naming the count. ~950 tasks behind a blocking
    // window.confirm() each is not a feature.
    if (!window.confirm(
      `Archive ${count} task${count === 1 ? '' : 's'}?${subtaskNote(selectedTasks)} They are removed from the list.${survivesNote}`,
    )) return
    bulkDeleteMut.mutate(selectedTasks.map(t => t.id))
  }

  if (!canRead) return <Navigate to="/401" replace />

  return (
    <div className="p-6 max-w-6xl">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <div
            aria-hidden="true"
            className="w-11 h-11 rounded-xl bg-status-success/10 flex items-center justify-center shrink-0"
          >
            <ListChecks className="w-5 h-5 text-status-success" />
          </div>
          <div>
            <h1 className="text-[22px] leading-tight font-semibold text-text-primary">Tasks</h1>
            <p className="text-xs text-text-quaternary mt-0.5">{tasks.length} tasks</p>
          </div>
        </div>
        {canWrite && (
          <Button
            onClick={() => { setCreateForm({ ...EMPTY_FORM, project: projectFilter || projects[0]?.name || '' }); setCreating(true) }}

          >
            <Plus className="w-3.5 h-3.5" />
            New Task
          </Button>
        )}
      </div>

      {/* Stat tiles + status distribution — derived from the already-fetched,
          filter-scoped task list (same data backing "N tasks" above). No
          separate endpoint, no fabricated numbers. */}
      <TasksStats tasks={tasks} />

      {/* Filters */}
      <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
        <div className="flex min-w-0 flex-[1_1_100%] flex-wrap items-center gap-2 md:flex-1">
          <Select value={projectFilter} onValueChange={setProjectFilter}>
            <SelectTrigger className="w-full min-w-0 sm:w-48" aria-label="Project">
              <SelectValue placeholder="All projects" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="" icon={<FolderKanban />} description="Tasks across every project">All projects</SelectItem>
              {projects.map(p => (
                <SelectItem key={p.id} value={p.name} icon={<FolderKanban />} description={p.description || `Tasks in ${p.name}`}>{p.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={statusFilter} onValueChange={setStatusFilter}>
            <SelectTrigger className="w-full min-w-0 sm:w-40" aria-label="Status">
              <SelectValue placeholder="All statuses" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="" icon={<ListChecks />} description="Tasks at every stage">All statuses</SelectItem>
              {STATUS_OPTIONS.map(status => <TaskStatusOption key={status} status={status} />)}
            </SelectContent>
          </Select>

          <Select value={assigneeFilter} onValueChange={setAssigneeFilter}>
            <SelectTrigger className="w-full min-w-0 sm:w-48" aria-label="Assignee">
              <SelectValue placeholder="All assignees" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="" icon={<Users />} description="Tasks assigned to anyone">All assignees</SelectItem>
              <SelectItem value="me" icon={<UserRoundCheck />} description="Only tasks assigned to you">Assigned to me</SelectItem>
              {users.map(u => (
                <SelectItem key={u.id} value={u.id} icon={<UserRound />} description={u.name ? u.email : 'Workspace member'}>{u.name || u.email}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
            <input
              type="checkbox"
              aria-label="Show archived"
              checked={showArchived}
              onChange={e => { setShowArchived(e.target.checked); setSelectedIds(new Set()) }}
              className="accent-accent-blue"
            />
            Show archived
          </label>

          <SegmentedControl<TasksView>
            size="sm"
            value={view}
            onChange={setView}
            options={[
              { value: 'list', icon: <List className="w-3.5 h-3.5" />, 'aria-label': 'List view' },
              { value: 'board', icon: <LayoutGrid className="w-3.5 h-3.5" />, 'aria-label': 'Board view' },
              { value: 'timeline', icon: <ChartGantt className="w-3.5 h-3.5" />, 'aria-label': 'Timeline view' },
            ]}
          />
        </div>
      </div>

      {/* Bulk action bar — one confirmation for the whole batch. */}
      {canDelete && selectedTasks.length > 0 && (
        <div className="flex items-center justify-between gap-3 mb-3 rounded-xl border border-border-primary bg-surface-primary px-4 py-2">
          <span className="text-xs text-text-secondary">
            {selectedTasks.length} selected
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setSelectedIds(new Set())}
              className="px-3 py-1.5 rounded-md border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
            >
              Clear
            </button>
            <button
              onClick={handleBulkDelete}
              disabled={bulkDeleteMut.isPending}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-status-error text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              <Trash2 className="w-3.5 h-3.5" />
              {bulkDeleteMut.isPending
                ? 'Deleting…'
                : `Delete ${selectedTasks.length} selected`}
            </button>
          </div>
        </div>
      )}

      {showArchived && (
        // There is no restore endpoint for tasks: the router exposes /restore for
        // memories, projects, conventions, code projects and backups — but not tasks —
        // and PatchTaskRequest has no `archived_at` field. So this toggle is a
        // read-only window onto archived rows. Say that plainly instead of shipping a
        // Restore button that cannot work.
        <p className="text-xs text-text-quaternary mb-3">
          Archived tasks are shown for reference. The API exposes no task-restore
          endpoint, so they cannot be restored from the admin.
        </p>
      )}

      {/* Task list/board */}
      {isLoading ? (
        <div className="space-y-2">
          {[...Array(4)].map((_, i) => (
            <div key={i} className="rounded-xl border border-border-primary bg-surface-primary h-14 animate-pulse" />
          ))}
        </div>
      ) : tasks.length === 0 ? (
        <EmptyState
          icon={<ListTodo />}
          title="No tasks found"
          description="No tasks match the current filters. Try adjusting the filters or create a new task."
        />
      ) : view === 'board' ? (
        <TasksBoard tasks={tasks} onTaskClick={setDetailTask} />
      ) : view === 'timeline' ? (
        <TasksTimeline tasks={tasks} onTaskClick={setDetailTask} />
      ) : (
        <div className="overflow-x-auto rounded-xl border border-border-primary bg-surface-primary ">
          <table className="admin-data-table w-full min-w-[760px] table-fixed border-collapse text-left">
            {/* table-fixed: without it a long title stretches the Title column until the
                later columns — Actions among them — are pushed out of the viewport, and the
                delete button becomes unreachable. The bug reads as "you cannot delete tasks",
                which is how it was reported. */}
            <thead className="bg-foreground/[0.03] border-b border-border-primary">
              <tr>
                {canDelete && (
                  <th className="px-4 py-3 w-[5%]">
                    <input
                      type="checkbox"
                      aria-label="Select all tasks"
                      checked={allSelected}
                      onChange={toggleAll}
                      disabled={selectableTasks.length === 0}
                      className="accent-accent-blue"
                    />
                  </th>
                )}
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide w-[35%]">Title</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide w-[12%]">Status</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide w-[10%]">Priority</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide w-[18%]">Assignees</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide w-[12%]">Due date</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide w-[8%]">Actions</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map(task => (
                <tr
                  key={task.id}
                  onClick={() => setDetailTask(task)}
                  className="border-b border-border-primary last:border-b-0 cursor-pointer hover:bg-action-primary/[0.05] transition-colors"
                >
                  {canDelete && (
                    <td className="px-4 py-3" onClick={e => e.stopPropagation()}>
                      {/* Archived rows are not selectable — archiving them again is a
                          no-op the backend would silently swallow. */}
                      {!task.archived_at && (
                        <input
                          type="checkbox"
                          aria-label={`Select task ${task.title}`}
                          checked={selectedIds.has(task.id)}
                          onChange={() => toggleOne(task.id)}
                          className="accent-accent-blue"
                        />
                      )}
                    </td>
                  )}
                  <td className="px-4 py-3 text-xs text-text-primary font-semibold max-w-0">
                    {/* `title` gives the native tooltip with the full text on hover — the
                        truncation must never be the only place the text exists. */}
                    <span className="flex items-center gap-1.5">
                      <span className="block truncate" title={task.title}>{task.title}</span>
                      {task.archived_at && <Badge variant="default" size="sm">Archived</Badge>}
                    </span>
                  </td>
                  <td className="px-4 py-3">
                    <StatusPill status={task.status} />
                  </td>
                  <td className="px-4 py-3">
                    <PriorityPill priority={task.priority} />
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {task.assignees.length === 0
                      ? <span className="text-text-quaternary">Unassigned</span>
                      : task.assignees.map(a => a.name).join(', ')}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {task.due_date ? new Date(task.due_date).toLocaleDateString() : '—'}
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-2">
                      {canWrite && (
                        <button
                          onClick={(e) => { e.stopPropagation(); setDetailTask(task) }}
                          aria-label={`Edit ${task.title}`}
                          title="Edit"
                          className="text-text-quaternary hover:text-accent-blue transition-colors"
                        >
                          <Pencil className="w-3.5 h-3.5" />
                        </button>
                      )}
                      {canDelete && !task.archived_at && (
                        <button
                          onClick={(e) => { e.stopPropagation(); handleDelete(task) }}
                          aria-label={`Delete ${task.title}`}
                          title="Delete"
                          className="text-text-quaternary hover:text-status-error transition-colors"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Create Task Modal */}
      <Modal open={creating} onOpenChange={setCreating}>
        <ModalCloseButton />
        <div className="w-full">
          <h2 className="text-lg font-semibold text-text-primary mb-5">New Task</h2>
          <form onSubmit={handleCreateSubmit} className="space-y-4 text-base md:text-sm">
            <div className="space-y-1">
              <label htmlFor="task-title" className="text-xs font-medium text-text-secondary">Title</label>
              <input
                id="task-title"
                type="text"
                value={createForm.title}
                onChange={e => setCreateForm(f => ({ ...f, title: e.target.value }))}
                className="w-full bg-transparent border border-input rounded-md px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
                required
              />
            </div>

            <div className="space-y-1">
              <label htmlFor="task-description" className="text-xs font-medium text-text-secondary">Description</label>
              <textarea
                id="task-description"
                value={createForm.description}
                onChange={e => setCreateForm(f => ({ ...f, description: e.target.value }))}
                className="w-full bg-transparent border border-input rounded-md px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 h-20 resize-none shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
              />
            </div>

            <div className="space-y-1">
              <label className="text-xs font-medium text-text-secondary">Project</label>
              <Select value={createForm.project} onValueChange={v => setCreateForm(f => ({ ...f, project: v }))}>
                <SelectTrigger className="h-9 w-full">
                  <SelectValue placeholder="Choose project…" />
                </SelectTrigger>
                <SelectContent>
                  {projects.map(p => (
                    <SelectItem key={p.id} value={p.name} icon={<FolderKanban />} description={p.description || `Tasks in ${p.name}`}>{p.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="flex items-center gap-3">
              <div className="min-w-0 flex-1 space-y-1">
                <label className="text-xs font-medium text-text-secondary">Status</label>
                <Select value={createForm.status} onValueChange={v => setCreateForm(f => ({ ...f, status: v as TaskStatus }))}>
                  <SelectTrigger className="h-9 w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {STATUS_OPTIONS.map(status => <TaskStatusOption key={status} status={status} />)}
                  </SelectContent>
                </Select>
              </div>
              <div className="min-w-0 flex-1 space-y-1">
                <label className="text-xs font-medium text-text-secondary">Priority</label>
                <Select value={createForm.priority} onValueChange={v => setCreateForm(f => ({ ...f, priority: v as TaskPriority }))}>
                  <SelectTrigger className="h-9 w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {PRIORITY_OPTIONS.map(priority => <TaskPriorityOption key={priority} priority={priority} />)}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="space-y-1">
              <label htmlFor="task-due-date" className="text-xs font-medium text-text-secondary">Due date</label>
              <input
                id="task-due-date"
                type="date"
                value={createForm.due_date}
                onChange={e => setCreateForm(f => ({ ...f, due_date: e.target.value }))}
                className="w-full bg-transparent border border-input rounded-md px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
              />
            </div>

            <div className="flex items-center justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setCreating(false)}
                className="px-4 py-2 rounded-md border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
              >
                Cancel
              </button>
              <Button
                type="submit"
                disabled={createMut.isPending || !createForm.title.trim()}

              >
                {createMut.isPending ? 'Creating…' : 'Create'}
              </Button>
            </div>
          </form>
        </div>
      </Modal>

      {/* Task Detail Modal */}
      <Modal open={!!detailTask} ariaLabel="Task details" onOpenChange={(open) => { if (!open) setDetailTask(null) }} size="2xl">
        {detailTask && (
          <TaskDetail
            task={tasks.find(t => t.id === detailTask.id) ?? detailTask}
            onClose={() => setDetailTask(null)}
          />
        )}
      </Modal>
    </div>
  )
}
