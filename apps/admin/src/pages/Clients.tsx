import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import {
  Building2, Plus, Users, UserPlus, UserMinus,
  ChevronDown, Loader2, Archive, Trash2, Settings, Search,
} from 'lucide-react'
import { cn } from '../lib/utils'
import { Modal, ModalCloseButton } from '../components/ui/Modal/Modal'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import { StatTile } from './dashboard/StatTile'
import { accentFor } from './dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { CLIENT_STATUSES } from '../types'
import type { Client, ClientMember, ClientStatus, User as UserType } from '../types'

// Same glass recipe used across the admin pages (see Projects.tsx / StatTile).
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// ─── Slug validation — mirrors backend `validate_slug` in models/types.rs ─────
//
// Lowercase alphanumeric with internal dashes, 1–64 chars, must start with a
// lowercase letter or digit. Validated client-side to give instant feedback;
// the backend re-validates authoritatively.
function slugError(slug: string): string | null {
  if (slug.length === 0 || slug.length > 64) return 'Slug must be 1–64 characters.'
  if (!/^[a-z0-9]/.test(slug)) return 'Slug must start with a lowercase letter or digit.'
  if (!/^[a-z0-9-]+$/.test(slug)) return 'Slug may contain only lowercase letters, digits and dashes.'
  return null
}

/** Derives a kebab-case slug candidate from a free-text name. */
function slugify(name: string): string {
  return name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64)
}

const STATUS_BADGE: Record<ClientStatus, string> = {
  active: 'bg-status-success/10 text-status-success border-status-success/20',
  paused: 'bg-status-warning/10 text-status-warning border-status-warning/20',
  offboarded: 'bg-white/[0.06] text-text-tertiary border-white/[0.09]',
}

// ─── Inline Members Panel ─────────────────────────────────────────────────────

interface MembersPanelProps {
  clientId: string
  clientName: string
  client: ReturnType<typeof createClient>
  users: UserType[] | undefined
  usersLoading: boolean
  allAvailableRoles: string[]
}

