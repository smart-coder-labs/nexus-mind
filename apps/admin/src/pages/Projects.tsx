import { useMemo, useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import {
  FolderGit, Plus, Users, UserPlus, UserMinus,
  FolderOpen, ChevronRight, ChevronDown, Brain, GitBranch, Loader2,
  Archive, RotateCcw,
} from 'lucide-react'
import { cn } from '../lib/utils'
import {
  Modal, ModalCloseButton,
} from '../components/ui/Modal/Modal'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import type { ProjectMember, ProjectEventOverrides, User as UserType } from '../types'

// ─── Three-way toggle: null (inherit) | true (on) | false (off) ──────────────

type TriState = boolean | null

interface ThreeWayToggleProps {
  value: TriState
  onChange: (next: TriState) => void
}

function ThreeWayToggle({ value, onChange }: ThreeWayToggleProps) {
  const cycle = () => {
    if (value === null) onChange(true)
    else if (value === true) onChange(false)
    else onChange(null)
  }

  if (value === null) {
    return (
      <button
        onClick={cycle}
        className="border border-border-secondary rounded-[8px] px-2.5 py-1 text-[11px] text-text-quaternary hover:text-text-tertiary transition-colors"
        title="Inherits org setting — click to override"
      >
        Inherit
      </button>
    )
  }
  if (value === true) {
    return (
      <button
        onClick={cycle}
        className="border border-status-success/30 rounded-[8px] px-2.5 py-1 text-[11px] text-status-success bg-status-success/10 font-semibold"
        title="Enabled for this project — click to disable"
      >
        On
      </button>
    )
  }
  return (
    <button
      onClick={cycle}
      className="border border-status-error/30 rounded-[8px] px-2.5 py-1 text-[11px] text-status-error bg-status-error/10 font-semibold"
      title="Disabled for this project — click to clear override"
    >
      Off
    </button>
  )
}

// ─── Saved badge: shows "Saved" for 2s then disappears ──────────────────────

function SavedBadge() {
  const [visible, setVisible] = useState(true)
  useEffect(() => {
    const t = setTimeout(() => setVisible(false), 2000)
    return () => clearTimeout(t)
  }, [])
  if (!visible) return null
  return <span className="text-[10px] text-status-success">Saved</span>
}

const EVENT_KEYS: { key: keyof ProjectEventOverrides; label: string }[] = [
  { key: 'resolve_issues', label: 'Resolve Issues' },
  { key: 'review_prs', label: 'Review PRs' },
  { key: 'respond_comments', label: 'Respond to Comments' },
  { key: 'auto_index', label: 'Auto Index' },
  { key: 'scanner', label: 'Scanner' },
]

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  return `${Math.floor(hrs / 24)}d ago`
}

// ─── Inline Members Panel ────────────────────────────────────────────────────

interface MembersPanelProps {
  projectId: string
  projectName: string
  client: ReturnType<typeof createClient>
  users: UserType[] | undefined
  usersLoading: boolean
  allAvailableRoles: string[]
}

