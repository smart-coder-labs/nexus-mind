import { useMemo, useState, useEffect, useRef } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { formatDistanceToNow, isPast, addDays, addMonths, addYears } from 'date-fns'
import { Trash2, RotateCcw, Bot, X, Copy, Check } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { ApiKeyWithUser, AgentActivity, CustomRole } from '../types'

// ── Helpers ───────────────────────────────────────────────────────────────────

// Keyboard focus indicator (design direction §6): 2px --color-focus-ring outline,
// 2px offset. Uses outline (not ring) so it isn't clipped by overflow-hidden
// ancestors. Both aliases are identical now; kept for call-site readability.
const FOCUS_CANVAS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'
const FOCUS_TILE = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

// Server timestamps from SQLite datetime('now') are naive UTC (no zone). Parse
// them as UTC so past events don't render in the future. No-op for zoned or
// date-only strings (a bare date is already parsed as UTC midnight).
function toDate(iso: string): Date {
  if (/[zZ]$|[+-]\d{2}:?\d{2}$/.test(iso)) return new Date(iso)
  if (/\d{2}:\d{2}/.test(iso)) return new Date(iso.replace(' ', 'T') + 'Z')
  return new Date(iso)
}

function relativeTime(iso: string | null): string {
  if (!iso) return 'Never'
  return formatDistanceToNow(toDate(iso), { addSuffix: true })
}

function keyPrefix(key: ApiKeyWithUser): string {
  // label is typically something like "nm_abc123..." — show first 6 chars + ****
  const raw = key.label ?? ''
  if (raw.length >= 6) return `${raw.slice(0, 6)}****`
  return `${raw}****`
}

function agentStatus(key: ApiKeyWithUser): { label: string; className: string } {
  if (key.revoked) return { label: 'Revoked', className: 'text-status-error bg-status-error/10 border-status-error/20' }
  if (key.expires_at && isPast(toDate(key.expires_at)))
    return { label: 'Expired', className: 'text-status-warning bg-status-warning/10 border-status-warning/20' }
  return { label: 'Active', className: 'text-status-success bg-status-success/10 border-status-success/20' }
}

