import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { saveSession } from '../auth/session'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'
import { ThemeToggle } from '@/components/ui/ThemeToggle'

export default function Login() {
  const [apiKey, setApiKey] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const { setSession } = useAuth()
  const navigate = useNavigate()

  const handleSubmit = async (e: React.FormEvent) => {
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
      {/* Theme toggle top-right */}
      <div className="fixed top-4 right-4">
        <ThemeToggle label="" allowSystem={false} />
      </div>

      <div className="w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-text-primary">NexusMind</h1>
          <p className="text-text-secondary mt-2 text-sm">Enterprise Memory Control Plane</p>
        </div>

        <form
          onSubmit={handleSubmit}
          className="bg-surface-primary border border-border-primary rounded-2xl p-8 space-y-6 shadow-sm"
        >
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

        <p className="text-center text-text-tertiary text-sm mt-6">
          Run <code className="text-text-secondary">./scripts/reset-demo.sh</code> to get a demo key.
        </p>
      </div>
    </div>
  )
}
