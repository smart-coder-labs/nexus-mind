import { useMemo, useState, useEffect, useRef, type ReactNode, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient, NexusMindClient } from '../api/client'
import type { User, AuditEntry } from '../types'
import { InviteUserModal } from '../components/InviteUserModal'
import { InviteLinkModal } from '../components/InviteLinkModal'
import { ConfirmModal } from '../components/ConfirmModal'
import {
  UserPlus, Link, X, FileText, Search, KeyRound,
  Users as UsersIcon, UserCheck, Ban, UserMinus, RotateCw, RefreshCcw,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { Badge } from '../components/ui/Badge/Badge'
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from '../components/ui/Select'
import { StatTile } from './dashboard/StatTile'
import { accentFor } from './dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { cn } from '../lib/utils'

// Keyboard focus indicator (design direction §6): 2px --color-focus-ring outline,
// 2px offset. Uses outline (not ring) so it isn't clipped by overflow-hidden ancestors.
const FOCUS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Users page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// Server timestamps from SQLite datetime('now') are naive UTC (no zone). Parse
// them as UTC so past events don't render in the future. No-op for zoned or
// date-only strings (a bare date is already parsed as UTC midnight).
function toDate(iso: string): Date {
  if (/[zZ]$|[+-]\d{2}:?\d{2}$/.test(iso)) return new Date(iso)
  if (/\d{2}:\d{2}/.test(iso)) return new Date(iso.replace(' ', 'T') + 'Z')
  return new Date(iso)
}

// Mockup rule: "just now" only reads green for genuinely fresh activity
// (< 5 minutes) — everything older uses the neutral secondary/tertiary scale
// so the green doesn't get diluted across a whole day of activity.
function relativeTime(dateStr: string | null | undefined): ReactNode {
  if (!dateStr) {
    return <span className="text-[13px] text-text-tertiary">Never</span>
  }
  const now = Date.now()
  const ms = now - toDate(dateStr).getTime()
  const minutes = ms / (1000 * 60)
  const hours = minutes / 60
  const days = hours / 24

  let label: string
  let cls: string

  if (minutes < 5) {
    label = 'just now'
    cls = 'text-status-success'
  } else if (hours < 1) {
    label = `${Math.floor(minutes)}m ago`
    cls = 'text-text-secondary'
  } else if (hours < 24) {
    label = `${Math.floor(hours)}h ago`
    cls = 'text-text-secondary'
  } else if (days <= 30) {
    label = `${Math.floor(days)}d ago`
    cls = 'text-text-secondary'
  } else {
    label = `${Math.floor(days)}d ago`
    cls = 'text-text-tertiary'
  }

  return <span className={`text-[13px] ${cls}`}>{label}</span>
}

// Stable per-user avatar accent: hashes the user's id (falls back to email)
// so a given user always renders the same color regardless of table sort
// order or filtering — unlike `accentFor(rowIndex)`, which reshuffled avatar
// colors every time the visible row order changed.
function hashString(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) {
    h = (h * 31 + s.charCodeAt(i)) | 0
  }
  return Math.abs(h)
}

// Single source of truth for a user's status — rendered once as a Badge. A
// disabled user reads as a neutral "Disabled" regardless of the underlying status.
function statusBadge(user: User) {
  if (user.disabled_at) return <Badge variant="default" size="sm">Disabled</Badge>
  const variant = { active: 'success', invited: 'warning', suspended: 'error' } as const
  const label = user.status.charAt(0).toUpperCase() + user.status.slice(1)
  return <Badge variant={variant[user.status]} size="sm">{label}</Badge>
}

