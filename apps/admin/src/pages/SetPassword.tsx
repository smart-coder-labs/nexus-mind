import { useState, useEffect, useCallback } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'

const BASE_URL = import.meta.env.VITE_API_URL ?? ''

interface InviteValidation {
  valid: boolean
  role?: string
  org_name?: string
  reason?: string
}

function ApiKeyModal({ apiKey, onClose }: { apiKey: string; onClose: () => void }) {
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    document.body.style.overflow = 'hidden'
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handleEscape)
    return () => {
      document.body.style.overflow = ''
      document.removeEventListener('keydown', handleEscape)
    }
  }, [onClose])

  const handleCopy = () => {
    navigator.clipboard.writeText(apiKey)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center" onClick={onClose}>
      <div
        className="bg-[#272729] border border-border-primary rounded-[18px] p-6 max-w-sm w-full mx-4 space-y-4"
        onClick={e => e.stopPropagation()}
      >
        <p className="text-text-primary font-semibold">Setup complete!</p>
        <p className="text-xs text-status-warning">Copy your API key — it won't be shown again.</p>
        <div className="font-mono text-sm bg-[#1d1d1f] rounded-[11px] p-3 break-all select-all text-text-primary border border-border-secondary flex items-center gap-2">
          <span className="flex-1">{apiKey}</span>
          <button
            onClick={handleCopy}
            className="shrink-0 text-text-tertiary hover:text-text-primary transition-colors"
            aria-label="Copy API key"
          >
            {copied ? '✓' : '⧉'}
          </button>
        </div>
        <Button variant="primary" fullWidth onClick={onClose}>
          Go to login
        </Button>
      </div>
    </div>
  )
}

