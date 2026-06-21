import { useMemo, useState, useEffect, useRef, type ReactNode, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient, NexusMindClient } from '../api/client'
import type { User, AuditEntry } from '../types'
import { InviteUserModal } from '../components/InviteUserModal'
import { InviteLinkModal } from '../components/InviteLinkModal'
import { ConfirmModal } from '../components/ConfirmModal'
import { UserPlus, RefreshCw, Link, X, FileText } from 'lucide-react'

function relativeTime(dateStr: string | null | undefined): ReactNode {
  if (!dateStr) {
    return <span className="text-xs text-text-tertiary italic">Never</span>
  }
  const now = Date.now()
  const ms = now - new Date(dateStr).getTime()
  const hours = ms / (1000 * 60 * 60)
  const days = hours / 24

  let label: string
  let cls: string

  if (hours < 24) {
    const h = Math.floor(hours)
    label = h <= 0 ? 'just now' : `${h}h ago`
    cls = 'text-status-success'
  } else if (days <= 30) {
    label = `${Math.floor(days)}d ago`
    cls = 'text-text-secondary'
  } else {
    label = `${Math.floor(days)}d ago`
    cls = 'text-text-quaternary'
  }

  return <span className={`text-xs ${cls}`}>{label}</span>
}

function statusDot(status: User['status']) {
  const colors: Record<User['status'], string> = {
    active:    'bg-status-success',
    invited:   'bg-status-warning',
    suspended: 'bg-status-error',
  }
  return (
    <span className="flex items-center gap-1.5">
      <span className={`w-2 h-2 rounded-full ${colors[status]}`} />
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

  const [selectedUser, setSelectedUser] = useState<User | null>(null)
  const [inviteOpen, setInviteOpen] = useState(false)
  const [inviteLinkOpen, setInviteLinkOpen] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<User | null>(null)
  const [rotateTarget, setRotateTarget] = useState<User | null>(null)
  const [resetTarget, setResetTarget] = useState<User | null>(null)
  const [newKey, setNewKey] = useState<string | null>(null)
  const [newKeyUser, setNewKeyUser] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [selectedUsers, setSelectedUsers] = useState<Set<string>>(new Set())
  const [selectMode, setSelectMode] = useState(false)

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
  })

  const { data: roles } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
    enabled: session?.user.role === 'admin',
  })

  const [roleSavedFor, setRoleSavedFor] = useState<string | null>(null)

  const updateRoleMut = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) => client.updateUserRole(userId, role),
    onSuccess: (_, { userId }) => {
      qc.invalidateQueries({ queryKey: ['users'] })
      setRoleSavedFor(userId)
      setTimeout(() => setRoleSavedFor(null), 2000)
    },
  })

  const revokeMut = useMutation({
    mutationFn: (id: string) => client.removeUser(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['users'] }); setRevokeTarget(null) },
  })

  const rotateMut = useMutation({
    mutationFn: (id: string) => client.rotateKey(id),
    onSuccess: (data) => { qc.invalidateQueries({ queryKey: ['users'] }); setRotateTarget(null); setNewKeyUser(null); setNewKey(data.api_key) },
  })

  const resetMut = useMutation({
    mutationFn: (id: string) => client.resetUserKey(id),
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ['users'] })
      const user = resetTarget
      setResetTarget(null)
      setNewKeyUser(user?.name ?? null)
      setNewKey(data.new_key)
    },
  })

  const disableMut = useMutation({
    mutationFn: (id: string) => client.disableUser(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['users'] }) },
  })

  const enableMut = useMutation({
    mutationFn: (id: string) => client.enableUser(id),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['users'] }) },
  })

  const bulkEnableMut = useMutation({
    mutationFn: async (ids: string[]) => {
      await Promise.all(ids.map(id => client.enableUser(id)))
    },
    onSuccess: () => {
      setSelectedUsers(new Set())
      setSelectMode(false)
      qc.invalidateQueries({ queryKey: ['users'] })
    },
  })

  const bulkDisableMut = useMutation({
    mutationFn: async (ids: string[]) => {
      await Promise.all(ids.map(id => client.disableUser(id)))
    },
    onSuccess: () => {
      setSelectedUsers(new Set())
      setSelectMode(false)
      qc.invalidateQueries({ queryKey: ['users'] })
    },
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
          <div className="flex items-center gap-2">
            <button
              onClick={() => setInviteLinkOpen(true)}
              className="border border-border-primary rounded-full px-4 py-2 text-sm text-text-secondary hover:text-text-primary flex items-center gap-2 transition-colors"
            >
              <Link className="w-4 h-4" />
              Invite link
            </button>
            <button
              onClick={() => setInviteOpen(true)}
              className="flex items-center gap-2 px-3 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold transition-colors"
            >
              <UserPlus className="w-4 h-4" />
              Invite user
            </button>
          </div>
        )}
      </div>

      <div className="border border-border-primary rounded-[18px] overflow-hidden overflow-x-auto">
        <table className="w-full text-sm min-w-[520px]">
          <thead>
            <tr className="border-b border-border-secondary">
              {session?.user.role === 'admin' && (
                <th className="px-4 py-3 w-8">
                  <input
                    type="checkbox"
                    checked={selectedUsers.size === (users?.length ?? 0) && (users?.length ?? 0) > 0}
                    onChange={e => {
                      setSelectedUsers(e.target.checked ? new Set(users?.map((u: User) => u.id) ?? []) : new Set())
                      setSelectMode(e.target.checked)
                    }}
                    className="rounded border-border-primary bg-white/[0.04] accent-accent-blue w-3.5 h-3.5 cursor-pointer"
                  />
                </th>
              )}
              <th className="text-left px-4 py-3 text-[11px] text-text-quaternary tracking-[-0.12px] font-semibold">User</th>
              <th className="text-left px-4 py-3 text-[11px] text-text-quaternary tracking-[-0.12px] font-semibold">Role</th>
              <th className="text-left px-4 py-3 text-[11px] text-text-quaternary tracking-[-0.12px] font-semibold">Status</th>
              <th className="text-left px-4 py-3 text-[11px] text-text-quaternary tracking-[-0.12px] font-semibold">Last active</th>
              <th className="text-right px-4 py-3 text-[11px] text-text-quaternary tracking-[-0.12px] font-semibold">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-secondary">
            {isLoading
              ? Array.from({ length: 4 }).map((_, i) => (
                <tr key={i}>
                  {session?.user.role === 'admin' && (
                    <td className="px-4 py-3 w-8">
                      <div className="h-3.5 w-3.5 rounded bg-[#272729] animate-pulse" />
                    </td>
                  )}
                  {Array.from({ length: 5 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded-[5px] bg-[#272729] animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : users?.map(user => (
                <tr
                  key={user.id}
                  className={`hover:bg-white/[0.02] transition-colors duration-150 cursor-pointer${user.disabled_at ? ' opacity-60' : ''}`}
                  onClick={() => setSelectedUser(user)}
                >
                  {session?.user.role === 'admin' && (
                    <td className="px-4 py-3 w-8" onClick={e => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        checked={selectedUsers.has(user.id)}
                        onChange={e => {
                          const next = new Set(selectedUsers)
                          e.target.checked ? next.add(user.id) : next.delete(user.id)
                          setSelectedUsers(next)
                          setSelectMode(next.size > 0)
                        }}
                        className="rounded border-border-primary bg-white/[0.04] accent-accent-blue w-3.5 h-3.5 cursor-pointer"
                      />
                    </td>
                  )}
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <div className="w-7 h-7 rounded-full bg-accent-blue/15 border border-accent-blue/20 flex items-center justify-center text-xs font-semibold text-accent-blue">
                        {user.name[0].toUpperCase()}
                      </div>
                      <div>
                        <p className="text-xs text-text-secondary font-semibold leading-tight">{user.name}</p>
                        <p className="text-text-tertiary text-xs">{user.email}</p>
                      </div>
                    </div>
                  </td>
                  <td className="px-4 py-3" onClick={e => e.stopPropagation()}>
                    {session?.user.role === 'admin' ? (
                      <div className="space-y-1">
                        <select
                          value={user.role}
                          onChange={e => updateRoleMut.mutate({ userId: user.id, role: e.target.value })}
                          disabled={updateRoleMut.isPending}
                          className="bg-transparent border border-border-primary rounded-[11px] px-2 py-0.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors"
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
                        {roleSavedFor === user.id && (
                          <p className="text-[10px] text-status-success">Saved!</p>
                        )}
                        {updateRoleMut.isError && updateRoleMut.variables?.userId === user.id && (
                          <p className="text-[10px] text-status-error/80">{(updateRoleMut.error as Error)?.message ?? 'Something went wrong'}</p>
                        )}
                      </div>
                    ) : (
                      roleBadge(user.role)
                    )}
                  </td>
                  <td className="px-4 py-3 text-xs">
                    <div className="flex items-center gap-2">
                      {statusDot(user.status)}
                      {user.disabled_at && (
                        <span className="flex items-center gap-1">
                          <span className="w-2 h-2 rounded-full bg-status-error" />
                          <span className="text-[10px] text-text-quaternary ml-1">Disabled</span>
                        </span>
                      )}
                      {!user.disabled_at && user.status === 'active' && (
                        <span className="text-[10px] text-text-quaternary ml-1">Active</span>
                      )}
                    </div>
                  </td>
                  <td className="px-4 py-3">{relativeTime(user.last_active)}</td>
                  <td className="px-4 py-3" onClick={e => e.stopPropagation()}>
                    <div className="flex items-center justify-end gap-2">
                      {/* Disable / Enable toggle — admin only, cannot self-disable */}
                      {session?.user.role === 'admin' && session.user.id !== user.id && (
                        user.disabled_at ? (
                          <button
                            onClick={() => enableMut.mutate(user.id)}
                            disabled={enableMut.isPending && enableMut.variables === user.id}
                            className="border border-border-secondary/50 rounded-full px-2 py-0.5 text-[10px] text-text-quaternary hover:text-status-success hover:border-status-success/50 transition-colors"
                          >
                            Enable
                          </button>
                        ) : (
                          <button
                            onClick={() => disableMut.mutate(user.id)}
                            disabled={disableMut.isPending && disableMut.variables === user.id}
                            className="border border-border-secondary/50 rounded-full px-2 py-0.5 text-[10px] text-text-quaternary hover:text-status-error hover:border-status-error/50 transition-colors"
                          >
                            Disable
                          </button>
                        )
                      )}
                      {/* Rotate own key: visible to all roles. Rotate other's key: admin only */}
                      {(session?.user.role === 'admin' || user.id === session?.user.id) && (
                        <button
                          onClick={() => setRotateTarget(user)}
                          className="text-xs text-text-tertiary hover:text-text-secondary transition-colors px-2 py-1 rounded-full hover:bg-[#272729]"
                        >
                          Rotate key
                        </button>
                      )}
                      {/* Reset key: admin-only endpoint — useful when a key is compromised */}
                      {session?.user.role === 'admin' && (
                        <button
                          onClick={() => setResetTarget(user)}
                          className="border border-border-primary rounded-[8px] px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary transition-colors flex items-center gap-1"
                        >
                          <RefreshCw className="w-3.5 h-3.5" />
                          Reset key
                        </button>
                      )}
                      {session?.user.role === 'admin' && (
                        <button
                          onClick={() => setRevokeTarget(user)}
                          className="text-xs text-status-error/60 hover:text-status-error transition-colors px-2 py-1 rounded-full hover:bg-[#272729]"
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
          <div className="flex flex-col items-center gap-2 py-16 text-center">
            <UserPlus className="w-6 h-6 text-text-quaternary/50" />
            <p className="text-sm font-semibold text-text-secondary">No team members yet</p>
            <p className="text-xs text-text-quaternary max-w-xs">Invite your first user to start collaborating on memories and projects.</p>
            {session?.user.role === 'admin' && (
              <button
                onClick={() => setInviteOpen(true)}
                className="mt-2 flex items-center gap-2 px-3 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-semibold transition-colors"
              >
                <UserPlus className="w-3.5 h-3.5" />
                Invite user
              </button>
            )}
          </div>
        )}
      </div>

      {/* Bulk action bar */}
      {selectMode && selectedUsers.size > 0 && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 flex items-center gap-3 rounded-full border border-border-primary bg-[#1d1d1f]/90 backdrop-blur-sm px-5 py-2.5 shadow-2xl">
          <span className="text-xs text-text-secondary">{selectedUsers.size} selected</span>
          <div className="w-px h-4 bg-border-primary" />
          <button
            onClick={() => bulkEnableMut.mutate([...selectedUsers])}
            disabled={bulkEnableMut.isPending}
            className="text-xs text-status-success hover:text-status-success/80 transition-colors disabled:opacity-40"
          >
            Enable
          </button>
          <button
            onClick={() => bulkDisableMut.mutate([...selectedUsers])}
            disabled={bulkDisableMut.isPending}
            className="text-xs text-status-error hover:text-status-error/80 transition-colors disabled:opacity-40"
          >
            Disable
          </button>
          <button
            onClick={() => { setSelectedUsers(new Set()); setSelectMode(false) }}
            className="text-text-quaternary hover:text-text-primary transition-colors ml-1"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* Modals */}
      <InviteUserModal
        open={inviteOpen}
        client={client}
        onClose={() => setInviteOpen(false)}
        onSuccess={() => qc.invalidateQueries({ queryKey: ['users'] })}
        roles={roles}
      />

      <InviteLinkModal
        open={inviteLinkOpen}
        client={client}
        onClose={() => setInviteLinkOpen(false)}
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
      {revokeMut.isError && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 bg-[#1d1d1f] border border-status-error/30 rounded-[11px] px-4 py-2 text-xs text-status-error/80 shadow-xl">
          {(revokeMut.error as Error)?.message ?? 'Failed to revoke user'}
        </div>
      )}

      <ConfirmModal
        open={!!rotateTarget}
        title="Rotate API key"
        description={`Generate a new API key for ${rotateTarget?.name}? The current key will stop working immediately.`}
        confirmLabel="Rotate"
        loading={rotateMut.isPending}
        onConfirm={() => rotateTarget && rotateMut.mutate(rotateTarget.id)}
        onClose={() => setRotateTarget(null)}
      />

      <ConfirmModal
        open={!!resetTarget}
        title="Reset API key"
        description={`Reset API key for ${resetTarget?.name}? The old key will stop working immediately.`}
        confirmLabel="Reset"
        danger
        loading={resetMut.isPending}
        onConfirm={() => resetTarget && resetMut.mutate(resetTarget.id)}
        onClose={() => setResetTarget(null)}
      />
      {resetMut.isError && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 bg-[#1d1d1f] border border-status-error/30 rounded-[11px] px-4 py-2 text-xs text-status-error/80 shadow-xl">
          {(resetMut.error as Error)?.message ?? 'Failed to reset API key'}
        </div>
      )}

      {/* New key reveal */}
      {newKey && (
        <NewKeyModal
          userName={newKeyUser}
          apiKey={newKey}
          copied={copied}
          onCopy={handleCopy}
          onClose={() => { setNewKey(null); setNewKeyUser(null); setCopied(false) }}
        />
      )}

      {/* User activity drawer */}
      {selectedUser && (
        <UserActivityDrawer
          user={selectedUser}
          client={client}
          onClose={() => setSelectedUser(null)}
          onNoteUpdate={() => qc.invalidateQueries({ queryKey: ['users'] })}
        />
      )}
    </div>
  )
}

// ── User Activity Drawer ──────────────────────────────────────────────────────

function actionChipCls(action: string): string {
  if (action.startsWith('memory.')) return 'text-accent-blue bg-accent-blue/15 border-accent-blue/30'
  if (action.startsWith('key.') || action.startsWith('invite')) return 'text-status-warning bg-status-warning/10 border-status-warning/30'
  if (action.startsWith('user.') || action === 'revoke') return 'text-status-error bg-status-error/10 border-status-error/30'
  return 'text-text-tertiary bg-[#272729] border-border-primary'
}

function UserActivityFeed({ userId, client }: { userId: string; client: NexusMindClient }) {
  const { data, isLoading, isError } = useQuery<AuditEntry[]>({
    queryKey: ['user-activity', userId],
    queryFn: () => client.getAuditLog({ user_id: userId, limit: 30 }),
    staleTime: 30_000,
  })

  if (isLoading) {
    return (
      <div className="px-5 py-3 space-y-3">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="flex items-center gap-3">
            <div className="h-5 w-20 rounded-[5px] bg-[#272729] animate-pulse shrink-0" />
            <div className="h-3.5 flex-1 rounded-[5px] bg-[#272729] animate-pulse" />
          </div>
        ))}
      </div>
    )
  }

  if (isError) {
    return (
      <div className="px-5 py-8 text-center">
        <p className="text-xs text-status-error/70">Failed to load activity</p>
      </div>
    )
  }

  if (!data || data.length === 0) {
    return (
      <div className="px-5 py-8 text-center">
        <p className="text-xs text-text-quaternary">No activity recorded yet</p>
      </div>
    )
  }

  return (
    <div className="divide-y divide-border-secondary/30">
      {data.map(entry => (
        <div key={entry.id} className="px-5 py-3 flex items-start gap-3">
          <span className={`text-[10px] font-semibold border rounded-[5px] px-1.5 py-0.5 shrink-0 mt-0.5 ${actionChipCls(entry.action)}`}>
            {entry.action}
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[10px] text-text-secondary truncate">
              {entry.resource_type}{entry.resource_id ? ` · ${entry.resource_id.slice(0, 8)}` : ''}
            </p>
            <p className="text-[10px] text-text-quaternary mt-0.5">
              {new Date(entry.timestamp).toLocaleString()}
            </p>
          </div>
        </div>
      ))}
    </div>
  )
}

function UserNoteSection({
  user,
  client,
  onSaved,
}: {
  user: User
  client: NexusMindClient
  onSaved: () => void
}) {
  const [noteOpen, setNoteOpen] = useState(false)
  const [noteInput, setNoteInput] = useState(user.admin_note ?? '')
  const [saving, setSaving] = useState(false)

  const savedNote = user.admin_note ?? null
  const hasNote = !!savedNote

  const handleSave = async () => {
    if (saving) return
    setSaving(true)
    try {
      await client.updateUserNote(user.id, noteInput.trim() || null)
      onSaved()
      setNoteOpen(false)
    } catch {
      // keep open on error
    } finally {
      setSaving(false)
    }
  }

  const handleKeyDown = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && e.ctrlKey) {
      e.preventDefault()
      handleSave()
    }
    if (e.key === 'Escape') {
      setNoteInput(savedNote ?? '')
      setNoteOpen(false)
    }
  }

  return (
    <div className="px-5 py-3 border-b border-border-secondary/30">
      <div className="flex items-center justify-between mb-1.5">
        <p className="text-[10px] font-semibold text-text-quaternary">Admin Note</p>
        <button
          onClick={() => { setNoteInput(savedNote ?? ''); setNoteOpen(v => !v) }}
          className="text-text-quaternary hover:text-text-secondary transition-colors"
          title={hasNote ? 'Edit note' : 'Add note'}
        >
          <FileText className={`w-3 h-3 ${hasNote ? 'text-status-warning' : 'text-text-quaternary'}`} />
        </button>
      </div>
      {!noteOpen && savedNote && (
        <p className="text-xs text-text-tertiary italic leading-relaxed">{savedNote}</p>
      )}
      {!noteOpen && !savedNote && (
        <p className="text-[10px] text-text-quaternary italic">No note — click to add</p>
      )}
      {noteOpen && (
        <div className="space-y-1.5">
          <textarea
            rows={3}
            value={noteInput}
            onChange={e => setNoteInput(e.target.value)}
            onKeyDown={handleKeyDown}
            onBlur={handleSave}
            maxLength={500}
            placeholder="Private admin note…"
            className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary resize-none w-full p-2 focus:outline-none focus:border-accent-blue/60"
          />
          <div className="flex items-center justify-between">
            <span className="text-[10px] text-text-quaternary">{noteInput.length} / 500</span>
            <span className="text-[10px] text-text-quaternary">Ctrl+Enter to save · Esc to cancel</span>
          </div>
        </div>
      )}
    </div>
  )
}

function UserActivityDrawer({
  user,
  client,
  onClose,
  onNoteUpdate,
}: {
  user: User
  client: NexusMindClient
  onClose: () => void
  onNoteUpdate: () => void
}) {
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handleEscape)
    return () => document.removeEventListener('keydown', handleEscape)
  }, [onClose])

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/30 z-40"
        onClick={onClose}
      />
      {/* Drawer panel */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label={`Activity for ${user.name || user.email}`}
        className="fixed right-0 top-0 h-full w-96 bg-[#1d1d1f] border-l border-border-primary z-50 flex flex-col shadow-2xl"
      >
        {/* Header */}
        <div className="flex items-center justify-between p-5 border-b border-border-secondary/50">
          <div>
            <p className="text-xs font-semibold text-text-primary">{user.name || user.email}</p>
            <p className="text-xs text-text-tertiary mt-0.5">{user.role} · {user.email}</p>
            <div className="mt-0.5">
              <p className="text-[10px] text-text-quaternary uppercase tracking-wide">Last login</p>
              {user.last_login_at
                ? <span className="text-xs text-text-secondary">{relativeTime(user.last_login_at)}</span>
                : <span className="text-xs text-text-quaternary italic">Never logged in</span>}
            </div>
          </div>
          <button
            onClick={onClose}
            aria-label="Close"
            className="text-text-quaternary hover:text-text-secondary transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Note section */}
        <UserNoteSection user={user} client={client} onSaved={onNoteUpdate} />

        {/* Section label */}
        <div className="px-5 py-3 border-b border-border-secondary/30">
          <p className="text-[10px] font-semibold text-text-quaternary uppercase tracking-wide">Recent Activity</p>
        </div>

        {/* Feed */}
        <div className="flex-1 overflow-y-auto">
          <UserActivityFeed userId={user.id} client={client} />
        </div>
      </div>
    </>
  )
}