function StatusPill({ keyData }: { keyData: ApiKeyWithUser }) {
  const { label, className } = agentStatus(keyData)
  return (
    <span className={`text-[11px] font-semibold rounded-full px-2 py-0.5 ${className}`}>
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
        className="bg-background-secondary border border-border-primary rounded-[18px] p-6 w-full max-w-md space-y-5"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between">
          <p className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">
            {newKey ? 'Agent created' : 'Create agent'}
          </p>
          <button
            onClick={handleClose}
            aria-label="Close"
            className={`text-text-tertiary hover:text-text-primary transition-colors rounded-full ${FOCUS_CANVAS}`}
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {newKey ? (
          /* Key reveal */
          <div className="space-y-4">
            <p className="text-[13px] text-text-tertiary">
              Agent created. Copy this API key — it will only be shown once.
            </p>
            <div className="flex items-center gap-2 bg-white/[0.04] border border-border-primary rounded-[11px] px-3 py-2">
              <code className="flex-1 text-[13px] text-text-primary break-all font-mono">{newKey}</code>
              <button
                onClick={handleCopy}
                className={`shrink-0 text-text-tertiary hover:text-text-secondary transition-colors rounded-[8px] ${FOCUS_CANVAS}`}
                aria-label="Copy key"
              >
                {copied ? <Check className="w-3.5 h-3.5 text-status-success" /> : <Copy className="w-3.5 h-3.5" />}
              </button>
            </div>
            <button
              onClick={handleClose}
              className={`w-full py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-[13px] font-semibold transition-colors ${FOCUS_CANVAS}`}
            >
              Done
            </button>
          </div>
        ) : (
          /* Create form */
          <form onSubmit={handleSubmit} className="space-y-4">
            {/* Name */}
            <div className="space-y-1.5">
              <label htmlFor="agent-name" className="text-[12px] font-medium text-text-secondary">
                Agent name <span className="text-status-error">*</span>
              </label>
              <input
                id="agent-name"
                type="text"
                value={name}
                onChange={e => setName(e.target.value)}
                placeholder="My CI agent"
                required
                className={`w-full bg-white/[0.04] border border-border-primary rounded-[11px] px-3 h-9 text-[13px] text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors ${FOCUS_CANVAS}`}
              />
            </div>

            {/* Role */}
            <div className="space-y-1.5">
              <label htmlFor="agent-role" className="text-[12px] font-medium text-text-secondary">Role</label>
              <select
                id="agent-role"
                value={role}
                onChange={e => setRole(e.target.value)}
                className={`w-full bg-background-secondary border border-border-primary rounded-[11px] px-3 h-9 text-[13px] text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors ${FOCUS_CANVAS}`}
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
              <label htmlFor="agent-expires" className="text-[12px] font-medium text-text-secondary">Expires in</label>
              <select
                id="agent-expires"
                value={expires}
                onChange={e => setExpires(e.target.value as ExpiresOption)}
                className={`w-full bg-background-secondary border border-border-primary rounded-[11px] px-3 h-9 text-[13px] text-text-secondary focus:outline-none focus:border-accent-blue/60 transition-colors ${FOCUS_CANVAS}`}
              >
                {EXPIRES_OPTIONS.map(o => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </select>
              {expires !== 'never' && (
                <p className="text-[12px] text-text-tertiary">
                  Expires {formatDistanceToNow(new Date(expiresAt(expires)!), { addSuffix: true })}
                </p>
              )}
            </div>

            {error && <p className="text-[13px] text-status-error/80">{error}</p>}

            <div className="flex gap-2 pt-1">
              <button
                type="button"
                onClick={handleClose}
                className={`flex-1 py-2 rounded-full border border-border-primary text-[13px] text-text-secondary hover:text-text-primary hover:bg-white/[0.04] transition-colors ${FOCUS_CANVAS}`}
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={loading || !name.trim()}
                className={`flex-1 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-[13px] font-semibold disabled:opacity-40 transition-colors ${FOCUS_CANVAS}`}
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
    <div className="bg-background-tertiary rounded-[18px] border border-border-primary p-5 flex flex-col gap-3">
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
    <div className="bg-background-tertiary rounded-[18px] border border-border-primary p-5 flex flex-col gap-3 group relative transition-colors hover:border-white/[0.12]">
      {/* Hover actions */}
      <div className="absolute top-4 right-4 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          onClick={() => onRotate(keyData)}
          className={`p-1.5 rounded-[8px] text-text-tertiary hover:text-text-primary hover:bg-white/[0.06] transition-colors ${FOCUS_TILE}`}
          aria-label="Rotate key"
          title="Rotate key"
        >
          <RotateCcw className="w-3.5 h-3.5" />
        </button>
        <button
          onClick={() => onRevoke(keyData)}
          disabled={revoking || keyData.revoked}
          className={`p-1.5 rounded-[8px] text-text-tertiary hover:text-status-error hover:bg-status-error/10 transition-colors disabled:opacity-40 ${FOCUS_TILE}`}
          aria-label="Revoke key"
          title="Revoke key"
        >
          <Trash2 className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Top row: name + status */}
      <div className="flex items-start gap-2 pr-16">
        <div className="w-7 h-7 rounded-full bg-accent-blue/15 border border-accent-blue/20 text-accent-blue text-[13px] font-semibold flex items-center justify-center shrink-0 mt-0.5">
          {keyData.user_name?.charAt(0).toUpperCase() ?? <Bot className="w-3.5 h-3.5" />}
        </div>
        <div className="min-w-0">
          <p className="text-[13px] font-semibold text-text-primary truncate">{keyData.user_name}</p>
          <p className="text-[11px] text-text-tertiary font-mono mt-0.5">{keyPrefix(keyData)}</p>
        </div>
      </div>

      {/* Status pill */}
      <div className="flex items-center gap-2">
        <StatusPill keyData={keyData} />
        <span className="text-[12px] text-text-tertiary">{keyData.user_email}</span>
      </div>

      {/* Stats row */}
      <div className="flex items-center gap-4 text-[12px] text-text-tertiary">
        <span>
          <span className="text-text-secondary font-semibold">{keyData.times_used ?? 0}</span> requests
        </span>
        <span>
          Last used: <span className="text-text-secondary">{relativeTime(keyData.last_used_at ?? null)}</span>
        </span>
      </div>

      {/* Footer row */}
      <div className="flex items-center justify-between text-[12px] text-text-tertiary border-t border-border-secondary/30 pt-2.5 mt-0.5">
        <span>Created {toDate(keyData.created_at).toLocaleDateString()}</span>
        {keyData.expires_at ? (
          <span>
            {isPast(toDate(keyData.expires_at))
              ? <span className="text-status-warning">Expired</span>
              : <>Expires {formatDistanceToNow(toDate(keyData.expires_at), { addSuffix: true })}</>
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
          <div key={i} className="animate-pulse h-10 bg-background-tertiary rounded-[11px]" />
        ))}
      </div>
    )
  }

  if (!activity?.length) {
    return (
      <p className="text-[13px] text-text-tertiary text-center py-8">
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
            className="flex items-center justify-between px-4 py-2.5 rounded-[11px] bg-background-tertiary border border-border-secondary/50"
          >
            <div className="flex items-center gap-3 min-w-0">
              <div className="w-6 h-6 rounded-full bg-accent-blue/10 flex items-center justify-center shrink-0">
                <Bot className="w-3 h-3 text-accent-blue" />
              </div>
              <div className="min-w-0">
                <p className="text-[13px] font-semibold text-text-primary truncate">{item.tool}</p>
                <p className="text-[12px] text-text-tertiary">
                  Last seen {relativeTime(item.last_seen)}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-4 text-[12px] text-text-tertiary shrink-0 ml-4">
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
          <p className="text-[11px] font-semibold text-text-tertiary uppercase tracking-wide mb-2">
            Top agents by requests
          </p>
          <div className="bg-background-tertiary rounded-[18px] border border-border-primary p-5">
            {topAgents.map((agent, idx) => (
              <div key={agent.name} className="flex items-center justify-between py-1.5 border-b border-border-primary last:border-0">
                <span className="text-[13px] font-semibold text-text-primary truncate">{idx + 1}. {agent.name}</span>
                <span className="text-[13px] text-text-tertiary shrink-0 ml-4">{agent.count}</span>
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
    enabled: (session?.user.role === 'admin' || session?.user.role === 'super_user'),
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
    <div className="p-6 max-w-5xl mx-auto space-y-8">
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">Agents</h1>
          <p className="text-[13px] text-text-secondary mt-1">
            AI agents connected to NexusMind via API key.
          </p>
        </div>
        <button
          onClick={() => setCreateOpen(true)}
          className={`bg-accent-blue text-white rounded-full px-4 py-1.5 text-[13px] font-semibold hover:bg-accent-blue-hover transition-colors shrink-0 ${FOCUS_CANVAS}`}
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
              className={`px-3 py-1 rounded-full text-[13px] transition-colors border ${FOCUS_CANVAS} ${
                statusFilter === filter
                  ? 'bg-accent-blue/10 text-accent-blue border-accent-blue/40 font-semibold'
                  : 'text-text-tertiary border-transparent hover:text-text-secondary'
              }`}
            >
              {filter}
            </button>
          ))}
        </div>
      </div>

      {/* Revoke error */}
      {revokeMut.isError && (
        <div className="rounded-[11px] border border-status-error/20 bg-status-error/5 px-4 py-3 text-[13px] text-status-error">
          {revokeMut.error instanceof Error ? revokeMut.error.message : 'Failed to revoke key'}
        </div>
      )}

      {/* Agent cards grid */}
      <section>
        <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-3">
          Connected agents
        </h2>
        {isLoading ? (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {[1, 2, 3].map(i => <SkeletonCard key={i} />)}
          </div>
        ) : !keys?.length ? (
          <div className="flex flex-col items-center justify-center py-16 gap-4">
            <div className="w-12 h-12 rounded-full bg-background-tertiary flex items-center justify-center">
              <Bot className="w-6 h-6 text-text-quaternary" />
            </div>
            <p className="text-[13px] font-semibold text-text-tertiary">No agents yet</p>
            <p className="text-[13px] text-text-tertiary text-center max-w-xs">
              Create an agent to give an AI assistant a dedicated API key and identity.
            </p>
          </div>
        ) : filteredKeys.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 gap-2">
            <p className="text-[13px] text-text-tertiary">No {statusFilter.toLowerCase()} agents.</p>
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
        <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-3">
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
