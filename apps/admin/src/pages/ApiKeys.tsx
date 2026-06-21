import { useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { formatDistanceToNow, differenceInDays, isPast } from 'date-fns'
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
    return <span className="text-[11px] text-text-quaternary">Never</span>
  }
  const expired = isPast(new Date(expiresAt))
  if (expired) {
    return <span className="text-[11px] text-status-error">Expired</span>
  }
  return (
    <span className="text-[11px] text-text-secondary" title={expiresAt}>
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

export default function ApiKeys() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const { data: keys, isLoading } = useQuery<ApiKeyWithUser[]>({
    queryKey: ['org-keys'],
    queryFn: () => client.listOrgKeys(),
  })

  const revokeMut = useMutation({
    mutationFn: (keyId: string) => client.revokeOrgKey(keyId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['org-keys'] }),
  })

  const revokeError = revokeMut.isError
    ? (revokeMut.error instanceof Error ? revokeMut.error.message : 'Failed to revoke key')
    : null

  const handleRevoke = (key: ApiKeyWithUser) => {
    if (!window.confirm(`Revoke API key "${key.label}" for ${key.user_name}? This cannot be undone.`)) return
    revokeMut.mutate(key.id)
  }

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-8">
      {/* Header */}
      <div>
        <h1 className="text-[21px] font-semibold tracking-[0.231px] text-text-primary">API Keys</h1>
        <p className="mt-1 text-[14px] text-text-tertiary tracking-[-0.224px]">
          All active API keys in this organization.
        </p>
      </div>

      {/* Revoke error notification */}
      {revokeError && (
        <div className="rounded-[11px] border border-status-error/20 bg-status-error/5 px-4 py-3 text-sm text-status-error">
          {revokeError}
        </div>
      )}

      {/* Table */}
      <div className="rounded-[18px] border border-border-primary overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border-primary bg-[#272729]/40">
                <th className="px-4 py-3 text-left text-[11px] font-semibold text-text-quaternary">User</th>
                <th className="px-4 py-3 text-left text-[11px] font-semibold text-text-quaternary">Label</th>
                <th className="px-4 py-3 text-left text-[11px] font-semibold text-text-quaternary">Last used</th>
                <th className="px-4 py-3 text-left text-[11px] font-semibold text-text-quaternary">Created</th>
                <th className="px-4 py-3 text-left text-[11px] font-semibold text-text-quaternary uppercase tracking-wide">Expires</th>
                <th className="px-4 py-3 text-right text-[11px] font-semibold text-text-quaternary">Action</th>
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
                  className="border-b border-border-primary last:border-0 hover:bg-white/[0.02] transition-colors"
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
    </div>
  )
}