// ── New key reveal modal ──────────────────────────────────────────────────────

function NewKeyModal({ userName, apiKey, copied, onCopy, onClose }: { userName: string | null; apiKey: string; copied: boolean; onCopy: (k: string) => void; onClose: () => void }) {
  const modalRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    document.body.style.overflow = 'hidden'
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handleEscape)
    return () => {
      document.body.style.overflow = ''
      document.removeEventListener('keydown', handleEscape)
    }
  }, [onClose])

  // Focus trap
  useEffect(() => {
    const modal = modalRef.current
    if (!modal) return
    const focusable = modal.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    )
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    first?.focus()
    const trap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last?.focus() }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first?.focus() }
      }
    }
    document.addEventListener('keydown', trap)
    return () => document.removeEventListener('keydown', trap)
  }, [])

  return (
    <div
      className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label={userName ? `New API key for ${userName}` : 'New API key generated'}
    >
      <div ref={modalRef} className="bg-[#272729] border border-border-primary rounded-[18px] p-6 max-w-sm w-full mx-4 space-y-4" onClick={e => e.stopPropagation()}>
        <p className="text-text-primary font-semibold">
          {userName ? `New API key for ${userName}` : 'New API key generated'}
        </p>
        <p className="text-xs text-status-warning">Copy this key now — it won't be shown again.</p>
        <div className="font-mono text-sm bg-[#1d1d1f] rounded-[11px] p-3 break-all select-all text-text-primary border border-border-secondary flex items-center gap-2">
          <span className="flex-1">{apiKey}</span>
          <button
            onClick={() => onCopy(apiKey)}
            className="text-xs text-text-tertiary hover:text-text-secondary transition-colors shrink-0"
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>
        <button
          onClick={onClose}
          className="w-full py-2 rounded-full bg-accent-blue text-white text-sm font-semibold hover:bg-accent-blue-hover transition-colors"
        >
          Done
        </button>
      </div>
    </div>
  )
}
