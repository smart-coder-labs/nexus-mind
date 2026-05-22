import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import { createClient, loginWithEmail } from '../api/client'
import { saveSession } from '../auth/session'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'

type Mode = 'email' | 'apikey'

export default function Login() {
  const [mode, setMode] = useState<Mode>('email')

  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')

  const [apiKey, setApiKey] = useState('')

  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const { setSession } = useAuth()
  const navigate = useNavigate()

  const handleEmailSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!email.trim() || !password.trim()) return
    setLoading(true)
    setError('')
    try {
      const { api_key, org, user } = await loginWithEmail(email.trim(), password)
      const session = { apiKey: api_key, org, user }
      saveSession(session)
      setSession(session)
      navigate('/')
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Login failed'
      setError(msg.includes('Password not set')
        ? 'Password not set yet. Check your email for a setup link.'
        : 'Invalid email or password.')
    } finally {
      setLoading(false)
    }
  }

  const handleApiKeySubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!apiKey.trim()) return
    setLoading(true)
    setError('')
    try {
      const client = createClient(apiKey.trim())
      const { org, user } = await client.validateKey()
      const session = { apiKey: apiKey.trim(), org, user }
      saveSession(session)
      setSession(session)
      navigate('/')
    } catch {
      setError('Invalid API key. Check your key and try again.')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen bg-bg-primary flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-text-primary">NexusMind</h1>
          <p className="text-text-secondary mt-2 text-sm">Enterprise Memory Control Plane</p>
        </div>

        <div className="bg-surface-primary border border-border-primary rounded-2xl p-8 space-y-6 shadow-sm">
          {/* Mode toggle */}
          <div className="flex rounded-lg border border-border-primary overflow-hidden text-sm">
            <button
              type="button"
              onClick={() => { setMode('email'); setError('') }}
              className={`flex-1 py-2 transition-colors ${
                mode === 'email'
                  ? 'bg-accent-blue/10 text-accent-blue font-medium'
                  : 'text-text-tertiary hover:text-text-secondary'
              }`}
            >
              Email & Password
            </button>
            <button
              type="button"
              onClick={() => { setMode('apikey'); setError('') }}
              className={`flex-1 py-2 transition-colors border-l border-border-primary ${
                mode === 'apikey'
                  ? 'bg-accent-blue/10 text-accent-blue font-medium'
                  : 'text-text-tertiary hover:text-text-secondary'
              }`}
            >
              API Key
            </button>
          </div>

          {mode === 'email' ? (
            <form onSubmit={handleEmailSubmit} className="space-y-4">
              <Input
                type="email"
                label="Email"
                value={email}
                onChange={e => setEmail(e.target.value)}
                placeholder="admin@company.com"
                autoFocus
                autoComplete="email"
              />
              <Input
                type="password"
                label="Password"
                value={password}
                onChange={e => setPassword(e.target.value)}
                placeholder="••••••••"
                error={error}
                autoComplete="current-password"
              />
              <Button
                type="submit"
                variant="primary"
                fullWidth
                disabled={loading || !email.trim() || !password.trim()}
                loading={loading}
              >
                {loading ? 'Signing in…' : 'Sign in'}
              </Button>
            </form>
          ) : (
            <form onSubmit={handleApiKeySubmit} className="space-y-4">
              <Input
                type="password"
                label="API Key"
                value={apiKey}
                onChange={e => setApiKey(e.target.value)}
                placeholder="nm_..."
                error={error}
                autoFocus
                autoComplete="off"
              />
              <Button
                type="submit"
                variant="primary"
                fullWidth
                disabled={loading || !apiKey.trim()}
                loading={loading}
              >
                {loading ? 'Verifying…' : 'Sign in'}
              </Button>
            </form>
          )}
        </div>

        <p className="text-center text-text-tertiary text-sm mt-6">
          Run <code className="text-text-secondary">./scripts/reset-demo.sh</code> to get a demo key.
        </p>
      </div>
    </div>
  )
}
