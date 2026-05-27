import { useState, useEffect, useCallback, useRef } from 'react'
import { useParams, useNavigate, Link } from 'react-router-dom'
import { ArrowLeft, Building2, Users, AlertCircle, RefreshCw, Copy, Check, Trash2, KeyRound } from 'lucide-react'
import { getOrg, listOrgUsers, suspendUser, impersonateOrg, deleteOrg } from '../api/client'
import type { OrgWithStats, User } from '../types'
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
    active:    'bg-status-success/15 text-status-success',
    invited:   'bg-status-warning/15 text-status-warning',
    suspended: 'bg-status-error/15 text-status-error',
  }
  return (
    <span className={cn('inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium', styles[status])}>
      {status}
    </span>
  )
}

function ImpersonateModal({ token, onClose }: { token: string; onClose: () => void }) {
  const [copied, setCopied] = useState(false)

  const copy = () => {
    navigator.clipboard.writeText(token).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm">
      <div className="bg-bg-primary border border-border-primary rounded-xl w-full max-w-md p-6 space-y-4">
        <div className="flex items-center gap-2">
          <KeyRound className="w-4 h-4 text-accent-blue" />
          <h2 className="text-sm font-semibold text-text-primary">Impersonation Token</h2>
        </div>

        <div className="relative">
          <div className="bg-surface-secondary border border-border-primary rounded-lg px-3 py-3 pr-10 font-mono text-xs text-text-secondary break-all">
            {token}
          </div>
          <button
            onClick={copy}
            className="absolute right-2.5 top-2.5 text-text-quaternary hover:text-text-secondary transition-colors"
            aria-label="Copy token"
          >
            {copied ? <Check className="w-3.5 h-3.5 text-status-success" /> : <Copy className="w-3.5 h-3.5" />}
          </button>
        </div>

        <button
          onClick={copy}
          className="w-full flex items-center justify-center gap-2 bg-white text-black text-xs font-medium px-4 py-2 rounded-lg hover:bg-white/90 transition-colors"
        >
          {copied ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
          {copied ? 'Copied!' : 'Copy token'}
        </button>

        <p className="text-[11px] text-text-quaternary leading-relaxed">
          Use this token to log in to the admin panel with API key auth. Token expires when the session is revoked.
        </p>

        <button
          onClick={onClose}
          className="w-full text-xs text-text-tertiary hover:text-text-secondary transition-colors py-1"
        >
          Close
        </button>
      </div>
    </div>
  )
}

function DeleteOrgModal({ slug, onConfirm, onClose, loading }: {
  slug: string
  onConfirm: () => void
  onClose: () => void
  loading: boolean
}) {
  const [input, setInput] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm">
      <div className="bg-bg-primary border border-border-primary rounded-xl w-full max-w-md p-6 space-y-4">
        <div className="flex items-center gap-2">
          <Trash2 className="w-4 h-4 text-status-error" />
          <h2 className="text-sm font-semibold text-text-primary">Delete Organization</h2>
        </div>

        <p className="text-xs text-text-secondary leading-relaxed">
          This action is irreversible. All users and memories will be permanently deleted.
          Type <span className="font-mono text-text-primary">{slug}</span> to confirm.
        </p>

        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={e => setInput(e.target.value)}
          placeholder={slug}
          className="w-full bg-surface-secondary border border-border-primary rounded-lg px-3 py-2 text-sm text-text-primary placeholder:text-text-quaternary font-mono focus:outline-none focus:border-status-error/40 focus:ring-2 focus:ring-status-error/10 transition-colors"
        />

        <div className="flex gap-2">
          <button
            onClick={onClose}
            disabled={loading}
            className="flex-1 text-xs text-text-tertiary hover:text-text-secondary bg-surface-secondary border border-border-primary rounded-lg px-4 py-2 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={input !== slug || loading}
            className={cn(
              'flex-1 text-xs font-medium rounded-lg px-4 py-2 transition-colors',
              input === slug && !loading
                ? 'bg-status-error text-white hover:bg-status-error/90'
                : 'bg-status-error/20 text-status-error/40 cursor-not-allowed',
            )}
          >
            {loading ? 'Deleting…' : 'Delete org'}
          </button>
        </div>
      </div>
    </div>
  )
}

