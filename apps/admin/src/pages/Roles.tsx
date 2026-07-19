import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Shield, Trash2, Plus, Users, X, UserMinus, Search, KeyRound, Sparkles } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { CustomRole } from '../types'
import { StatTile } from './dashboard/StatTile'
import { accentFor } from './dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { cn } from '../lib/utils'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Roles page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

const ROLE_DESCRIPTIONS: Record<string, string> = {
  admin: 'Full access to all settings and data',
  member: 'Can store and search memories, view projects',
  viewer: 'Read-only access to memories and projects',
}

const AVAILABLE_PERMISSIONS = [
  { key: 'memory:read', name: 'Read Memories', description: 'Allows viewing and reading memories.' },
  { key: 'memory:write', name: 'Write Memories', description: 'Allows storing and updating memories.' },
  { key: 'memory:delete', name: 'Delete Memories', description: 'Allows deleting memories.' },
  { key: 'memory:search', name: 'Search Memories', description: 'Allows searching memories (keyword/semantic).' },
  { key: 'user:invite', name: 'Invite Users', description: 'Allows inviting new users to the organization.' },
  { key: 'user:revoke', name: 'Revoke Users', description: 'Allows suspending or revoking users.' },
  { key: 'audit:read', name: 'Read Audit Logs', description: 'Allows viewing organizational audit logs.' },
  { key: 'settings:write', name: 'Write Settings', description: 'Allows updating organization settings.' },
  { key: 'project:read', name: 'Read Projects', description: 'Allows listing and viewing projects.' },
  { key: 'project:write', name: 'Write Projects', description: 'Allows creating and updating projects.' },
  { key: 'session:read', name: 'Read Sessions', description: 'Allows viewing agent sessions.' },
  { key: 'api_key:read', name: 'Read API Keys', description: 'Allows viewing API keys.' },
  { key: 'convention:read', name: 'Read Conventions', description: 'Allows viewing team conventions.' },
  { key: 'convention:write', name: 'Write Conventions', description: 'Allows creating and updating conventions.' },
  { key: 'policy:read', name: 'Read Policies', description: 'Allows viewing access policies.' },
  { key: 'policy:write', name: 'Write Policies', description: 'Allows creating and updating policies.' },
  { key: 'webhook:read', name: 'Read Webhooks', description: 'Allows viewing webhooks.' },
  { key: 'code:read', name: 'Read Code Index', description: 'Allows searching the code knowledge base.' },
  { key: 'audit:write', name: 'Write Audit Logs', description: 'Allows writing external audit events.' },
  { key: 'code:write', name: 'Code Write', description: 'Allows writing and updating code index entries.' },
  { key: 'code:index', name: 'Code Index', description: 'Allows triggering code indexing operations.' },
  { key: 'collection:read', name: 'Collection Read', description: 'Allows viewing memory collections.' },
  { key: 'collection:write', name: 'Collection Write', description: 'Allows creating and updating memory collections.' },
  { key: 'backup:read', name: 'Backup Read', description: 'Allows viewing and downloading backups.' },
  { key: 'backup:write', name: 'Backup Write', description: 'Allows creating and managing backups.' },
  { key: 'graph:read', name: 'Graph Read', description: 'Allows viewing the memory graph.' },
  { key: 'tag:read', name: 'Tag Read', description: 'Allows viewing memory tags.' },
  { key: 'tag:write', name: 'Tag Write', description: 'Allows creating, renaming, and deleting tags.' },
  { key: 'harness:read', name: 'Read Harnesses', description: 'Allows viewing harness catalog metadata and recommendations.' },
  { key: 'harness:write', name: 'Write Harnesses', description: 'Allows creating harnesses and publishing immutable versions.' },
  { key: 'harness:download', name: 'Download Harnesses', description: 'Allows downloading approved harness manifests.' },
  { key: 'harness:install', name: 'Approve Harness Installs', description: 'Allows approving a harness version and manifest hash for local installation tools.' },
  { key: 'harness:review_config', name: 'Review Harness Config', description: 'Allows sharing and inspecting redacted Claude configuration reviews.' },
]

