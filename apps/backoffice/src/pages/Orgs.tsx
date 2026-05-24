import { useState, useEffect, useCallback } from 'react'
import { Link } from 'react-router-dom'
import {
  Building2,
  Plus,
  Search,
  RefreshCw,
  ArrowRight,
  AlertCircle,
  X,
  Check,
  Loader2,
} from 'lucide-react'
import { listOrgs, createOrg } from '../api/client'
import type { Org, CreateOrgResponse } from '../types'
import { cn } from '@/lib/utils'

// ── Create Org Modal ──────────────────────────────────────────────────────────

interface CreateOrgModalProps {
  onClose: () => void
  onCreated: (result: CreateOrgResponse) => void
}

function CreateOrgModal({ onClose, onCreated }: CreateOrgModalProps) {
  const [form, setForm] = useState({
    org_name: '',
    org_slug: '',
    admin_email: '',
    admin_name: '',
  })
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')

  const autoSlug = (name: string) =>
    name.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '')

  const handleNameChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const name = e.target.value
    setForm(prev => ({
      ...prev,
      org_name: name,
      org_slug: prev.org_slug === autoSlug(prev.org_name) ? autoSlug(name) : prev.org_slug,
    }))
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!form.org_name.trim() || !form.org_slug.trim() || !form.admin_email.trim() || !form.admin_name.trim()) return
    setLoading(true)
    setError('')
    try {
      const result = await createOrg(form)
      onCreated(result)
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to create organization')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-fade-in">
      <div className="w-full max-w-md bg-surface-primary border border-border-primary rounded-2xl shadow-xl animate-scale-in">
        <div className="flex items-center justify-between px-6 pt-6 pb-4 border-b border-border-secondary">
          <h2 className="text-base font-semibold text-text-primary">Create Organization</h2>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-text-tertiary hover:text-text-secondary hover:bg-surface-secondary transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-6 space-y-4">
          <div className="grid grid-cols-2 gap-3">
            <div className="col-span-2 space-y-1.5">
              <label className="block text-xs font-medium text-text-secondary">Organization name</label>
              <input
                type="text"
                value={form.org_name}
                onChange={handleNameChange}
                placeholder="Acme Corp"
                required
                className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3.5 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
              />
            </div>

            <div className="col-span-2 space-y-1.5">
              <label className="block text-xs font-medium text-text-secondary">Slug</label>
              <input
                type="text"
                value={form.org_slug}
                onChange={e => setForm(prev => ({ ...prev, org_slug: e.target.value }))}
                placeholder="acme-corp"
                required
                className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3.5 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary font-mono focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
              />
            </div>

            <div className="space-y-1.5">
              <label className="block text-xs font-medium text-text-secondary">Admin name</label>
              <input
                type="text"
                value={form.admin_name}
                onChange={e => setForm(prev => ({ ...prev, admin_name: e.target.value }))}
                placeholder="Jane Doe"
                required
                className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3.5 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
              />
            </div>

            <div className="space-y-1.5">
              <label className="block text-xs font-medium text-text-secondary">Admin email</label>
              <input
                type="email"
                value={form.admin_email}
                onChange={e => setForm(prev => ({ ...prev, admin_email: e.target.value }))}
                placeholder="jane@acme.com"
                required
                className="w-full bg-bg-secondary border border-border-primary rounded-lg px-3.5 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
              />
            </div>
          </div>

          {error && (
            <div className="flex items-center gap-2 px-3 py-2.5 bg-status-error/10 border border-status-error/20 rounded-lg text-xs text-status-error">
              <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
              {error}
            </div>
          )}

          <div className="flex gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="flex-1 px-4 py-2.5 rounded-lg text-sm text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors"
            >
              Cancel
            </button>
            <button
              id="create-org-submit"
              type="submit"
              disabled={loading || !form.org_name.trim() || !form.admin_email.trim()}
              className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-accent-blue hover:bg-accent-blue-hover disabled:opacity-40 disabled:cursor-not-allowed text-bg-primary text-sm font-semibold transition-colors"
            >
              {loading ? (
                <><Loader2 className="w-3.5 h-3.5 animate-spin" /> Creating…</>
              ) : (
                <><Check className="w-3.5 h-3.5" /> Create</>
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Success Banner ────────────────────────────────────────────────────────────

function SuccessBanner({ result, onDismiss }: { result: CreateOrgResponse; onDismiss: () => void }) {
  return (
    <div className="bg-status-success/10 border border-status-success/20 rounded-xl p-4 animate-slide-up space-y-2">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Check className="w-4 h-4 text-status-success" />
          <p className="text-sm font-medium text-status-success">Organization created</p>
        </div>
        <button onClick={onDismiss} className="text-text-tertiary hover:text-text-secondary transition-colors">
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
      <p className="text-xs text-text-secondary">
        <span className="font-medium text-text-primary">{result.org.name}</span> — Admin:{' '}
        <span className="font-medium text-text-primary">{result.user.email}</span>
      </p>
      <div className="flex items-center gap-2 bg-bg-primary rounded-lg px-3 py-2">
        <span className="text-xs text-text-tertiary">API Key:</span>
        <code className="text-xs font-mono text-accent-blue break-all">{result.api_key}</code>
      </div>
      <p className="text-[11px] text-text-tertiary">
        ⚠ Save this API key now — it will not be shown again.
      </p>
    </div>
  )
}

// ── Main Page ─────────────────────────────────────────────────────────────────

export default function Orgs() {
  const [orgs, setOrgs] = useState<Org[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [search, setSearch] = useState('')
  const [showModal, setShowModal] = useState(false)
  const [lastCreated, setLastCreated] = useState<CreateOrgResponse | null>(null)

  const fetchOrgs = useCallback(() => {
    setLoading(true)
    setError('')
    listOrgs()
      .then(setOrgs)
      .catch(err => setError(err.message ?? 'Failed to load organizations'))
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => { fetchOrgs() }, [fetchOrgs])

  const filtered = orgs.filter(o =>
    o.name.toLowerCase().includes(search.toLowerCase()) ||
    o.slug.toLowerCase().includes(search.toLowerCase()),
  )

  const handleCreated = (result: CreateOrgResponse) => {
    setLastCreated(result)
    setShowModal(false)
    fetchOrgs()
  }

  return (
    <>
      <div className="p-6 max-w-5xl mx-auto space-y-6 animate-fade-in">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold text-text-primary">Organizations</h1>
            <p className="text-sm text-text-secondary mt-0.5">
              {loading ? 'Loading…' : `${orgs.length} organization${orgs.length !== 1 ? 's' : ''}`}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              id="orgs-refresh"
              onClick={fetchOrgs}
              disabled={loading}
              className={cn(
                'p-2 rounded-lg text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors',
                loading && 'opacity-50 cursor-not-allowed',
              )}
              title="Refresh"
            >
              <RefreshCw className={cn('w-4 h-4', loading && 'animate-spin')} />
            </button>
            <button
              id="orgs-create"
              onClick={() => setShowModal(true)}
              className="flex items-center gap-2 px-4 py-2 rounded-lg bg-accent-blue hover:bg-accent-blue-hover text-bg-primary text-sm font-semibold transition-colors"
            >
              <Plus className="w-4 h-4" />
              New org
            </button>
          </div>
        </div>

        {/* Success banner */}
        {lastCreated && (
          <SuccessBanner result={lastCreated} onDismiss={() => setLastCreated(null)} />
        )}

        {/* Error */}
        {error && (
          <div className="flex items-center gap-2 px-4 py-3 bg-status-error/10 border border-status-error/20 rounded-lg text-sm text-status-error">
            <AlertCircle className="w-4 h-4 flex-shrink-0" />
            {error}
          </div>
        )}

        {/* Search */}
        <div className="relative">
          <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-text-quaternary" />
          <input
            type="text"
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder="Search organizations…"
            className="w-full bg-surface-primary border border-border-primary rounded-lg pl-10 pr-4 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/40 focus:ring-2 focus:ring-accent-blue/10 transition-colors"
          />
          {search && (
            <button
              onClick={() => setSearch('')}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-secondary transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>

        {/* Table */}
        <div className="bg-surface-primary border border-border-primary rounded-xl overflow-hidden">
          {/* Table header */}
          <div className="grid grid-cols-[1fr_160px_120px_40px] gap-4 px-5 py-3 border-b border-border-secondary">
            <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Organization</span>
            <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Slug</span>
            <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">Created</span>
            <span />
          </div>

          {loading ? (
            <div className="divide-y divide-border-secondary">
              {[...Array(5)].map((_, i) => (
                <div key={i} className="grid grid-cols-[1fr_160px_120px_40px] gap-4 px-5 py-4 items-center">
                  <div className="flex items-center gap-3">
                    <div className="w-7 h-7 bg-surface-secondary animate-pulse rounded-md" />
                    <div className="h-4 w-40 bg-surface-secondary animate-pulse rounded" />
                  </div>
                  <div className="h-3 w-24 bg-surface-secondary animate-pulse rounded" />
                  <div className="h-3 w-20 bg-surface-secondary animate-pulse rounded" />
                  <div />
                </div>
              ))}
            </div>
          ) : filtered.length === 0 ? (
            <div className="py-16 text-center">
              <Building2 className="w-8 h-8 text-text-quaternary mx-auto mb-3" />
              <p className="text-sm text-text-tertiary">
                {search ? 'No organizations match your search' : 'No organizations yet'}
              </p>
              {!search && (
                <button
                  onClick={() => setShowModal(true)}
                  className="mt-3 text-sm text-accent-blue hover:underline"
                >
                  Create the first organization
                </button>
              )}
            </div>
          ) : (
            <div className="divide-y divide-border-secondary">
              {filtered.map(org => (
                <Link
                  key={org.id}
                  to={`/orgs/${org.id}`}
                  className="grid grid-cols-[1fr_160px_120px_40px] gap-4 px-5 py-3.5 items-center hover:bg-surface-secondary/40 transition-colors group"
                >
                  <div className="flex items-center gap-3 min-w-0">
                    <div className="w-7 h-7 rounded-md bg-accent-blue-tint flex items-center justify-center flex-shrink-0">
                      <Building2 className="w-3.5 h-3.5 text-accent-blue" />
                    </div>
                    <span className="text-sm font-medium text-text-primary truncate">{org.name}</span>
                  </div>
                  <span className="text-xs font-mono text-text-tertiary truncate">{org.slug}</span>
                  <span className="text-xs text-text-tertiary">
                    {new Date(org.created_at).toLocaleDateString()}
                  </span>
                  <ArrowRight className="w-3.5 h-3.5 text-text-quaternary group-hover:text-text-secondary transition-colors" />
                </Link>
              ))}
            </div>
          )}
        </div>
      </div>

      {showModal && (
        <CreateOrgModal onClose={() => setShowModal(false)} onCreated={handleCreated} />
      )}
    </>
  )
}
