import { StyledSelect } from '@/components/ui/Select/StyledSelect'
import { useState, useEffect } from 'react'
import { Link } from 'lucide-react'
import { Modal, ModalCloseButton, ModalTitle } from './ui/Modal'
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
    <Modal open={open} onOpenChange={next => { if (!next) handleClose() }} size="sm">
      <ModalCloseButton />
      <div className="space-y-4">
        <ModalTitle className="pr-6 text-text-primary">Invite team member</ModalTitle>

        <div className="space-y-1.5">
          <label htmlFor="invite-link-role" className="text-xs font-medium text-text-secondary">Role</label>
          <StyledSelect
            id="invite-link-role"
            value={role}
            onChange={e => setRole(e.target.value)}
            disabled={!!invite}
            className="w-full bg-transparent dark:bg-input/30 border border-input rounded-md h-9 px-3 py-2 text-base sm:text-sm text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors disabled:opacity-50 shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
          >
            <option value="user">User</option>
            <option value="member">Member</option>
            <option value="admin">Admin</option>
          </StyledSelect>
        </div>

        {!invite ? (
          <>
            {error && (
              <p className="text-xs text-status-error/80">{error}</p>
            )}
            <button
              onClick={handleGenerate}
              disabled={loading}
              className="w-full py-2 rounded-md bg-action-primary hover:bg-action-primary-hover text-action-foreground text-sm font-medium disabled:opacity-40 transition-colors flex items-center justify-center gap-2 h-9 shadow-xs"
            >
              <Link className="w-4 h-4" />
              {loading ? 'Generating…' : 'Generate invite link'}
            </button>
          </>
        ) : (
          <div className="space-y-3">
            <div className="font-mono text-xs border border-border-primary bg-surface-primary rounded-md p-3 break-all text-text-primary select-all">
              {`${window.location.origin}${invite.invite_url}`}
            </div>
            <button
              onClick={handleCopy}
              className="w-full py-2 rounded-md bg-action-primary hover:bg-action-primary-hover text-action-foreground text-sm font-medium transition-colors h-9 shadow-xs"
            >
              {copied ? 'Copied!' : 'Copy link'}
            </button>
            <p className="text-xs text-text-tertiary text-center">
              Expires in 7 days &middot; one-time use
            </p>
          </div>
        )}
      </div>
    </Modal>
  )
}