export default function OrgDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()

  const [org, setOrg] = useState<OrgWithStats | null>(null)
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')

  const [suspending, setSuspending] = useState<string | null>(null)
  const [impersonateToken, setImpersonateToken] = useState<string | null>(null)
  const [impersonating, setImpersonating] = useState(false)
  const [showDeleteModal, setShowDeleteModal] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const fetchAll = useCallback(() => {
    if (!id) return
    setLoading(true)
    setError('')
    Promise.all([getOrg(id), listOrgUsers(id)])
      .then(([o, u]) => {
        setOrg(o)
        setUsers(u)
      })
      .catch(err => setError(err.message ?? 'Failed to load organization'))
      .finally(() => setLoading(false))
  }, [id])

  useEffect(() => { fetchAll() }, [fetchAll])

  const handleSuspend = (userId: string) => {
    setSuspending(userId)
    suspendUser(userId)
      .then(() => fetchAll())
      .catch(err => setError(err.message ?? 'Failed to suspend user'))
      .finally(() => setSuspending(null))
  }

  const handleImpersonate = () => {
    if (!id) return
    setImpersonating(true)
    impersonateOrg(id)
      .then(({ token }) => setImpersonateToken(token))
      .catch(err => setError(err.message ?? 'Failed to impersonate org'))
      .finally(() => setImpersonating(false))
  }

  const handleDelete = () => {
    if (!id) return
    setDeleting(true)
    deleteOrg(id)
      .then(() => navigate('/orgs'))
      .catch(err => {
        setError(err.message ?? 'Failed to delete organization')
        setDeleting(false)
        setShowDeleteModal(false)
      })
  }

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6 animate-fade-in">
      {/* Back link */}
      <div className="flex items-center gap-3">
        <Link
          to="/orgs"
          className="flex items-center gap-1.5 text-xs text-text-tertiary hover:text-text-secondary transition-colors"
        >
          <ArrowLeft className="w-3.5 h-3.5" />
          Organizations
        </Link>
      </div>

      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-10 h-10 rounded-xl bg-accent-blue-tint flex items-center justify-center flex-shrink-0">
            <Building2 className="w-5 h-5 text-accent-blue" />
          </div>
          <div className="min-w-0">
            {loading || !org ? (
              <>
                <div className="h-5 w-40 bg-surface-secondary animate-pulse rounded mb-1" />
                <div className="h-3 w-24 bg-surface-secondary animate-pulse rounded" />
              </>
            ) : (
              <>
                <h1 className="text-xl font-semibold text-text-primary truncate">{org.name}</h1>
                <div className="flex items-center gap-3 mt-0.5">
                  <p className="text-xs font-mono text-text-tertiary">{org.slug}</p>
                  <span className="text-text-quaternary text-xs">·</span>
                  <p className="text-xs text-text-tertiary">{org.user_count} users</p>
                  <span className="text-text-quaternary text-xs">·</span>
                  <p className="text-xs text-text-tertiary">{org.memory_count} memories</p>
                </div>
              </>
            )}
          </div>
        </div>

        <div className="flex items-center gap-2 flex-shrink-0">
          <button
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

          <button
            onClick={handleImpersonate}
            disabled={impersonating || loading}
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border transition-colors',
              'bg-accent-blue-tint text-accent-blue border-accent-blue/20 hover:bg-accent-blue/20',
              (impersonating || loading) && 'opacity-50 cursor-not-allowed',
            )}
          >
            <KeyRound className="w-3 h-3" />
            {impersonating ? 'Getting token…' : 'Impersonate'}
          </button>

          <button
            onClick={() => setShowDeleteModal(true)}
            disabled={loading || deleting}
            className={cn(
              'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border transition-colors',
              'bg-status-error/10 text-status-error border-status-error/20 hover:bg-status-error/20',
              (loading || deleting) && 'opacity-50 cursor-not-allowed',
            )}
          >
            <Trash2 className="w-3 h-3" />
            Delete org
          </button>
        </div>
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
        <div className="grid grid-cols-[1fr_120px_120px_140px_100px] gap-4 px-5 py-3 border-b border-border-secondary">
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">User</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Role</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Status</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Joined</span>
          <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Actions</span>
        </div>

        {loading ? (
          <div className="divide-y divide-border-secondary">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="grid grid-cols-[1fr_120px_120px_140px_100px] gap-4 px-5 py-4 items-center">
                <div className="space-y-1">
                  <div className="h-4 w-32 bg-surface-secondary animate-pulse rounded" />
                  <div className="h-3 w-40 bg-surface-secondary animate-pulse rounded" />
                </div>
                <div className="h-5 w-14 bg-surface-secondary animate-pulse rounded-full" />
                <div className="h-5 w-14 bg-surface-secondary animate-pulse rounded-full" />
                <div className="h-3 w-20 bg-surface-secondary animate-pulse rounded" />
                <div className="h-6 w-16 bg-surface-secondary animate-pulse rounded" />
              </div>
            ))}
          </div>
        ) : users.length === 0 ? (
          <div className="py-12 text-center text-sm text-text-tertiary">No users in this organization</div>
        ) : (
          <div className="divide-y divide-border-secondary">
            {users.map(user => (
              <div key={user.id} className="grid grid-cols-[1fr_120px_120px_140px_100px] gap-4 px-5 py-3.5 items-center">
                <div className="min-w-0">
                  <p className="text-sm font-medium text-text-primary truncate">{user.name}</p>
                  <p className="text-xs text-text-tertiary truncate">{user.email}</p>
                </div>
                <RoleBadge role={user.role} />
                <StatusBadge status={user.status} />
                <span className="text-xs text-text-tertiary">
                  {new Date(user.created_at).toLocaleDateString()}
                </span>
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

      {/* Impersonate modal */}
      {impersonateToken && (
        <ImpersonateModal
          token={impersonateToken}
          onClose={() => setImpersonateToken(null)}
        />
      )}

      {/* Delete modal */}
      {showDeleteModal && org && (
        <DeleteOrgModal
          slug={org.slug}
          onConfirm={handleDelete}
          onClose={() => setShowDeleteModal(false)}
          loading={deleting}
        />
      )}
    </div>
  )
}
