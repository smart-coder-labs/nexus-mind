import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { saveSession } from '../auth/session'

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
      setError('Invalid API key.')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen bg-[#0c0c0e] flex items-center justify-center p-6">
      <div className="w-full max-w-sm">
        {/* Brand */}
        <div className="mb-12">
          <p className="text-[13px] font-semibold tracking-wide text-white">NexusMind</p>
          <p className="text-[11px] text-white/30 mt-1">Enterprise Memory Control Plane</p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-[11px] text-white/30 tracking-wide uppercase">
              API Key
            </label>
            <input
              type="password"
              value={apiKey}
              onChange={e => setApiKey(e.target.value)}
              placeholder="nm_..."
              autoFocus
              autoComplete="off"
              spellCheck={false}
              className="w-full bg-transparent border border-white/8 rounded-lg px-3 py-2.5 text-sm text-white placeholder:text-white/15 focus:outline-none focus:border-white/20 transition-colors"
            />
            {error && (
              <p className="text-[11px] text-red-400/80">{error}</p>
            )}
          </div>

          <button
            type="submit"
            disabled={loading || !apiKey.trim()}
            className="w-full rounded-lg px-4 py-2.5 text-sm font-medium bg-white text-[#0c0c0e] hover:bg-white/90 disabled:opacity-30 disabled:cursor-not-allowed transition-all duration-150 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-white/20"
          >
            {loading ? 'Verifying…' : 'Sign in'}
          </button>
        </form>

        <p className="mt-10 text-[11px] text-white/20">
          Run <code className="text-white/35">./scripts/reset-demo.sh</code> to get a demo key.
        </p>
      </div>
    </div>
  )
}
