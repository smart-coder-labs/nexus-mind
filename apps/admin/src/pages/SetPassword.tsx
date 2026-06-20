import { useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'

const BASE_URL = import.meta.env.VITE_API_URL ?? ''

export default function SetPassword() {
  const [params] = useSearchParams()
  const token = params.get('token') ?? ''
  const navigate = useNavigate()

  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [done, setDone] = useState(false)

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
    setLoading(true)
    setError('')
    try {
      const res = await fetch(`${BASE_URL}/v1/admin/auth/set-password`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token, password }),
      })
      if (!res.ok) {
        const body = await res.json().catch(() => ({ error: 'Request failed' }))
        throw new Error(body.error ?? 'Request failed')
      }
      setDone(true)
      setTimeout(() => navigate('/login'), 2000)
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Something went wrong.')
    } finally {
      setLoading(false)
    }
  }

  if (!token) {
    return (
      <div className="min-h-screen bg-[#1d1d1f] flex items-center justify-center p-4">
        <div className="text-center space-y-2">
          <p className="text-text-primary font-semibold">Invalid link</p>
          <p className="text-text-tertiary text-sm">This link is missing a token.</p>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-[#1d1d1f] flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-[34px] font-semibold text-text-primary tracking-[-0.374px]">NexusMind</h1>
          <p className="text-text-secondary mt-2 text-sm">Set your password to get started</p>
        </div>

        <div className="bg-surface-primary border border-border-primary rounded-[18px] p-8">
          {done ? (
            <div className="text-center space-y-2 py-4">
              <p className="text-text-primary font-semibold">Password set!</p>
              <p className="text-text-tertiary text-sm">Redirecting to login…</p>
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-4">
              <Input
                type="password"
                label="New password"
                value={password}
                onChange={e => setPassword(e.target.value)}
                placeholder="At least 8 characters"
                autoFocus
                autoComplete="new-password"
              />
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
                disabled={loading || !password || !confirm}
                loading={loading}
              >
                {loading ? 'Setting password…' : 'Set password'}
              </Button>
            </form>
          )}
        </div>
      </div>
    </div>
  )
}
