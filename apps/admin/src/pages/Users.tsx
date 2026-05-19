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
    active:    'bg-green-500',
    invited:   'bg-yellow-500',
    suspended: 'bg-red-500',
  }
  return (
    <span className="flex items-center gap-1.5">
      <span className={`w-1.5 h-1.5 rounded-full ${colors[status]}`} />
      <span className="capitalize text-white/50">{status}</span>
    </span>
  )
}

function roleBadge(role: User['role']) {
  const styles: Record<User['role'], string> = {
    admin:  'text-accent-blue border-accent-blue/30',
    member: 'text-white/40 border-white/10',
    viewer: 'text-white/30 border-white/10',
  }
  return (
    <span className={`text-[11px] border rounded px-1.5 py-0.5 capitalize ${styles[role]}`}>
      {role}
    </span>
  )
}

export default function Users() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(session!.apiKey), [session])

  const [inviteOpen, setInviteOpen] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<User | null>(null)
  const [rotateTarget, setRotateTarget] = useState<User | null>(null)
  const [newKey, setNewKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
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
          <h1 className="text-lg font-semibold text-white">Users</h1>
          <p className="text-[12px] text-white/30 mt-0.5">Manage team members and API keys</p>
        </div>
        <button
          onClick={() => setInviteOpen(true)}
          className="flex items-center gap-2 px-3 py-2 rounded-lg bg-white text-[#0c0c0e] text-sm font-medium hover:bg-white/90 transition-colors"
        >
          <UserPlus className="w-4 h-4" />
          Invite user
        </button>
      </div>

      <div className="border border-white/8 rounded-xl overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-white/5">
              <th className="text-left px-4 py-3 text-[11px] text-white/30 uppercase tracking-wide font-normal">User</th>
              <th className="text-left px-4 py-3 text-[11px] text-white/30 uppercase tracking-wide font-normal">Role</th>
              <th className="text-left px-4 py-3 text-[11px] text-white/30 uppercase tracking-wide font-normal">Status</th>
              <th className="text-right px-4 py-3 text-[11px] text-white/30 uppercase tracking-wide font-normal">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-white/5">
            {isLoading
              ? Array.from({ length: 4 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 4 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded bg-white/5 animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : users?.map(user => (
                <tr key={user.id} className="hover:bg-white/[0.02] transition-colors">
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <div className="w-7 h-7 rounded-full bg-white/8 flex items-center justify-center text-xs font-medium text-white/60">
                        {user.name[0].toUpperCase()}
                      </div>
                      <div>
                        <p className="text-white/80 font-medium leading-tight">{user.name}</p>
                        <p className="text-white/30 text-xs">{user.email}</p>
                      </div>
                    </div>
                  </td>
                  <td className="px-4 py-3">{roleBadge(user.role)}</td>
                  <td className="px-4 py-3 text-xs">{statusDot(user.status)}</td>
                  <td className="px-4 py-3">
                    <div className="flex items-center justify-end gap-2">
                      <button
                        onClick={() => setRotateTarget(user)}
                        className="text-xs text-white/30 hover:text-white/60 transition-colors px-2 py-1 rounded hover:bg-white/5"
                      >
                        Rotate key
                      </button>
                      <button
                        onClick={() => setRevokeTarget(user)}
                        className="text-xs text-red-400/60 hover:text-red-400 transition-colors px-2 py-1 rounded hover:bg-red-400/5"
                      >
                        Revoke
                      </button>
                    </div>
                  </td>
                </tr>
              ))
            }
          </tbody>
        </table>
        {!isLoading && users?.length === 0 && (
          <p className="text-center text-white/20 text-sm py-10">No users yet.</p>
        )}
      </div>

      {/* Modals */}
      <InviteUserModal
        open={inviteOpen}
        client={client}
        onClose={() => setInviteOpen(false)}
        onSuccess={() => qc.invalidateQueries({ queryKey: ['users'] })}
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
          <div className="bg-[#161618] border border-white/8 rounded-xl p-6 w-full max-w-md space-y-4">
            <p className="text-white font-medium">New API key generated</p>
            <p className="text-xs text-white/40">Copy this key now — it won't be shown again.</p>
            <div className="flex items-center gap-2 bg-white/5 rounded-lg px-3 py-2">
              <code className="flex-1 text-xs text-white/70 break-all">{newKey}</code>
              <button
                onClick={() => handleCopy(newKey)}
                className="text-xs text-white/40 hover:text-white/70 transition-colors shrink-0"
              >
                {copied ? 'Copied!' : 'Copy'}
              </button>
            </div>
            <button
              onClick={() => { setNewKey(null); setCopied(false) }}
              className="w-full py-2 rounded-lg bg-white text-[#0c0c0e] text-sm font-medium hover:bg-white/90 transition-colors"
            >
              Done
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
