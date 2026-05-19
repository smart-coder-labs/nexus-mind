import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { saveSession } from '../auth/session'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Input'

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
    <div className="min-h-screen bg-gray-950 flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-white">NexusMind</h1>
          <p className="text-gray-400 mt-2">Enterprise Memory Control Plane</p>
        </div>

        <form onSubmit={handleSubmit} className="bg-gray-900 rounded-2xl p-8 space-y-6">
          <Input
            type="password"
            label="API Key"
            value={apiKey}
            onChange={e => setApiKey(e.target.value)}
            placeholder="nm_..."
            error={error}
            autoFocus
          />

          <Button
            type="submit"
            variant="primary"
            fullWidth
            disabled={loading || !apiKey.trim()}
            loading={loading}
          >
            {loading ? 'Verifying...' : 'Sign in'}
          </Button>
        </form>

        <p className="text-center text-gray-600 text-sm mt-6">
          Run <code className="text-gray-400">./scripts/reset-demo.sh</code> to get a demo key
        </p>
      </div>
    </div>
  )
}
