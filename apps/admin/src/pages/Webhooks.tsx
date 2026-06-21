import { useState, useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import type { Webhook, WebhookDelivery } from '../types'
import { Zap, Trash2, X, Eye, EyeOff } from 'lucide-react'

const ALL_EVENTS = [
  'memory.created',
  'memory.updated',
  'memory.deleted',
  'convention.created',
  'convention.updated',
  'user.invited',
]

function WebhookIcon() {
  return (
    <svg
      className="w-10 h-10 text-text-quaternary"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeWidth={1.5}
      aria-hidden="true"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244"
      />
    </svg>
  )
}

function relativeTime(iso: string): string {
  const diff = Math.floor((Date.now() - new Date(iso).getTime()) / 1000)
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

interface CreateWebhookModalProps {
  onClose: () => void
  onCreated: () => void
}

function CreateWebhookModal({ onClose, onCreated }: CreateWebhookModalProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const [url, setUrl] = useState('')
  const [selectedEvents, setSelectedEvents] = useState<Set<string>>(new Set(ALL_EVENTS))
  const [secret, setSecret] = useState('')
  const [showSecret, setShowSecret] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  const toggleEvent = (event: string) => {
    setSelectedEvents(prev => {
      const next = new Set(prev)
      if (next.has(event)) next.delete(event)
      else next.add(event)
      return next
    })
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!url.trim()) { setError('URL is required'); return }
    setError(null)
    setSubmitting(true)
    try {
      await client.createWebhook({
        name: url.trim(),
        target_url: url.trim(),
        events: Array.from(selectedEvents),
        secret: secret.trim() || undefined,
      })
      onCreated()
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create webhook')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div
      className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-[#1d1d1f] rounded-[18px] border border-border-primary p-6 w-full max-w-md"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h2 className="text-sm font-semibold text-text-primary">Add webhook</h2>
          <button
            onClick={onClose}
            className="text-text-quaternary hover:text-text-secondary transition-colors"
            aria-label="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-xs text-text-secondary">Endpoint URL</label>
            <input
              type="url"
              value={url}
              onChange={e => setUrl(e.target.value)}
              placeholder="https://example.com/webhook"
              className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary"
            />
          </div>

          <div className="space-y-2">
            <label className="text-xs text-text-secondary">Events</label>
            <div className="space-y-1.5">
              {ALL_EVENTS.map(event => (
                <label key={event} className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={selectedEvents.has(event)}
                    onChange={() => toggleEvent(event)}
                    className="accent-accent-blue w-3.5 h-3.5 rounded border-border-primary"
                  />
                  <span className="text-xs text-text-secondary">{event}</span>
                </label>
              ))}
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs text-text-secondary">
              Secret <span className="text-text-quaternary">(optional)</span>
            </label>
            <div className="relative">
              <input
                type={showSecret ? 'text' : 'password'}
                value={secret}
                onChange={e => setSecret(e.target.value)}
                placeholder="Signing secret"
                className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-3 py-2 pr-8 focus:outline-none focus:border-accent-blue/60 placeholder:text-text-quaternary"
              />
              <button
                type="button"
                onClick={() => setShowSecret(s => !s)}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-text-quaternary hover:text-text-secondary transition-colors"
                aria-label={showSecret ? 'Hide secret' : 'Show secret'}
              >
                {showSecret ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              </button>
            </div>
          </div>

          {error && (
            <p className="text-xs text-status-error">{error}</p>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="text-xs px-4 py-2 rounded-[8px] border border-border-primary text-text-secondary hover:text-text-primary transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="text-xs px-4 py-2 rounded-[8px] bg-accent-blue text-white hover:bg-accent-blue/90 transition-colors disabled:opacity-50"
            >
              {submitting ? 'Adding…' : 'Add webhook'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

interface DeliveryLogProps {
  webhook: Webhook
}

function DeliveryLog({ webhook }: DeliveryLogProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])

  const { data, isLoading } = useQuery({
    queryKey: ['webhook-deliveries', webhook.id],
    queryFn: () => client.listWebhookDeliveries(webhook.id, 20),
    refetchInterval: 30000,
  })

  const deliveries = data?.deliveries ?? []

  return (
    <div className="space-y-2">
      {isLoading && (
        <div className="space-y-2">
          {[1, 2, 3].map(i => (
            <div key={i} className="animate-pulse h-10 bg-white/[0.03] rounded-[8px]" />
          ))}
        </div>
      )}
      {!isLoading && deliveries.length === 0 && (
        <p className="text-xs text-text-quaternary text-center py-6">No deliveries yet</p>
      )}
      {deliveries.map(d => (
        <DeliveryRow key={d.id} delivery={d} />
      ))}
    </div>
  )
}

function DeliveryRow({ delivery }: { delivery: WebhookDelivery }) {
  const [expanded, setExpanded] = useState(false)
  return (
    <div className="rounded-[8px] border border-border-primary bg-white/[0.04] overflow-hidden">
      <button
        onClick={() => setExpanded(e => !e)}
        className="w-full flex items-center gap-3 px-3 py-2 text-left hover:bg-white/[0.02] transition-colors"
      >
        <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${delivery.success ? 'bg-status-success' : 'bg-status-error'}`} />
        <span className="text-xs text-text-secondary flex-1 truncate">{delivery.event_type}</span>
        {delivery.status_code && (
          <span className={`text-[10px] rounded-[5px] px-1.5 py-0.5 ${delivery.success ? 'bg-status-success/10 text-status-success' : 'bg-status-error/10 text-status-error'}`}>
            {delivery.status_code}
          </span>
        )}
        <span className="text-[10px] text-text-quaternary flex-shrink-0">{relativeTime(delivery.delivered_at)}</span>
      </button>
      {expanded && (
        <div className="px-3 pb-2 border-t border-border-primary/50">
          {delivery.error && (
            <p className="text-[10px] text-status-error mt-1.5">{delivery.error}</p>
          )}
          <pre className="text-[10px] text-text-quaternary mt-1.5 overflow-x-auto whitespace-pre-wrap break-all">
            {(() => { try { return JSON.stringify(JSON.parse(delivery.payload), null, 2) } catch { return delivery.payload } })()}
          </pre>
        </div>
      )}
    </div>
  )
}

export default function Webhooks() {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const qc = useQueryClient()
  const [showCreate, setShowCreate] = useState(false)
  const [selectedWebhookId, setSelectedWebhookId] = useState<string | null>(null)

  const { data, isLoading } = useQuery({
    queryKey: ['webhooks'],
    queryFn: () => client.listWebhooks(),
  })

  const webhooks = data?.webhooks ?? []

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteWebhook(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ['webhooks'] })
      if (selectedWebhookId === id) setSelectedWebhookId(null)
    },
  })

  const testWebhookMut = useMutation({
    mutationFn: (id: string) => client.testWebhook(id),
  })

  const handleDelete = (webhook: Webhook) => {
    if (!window.confirm(`Delete webhook "${webhook.target_url}"? This cannot be undone.`)) return
    deleteMut.mutate(webhook.id)
  }

  const selectedWebhook = webhooks.find(w => w.id === selectedWebhookId) ?? null

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div>
        <h1 className="text-[21px] font-semibold tracking-[0.231px] text-text-primary">Webhooks</h1>
        <p className="mt-1 text-[14px] text-text-tertiary tracking-[-0.224px]">
          Receive real-time HTTP notifications when events occur in your organization.
        </p>
      </div>

      <div className="flex gap-5">
        {/* Webhook list */}
        <div className="flex-1 bg-[#272729] rounded-[18px] border border-border-primary p-5">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-sm font-semibold text-text-primary">Webhooks</h2>
            <button
              onClick={() => setShowCreate(true)}
              className="text-xs px-3 py-1.5 rounded-full border border-border-primary text-text-secondary hover:text-text-primary hover:border-accent-blue/40 transition-colors"
            >
              Add webhook
            </button>
          </div>

          {isLoading && (
            <div className="space-y-3">
              {[1, 2, 3].map(i => (
                <div key={i} className="animate-pulse h-12 bg-white/[0.04] rounded-[8px]" />
              ))}
            </div>
          )}

          {!isLoading && webhooks.length === 0 && (
            <div className="flex flex-col items-center gap-3 py-12">
              <WebhookIcon />
              <p className="text-sm font-semibold text-text-tertiary">No webhooks configured</p>
              <p className="text-xs text-text-quaternary">Add a webhook to receive event notifications.</p>
              <button
                onClick={() => setShowCreate(true)}
                className="text-xs px-3 py-1.5 rounded-full border border-border-primary text-text-secondary hover:text-text-primary transition-colors"
              >
                Add webhook
              </button>
            </div>
          )}

          {webhooks.map(webhook => (
            <div
              key={webhook.id}
              onClick={() => setSelectedWebhookId(webhook.id === selectedWebhookId ? null : webhook.id)}
              className={`group flex items-center gap-3 px-3 py-2.5 rounded-[8px] cursor-pointer transition-colors ${
                selectedWebhookId === webhook.id
                  ? 'bg-accent-blue/10 border border-accent-blue/20'
                  : 'hover:bg-white/[0.04] border border-transparent'
              }`}
            >
              <span
                className={`w-2 h-2 rounded-full flex-shrink-0 ${webhook.active ? 'bg-status-success' : 'bg-status-error'}`}
                title={webhook.active ? 'Active' : 'Inactive'}
              />
              <span className="text-xs font-mono text-text-secondary truncate max-w-[280px] flex-1">
                {webhook.target_url}
              </span>
              <span className="rounded-[5px] bg-white/[0.06] px-2 py-0.5 text-[10px] text-text-secondary flex-shrink-0">
                {webhook.events.length} event{webhook.events.length !== 1 ? 's' : ''}
              </span>
              <span className="text-[10px] text-text-quaternary flex-shrink-0">
                {new Date(webhook.created_at).toLocaleDateString()}
              </span>
              <button
                onClick={e => { e.stopPropagation(); testWebhookMut.mutate(webhook.id) }}
                disabled={testWebhookMut.isPending && testWebhookMut.variables === webhook.id}
                className="opacity-0 group-hover:opacity-100 border border-border-primary rounded-full px-2.5 py-1 text-[10px] text-text-quaternary hover:text-text-primary transition-colors disabled:opacity-40 flex items-center gap-1"
              >
                <Zap className="w-3 h-3" />
                {testWebhookMut.isPending && testWebhookMut.variables === webhook.id ? 'Sending…' : 'Test'}
              </button>
              <button
                onClick={e => { e.stopPropagation(); handleDelete(webhook) }}
                disabled={deleteMut.isPending}
                className="ml-1 opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-status-error transition-all"
                aria-label="Delete webhook"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            </div>
          ))}
        </div>

        {/* Event log */}
        <div className="w-80 bg-[#272729] rounded-[18px] border border-border-primary p-5 flex flex-col">
          <h2 className="text-sm font-semibold text-text-primary mb-4">Event log</h2>
          {!selectedWebhook ? (
            <div className="flex flex-col items-center justify-center flex-1 gap-2 py-8">
              <Zap className="w-8 h-8 text-text-quaternary" />
              <p className="text-xs text-text-quaternary text-center">Select a webhook to view recent deliveries</p>
            </div>
          ) : (
            <div className="flex-1 overflow-y-auto">
              <p className="text-[10px] text-text-quaternary mb-3 truncate">{selectedWebhook.target_url}</p>
              <DeliveryLog webhook={selectedWebhook} />
            </div>
          )}
        </div>
      </div>

      {showCreate && (
        <CreateWebhookModal
          onClose={() => setShowCreate(false)}
          onCreated={() => qc.invalidateQueries({ queryKey: ['webhooks'] })}
        />
      )}
    </div>
  )
}
