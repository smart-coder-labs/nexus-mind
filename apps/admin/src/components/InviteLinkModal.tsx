import { useState, useEffect } from 'react'
import { X, Link } from 'lucide-react'
import type { NexusMindClient } from '../api/client'
import type { InviteLinkResponse } from '../types'

interface Props {
  open: boolean
  client: NexusMindClient
  onClose: () => void
}

export function InviteLinkModal({ open, client, onClose }: Props) {
  const [role, setRole] = useState('user')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [invite, setInvite] = useState<InviteLinkResponse | null>(null)
  const [copied, setCopied] = useState(false)

  // Reset state when modal is closed
  useEffect(() => {
    if (!open) {
      setRole('user')
      setLoading(false)
      setError('')
      setInvite(null)
      setCopied(false)
    }
  }, [open])

  // Trap escape key
  useEffect(() => {
    if (!open) return
    const handle = (e: KeyboardEvent) => { if (e.key === 'Escape') handleClose() }
    document.addEventListener('keydown', handle)
    return () => document.removeEventListener('keydown', handle)
  }, [open])

  if (!open) return null

  const handleGenerate = async () => {
    setLoading(true)
    setError('')
    try {
      const res = await client.createInviteLink(role)
      setInvite(res)
    } catch {
      setError('Failed to generate invite link. Try again.')
    } finally {
      setLoading(false)
    }
  }

  const handleCopy = () => {
    if (!invite) return
    const fullUrl = `${window.location.origin}${invite.invite_url}`
    navigator.clipboard.writeText(fullUrl)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const handleClose = () => {
    onClose()
  }

  return (
    <div
      className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center backdrop-blur-sm"
      onClick={handleClose}
    >
      <div
        className="bg-[#272729] border border-border-primary rounded-[18px] p-6 max-w-sm w-full mx-4 space-y-4"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <p className="text-text-primary font-semibold">Invite team member</p>
          <button
            onClick={handleClose}
            className="text-text-tertiary hover:text-text-primary transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="space-y-1.5">
          <label className="text-[11px] text-text-tertiary tracking-[-0.224px]">Role</label>
          <select
            value={role}
            onChange={e => setRole(e.target.value)}
            disabled={!!invite}
            className="w-full bg-white/[0.04] border border-border-primary rounded-[11px] px-3 py-2 text-sm text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors disabled:opacity-50"
          >
            <option value="user">User</option>
            <option value="member">Member</option>
            <option value="admin">Admin</option>
          </select>
        </div>

        {!invite ? (
          <>
            {error && (
              <p className="text-xs text-status-error/80">{error}</p>
            )}
            <button
              onClick={handleGenerate}
              disabled={loading}
              className="w-full py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold disabled:opacity-40 transition-colors flex items-center justify-center gap-2"
            >
              <Link className="w-4 h-4" />
              {loading ? 'Generating…' : 'Generate invite link'}
            </button>
          </>
        ) : (
          <div className="space-y-3">
            <div className="font-mono text-xs bg-[#1d1d1f] rounded-[11px] p-3 break-all border border-border-secondary text-text-primary select-all">
              {`${window.location.origin}${invite.invite_url}`}
            </div>
            <button
              onClick={handleCopy}
              className="w-full py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold transition-colors"
            >
              {copied ? 'Copied!' : 'Copy link'}
            </button>
            <p className="text-xs text-text-tertiary text-center">
              Expires in 7 days &middot; one-time use
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
