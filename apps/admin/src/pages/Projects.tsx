import { Fragment, useMemo, useState, useEffect, useRef } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import {
  FolderGit, Plus, Users, UserPlus, UserMinus,
  FolderOpen, ChevronRight, ChevronDown, Brain, GitBranch, Loader2,
  Archive, RotateCcw, BookMarked, Settings, X, Search,
} from 'lucide-react'
import { cn } from '../lib/utils'
import {
  Modal, ModalCloseButton,
} from '../components/ui/Modal/Modal'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import { StatTile } from './dashboard/StatTile'
import { accentFor } from './dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import type { ProjectMember, ProjectEventOverrides, User as UserType, Convention } from '../types'

// Sentinel for the "Internal (no client)" filter option — a project with a
// null `client_id` is internal work, which the backend `client_id` query param
// cannot express, so that case is filtered client-side.
const INTERNAL_CLIENT = '__internal__'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Projects page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

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
        className="border border-border-secondary rounded-[8px] px-2.5 py-1 text-[10px] text-text-quaternary hover:text-text-tertiary transition-colors"
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
        className="border border-status-success/30 rounded-[8px] px-2.5 py-1 text-[10px] text-status-success bg-status-success/10 font-semibold"
        title="Enabled for this project — click to disable"
      >
        On
      </button>
    )
  }
  return (
    <button
      onClick={cycle}
      className="border border-status-error/30 rounded-[8px] px-2.5 py-1 text-[10px] text-status-error bg-status-error/10 font-semibold"
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
  const { session } = useAuth()
  const isAdmin = isPrivileged(session?.user.role)
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
    enabled: !!projectId && isAdmin,
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
    enabled: !!projectId && isAdmin,
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
    enabled: !!projectId && isAdmin,
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
    <div className="rounded-b-[18px] border border-t-0 border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] px-5 pb-5 pt-4 space-y-4">
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
                <div className="w-8 h-8 rounded-full bg-white/[0.04] animate-pulse shrink-0" />
                <div className="flex-1 space-y-1.5">
                  <div className="h-3 rounded-[5px] bg-white/[0.04] animate-pulse w-1/3" />
                  <div className="h-2.5 rounded-[5px] bg-white/[0.04] animate-pulse w-1/2" />
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
                    <div className="text-xs font-semibold text-text-primary truncate">
                      {member.name || member.email}
                    </div>
                    {member.name && (
                      <div className="text-[10px] text-text-quaternary truncate">{member.email}</div>
                    )}
                  </div>

                  {/* Role badge */}
                  <span className="rounded-[5px] px-1.5 py-0.5 text-[10px] font-semibold bg-white/[0.06] border border-white/[0.09] text-text-tertiary shrink-0">
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
                <div className="flex-1 h-9 rounded-[11px] bg-white/[0.04] animate-pulse" />
              ) : (
                <Select value={addUserId} onValueChange={setAddUserId}>
                  <SelectTrigger className="flex-1 h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60">
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
                <SelectTrigger className="w-32 h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60 shrink-0">
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
                className="rounded-full bg-accent-blue text-white px-3 py-1.5 text-xs font-semibold hover:opacity-90 disabled:opacity-50 flex items-center gap-1.5 shrink-0"
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
          <p className="text-[10px] font-semibold text-text-quaternary">Agent Event Overrides</p>
          {overridesMut.isPending && (
            <span className="text-[10px] text-text-quaternary">Saving…</span>
          )}
          {!overridesMut.isPending && overridesMut.isSuccess && (
            <SavedBadge />
          )}
        </div>
        <p className="text-[10px] text-text-quaternary mb-3">
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
  const isAdmin = isPrivileged(session?.user.role)
  const navigate = useNavigate()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  // Sheet state (for memories)
  const [sheetOpen, setSheetOpen] = useState(false)
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null)

  // Accordion state (inline members panel)
  const [expandedProjectId, setExpandedProjectId] = useState<string | null>(null)

  // Child-project tree expansion (separate from the members accordion above)
  const [expandedTreeIds, setExpandedTreeIds] = useState<Set<string>>(new Set())

  // Archived toggle
  const [showArchived, setShowArchived] = useState(false)

  // Client filter: '' = all, INTERNAL_CLIENT = internal-only, else a client id
  const [clientFilter, setClientFilter] = useState('')

  // Create Project modal (header trigger, matches mockup)
  const [createOpen, setCreateOpen] = useState(false)

  // Client-side name filter for the projects tree (mockup search field)
  const [filterQuery, setFilterQuery] = useState('')

  // Project settings modal
  const [editingProjectId, setEditingProjectId] = useState<string | null>(null)
  const [settingsDescription, setSettingsDescription] = useState('')
  const [settingsCustomInstructions, setSettingsCustomInstructions] = useState('')
  const [settingsRetentionDays, setSettingsRetentionDays] = useState<number | ''>('')
  const [addChildQuery, setAddChildQuery] = useState('')
  const [addChildOpen, setAddChildOpen] = useState(false)

  // Create Project Form
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [parentId, setParentId] = useState('')
  const [createClientId, setCreateClientId] = useState('')
  const [errorMsg, setErrorMsg] = useState('')

  // Queries
  const { data: projects, isLoading: projectsLoading } = useQuery({
    queryKey: ['projects', showArchived, clientFilter],
    queryFn: () => {
      // A specific client uses the backend `client_id` filter param; the
      // "Internal" pseudo-filter has no backend equivalent, so it fetches all
      // and narrows to null-client projects client-side.
      const backendClientId = clientFilter && clientFilter !== INTERNAL_CLIENT ? clientFilter : undefined
      return client.listProjects({ include_archived: showArchived, client_id: backendClientId })
        .then(list =>
          clientFilter === INTERNAL_CLIENT ? list.filter(p => !p.client_id) : list,
        )
    },
  })

  const { data: clients } = useQuery({
    queryKey: ['clients'],
    queryFn: () => client.listClients(),
    enabled: isAdmin,
  })

  const clientsById = useMemo(
    () => new Map((clients ?? []).map(c => [c.id, c])),
    [clients],
  )

  const { data: users, isLoading: usersLoading } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    enabled: isAdmin,
  })

  const { data: roles } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
    enabled: isAdmin,
  })

  const { data: allConventions } = useQuery({
    queryKey: ['conventions'],
    queryFn: () => client.listConventions(),
  })

  const editingProject = useMemo(
    () => projects?.find(p => p.id === editingProjectId) ?? null,
    [projects, editingProjectId],
  )

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

  // Names of projects that have children — derived from already-fetched `projects`,
  // used only for the "With children" stat tile sub-caption (never fabricated).
  const parentsWithChildrenNames = useMemo(() => {
    const idToName = new Map((projects ?? []).map(p => [p.id, p.name]))
    return Object.keys(childrenMap).map(id => idToName.get(id)).filter((n): n is string => !!n)
  }, [projects, childrenMap])

  // Client-side name filter over the tree — a project matches if its own name
  // matches, or if any of its descendants match (so the ancestor stays visible).
  const filteredRootProjects = useMemo(() => {
    const q = filterQuery.trim().toLowerCase()
    if (!q) return rootProjects
    const matches = (p: NonNullable<typeof projects>[number]): boolean => {
      if (p.name.toLowerCase().includes(q)) return true
      return (childrenMap[p.id] ?? []).some(matches)
    }
    return rootProjects.filter(matches)
  }, [rootProjects, childrenMap, filterQuery])

  const parentOptions = useMemo(
    () => (projects ?? []).filter(p => p.id !== selectedProjectId),
    [projects, selectedProjectId],
  )

  // Descendants of the editing project (for parent selector — exclude to prevent cycles)
  const editingProjectDescendants = useMemo(() => {
    if (!editingProject || !projects) return new Set<string>()
    const descendants = new Set<string>()
    const queue = [editingProject.id]
    while (queue.length) {
      const curr = queue.shift()!
      for (const p of projects) {
        if (p.parent_id === curr && !descendants.has(p.id)) {
          descendants.add(p.id)
          queue.push(p.id)
        }
      }
    }
    return descendants
  }, [projects, editingProject])

  // Valid parent options for the editing project (exclude itself, descendants, archived)
  const parentOptionsForEdit = useMemo(
    () => (projects ?? []).filter(p =>
      p.id !== editingProject?.id &&
      !editingProjectDescendants.has(p.id) &&
      !p.archived_at,
    ),
    [projects, editingProject, editingProjectDescendants],
  )

  // Current children of the editing project
  const currentChildren = useMemo(
    () => (projects ?? []).filter(p => p.parent_id === editingProject?.id),
    [projects, editingProject],
  )

  // Ancestors of the editing project (for cycle prevention in add-child)
  const editingProjectAncestors = useMemo(() => {
    if (!editingProject || !projects) return new Set<string>()
    const ancestors = new Set<string>()
    const projectMap = new Map(projects.map(p => [p.id, p]))
    let cur: typeof projects[number] | undefined = editingProject.parent_id
      ? projectMap.get(editingProject.parent_id)
      : undefined
    while (cur) {
      if (ancestors.has(cur.id)) break // guard against cyclic parent chains in data
      ancestors.add(cur.id)
      cur = cur.parent_id ? projectMap.get(cur.parent_id) : undefined
    }
    return ancestors
  }, [projects, editingProject])

  // Candidate projects that can be assigned as children
  const childCandidates = useMemo(() => {
    if (!editingProject || !projects) return []
    const childrenIds = new Set(currentChildren.map(c => c.id))
    return projects.filter(p =>
      p.id !== editingProject.id &&
      !p.archived_at &&
      !childrenIds.has(p.id) &&
      !editingProjectAncestors.has(p.id),
    )
  }, [projects, editingProject, currentChildren, editingProjectAncestors])

  // Filtered candidate list by search query
  const filteredChildCandidates = useMemo(() => {
    const q = addChildQuery.toLowerCase().trim()
    if (!q) return childCandidates
    return childCandidates.filter(p => p.name.toLowerCase().includes(q))
  }, [childCandidates, addChildQuery])

  const allAvailableRoles = useMemo(() => {
    const standard = ['admin', 'member', 'viewer']
    const custom = roles?.map(r => r.name) || []
    return Array.from(new Set([...standard, ...custom]))
  }, [roles])

  // Mutations
  const [projectCreated, setProjectCreated] = useState(false)

  const createProjectMut = useMutation({
    mutationFn: (data: { name: string; description?: string; parent_id?: string; client_id?: string }) =>
      client.createProject(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      setName('')
      setDescription('')
      setParentId('')
      setCreateClientId('')
      setErrorMsg('')
      setProjectCreated(true)
      setCreateOpen(false)
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

  const updateProjectSettingsMut = useMutation({
    mutationFn: ({ id, description, custom_instructions, retention_days }: {
      id: string
      description: string
      custom_instructions: string
      retention_days: number | ''
    }) =>
      client.updateProject(id, {
        description: description || undefined,
        custom_instructions: custom_instructions || undefined,
        retention_days: retention_days !== '' ? Number(retention_days) : undefined,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['projects'] })
      setEditingProjectId(null)
    },
  })

  const lastInitializedId = useRef<string | null>(null)
  useEffect(() => {
    if (editingProjectId && editingProject && lastInitializedId.current !== editingProjectId) {
      lastInitializedId.current = editingProjectId
      setSettingsDescription(editingProject.description ?? '')
      setSettingsCustomInstructions('')
      setSettingsRetentionDays('')
      setAddChildQuery('')
      setAddChildOpen(false)
    }
    if (!editingProjectId) lastInitializedId.current = null
  }, [editingProjectId, editingProject])

  const handleMemoriesClick = (projectId: string) => {
    setSelectedProjectId(projectId)
    setSheetOpen(true)
  }

  const handleToggleExpand = (projectId: string) => {
    setExpandedProjectId(prev => (prev === projectId ? null : projectId))
  }

  const handleToggleTreeExpand = (projectId: string) => {
    setExpandedTreeIds(prev => {
      const next = new Set(prev)
      if (next.has(projectId)) next.delete(projectId)
      else next.add(projectId)
      return next
    })
  }

  const handleCreateProject = (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { setErrorMsg('Project Name is required.'); return }
    createProjectMut.mutate({
      name: name.trim().toLowerCase().replace(/\s+/g, '-'),
      description: description.trim() || undefined,
      parent_id: parentId || undefined,
      client_id: createClientId || undefined,
    })
  }

  // Map of `depth → Tailwind left-padding class` for the nested indent guide.
  // Capped at 5 levels to keep the class set bounded; deeper trees cap at `pl-32`.
  const DEPTH_PAD: Record<number, string> = {
    1: 'pl-10',
    2: 'pl-14',
    3: 'pl-20',
    4: 'pl-24',
    5: 'pl-32',
  }
  const depthPad = (depth: number): string => DEPTH_PAD[Math.min(depth, 5)] ?? 'pl-32'

  const renderProjectRow = (
    project: NonNullable<typeof projects>[number],
    depth = 0,
    visited: Set<string> = new Set(),
  ): React.ReactNode => {
    // Defense-in-depth: backend already prevents cycles, but a cycle in legacy
    // data would otherwise loop forever. Stop and skip if we re-enter.
    if (visited.has(project.id)) return null
    const nextVisited = new Set(visited)
    nextVisited.add(project.id)

    const isExpanded = expandedProjectId === project.id
    const isArchived = !!project.archived_at
    const childList = childrenMap[project.id] ?? []
    const hasExpandableChildren = !isArchived && childList.length > 0
    const isTreeExpanded = !isArchived && expandedTreeIds.has(project.id) && hasExpandableChildren

    return (
      <div key={project.id}>
        {/* Row */}
        <div
          className={`group p-4 flex items-start justify-between gap-4 transition-colors ${
            isExpanded ? 'bg-accent-blue/10' : 'hover:bg-accent-blue/[0.05]'
          } ${depth > 0 ? `${depthPad(depth)} border-l-2 border-border-secondary ml-4` : ''} ${
            isArchived ? 'opacity-60' : ''
          }`}
        >
          <div className="flex items-center gap-1.5 flex-1 min-w-0">
            {hasExpandableChildren ? (
              <button
                type="button"
                onClick={(e) => { e.stopPropagation(); handleToggleTreeExpand(project.id) }}
                aria-label={isTreeExpanded ? `Collapse child projects of ${project.name}` : `Expand child projects of ${project.name}`}
                aria-expanded={isTreeExpanded}
                title={isTreeExpanded ? 'Collapse children' : 'Expand children'}
                className="p-0.5 -ml-0.5 rounded text-text-tertiary hover:text-text-primary transition-colors flex-shrink-0"
              >
                <ChevronRight
                  className={cn(
                    'w-3.5 h-3.5 transition-transform duration-200',
                    isTreeExpanded && 'rotate-90',
                  )}
                />
              </button>
            ) : (
              <span className="w-3.5 h-3.5 flex-shrink-0" aria-hidden="true" />
            )}
            <FolderOpen className="w-4 h-4 text-text-tertiary flex-shrink-0" />
            <div className="min-w-0">
              <span className="text-xs font-semibold text-text-primary truncate block">{project.name}</span>
              {project.description && (
                <p className="text-[10px] text-text-quaternary truncate">{project.description}</p>
              )}
              <div className="flex items-center gap-2 mt-1 flex-wrap">
                <span className="text-[10px] text-text-tertiary">{new Date(project.created_at).toLocaleDateString()}</span>
                {/* Owning client — "Internal" when null (not unassigned). */}
                <span
                  className={cn(
                    'text-[10px] rounded-[5px] px-1.5 py-0.5 border',
                    project.client_id
                      ? 'bg-accent-blue/10 text-accent-blue border-accent-blue/20'
                      : 'bg-white/[0.06] text-text-tertiary border-white/[0.09]',
                  )}
                >
                  {project.client_id ? (clientsById.get(project.client_id)?.name ?? 'Client') : 'Internal'}
                </span>
                {isArchived && (
                  <span className="text-[10px] bg-status-warning/10 text-status-warning border border-status-warning/20 rounded-[5px] px-1.5 py-0.5">
                    archived
                  </span>
                )}
                {/* Convention count badge */}
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    navigate('/conventions')
                  }}
                  title="View conventions"
                  className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-text-secondary flex items-center gap-1 hover:bg-white/[0.10] transition-colors"
                >
                  <BookMarked className="w-3 h-3" />
                  {(allConventions ?? []).filter((c: Convention) => !c.archived_at && (c.project_id === project.id || c.project_id == null)).length} conventions
                </button>
                {/* Memory count badge */}
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    handleMemoriesClick(project.id)
                  }}
                  title="View memories"
                  className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-text-secondary flex items-center gap-1 hover:bg-white/[0.10] transition-colors"
                >
                  <Brain className="w-3 h-3" />
                  memories
                </button>
              </div>
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
                  className="p-1.5 rounded-[8px] text-text-tertiary hover:text-accent-blue hover:bg-white/[0.10] transition-colors"
                >
                  <Brain className="w-4 h-4" />
                </button>

                {/* Project settings button */}
                <button
                  onClick={(e) => { e.stopPropagation(); setEditingProjectId(project.id) }}
                  aria-label={`Settings for ${project.name}`}
                  title="Project settings"
                  className="p-1.5 rounded-[8px] text-text-tertiary hover:text-text-primary hover:bg-white/[0.10] opacity-0 group-hover:opacity-100 transition-all"
                >
                  <Settings className="w-3 h-3" />
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

        {/* Recursive children — only render when the parent is tree-expanded. */}
        {isTreeExpanded && childList.map(child =>
          <Fragment key={child.id}>
            {renderProjectRow(child, depth + 1, nextVisited)}
          </Fragment>,
        )}
      </div>
    )
  }

  const statTiles = [
    {
      label: 'Projects',
      value: String(projects?.length ?? 0),
      sub: showArchived ? 'including archived' : 'active',
      icon: FolderOpen,
    },
    {
      label: 'With children',
      value: String(Object.keys(childrenMap).length),
      sub: parentsWithChildrenNames.length ? parentsWithChildrenNames.slice(0, 2).join(' · ') : undefined,
      icon: GitBranch,
    },
    {
      label: 'Archived',
      value: String((projects ?? []).filter(p => p.archived_at).length),
      sub: 'in current view',
      icon: Archive,
    },
    // "Total memories" / "Most active" tiles from the mockup would require an
    // org-wide memory aggregate this page doesn't fetch — omitted rather than
    // fabricated. Per-project memory counts are still available in each row's
    // members panel.
  ]

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 rounded-[13px] bg-accent-blue/12 flex items-center justify-center shrink-0">
            <FolderGit className="w-5 h-5 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-base font-semibold text-text-primary">Projects & Scopes</h1>
            <p className="text-xs text-text-quaternary mt-0.5">
              Manage organization projects and configure dynamic per-project user role overrides.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-44">
            <Select value={clientFilter} onValueChange={setClientFilter}>
              <SelectTrigger className="h-8 text-xs" aria-label="Filter by client">
                <SelectValue placeholder="All clients" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">All clients</SelectItem>
                <SelectItem value={INTERNAL_CLIENT}>Internal (no client)</SelectItem>
                {(clients ?? []).map(c => (
                  <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <button
            onClick={() => setShowArchived(v => !v)}
            className={cn(
              'flex items-center gap-1.5 text-[11px] px-3 py-1.5 rounded-full border transition-colors',
              showArchived
                ? 'bg-accent-blue/10 border-accent-blue/30 text-accent-blue font-semibold'
                : 'border-border-primary text-text-quaternary hover:text-text-tertiary',
            )}
          >
            <Archive className="w-3 h-3" />
            {showArchived ? 'Showing archived' : 'Show archived'}
          </button>
          <button
            onClick={() => setCreateOpen(true)}
            className="flex items-center gap-1.5 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white px-3.5 py-1.5 text-xs font-semibold transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
            New Project
          </button>
        </div>
      </div>

      <KpiMarquee role="list" aria-label="Project statistics">
        {statTiles.map((tile, i) => (
          <div key={tile.label} className="w-[232px] flex-none">
            <StatTile label={tile.label} value={tile.value} sub={tile.sub} icon={tile.icon} accent={accentFor(i)} />
          </div>
        ))}
      </KpiMarquee>

      {/* Project List (full width, matches mockup) */}
      <div className={`rounded-[18px] overflow-hidden ${GLASS_PANEL}`}>
        <div className="px-5 py-4 border-b border-border-secondary flex items-center justify-between gap-3 flex-wrap">
          <span className="text-sm font-semibold text-text-primary">Projects</span>
          <div className="flex items-center gap-2 h-8 w-60 px-3 rounded-[10px] border border-border-primary bg-white/[0.02]">
            <Search className="w-3.5 h-3.5 text-text-quaternary shrink-0" />
            <input
              type="text"
              value={filterQuery}
              onChange={e => setFilterQuery(e.target.value)}
              placeholder="Filter projects…"
              aria-label="Filter projects"
              className="flex-1 min-w-0 bg-transparent border-none outline-none text-xs text-text-primary placeholder:text-text-quaternary"
            />
          </div>
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
                <div className="w-4 h-4 rounded-[5px] bg-white/[0.04] animate-pulse flex-shrink-0" />
                <div className="flex-1 space-y-2">
                  <div className="h-3.5 rounded-[5px] bg-white/[0.04] animate-pulse w-1/3" />
                  <div className="h-2.5 rounded-[5px] bg-white/[0.04] animate-pulse w-2/3" />
                </div>
              </div>
            ))
          ) : !projects?.length ? (
            <div className="flex flex-col items-center gap-2 py-12 text-center">
              <FolderOpen className="w-6 h-6 text-text-quaternary/50" />
              <p className="text-xs font-semibold text-text-secondary">No projects yet</p>
              <p className="text-xs text-text-quaternary max-w-xs">Create your first project using the "New Project" button above to organize memories by workspace scope.</p>
            </div>
          ) : !filteredRootProjects.length ? (
            <div className="flex flex-col items-center gap-2 py-12 text-center">
              <Search className="w-6 h-6 text-text-quaternary/50" />
              <p className="text-xs font-semibold text-text-secondary">No projects match "{filterQuery}"</p>
            </div>
          ) : (
            filteredRootProjects.map(root => renderProjectRow(root))
          )}
        </div>
      </div>

      {/* Create Project Modal (mockup: header "New Project" trigger) */}
      <Modal open={createOpen} onOpenChange={setCreateOpen}>
        <ModalCloseButton />
        <div className="rounded-[18px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] p-6 w-full max-w-md">
          <h2 className="text-xs font-semibold text-text-primary mb-1 flex items-center gap-2">
            <FolderGit className="w-4 h-4 text-accent-blue" />
            Create Project
          </h2>
          <p className="text-[10px] text-text-quaternary mb-5">
            Register a new workspace scope inside the organization.
          </p>

          {errorMsg && (
            <div className="mb-4 p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[11px]">
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

            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                Client
              </label>
              <Select value={createClientId} onValueChange={setCreateClientId}>
                <SelectTrigger className="h-8 text-xs">
                  <SelectValue placeholder="— Internal (no client) —" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="">— Internal (no client) —</SelectItem>
                  {(clients ?? []).map(c => (
                    <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="flex items-center justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setCreateOpen(false)}
                className="px-4 py-2 rounded-full border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={createProjectMut.isPending}
                className="flex items-center gap-2 px-4 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white font-semibold transition-colors disabled:opacity-50"
              >
                <Plus className="w-3.5 h-3.5" />
                {createProjectMut.isPending ? 'Creating…' : projectCreated ? 'Created!' : 'Create Project'}
              </button>
            </div>
          </form>
        </div>
      </Modal>

      {/* Project Settings Modal */}
      <Modal open={!!editingProjectId} onOpenChange={(open) => { if (!open) setEditingProjectId(null) }}>
        <ModalCloseButton />
        {editingProject && (
          <div className="rounded-[18px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] p-6 w-full max-w-md">
            <h2 className="text-xs font-semibold text-text-primary mb-1 flex items-center gap-2">
              <Settings className="w-4 h-4 text-accent-blue" />
              Project Settings
            </h2>
            <p className="text-[10px] text-text-quaternary mb-5 font-mono">{editingProject.name}</p>

            {updateProjectSettingsMut.isError && (
              <div className="mb-4 p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[11px]">
                {(updateProjectSettingsMut.error as Error)?.message ?? 'Failed to save settings'}
              </div>
            )}

            <form
              onSubmit={(e) => {
                e.preventDefault()
                updateProjectSettingsMut.mutate({
                  id: editingProject.id,
                  description: settingsDescription,
                  custom_instructions: settingsCustomInstructions,
                  retention_days: settingsRetentionDays,
                })
              }}
              className="space-y-4"
            >
              <div className="space-y-1">
                <label className="text-[10px] text-text-quaternary">Description</label>
                <textarea
                  value={settingsDescription}
                  onChange={e => setSettingsDescription(e.target.value)}
                  placeholder="Project description…"
                  className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary p-3 resize-none h-20 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary w-full"
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] text-text-quaternary">Custom AI Instructions</label>
                <textarea
                  value={settingsCustomInstructions}
                  onChange={e => setSettingsCustomInstructions(e.target.value)}
                  placeholder="Instructions injected into agent context for this project…"
                  className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary p-3 resize-none h-28 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary w-full"
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] text-text-quaternary">Retention Days</label>
                <input
                  type="number"
                  min={0}
                  value={settingsRetentionDays}
                  onChange={e => setSettingsRetentionDays(e.target.value === '' ? '' : Number(e.target.value))}
                  placeholder="Inherit from org (leave blank)"
                  className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2.5 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary"
                />
              </div>

              {/* Parent project */}
              <div className="space-y-1">
                <label className="text-[10px] text-text-quaternary">Parent project</label>
                <Select
                  key={editingProject.id + '-parent'}
                  value={editingProject.parent_id ?? ''}
                  onValueChange={v =>
                    updateProjectMut.mutate({ id: editingProject.id, parent_id: v || null })
                  }
                >
                  <SelectTrigger className="h-8 text-xs" disabled={updateProjectMut.isPending}>
                    <SelectValue placeholder="— No parent (root) —" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="">— No parent (root) —</SelectItem>
                    {parentOptionsForEdit.map(p => (
                      <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Child projects */}
              <div className="space-y-2 pt-1">
                <span className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">
                  Child projects
                </span>

                {/* Current children list */}
                {currentChildren.length === 0 ? (
                  <p className="text-xs text-text-quaternary py-1">No child projects assigned.</p>
                ) : (
                  <div className="space-y-0 divide-y divide-border-secondary/40">
                    {currentChildren.map(child => (
                      <div key={child.id} className="flex items-center justify-between py-1.5">
                        <span className="text-xs text-text-secondary font-mono">{child.name}</span>
                        <button
                          type="button"
                          onClick={() => {
                            if (confirm(`Remove "${child.name}" as a child of "${editingProject.name}"?`)) {
                              updateProjectMut.mutate({ id: child.id, parent_id: null })
                            }
                          }}
                          disabled={updateProjectMut.isPending}
                          aria-label={`Remove ${child.name} as child`}
                          className="text-text-quaternary hover:text-status-error transition-colors disabled:opacity-40"
                        >
                          <X className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                {/* Add child — searchable combobox */}
                <div className="relative">
                  <input
                    type="text"
                    placeholder="Add child project…"
                    value={addChildQuery}
                    disabled={updateProjectMut.isPending}
                    onFocus={() => setAddChildOpen(true)}
                    onChange={e => {
                      setAddChildQuery(e.target.value)
                      setAddChildOpen(true)
                    }}
                    className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary disabled:opacity-40"
                  />
                  {addChildOpen && filteredChildCandidates.length > 0 && (
                    <>
                      <div
                        className="fixed inset-0 z-40"
                        onClick={() => setAddChildOpen(false)}
                      />
                      <div className="absolute top-full left-0 right-0 mt-1 rounded-[11px] border border-white/[0.10] bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] z-50 max-h-48 overflow-y-auto">
                        {filteredChildCandidates.map(p => (
                          <button
                            key={p.id}
                            type="button"
                            disabled={updateProjectMut.isPending}
                            onClick={() => {
                              updateProjectMut.mutate({ id: p.id, parent_id: editingProject.id })
                              setAddChildQuery('')
                              setAddChildOpen(false)
                            }}
                            className="w-full px-3 py-2 text-xs text-left text-text-secondary hover:bg-white/[0.06] flex items-center justify-between gap-2 first:rounded-t-[11px] last:rounded-b-[11px] disabled:opacity-40"
                          >
                            <span className="font-mono">{p.name}</span>
                            {p.parent_id && (
                              <span className="text-[10px] text-text-quaternary shrink-0">
                                currently under {projects?.find(x => x.id === p.parent_id)?.name ?? '…'}
                              </span>
                            )}
                          </button>
                        ))}
                      </div>
                    </>
                  )}
                  {addChildOpen && filteredChildCandidates.length === 0 && addChildQuery && (
                    <>
                      <div
                        className="fixed inset-0 z-40"
                        onClick={() => setAddChildOpen(false)}
                      />
                      <div className="absolute top-full left-0 right-0 mt-1 rounded-[11px] border border-white/[0.10] bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] z-50 px-3 py-2">
                        <p className="text-xs text-text-quaternary">No assignable projects found.</p>
                      </div>
                    </>
                  )}
                </div>

                {updateProjectMut.isError && (
                  <p className="text-xs text-status-error/80">
                    {(updateProjectMut.error as Error)?.message ?? 'Failed to update project hierarchy'}
                  </p>
                )}
              </div>

              <div className="flex items-center justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={() => setEditingProjectId(null)}
                  className="px-4 py-2 rounded-full border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={updateProjectSettingsMut.isPending}
                  className="px-4 py-2 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50 flex items-center gap-1.5"
                >
                  {updateProjectSettingsMut.isPending && <Loader2 className="w-3 h-3 animate-spin" />}
                  {updateProjectSettingsMut.isPending ? 'Saving…' : 'Save'}
                </button>
              </div>
            </form>
          </div>
        )}
      </Modal>

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
                <p className="text-xs text-text-tertiary mb-3">{selectedProject.description}</p>
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
                <div className="text-center py-8 text-xs text-text-tertiary">Loading memories...</div>
              ) : !projectMemories?.length ? (
                <div className="text-center py-8 text-xs text-text-tertiary border border-dashed border-border-secondary rounded-[18px]">
                  No memories stored for this project.
                </div>
              ) : (
                projectMemories.map(memory => (
                  <div
                    key={memory.id}
                    className="p-3 rounded-[11px] border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] space-y-1"
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
