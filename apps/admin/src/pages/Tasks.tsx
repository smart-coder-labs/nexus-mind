import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Pencil, Trash2, ListTodo, List, LayoutGrid } from 'lucide-react'
import { Navigate } from 'react-router-dom'
import { createClient } from '../api/client'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import { Modal, ModalCloseButton } from '../components/ui/Modal/Modal'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import { Badge } from '../components/ui/Badge/Badge'
import { EmptyState } from '../components/ui/EmptyState/EmptyState'
import TaskDetail from './tasks/TaskDetail'
import TasksBoard from './tasks/TasksBoard'
import type { Task, TaskStatus, TaskPriority } from '../types'

type TasksView = 'list' | 'board'

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

  const [creating, setCreating] = useState(false)
  const [createForm, setCreateForm] = useState<TaskFormState>(EMPTY_FORM)

  const [detailTask, setDetailTask] = useState<Task | null>(null)
  const [view, setView] = useState<TasksView>('list')

  const filters = useMemo(
    () => ({
      project: projectFilter || undefined,
      status: statusFilter ? (statusFilter as TaskStatus) : undefined,
      // `undefined`, never `''` — the client serializes every non-null value, so an
      // empty string would go out as `?assignee=` and match no one, rendering an
      // empty list that reads as "there are no tasks".
      assignee: assigneeFilter || undefined,
    }),
    [projectFilter, statusFilter, assigneeFilter],
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

  // Populates the assignee filter. Gated on canRead like the task list itself: a
  // 403 here would trip the client's global handler and redirect the whole app to
  // /401, ejecting a user who is merely not allowed to list tasks.
  const { data: users = [] } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    enabled: canRead,
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

  const handleCreateSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!createForm.title.trim()) return
    createMut.mutate()
  }

  const handleDelete = (task: Task) => {
    if (!window.confirm(`Delete task "${task.title}"? This cannot be undone.`)) return
    deleteMut.mutate(task.id)
  }

  if (!canRead) return <Navigate to="/401" replace />

  return (
    <div className="p-6 max-w-6xl">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-base font-semibold text-text-primary">Tasks</h1>
          <p className="text-xs text-text-quaternary mt-0.5">{tasks.length} tasks</p>
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

        <div className="flex items-center gap-1 rounded-full border border-border-primary p-0.5">
          <button
            onClick={() => setView('list')}
            aria-label="List view"
            aria-pressed={view === 'list'}
            title="List view"
            className={`p-1.5 rounded-full transition-colors ${view === 'list' ? 'bg-accent-blue text-white' : 'text-text-quaternary hover:text-text-primary'}`}
          >
            <List className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => setView('board')}
            aria-label="Board view"
            aria-pressed={view === 'board'}
            title="Board view"
            className={`p-1.5 rounded-full transition-colors ${view === 'board' ? 'bg-accent-blue text-white' : 'text-text-quaternary hover:text-text-primary'}`}
          >
            <LayoutGrid className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Task list/board */}
      {isLoading ? (
        <div className="space-y-2">
          {[...Array(4)].map((_, i) => (
            <div key={i} className="rounded-[18px] bg-[#272729] border border-border-primary h-14 animate-pulse" />
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
      ) : (
        <div className="overflow-hidden border border-border-primary rounded-[18px] bg-[#272729]">
          <table className="w-full border-collapse text-left">
            <thead className="bg-[#272729]/40 border-b border-border-secondary">
              <tr>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Title</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Status</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Priority</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Assignees</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Due date</th>
                <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Actions</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map(task => (
                <tr
                  key={task.id}
                  onClick={() => setDetailTask(task)}
                  className="border-b border-border-secondary last:border-b-0 cursor-pointer hover:bg-background-tertiary/40 transition-colors"
                >
                  <td className="px-4 py-3 text-xs text-text-primary font-semibold">{task.title}</td>
                  <td className="px-4 py-3">
                    <Badge variant={STATUS_BADGE_VARIANT[task.status]} size="sm">{task.status}</Badge>
                  </td>
                  <td className="px-4 py-3">
                    <Badge variant={PRIORITY_BADGE_VARIANT[task.priority]} size="sm">{task.priority}</Badge>
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
                      {canDelete && (
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
        <div className="bg-[#1d1d1f] rounded-[18px] border border-border-primary p-6 w-full max-w-md">
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
