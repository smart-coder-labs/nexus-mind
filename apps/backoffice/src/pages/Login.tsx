import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Shield, Zap } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'

export default function Login() {
  const [key, setKey] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const { login } = useAuth()
  const navigate = useNavigate()

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!key.trim()) return
    setLoading(true)
    setError('')
    try {
      const ok = await login(key.trim())
      if (ok) {
        navigate('/')
      } else {
        setError('Invalid superuser key. Access denied.')
      }
    } catch {
      setError('Connection error. Check that the backend is running.')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen bg-bg-primary flex items-center justify-center p-4">
      <div className="w-full max-w-sm animate-scale-in">
        {/* Header */}
        <div className="text-center mb-10">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-2xl bg-accent-blue-tint border border-accent-blue/20 mb-4">
            <Zap className="w-5 h-5 text-accent-blue" />
          </div>
          <h1 className="text-2xl font-semibold text-text-primary">NexusMind</h1>
          <div className="mt-1.5 inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-surface-secondary border border-border-primary">
            <Shield className="w-2.5 h-2.5 text-accent-blue" />
            <span className="text-[11px] text-accent-blue font-medium tracking-widest uppercase">Backoffice</span>
          </div>
          <p className="text-text-secondary text-sm mt-3">Internal operations panel. Superadmin access only.</p>
        </div>

        {/* Card */}
        <div className="bg-surface-primary border border-border-primary rounded-2xl p-8 shadow-lg">
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-1.5">
              <label htmlFor="superkey" className="block text-xs font-medium text-text-secondary uppercase tracking-wider">
                Superuser Key
              </label>
              <input
                id="superkey"
                type="password"
                value={key}
                onChange={e => setKey(e.target.value)}
                placeholder="sk_..."
                autoFocus
                autoComplete="off"
                className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3.5 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors font-mono"
              />
              {error && (
                <p className="text-xs text-status-error animate-fade-in">{error}</p>
              )}
            </div>

            <button
              id="login-submit"
              type="submit"
              disabled={loading || !key.trim()}
              className="w-full bg-accent-blue hover:bg-accent-blue-hover disabled:opacity-40 disabled:cursor-not-allowed text-bg-primary font-semibold text-sm rounded-lg py-2.5 transition-colors duration-150"
            >
              {loading ? 'Verifying…' : 'Access Backoffice'}
            </button>
          </form>
        </div>

        <p className="text-center text-xs text-text-quaternary mt-6">
          This panel is for internal use only. Unauthorized access is prohibited.
        </p>
      </div>
    </div>
  )
}
