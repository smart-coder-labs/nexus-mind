import { useState, useEffect, useCallback } from 'react'
import { Users as UsersIcon, RefreshCw, Search, X, AlertCircle } from 'lucide-react'
import { listOrgs, listAllUsers, suspendUser } from '../api/client'
import type { OrgWithStats, User } from '../types'
import { cn } from '@/lib/utils'

type UserWithOrg = User & { org_name: string; org_slug: string }

function RoleBadge({ role }: { role: string }) {
  const styles: Record<string, string> = {
    admin:  'bg-accent-blue-tint text-accent-blue',
    member: 'bg-surface-secondary text-text-secondary',
    viewer: 'bg-surface-secondary text-text-tertiary',
  }
  return (
    <span className={cn('inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium', styles[role] ?? 'bg-surface-secondary text-text-tertiary')}>
      {role}
    </span>
  )
}

function StatusDot({ status }: { status: User['status'] }) {
  const colors: Record<User['status'], string> = {
    active:    'bg-status-success',
    invited:   'bg-status-warning',
    suspended: 'bg-status-error',
  }
  return <span className={cn('inline-block w-1.5 h-1.5 rounded-full', colors[status])} />
}

export default function UsersPage() {
  const [users, setUsers] = useState<UserWithOrg[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [search, setSearch] = useState('')
  const [suspending, setSuspending] = useState<string | null>(null)

  const fetchAll = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const [allUsers, orgs] = await Promise.all([listAllUsers(), listOrgs()])
      const orgMap = new Map<string, OrgWithStats>(orgs.map(o => [o.id, o]))
      setUsers(allUsers.map(u => {
        const org = orgMap.get(u.org_id)
        return { ...u, org_name: org?.name ?? u.org_id, org_slug: org?.slug ?? '' }
      }))
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to load users')
    } finally {
      setLoading(false)
    }
  }, [])

  const handleSuspend = (userId: string) => {
    setSuspending(userId)
    suspendUser(userId)
      .then(() => fetchAll())
      .catch(err => setError(err instanceof Error ? err.message : 'Failed to suspend user'))
      .finally(() => setSuspending(null))
  }

  useEffect(() => { fetchAll() }, [fetchAll])

  const filtered = users.filter(u =>
    !search ||
    u.name.toLowerCase().includes(search.toLowerCase()) ||
    u.email.toLowerCase().includes(search.toLowerCase()) ||
    u.org_name.toLowerCase().includes(search.toLowerCase()),
  )

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6 animate-fade-in">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-text-primary">Users</h1>
          <p className="text-sm text-text-secondary mt-0.5">
            {loading ? 'Loading…' : `${users.length} users across all organizations`}
          </p>
        </div>
        <button
          id="users-refresh"
          onClick={fetchAll}
          disabled={loading}
          className={cn(
            'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors',
            loading && 'opacity-50 cursor-not-allowed',
          )}
        >
          <RefreshCw className={cn('w-3 h-3', loading && 'animate-spin')} />
          Refresh
        </button>
      </div>

      {error && (
        <div className="flex items-center gap-2 px-4 py-3 bg-status-error/10 border border-status-error/20 rounded-lg text-sm text-status-error">
          <AlertCircle className="w-4 h-4 flex-shrink-0" />
          {error}
        </div>
      )}

      <div className="relative">
        <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-text-quaternary" />
        <input
          type="text"
          value={search}
          onChange={e => setSearch(e.target.value)}
          placeholder="Search by name, email, or organization…"
          className="w-full bg-surface-primary border border-border-primary rounded-lg pl-10 pr-4 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
        />
        {search && (
          <button onClick={() => setSearch('')} className="absolute right-3 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-secondary">
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      <div className="bg-surface-primary border border-border-primary rounded-xl overflow-hidden">
        <div className="grid grid-cols-[1fr_160px_100px_100px_80px] gap-4 px-5 py-3 border-b border-border-secondary">
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">User</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Organization</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Role</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Status</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Actions</span>
        </div>

        {loading ? (
          <div className="divide-y divide-border-secondary">
            {[...Array(6)].map((_, i) => (
              <div key={i} className="grid grid-cols-[1fr_160px_100px_100px_80px] gap-4 px-5 py-4 items-center">
                <div className="space-y-1">
                  <div className="h-4 w-28 bg-surface-secondary animate-pulse rounded" />
                  <div className="h-3 w-40 bg-surface-secondary animate-pulse rounded" />
                </div>
                <div className="h-3 w-24 bg-surface-secondary animate-pulse rounded" />
                <div className="h-5 w-14 bg-surface-secondary animate-pulse rounded-full" />
                <div className="h-5 w-14 bg-surface-secondary animate-pulse rounded-full" />
                <div className="h-6 w-14 bg-surface-secondary animate-pulse rounded" />
              </div>
            ))}
          </div>
        ) : filtered.length === 0 ? (
          <div className="py-16 text-center">
            <UsersIcon className="w-8 h-8 text-text-quaternary mx-auto mb-3" />
            <p className="text-sm text-text-tertiary">
              {search ? 'No users match your search' : 'No users found'}
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border-secondary">
            {filtered.map(user => (
              <div key={`${user.org_id}-${user.id}`} className="grid grid-cols-[1fr_160px_100px_100px_80px] gap-4 px-5 py-3.5 items-center hover:bg-surface-secondary/40 transition-colors">
                <div className="min-w-0">
                  <p className="text-sm font-medium text-text-primary truncate">{user.name}</p>
                  <p className="text-xs text-text-tertiary truncate">{user.email}</p>
                </div>
                <div className="min-w-0">
                  <p className="text-xs font-medium text-text-secondary truncate">{user.org_name}</p>
                  <p className="text-[11px] font-mono text-text-quaternary truncate">{user.org_slug}</p>
                </div>
                <RoleBadge role={user.role} />
                <div className="flex items-center gap-1.5">
                  <StatusDot status={user.status} />
                  <span className="text-xs text-text-secondary">{user.status}</span>
                </div>
                <div>
                  {(user.status === 'active' || user.status === 'invited') && (
                    <button
                      onClick={() => handleSuspend(user.id)}
                      disabled={suspending === user.id}
                      className={cn(
                        'text-[11px] font-medium px-2 py-1 rounded border transition-colors',
                        'bg-status-error/10 text-status-error border-status-error/20 hover:bg-status-error/20',
                        suspending === user.id && 'opacity-50 cursor-not-allowed',
                      )}
                    >
                      {suspending === user.id ? '…' : 'Suspend'}
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
