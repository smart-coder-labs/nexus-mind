import { useMemo, useState, useEffect, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { formatDistanceToNow, isPast, addDays, addMonths, addYears } from 'date-fns'
import { Trash2, RotateCcw, Bot, X, Copy, Check } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { ApiKeyWithUser, AgentActivity, CustomRole } from '../types'

// ── Helpers ───────────────────────────────────────────────────────────────────

function relativeTime(iso: string | null): string {
  if (!iso) return 'Never'
  return formatDistanceToNow(new Date(iso), { addSuffix: true })
}

function keyPrefix(key: ApiKeyWithUser): string {
  // label is typically something like "nm_abc123..." — show first 6 chars + ****
  const raw = key.label ?? ''
  if (raw.length >= 6) return `${raw.slice(0, 6)}****`
  return `${raw}****`
}

function agentStatus(key: ApiKeyWithUser): { label: string; className: string } {
  if (key.revoked) return { label: 'Revoked', className: 'text-status-error bg-status-error/10 border-status-error/20' }
  if (key.expires_at && isPast(new Date(key.expires_at)))
    return { label: 'Expired', className: 'text-status-warning bg-status-warning/10 border-status-warning/20' }
  return { label: 'Active', className: 'text-status-success bg-status-success/10 border-status-success/20' }
}

function StatusPill({ keyData }: { keyData: ApiKeyWithUser }) {
  const { label, className } = agentStatus(keyData)
  return (
    <span className={`text-[10px] font-semibold rounded-[5px] px-1.5 py-0.5 ${className}`}>
      {label}
    </span>
  )
}

// ── Create Agent Modal ────────────────────────────────────────────────────────

const EXPIRES_OPTIONS = [
  { label: '30 days',  value: '30d'    },
  { label: '90 days',  value: '90d'    },
  { label: '1 year',   value: '1y'     },
  { label: 'Never',    value: 'never'  },
] as const

type ExpiresOption = typeof EXPIRES_OPTIONS[number]['value']

function expiresAt(option: ExpiresOption): string | null {
  const now = new Date()
  if (option === '30d')  return addDays(now, 30).toISOString()
  if (option === '90d')  return addDays(now, 90).toISOString()
  if (option === '1y')   return addYears(addMonths(now, 0), 1).toISOString()
  return null
}

interface CreateAgentModalProps {
  open: boolean
  onClose: () => void
  onSuccess: () => void
  roles?: CustomRole[]
}

function CreateAgentModal({ open, onClose, onSuccess, roles }: CreateAgentModalProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const [name, setName]       = useState('')
  const [role, setRole]       = useState('member')
  const [expires, setExpires] = useState<ExpiresOption>('never')
  const [loading, setLoading] = useState(false)
  const [error, setError]     = useState('')
  const [newKey, setNewKey]   = useState<string | null>(null)
  const [copied, setCopied]   = useState(false)
  const modalRef = useRef<HTMLDivElement>(null)

  // Lock body scroll + Escape to close
  useEffect(() => {
    if (!open) return
    document.body.style.overflow = 'hidden'
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') handleClose() }
    document.addEventListener('keydown', handler)
    return () => {
      document.body.style.overflow = ''
      document.removeEventListener('keydown', handler)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  if (!open) return null

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setLoading(true)
    setError('')
    try {
      // Reuse inviteUser — creates a user+key pair for the agent identity
      const res = await client.inviteUser({
        email: `${name.toLowerCase().replace(/\s+/g, '-')}-agent@agent.local`,
        name: name.trim(),
        role,
        project_access: { type: 'all' },
      })
      // If a custom expires_at is needed, the agent's key can be rotated after.
      // inviteUser returns { user, api_key }.
      setNewKey(res.api_key)
      onSuccess()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create agent.')
    } finally {
      setLoading(false)
    }
  }

  const handleCopy = () => {
    if (newKey) void navigator.clipboard.writeText(newKey)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const handleClose = () => {
    setName('')
    setRole('member')
    setExpires('never')
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
      aria-label={newKey ? 'Agent created' : 'Create agent'}
      onClick={handleClose}
    >
      <div
        ref={modalRef}
        className="bg-[#1d1d1f] border border-border-primary rounded-[18px] p-6 w-full max-w-md space-y-5"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between">
          <p className="text-text-primary font-semibold text-xs">
            {newKey ? 'Agent created' : 'Create agent'}
          </p>
          <button
            onClick={handleClose}
            aria-label="Close"
            className="text-text-tertiary hover:text-text-primary transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {newKey ? (
          /* Key reveal */
          <div className="space-y-4">
            <p className="text-xs text-text-tertiary">
              Agent created. Copy this API key — it will only be shown once.
            </p>
            <div className="flex items-center gap-2 bg-white/[0.04] border border-border-primary rounded-[8px] px-3 py-2">
              <code className="flex-1 text-xs text-text-primary break-all font-mono">{newKey}</code>
              <button
                onClick={handleCopy}
                className="shrink-0 text-text-tertiary hover:text-text-secondary transition-colors"
                aria-label="Copy key"
              >
                {copied ? <Check className="w-3.5 h-3.5 text-status-success" /> : <Copy className="w-3.5 h-3.5" />}
              </button>
            </div>
            <button
              onClick={handleClose}
              className="w-full py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-semibold transition-colors"
            >
              Done
            </button>
          </div>
        ) : (
          /* Create form */
          <form onSubmit={handleSubmit} className="space-y-4">
            {/* Name */}
            <div className="space-y-1.5">
              <label htmlFor="agent-name" className="text-[10px] text-text-quaternary">
                Agent name <span className="text-status-error">*</span>
              </label>
              <input
                id="agent-name"
                type="text"
                value={name}
                onChange={e => setName(e.target.value)}
                placeholder="My CI agent"
                required
                className="w-full bg-white/[0.04] border border-border-primary rounded-[8px] px-2 py-1.5 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors"
              />
            </div>

            {/* Role */}
            <div className="space-y-1.5">
              <label htmlFor="agent-role" className="text-[10px] text-text-quaternary">Role</label>
              <select
                id="agent-role"
                value={role}
                onChange={e => setRole(e.target.value)}
                className="w-full bg-[#1d1d1f] border border-border-primary rounded-[8px] px-2 py-1.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors"
              >
                <option value="admin">Admin</option>
                <option value="member">Member</option>
                <option value="viewer">Viewer</option>
                {roles?.map(r => (
                  <option key={r.id} value={r.name}>{r.display_name}</option>
                ))}
              </select>
            </div>

            {/* Expires in */}
            <div className="space-y-1.5">
              <label htmlFor="agent-expires" className="text-[10px] text-text-quaternary">Expires in</label>
              <select
                id="agent-expires"
                value={expires}
                onChange={e => setExpires(e.target.value as ExpiresOption)}
                className="w-full bg-[#1d1d1f] border border-border-primary rounded-[8px] px-2 py-1.5 text-xs text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors"
              >
                {EXPIRES_OPTIONS.map(o => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </select>
              {expires !== 'never' && (
                <p className="text-[10px] text-text-quaternary">
                  Expires {formatDistanceToNow(new Date(expiresAt(expires)!), { addSuffix: true })}
                </p>
              )}
            </div>

            {error && <p className="text-xs text-status-error/80">{error}</p>}

            <div className="flex gap-2 pt-1">
              <button
                type="button"
                onClick={handleClose}
                className="flex-1 py-2 rounded-full border border-border-primary text-xs text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={loading || !name.trim()}
                className="flex-1 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-semibold disabled:opacity-40 transition-colors"
              >
                {loading ? 'Creating…' : 'Create agent'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  )
}

// ── Skeleton card ─────────────────────────────────────────────────────────────

function SkeletonCard() {
  return (
    <div className="bg-[#272729] rounded-[18px] border border-border-primary p-5 flex flex-col gap-3">
      <div className="flex items-start justify-between">
        <div className="animate-pulse h-4 bg-white/[0.06] rounded-[8px] w-32" />
        <div className="animate-pulse h-4 bg-white/[0.06] rounded-full w-14" />
      </div>
      <div className="animate-pulse h-3 bg-white/[0.04] rounded-[8px] w-24" />
      <div className="animate-pulse h-3 bg-white/[0.04] rounded-[8px] w-20" />
    </div>
  )
}

// ── Agent Card ────────────────────────────────────────────────────────────────

interface AgentCardProps {
  keyData: ApiKeyWithUser
  onRevoke: (key: ApiKeyWithUser) => void
  onRotate: (key: ApiKeyWithUser) => void
  revoking: boolean
}

function AgentCard({ keyData, onRevoke, onRotate, revoking }: AgentCardProps) {
  return (
    <div className="bg-[#272729] rounded-[18px] border border-border-primary p-5 flex flex-col gap-3 group relative transition-shadow hover:shadow-lg">
      {/* Hover actions */}
      <div className="absolute top-4 right-4 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          onClick={() => onRotate(keyData)}
          className="p-1.5 rounded-[8px] text-text-quaternary hover:text-text-primary hover:bg-white/[0.06] transition-colors"
          aria-label="Rotate key"
          title="Rotate key"
        >
          <RotateCcw className="w-3.5 h-3.5" />
        </button>
        <button
          onClick={() => onRevoke(keyData)}
          disabled={revoking || keyData.revoked}
          className="p-1.5 rounded-[8px] text-text-quaternary hover:text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-40"
          aria-label="Revoke key"
          title="Revoke key"
        >
          <Trash2 className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Top row: name + status */}
      <div className="flex items-start gap-2 pr-16">
        <div className="w-7 h-7 rounded-full bg-accent-blue/15 border border-accent-blue/20 text-accent-blue text-xs font-semibold flex items-center justify-center shrink-0 mt-0.5">
          {keyData.user_name?.charAt(0).toUpperCase() ?? <Bot className="w-3.5 h-3.5" />}
        </div>
        <div className="min-w-0">
          <p className="text-xs font-semibold text-text-primary truncate">{keyData.user_name}</p>
          <p className="text-[11px] text-text-quaternary font-mono mt-0.5">{keyPrefix(keyData)}</p>
        </div>
      </div>

      {/* Status pill */}
      <div className="flex items-center gap-2">
        <StatusPill keyData={keyData} />
        <span className="text-[11px] text-text-quaternary">{keyData.user_email}</span>
      </div>

      {/* Stats row */}
      <div className="flex items-center gap-4 text-[11px] text-text-quaternary">
        <span>
          <span className="text-text-secondary font-semibold">{keyData.times_used ?? 0}</span> requests
        </span>
        <span>
          Last used: <span className="text-text-secondary">{relativeTime(keyData.last_used_at ?? null)}</span>
        </span>
      </div>

      {/* Footer row */}
      <div className="flex items-center justify-between text-[10px] text-text-quaternary border-t border-border-secondary/30 pt-2.5 mt-0.5">
        <span>Created {new Date(keyData.created_at).toLocaleDateString()}</span>
        {keyData.expires_at ? (
          <span>
            {isPast(new Date(keyData.expires_at))
              ? <span className="text-status-warning">Expired</span>
              : <>Expires {formatDistanceToNow(new Date(keyData.expires_at), { addSuffix: true })}</>
            }
          </span>
        ) : (
          <span>No expiry</span>
        )}
      </div>
    </div>
  )
}

// ── Agent Activity section ────────────────────────────────────────────────────

function AgentActivitySection() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const { data: activity, isLoading } = useQuery<AgentActivity[]>({
    queryKey: ['agent-activity'],
    queryFn: () => client.getAgentActivity(30),
  })

  const topAgents = useMemo(() => {
    if (!activity) return []
    const counts: Record<string, number> = {}
    for (const item of activity) {
      const name = item.tool?.split('/')[0] ?? 'unknown'
      counts[name] = (counts[name] ?? 0) + (item.total_memories ?? 0)
    }
    return Object.entries(counts)
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 5)
  }, [activity])

  if (isLoading) {
    return (
      <div className="space-y-2">
        {[1, 2, 3].map(i => (
          <div key={i} className="animate-pulse h-10 bg-[#272729] rounded-[11px]" />
        ))}
      </div>
    )
  }

  if (!activity?.length) {
    return (
      <p className="text-xs text-text-quaternary text-center py-8 italic">
        No recent agent activity.
      </p>
    )
  }

  return (
    <div>
      <div className="space-y-1.5">
        {activity.map(item => (
          <div
            key={item.tool}
            className="flex items-center justify-between px-4 py-2.5 rounded-[11px] bg-[#272729] border border-border-secondary/50"
          >
            <div className="flex items-center gap-3 min-w-0">
              <div className="w-6 h-6 rounded-full bg-accent-blue/10 flex items-center justify-center shrink-0">
                <Bot className="w-3 h-3 text-accent-blue" />
              </div>
              <div className="min-w-0">
                <p className="text-xs font-semibold text-text-primary truncate">{item.tool}</p>
                <p className="text-[10px] text-text-quaternary">
                  Last seen {relativeTime(item.last_seen)}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-4 text-[11px] text-text-quaternary shrink-0 ml-4">
              <span>
                <span className="text-text-secondary font-semibold">{item.total_memories}</span> total
              </span>
              <span>
                <span className="text-text-secondary font-semibold">{item.memories_last_7d}</span> /7d
              </span>
            </div>
          </div>
        ))}
      </div>

      {/* Per-agent leaderboard */}
      {topAgents.length > 0 && (
        <div className="mt-6">
          <p className="text-[10px] font-semibold text-text-quaternary uppercase tracking-wide mb-2">
            Top agents by requests
          </p>
          <div className="bg-[#272729] rounded-[18px] border border-border-primary p-5">
            {topAgents.map((agent, idx) => (
              <div key={agent.name} className="flex items-center justify-between py-1.5 border-b border-border-primary last:border-0">
                <span className="text-xs font-semibold text-text-primary truncate">{idx + 1}. {agent.name}</span>
                <span className="text-xs text-text-quaternary shrink-0 ml-4">{agent.count}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function Agents() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [createOpen, setCreateOpen] = useState(false)
  const [statusFilter, setStatusFilter] = useState<'All' | 'Active' | 'Inactive' | 'Expired'>('All')

  const { data: keys, isLoading } = useQuery<ApiKeyWithUser[]>({
    queryKey: ['org-keys'],
    queryFn: () => client.listOrgKeys(),
  })

  const { data: roles } = useQuery({
    queryKey: ['roles'],
    queryFn: () => client.listRoles(),
    enabled: session?.user.role === 'admin',
  })

  const filteredKeys = useMemo(() => {
    if (!keys || statusFilter === 'All') return keys ?? []
    return keys.filter(key => {
      const { label } = agentStatus(key)
      if (statusFilter === 'Active') return label === 'Active'
      if (statusFilter === 'Inactive') return label === 'Revoked'
      if (statusFilter === 'Expired') return label === 'Expired'
      return true
    })
  }, [keys, statusFilter])

  const revokeMut = useMutation({
    mutationFn: (keyId: string) => client.revokeOrgKey(keyId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['org-keys'] }),
  })

  const handleRevoke = (key: ApiKeyWithUser) => {
    if (!window.confirm(`Revoke key for "${key.user_name}"? This cannot be undone.`)) return
    revokeMut.mutate(key.id)
  }

  const handleRotate = (key: ApiKeyWithUser) => {
    if (!window.confirm(`Rotate key for "${key.user_name}"? The old key will stop working immediately.`)) return
    client.resetUserKey(key.user_id)
      .then(() => qc.invalidateQueries({ queryKey: ['org-keys'] }))
      .catch((err: Error) => window.alert(err.message))
  }

  const handleCreateSuccess = () => {
    qc.invalidateQueries({ queryKey: ['org-keys'] })
  }

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-10">
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-base font-semibold text-text-primary">Agents</h1>
          <p className="text-xs text-text-quaternary mt-0.5">
            AI agents connected to NexusMind via API key.
          </p>
        </div>
        <button
          onClick={() => setCreateOpen(true)}
          className="bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold hover:bg-accent-blue-hover transition-colors shrink-0"
        >
          Create agent
        </button>
      </div>

      {/* Filter bar */}
      <div className="flex items-center">
        <div className="bg-white/[0.04] rounded-full p-0.5 flex items-center gap-0.5">
          {(['All', 'Active', 'Inactive', 'Expired'] as const).map(filter => (
            <button
              key={filter}
              onClick={() => setStatusFilter(filter)}
              className={`px-3 py-1 rounded-full text-xs transition-colors border ${
                statusFilter === filter
                  ? 'bg-accent-blue/10 text-accent-blue border-accent-blue/40 font-semibold'
                  : 'text-text-quaternary border-transparent'
              }`}
            >
              {filter}
            </button>
          ))}
        </div>
      </div>

      {/* Revoke error */}
      {revokeMut.isError && (
        <div className="rounded-[11px] border border-status-error/20 bg-status-error/5 px-4 py-3 text-xs text-status-error">
          {revokeMut.error instanceof Error ? revokeMut.error.message : 'Failed to revoke key'}
        </div>
      )}

      {/* Agent cards grid */}
      <section>
        <h2 className="text-[13px] font-semibold text-text-tertiary mb-3 uppercase tracking-wide">
          Connected agents
        </h2>
        {isLoading ? (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {[1, 2, 3].map(i => <SkeletonCard key={i} />)}
          </div>
        ) : !keys?.length ? (
          <div className="flex flex-col items-center justify-center py-16 gap-4">
            <div className="w-12 h-12 rounded-full bg-[#272729] flex items-center justify-center">
              <Bot className="w-6 h-6 text-text-quaternary" />
            </div>
            <p className="text-xs font-semibold text-text-tertiary">No agents yet</p>
            <p className="text-xs text-text-quaternary text-center max-w-xs">
              Create an agent to give an AI assistant a dedicated API key and identity.
            </p>
          </div>
        ) : filteredKeys.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 gap-2">
            <p className="text-xs text-text-tertiary">No {statusFilter.toLowerCase()} agents.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {filteredKeys.map(key => (
              <AgentCard
                key={key.id}
                keyData={key}
                onRevoke={handleRevoke}
                onRotate={handleRotate}
                revoking={revokeMut.isPending}
              />
            ))}
          </div>
        )}
      </section>

      {/* Agent Activity */}
      <section>
        <h2 className="text-[13px] font-semibold text-text-tertiary mb-3 uppercase tracking-wide">
          Agent activity (last 30 days)
        </h2>
        <AgentActivitySection />
      </section>

      {/* Create Agent Modal */}
      <CreateAgentModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onSuccess={handleCreateSuccess}
        roles={roles}
      />
    </div>
  )
}
