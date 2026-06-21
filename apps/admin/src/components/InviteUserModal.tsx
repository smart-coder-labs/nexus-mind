import { useState, useEffect, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { X } from 'lucide-react'
import type { NexusMindClient } from '../api/client'
import type { CustomRole, ProjectAccess } from '../types'

interface Props {
  open: boolean
  client: NexusMindClient
  onClose: () => void
  onSuccess: () => void
  roles?: CustomRole[]
}

export function InviteUserModal({ open, client, onClose, onSuccess, roles }: Props) {
  const [form, setForm] = useState({ email: '', name: '', role: 'member' })
  const [projectAccess, setProjectAccess] = useState<'all' | 'specific'>('all')
  const [selectedProjectIds, setSelectedProjectIds] = useState<string[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [newKey, setNewKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const modalRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    document.body.style.overflow = 'hidden'
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('keydown', handleEscape)
    return () => {
      document.body.style.overflow = ''
      document.removeEventListener('keydown', handleEscape)
    }
  }, [open, onClose])

  // Focus trap
  useEffect(() => {
    if (!open) return
    const modal = modalRef.current
    if (!modal) return
    const focusable = modal.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    )
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    first?.focus()
    const trap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last?.focus() }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first?.focus() }
      }
    }
    document.addEventListener('keydown', trap)
    return () => document.removeEventListener('keydown', trap)
  }, [open])

  const { data: projects } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    enabled: open && projectAccess === 'specific',
  })

  if (!open) return null

  const set = (field: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm(f => ({ ...f, [field]: e.target.value }))

  const toggleProject = (id: string) => {
    setSelectedProjectIds(prev =>
      prev.includes(id) ? prev.filter(p => p !== id) : [...prev, id]
    )
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLoading(true)
    setError('')
    try {
      const access: ProjectAccess =
        projectAccess === 'all'
          ? { type: 'all' }
          : { type: 'specific', project_ids: selectedProjectIds }
      const res = await client.inviteUser({ ...form, project_access: access })
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
    setProjectAccess('all')
    setSelectedProjectIds([])
    setNewKey(null)
    setCopied(false)
    setError('')
    onClose()
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label={newKey ? 'User invited' : 'Invite user'}
      onClick={handleClose}
    >
      <div
        ref={modalRef}
        className="bg-[#272729] border border-white/[0.08] rounded-[18px] p-6 w-full max-w-md space-y-5"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <p className="text-text-primary font-semibold">{newKey ? 'User invited' : 'Invite user'}</p>
          <button
            onClick={handleClose}
            aria-label="Close invite user modal"
            className="text-text-tertiary hover:text-text-primary transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {newKey ? (
          <div className="space-y-4">
            <p className="text-xs text-text-tertiary">
              User created. Share this API key — it will only be shown once.
            </p>
            <div className="flex items-center gap-2 bg-[#272729] rounded-[11px] px-3 py-2">
              <code className="flex-1 text-xs text-text-secondary break-all">{newKey}</code>
              <button
                onClick={handleCopy}
                className="text-xs text-text-tertiary hover:text-text-secondary transition-colors shrink-0"
              >
                {copied ? 'Copied!' : 'Copy'}
              </button>
            </div>
            <button
              onClick={handleClose}
              className="w-full py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal transition-colors"
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
                <label htmlFor={`invite-${f.id}`} className="text-[11px] text-text-tertiary tracking-[-0.224px]">{f.label}</label>
                <input
                  id={`invite-${f.id}`}
                  type={f.type}
                  value={form[f.id as 'name' | 'email']}
                  onChange={set(f.id)}
                  placeholder={f.placeholder}
                  required
                  className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus transition-colors"
                />
              </div>
            ))}

            <div className="space-y-1.5">
              <label htmlFor="invite-role" className="text-[11px] text-text-tertiary tracking-[-0.224px]">Role</label>
              <select
                id="invite-role"
                value={form.role}
                onChange={set('role')}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-sm text-text-secondary focus:outline-none focus:border-border-focus transition-colors"
              >
                <option value="admin">Admin</option>
                <option value="member">Member</option>
                <option value="viewer">Viewer</option>
                {roles?.map(r => (
                  <option key={r.id} value={r.name}>
                    {r.display_name}
                  </option>
                ))}
              </select>
            </div>

            {/* Project access section */}
            <div className="space-y-2">
              <label className="text-[11px] text-text-tertiary tracking-[-0.224px]">Project Access</label>
              <div className="flex gap-3">
                {(['all', 'specific'] as const).map(opt => (
                  <label key={opt} className="flex items-center gap-1.5 cursor-pointer text-xs text-text-secondary">
                    <input
                      type="radio"
                      name="projectAccess"
                      value={opt}
                      checked={projectAccess === opt}
                      onChange={() => setProjectAccess(opt)}
                      className="accent-accent-blue"
                    />
                    {opt === 'all' ? 'All projects' : 'Specific projects'}
                  </label>
                ))}
              </div>

              {projectAccess === 'specific' && (
                <div className="mt-2 space-y-1 max-h-36 overflow-y-auto border border-border-primary rounded-[11px] p-2 bg-[#272729]">
                  {!projects?.length ? (
                    <p className="text-[11px] text-text-tertiary">No projects found.</p>
                  ) : (
                    projects.map(p => (
                      <label key={p.id} className="flex items-center gap-2 cursor-pointer py-0.5">
                        <input
                          type="checkbox"
                          checked={selectedProjectIds.includes(p.id)}
                          onChange={() => toggleProject(p.id)}
                          className="accent-accent-blue"
                        />
                        <span className="text-xs text-text-secondary">{p.name}</span>
                      </label>
                    ))
                  )}
                </div>
              )}
            </div>

            {error && <p className="text-xs text-status-error/80">{error}</p>}

            <div className="flex gap-2 pt-1">
              <button
                type="button"
                onClick={handleClose}
                className="flex-1 py-2 rounded-full border border-border-primary text-sm text-text-tertiary hover:text-text-secondary hover:bg-[#272729] transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={loading}
                className="flex-1 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal disabled:opacity-40 transition-colors"
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