function MembersPanel({
  projectId,
  projectName,
  client,
  users,
  usersLoading,
  allAvailableRoles,
}: MembersPanelProps) {
  const qc = useQueryClient()
  const [addUserId, setAddUserId] = useState('')
  const [addRole, setAddRole] = useState('viewer')
  const [addError, setAddError] = useState('')
  const [addSaved, setAddSaved] = useState(false)

  // Bulk add state
  const [bulkMode, setBulkMode] = useState(false)
  const [bulkInput, setBulkInput] = useState('')
  const [bulkProgress, setBulkProgress] = useState<string | null>(null)
  const [bulkResult, setBulkResult] = useState<{ added: number; failed: string[] } | null>(null)

  // ── Project event override state ───────────────────────────────────────────
  const { data: overridesData } = useQuery({
    queryKey: ['project-settings', projectId],
    queryFn: () => client.getProjectSettings(projectId),
  })

  const overridesMut = useMutation({
    mutationFn: (overrides: ProjectEventOverrides) =>
      client.updateProjectSettings(projectId, overrides),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['project-settings', projectId] })
    },
  })

  const currentOverrides: ProjectEventOverrides = overridesData ?? {}

  const { data: stats } = useQuery({
    queryKey: ['project-stats', projectId],
    queryFn: () => client.getProjectStats(projectId),
  })

  const handleOverrideChange = (key: keyof ProjectEventOverrides, value: boolean | null) => {
    const next: ProjectEventOverrides = { ...currentOverrides }
    if (value === null) {
      delete next[key]
    } else {
      next[key] = value
    }
    overridesMut.mutate(next)
  }

  const { data: members, isLoading: membersLoading } = useQuery({
    queryKey: ['project-members', projectId],
    queryFn: () => client.listProjectMembers(projectId),
  })

  const addMut = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      client.upsertProjectMember(projectId, userId, role),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['project-members', projectId] })
      // also invalidate the sheet query key used elsewhere
      qc.invalidateQueries({ queryKey: ['projects', projectId, 'members'] })
      setAddUserId('')
      setAddRole('viewer')
      setAddError('')
      setAddSaved(true)
      setTimeout(() => setAddSaved(false), 2000)
    },
    onError: (err: any) => setAddError(err.message || 'Failed to add member'),
  })

  const removeMut = useMutation({
    mutationFn: (userId: string) => client.deleteProjectMember(projectId, userId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['project-members', projectId] })
      qc.invalidateQueries({ queryKey: ['projects', projectId, 'members'] })
    },
  })

  const memberIds = useMemo(() => new Set((members ?? []).map((m: ProjectMember) => m.user_id)), [members])
  const availableUsers = useMemo(
    () => (users ?? []).filter(u => !memberIds.has(u.id) && u.status === 'active'),
    [users, memberIds],
  )

  const handleAdd = (e: React.FormEvent) => {
    e.preventDefault()
    if (!addUserId) { setAddError('Please select a user.'); return }
    addMut.mutate({ userId: addUserId, role: addRole })
  }

  const handleBulkAdd = async () => {
    const ids = bulkInput.split('\n').map(l => l.trim()).filter(Boolean)
    if (ids.length === 0) return
    let added = 0
    const failures: string[] = []
    for (let i = 0; i < ids.length; i++) {
      setBulkProgress(`Adding ${i + 1} of ${ids.length}…`)
      try {
        await client.upsertProjectMember(projectId, ids[i], addRole)
        added++
      } catch (err: any) {
        const msg = err?.message ?? 'unknown error'
        failures.push(`${ids[i]} (${msg})`)
      }
    }
    setBulkProgress(null)
    qc.invalidateQueries({ queryKey: ['project-members', projectId] })
    qc.invalidateQueries({ queryKey: ['projects', projectId, 'members'] })
    setBulkResult({ added, failed: failures })
    setBulkInput('')
  }

  return (
    <div className="bg-[#272729]/40 rounded-b-[18px] border border-t-0 border-border-primary px-5 pb-5 pt-4 space-y-4">
      {stats && (
        <div className="flex items-center gap-6 pb-4 mb-4 border-b border-border-secondary/40">
          <div>
            <p className="text-lg font-semibold text-text-primary">{stats.total_memories}</p>
            <p className="text-[10px] text-text-quaternary">Total memories</p>
          </div>
          <div>
            <p className="text-lg font-semibold text-text-primary">{stats.memories_this_week}</p>
            <p className="text-[10px] text-text-quaternary">This week</p>
          </div>
          {stats.last_memory_at && (
            <div>
              <p className="text-xs font-semibold text-text-secondary">{relativeTime(stats.last_memory_at)}</p>
              <p className="text-[10px] text-text-quaternary">Last activity</p>
            </div>
          )}
          {stats.top_tags.length > 0 && (
            <div className="flex flex-wrap gap-1 ml-auto">
              {stats.top_tags.map(tag => (
                <span key={tag} className="text-[10px] px-1.5 py-0.5 rounded-[5px] bg-white/[0.04] text-text-quaternary border border-border-secondary/50">{tag}</span>
              ))}
            </div>
          )}
        </div>
      )}
      {/* Members list */}
      <div className="space-y-1">
        <span className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">
          Members
        </span>

        {membersLoading ? (
          <div className="pt-1">
            {Array.from({ length: 2 }).map((_, i) => (
              <div key={i} className="flex items-center gap-3 py-2.5 border-b border-border-secondary/50">
                <div className="w-8 h-8 rounded-full bg-[#1d1d1f] animate-pulse shrink-0" />
                <div className="flex-1 space-y-1.5">
                  <div className="h-3 rounded-[5px] bg-[#1d1d1f] animate-pulse w-1/3" />
                  <div className="h-2.5 rounded-[5px] bg-[#1d1d1f] animate-pulse w-1/2" />
                </div>
              </div>
            ))}
          </div>
        ) : !members?.length ? (
          <div className="flex flex-col items-center gap-1.5 py-5 text-center border border-dashed border-border-secondary rounded-[11px] mt-2">
            <Users className="w-4 h-4 text-text-quaternary/60" />
            <p className="text-xs text-text-tertiary">No members in this project yet.</p>
          </div>
        ) : (
          <div className="pt-1">
            {members.map((member: ProjectMember) => {
              const initial = (member.name || member.email || '?')[0].toUpperCase()
              return (
                <div
                  key={member.id}
                  className="flex items-center gap-3 py-2.5 border-b border-border-secondary/50 last:border-b-0"
                >
                  {/* Avatar */}
                  <div className="w-8 h-8 rounded-full bg-accent-blue/15 text-accent-blue text-xs font-semibold flex items-center justify-center shrink-0">
                    {initial}
                  </div>

                  {/* Name + email */}
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-semibold text-text-primary truncate">
                      {member.name || member.email}
                    </div>
                    {member.name && (
                      <div className="text-xs text-text-tertiary truncate">{member.email}</div>
                    )}
                  </div>

                  {/* Role badge */}
                  <span className="rounded-[5px] px-1.5 py-0.5 text-[10px] font-semibold bg-[#272729] border border-border-secondary text-text-tertiary shrink-0">
                    {member.role}
                  </span>

                  {/* Remove */}
                  <button
                    onClick={() => {
                      if (confirm(`Remove ${member.name || member.email} from "${projectName}"?`)) {
                        removeMut.mutate(member.user_id)
                      }
                    }}
                    disabled={removeMut.isPending}
                    aria-label="Remove member"
                    className="text-text-quaternary hover:text-status-error transition-colors disabled:opacity-40 shrink-0"
                  >
                    <UserMinus className="w-3.5 h-3.5" />
                  </button>
                </div>
              )
            })}
          </div>
        )}
      </div>

      {removeMut.isError && (
        <p className="text-xs text-status-error/80">
          {(removeMut.error as Error)?.message ?? 'Failed to remove member'}
        </p>
      )}

      {/* Add member section */}
      <div className="mt-3 pt-3 border-t border-border-secondary/50 space-y-3">
        {/* Mode toggle header */}
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">Add members</span>
          <button
            type="button"
            onClick={() => { setBulkMode(false); setBulkResult(null) }}
            className={`border border-border-primary rounded-full px-2.5 py-1 text-xs transition-colors ${!bulkMode ? 'bg-white/[0.06] text-text-primary' : 'text-text-secondary hover:text-text-primary'}`}
          >
            Single
          </button>
          <button
            type="button"
            onClick={() => { setBulkMode(true); setBulkResult(null) }}
            className={`border border-border-primary rounded-full px-2.5 py-1 text-xs transition-colors ${bulkMode ? 'bg-white/[0.06] text-text-primary' : 'text-text-secondary hover:text-text-primary'}`}
          >
            Bulk add
          </button>
        </div>

        {bulkMode ? (
          <div className="space-y-2">
            <textarea
              value={bulkInput}
              onChange={e => { setBulkInput(e.target.value); setBulkResult(null) }}
              placeholder={"Paste user IDs, one per line…"}
              className="bg-white/[0.04] border border-border-primary rounded-[11px] px-3 py-2 text-xs text-text-secondary resize-none w-full h-20 focus:border-accent-blue/60 focus:outline-none placeholder:text-text-quaternary"
            />
            <div className="flex items-center gap-2">
              {/* Role select for bulk */}
              <Select value={addRole} onValueChange={setAddRole}>
                <SelectTrigger className="w-32 h-8 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60 shrink-0">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {allAvailableRoles.map(r => (
                    <SelectItem key={r} value={r}>{r}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <button
                type="button"
                onClick={handleBulkAdd}
                disabled={!!bulkProgress || !bulkInput.trim()}
                className="rounded-full bg-accent-blue text-white px-3 py-1.5 text-xs font-semibold hover:opacity-90 disabled:opacity-50 flex items-center gap-1.5 shrink-0"
              >
                {bulkProgress
                  ? <Loader2 className="w-3 h-3 animate-spin" />
                  : <UserPlus className="w-3 h-3" />
                }
                {bulkProgress ?? 'Add all'}
              </button>
            </div>
            {bulkResult && (
              <p className="text-xs">
                <span className="text-status-success">{bulkResult.added} added</span>
                {bulkResult.failed.length > 0 && (
                  <span className="text-status-error">, {bulkResult.failed.length} failed ({bulkResult.failed.join(', ')})</span>
                )}
              </p>
            )}
          </div>
        ) : (
          <>
            <form onSubmit={handleAdd} className="flex items-center gap-2">
              {/* User select */}
              {usersLoading ? (
                <div className="flex-1 h-9 rounded-[11px] bg-[#1d1d1f] animate-pulse" />
              ) : (
                <Select value={addUserId} onValueChange={setAddUserId}>
                  <SelectTrigger className="flex-1 h-9 text-sm bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60">
                    <SelectValue placeholder="Choose user…" />
                  </SelectTrigger>
                  <SelectContent>
                    {availableUsers.length === 0 ? (
                      <SelectItem value="_none" disabled>All users already added</SelectItem>
                    ) : (
                      availableUsers.map(u => (
                        <SelectItem key={u.id} value={u.id}>
                          {u.name} ({u.email})
                        </SelectItem>
                      ))
                    )}
                  </SelectContent>
                </Select>
              )}

              {/* Role select */}
              <Select value={addRole} onValueChange={setAddRole}>
                <SelectTrigger className="w-32 h-9 text-sm bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60 shrink-0">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {allAvailableRoles.map(r => (
                    <SelectItem key={r} value={r}>{r}</SelectItem>
                  ))}
                </SelectContent>
              </Select>

              {/* Submit */}
              <button
                type="submit"
                disabled={addMut.isPending || !addUserId}
                className="rounded-full bg-accent-blue text-white px-3 py-2 text-sm font-semibold hover:opacity-90 disabled:opacity-50 flex items-center gap-1.5 shrink-0"
              >
                {addMut.isPending
                  ? <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  : <UserPlus className="w-3.5 h-3.5" />
                }
                {addMut.isPending ? 'Adding…' : addSaved ? 'Added!' : 'Add'}
              </button>
            </form>

            {(addMut.isError || addError) && (
              <p className="text-xs text-status-error/80 mt-1">
                {(addMut.error as Error)?.message ?? addError}
              </p>
            )}
          </>
        )}
      </div>

      {/* Agent Event Overrides */}
      <div className="mt-4 pt-4 border-t border-border-secondary/50">
        <div className="flex items-center gap-2 mb-2">
          <p className="text-[11px] font-semibold text-text-quaternary">Agent Event Overrides</p>
          {overridesMut.isPending && (
            <span className="text-[10px] text-text-quaternary">Saving…</span>
          )}
          {!overridesMut.isPending && overridesMut.isSuccess && (
            <SavedBadge />
          )}
        </div>
        <p className="text-[11px] text-text-quaternary mb-3">
          Override org-level event settings for this project. Leave as "Inherit" to use org defaults.
        </p>
        {overridesMut.isError && (
          <p className="text-xs text-status-error/80 mb-2">
            {(overridesMut.error as Error)?.message ?? 'Failed to save overrides'}
          </p>
        )}
        <div>
          {EVENT_KEYS.map(({ key, label }) => {
            const rawVal = currentOverrides[key]
            const triState: boolean | null = rawVal === undefined ? null : rawVal
            return (
              <div
                key={key}
                className="flex items-center justify-between py-1.5"
              >
                <span className="text-xs text-text-secondary">{label}</span>
                <ThreeWayToggle
                  value={triState}
                  onChange={(v) => handleOverrideChange(key, v)}
                />
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

// ─── Main Page ───────────────────────────────────────────────────────────────

export default function Projects() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  // Sheet state (for memories)
  const [sheetOpen, setSheetOpen] = useState(false)
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null)

  // Accordion state (inline members panel)
  const [expandedProjectId, setExpandedProjectId] = useState<string | null>(null)

  // Archived toggle
  const [showArchived, setShowArchived] = useState(false)

  // Create Project Form
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [parentId, setParentId] = useState('')
  const [errorMsg, setErrorMsg] = useState('')

  // Queries
  const { data: projects, isLoading: projectsLoading } = useQuery({
    queryKey: ['projects', showArchived],
    queryFn: () => client.listProjects({ include_archived: showArchived }),
  })

  const { data: users, isLoading: usersLoading } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
  })

  const { data: roles } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
  })

  const selectedProject = useMemo(
    () => projects?.find(p => p.id === selectedProjectId) ?? null,
    [projects, selectedProjectId],
  )

  const { data: projectMemories, isLoading: memoriesLoading } = useQuery({
    queryKey: ['memories', 'project', selectedProject?.name],
    queryFn: () => client.listMemories({ project: selectedProject!.name, limit: 30 }),
    enabled: !!selectedProject && sheetOpen,
  })

  // Hierarchical tree
  const { rootProjects, childrenMap } = useMemo(() => {
    const projectIds = new Set((projects ?? []).map(p => p.id))
    const roots = (projects ?? []).filter(p => !p.parent_id || !projectIds.has(p.parent_id))
    const map: Record<string, typeof projects> = {}
    ;(projects ?? []).forEach(p => {
      if (p.parent_id && projectIds.has(p.parent_id)) {
        if (!map[p.parent_id]) map[p.parent_id] = []
        map[p.parent_id]!.push(p)
      }
    })
    return { rootProjects: roots, childrenMap: map }
  }, [projects])

  const parentOptions = useMemo(
    () => (projects ?? []).filter(p => p.id !== selectedProjectId),
    [projects, selectedProjectId],
  )

  const allAvailableRoles = useMemo(() => {
    const standard = ['admin', 'member', 'viewer']
    const custom = roles?.map(r => r.name) || []
    return Array.from(new Set([...standard, ...custom]))
  }, [roles])

  // Mutations
  const [projectCreated, setProjectCreated] = useState(false)

  const createProjectMut = useMutation({
    mutationFn: (data: { name: string; description?: string; parent_id?: string }) =>
      client.createProject(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      setName('')
      setDescription('')
      setParentId('')
      setErrorMsg('')
      setProjectCreated(true)
      setTimeout(() => setProjectCreated(false), 2000)
    },
    onError: (err: any) => setErrorMsg(err.message || 'Failed to create project'),
  })

  const deleteProjectMut = useMutation({
    mutationFn: (id: string) => client.deleteProject(id),
    onSuccess: (_, deletedId) => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      if (selectedProjectId === deletedId) {
        setSheetOpen(false)
        setSelectedProjectId(null)
      }
      if (expandedProjectId === deletedId) {
        setExpandedProjectId(null)
      }
    },
    onError: () => {},
  })

  const archiveProjectMut = useMutation({
    mutationFn: (id: string) => client.archiveProject(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const restoreProjectMut = useMutation({
    mutationFn: (id: string) => client.restoreProject(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const updateProjectMut = useMutation({
    mutationFn: ({ id, parent_id }: { id: string; parent_id: string | null }) =>
      client.updateProject(id, { parent_id }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  const handleMemoriesClick = (projectId: string) => {
    setSelectedProjectId(projectId)
    setSheetOpen(true)
  }

  const handleToggleExpand = (projectId: string) => {
    setExpandedProjectId(prev => (prev === projectId ? null : projectId))
  }

  const handleCreateProject = (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { setErrorMsg('Project Name is required.'); return }
    createProjectMut.mutate({
      name: name.trim().toLowerCase().replace(/\s+/g, '-'),
      description: description.trim() || undefined,
      parent_id: parentId || undefined,
    })
  }

  const renderProjectRow = (project: NonNullable<typeof projects>[number], indent = false) => {
    const isExpanded = expandedProjectId === project.id
    const isArchived = !!project.archived_at
    return (
      <div key={project.id}>
        {/* Row */}
        <div
          className={`p-4 flex items-start justify-between gap-4 transition-colors ${
            isExpanded ? 'bg-[#272729]' : 'hover:bg-[#272729]/20'
          } ${indent ? 'pl-10 border-l-2 border-border-secondary ml-4' : ''} ${
            isArchived ? 'opacity-60' : ''
          }`}
        >
          <div className="flex items-center gap-2 flex-1 min-w-0">
            {indent
              ? <ChevronRight className="w-3.5 h-3.5 text-text-tertiary flex-shrink-0" />
              : <FolderOpen className="w-4 h-4 text-text-tertiary flex-shrink-0" />
            }
            <div className="min-w-0">
              <span className="font-semibold text-text-primary text-sm truncate block">{project.name}</span>
              {project.description && (
                <p className="text-xs text-text-tertiary truncate">{project.description}</p>
              )}
              <span className="text-[10px] text-text-tertiary">{new Date(project.created_at).toLocaleDateString()}</span>
              {isArchived && (
                <span className="ml-2 text-[10px] bg-status-warning/10 text-status-warning border border-status-warning/20 rounded-[5px] px-1.5 py-0.5">
                  archived
                </span>
              )}
            </div>
          </div>

          <div className="flex items-center gap-1 flex-shrink-0">
            {!isArchived && (
              <>
                {/* Memories button */}
                <button
                  onClick={() => handleMemoriesClick(project.id)}
                  aria-label={`View memories for ${project.name}`}
                  title="View memories"
                  className="p-1.5 rounded-[8px] text-text-tertiary hover:text-accent-blue hover:bg-[#272729]/60 transition-colors"
                >
                  <Brain className="w-4 h-4" />
                </button>

                {/* Members expand/collapse toggle */}
                <button
                  onClick={() => handleToggleExpand(project.id)}
                  aria-label={isExpanded ? `Collapse members for ${project.name}` : `Expand members for ${project.name}`}
                  aria-expanded={isExpanded}
                  title={isExpanded ? 'Collapse members' : 'Expand members'}
                  className={cn(
                    'rounded-full p-1.5 transition-colors flex items-center gap-1',
                    isExpanded
                      ? 'text-accent-blue bg-accent-blue/10'
                      : 'text-text-tertiary hover:text-accent-blue hover:bg-white/[0.06]',
                  )}
                >
                  <Users className="w-4 h-4" />
                  <ChevronDown className={cn('w-3.5 h-3.5 transition-transform duration-200', isExpanded && 'rotate-180')} />
                </button>
              </>
            )}

            {/* Archive / Restore */}
            {isArchived ? (
              <button
                onClick={e => {
                  e.stopPropagation()
                  restoreProjectMut.mutate(project.id)
                }}
                aria-label={`Restore project ${project.name}`}
                title="Restore project"
                disabled={restoreProjectMut.isPending}
                className="p-1.5 rounded-[8px] text-text-quaternary hover:text-status-success hover:bg-status-success/10 transition-colors disabled:opacity-40"
              >
                <RotateCcw className="w-4 h-4" />
              </button>
            ) : (
              <button
                onClick={e => {
                  e.stopPropagation()
                  if (confirm(`Archive project "${project.name}"? It can be restored later.`)) {
                    archiveProjectMut.mutate(project.id)
                  }
                }}
                aria-label={`Archive project ${project.name}`}
                title="Archive project"
                disabled={archiveProjectMut.isPending}
                className="p-1.5 rounded-[8px] text-text-quaternary hover:text-status-warning hover:bg-status-warning/10 transition-colors disabled:opacity-40"
              >
                <Archive className="w-4 h-4" />
              </button>
            )}
          </div>
        </div>

        {/* Inline members accordion — CSS transition wrapper */}
        <div className={cn('overflow-hidden transition-all duration-200', isExpanded ? 'max-h-[600px]' : 'max-h-0')}>
          <MembersPanel
            projectId={project.id}
            projectName={project.name}
            client={client}
            users={users}
            usersLoading={usersLoading}
            allAvailableRoles={allAvailableRoles}
          />
        </div>
      </div>
    )
  }

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div>
        <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Projects & Scopes</h1>
        <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">
          Manage organization projects and configure dynamic per-project user role overrides.
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Project List */}
        <div className="lg:col-span-2">
          <div className="border border-border-primary rounded-[18px] overflow-hidden bg-[#272729]">
            <div className="px-4 py-3 border-b border-border-secondary bg-[#272729]/40 flex items-center justify-between">
              <span className="text-xs font-semibold text-text-secondary">Projects</span>
              <button
                onClick={() => setShowArchived(v => !v)}
                className={cn(
                  'text-[11px] px-2.5 py-1 rounded-full border transition-colors',
                  showArchived
                    ? 'bg-accent-blue/10 border-accent-blue/30 text-accent-blue font-semibold'
                    : 'border-border-secondary text-text-quaternary hover:text-text-tertiary',
                )}
              >
                {showArchived ? 'Showing archived' : 'Show archived'}
              </button>
            </div>
            {deleteProjectMut.isError && (
              <div className="mx-4 mt-3 p-2 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[8px]">
                {(deleteProjectMut.error as Error)?.message ?? 'Failed to delete project'}
              </div>
            )}
            <div className="divide-y divide-border-secondary">
              {projectsLoading ? (
                Array.from({ length: 3 }).map((_, i) => (
                  <div key={i} className="p-4 flex items-center gap-3">
                    <div className="w-4 h-4 rounded-[5px] bg-[#1d1d1f] animate-pulse flex-shrink-0" />
                    <div className="flex-1 space-y-2">
                      <div className="h-3.5 rounded-[5px] bg-[#1d1d1f] animate-pulse w-1/3" />
                      <div className="h-2.5 rounded-[5px] bg-[#1d1d1f] animate-pulse w-2/3" />
                    </div>
                  </div>
                ))
              ) : !projects?.length ? (
                <div className="flex flex-col items-center gap-2 py-12 text-center">
                  <FolderOpen className="w-6 h-6 text-text-quaternary/50" />
                  <p className="text-sm font-semibold text-text-secondary">No projects yet</p>
                  <p className="text-xs text-text-quaternary max-w-xs">Create your first project using the form on the right to organize memories by workspace scope.</p>
                </div>
              ) : (
                rootProjects.map(root => (
                  <div key={root.id} className="divide-y divide-border-secondary">
                    {renderProjectRow(root)}
                    {childrenMap[root.id]?.map(child => (
                      <div key={child.id}>{renderProjectRow(child, true)}</div>
                    ))}
                  </div>
                ))
              )}
            </div>
          </div>
        </div>

        {/* Create Project Form */}
        <div>
          <div className="border border-border-primary rounded-[18px] p-5 bg-[#272729] space-y-4">
            <div>
              <h3 className="text-sm font-semibold text-text-primary flex items-center gap-2">
                <FolderGit className="w-4 h-4 text-accent-blue" />
                Create Project
              </h3>
              <p className="text-[11px] text-text-tertiary mt-0.5">
                Register a new workspace scope inside the organization.
              </p>
            </div>

            {errorMsg && (
              <div className="p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[11px]">
                {errorMsg}
              </div>
            )}

            <form onSubmit={handleCreateProject} className="space-y-4 text-xs">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                  Project Name (slug)
                </label>
                <input
                  type="text"
                  placeholder="e.g. core-payments"
                  value={name}
                  onChange={e => setName(e.target.value)}
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60"
                  required
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                  Description
                </label>
                <textarea
                  placeholder="What is this scope about?"
                  value={description}
                  onChange={e => setDescription(e.target.value)}
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 h-20 resize-none"
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                  Parent Project
                </label>
                <Select value={parentId} onValueChange={setParentId}>
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue placeholder="— None (root) —" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="">— None (root) —</SelectItem>
                    {(projects ?? []).map(p => (
                      <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <button
                type="submit"
                disabled={createProjectMut.isPending}
                className="w-full flex items-center justify-center gap-2 px-3 py-2.5 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white font-semibold transition-colors disabled:opacity-50"
              >
                <Plus className="w-4 h-4" />
                {createProjectMut.isPending ? 'Creating…' : projectCreated ? 'Created!' : 'Create Project'}
              </button>
            </form>
          </div>
        </div>
      </div>

      {/* Memories Sheet */}
      <Modal open={sheetOpen} onOpenChange={setSheetOpen} position="right" size="lg">
        <ModalCloseButton />

        {selectedProject && (
          <div className="flex flex-col h-full pt-2">
            {/* Header */}
            <div className="mb-5">
              <div className="flex items-center gap-2 mb-1">
                <FolderOpen className="w-5 h-5 text-accent-blue flex-shrink-0" />
                <h2 className="text-lg font-semibold text-text-primary font-mono truncate">
                  {selectedProject.name}
                </h2>
              </div>
              {selectedProject.description && (
                <p className="text-sm text-text-tertiary mb-3">{selectedProject.description}</p>
              )}

              {/* Parent selector */}
              <div className="flex items-center gap-2 mt-3">
                <GitBranch className="w-3.5 h-3.5 text-text-tertiary flex-shrink-0" />
                <div className="flex-1">
                  <Select
                    key={selectedProject.id}
                    value={selectedProject.parent_id ?? ''}
                    onValueChange={v =>
                      updateProjectMut.mutate({
                        id: selectedProject.id,
                        parent_id: v || null,
                      })
                    }
                  >
                    <SelectTrigger className="h-8 text-xs">
                      <SelectValue placeholder="— No parent —" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="">— No parent —</SelectItem>
                      {parentOptions.map(p => (
                        <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </div>

            {/* Memories label */}
            <div className="flex items-center gap-2 border-b border-border-secondary mb-4 pb-2">
              <Brain className="w-3.5 h-3.5 text-accent-blue" />
              <span className="text-xs font-semibold text-accent-blue">Memories</span>
            </div>

            {/* Memories list */}
            <div className="flex-1 overflow-y-auto min-h-0 space-y-2">
              {memoriesLoading ? (
                <div className="text-center py-8 text-sm text-text-tertiary">Loading memories...</div>
              ) : !projectMemories?.length ? (
                <div className="text-center py-8 text-sm text-text-tertiary border border-dashed border-border-secondary rounded-[18px]">
                  No memories stored for this project.
                </div>
              ) : (
                projectMemories.map(memory => (
                  <div
                    key={memory.id}
                    className="p-3 border border-border-secondary rounded-[11px] bg-[#272729]/10 space-y-1"
                  >
                    {memory.title && (
                      <div className="text-xs font-semibold text-text-primary">{memory.title}</div>
                    )}
                    <p className="text-xs text-text-secondary line-clamp-3">{memory.content}</p>
                    <div className="flex items-center gap-2 pt-1">
                      {memory.type && (
                        <span className="text-[10px] bg-accent-blue/10 text-accent-blue px-1.5 py-0.5 rounded-[5px] font-mono">
                          {memory.type}
                        </span>
                      )}
                      <span className="text-[10px] text-text-tertiary ml-auto">
                        {new Date(memory.created_at).toLocaleDateString()}
                      </span>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        )}
      </Modal>
    </div>
  )
}