function roleBadge(role: string) {
  const styles: Record<string, string> = {
    admin:  'text-accent-blue border-accent-blue/30 bg-accent-blue/5',
    member: 'text-text-tertiary border-border-primary',
    viewer: 'text-text-tertiary border-border-secondary',
  }
  const cls = styles[role] || 'text-status-success border-status-success/30 bg-status-success/5'
  return (
    <span className={`text-[11px] font-medium border rounded-full px-2 py-0.5 capitalize ${cls}`}>
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
  const [searchQuery, setSearchQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<'active' | 'suspended' | 'disabled' | null>(null)

  const isAdmin = session?.user.role === 'admin' || session?.user.role === 'super_user'

  const { data: users, isLoading } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
  })

  const { data: roles } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
    enabled: isAdmin,
  })

  // Org-wide API keys — admin-only endpoint (see ApiKeys.tsx). Powers the
  // "API keys" stat tile ONLY when it loads (isAdmin), matching the task's
  // "omit if the caller can't see it" rule rather than fabricating a count.
  const { data: orgKeys } = useQuery({
    queryKey: ['org-api-keys'],
    queryFn: () => client.listOrgKeys(),
    enabled: isAdmin,
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

  // Stat tiles are derived strictly from the already-fetched `users` (and,
  // for admins, `orgKeys`) arrays — every sub-caption below is a real
  // aggregate, never a fabricated figure like the mockup's "8 humans · 11 QA".
  const userList = users ?? []
  const activeUsers = userList.filter(u => !u.disabled_at && u.status === 'active')
  const suspendedUsers = userList.filter(u => !u.disabled_at && u.status === 'suspended')
  const activeCount = activeUsers.length
  const suspendedCount = suspendedUsers.length
  const disabledCount = userList.filter(u => u.disabled_at).length
  const activeTodayCount = activeUsers.filter(u => {
    if (!u.last_active) return false
    return Date.now() - toDate(u.last_active).getTime() < 24 * 60 * 60 * 1000
  }).length

  // Mockup order: Active, Suspended, Disabled, API keys, Total users.
  const statTiles: { label: string; value: string; sub?: string; icon: LucideIcon }[] = [
    {
      label: 'Active',
      value: String(activeCount),
      sub: activeTodayCount > 0 ? `${activeTodayCount} online today` : undefined,
      icon: UserCheck,
    },
    {
      label: 'Suspended',
      value: String(suspendedCount),
      // Exactly one suspended user → name them (matches mockup); otherwise a
      // real count-based caption instead of a fabricated one.
      sub: suspendedCount === 1
        ? suspendedUsers[0].name
        : suspendedCount > 1 ? `${suspendedCount} users` : undefined,
      icon: Ban,
    },
    { label: 'Disabled', value: String(disabledCount), icon: UserMinus },
  ]
  if (isAdmin && orgKeys) {
    const activeKeysCount = orgKeys.filter(k => !k.revoked).length
    const revokedKeysCount = orgKeys.length - activeKeysCount
    statTiles.push({
      label: 'API keys',
      value: String(activeKeysCount),
      sub: revokedKeysCount > 0 ? `${revokedKeysCount} revoked` : undefined,
      icon: KeyRound,
    })
  }
  statTiles.push({ label: 'Total users', value: String(userList.length), icon: UsersIcon })

  // Client-side search (name/email) + status filter, applied to the table only
  // — the stat tiles above always reflect the full org, per mockup.
  const filteredUsers = userList.filter(u => {
    const effectiveStatus = u.disabled_at ? 'disabled' : u.status
    if (statusFilter && effectiveStatus !== statusFilter) return false
    if (searchQuery.trim()) {
      const q = searchQuery.trim().toLowerCase()
      return u.name.toLowerCase().includes(q) || u.email.toLowerCase().includes(q)
    }
    return true
  })

  const STATUS_FILTERS: { key: 'active' | 'suspended' | 'disabled'; label: string; dotClass: string; count: number }[] = [
    { key: 'active', label: 'Active', dotClass: 'bg-status-success', count: activeCount },
    { key: 'suspended', label: 'Suspended', dotClass: 'bg-status-error', count: suspendedCount },
    { key: 'disabled', label: 'Disabled', dotClass: 'bg-text-secondary', count: disabledCount },
  ]

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-8">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 rounded-[13px] bg-accent-blue/12 flex items-center justify-center shrink-0">
            <UsersIcon className="w-5 h-5 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">Users</h1>
            <p className="text-[13px] text-text-secondary mt-1">Manage team members and API keys</p>
          </div>
        </div>
        {isAdmin && (
          <div className="flex items-center gap-2">
            <button
              onClick={() => setInviteLinkOpen(true)}
              className={`border border-border-primary rounded-full px-4 py-2 text-[13px] text-text-secondary hover:text-text-primary flex items-center gap-2 transition-colors ${FOCUS}`}
            >
              <Link className="w-4 h-4" />
              Invite link
            </button>
            <button
              onClick={() => setInviteOpen(true)}
              className={`flex items-center gap-2 px-3 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-[13px] font-semibold transition-colors ${FOCUS}`}
            >
              <UserPlus className="w-4 h-4" />
              Invite user
            </button>
          </div>
        )}
      </div>

      {users && (
        <KpiMarquee role="list" aria-label="User statistics">
          {statTiles.map((t, i) => (
            <div key={t.label} className="w-[232px] flex-none">
              <StatTile label={t.label} value={t.value} sub={t.sub} icon={t.icon} accent={accentFor(i)} />
            </div>
          ))}
        </KpiMarquee>
      )}

      {/* Search + status filter toolbar (mockup: glass search input, dot
          filter chips toggling the table below, right-aligned shown count) */}
      <div className="flex items-center gap-2 flex-wrap">
        <div className={`flex items-center gap-2 h-9 w-[280px] px-3.5 rounded-[10px] border border-border-primary bg-[#0d0f14]/60 ${FOCUS}`}>
          <Search className="w-3.5 h-3.5 text-text-quaternary shrink-0" />
          <input
            type="text"
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            placeholder="Search users…"
            aria-label="Search users"
            className="flex-1 min-w-0 bg-transparent border-none outline-none text-text-primary text-[12.5px] placeholder:text-text-quaternary"
          />
        </div>
        {STATUS_FILTERS.map(f => {
          const active = statusFilter === f.key
          return (
            <button
              key={f.key}
              onClick={() => setStatusFilter(prev => (prev === f.key ? null : f.key))}
              aria-pressed={active}
              className={cn(
                'flex items-center gap-1.5 h-8 px-3 rounded-full border text-[12px] font-semibold transition-colors',
                active ? 'border-white/25 bg-white/[0.07] text-text-primary' : 'border-border-primary bg-[#0d0f14]/60 text-text-secondary hover:border-white/25',
                FOCUS,
              )}
            >
              <span className={cn('w-[7px] h-[7px] rounded-full', f.dotClass)} />
              {f.label}
              <span className="text-[10.5px] text-text-quaternary">{f.count}</span>
            </button>
          )
        })}
        <div className="flex-1" />
        <span className="text-[12px] text-text-tertiary">{filteredUsers.length} users</span>
      </div>

      <div className={`rounded-[18px] overflow-hidden overflow-x-auto ${GLASS_PANEL}`}>
        <table className="w-full text-[13px] min-w-[520px]">
          <thead>
            <tr className="border-b border-border-secondary">
              {isAdmin && (
                <th className="px-4 py-3 w-8">
                  <input
                    type="checkbox"
                    checked={selectedUsers.size === filteredUsers.length && filteredUsers.length > 0}
                    onChange={e => {
                      setSelectedUsers(e.target.checked ? new Set(filteredUsers.map((u: User) => u.id)) : new Set())
                      setSelectMode(e.target.checked)
                    }}
                    className="rounded border-border-primary bg-white/[0.04] accent-accent-blue w-3.5 h-3.5 cursor-pointer"
                  />
                </th>
              )}
              <th className="text-left px-4 py-3 text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">User</th>
              <th className="text-left px-4 py-3 text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">Role</th>
              <th className="text-left px-4 py-3 text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">Status</th>
              <th className="text-left px-4 py-3 text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">Last active</th>
              <th className="text-right px-4 py-3 text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-secondary">
            {isLoading
              ? Array.from({ length: 4 }).map((_, i) => (
                <tr key={i}>
                  {isAdmin && (
                    <td className="px-4 py-3 w-8">
                      <div className="h-3.5 w-3.5 rounded bg-white/[0.04] animate-pulse" />
                    </td>
                  )}
                  {Array.from({ length: 5 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded-[5px] bg-white/[0.04] animate-pulse" />
                    </td>
                  ))}
                </tr>
              ))
              : filteredUsers.map(user => {
                // Stable per-user accent (see hashString above) — not tied
                // to row position, so it survives filtering/sorting.
                const avatarAccent = accentFor(hashString(user.id || user.email))
                return (
                <tr
                  key={user.id}
                  className={`hover:bg-accent-blue/[0.05] transition-colors duration-150 cursor-pointer${user.disabled_at ? ' opacity-60' : ''}`}
                  onClick={() => setSelectedUser(user)}
                >
                  {isAdmin && (
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
                      <div
                        className="w-8 h-8 rounded-full border flex items-center justify-center text-[13px] font-semibold shrink-0"
                        style={{
                          backgroundColor: `color-mix(in srgb, ${avatarAccent} 15%, transparent)`,
                          borderColor: `color-mix(in srgb, ${avatarAccent} 30%, transparent)`,
                          color: avatarAccent,
                        }}
                      >
                        {user.name[0].toUpperCase()}
                      </div>
                      <div>
                        <p className="text-xs text-text-secondary font-semibold leading-tight">{user.name}</p>
                        <p className="text-text-tertiary text-xs">{user.email}</p>
                      </div>
                    </div>
                  </td>
                  <td className="px-4 py-3" onClick={e => e.stopPropagation()}>
                    {isAdmin ? (
                      <div className="space-y-1">
                        <Select
                          value={user.role}
                          onValueChange={role => updateRoleMut.mutate({ userId: user.id, role })}
                          disabled={updateRoleMut.isPending}
                        >
                          <SelectTrigger className="w-[132px]" aria-label={`Role for ${user.name}`}>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="admin">Admin</SelectItem>
                            <SelectItem value="member">Member</SelectItem>
                            <SelectItem value="viewer">Viewer</SelectItem>
                            {roles?.map(r => (
                              <SelectItem key={r.id} value={r.name}>{r.display_name}</SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                        {roleSavedFor === user.id && (
                          <p className="text-[12px] text-status-success">Saved!</p>
                        )}
                        {updateRoleMut.isError && updateRoleMut.variables?.userId === user.id && (
                          <p className="text-[12px] text-status-error/80">{(updateRoleMut.error as Error)?.message ?? 'Something went wrong'}</p>
                        )}
                      </div>
                    ) : (
                      roleBadge(user.role)
                    )}
                  </td>
                  <td className="px-4 py-3">
                    {statusBadge(user)}
                  </td>
                  <td className="px-4 py-3">{relativeTime(user.last_active)}</td>
                  <td className="px-4 py-3" onClick={e => e.stopPropagation()}>
                    <div className="flex items-center justify-end gap-1">
                      {/* Disable / Enable toggle — admin only, cannot self-disable */}
                      {isAdmin && session.user.id !== user.id && (
                        user.disabled_at ? (
                          <button
                            onClick={() => enableMut.mutate(user.id)}
                            disabled={enableMut.isPending && enableMut.variables === user.id}
                            className={`h-[26px] px-2.5 rounded-[8px] border border-border-primary text-[12px] font-semibold text-text-secondary hover:text-status-success hover:border-status-success/30 transition-colors disabled:opacity-40 ${FOCUS}`}
                          >
                            Enable
                          </button>
                        ) : (
                          <button
                            onClick={() => disableMut.mutate(user.id)}
                            disabled={disableMut.isPending && disableMut.variables === user.id}
                            className={`h-[26px] px-2.5 rounded-[8px] border border-border-primary text-[12px] font-semibold text-text-secondary hover:text-status-error hover:border-status-error/30 transition-colors disabled:opacity-40 ${FOCUS}`}
                          >
                            Disable
                          </button>
                        )
                      )}
                      {/* Rotate own key: visible to all roles. Rotate other's key: admin only */}
                      {(isAdmin || user.id === session?.user.id) && (
                        <button
                          onClick={() => setRotateTarget(user)}
                          aria-label="Rotate key"
                          title="Rotate key"
                          className={`w-[26px] h-[26px] rounded-[8px] flex items-center justify-center text-text-quaternary hover:text-text-primary hover:bg-white/[0.07] transition-colors ${FOCUS}`}
                        >
                          <RotateCw className="w-3.5 h-3.5" />
                        </button>
                      )}
                      {/* Reset key: admin-only endpoint — useful when a key is compromised */}
                      {isAdmin && (
                        <button
                          onClick={() => setResetTarget(user)}
                          aria-label="Reset key"
                          title="Reset key"
                          className={`w-[26px] h-[26px] rounded-[8px] flex items-center justify-center text-status-warning/70 hover:text-status-warning hover:bg-status-warning/10 transition-colors ${FOCUS}`}
                        >
                          <RefreshCcw className="w-3.5 h-3.5" />
                        </button>
                      )}
                      {isAdmin && (
                        <button
                          onClick={() => setRevokeTarget(user)}
                          aria-label="Revoke"
                          title="Revoke access"
                          className={`w-[26px] h-[26px] rounded-[8px] flex items-center justify-center text-status-error/70 hover:text-status-error hover:bg-status-error/10 transition-colors ${FOCUS}`}
                        >
                          <Ban className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
                )
              })
            }
          </tbody>
        </table>
        {!isLoading && userList.length === 0 && (
          <div className="flex flex-col items-center gap-2 py-16 text-center">
            <UserPlus className="w-6 h-6 text-text-quaternary/50" />
            <p className="text-[13px] font-semibold text-text-secondary">No team members yet</p>
            <p className="text-[13px] text-text-tertiary max-w-xs">Invite your first user to start collaborating on memories and projects.</p>
            {isAdmin && (
              <button
                onClick={() => setInviteOpen(true)}
                className={`mt-2 flex items-center gap-2 px-3 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-[13px] font-semibold transition-colors ${FOCUS}`}
              >
                <UserPlus className="w-3.5 h-3.5" />
                Invite user
              </button>
            )}
          </div>
        )}
        {!isLoading && userList.length > 0 && filteredUsers.length === 0 && (
          <div className="flex flex-col items-center gap-2 py-16 text-center">
            <Search className="w-6 h-6 text-text-quaternary/50" />
            <p className="text-[13px] font-semibold text-text-secondary">No users match your search</p>
            <p className="text-[13px] text-text-tertiary max-w-xs">Try a different name, email, or clear the status filter.</p>
          </div>
        )}
      </div>

      {/* Bulk action bar */}
      {selectMode && selectedUsers.size > 0 && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 flex items-center gap-3 rounded-full border border-white/[0.10] bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] px-5 py-2.5">
          <span className="text-[13px] text-text-secondary">{selectedUsers.size} selected</span>
          <div className="w-px h-4 bg-border-primary" />
          <button
            onClick={() => bulkEnableMut.mutate([...selectedUsers])}
            disabled={bulkEnableMut.isPending}
            className={`text-[13px] px-2 py-1 rounded-[8px] text-status-success hover:bg-status-success/10 transition-colors disabled:opacity-40 ${FOCUS}`}
          >
            Enable
          </button>
          <button
            onClick={() => bulkDisableMut.mutate([...selectedUsers])}
            disabled={bulkDisableMut.isPending}
            className={`text-[13px] px-2 py-1 rounded-[8px] text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-40 ${FOCUS}`}
          >
            Disable
          </button>
          <button
            onClick={() => { setSelectedUsers(new Set()); setSelectMode(false) }}
            aria-label="Clear selection"
            className={`text-text-tertiary hover:text-text-primary transition-colors ml-1 rounded-full ${FOCUS}`}
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
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] border border-status-error/30 rounded-[11px] px-4 py-2 text-[13px] text-status-error/80">
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
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] border border-status-error/30 rounded-[11px] px-4 py-2 text-[13px] text-status-error/80">
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
  return 'text-text-tertiary bg-white/[0.06] border-border-primary'
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
            <div className="h-5 w-20 rounded-[5px] bg-white/[0.04] animate-pulse shrink-0" />
            <div className="h-3.5 flex-1 rounded-[5px] bg-white/[0.04] animate-pulse" />
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
          <span className={`text-[11px] font-medium border rounded-full px-2 py-0.5 shrink-0 mt-0.5 ${actionChipCls(entry.action)}`}>
            {entry.action}
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[13px] text-text-secondary truncate">
              {entry.resource_type}{entry.resource_id ? ` · ${entry.resource_id.slice(0, 8)}` : ''}
            </p>
            <p className="text-[12px] text-text-tertiary mt-0.5">
              {toDate(entry.timestamp).toLocaleString()}
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
        className="fixed right-0 top-0 h-full w-96 bg-[#0f1117]/[0.94] backdrop-blur-[22px] border-l border-white/10 z-50 flex flex-col shadow-2xl"
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
      <div ref={modalRef} className="border border-white/10 bg-[#0f1117]/[0.94] backdrop-blur-[22px] rounded-[18px] p-6 max-w-sm w-full mx-4 space-y-4" onClick={e => e.stopPropagation()}>
        <p className="text-text-primary font-semibold">
          {userName ? `New API key for ${userName}` : 'New API key generated'}
        </p>
        <p className="text-xs text-status-warning">Copy this key now — it won't be shown again.</p>
        <div className="font-mono text-xs bg-white/[0.03] border border-white/[0.09] rounded-[11px] p-3 break-all select-all text-text-primary flex items-center gap-2">
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
          className="w-full py-2 rounded-full bg-accent-blue text-white text-xs font-semibold hover:bg-accent-blue-hover transition-colors"
        >
          Done
        </button>
      </div>
    </div>
  )
}
