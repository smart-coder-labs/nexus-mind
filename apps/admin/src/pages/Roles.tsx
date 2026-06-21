import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Shield, Trash2, Plus } from 'lucide-react'

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
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div>
        <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Roles & Permissions</h1>
        <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">Define custom roles and manage fine-grained permissions.</p>
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
              roles?.map(role => (
                <div key={role.id} className="bg-[#272729] rounded-[18px] p-5 border border-border-primary flex items-start justify-between gap-4">
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-text-primary">{role.display_name}</span>
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
                    </div>
                    {role.description && (
                      <p className="text-xs text-text-tertiary">{role.description}</p>
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
              ))
            )}
          </div>
        </div>

        {/* Create Role Form */}
        <div className="space-y-4">
          <div className="border border-border-primary rounded-[18px] p-5 bg-[#272729] space-y-4">
            <div>
              <h3 className="text-sm font-semibold text-text-primary flex items-center gap-2">
                <Shield className="w-4 h-4 text-accent-blue" />
                Create Custom Role
              </h3>
              <p className="text-[11px] text-text-tertiary mt-0.5">Customize specific access guidelines</p>
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
                        <div className="font-semibold text-text-secondary text-[11px]">{perm.name}</div>
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
  )
}
