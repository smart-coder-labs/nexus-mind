import { useState, useEffect } from 'react'
import { useParams, Link } from 'react-router-dom'
import { ArrowLeft, Building2, Users, AlertCircle, RefreshCw } from 'lucide-react'
import { listOrgUsers } from '../api/client'
import type { User } from '../types'
import { cn } from '@/lib/utils'

function RoleBadge({ role }: { role: string }) {
  const styles: Record<string, string> = {
    admin:   'bg-accent-blue-tint text-accent-blue',
    member:  'bg-surface-secondary text-text-secondary',
    viewer:  'bg-surface-secondary text-text-tertiary',
  }
  return (
    <span className={cn('inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium', styles[role] ?? 'bg-surface-secondary text-text-tertiary')}>
      {role}
    </span>
  )
}

function StatusBadge({ status }: { status: User['status'] }) {
  const styles: Record<User['status'], string> = {
    active:   'bg-status-success/15 text-status-success',
    invited:  'bg-status-warning/15 text-status-warning',
    suspended:'bg-status-error/15 text-status-error',
  }
  return (
    <span className={cn('inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium', styles[status])}>
      {status}
    </span>
  )
}

export default function OrgDetail() {
  const { id } = useParams<{ id: string }>()
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')

  const fetchUsers = () => {
    if (!id) return
    setLoading(true)
    setError('')
    listOrgUsers(id)
      .then(setUsers)
      .catch(err => setError(err.message ?? 'Failed to load users'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { fetchUsers() }, [id])

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6 animate-fade-in">
      {/* Header */}
      <div className="flex items-center gap-3">
        <Link
          to="/orgs"
          className="flex items-center gap-1.5 text-xs text-text-tertiary hover:text-text-secondary transition-colors"
        >
          <ArrowLeft className="w-3.5 h-3.5" />
          Organizations
        </Link>
      </div>

      <div className="flex items-start justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-accent-blue-tint flex items-center justify-center">
            <Building2 className="w-5 h-5 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-xl font-semibold text-text-primary">Organization</h1>
            <p className="text-xs font-mono text-text-tertiary mt-0.5">{id}</p>
          </div>
        </div>
        <button
          id="org-detail-refresh"
          onClick={fetchUsers}
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

      {/* Users table */}
      <div className="bg-surface-primary border border-border-primary rounded-xl overflow-hidden">
        <div className="flex items-center gap-2 px-5 py-4 border-b border-border-secondary">
          <Users className="w-4 h-4 text-text-tertiary" />
          <h2 className="text-sm font-medium text-text-primary">
            Members
            {!loading && <span className="ml-2 text-text-tertiary font-normal">({users.length})</span>}
          </h2>
        </div>

        {/* Table header */}
        <div className="grid grid-cols-[1fr_120px_120px_140px] gap-4 px-5 py-3 border-b border-border-secondary">
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">User</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Role</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Status</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Joined</span>
        </div>

        {loading ? (
          <div className="divide-y divide-border-secondary">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="grid grid-cols-[1fr_120px_120px_140px] gap-4 px-5 py-4 items-center">
                <div className="space-y-1">
                  <div className="h-4 w-32 bg-surface-secondary animate-pulse rounded" />
                  <div className="h-3 w-40 bg-surface-secondary animate-pulse rounded" />
                </div>
                <div className="h-5 w-14 bg-surface-secondary animate-pulse rounded-full" />
                <div className="h-5 w-14 bg-surface-secondary animate-pulse rounded-full" />
                <div className="h-3 w-20 bg-surface-secondary animate-pulse rounded" />
              </div>
            ))}
          </div>
        ) : users.length === 0 ? (
          <div className="py-12 text-center text-sm text-text-tertiary">No users in this organization</div>
        ) : (
          <div className="divide-y divide-border-secondary">
            {users.map(user => (
              <div key={user.id} className="grid grid-cols-[1fr_120px_120px_140px] gap-4 px-5 py-3.5 items-center">
                <div className="min-w-0">
                  <p className="text-sm font-medium text-text-primary truncate">{user.name}</p>
                  <p className="text-xs text-text-tertiary truncate">{user.email}</p>
                </div>
                <RoleBadge role={user.role} />
                <StatusBadge status={user.status} />
                <span className="text-xs text-text-tertiary">
                  {new Date(user.created_at).toLocaleDateString()}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
