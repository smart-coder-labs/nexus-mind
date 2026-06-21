import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Building2, Plus, Key, Copy, Check, Eye, EyeOff } from 'lucide-react'
import { listOrgs, createOrg } from '../api/client'
import type { Org } from '../types'

// ── Superuser key gate ────────────────────────────────────────────────────────

function SuperuserKeyGate({ onUnlock }: { onUnlock: (key: string) => void }) {
  const [key, setKey] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [visible, setVisible] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!key.trim()) return
    setLoading(true)
    setError('')
    try {
      await listOrgs(key.trim())
      onUnlock(key.trim())
    } catch {
      setError('Invalid superuser key.')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="flex items-center justify-center min-h-[60vh]">
      <div className="w-full max-w-sm bg-[#1d1d1f] border border-border-primary rounded-[18px] p-8 space-y-5">
        <div className="flex items-center gap-3">
          <Key className="w-5 h-5 text-accent-blue" />
          <h2 className="text-base font-semibold text-text-primary">Superuser access required</h2>
        </div>
        <p className="text-sm text-text-secondary">
          Org management requires your superuser key. It is only held in memory for this session.
        </p>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-[11px] text-text-tertiary tracking-[-0.224px]">Superuser key</label>
            <div className="relative">
              <input
                type={visible ? 'text' : 'password'}
                value={key}
                onChange={e => setKey(e.target.value)}
                placeholder="sk_…"
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus pr-10"
                autoFocus
              />
              <button
                type="button"
                onClick={() => setVisible(v => !v)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-tertiary hover:text-text-secondary"
              >
                {visible ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
              </button>
            </div>
          </div>
          {error && <p className="text-sm text-status-error">{error}</p>}
          <button
            type="submit"
            disabled={loading || !key.trim()}
            className="w-full bg-accent-blue hover:bg-accent-blue-hover disabled:opacity-40 text-white text-sm font-normal rounded-full px-4 py-2 transition-colors"
          >
            {loading ? 'Verifying…' : 'Unlock'}
          </button>
        </form>
      </div>
    </div>
  )
}

// ── Create org modal ──────────────────────────────────────────────────────────

interface CreateOrgModalProps {
  superuserKey: string
  onClose: () => void
  onSuccess: (result: { org: Org; api_key: string }) => void
}

function CreateOrgModal({ superuserKey, onClose, onSuccess }: CreateOrgModalProps) {
  const [form, setForm] = useState({ org_name: '', org_slug: '', admin_email: '', admin_name: '' })
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  const set = (field: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => {
    setForm(f => ({ ...f, [field]: e.target.value }))
    if (field === 'org_name' && !form.org_slug) {
      setForm(f => ({ ...f, org_name: e.target.value, org_slug: e.target.value.toLowerCase().replace(/\s+/g, '_').replace(/[^a-z0-9_]/g, '') }))
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLoading(true)
    setError('')
    try {
      const result = await createOrg(superuserKey, form)
      onSuccess({ org: result.org, api_key: result.api_key })
    } catch (err: unknown) {
      const e = err as { code?: string; message?: string }
      setError(e.code === 'slug_conflict' ? 'Slug already in use. Choose a different one.' : (e.message ?? 'Something went wrong.'))
    } finally {
      setLoading(false)
    }
  }

  const fields: { key: keyof typeof form; label: string; placeholder: string; type?: string }[] = [
    { key: 'org_name',    label: 'Organization name', placeholder: 'Acme Corp' },
    { key: 'org_slug',    label: 'Slug',               placeholder: 'acme' },
    { key: 'admin_email', label: 'Admin email',        placeholder: 'admin@acme.com', type: 'email' },
    { key: 'admin_name',  label: 'Admin name',         placeholder: 'Jane Doe' },
  ]

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
      <div className="w-full max-w-md bg-[#1d1d1f] border border-border-primary rounded-[18px] p-6 space-y-5">
        <div className="flex items-center justify-between">
          <h2 className="text-base font-semibold text-text-primary">Create organization</h2>
          <button onClick={onClose} className="text-text-tertiary hover:text-text-secondary text-[18px] leading-none">×</button>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4">
          {fields.map(({ key, label, placeholder, type }) => (
            <div key={key} className="space-y-1.5">
              <label className="text-[11px] text-text-tertiary tracking-[-0.224px]">{label}</label>
              <input
                type={type ?? 'text'}
                value={form[key]}
                onChange={set(key)}
                placeholder={placeholder}
                required
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus"
              />
            </div>
          ))}
          {error && <p className="text-sm text-status-error">{error}</p>}
          <div className="flex gap-3 pt-1">
            <button type="button" onClick={onClose} className="flex-1 border border-border-primary text-text-secondary hover:text-text-primary text-sm rounded-full px-4 py-2 transition-colors">
              Cancel
            </button>
            <button
              type="submit"
              disabled={loading || !form.org_name || !form.org_slug || !form.admin_email || !form.admin_name}
              className="flex-1 bg-accent-blue hover:bg-accent-blue-hover disabled:opacity-40 text-white text-sm font-normal rounded-full px-4 py-2 transition-colors"
            >
              {loading ? 'Creating…' : 'Create'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── API key reveal ────────────────────────────────────────────────────────────

function ApiKeyReveal({ org, apiKey, onDone }: { org: Org; apiKey: string; onDone: () => void }) {
  const [copied, setCopied] = useState(false)

  const copy = () => {
    navigator.clipboard.writeText(apiKey)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
      <div className="w-full max-w-md bg-[#1d1d1f] border border-border-primary rounded-[18px] p-6 space-y-4">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-status-success" />
          <h2 className="text-base font-semibold text-text-primary">Organization created</h2>
        </div>
        <p className="text-sm text-text-secondary">
          <span className="text-text-primary font-semibold">{org.name}</span> is ready.
          Save the admin API key — it won't be shown again.
        </p>
        <div className="space-y-1.5">
          <label className="text-[11px] text-text-tertiary tracking-[-0.224px]">Admin API key</label>
          <div className="flex items-center gap-2 bg-[#272729] border border-border-secondary rounded-[11px] px-3 py-2">
            <code className="flex-1 text-xs text-text-primary font-mono break-all">{apiKey}</code>
            <button onClick={copy} className="flex-shrink-0 text-text-tertiary hover:text-accent-blue transition-colors">
              {copied ? <Check className="w-4 h-4 text-status-success" /> : <Copy className="w-4 h-4" />}
            </button>
          </div>
        </div>
        <button
          onClick={onDone}
          className="w-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal rounded-full px-4 py-2 transition-colors"
        >
          Done
        </button>
      </div>
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function Orgs() {
  const [superuserKey, setSuperuserKey] = useState<string | null>(null)
  const [showCreate, setShowCreate] = useState(false)
  const [newOrgResult, setNewOrgResult] = useState<{ org: Org; api_key: string } | null>(null)
  const qc = useQueryClient()

  const { data: orgs = [], isLoading, error } = useQuery({
    queryKey: ['orgs', superuserKey],
    queryFn: () => listOrgs(superuserKey!),
    enabled: !!superuserKey,
  })

  if (!superuserKey) {
    return <SuperuserKeyGate onUnlock={setSuperuserKey} />
  }

  return (
    <div className="p-8 max-w-4xl mx-auto space-y-8">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Organizations</h1>
          <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">All tenants on this NexusMind instance.</p>
        </div>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-2 bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal rounded-full px-4 py-2 transition-colors"
        >
          <Plus className="w-4 h-4" />
          New org
        </button>
      </div>

      {/* Table */}
      <div className="border border-border-primary rounded-[18px] overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border-secondary bg-[#272729]/50">
              <th className="px-4 py-3 text-left text-[11px] text-text-tertiary tracking-[-0.12px] font-normal">Name</th>
              <th className="px-4 py-3 text-left text-[11px] text-text-tertiary tracking-[-0.12px] font-normal">Slug</th>
              <th className="px-4 py-3 text-left text-[11px] text-text-tertiary tracking-[-0.12px] font-normal">Created</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-secondary">
            {isLoading && (
              Array.from({ length: 3 }).map((_, i) => (
                <tr key={i}>
                  {Array.from({ length: 3 }).map((_, j) => (
                    <td key={j} className="px-4 py-3">
                      <div className="h-4 rounded-[5px] bg-[#272729] animate-pulse" style={{ width: `${[60, 40, 50][j]}%` }} />
                    </td>
                  ))}
                </tr>
              ))
            )}
            {!isLoading && error && (
              <tr>
                <td colSpan={3} className="px-4 py-6 text-center text-sm text-status-error">
                  Failed to load organizations.
                </td>
              </tr>
            )}
            {!isLoading && !error && orgs.length === 0 && (
              <tr>
                <td colSpan={3} className="px-4 py-10 text-center">
                  <Building2 className="w-8 h-8 text-text-quaternary mx-auto mb-2" />
                  <p className="text-sm text-text-tertiary">No organizations yet.</p>
                </td>
              </tr>
            )}
            {orgs.map(org => (
              <tr key={org.id} className="hover:bg-[#272729]/40 transition-colors">
                <td className="px-4 py-3 text-text-primary font-semibold">{org.name}</td>
                <td className="px-4 py-3">
                  <span className="font-mono text-xs bg-[#272729] px-2 py-0.5 rounded-[5px] text-text-secondary">{org.slug}</span>
                </td>
                <td className="px-4 py-3 text-text-tertiary">
                  {new Date(org.created_at).toLocaleDateString()}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Modals */}
      {showCreate && (
        <CreateOrgModal
          superuserKey={superuserKey}
          onClose={() => setShowCreate(false)}
          onSuccess={result => {
            setShowCreate(false)
            setNewOrgResult(result)
            qc.invalidateQueries({ queryKey: ['orgs'] })
          }}
        />
      )}
      {newOrgResult && (
        <ApiKeyReveal
          org={newOrgResult.org}
          apiKey={newOrgResult.api_key}
          onDone={() => setNewOrgResult(null)}
        />
      )}
    </div>
  )
}
