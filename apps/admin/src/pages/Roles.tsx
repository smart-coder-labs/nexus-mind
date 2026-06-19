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
        <h1 className="text-lg font-semibold text-text-primary">Roles & Permissions</h1>
        <p className="text-[12px] text-text-tertiary mt-0.5">Define custom roles and manage fine-grained permissions.</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Roles List */}
        <div className="lg:col-span-2 space-y-4">
          <div className="border border-border-primary rounded-xl overflow-hidden bg-bg-primary">
            <div className="px-4 py-3 border-b border-border-secondary bg-surface-secondary/40">
              <span className="text-xs font-semibold text-text-secondary">Active Roles</span>
            </div>
            <div className="divide-y divide-border-secondary">
              {isLoading ? (
                <div className="p-4 text-center text-sm text-text-tertiary">Loading roles...</div>
              ) : roles?.length === 0 ? (
                <div className="p-4 text-center text-sm text-text-tertiary">No roles defined yet.</div>
              ) : (
                roles?.map(role => (
                  <div key={role.id} className="p-4 flex items-start justify-between gap-4">
                    <div className="space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium text-text-primary">{role.display_name}</span>
                        <span className="text-xs text-text-tertiary font-mono">({role.name})</span>
                        {role.is_template ? (
                          <span className="text-[10px] bg-accent-blue/10 text-accent-blue px-1.5 py-0.5 rounded font-medium">
                            Template
                          </span>
                        ) : (
                          <span className="text-[10px] bg-status-success/15 text-status-success px-1.5 py-0.5 rounded font-medium">
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
                            className="text-[10px] border border-border-secondary bg-surface-secondary text-text-secondary px-1.5 py-0.5 rounded"
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
                        className="p-1.5 rounded-lg text-text-tertiary hover:text-status-error hover:bg-surface-secondary/60 transition-colors"
                        title="Delete Role"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    )}
                  </div>
                ))
              )}
            </div>
          </div>
        </div>

        {/* Create Role Form */}
        <div className="space-y-4">
          <div className="border border-border-primary rounded-xl p-5 bg-bg-primary space-y-4">
            <div>
              <h3 className="text-sm font-semibold text-text-primary flex items-center gap-2">
                <Shield className="w-4 h-4 text-accent-blue" />
                Create Custom Role
              </h3>
              <p className="text-[11px] text-text-tertiary mt-0.5">Customize specific access guidelines</p>
            </div>

            {errorMsg && (
              <div className="p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-lg">
                {errorMsg}
              </div>
            )}

            <form onSubmit={handleSubmit} className="space-y-4 text-xs">
              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
                  Role Name (slug)
                </label>
                <input
                  type="text"
                  placeholder="e.g. security-officer"
                  value={name}
                  onChange={e => setName(e.target.value)}
                  className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/40"
                  required
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
                  Display Name
                </label>
                <input
                  type="text"
                  placeholder="e.g. Security Officer"
                  value={displayName}
                  onChange={e => setDisplayName(e.target.value)}
                  className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/40"
                  required
                />
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
                  Description
                </label>
                <textarea
                  placeholder="What is this role for?"
                  value={description}
                  onChange={e => setDescription(e.target.value)}
                  className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent-blue/40 h-20 resize-none"
                />
              </div>

              <div className="space-y-2">
                <label className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider block">
                  Permissions
                </label>
                <div className="space-y-1.5 max-h-48 overflow-y-auto border border-border-secondary p-2 rounded-lg bg-surface-secondary/20">
                  {AVAILABLE_PERMISSIONS.map(perm => (
                    <label key={perm.key} className="flex items-start gap-2 p-1.5 rounded hover:bg-surface-secondary/40 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={selectedPermissions.includes(perm.key)}
                        onChange={() => handlePermissionToggle(perm.key)}
                        className="mt-0.5 rounded border-border-primary text-accent-blue focus:ring-accent-blue/30"
                      />
                      <div>
                        <div className="font-medium text-text-secondary text-[11px]">{perm.name}</div>
                        <div className="text-[10px] text-text-tertiary leading-tight">{perm.description}</div>
                      </div>
                    </label>
                  ))}
                </div>
              </div>

              <button
                type="submit"
                disabled={createMut.isPending}
                className="w-full flex items-center justify-center gap-2 px-3 py-2.5 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Plus className="w-4 h-4" />
                Create Role
              </button>
            </form>
          </div>
        </div>
      </div>
    </div>
  )
}
