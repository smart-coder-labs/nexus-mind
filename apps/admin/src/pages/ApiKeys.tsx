import { useMemo, useState, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { formatDistanceToNow, differenceInDays, isPast } from 'date-fns'
import { Trash2, Plus, Copy, Check, X } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { ApiKeyWithUser } from '../types'

function RelativeTime({ iso }: { iso: string | null }) {
  if (!iso)
    return <span className="text-xs text-text-quaternary italic">Never</span>

  const days = differenceInDays(new Date(), new Date(iso))
  const colorClass =
    days < 7 ? 'text-status-success' :
    days < 30 ? 'text-text-secondary' :
    'text-text-quaternary'

  return (
    <span className={`text-xs ${colorClass}`} title={iso}>
      {formatDistanceToNow(new Date(iso), { addSuffix: true })}
    </span>
  )
}

function ExpiryCell({ expiresAt }: { expiresAt: string | null }) {
  if (!expiresAt) {
    return <span className="text-[10px] text-text-quaternary">Never</span>
  }
  const expired = isPast(new Date(expiresAt))
  if (expired) {
    return (
      <span className="bg-status-error/10 text-status-error rounded-[5px] text-[10px] px-1.5 py-0.5">
        Expired
      </span>
    )
  }
  const daysLeft = differenceInDays(new Date(expiresAt), new Date())
  if (daysLeft < 7) {
    return (
      <span className="bg-status-warning/10 text-status-warning rounded-[5px] text-[10px] px-1.5 py-0.5" title={expiresAt}>
        {formatDistanceToNow(new Date(expiresAt), { addSuffix: true })}
      </span>
    )
  }
  return (
    <span className="bg-status-success/10 text-status-success rounded-[5px] text-[10px] px-1.5 py-0.5" title={expiresAt}>
      {formatDistanceToNow(new Date(expiresAt), { addSuffix: true })}
    </span>
  )
}

function SkeletonRow() {
  return (
    <tr className="border-b border-border-primary">
      {/* User cell */}
      <td className="px-4 py-3">
        <div className="flex items-center gap-3">
          <div className="animate-pulse w-7 h-7 rounded-full bg-[#272729] shrink-0" />
          <div className="space-y-1.5">
            <div className="animate-pulse h-3.5 bg-[#272729] rounded-[8px] w-24" />
            <div className="animate-pulse h-3 bg-[#272729] rounded-[8px] w-32" />
          </div>
        </div>
      </td>
      {/* Label */}
      <td className="px-4 py-3">
        <div className="animate-pulse h-3.5 bg-[#272729] rounded-[8px] w-28" />
      </td>
      {/* Last used */}
      <td className="px-4 py-3">
        <div className="animate-pulse h-3.5 bg-[#272729] rounded-[8px] w-20" />
      </td>
      {/* Created */}
      <td className="px-4 py-3">
        <div className="animate-pulse h-3.5 bg-[#272729] rounded-[8px] w-20" />
      </td>
      {/* Expires */}
      <td className="px-4 py-3">
        <div className="animate-pulse h-3.5 bg-[#272729] rounded-[8px] w-16" />
      </td>
      {/* Action */}
      <td className="px-4 py-3">
        <div className="animate-pulse h-6 bg-[#272729] rounded-[8px] w-14 ml-auto" />
      </td>
    </tr>
  )
}

function KeyIcon() {
  return (
    <svg
      className="w-10 h-10 text-text-quaternary"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={1.5}
      aria-hidden="true"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"
      />
    </svg>
  )
}

interface CreateKeyForm {
  name: string
  expires_at: string
  role: string
  description: string
}

interface CreatedKey {
  id: string
  name: string
  key: string
}

function CreateKeyModal({
  onClose,
  onCreated,
}: {
  onClose: () => void
  onCreated: (key: CreatedKey) => void
}) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [form, setForm] = useState<CreateKeyForm>({
    name: '',
    expires_at: '',
    role: 'member',
    description: '',
  })
  const [error, setError] = useState<string | null>(null)

  const createMut = useMutation({
    mutationFn: () =>
      client.createOrgKey({
        name: form.name.trim(),
        expires_at: form.expires_at || undefined,
        role: form.role || undefined,
        description: form.description.trim() || undefined,
      }),
    onSuccess: (res) => {
      onCreated({ id: String(res.id), name: res.name, key: res.key })
    },
    onError: (err) => {
      setError(err instanceof Error ? err.message : 'Failed to create key')
    },
  })

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!form.name.trim()) {
      setError('Key name is required')
      return
    }
    setError(null)
    createMut.mutate()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-[18px] border border-border-primary bg-[#1d1d1f] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-primary">
          <h2 className="text-[15px] font-semibold text-text-primary">New API Key</h2>
          <button
            onClick={onClose}
            className="rounded-full p-1 text-text-quaternary hover:text-text-secondary hover:bg-white/[0.06] transition-colors"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="px-5 py-4 space-y-4">
          {error && (
            <div className="rounded-[10px] border border-status-error/20 bg-status-error/5 px-3 py-2 text-xs text-status-error">
              {error}
            </div>
          )}

          {/* Name */}
          <div className="space-y-1.5">
            <label className="block text-[10px] text-text-quaternary mb-1">
              Key Name <span className="text-status-error">*</span>
            </label>
            <input
              type="text"
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="e.g. CI/CD pipeline"
              className="rounded-[8px] bg-white/[0.04] border border-border-primary text-xs text-text-secondary px-3 py-2 focus:outline-none focus:border-accent-blue/60 w-full placeholder:text-text-quaternary"
              required
            />
          </div>

          {/* Role */}
          <div className="space-y-1.5">
            <label className="block text-[10px] text-text-quaternary mb-1">
              Role
            </label>
            <select
              value={form.role}
              onChange={(e) => setForm((f) => ({ ...f, role: e.target.value }))}
              className="rounded-[8px] bg-white/[0.04] border border-border-primary text-xs text-text-secondary px-3 py-2 focus:outline-none focus:border-accent-blue/60 w-full"
            >
              <option value="admin">admin</option>
              <option value="member">member</option>
              <option value="viewer">viewer</option>
            </select>
          </div>

          {/* Expiry */}
          <div className="space-y-1.5">
            <label className="block text-[10px] text-text-quaternary mb-1">
              Expiry Date <span className="text-text-quaternary">(optional)</span>
            </label>
            <input
              type="date"
              value={form.expires_at}
              onChange={(e) => setForm((f) => ({ ...f, expires_at: e.target.value }))}
              min={new Date().toISOString().slice(0, 10)}
              className="rounded-[8px] bg-white/[0.04] border border-border-primary text-xs text-text-secondary px-3 py-2 focus:outline-none focus:border-accent-blue/60 w-full"
            />
          </div>

          {/* Description */}
          <div className="space-y-1.5">
            <label className="block text-[10px] text-text-quaternary mb-1">
              Description <span className="text-text-quaternary">(optional)</span>
            </label>
            <textarea
              value={form.description}
              onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
              placeholder="What is this key for?"
              rows={2}
              className="rounded-[8px] bg-white/[0.04] border border-border-primary text-xs text-text-secondary px-3 py-2 focus:outline-none focus:border-accent-blue/60 w-full placeholder:text-text-quaternary resize-none"
            />
          </div>

          {/* Actions */}
          <div className="flex gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="flex-1 rounded-full border border-border-primary px-4 py-1.5 text-xs text-text-secondary hover:bg-white/[0.04] transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={createMut.isPending}
              className="flex-1 rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:opacity-90 transition-opacity disabled:opacity-40"
            >
              {createMut.isPending ? 'Creating…' : 'Create Key'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

function CreatedKeyModal({
  created,
  onClose,
}: {
  created: CreatedKey
  onClose: () => void
}) {
  const [copied, setCopied] = useState(false)
  const codeRef = useRef<HTMLElement>(null)

  const handleCopy = () => {
    navigator.clipboard.writeText(created.key).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-[18px] border border-border-primary bg-[#1d1d1f] shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-primary">
          <h2 className="text-[15px] font-semibold text-text-primary">Key Created</h2>
          <button
            onClick={onClose}
            className="rounded-full p-1 text-text-quaternary hover:text-text-secondary hover:bg-white/[0.06] transition-colors"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="px-5 py-4 space-y-4">
          {/* Warning */}
          <div className="rounded-[11px] bg-status-warning/10 border border-status-warning/30 p-3 text-xs text-status-warning leading-relaxed">
            Save this key now — it won't be shown again.
          </div>

          <div className="space-y-1.5">
            <p className="text-[10px] text-text-quaternary mb-1">
              {created.name}
            </p>
            <div className="relative flex items-center gap-2">
              <code
                ref={codeRef}
                className="flex-1 font-mono text-xs bg-white/[0.06] rounded-[8px] px-3 py-2 break-all text-text-primary select-all"
              >
                {created.key}
              </code>
              <button
                onClick={handleCopy}
                className="shrink-0 p-1.5 transition-colors"
                aria-label="Copy key"
              >
                {copied ? <Check className="w-3.5 h-3.5 text-status-success" /> : <Copy className="w-3.5 h-3.5 text-text-quaternary hover:text-text-primary" />}
              </button>
            </div>
          </div>

          <button
            onClick={onClose}
            className="w-full rounded-full bg-accent-blue px-4 py-1.5 text-xs font-semibold text-white hover:opacity-90 transition-opacity"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  )
}

export default function ApiKeys() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [showCreateModal, setShowCreateModal] = useState(false)
  const [createdKey, setCreatedKey] = useState<CreatedKey | null>(null)

  const { data: keys, isLoading } = useQuery<ApiKeyWithUser[]>({
    queryKey: ['org-keys'],
    queryFn: () => client.listOrgKeys(),
  })

  const revokeMut = useMutation({
    mutationFn: (keyId: string) => client.revokeOrgKey(keyId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['org-keys'] }),
  })

  const bulkRevokeExpiredMut = useMutation({
    mutationFn: (ids: string[]) =>
      Promise.all(ids.map((id) => client.revokeOrgKey(id))),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['org-keys'] }),
  })

  const revokeError = revokeMut.isError
    ? (revokeMut.error instanceof Error ? revokeMut.error.message : 'Failed to revoke key')
    : null

  const bulkError = bulkRevokeExpiredMut.isError
    ? (bulkRevokeExpiredMut.error instanceof Error ? bulkRevokeExpiredMut.error.message : 'Failed to revoke expired keys')
    : null

  const handleRevoke = (key: ApiKeyWithUser) => {
    if (!window.confirm(`Revoke API key "${key.label}" for ${key.user_name}? This cannot be undone.`)) return
    revokeMut.mutate(key.id)
  }

  const expiredKeys = (keys ?? []).filter((k: any) => k.expires_at && isPast(new Date(k.expires_at)))

  const handleKeyCreated = (key: CreatedKey) => {
    setShowCreateModal(false)
    setCreatedKey(key)
    qc.invalidateQueries({ queryKey: ['org-keys'] })
  }

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-8">
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-[21px] font-semibold tracking-[0.231px] text-text-primary">API Keys</h1>
          <p className="mt-1 text-[14px] text-text-tertiary tracking-[-0.224px]">
            All active API keys in this organization.
          </p>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {expiredKeys.length > 0 && (
            <button
              onClick={() => bulkRevokeExpiredMut.mutate(expiredKeys.map((k: any) => k.id))}
              disabled={bulkRevokeExpiredMut.isPending}
              className="border border-status-error/40 text-status-error rounded-full px-3 py-1.5 text-xs hover:bg-status-error/10 transition-colors disabled:opacity-40 flex items-center gap-1.5"
            >
              <Trash2 className="w-3 h-3" />
              {bulkRevokeExpiredMut.isPending ? 'Revoking…' : `Revoke ${expiredKeys.length} expired`}
            </button>
          )}

          <button
            onClick={() => setShowCreateModal(true)}
            className="flex items-center gap-1.5 rounded-full bg-accent-blue px-3 py-1.5 text-xs font-semibold text-white hover:opacity-90 transition-opacity"
          >
            <Plus className="w-3.5 h-3.5" />
            New Key
          </button>
        </div>
      </div>

      {/* Error notifications */}
      {revokeError && (
        <div className="rounded-[11px] border border-status-error/20 bg-status-error/5 px-4 py-3 text-sm text-status-error">
          {revokeError}
        </div>
      )}
      {bulkError && (
        <div className="rounded-[11px] border border-status-error/20 bg-status-error/5 px-4 py-3 text-sm text-status-error">
          {bulkError}
        </div>
      )}

      {/* Table */}
      <div className="rounded-[18px] border border-border-primary overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border-primary bg-[#272729]/40">
                <th className="px-4 py-3 text-left text-[10px] font-semibold text-text-quaternary uppercase tracking-wider">User</th>
                <th className="px-4 py-3 text-left text-[10px] font-semibold text-text-quaternary uppercase tracking-wider">Label</th>
                <th className="px-4 py-3 text-left text-[10px] font-semibold text-text-quaternary uppercase tracking-wider">Last used</th>
                <th className="px-4 py-3 text-left text-[10px] font-semibold text-text-quaternary uppercase tracking-wider">Created</th>
                <th className="px-4 py-3 text-left text-[10px] font-semibold text-text-quaternary uppercase tracking-wider">Expires</th>
                <th className="px-4 py-3 text-right text-[10px] font-semibold text-text-quaternary uppercase tracking-wider">Action</th>
              </tr>
            </thead>
            <tbody>
              {isLoading && [1, 2, 3, 4, 5].map((i) => <SkeletonRow key={i} />)}

              {!isLoading && keys?.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-4 py-16 text-center">
                    <div className="flex flex-col items-center gap-3">
                      <KeyIcon />
                      <p className="text-sm font-semibold text-text-tertiary">No active API keys</p>
                      <p className="text-xs text-text-quaternary">
                        API keys created by organization members will appear here.
                      </p>
                    </div>
                  </td>
                </tr>
              )}

              {keys?.map((key) => (
                <tr
                  key={key.id}
                  className="border-b border-border-primary last:border-0 hover:bg-white/[0.04] transition-colors"
                >
                  {/* User cell */}
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-3">
                      <div className="w-7 h-7 rounded-full bg-accent-blue/15 border border-accent-blue/20 text-accent-blue text-xs font-semibold flex items-center justify-center shrink-0">
                        {key.user_name?.charAt(0).toUpperCase() ?? '?'}
                      </div>
                      <div>
                        <div className="text-sm text-text-primary font-semibold">{key.user_name}</div>
                        <div className="text-xs text-text-tertiary mt-0.5">{key.user_email}</div>
                      </div>
                    </div>
                  </td>

                  {/* Label cell */}
                  <td className="px-4 py-3 text-sm text-text-secondary">
                    {key.label}
                  </td>

                  {/* Last used cell */}
                  <td className="px-4 py-3">
                    <div className="space-y-0.5">
                      <RelativeTime iso={key.last_used} />
                      <div className="text-xs text-text-quaternary">
                        {(key.times_used ?? 0)} {(key.times_used ?? 0) === 1 ? 'use' : 'uses'}
                      </div>
                    </div>
                  </td>

                  {/* Created cell */}
                  <td className="px-4 py-3 text-text-tertiary text-xs">
                    {new Date(key.created_at).toLocaleDateString()}
                  </td>

                  {/* Expires cell */}
                  <td className="px-4 py-3">
                    <ExpiryCell expiresAt={key.expires_at} />
                  </td>

                  {/* Action cell */}
                  <td className="px-4 py-3 text-right">
                    <button
                      onClick={() => handleRevoke(key)}
                      disabled={revokeMut.isPending}
                      className="text-xs border border-status-error/30 rounded-[8px] px-3 py-1 text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-50"
                      aria-label={`Revoke key for ${key.user_name}`}
                    >
                      Revoke
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {/* Modals */}
      {showCreateModal && (
        <CreateKeyModal
          onClose={() => setShowCreateModal(false)}
          onCreated={handleKeyCreated}
        />
      )}
      {createdKey && (
        <CreatedKeyModal
          created={createdKey}
          onClose={() => setCreatedKey(null)}
        />
      )}
    </div>
  )
}
