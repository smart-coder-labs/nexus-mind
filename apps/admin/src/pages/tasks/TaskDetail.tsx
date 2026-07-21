import { useEffect, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { X, UserMinus, Tag, Trash2 } from 'lucide-react'
import { Link } from 'react-router-dom'
import { createClient } from '../../api/client'
import { useAuth, isPrivileged } from '../../auth/AuthContext'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../../components/ui/Select/Select'
import { Badge } from '../../components/ui/Badge/Badge'
import { STATUS_BADGE_VARIANT, STATUS_OPTIONS, PRIORITY_OPTIONS } from '../Tasks'
import type { Task, TaskStatus, TaskPriority } from '../../types'

const client = createClient()

interface TaskDetailProps {
  task: Task
  onClose: () => void
}

interface TaskEditFormState {
  title: string
  description: string
  status: TaskStatus
  priority: TaskPriority
  due_date: string
}

function formStateFromTask(t: Task): TaskEditFormState {
  return {
    title: t.title,
    description: t.description ?? '',
    status: t.status,
    priority: t.priority,
    due_date: t.due_date ?? '',
  }
}

export default function TaskDetail({ task, onClose }: TaskDetailProps) {
  const { session } = useAuth()
  const qc = useQueryClient()
  const isAdmin = isPrivileged(session?.user.role)
  const permissions = session?.user.permissions ?? []
  const canWrite = isAdmin || permissions.includes('task:write')
  const canAssign = isAdmin || permissions.includes('task:assign')
  const canDelete = isAdmin || permissions.includes('task:delete')
  // The spec-link resolution below hits `GET /v1/sdd/changes`, which requires
  // `sdd:read`. Un-gated, a caller without it gets a 403 — and the client's global
  // handler bounces the whole admin to /401. So gate the query, don't just catch it.
  const canReadSdd = isAdmin || permissions.includes('sdd:read')

  // The two people-listing endpoints — `GET /v1/users` (api/users.rs) and
  // `GET /v1/projects/:id/members` (api/admin.rs) — both gate on
  // `auth.role.is_privileged()`, NOT on a permission string. So `task:assign` alone
  // does not buy you either of them: a plain member holding it still gets a 403, and
  // the client's global handler turns that 403 into window.location.replace('/401'),
  // ejecting them from the whole admin for opening a task. The gate is the ROLE.
  const canListPeople = isAdmin && canAssign

  const [commentBody, setCommentBody] = useState('')
  const [labelInput, setLabelInput] = useState('')
  const [subtaskTitle, setSubtaskTitle] = useState('')
  const [selectedSpec, setSelectedSpec] = useState('')
  const [selectedAssignee, setSelectedAssignee] = useState('')

  // The list view (list_tasks) returns tasks with empty assignees/labels to
  // avoid N+1 queries. Fetch the hydrated task so assign/label mutations —
  // which invalidate ['task', task.id] — actually reflect in this view.
  const { data: fullTask } = useQuery({
    queryKey: ['task', task.id],
    queryFn: () => client.getTask(task.id),
  })

  const t = fullTask ?? task

  const [editForm, setEditForm] = useState<TaskEditFormState>(() => formStateFromTask(t))

  // Seed the edit form only when the task identity changes (opening a different
  // task), NOT on every fullTask refetch — otherwise adding a label/assignee mid-edit
  // would refetch and silently clobber in-progress Title/Description edits. The
  // editable fields (title/desc/status/priority/due) are already present on the list
  // item, so seeding from `task` needs no hydration.
  useEffect(() => {
    setEditForm(formStateFromTask(task))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [task.id])

  // `tasks.project` is a NAME, not a project_id — so the members lookup needs a
  // name → id hop through the project list first. `GET /v1/projects` needs no
  // permission (it self-scopes to what the caller can see), so it is safe ungated.
  const { data: projects = [], isSuccess: projectsResolved } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
  })

  const projectId = projects.find(p => p.name === t.project)?.id ?? null

  const { data: projectMembers = [] } = useQuery({
    queryKey: ['project-members', projectId],
    queryFn: () => client.listProjectMembers(projectId as string),
    enabled: canListPeople && !!projectId,
  })

  // An unregistered project NAME is legal and deliberate — it is how org-shared and
  // unregistered projects stay visible (list_tasks' viewer filter admits any task
  // whose project has no `projects` row). Such a task has no membership to read, and
  // an empty assignee dropdown on a legitimate task reads as a bug — so the fallback
  // is every org user, not an empty list. Wait for `projectsResolved` before deciding
  // the name is unregistered: until the list lands, `projectId` is null merely
  // because nothing has loaded.
  const { data: orgUsers = [] } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    enabled: canListPeople && projectsResolved && !projectId,
  })

  const { data: comments = [] } = useQuery({
    queryKey: ['task-comments', task.id],
    queryFn: () => client.listTaskComments(task.id),
  })

  const { data: subtasks = [] } = useQuery({
    queryKey: ['task-subtasks', task.id],
    queryFn: () => client.listTaskSubtasks(task.id),
  })

  const { data: specLinks = [] } = useQuery({
    queryKey: ['task-spec-links', task.id],
    queryFn: () => client.listTaskSpecLinks(task.id),
  })

  // Doubles as (a) the resolver that turns each spec-link name into a real link with
  // its phase — a dangling name degrades to inert text rather than a broken nav
  // target — and (b) the option list for the link picker below.
  //
  // `include_archived` is deliberate: an archived change is still a valid link
  // target. A change is archived AFTER its tasks exist, so if archiving dropped it
  // from the picker you would lose traceability exactly when the work completes.
  const sddChangeParams = { project: t.project, include_archived: true }
  const { data: sddChanges = [] } = useQuery({
    queryKey: ['sdd-changes', sddChangeParams],
    queryFn: () => client.listSddChanges(sddChangeParams),
    enabled: canReadSdd,
  })

  const addCommentMut = useMutation({
    mutationFn: (body: string) => client.addTaskComment(task.id, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['task-comments', task.id] })
      setCommentBody('')
    },
  })

  const addLabelMut = useMutation({
    mutationFn: (label: string) => client.addTaskLabel(task.id, label),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      qc.invalidateQueries({ queryKey: ['task', task.id] })
      setLabelInput('')
    },
  })

  const removeLabelMut = useMutation({
    mutationFn: (label: string) => client.removeTaskLabel(task.id, label),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      qc.invalidateQueries({ queryKey: ['task', task.id] })
    },
  })

  const createSubtaskMut = useMutation({
    mutationFn: (title: string) =>
      client.createTask({ project: task.project, title, parent_id: task.id }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['task-subtasks', task.id] })
      qc.invalidateQueries({ queryKey: ['tasks'] })
      setSubtaskTitle('')
    },
  })

  const linkSpecMut = useMutation({
    mutationFn: (specChangeName: string) => client.linkTaskSpec(task.id, specChangeName),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['task-spec-links', task.id] })
      setSelectedSpec('')
    },
  })

  const deleteMut = useMutation({
    mutationFn: () => client.deleteTask(task.id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      onClose()
    },
  })

  const unlinkSpecMut = useMutation({
    mutationFn: (specChangeName: string) => client.unlinkTaskSpec(task.id, specChangeName),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['task-spec-links', task.id] }),
  })

  const assignMut = useMutation({
    mutationFn: (userId: string) => client.assignTask(task.id, [userId]),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      qc.invalidateQueries({ queryKey: ['task', task.id] })
      setSelectedAssignee('')
    },
  })

  const unassignMut = useMutation({
    mutationFn: (userId: string) => client.unassignTask(task.id, userId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      qc.invalidateQueries({ queryKey: ['task', task.id] })
    },
  })

  const updateMut = useMutation({
    mutationFn: () =>
      client.updateTask(task.id, {
        title: editForm.title,
        description: editForm.description || undefined,
        status: editForm.status,
        priority: editForm.priority,
        due_date: editForm.due_date || undefined,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      qc.invalidateQueries({ queryKey: ['task', task.id] })
    },
  })

  // Project members when the project name resolves to a real project; every org user
  // when it does not. NOTE: a ProjectMember's `id` is the MEMBERSHIP row id — the
  // user is `user_id`, and that is what assignTask takes.
  const assigneePool: { id: string; name: string }[] = projectId
    ? projectMembers.map(m => ({ id: m.user_id, name: m.name || m.email }))
    : orgUsers.map(u => ({ id: u.id, name: u.name || u.email }))

  const availableAssignees = assigneePool.filter(
    c => !(t.assignees ?? []).some(a => a.id === c.id),
  )

  // A change already linked to this task is not a link target.
  const availableSpecs = sddChanges.filter(c => !specLinks.includes(c.name))

  const handleCommentSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!commentBody.trim()) return
    addCommentMut.mutate(commentBody)
  }

  const handleLabelSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!labelInput.trim()) return
    addLabelMut.mutate(labelInput.trim())
  }

  const handleSubtaskSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!subtaskTitle.trim()) return
    createSubtaskMut.mutate(subtaskTitle.trim())
  }

  const handleSpecSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!selectedSpec) return
    linkSpecMut.mutate(selectedSpec)
  }

  const handleDelete = () => {
    // The FK `tasks.parent_id` is ON DELETE CASCADE, but that only fires on a hard
    // DELETE and the API never issues one: soft_delete_task is a plain
    // `UPDATE tasks SET archived_at = …`. So subtasks are NOT archived with their
    // parent — the backend asserts exactly this in
    // `soft_delete_parent_does_not_cascade_to_subtasks`. Say what will actually
    // happen; promising a cascade that does not occur is how a user ends up with
    // orphans they never looked for.
    // `subtask_count` rides along on the task itself, so the warning is correct even
    // if the subtask list query has not settled when the user hits Delete.
    const n = subtasks.length > 0 ? subtasks.length : (t.subtask_count ?? 0)
    const note = n > 0
      ? ` Its ${n} subtask${n === 1 ? '' : 's'} ${n === 1 ? 'is' : 'are'} NOT archived with it — ${n === 1 ? 'it remains' : 'they remain'} in the list.`
      : ''
    // Soft delete: the row survives with `archived_at` set. But the API exposes no
    // task-restore endpoint, so "can be restored" would be a promise the admin cannot
    // keep — say what is true instead. ("Show archived" on the list makes it visible.)
    if (!window.confirm(
      `Archive task "${t.title}"?${note} It is removed from the list. The row survives and stays visible under "Show archived", but the API has no restore endpoint.`,
    )) return
    deleteMut.mutate()
  }

  const handleEditSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!editForm.title.trim()) return
    updateMut.mutate()
  }

  const handleAddAssignee = () => {
    if (!selectedAssignee) return
    assignMut.mutate(selectedAssignee)
  }

  return (
    <div className="relative rounded-[18px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] p-6 w-full max-w-2xl max-h-[85vh] overflow-y-auto">
      <button
        onClick={onClose}
        aria-label="Close"
        className="absolute top-4 right-4 w-8 h-8 flex items-center justify-center rounded-full bg-white/[0.06] text-text-secondary hover:bg-white/[0.10] hover:text-text-primary transition-colors"
      >
        <X className="w-3.5 h-3.5" />
      </button>

      <div className="mb-5">
        {canWrite ? (
          <form id="task-edit-form" onSubmit={handleEditSubmit} className="space-y-4 text-xs">
            <div className="space-y-1">
              <label htmlFor="task-detail-title" className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Title</label>
              <input
                id="task-detail-title"
                type="text"
                value={editForm.title}
                onChange={e => setEditForm(f => ({ ...f, title: e.target.value }))}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60"
                required
              />
            </div>

            <div className="space-y-1">
              <label htmlFor="task-detail-description" className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Description</label>
              <textarea
                id="task-detail-description"
                value={editForm.description}
                onChange={e => setEditForm(f => ({ ...f, description: e.target.value }))}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 h-20 resize-none"
              />
            </div>

            <div className="flex items-center gap-3">
              <div className="flex-1 space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Status</label>
                <Select value={editForm.status} onValueChange={v => setEditForm(f => ({ ...f, status: v as TaskStatus }))}>
                  <SelectTrigger className="h-8 text-xs" aria-label="Status">
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
                <Select value={editForm.priority} onValueChange={v => setEditForm(f => ({ ...f, priority: v as TaskPriority }))}>
                  <SelectTrigger className="h-8 text-xs" aria-label="Priority">
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
              <label htmlFor="task-detail-due-date" className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Due date</label>
              <input
                id="task-detail-due-date"
                type="date"
                value={editForm.due_date}
                onChange={e => setEditForm(f => ({ ...f, due_date: e.target.value }))}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60"
              />
            </div>
          </form>
        ) : (
          <>
            <div className="flex items-center gap-2 mb-1">
              <Badge variant={STATUS_BADGE_VARIANT[t.status]} size="sm">{t.status}</Badge>
            </div>
            <h2 className="text-sm font-semibold text-text-primary">{t.title}</h2>
            {t.description && (
              <p className="text-xs text-text-tertiary mt-1">{t.description}</p>
            )}
          </>
        )}
      </div>

      {/* Project — READ-ONLY.
          The backend's PatchTaskRequest (models/types.rs) carries exactly
          title/description/status/priority/due_date/sprint_id. There is no `project`
          field and `patch_task` has no branch that would write one, so a project
          picker here would be a control that silently does nothing. Show the value,
          say why it is fixed, and do not fake the mutation. */}
      <section className="mb-6" data-testid="task-detail-project">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Project</h3>
        <div className="flex items-center gap-2">
          <span className="rounded-full border border-white/[0.09] bg-white/[0.06] px-2.5 py-1 text-xs text-text-secondary">
            {t.project}
          </span>
        </div>
        <p className="text-[10px] text-text-quaternary mt-1.5">
          A task cannot be moved between projects from the admin — the API's task
          patch does not accept a project.
        </p>
      </section>

      {/* Assignees */}
      <section className="mb-6">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Assignees</h3>
        <div className="flex flex-wrap items-center gap-2 mb-2">
          {(t.assignees ?? []).length === 0 && (
            <span className="text-xs text-text-quaternary">Unassigned</span>
          )}
          {(t.assignees ?? []).map(a => (
            <span
              key={a.id}
              className="flex items-center gap-1.5 rounded-full border border-white/[0.09] bg-white/[0.06] px-2.5 py-1 text-xs text-text-secondary"
            >
              {a.name}
              {canAssign && (
                <button
                  onClick={() => unassignMut.mutate(a.id)}
                  aria-label={`Unassign ${a.name}`}
                  title="Unassign"
                  className="text-text-quaternary hover:text-status-error transition-colors"
                >
                  <UserMinus className="w-3 h-3" />
                </button>
              )}
            </span>
          ))}
        </div>
        {canAssign && canListPeople && (
          <div className="flex items-center gap-2">
            <Select value={selectedAssignee} onValueChange={setSelectedAssignee}>
              <SelectTrigger className="w-56 h-8 text-xs" aria-label="Assignee">
                <SelectValue placeholder="Add assignee…" />
              </SelectTrigger>
              <SelectContent>
                {availableAssignees.map(u => (
                  <SelectItem key={u.id} value={u.id}>{u.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            <button
              type="button"
              onClick={handleAddAssignee}
              aria-label="Add assignee"
              disabled={!selectedAssignee || assignMut.isPending}
              className="px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              Add
            </button>
          </div>
        )}
        {canAssign && !canListPeople && (
          // They may assign, but neither people-listing endpoint will serve them —
          // both are privileged-only. Say so instead of rendering an empty picker.
          <p className="text-[10px] text-text-quaternary">
            Listing people to assign requires an admin role.
          </p>
        )}
      </section>

      {/* Labels */}
      <section className="mb-6">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Labels</h3>
        <div className="flex flex-wrap items-center gap-2 mb-2">
          {(t.labels ?? []).length === 0 && (
            <span className="text-xs text-text-quaternary">No labels</span>
          )}
          {(t.labels ?? []).map(label => (
            <span
              key={label}
              className="flex items-center gap-1.5 rounded-full border border-white/[0.09] bg-white/[0.06] px-2.5 py-1 text-xs text-text-secondary"
            >
              <Tag className="w-3 h-3" />
              {label}
              {canWrite && (
                <button
                  onClick={() => removeLabelMut.mutate(label)}
                  aria-label={`Remove label ${label}`}
                  title="Remove label"
                  className="text-text-quaternary hover:text-status-error transition-colors"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
            </span>
          ))}
        </div>
        {canWrite && (
          <form onSubmit={handleLabelSubmit} className="flex items-center gap-2">
            <input
              id="task-detail-label-input"
              aria-label="Add label"
              type="text"
              value={labelInput}
              onChange={e => setLabelInput(e.target.value)}
              placeholder="New label…"
              className="flex-1 bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60"
            />
            <button
              type="submit"
              disabled={!labelInput.trim() || addLabelMut.isPending}
              className="px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              Add
            </button>
          </form>
        )}
      </section>

      {/* Subtasks */}
      <section className="mb-6">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Subtasks</h3>
        {subtasks.length === 0 ? (
          <p className="text-xs text-text-quaternary mb-2">No subtasks</p>
        ) : (
          <ul className="space-y-1.5 mb-2">
            {subtasks.map(st => (
              <li
                key={st.id}
                className="flex items-center justify-between rounded-[11px] border border-border-secondary px-3 py-2"
              >
                <span className="text-xs text-text-primary">{st.title}</span>
                <Badge variant={STATUS_BADGE_VARIANT[st.status]} size="sm">{st.status}</Badge>
              </li>
            ))}
          </ul>
        )}
        {canWrite && (
          <form onSubmit={handleSubtaskSubmit} className="flex items-center gap-2">
            <input
              id="task-detail-subtask-input"
              aria-label="New subtask title"
              type="text"
              value={subtaskTitle}
              onChange={e => setSubtaskTitle(e.target.value)}
              placeholder="New subtask title…"
              className="flex-1 bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60"
            />
            <button
              type="submit"
              disabled={!subtaskTitle.trim() || createSubtaskMut.isPending}
              className="px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              Add
            </button>
          </form>
        )}
      </section>

      {/* Spec links */}
      <section className="mb-6">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Linked Specs</h3>
        <div className="flex flex-wrap items-center gap-2 mb-2">
          {specLinks.length === 0 && (
            <span className="text-xs text-text-quaternary">No linked specs</span>
          )}
          {specLinks.map(name => {
            const change = sddChanges.find(c => c.name === name)
            return (
            <span
              key={name}
              className="flex items-center gap-1.5 rounded-full border border-white/[0.09] bg-white/[0.06] px-2.5 py-1 text-xs text-text-secondary"
            >
              {change ? (
                <Link
                  to={`/sdd?change=${encodeURIComponent(name)}`}
                  className="text-text-secondary hover:text-accent-blue transition-colors"
                >
                  {name}
                </Link>
              ) : (
                // Dangling link (e.g. after a rename): shown, but not navigable.
                name
              )}
              {change && <Badge variant="primary" size="sm">{change.phase}</Badge>}
              {canWrite && (
                <button
                  onClick={() => unlinkSpecMut.mutate(name)}
                  aria-label={`Unlink ${name}`}
                  title="Unlink"
                  className="text-text-quaternary hover:text-status-error transition-colors"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
            </span>
            )
          })}
        </div>
        {/* A spec link is a FOREIGN KEY, so it is picked, never typed. It used to be a
            free-text box, which was survivable only while the backend validated the
            name against a filesystem that does not exist in production and so accepted
            anything. It now validates against `sdd_changes` and 422s on an unknown
            name — a typo is no longer silent, and a blind text field is no longer
            defensible. Without `sdd:read` we cannot list the changes to choose from,
            so the picker is withheld rather than shown empty. */}
        {canWrite && canReadSdd && (
          <form onSubmit={handleSpecSubmit} className="flex items-center gap-2">
            <Select value={selectedSpec} onValueChange={setSelectedSpec}>
              <SelectTrigger className="flex-1 h-8 text-xs" aria-label="Link spec change">
                <SelectValue placeholder="Choose a change…" />
              </SelectTrigger>
              <SelectContent>
                {availableSpecs.map(c => (
                  <SelectItem key={c.id} value={c.name}>
                    <span className="flex items-center gap-1.5">
                      {c.name}
                      <Badge variant="primary" size="sm">{c.phase}</Badge>
                      {c.archived_at && <Badge variant="default" size="sm">archived</Badge>}
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <button
              type="submit"
              disabled={!selectedSpec || linkSpecMut.isPending}
              className="px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              Link
            </button>
          </form>
        )}
        {canWrite && !canReadSdd && (
          <p className="text-[10px] text-text-quaternary">
            Linking a spec change requires the sdd:read permission.
          </p>
        )}
      </section>

      {/* Comments */}
      <section>
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Comments</h3>
        {comments.length === 0 ? (
          <p className="text-xs text-text-quaternary mb-2">No comments yet</p>
        ) : (
          <ul className="space-y-3 mb-3">
            {comments.map(c => (
              <li key={c.id} className="rounded-[11px] border border-border-secondary px-3 py-2">
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs font-semibold text-text-primary">{c.author_name}</span>
                  <span className="text-[10px] text-text-quaternary">
                    {new Date(c.created_at).toLocaleString()}
                  </span>
                </div>
                <p className="text-xs text-text-secondary">{c.body}</p>
              </li>
            ))}
          </ul>
        )}
        {canWrite && (
          <form onSubmit={handleCommentSubmit} className="flex items-center gap-2">
            <input
              id="task-detail-comment-input"
              aria-label="Add a comment"
              type="text"
              value={commentBody}
              onChange={e => setCommentBody(e.target.value)}
              placeholder="Add a comment…"
              className="flex-1 bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60"
            />
            <button
              type="submit"
              disabled={!commentBody.trim() || addCommentMut.isPending}
              className="px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              Post
            </button>
          </form>
        )}
      </section>

      {(canWrite || canDelete) && (
        <div className="flex items-center justify-between pt-4 mt-2 border-t border-border-primary">
          {canDelete ? (
            <button
              type="button"
              onClick={handleDelete}
              aria-label="Delete task"
              disabled={deleteMut.isPending}
              className="flex items-center gap-1.5 px-3 py-2 rounded-full border border-border-primary text-xs font-semibold text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-50"
            >
              <Trash2 className="w-3.5 h-3.5" />
              {deleteMut.isPending ? 'Deleting…' : 'Delete'}
            </button>
          ) : (
            <span />
          )}
          {canWrite && (
            <button
              type="submit"
              form="task-edit-form"
              disabled={updateMut.isPending || !editForm.title.trim()}
              className="px-4 py-2 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              {updateMut.isPending ? 'Saving…' : 'Save'}
            </button>
          )}
        </div>
      )}
    </div>
  )
}