function MembersPanel({
  clientId,
  clientName,
  client,
  users,
  usersLoading,
  allAvailableRoles,
}: MembersPanelProps) {
  const qc = useQueryClient()
  const [addUserId, setAddUserId] = useState('')
  const [addRole, setAddRole] = useState('member')
  const [addError, setAddError] = useState('')
  const [addSaved, setAddSaved] = useState(false)

  const { data: members, isLoading: membersLoading } = useQuery({
    queryKey: ['client-members', clientId],
    queryFn: () => client.listClientMembers(clientId),
    enabled: !!clientId,
  })

  const addMut = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      client.addClientMember(clientId, userId, role),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['client-members', clientId] })
      setAddUserId('')
      setAddRole('member')
      setAddError('')
      setAddSaved(true)
      setTimeout(() => setAddSaved(false), 2000)
    },
    onError: (err: any) => setAddError(err.message || 'Failed to add member'),
  })

  const removeMut = useMutation({
    mutationFn: (userId: string) => client.removeClientMember(clientId, userId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['client-members', clientId] }),
  })

  const memberIds = useMemo(
    () => new Set((members ?? []).map((m: ClientMember) => m.user_id)),
    [members],
  )
  const availableUsers = useMemo(
    () => (users ?? []).filter(u => !memberIds.has(u.id) && u.status === 'active'),
    [users, memberIds],
  )

  const handleAdd = (e: React.FormEvent) => {
    e.preventDefault()
    if (!addUserId) { setAddError('Please select a user.'); return }
    addMut.mutate({ userId: addUserId, role: addRole })
  }

  return (
    <div className="rounded-b-[18px] border border-t-0 border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] px-5 pb-5 pt-4 space-y-4">
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
            <p className="text-xs text-text-tertiary">No members on this client yet.</p>
          </div>
        ) : (
          <div className="pt-1">
            {members.map((member: ClientMember) => {
              const initial = (member.name || member.email || '?')[0].toUpperCase()
              return (
                <div
                  key={member.id}
                  className="flex items-center gap-3 py-2.5 border-b border-border-secondary/50 last:border-b-0"
                >
                  <div className="w-8 h-8 rounded-full bg-accent-blue/15 text-accent-blue text-xs font-semibold flex items-center justify-center shrink-0">
                    {initial}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-semibold text-text-primary truncate">
                      {member.name || member.email}
                    </div>
                    {member.name && (
                      <div className="text-[10px] text-text-quaternary truncate">{member.email}</div>
                    )}
                  </div>
                  <span className="rounded-[5px] px-1.5 py-0.5 text-[10px] font-semibold bg-white/[0.06] border border-white/[0.09] text-text-tertiary shrink-0">
                    {member.role}
                  </span>
                  <button
                    onClick={() => {
                      if (confirm(`Remove ${member.name || member.email} from "${clientName}"?`)) {
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

      {/* Add member */}
      <div className="mt-3 pt-3 border-t border-border-secondary/50 space-y-3">
        <span className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">Add member</span>
        <form onSubmit={handleAdd} className="flex items-center gap-2">
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
      </div>
    </div>
  )
}

// ─── Main Page ─────────────────────────────────────────────────────────────────

export default function Clients() {
  const { session } = useAuth()
  const isAdmin = isPrivileged(session?.user.role)
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [showArchived, setShowArchived] = useState(false)
  const [filterQuery, setFilterQuery] = useState('')
  const [expandedClientId, setExpandedClientId] = useState<string | null>(null)

  // Create modal
  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState('')
  const [slug, setSlug] = useState('')
  const [slugTouched, setSlugTouched] = useState(false)
  const [status, setStatus] = useState<ClientStatus>('active')
  const [createError, setCreateError] = useState('')
  const [created, setCreated] = useState(false)

  // Edit modal
  const [editingClientId, setEditingClientId] = useState<string | null>(null)
  const [editName, setEditName] = useState('')
  const [editStatus, setEditStatus] = useState<ClientStatus>('active')

  // Queries
  const { data: clients, isLoading: clientsLoading, isError, error } = useQuery({
    queryKey: ['clients', showArchived],
    queryFn: () => client.listClients(showArchived),
  })

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

  const allAvailableRoles = useMemo(() => {
    const standard = ['admin', 'member', 'viewer']
    const custom = roles?.map(r => r.name) || []
    return Array.from(new Set([...standard, ...custom]))
  }, [roles])

  const editingClient = useMemo(
    () => clients?.find(c => c.id === editingClientId) ?? null,
    [clients, editingClientId],
  )

  const filteredClients = useMemo(() => {
    const q = filterQuery.trim().toLowerCase()
    if (!q) return clients ?? []
    return (clients ?? []).filter(
      c => c.name.toLowerCase().includes(q) || c.slug.toLowerCase().includes(q),
    )
  }, [clients, filterQuery])

  // Mutations
  const createMut = useMutation({
    mutationFn: (data: { name: string; slug: string; status: ClientStatus }) =>
      client.createClientEntity(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['clients'] })
      setName('')
      setSlug('')
      setSlugTouched(false)
      setStatus('active')
      setCreateError('')
      setCreateOpen(false)
      setCreated(true)
      setTimeout(() => setCreated(false), 2000)
    },
    onError: (err: any) => setCreateError(err.message || 'Failed to create client'),
  })

  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: { name?: string; status?: ClientStatus } }) =>
      client.updateClient(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['clients'] })
      setEditingClientId(null)
    },
  })

  const archiveMut = useMutation({
    mutationFn: (id: string) => client.archiveClient(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['clients'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteClient(id),
    onSuccess: (_, deletedId) => {
      qc.invalidateQueries({ queryKey: ['clients'] })
      if (expandedClientId === deletedId) setExpandedClientId(null)
    },
  })

  // The slug field auto-follows the name until the user edits it directly.
  const effectiveSlug = slugTouched ? slug : slugify(name)
  const currentSlugError = effectiveSlug ? slugError(effectiveSlug) : null

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = name.trim()
    if (!trimmed) { setCreateError('Client name is required.'); return }
    const finalSlug = effectiveSlug
    const se = slugError(finalSlug)
    if (se) { setCreateError(se); return }
    createMut.mutate({ name: trimmed, slug: finalSlug, status })
  }

  const openEdit = (c: Client) => {
    setEditingClientId(c.id)
    setEditName(c.name)
    setEditStatus(c.status)
  }

  const handleUpdate = (e: React.FormEvent) => {
    e.preventDefault()
    if (!editingClient) return
    const data: { name?: string; status?: ClientStatus } = {}
    if (editName.trim() && editName.trim() !== editingClient.name) data.name = editName.trim()
    if (editStatus !== editingClient.status) data.status = editStatus
    updateMut.mutate({ id: editingClient.id, data })
  }

  const statTiles = [
    {
      label: 'Clients',
      value: String(clients?.length ?? 0),
      sub: showArchived ? 'including archived' : 'active view',
      icon: Building2,
    },
    {
      label: 'Active',
      value: String((clients ?? []).filter(c => c.status === 'active' && !c.archived_at).length),
      sub: 'status = active',
      icon: Users,
    },
    {
      label: 'Archived',
      value: String((clients ?? []).filter(c => c.archived_at).length),
      sub: 'in current view',
      icon: Archive,
    },
  ]

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 rounded-[13px] bg-accent-blue/12 flex items-center justify-center shrink-0">
            <Building2 className="w-5 h-5 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-base font-semibold text-text-primary">Clients</h1>
            <p className="text-xs text-text-quaternary mt-0.5">
              Manage consultancy clients and the team members assigned to each.
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
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
            New Client
          </button>
        </div>
      </div>

      <KpiMarquee role="list" aria-label="Client statistics">
        {statTiles.map((tile, i) => (
          <div key={tile.label} className="w-[232px] flex-none">
            <StatTile label={tile.label} value={tile.value} sub={tile.sub} icon={tile.icon} accent={accentFor(i)} />
          </div>
        ))}
      </KpiMarquee>

      <div className={`rounded-[18px] overflow-hidden ${GLASS_PANEL}`}>
        <div className="px-5 py-4 border-b border-border-secondary flex items-center justify-between gap-3 flex-wrap">
          <span className="text-sm font-semibold text-text-primary">Clients</span>
          <div className="flex items-center gap-2 h-8 w-60 px-3 rounded-[10px] border border-border-primary bg-white/[0.02]">
            <Search className="w-3.5 h-3.5 text-text-quaternary shrink-0" />
            <input
              type="text"
              value={filterQuery}
              onChange={e => setFilterQuery(e.target.value)}
              placeholder="Filter clients…"
              aria-label="Filter clients"
              className="flex-1 min-w-0 bg-transparent border-none outline-none text-xs text-text-primary placeholder:text-text-quaternary"
            />
          </div>
        </div>

        {(deleteMut.isError || archiveMut.isError) && (
          <div className="mx-4 mt-3 p-2 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[8px]">
            {((deleteMut.error || archiveMut.error) as Error)?.message ?? 'Action failed'}
          </div>
        )}

        <div className="divide-y divide-border-secondary">
          {clientsLoading ? (
            Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="p-4 flex items-center gap-3">
                <div className="w-8 h-8 rounded-[9px] bg-white/[0.04] animate-pulse flex-shrink-0" />
                <div className="flex-1 space-y-2">
                  <div className="h-3.5 rounded-[5px] bg-white/[0.04] animate-pulse w-1/3" />
                  <div className="h-2.5 rounded-[5px] bg-white/[0.04] animate-pulse w-2/3" />
                </div>
              </div>
            ))
          ) : isError ? (
            <div className="flex flex-col items-center gap-2 py-12 text-center">
              <Building2 className="w-6 h-6 text-status-error/60" />
              <p className="text-xs font-semibold text-text-secondary">Couldn't load clients</p>
              <p className="text-xs text-text-quaternary max-w-xs">{(error as Error)?.message ?? 'Unknown error'}</p>
            </div>
          ) : !clients?.length ? (
            <div className="flex flex-col items-center gap-2 py-12 text-center">
              <Building2 className="w-6 h-6 text-text-quaternary/50" />
              <p className="text-xs font-semibold text-text-secondary">No clients yet</p>
              <p className="text-xs text-text-quaternary max-w-xs">Create your first client using the "New Client" button above to group projects and team members by engagement.</p>
            </div>
          ) : !filteredClients.length ? (
            <div className="flex flex-col items-center gap-2 py-12 text-center">
              <Search className="w-6 h-6 text-text-quaternary/50" />
              <p className="text-xs font-semibold text-text-secondary">No clients match "{filterQuery}"</p>
            </div>
          ) : (
            filteredClients.map(c => {
              const isExpanded = expandedClientId === c.id
              const isArchived = !!c.archived_at
              return (
                <div key={c.id}>
                  <div
                    className={cn(
                      'group p-4 flex items-start justify-between gap-4 transition-colors',
                      isExpanded ? 'bg-accent-blue/10' : 'hover:bg-accent-blue/[0.05]',
                      isArchived && 'opacity-60',
                    )}
                  >
                    <div className="flex items-center gap-3 flex-1 min-w-0">
                      <div className="w-8 h-8 rounded-[9px] bg-accent-blue/12 flex items-center justify-center shrink-0">
                        <Building2 className="w-4 h-4 text-accent-blue" />
                      </div>
                      <div className="min-w-0">
                        <span className="text-xs font-semibold text-text-primary truncate block">{c.name}</span>
                        <div className="flex items-center gap-2 mt-1 flex-wrap">
                          <span className="text-[10px] font-mono text-text-quaternary">{c.slug}</span>
                          <span className={cn('text-[10px] border rounded-[5px] px-1.5 py-0.5', STATUS_BADGE[c.status])}>
                            {c.status}
                          </span>
                          <span className="text-[10px] text-text-tertiary">{new Date(c.created_at).toLocaleDateString()}</span>
                          {isArchived && (
                            <span className="text-[10px] bg-status-warning/10 text-status-warning border border-status-warning/20 rounded-[5px] px-1.5 py-0.5">
                              archived
                            </span>
                          )}
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center gap-1 flex-shrink-0">
                      {!isArchived && (
                        <>
                          <button
                            onClick={() => openEdit(c)}
                            aria-label={`Edit ${c.name}`}
                            title="Edit client"
                            className="p-1.5 rounded-[8px] text-text-tertiary hover:text-text-primary hover:bg-white/[0.10] opacity-0 group-hover:opacity-100 transition-all"
                          >
                            <Settings className="w-3 h-3" />
                          </button>

                          <button
                            onClick={() => setExpandedClientId(prev => (prev === c.id ? null : c.id))}
                            aria-label={isExpanded ? `Collapse members for ${c.name}` : `Expand members for ${c.name}`}
                            aria-expanded={isExpanded}
                            title={isExpanded ? 'Collapse members' : 'Manage members'}
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

                          <button
                            onClick={() => {
                              if (confirm(`Archive client "${c.name}"? It can be restored later.`)) {
                                archiveMut.mutate(c.id)
                              }
                            }}
                            aria-label={`Archive client ${c.name}`}
                            title="Archive client"
                            disabled={archiveMut.isPending}
                            className="p-1.5 rounded-[8px] text-text-quaternary hover:text-status-warning hover:bg-status-warning/10 transition-colors disabled:opacity-40"
                          >
                            <Archive className="w-4 h-4" />
                          </button>
                        </>
                      )}

                      <button
                        onClick={() => {
                          if (confirm(`Delete client "${c.name}"? This cannot be undone. Clients that still own projects cannot be deleted.`)) {
                            deleteMut.mutate(c.id)
                          }
                        }}
                        aria-label={`Delete client ${c.name}`}
                        title="Delete client"
                        disabled={deleteMut.isPending}
                        className="p-1.5 rounded-[8px] text-text-quaternary hover:text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-40"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>

                  {/* Inline members accordion */}
                  <div className={cn('overflow-hidden transition-all duration-200', isExpanded ? 'max-h-[600px]' : 'max-h-0')}>
                    {isExpanded && (
                      <MembersPanel
                        clientId={c.id}
                        clientName={c.name}
                        client={client}
                        users={users}
                        usersLoading={usersLoading}
                        allAvailableRoles={allAvailableRoles}
                      />
                    )}
                  </div>
                </div>
              )
            })
          )}
        </div>
      </div>

      {/* Create Client Modal */}
      <Modal open={createOpen} onOpenChange={setCreateOpen}>
        <ModalCloseButton />
        <div className="rounded-[18px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] p-6 w-full max-w-md">
          <h2 className="text-xs font-semibold text-text-primary mb-1 flex items-center gap-2">
            <Building2 className="w-4 h-4 text-accent-blue" />
            Create Client
          </h2>
          <p className="text-[10px] text-text-quaternary mb-5">
            Register a new consultancy client to group projects and members.
          </p>

          {createError && (
            <div className="mb-4 p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[11px]">
              {createError}
            </div>
          )}

          <form onSubmit={handleCreate} className="space-y-4 text-xs">
            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                Client Name
              </label>
              <input
                type="text"
                placeholder="e.g. Acme Corp"
                value={name}
                onChange={e => setName(e.target.value)}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60"
                required
              />
            </div>

            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                Slug (immutable)
              </label>
              <input
                type="text"
                placeholder="e.g. acme-corp"
                value={effectiveSlug}
                onChange={e => { setSlug(e.target.value); setSlugTouched(true) }}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary font-mono focus:outline-none focus:border-accent-blue/60"
              />
              {currentSlugError ? (
                <p className="text-[10px] text-status-error/80">{currentSlugError}</p>
              ) : (
                <p className="text-[10px] text-text-quaternary">Lowercase letters, digits and dashes. Cannot be changed later.</p>
              )}
            </div>

            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                Status
              </label>
              <Select value={status} onValueChange={v => setStatus(v as ClientStatus)}>
                <SelectTrigger className="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {CLIENT_STATUSES.map(s => (
                    <SelectItem key={s} value={s}>{s}</SelectItem>
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
                disabled={createMut.isPending || !!currentSlugError}
                className="flex items-center gap-2 px-4 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white font-semibold transition-colors disabled:opacity-50"
              >
                <Plus className="w-3.5 h-3.5" />
                {createMut.isPending ? 'Creating…' : created ? 'Created!' : 'Create Client'}
              </button>
            </div>
          </form>
        </div>
      </Modal>

      {/* Edit Client Modal */}
      <Modal open={!!editingClientId} onOpenChange={(open) => { if (!open) setEditingClientId(null) }}>
        <ModalCloseButton />
        {editingClient && (
          <div className="rounded-[18px] border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] p-6 w-full max-w-md">
            <h2 className="text-xs font-semibold text-text-primary mb-1 flex items-center gap-2">
              <Settings className="w-4 h-4 text-accent-blue" />
              Edit Client
            </h2>
            <p className="text-[10px] text-text-quaternary mb-5 font-mono">{editingClient.slug}</p>

            {updateMut.isError && (
              <div className="mb-4 p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[11px]">
                {(updateMut.error as Error)?.message ?? 'Failed to save client'}
              </div>
            )}

            <form onSubmit={handleUpdate} className="space-y-4 text-xs">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Client Name</label>
                <input
                  type="text"
                  value={editName}
                  onChange={e => setEditName(e.target.value)}
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60"
                  required
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Slug (immutable)</label>
                <input
                  type="text"
                  value={editingClient.slug}
                  readOnly
                  disabled
                  className="w-full bg-white/[0.02] border border-border-primary rounded-[11px] px-3 py-2 text-text-quaternary font-mono cursor-not-allowed"
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">Status</label>
                <Select value={editStatus} onValueChange={v => setEditStatus(v as ClientStatus)}>
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {CLIENT_STATUSES.map(s => (
                      <SelectItem key={s} value={s}>{s}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex items-center justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={() => setEditingClientId(null)}
                  className="px-4 py-2 rounded-full border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={updateMut.isPending}
                  className="px-4 py-2 rounded-full bg-accent-blue text-white text-xs font-semibold hover:opacity-90 disabled:opacity-50 flex items-center gap-1.5"
                >
                  {updateMut.isPending && <Loader2 className="w-3 h-3 animate-spin" />}
                  {updateMut.isPending ? 'Saving…' : 'Save'}
                </button>
              </div>
            </form>
          </div>
        )}
      </Modal>
    </div>
  )
}
