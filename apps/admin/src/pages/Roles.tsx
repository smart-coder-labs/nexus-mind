import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Shield, Trash2, Plus, Users, X, UserMinus, Search } from 'lucide-react'
import type { CustomRole } from '../types'

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
      <div>
        <h1 className="text-base font-semibold text-text-primary">Roles & Permissions</h1>
        <p className="text-xs text-text-quaternary mt-0.5">Define custom roles and manage fine-grained permissions.</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Roles List */}
        <div className="lg:col-span-2 space-y-4">
          <div className="space-y-3">
            <span className="text-xs font-semibold text-text-secondary">Active Roles</span>
            {deleteErrorMsg && (
              <div className="p-2 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[8px]">
                {deleteErrorMsg}
              </div>
            )}
            {isLoading ? (
              Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="bg-[#272729] rounded-[18px] p-5 border border-border-primary space-y-2 animate-pulse">
                  <div className="h-3.5 rounded-[5px] bg-[#1d1d1f] w-1/3" />
                  <div className="h-2.5 rounded-[5px] bg-[#1d1d1f] w-2/3" />
                  <div className="flex gap-1">
                    {Array.from({ length: 3 }).map((_, j) => (
                      <div key={j} className="h-4 w-16 rounded-[5px] bg-[#1d1d1f]" />
                    ))}
                  </div>
                </div>
              ))
            ) : roles?.length === 0 ? (
              <div className="bg-[#272729] rounded-[18px] p-5 border border-border-primary flex flex-col items-center gap-2 py-12 text-center">
                <Shield className="w-6 h-6 text-text-quaternary/50" />
                <p className="text-sm font-semibold text-text-secondary">No custom roles yet</p>
                <p className="text-xs text-text-quaternary max-w-xs">Create a custom role on the right to define fine-grained permission sets for your team.</p>
              </div>
            ) : (
              roles?.map(role => {
                const roleDescription = role.description || ROLE_DESCRIPTIONS[role.name] || null
                const userCount = (role as any).user_count
                return (
                  <div key={role.id} className="bg-[#272729] rounded-[18px] p-5 border border-border-primary flex items-start justify-between gap-4">
                    <div className="space-y-1 flex-1 min-w-0">
                      <div className="flex items-center gap-2 flex-wrap">
                        <span className="text-xs font-semibold text-text-primary">{role.display_name}</span>
                        <span className="text-xs text-text-tertiary font-mono">({role.name})</span>
                        {role.is_template ? (
                          <span className="text-[10px] bg-white/[0.06] text-text-quaternary px-1.5 py-0.5 rounded-[5px]">
                            System
                          </span>
                        ) : (
                          <span className="text-[10px] bg-status-success/15 text-status-success px-1.5 py-0.5 rounded-[5px] font-semibold">
                            Custom
                          </span>
                        )}
                        {/* Member count badge */}
                        <span className="rounded-[5px] bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-text-secondary flex items-center gap-1">
                          <Users className="w-3 h-3" />
                          {userCount != null ? userCount : '—'}
                        </span>
                      </div>
                      {roleDescription && (
                        <p className="text-[10px] text-text-quaternary">{roleDescription}</p>
                      )}
                      <div className="flex flex-wrap gap-1 mt-2">
                        {role.permissions.map(p => (
                          <span
                            key={p}
                            className="text-[10px] border border-border-secondary bg-[#272729] text-text-secondary px-1.5 py-0.5 rounded-[5px]"
                          >
                            {p}
                          </span>
                        ))}
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
                            if (confirm(`Are you sure you want to delete the role "${role.display_name}"?`)) {
                              deleteMut.mutate(role.id)
                            }
                          }}
                          aria-label={`Delete role ${role.display_name}`}
                          disabled={deleteMut.isPending}
                          className="p-1.5 rounded-[8px] text-text-tertiary hover:text-status-error hover:bg-[#272729]/60 transition-colors disabled:opacity-40"
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
          <div className="border border-border-primary rounded-[18px] p-5 bg-[#272729] space-y-4">
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
                <div className="space-y-1.5 max-h-48 overflow-y-auto border border-border-secondary p-2 rounded-[11px] bg-[#272729]/20">
                  {AVAILABLE_PERMISSIONS.map(perm => (
                    <label key={perm.key} className="flex items-start gap-2 p-1.5 rounded-[8px] hover:bg-[#272729]/40 cursor-pointer">
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
          <div className="bg-[#1d1d1f] border border-border-primary rounded-[18px] w-full max-w-md shadow-2xl flex flex-col max-h-[80vh]">
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
                <p className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
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
                <p className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
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
    </>
  )
}