export default function Roles() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  // Form State
  const [name, setName] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [description, setDescription] = useState('')
  const [selectedPermissions, setSelectedPermissions] = useState<string[]>([])
  const [errorMsg, setErrorMsg] = useState('')

  const { data: roles, isLoading } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
  })

  // Stat tiles derived strictly from the already-fetched `roles` array (no invented numbers).
  const roleStats = useMemo(() => {
    if (!roles) return null
    const systemCount = roles.filter(r => r.is_template).length
    const customCount = roles.length - systemCount
    // Per-role member counts come from an untyped `user_count` field some API responses include
    // (see `userCount` below); only roll it into a total tile when every role has it.
    const memberCountsKnown = roles.every(r => typeof (r as any).user_count === 'number')
    const totalMembers = memberCountsKnown
      ? roles.reduce((sum, r) => sum + (r as any).user_count, 0)
      : null

    const tiles: { label: string; value: string; sub: string; icon: LucideIcon }[] = [
      {
        label: 'Total Roles',
        value: String(roles.length),
        sub: `${systemCount} system · ${customCount} custom`,
        icon: Shield,
      },
      {
        // AVAILABLE_PERMISSIONS is the full permission catalog already defined in this file
        // (used to render the create/edit checklists) — a real app constant, not a fabricated figure.
        label: 'Permissions',
        value: String(AVAILABLE_PERMISSIONS.length),
        sub: 'fine-grained catalog',
        icon: KeyRound,
      },
      {
        label: 'Custom Roles',
        value: String(customCount),
        sub: 'team-defined roles',
        icon: Sparkles,
      },
    ]
    if (totalMembers !== null) {
      tiles.push({
        label: 'Assigned Members',
        value: String(totalMembers),
        sub: 'across all roles',
        icon: Users,
      })
    }
    // A per-system-role breakdown (Admins / Members / Viewers, as in the mockup) is omitted:
    // it would require per-role display data (e.g. an admin's name) that this page only fetches
    // once "Manage members" is opened for that specific role.
    return tiles
  }, [roles])

  // Manage members modal state
  const [managingRole, setManagingRole] = useState<CustomRole | null>(null)
  const [memberSearch, setMemberSearch] = useState('')
  const [addSearch, setAddSearch] = useState('')

  const { data: allUsers } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    enabled: managingRole !== null,
  })

  const { data: roleUsers, isLoading: roleUsersLoading } = useQuery({
    queryKey: ['users-by-role', managingRole?.name],
    queryFn: () => client.getUsersByRole(managingRole!.name),
    enabled: managingRole !== null,
    retry: false,
  })

  const assignRoleMut = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      client.assignUserRole(userId, role),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['users-by-role', managingRole?.name] })
      qc.invalidateQueries({ queryKey: ['users'] })
    },
  })

  const [roleSaved, setRoleSaved] = useState(false)
  const [deleteErrorMsg, setDeleteErrorMsg] = useState('')

  const [editingRole, setEditingRole] = useState<CustomRole | null>(null)
  const [editPermissions, setEditPermissions] = useState<string[]>([])
  const [editSaved, setEditSaved] = useState(false)

  const updatePermsMut = useMutation({
    mutationFn: ({ id, permissions }: { id: string; permissions: string[] }) =>
      client.updateRole(id, permissions),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['roles'] })
      setEditSaved(true)
      setTimeout(() => {
        setEditSaved(false)
        setEditingRole(null)
      }, 1500)
    },
  })

  const createMut = useMutation({
    mutationFn: (data: { name: string; display_name: string; permissions: string[]; description?: string }) =>
      client.createRole(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['roles'] })
      setName('')
      setDisplayName('')
      setDescription('')
      setSelectedPermissions([])
      setErrorMsg('')
      setRoleSaved(true)
      setTimeout(() => setRoleSaved(false), 2000)
    },
    onError: (err: any) => {
      setErrorMsg(err.message || 'Failed to create role')
    },
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteRole(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['roles'] })
    },
    onError: (err: any) => {
      setDeleteErrorMsg(err.message || 'Failed to delete role')
      setTimeout(() => setDeleteErrorMsg(''), 3000)
    },
  })

  const handlePermissionToggle = (permKey: string) => {
    setSelectedPermissions(prev =>
      prev.includes(permKey) ? prev.filter(p => p !== permKey) : [...prev, permKey]
    )
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!name || !displayName) {
      setErrorMsg('Name and Display Name are required.')
      return
    }
    // format name as slug
    const formattedName = name.trim().toLowerCase().replace(/\s+/g, '-')
    createMut.mutate({
      name: formattedName,
      display_name: displayName.trim(),
      description: description.trim() || undefined,
      permissions: selectedPermissions,
    })
  }

  return (
    <>
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-center gap-3">
        <div className="w-11 h-11 rounded-[13px] bg-accent-blue/12 flex items-center justify-center shrink-0">
          <Shield className="w-5 h-5 text-accent-blue" />
        </div>
        <div>
          <h1 className="text-base font-semibold text-text-primary">Roles & Permissions</h1>
          <p className="text-xs text-text-quaternary mt-0.5">Define custom roles and manage fine-grained permissions.</p>
        </div>
      </div>

      {roleStats && (
        <KpiMarquee role="list" aria-label="Role statistics">
          {roleStats.map((t, i) => (
            <div key={t.label} className="w-[232px] flex-none">
              <StatTile label={t.label} value={t.value} sub={t.sub} icon={t.icon} accent={accentFor(i)} />
            </div>
          ))}
        </KpiMarquee>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Roles List */}
        <div className="lg:col-span-2 space-y-4">
          <div className="space-y-3">
            <span className="text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">Active Roles</span>
            {deleteErrorMsg && (
              <div className="p-2 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[8px]">
                {deleteErrorMsg}
              </div>
            )}
            {isLoading ? (
              Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className={`rounded-[18px] p-5 space-y-2 animate-pulse ${GLASS_PANEL}`}>
                  <div className="h-3.5 rounded-[5px] bg-white/[0.06] w-1/3" />
                  <div className="h-2.5 rounded-[5px] bg-white/[0.06] w-2/3" />
                  <div className="flex gap-1">
                    {Array.from({ length: 3 }).map((_, j) => (
                      <div key={j} className="h-4 w-16 rounded-full bg-white/[0.06]" />
                    ))}
                  </div>
                </div>
              ))
            ) : roles?.length === 0 ? (
              <div className={`rounded-[18px] px-5 py-12 flex flex-col items-center gap-2 text-center ${GLASS_PANEL}`}>
                <Shield className="w-6 h-6 text-text-quaternary/50" />
                <p className="text-xs font-semibold text-text-secondary">No custom roles yet</p>
                <p className="text-xs text-text-quaternary max-w-xs">Create a custom role on the right to define fine-grained permission sets for your team.</p>
              </div>
            ) : (
              roles?.map(role => {
                const roleDescription = role.description || ROLE_DESCRIPTIONS[role.name] || null
                const userCount = (role as any).user_count
                return (
                  <div key={role.id} className={`rounded-[18px] p-5 flex items-start justify-between gap-4 ${GLASS_PANEL}`}>
                    <div className="flex items-start gap-3 flex-1 min-w-0">
                      <div
                        className={cn(
                          'w-9 h-9 rounded-[11px] flex items-center justify-center shrink-0',
                          role.is_template ? 'bg-accent-blue/12' : 'bg-status-success/12'
                        )}
                      >
                        <Shield className={cn('w-4 h-4', role.is_template ? 'text-accent-blue' : 'text-status-success')} />
                      </div>
                      <div className="space-y-1 flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="text-xs font-semibold text-text-primary">{role.display_name}</span>
                          <span className="text-xs text-text-tertiary font-mono">({role.name})</span>
                          {role.is_template ? (
                            <span className="text-[10px] font-semibold bg-white/[0.06] text-text-quaternary px-2 py-0.5 rounded-full">
                              System
                            </span>
                          ) : (
                            <span className="text-[10px] font-semibold bg-status-success/15 text-status-success px-2 py-0.5 rounded-full">
                              Custom
                            </span>
                          )}
                          {/* Member count badge */}
                          <span className="rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] text-text-secondary flex items-center gap-1">
                            <Users className="w-3 h-3" />
                            {userCount != null ? userCount : '—'}
                          </span>
                        </div>
                        {roleDescription && (
                          <p className="text-[10px] text-text-quaternary">{roleDescription}</p>
                        )}
                        <div className="space-y-1.5 mt-2">
                          <div className="flex items-center justify-between">
                            <span className="text-[10px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">
                              Permissions
                            </span>
                            <span className="text-[10px] font-semibold text-accent-blue">
                              {role.permissions.length}/{AVAILABLE_PERMISSIONS.length}
                            </span>
                          </div>
                          <div className="h-1 rounded-full bg-white/[0.06] overflow-hidden">
                            <div
                              className="h-full rounded-full bg-accent-blue"
                              style={{
                                width: `${Math.min(100, Math.round((role.permissions.length / AVAILABLE_PERMISSIONS.length) * 100))}%`,
                              }}
                            />
                          </div>
                          <div className="flex flex-wrap gap-1">
                            {role.permissions.map(p => (
                              <span
                                key={p}
                                className="text-[10px] font-mono border border-white/[0.08] bg-white/[0.02] text-text-tertiary px-2 py-0.5 rounded-full"
                              >
                                {p}
                              </span>
                            ))}
                          </div>
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center gap-1.5 shrink-0">
                      <button
                        onClick={() => {
                          setManagingRole(role)
                          setMemberSearch('')
                          setAddSearch('')
                        }}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-border-primary text-[10px] text-text-secondary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
                      >
                        <Users className="w-3.5 h-3.5" />
                        Manage members
                      </button>
                      {!role.is_template && (
                        <button
                          onClick={() => {
                            setEditingRole(role)
                            setEditPermissions(role.permissions)
                            setEditSaved(false)
                          }}
                          className="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-border-primary text-[10px] text-text-secondary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
                        >
                          <Shield className="w-3.5 h-3.5" />
                          Edit permissions
                        </button>
                      )}
                      {!role.is_template && (
                        <button
                          onClick={() => {
                            if (confirm(`Are you sure you want to delete the role "${role.display_name}"?`)) {
                              deleteMut.mutate(role.id)
                            }
                          }}
                          aria-label={`Delete role ${role.display_name}`}
                          disabled={deleteMut.isPending}
                          className="p-1.5 rounded-[8px] text-text-tertiary hover:text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-40"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      )}
                    </div>
                  </div>
                )
              })
            )}
          </div>
        </div>

        {/* Create Role Form */}
        <div className="space-y-4">
          <div className={`rounded-[18px] p-5 space-y-4 ${GLASS_PANEL}`}>
            <div>
              <h3 className="text-xs font-semibold text-text-primary flex items-center gap-2">
                <Shield className="w-4 h-4 text-accent-blue" />
                Create Custom Role
              </h3>
              <p className="text-[10px] text-text-quaternary mt-0.5">Customize specific access guidelines</p>
            </div>

            {errorMsg && (
              <div className="p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[11px]">
                {errorMsg}
              </div>
            )}

            <form onSubmit={handleSubmit} className="space-y-4 text-xs">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                  Role Name (slug)
                </label>
                <input
                  type="text"
                  placeholder="e.g. security-officer"
                  value={name}
                  onChange={e => setName(e.target.value)}
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors"
                  required
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                  Display Name
                </label>
                <input
                  type="text"
                  placeholder="e.g. Security Officer"
                  value={displayName}
                  onChange={e => setDisplayName(e.target.value)}
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors"
                  required
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                  Description
                </label>
                <textarea
                  placeholder="What is this role for?"
                  value={description}
                  onChange={e => setDescription(e.target.value)}
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors h-20 resize-none"
                />
              </div>

              <div className="space-y-2">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] block">
                  Permissions
                </label>
                <div className="space-y-1.5 max-h-48 overflow-y-auto border border-border-secondary p-2 rounded-[11px] bg-white/[0.02]">
                  {AVAILABLE_PERMISSIONS.map(perm => (
                    <label key={perm.key} className="flex items-start gap-2 p-1.5 rounded-[8px] hover:bg-white/[0.04] cursor-pointer">
                      <input
                        type="checkbox"
                        checked={selectedPermissions.includes(perm.key)}
                        onChange={() => handlePermissionToggle(perm.key)}
                        className="mt-0.5 rounded border-border-primary text-accent-blue focus:outline-none"
                      />
                      <div>
                        <div className="font-semibold text-text-secondary text-[10px]">{perm.name}</div>
                        <div className="text-[10px] text-text-tertiary leading-tight">{perm.description}</div>
                      </div>
                    </label>
                  ))}
                </div>
              </div>

              <button
                type="submit"
                disabled={createMut.isPending}
                className="w-full flex items-center justify-center gap-2 px-3 py-2.5 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Plus className="w-4 h-4" />
                {createMut.isPending ? 'Creating…' : roleSaved ? 'Created!' : 'Create Role'}
              </button>
            </form>
          </div>
        </div>
      </div>
    </div>

      {/* Manage members modal */}
      {managingRole && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
          <div className="bg-background-secondary border border-border-primary rounded-[18px] w-full max-w-md shadow-2xl flex flex-col max-h-[80vh]">
            {/* Header */}
            <div className="flex items-center justify-between p-5 border-b border-border-primary">
              <h2 className="text-xs font-semibold text-text-primary">
                Manage {managingRole.display_name} members
              </h2>
              <button
                onClick={() => setManagingRole(null)}
                className="p-1.5 rounded-[8px] text-text-tertiary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto p-5 space-y-5">
              {/* Current members */}
              <div className="space-y-2">
                <p className="text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">
                  Current members
                </p>
                {/* Search members */}
                <div className="relative">
                  <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3 h-3 text-text-quaternary" />
                  <input
                    type="text"
                    placeholder="Filter members…"
                    value={memberSearch}
                    onChange={e => setMemberSearch(e.target.value)}
                    className="w-full bg-white/[0.04] border border-border-primary rounded-[8px] pl-7 pr-3 py-1.5 text-xs text-text-secondary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors"
                  />
                </div>

                {roleUsersLoading ? (
                  <div className="space-y-2">
                    {Array.from({ length: 3 }).map((_, i) => (
                      <div key={i} className="h-9 rounded-[8px] bg-white/[0.04] animate-pulse" />
                    ))}
                  </div>
                ) : roleUsers && roleUsers.length > 0 ? (
                  <div className="space-y-1.5">
                    {roleUsers
                      .filter(u =>
                        !memberSearch ||
                        (u.name ?? '').toLowerCase().includes(memberSearch.toLowerCase()) ||
                        (u.email ?? '').toLowerCase().includes(memberSearch.toLowerCase())
                      )
                      .map(user => (
                        <div
                          key={user.id}
                          className="flex items-center justify-between gap-3 px-3 py-2 rounded-[8px] bg-white/[0.03] border border-border-primary/50"
                        >
                          <div className="min-w-0">
                            <p className="text-xs font-semibold text-text-primary truncate">{user.name ?? user.email ?? user.id}</p>
                            {user.email && user.name && (
                              <p className="text-[10px] text-text-quaternary truncate">{user.email}</p>
                            )}
                          </div>
                          <button
                            onClick={() => assignRoleMut.mutate({ userId: user.id, role: 'viewer' })}
                            disabled={assignRoleMut.isPending}
                            title="Remove from this role (set to viewer)"
                            className="p-1 rounded-[6px] text-text-tertiary hover:text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-40 shrink-0"
                          >
                            <UserMinus className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      ))
                    }
                  </div>
                ) : (
                  <p className="text-xs text-text-quaternary text-center py-4">
                    {roleUsers ? 'No members with this role.' : 'Could not load members for this role.'}
                  </p>
                )}
              </div>

              {/* Add user */}
              <div className="space-y-2">
                <p className="text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">
                  Add user
                </p>
                <div className="relative">
                  <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3 h-3 text-text-quaternary" />
                  <input
                    type="text"
                    placeholder="Search users to assign…"
                    value={addSearch}
                    onChange={e => setAddSearch(e.target.value)}
                    className="w-full bg-white/[0.04] border border-border-primary rounded-[8px] pl-7 pr-3 py-1.5 text-xs text-text-secondary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors"
                  />
                </div>
                {addSearch.trim() && (
                  <div className="space-y-1.5 max-h-40 overflow-y-auto">
                    {(allUsers ?? [])
                      .filter(u =>
                        u.role !== managingRole.name &&
                        (
                          (u.name ?? '').toLowerCase().includes(addSearch.toLowerCase()) ||
                          (u.email ?? '').toLowerCase().includes(addSearch.toLowerCase())
                        )
                      )
                      .slice(0, 8)
                      .map(user => (
                        <div
                          key={user.id}
                          className="flex items-center justify-between gap-3 px-3 py-2 rounded-[8px] bg-white/[0.03] border border-border-primary/50"
                        >
                          <div className="min-w-0">
                            <p className="text-xs font-semibold text-text-primary truncate">{user.name ?? user.email ?? user.id}</p>
                            {user.email && user.name && (
                              <p className="text-[10px] text-text-quaternary truncate">{user.email}</p>
                            )}
                          </div>
                          <button
                            onClick={() => {
                              assignRoleMut.mutate({ userId: user.id, role: managingRole.name })
                              setAddSearch('')
                            }}
                            disabled={assignRoleMut.isPending}
                            className="px-2.5 py-1 rounded-full text-[10px] font-semibold bg-accent-blue hover:bg-accent-blue-hover text-white transition-colors disabled:opacity-40 shrink-0"
                          >
                            Assign
                          </button>
                        </div>
                      ))
                    }
                    {(allUsers ?? []).filter(u =>
                      u.role !== managingRole.name &&
                      (
                        (u.name ?? '').toLowerCase().includes(addSearch.toLowerCase()) ||
                        (u.email ?? '').toLowerCase().includes(addSearch.toLowerCase())
                      )
                    ).length === 0 && (
                      <p className="text-xs text-text-quaternary text-center py-3">No users found.</p>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {editingRole && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
          <div className="bg-background-secondary border border-border-primary rounded-[18px] w-full max-w-md shadow-2xl flex flex-col max-h-[80vh]">
            <div className="flex items-center justify-between p-5 border-b border-border-primary">
              <h2 className="text-xs font-semibold text-text-primary">
                Edit permissions — {editingRole.display_name}
              </h2>
              <button
                onClick={() => setEditingRole(null)}
                className="p-1.5 rounded-[8px] text-text-tertiary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-5 space-y-1.5">
              {AVAILABLE_PERMISSIONS.map(perm => (
                <label key={perm.key} className="flex items-start gap-2 p-1.5 rounded-[8px] hover:bg-white/[0.04] cursor-pointer">
                  <input
                    type="checkbox"
                    checked={editPermissions.includes(perm.key)}
                    onChange={() =>
                      setEditPermissions(prev =>
                        prev.includes(perm.key) ? prev.filter(p => p !== perm.key) : [...prev, perm.key]
                      )
                    }
                    className="mt-0.5 rounded border-border-primary text-accent-blue focus:outline-none"
                  />
                  <div>
                    <div className="font-semibold text-text-secondary text-[10px]">{perm.name}</div>
                    <div className="text-[10px] text-text-tertiary leading-tight">{perm.description}</div>
                  </div>
                </label>
              ))}
            </div>
            <div className="p-5 border-t border-border-primary">
              <button
                onClick={() => updatePermsMut.mutate({ id: editingRole.id, permissions: editPermissions })}
                disabled={updatePermsMut.isPending}
                className="w-full flex items-center justify-center gap-2 px-3 py-2.5 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-semibold transition-colors disabled:opacity-50"
              >
                {updatePermsMut.isPending ? 'Saving…' : editSaved ? 'Saved!' : 'Save permissions'}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  )
}
