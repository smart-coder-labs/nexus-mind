import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Pencil, Trash2, ListTodo, List, LayoutGrid, ChartGantt, ListChecks } from 'lucide-react'
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

// Exact hex accents used by the target mockup's status/priority chips and the
// distribution bar (delta 3/5). Kept separate from STATUS_BADGE_VARIANT /
// PRIORITY_BADGE_VARIANT above — those still back the Badge component used by
// TaskDetail.tsx and sdd/ChangeDetail.tsx, which this change does not touch.
export const STATUS_COLORS: Record<TaskStatus, string> = {
  backlog: '#94a3b8',
  todo: '#64748b',
  in_progress: '#a78bfa',
  in_review: '#facc15',
  done: '#34d399',
  cancelled: '#f87171',
}

export const PRIORITY_COLORS: Record<TaskPriority, string> = {
  low: '#94a3b8',
  medium: '#60a5fa',
  high: '#facc15',
  urgent: '#f87171',
}

/** Subtle status chip: tinted background at ~14% of the status color. */
export function StatusPill({ status }: { status: TaskStatus }) {
  const color = STATUS_COLORS[status]
  return (
    <span
      className="inline-flex items-center rounded-full px-2.5 py-0.5 text-[11px] font-semibold whitespace-nowrap"
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
      className="inline-flex items-center rounded-full px-2.5 py-0.5 text-[11px] font-bold whitespace-nowrap"
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
            className="w-11 h-11 rounded-[13px] bg-status-success/10 flex items-center justify-center shrink-0"
          >
            <ListChecks className="w-5 h-5 text-status-success" />
          </div>
          <div>
            <h1 className="text-base font-semibold text-text-primary">Tasks</h1>
            <p className="text-xs text-text-quaternary mt-0.5">{tasks.length} tasks</p>
          </div>
        </div>
        {canWrite && (
          <button
            onClick={() => { setCreateForm({ ...EMPTY_FORM, project: projectFilter || projects[0]?.name || '' }); setCreating(true) }}
            className="bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold flex items-center gap-1.5"
          >
            <Plus className="w-3.5 h-3.5" />
            New Task
          </button>
        )}
      </div>

      {/* Stat tiles + status distribution — derived from the already-fetched,
          filter-scoped task list (same data backing "N tasks" above). No
          separate endpoint, no fabricated numbers. */}
      <TasksStats tasks={tasks} />

      {/* Filters */}
      <div className="flex items-center justify-between gap-3 mb-4">
        <div className="flex items-center gap-3">
          <Select value={projectFilter} onValueChange={setProjectFilter}>
            <SelectTrigger className="w-48" aria-label="Project">
              <SelectValue placeholder="All projects" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">All projects</SelectItem>
              {projects.map(p => (
                <SelectItem key={p.id} value={p.name}>{p.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={statusFilter} onValueChange={setStatusFilter}>
            <SelectTrigger className="w-40" aria-label="Status">
              <SelectValue placeholder="All statuses" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">All statuses</SelectItem>
              {STATUS_OPTIONS.map(s => (
                <SelectItem key={s} value={s}>{s}</SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={assigneeFilter} onValueChange={setAssigneeFilter}>
            <SelectTrigger className="w-48" aria-label="Assignee">
              <SelectValue placeholder="All assignees" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">All assignees</SelectItem>
              <SelectItem value="me">Assigned to me</SelectItem>
              {users.map(u => (
                <SelectItem key={u.id} value={u.id}>{u.name || u.email}</SelectItem>
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
        <div className="flex items-center justify-between gap-3 mb-3 rounded-[14px] border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] px-4 py-2">
          <span className="text-xs text-text-secondary">
            {selectedTasks.length} selected
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setSelectedIds(new Set())}
              className="px-3 py-1.5 rounded-full border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
            >
              Clear
            </button>
            <button
              onClick={handleBulkDelete}
              disabled={bulkDeleteMut.isPending}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-status-error text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
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
        <p className="text-[10px] text-text-quaternary mb-3">
          Archived tasks are shown for reference. The API exposes no task-restore
          endpoint, so they cannot be restored from the admin.
        </p>
      )}

      {/* Task list/board */}
      {isLoading ? (
        <div className="space-y-2">
          {[...Array(4)].map((_, i) => (
            <div key={i} className="rounded-[18px] border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] h-14 animate-pulse" />
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
        <div className="overflow-hidden rounded-[18px] border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]">
          <table className="w-full table-fixed border-collapse text-left">
            {/* table-fixed: without it a long title stretches the Title column until the
                later columns — Actions among them — are pushed out of the viewport, and the
                delete button becomes unreachable. The bug reads as "you cannot delete tasks",
                which is how it was reported. */}
            <thead className="bg-white/[0.03] border-b border-white/[0.06]">
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
                  className="border-b border-white/[0.05] last:border-b-0 cursor-pointer hover:bg-accent-blue/[0.05] transition-colors"
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
        <div className="rounded-[18px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] p-6 w-full max-w-md">
          <h2 className="text-xs font-semibold text-text-primary mb-4">New Task</h2>
          <form onSubmit={handleCreateSubmit} className="space-y-4 text-xs">
            <div className="space-y-1">
              <label htmlFor="task-title" className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Title</label>
              <input
                id="task-title"
                type="text"
                value={createForm.title}
                onChange={e => setCreateForm(f => ({ ...f, title: e.target.value }))}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60"
                required
              />
            </div>

            <div className="space-y-1">
              <label htmlFor="task-description" className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Description</label>
              <textarea
                id="task-description"
                value={createForm.description}
                onChange={e => setCreateForm(f => ({ ...f, description: e.target.value }))}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 h-20 resize-none"
              />
            </div>

            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Project</label>
              <Select value={createForm.project} onValueChange={v => setCreateForm(f => ({ ...f, project: v }))}>
                <SelectTrigger className="h-8 text-xs">
                  <SelectValue placeholder="Choose project…" />
                </SelectTrigger>
                <SelectContent>
                  {projects.map(p => (
                    <SelectItem key={p.id} value={p.name}>{p.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="flex items-center gap-3">
              <div className="flex-1 space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Status</label>
                <Select value={createForm.status} onValueChange={v => setCreateForm(f => ({ ...f, status: v as TaskStatus }))}>
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {STATUS_OPTIONS.map(s => (
                      <SelectItem key={s} value={s}>{s}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="flex-1 space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Priority</label>
                <Select value={createForm.priority} onValueChange={v => setCreateForm(f => ({ ...f, priority: v as TaskPriority }))}>
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {PRIORITY_OPTIONS.map(p => (
                      <SelectItem key={p} value={p}>{p}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="space-y-1">
              <label htmlFor="task-due-date" className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Due date</label>
              <input
                id="task-due-date"
                type="date"
                value={createForm.due_date}
                onChange={e => setCreateForm(f => ({ ...f, due_date: e.target.value }))}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60"
              />
            </div>

            <div className="flex items-center justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setCreating(false)}
                className="px-4 py-2 rounded-full border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={createMut.isPending || !createForm.title.trim()}
                className="px-4 py-2 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
              >
                {createMut.isPending ? 'Creating…' : 'Create'}
              </button>
            </div>
          </form>
        </div>
      </Modal>

      {/* Task Detail Modal */}
      <Modal open={!!detailTask} onOpenChange={(open) => { if (!open) setDetailTask(null) }} size="lg">
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
