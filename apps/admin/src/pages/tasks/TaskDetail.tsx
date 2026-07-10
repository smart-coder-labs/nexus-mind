import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { X, UserMinus, Tag } from 'lucide-react'
import { createClient } from '../../api/client'
import { useAuth, isPrivileged } from '../../auth/AuthContext'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../../components/ui/Select/Select'
import { Badge } from '../../components/ui/Badge/Badge'
import { STATUS_BADGE_VARIANT } from '../Tasks'
import type { Task } from '../../types'

const client = createClient()

interface TaskDetailProps {
  task: Task
  onClose: () => void
}

export default function TaskDetail({ task, onClose }: TaskDetailProps) {
  const { session } = useAuth()
  const qc = useQueryClient()
  const isAdmin = isPrivileged(session?.user.role)
  const permissions = session?.user.permissions ?? []
  const canWrite = isAdmin || permissions.includes('task:write')
  const canAssign = isAdmin || permissions.includes('task:assign')

  const [commentBody, setCommentBody] = useState('')
  const [labelInput, setLabelInput] = useState('')
  const [subtaskTitle, setSubtaskTitle] = useState('')
  const [specInput, setSpecInput] = useState('')

  const { data: users = [] } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    enabled: canAssign,
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
      setSpecInput('')
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
    },
  })

  const unassignMut = useMutation({
    mutationFn: (userId: string) => client.unassignTask(task.id, userId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      qc.invalidateQueries({ queryKey: ['task', task.id] })
    },
  })

  const availableAssignees = users.filter(
    u => !task.assignees.some(a => a.id === u.id),
  )

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
    if (!specInput.trim()) return
    linkSpecMut.mutate(specInput.trim())
  }

  return (
    <div className="relative bg-[#1d1d1f] rounded-[18px] border border-border-primary p-6 w-full max-w-2xl max-h-[85vh] overflow-y-auto">
      <button
        onClick={onClose}
        aria-label="Close"
        className="absolute top-4 right-4 w-8 h-8 flex items-center justify-center rounded-full bg-background-tertiary text-text-secondary hover:bg-background-secondary hover:text-text-primary transition-colors"
      >
        <X className="w-3.5 h-3.5" />
      </button>

      <div className="mb-5">
        <div className="flex items-center gap-2 mb-1">
          <Badge variant={STATUS_BADGE_VARIANT[task.status]} size="sm">{task.status}</Badge>
        </div>
        <h2 className="text-sm font-semibold text-text-primary">{task.title}</h2>
        {task.description && (
          <p className="text-xs text-text-tertiary mt-1">{task.description}</p>
        )}
      </div>

      {/* Assignees */}
      <section className="mb-6">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Assignees</h3>
        <div className="flex flex-wrap items-center gap-2 mb-2">
          {task.assignees.length === 0 && (
            <span className="text-xs text-text-quaternary">Unassigned</span>
          )}
          {task.assignees.map(a => (
            <span
              key={a.id}
              className="flex items-center gap-1.5 rounded-full border border-border-primary bg-background-tertiary px-2.5 py-1 text-xs text-text-secondary"
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
        {canAssign && (
          <Select value="" onValueChange={v => v && assignMut.mutate(v)}>
            <SelectTrigger className="w-56 h-8 text-xs" aria-label="Assignee">
              <SelectValue placeholder="Add assignee…" />
            </SelectTrigger>
            <SelectContent>
              {availableAssignees.map(u => (
                <SelectItem key={u.id} value={u.id}>{u.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </section>

      {/* Labels */}
      <section className="mb-6">
        <h3 className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">Labels</h3>
        <div className="flex flex-wrap items-center gap-2 mb-2">
          {task.labels.length === 0 && (
            <span className="text-xs text-text-quaternary">No labels</span>
          )}
          {task.labels.map(label => (
            <span
              key={label}
              className="flex items-center gap-1.5 rounded-full border border-border-primary bg-background-tertiary px-2.5 py-1 text-xs text-text-secondary"
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
          {specLinks.map(name => (
            <span
              key={name}
              className="flex items-center gap-1.5 rounded-full border border-border-primary bg-background-tertiary px-2.5 py-1 text-xs text-text-secondary"
            >
              {name}
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
          ))}
        </div>
        {canWrite && (
          <form onSubmit={handleSpecSubmit} className="flex items-center gap-2">
            <input
              id="task-detail-spec-input"
              aria-label="Link spec change"
              type="text"
              value={specInput}
              onChange={e => setSpecInput(e.target.value)}
              placeholder="openspec change name…"
              className="flex-1 bg-transparent border border-border-primary rounded-[11px] px-3 py-1.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60"
            />
            <button
              type="submit"
              disabled={!specInput.trim() || linkSpecMut.isPending}
              className="px-3 py-1.5 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50"
            >
              Link
            </button>
          </form>
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
    </div>
  )
}
