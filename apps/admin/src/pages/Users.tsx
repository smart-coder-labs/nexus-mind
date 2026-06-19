import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { User } from '../types'
import { InviteUserModal } from '../components/InviteUserModal'
import { ConfirmModal } from '../components/ConfirmModal'
import { UserPlus } from 'lucide-react'

function statusDot(status: User['status']) {
  const colors: Record<User['status'], string> = {
    active:    'bg-status-success',
    invited:   'bg-status-warning',
    suspended: 'bg-status-error',
  }
  return (
    <span className="flex items-center gap-1.5">
      <span className={`w-1.5 h-1.5 rounded-full ${colors[status]}`} />
      <span className="capitalize text-text-tertiary">{status}</span>
    </span>
  )
}

function roleBadge(role: string) {
  const styles: Record<string, string> = {
    admin:  'text-accent-blue border-accent-blue/30 bg-accent-blue/5',
    member: 'text-text-tertiary border-border-primary',
    viewer: 'text-text-quaternary border-border-secondary',
  }
  const cls = styles[role] || 'text-status-success border-status-success/30 bg-status-success/5'
  return (
    <span className={`text-[11px] border rounded-[5px] px-1.5 py-0.5 capitalize ${cls}`}>
      {role}
    </span>
  )
}

export default function Users() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [inviteOpen, setInviteOpen] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<User | null>(null)
  const [rotateTarget, setRotateTarget] = useState<User | null>(null)
  const [newKey, setNewKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
  })

  const { data: roles } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
    enabled: session?.user.role === 'admin',
  })

  const updateRoleMut = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) => client.updateUserRole(userId, role),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['users'] }) },
  })

  const revokeMut = useMutation({
    mutationFn: (id: string) => client.removeUser(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['users'] }); setRevokeTarget(null) },
  })

  const rotateMut = useMutation({
    mutationFn: (id: string) => client.rotateKey(id),
    onSuccess: (data) => { qc.invalidateQueries({ queryKey: ['users'] }); setRotateTarget(null); setNewKey(data.api_key) },
  })

  const handleCopy = (key: string) => {
    navigator.clipboard.writeText(key)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Users</h1>
          <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">Manage team members and API keys</p>
        </div>
        {session?.user.role === 'admin' && (
          <button
            onClick={() => setInviteOpen(true)}
            className="flex items-center gap-2 px-3 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal transition-colors"
          >
            <UserPlus className="w-4 h-4" />
            Invite user
          </button>
        )}
      </div>

      <div className="border border-border-primary rounded-[18px] overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border-secondary">
              <th className="text-left px-4 py-3 text-[12px] text-text-tertiary tracking-[-0.12px] font-normal">User</th>
              <th className="text-left px-4 py-3 text-[12px] text-text-tertiary tracking-[-0.12px] font-normal">Role</th>
              <th className="text-left px-4 py-3 text-[12px] text-text-tertiary tracking-[-0.12px] font-normal">Status</th>
              <th className="text-right px-4 py-3 text-[12px] text-text-tertiary tracking-[-0.12px] font-normal">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-secondary">
            {isLoading
              ? Array.from({ length: 4 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 4 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded bg-surface-secondary animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : users?.map(user => (
                <tr key={user.id} className="hover:bg-surface-secondary/40 transition-colors">
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <div className="w-7 h-7 rounded-full bg-surface-secondary flex items-center justify-center text-xs font-semibold text-text-secondary">
                        {user.name[0].toUpperCase()}
                      </div>
                      <div>
                        <p className="text-text-secondary font-semibold leading-tight">{user.name}</p>
                        <p className="text-text-tertiary text-xs">{user.email}</p>
                      </div>
                    </div>
                  </td>
                  <td className="px-4 py-3">
                    {session?.user.role === 'admin' ? (
                      <select
                        value={user.role}
                        onChange={e => updateRoleMut.mutate({ userId: user.id, role: e.target.value })}
                        disabled={updateRoleMut.isPending}
                        className="bg-transparent border border-border-primary rounded-[5px] px-2 py-0.5 text-xs text-text-secondary focus:outline-none focus:border-border-focus"
                      >
                        <option value="admin">Admin</option>
                        <option value="member">Member</option>
                        <option value="viewer">Viewer</option>
                        {roles?.map(r => (
                          <option key={r.id} value={r.name}>
                            {r.display_name}
                          </option>
                        ))}
                      </select>
                    ) : (
                      roleBadge(user.role)
                    )}
                  </td>
                  <td className="px-4 py-3 text-xs">{statusDot(user.status)}</td>
                  <td className="px-4 py-3">
                    <div className="flex items-center justify-end gap-2">
                      {/* Rotate own key: visible to all roles. Rotate other's key: admin only */}
                      {(session?.user.role === 'admin' || user.id === session?.user.id) && (
                        <button
                          onClick={() => setRotateTarget(user)}
                          className="text-xs text-text-tertiary hover:text-text-secondary transition-colors px-2 py-1 rounded-[5px] hover:bg-surface-secondary"
                        >
                          Rotate key
                        </button>
                      )}
                      {session?.user.role === 'admin' && (
                        <button
                          onClick={() => setRevokeTarget(user)}
                          className="text-xs text-status-error/60 hover:text-status-error transition-colors px-2 py-1 rounded-[5px] hover:bg-surface-secondary"
                        >
                          Revoke
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))
            }
          </tbody>
        </table>
        {!isLoading && users?.length === 0 && (
          <p className="text-center text-text-quaternary text-sm py-10">No users yet.</p>
        )}
      </div>

      {/* Modals */}
      <InviteUserModal
        open={inviteOpen}
        client={client}
        onClose={() => setInviteOpen(false)}
        onSuccess={() => qc.invalidateQueries({ queryKey: ['users'] })}
        roles={roles}
      />

      <ConfirmModal
        open={!!revokeTarget}
        title="Revoke access"
        description={`Remove ${revokeTarget?.name} from the organization? Their API key will stop working immediately.`}
        confirmLabel="Revoke"
        danger
        loading={revokeMut.isPending}
        onConfirm={() => revokeTarget && revokeMut.mutate(revokeTarget.id)}
        onClose={() => setRevokeTarget(null)}
      />

      <ConfirmModal
        open={!!rotateTarget}
        title="Rotate API key"
        description={`Generate a new API key for ${rotateTarget?.name}? The current key will stop working immediately.`}
        confirmLabel="Rotate"
        loading={rotateMut.isPending}
        onConfirm={() => rotateTarget && rotateMut.mutate(rotateTarget.id)}
        onClose={() => setRotateTarget(null)}
      />

      {/* New key reveal */}
      {newKey && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="bg-surface-primary border border-border-primary rounded-[18px] p-6 w-full max-w-md space-y-4">
            <p className="text-text-primary font-semibold">New API key generated</p>
            <p className="text-xs text-text-quaternary">Copy this key now — it won't be shown again.</p>
            <div className="flex items-center gap-2 bg-surface-secondary rounded-[11px] px-3 py-2">
              <code className="flex-1 text-xs text-text-secondary break-all">{newKey}</code>
              <button
                onClick={() => handleCopy(newKey)}
                className="text-xs text-text-tertiary hover:text-text-secondary transition-colors shrink-0"
              >
                {copied ? 'Copied!' : 'Copy'}
              </button>
            </div>
            <button
              onClick={() => { setNewKey(null); setCopied(false) }}
              className="w-full py-2 rounded-full bg-accent-blue text-white text-sm font-normal hover:bg-accent-blue-hover transition-colors"
            >
              Done
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
