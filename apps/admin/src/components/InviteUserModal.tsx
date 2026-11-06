import { StyledSelect } from '@/components/ui/Select/StyledSelect'
import { useState, useEffect, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { X } from 'lucide-react'
import type { NexusMindClient } from '../api/client'
import type { CustomRole, ProjectAccess } from '../types'
import { Radio } from './ui/Radio'

interface Props {
  open: boolean
  client: NexusMindClient
  onClose: () => void
  onSuccess: () => void
  roles?: CustomRole[]
}

const isValidEmail = (email: string) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)

export function InviteUserModal({ open, client, onClose, onSuccess, roles }: Props) {
  const [form, setForm] = useState({ email: '', name: '', role: 'member' })
  const [projectAccess, setProjectAccess] = useState<'all' | 'specific'>('specific')
  const [selectedProjectIds, setSelectedProjectIds] = useState<string[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [newKey, setNewKey] = useState<string | null>(null)
  const [inviteSuccess, setInviteSuccess] = useState<string | null>(null)
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
    if (!isValidEmail(form.email)) {
      setError('Please enter a valid email address.')
      return
    }
    setLoading(true)
    setError('')
    try {
      const access: ProjectAccess =
        projectAccess === 'all'
          ? { type: 'all' }
          : { type: 'specific', project_ids: selectedProjectIds }
      const res = await client.inviteUser({ ...form, project_access: access })
      setNewKey(res.api_key)
      setInviteSuccess(form.email)
      onSuccess()
      setTimeout(() => setInviteSuccess(null), 3000)
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
    setProjectAccess('specific')
    setSelectedProjectIds([])
    setNewKey(null)
    setInviteSuccess(null)
    setCopied(false)
    setError('')
    onClose()
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="dialog"
      aria-modal="true"
      aria-label={newKey ? 'User invited' : 'Invite user'}
      onClick={handleClose}
    >
      <div
        ref={modalRef}
        className="max-h-[calc(100dvh-2rem)] overflow-y-auto border border-border-primary bg-surface-elevated rounded-xl p-6 w-full max-w-md space-y-5 shadow-lg"
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
            <div className="flex items-center gap-2 border border-border-primary bg-surface-primary rounded-md px-3 py-2">
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
              className="w-full py-2 rounded-md bg-action-primary hover:bg-action-primary-hover text-action-foreground text-sm font-medium transition-colors h-9 shadow-xs"
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
                <label htmlFor={`invite-${f.id}`} className="text-xs font-medium text-text-secondary">{f.label}</label>
                <input
                  id={`invite-${f.id}`}
                  type={f.type}
                  value={form[f.id as 'name' | 'email']}
                  onChange={set(f.id)}
                  placeholder={f.placeholder}
                  required
                  className="h-9 w-full bg-transparent dark:bg-input/30 border border-input rounded-md px-3 py-2 text-base md:text-sm text-text-primary placeholder:text-muted-foreground focus:outline-none transition-colors shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
                />
              </div>
            ))}

            <div className="space-y-1.5">
              <label htmlFor="invite-role" className="text-xs font-medium text-text-secondary">Role</label>
              <StyledSelect
                id="invite-role"
                value={form.role}
                onChange={set('role')}
                className="h-9 w-full bg-transparent dark:bg-input/30 border border-input rounded-md px-3 py-2 text-base md:text-sm text-text-primary focus:outline-none transition-colors shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"
              >
                <option value="admin">Admin</option>
                <option value="member">Member</option>
                <option value="viewer">Viewer</option>
                {roles?.map(r => (
                  <option key={r.id} value={r.name}>
                    {r.display_name}
                  </option>
                ))}
              </StyledSelect>
            </div>

            {/* Project access section */}
            <div className="space-y-2">
              <label className="text-xs font-medium text-text-secondary">Project Access</label>
              <div className="flex flex-wrap gap-x-5 gap-y-2">
                {(['all', 'specific'] as const).map(opt => (
                  <Radio
                    key={opt}
                    name="projectAccess"
                    value={opt}
                    checked={projectAccess === opt}
                    onChange={v => setProjectAccess(v as 'all' | 'specific')}
                    label={opt === 'all' ? 'All projects' : 'Specific projects'}
                  />
                ))}
              </div>

              {projectAccess === 'specific' && (
                <div className="mt-2 space-y-1 max-h-36 overflow-y-auto border border-border-primary bg-surface-primary rounded-md p-2">
                  {!projects?.length ? (
                    <p className="text-[10px] text-text-quaternary">No projects found.</p>
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
            {inviteSuccess && (
              <p className="text-status-success text-[10px]">Invitation sent to {inviteSuccess}</p>
            )}

            <div className="flex gap-2 pt-1">
              <button
                type="button"
                onClick={handleClose}
                className="flex-1 py-2 rounded-md bg-foreground/[0.06] hover:bg-foreground/[0.10] border border-border-primary text-xs text-text-secondary hover:text-text-primary transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={loading || !isValidEmail(form.email)}
                className="flex-1 py-2 rounded-md bg-action-primary hover:bg-action-primary-hover text-action-foreground text-sm font-medium disabled:opacity-40 transition-colors h-9 shadow-xs"
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
