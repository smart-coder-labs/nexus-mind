import { useState } from 'react'
import { X } from 'lucide-react'
import type { NexusMindClient } from '../api/client'

interface Props {
  open: boolean
  client: NexusMindClient
  onClose: () => void
  onSuccess: () => void
}

export function InviteUserModal({ open, client, onClose, onSuccess }: Props) {
  const [form, setForm] = useState({ email: '', name: '', role: 'member' })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [newKey, setNewKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  if (!open) return null

  const set = (field: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm(f => ({ ...f, [field]: e.target.value }))

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLoading(true)
    setError('')
    try {
      const res = await client.inviteUser(form)
      setNewKey(res.api_key)
      onSuccess()
    } catch {
      setError('Failed to invite user.')
    } finally {
      setLoading(false)
    }
  }

  const handleCopy = () => {
    if (newKey) navigator.clipboard.writeText(newKey)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const handleClose = () => {
    setForm({ email: '', name: '', role: 'member' })
    setNewKey(null)
    setCopied(false)
    setError('')
    onClose()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-[#161618] border border-white/8 rounded-xl p-6 w-full max-w-md space-y-5">
        <div className="flex items-center justify-between">
          <p className="text-white font-medium">{newKey ? 'User invited' : 'Invite user'}</p>
          <button onClick={handleClose} className="text-white/30 hover:text-white/60 transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>

        {newKey ? (
          <div className="space-y-4">
            <p className="text-xs text-white/40">
              User created. Share this API key — it will only be shown once.
            </p>
            <div className="flex items-center gap-2 bg-white/5 rounded-lg px-3 py-2">
              <code className="flex-1 text-xs text-white/70 break-all">{newKey}</code>
              <button
                onClick={handleCopy}
                className="text-xs text-white/40 hover:text-white/70 transition-colors shrink-0"
              >
                {copied ? 'Copied!' : 'Copy'}
              </button>
            </div>
            <button
              onClick={handleClose}
              className="w-full py-2 rounded-lg bg-white text-[#0c0c0e] text-sm font-medium hover:bg-white/90 transition-colors"
            >
              Done
            </button>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            {[
              { id: 'name',  label: 'Name',  type: 'text',     placeholder: 'Sarah Chen' },
              { id: 'email', label: 'Email', type: 'email',    placeholder: 'sarah@acme.com' },
            ].map(f => (
              <div key={f.id} className="space-y-1.5">
                <label className="text-[11px] text-white/30 uppercase tracking-wide">{f.label}</label>
                <input
                  type={f.type}
                  value={form[f.id as 'name' | 'email']}
                  onChange={set(f.id)}
                  placeholder={f.placeholder}
                  required
                  className="w-full bg-transparent border border-white/8 rounded-lg px-3 py-2 text-sm text-white placeholder:text-white/15 focus:outline-none focus:border-white/20 transition-colors"
                />
              </div>
            ))}

            <div className="space-y-1.5">
              <label className="text-[11px] text-white/30 uppercase tracking-wide">Role</label>
              <select
                value={form.role}
                onChange={set('role')}
                className="w-full bg-[#161618] border border-white/8 rounded-lg px-3 py-2 text-sm text-white/70 focus:outline-none focus:border-white/20 transition-colors"
              >
                <option value="admin">Admin</option>
                <option value="member">Member</option>
                <option value="viewer">Viewer</option>
              </select>
            </div>

            {error && <p className="text-xs text-red-400/80">{error}</p>}

            <div className="flex gap-2 pt-1">
              <button
                type="button"
                onClick={handleClose}
                className="flex-1 py-2 rounded-lg border border-white/8 text-sm text-white/40 hover:text-white/60 hover:bg-white/5 transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={loading}
                className="flex-1 py-2 rounded-lg bg-white text-[#0c0c0e] text-sm font-medium hover:bg-white/90 disabled:opacity-40 transition-colors"
              >
                {loading ? 'Inviting…' : 'Invite'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}