export default function SetPassword() {
  const [params] = useSearchParams()
  const inviteToken = params.get('invite')
  const resetToken = params.get('token') ?? ''
  const navigate = useNavigate()

  // Invite flow state
  const [invite, setInvite] = useState<InviteValidation | null>(null)
  const [inviteLoading, setInviteLoading] = useState(false)
  const [name, setName] = useState('')

  // Shared state
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [done, setDone] = useState(false)
  const [newApiKey, setNewApiKey] = useState<string | null>(null)

  // Validate invite token on mount
  const validateInvite = useCallback(async (token: string) => {
    setInviteLoading(true)
    try {
      const res = await fetch(`${BASE_URL}/v1/invites/${encodeURIComponent(token)}`)
      const data = await res.json().catch(() => ({ valid: false, reason: 'server_error' }))
      setInvite(data)
    } catch {
      setInvite({ valid: false, reason: 'server_error' })
    } finally {
      setInviteLoading(false)
    }
  }, [])

  useEffect(() => {
    if (inviteToken) {
      validateInvite(inviteToken)
    }
  }, [inviteToken, validateInvite])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (password !== confirm) {
      setError('Passwords do not match.')
      return
    }
    if (password.length < 8) {
      setError('Password must be at least 8 characters.')
      return
    }
    if (inviteToken && name.trim() === '') {
      setError('Name is required.')
      return
    }
    setLoading(true)
    setError('')

    try {
      if (inviteToken) {
        // Invite redemption flow
        const res = await fetch(`${BASE_URL}/v1/invites/${encodeURIComponent(inviteToken)}/redeem`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ password, name }),
        })
        if (!res.ok) {
          const body = await res.json().catch(() => ({ error: 'Request failed' }))
          throw new Error(body.error ?? 'Request failed')
        }
        const data = await res.json()
        setNewApiKey(data.api_key)
      } else {
        // Password reset flow (existing behavior)
        const res = await fetch(`${BASE_URL}/v1/admin/auth/set-password`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ token: resetToken, password }),
        })
        if (!res.ok) {
          const body = await res.json().catch(() => ({ error: 'Request failed' }))
          throw new Error(body.error ?? 'Request failed')
        }
        setDone(true)
        setTimeout(() => navigate('/login'), 2000)
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Something went wrong.')
    } finally {
      setLoading(false)
    }
  }

  // No token at all
  if (!inviteToken && !resetToken) {
    return (
      <div className="min-h-screen bg-[#1d1d1f] flex items-center justify-center p-4">
        <div className="text-center space-y-2">
          <p className="text-text-primary font-semibold">Invalid link</p>
          <p className="text-text-tertiary text-sm">This link is missing a token.</p>
        </div>
      </div>
    )
  }

  // Invite token present but still loading validation
  if (inviteToken && inviteLoading) {
    return (
      <div className="min-h-screen bg-[#1d1d1f] flex items-center justify-center p-4">
        <p className="text-text-tertiary text-sm">Validating invite…</p>
      </div>
    )
  }

  // Invite token present but invalid/expired
  if (inviteToken && invite && !invite.valid) {
    const reason = invite.reason
    const message =
      reason === 'used' || reason === 'expired'
        ? 'This invite link has expired or already been used.'
        : 'This invite link is not valid.'

    return (
      <div className="min-h-screen bg-[#1d1d1f] flex items-center justify-center p-4">
        <div className="text-center space-y-2">
          <p className="text-status-error font-semibold">{message}</p>
          <p className="text-text-tertiary text-sm">Please ask your admin for a new link.</p>
        </div>
      </div>
    )
  }

  return (
    <>
      {newApiKey && (
        <ApiKeyModal
          apiKey={newApiKey}
          onClose={() => {
            setNewApiKey(null)
            navigate('/login')
          }}
        />
      )}

      <div className="min-h-screen bg-[#1d1d1f] flex items-center justify-center p-4">
        <div className="w-full max-w-md">
          <div className="text-center mb-8">
            <h1 className="text-[34px] font-semibold text-text-primary tracking-[-0.374px]">NexusMind</h1>
            <p className="text-text-secondary mt-2 text-sm">
              {inviteToken ? 'Create your account to get started' : 'Set your password to get started'}
            </p>
          </div>

          <div className="bg-[#272729] border border-white/[0.06] rounded-[18px] p-8">
            {done ? (
              <div className="text-center space-y-2 py-4">
                <p className="text-text-primary font-semibold">Password set!</p>
                <p className="text-text-tertiary text-sm">Redirecting to login…</p>
              </div>
            ) : (
              <form onSubmit={handleSubmit} className="space-y-4">
                {/* Invite welcome banner */}
                {inviteToken && invite?.valid && invite.org_name && (
                  <div className="bg-accent-blue/5 border border-accent-blue/20 rounded-[11px] p-3 text-sm text-text-secondary mb-4">
                    You've been invited to join <span className="text-text-primary font-semibold">{invite.org_name}</span>
                  </div>
                )}

                {/* Name field — invite flow only */}
                {inviteToken && (
                  <Input
                    type="text"
                    label="Your name"
                    value={name}
                    onChange={e => setName(e.target.value)}
                    placeholder="Full name"
                    autoFocus
                    autoComplete="name"
                  />
                )}

                <div>
                  <Input
                    type="password"
                    label="New password"
                    value={password}
                    onChange={e => setPassword(e.target.value)}
                    placeholder="At least 8 characters"
                    autoFocus={!inviteToken}
                    autoComplete="new-password"
                  />
                  <p className="text-[11px] text-text-quaternary mt-1">At least 8 characters</p>
                </div>
                <Input
                  type="password"
                  label="Confirm password"
                  value={confirm}
                  onChange={e => setConfirm(e.target.value)}
                  placeholder="Repeat password"
                  error={error}
                  autoComplete="new-password"
                />
                <Button
                  type="submit"
                  variant="primary"
                  fullWidth
                  disabled={loading || !password || !confirm || (inviteToken ? !name : false)}
                  loading={loading}
                >
                  {loading
                    ? inviteToken ? 'Creating account…' : 'Setting password…'
                    : inviteToken ? 'Create account' : 'Set password'}
                </Button>
              </form>
            )}
          </div>
        </div>
      </div>
    </>
  )
}
