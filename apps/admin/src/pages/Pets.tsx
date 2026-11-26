import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import {
  ChevronDown,
  ChevronRight,
  Cpu,
  Laptop,
  MessageSquare,
  RefreshCw,
  ScrollText,
  ShieldAlert,
  Sparkles,
} from 'lucide-react'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { PageHeader, PANEL_CLASS } from './factory/govern/ui'

interface MascotPet {
  id: string
  org_id: string
  user_id: string
  user_name?: string | null
  device_id: string
  pet_name: string
  hostname: string
  os_name: string
  os_version: string
  arch: string
  autonomy_level: string
  decision_mode: string
  visual_state: string
  skills_count: number
  mcp_tools_count: number
  last_seen_at: string
  created_at: string
  conversations_count: number
  events_count: number
  recent_outcomes: string[]
}

interface PetConversation {
  id: string
  pet_id: string
  pet_name: string
  hostname: string
  os_name: string
  run_id: string
  objective: string
  status: string
  route?: string | null
  steps_executed: number
  tool_errors: number
  transcript_json: string
  final_reply?: string | null
  started_at: string
  updated_at: string
}

interface PetEvent {
  id: string
  pet_id: string
  pet_name: string
  hostname: string
  os_name: string
  run_id?: string | null
  event_kind: string
  action_kind?: string | null
  decision_source?: string | null
  risk?: number | null
  outcome?: string | null
  summary: string
  detail_json: string
  created_at: string
}

const API_BASE = (import.meta.env.VITE_API_URL ?? '').replace(/\/+$/, '')

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { credentials: 'include' })
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }))
    throw new Error(body.error ?? res.statusText)
  }
  return res.json()
}

function isOnline(lastSeen: string): boolean {
  const ts = Date.parse(lastSeen)
  if (Number.isNaN(ts)) return false
  return Date.now() - ts < 180_000
}

function formatTime(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return d.toLocaleString()
}

function outcomeColor(outcome?: string | null): string {
  switch (outcome) {
    case 'success':
      return 'bg-emerald-400'
    case 'held':
    case 'warning':
      return 'bg-amber-400'
    case 'error':
    case 'blocked':
      return 'bg-rose-400'
    case 'cancelled':
      return 'bg-zinc-500'
    default:
      return 'bg-foreground/10'
  }
}

const AUTONOMY_LABEL: Record<string, string> = {
  observer: '0 · Observador',
  copilot: '1 · Copiloto',
  supervised_autonomous: '2 · Autónomo supervisado',
  full_autonomous: '3 · Autónomo completo',
}

const DECISION_LABEL: Record<string, string> = {
  hybrid: 'Híbrido (Laya → Jev)',
  laya_local: 'Laya Local',
  jev_cloud: 'Jev Cloud',
  deterministic_only: 'Solo reglas',
}

export default function Pets() {
  const [selectedPetId, setSelectedPetId] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'conversations' | 'logs'>('logs')
  const [eventFilter, setEventFilter] = useState<string>('all')
  const [expandedConvId, setExpandedConvId] = useState<string | null>(null)
  const [expandedEventId, setExpandedEventId] = useState<string | null>(null)

  const petsQuery = useQuery({
    queryKey: ['mascot-pets'],
    queryFn: () => fetchJson<{ pets: MascotPet[] }>('/v1/pets'),
    refetchInterval: 5000,
  })

  const convsQuery = useQuery({
    queryKey: ['mascot-pet-conversations', selectedPetId],
    queryFn: () => {
      const qs = selectedPetId ? `?pet_id=${encodeURIComponent(selectedPetId)}&limit=80` : '?limit=80'
      return fetchJson<{ conversations: PetConversation[] }>(`/v1/pets/conversations${qs}`)
    },
    refetchInterval: 5000,
  })

  const eventsQuery = useQuery({
    queryKey: ['mascot-pet-events', selectedPetId, eventFilter],
    queryFn: () => {
      const params = new URLSearchParams({ limit: '150' })
      if (selectedPetId) params.set('pet_id', selectedPetId)
      if (eventFilter !== 'all') params.set('event_kind', eventFilter)
      return fetchJson<{ events: PetEvent[] }>(`/v1/pets/events?${params.toString()}`)
    },
    refetchInterval: 5000,
  })

  const pets = petsQuery.data?.pets ?? []
  const conversations = convsQuery.data?.conversations ?? []
  const events = eventsQuery.data?.events ?? []

  const stats = useMemo(() => {
    const online = pets.filter((p) => isOnline(p.last_seen_at)).length
    const totalConvs = pets.reduce((acc, p) => acc + p.conversations_count, 0)
    const totalEvents = pets.reduce((acc, p) => acc + p.events_count, 0)
    return { total: pets.length, online, totalConvs, totalEvents }
  }, [pets])

  const refreshAll = () => {
    void petsQuery.refetch()
    void convsQuery.refetch()
    void eventsQuery.refetch()
  }

  return (
    <div className="p-6 max-w-[1280px] mx-auto space-y-6">
      <PageHeader
        title="Mascotas (Pets)"
        subtitle="Flota de mascotas conectadas por PC, conversaciones completas y auditoría en vivo."
        action={
          <Button variant="secondary" onClick={refreshAll}>
            <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
            Actualizar
          </Button>
        }
      />

      {/* Fleet summary tiles */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="rounded-[18px] border border-border-primary bg-foreground/[0.03] p-5">
          <div className="text-[28px] font-semibold tabular-nums leading-tight">{stats.total}</div>
          <div className="text-xs text-text-tertiary mt-1">Mascotas registradas</div>
        </div>
        <div className="rounded-[18px] border border-border-primary bg-foreground/[0.03] p-5">
          <div className="text-[28px] font-semibold tabular-nums leading-tight">{stats.online}</div>
          <div className="text-xs text-text-tertiary mt-1">Activas ahora</div>
        </div>
        <div className="rounded-[18px] border border-border-primary bg-foreground/[0.03] p-5">
          <div className="text-[28px] font-semibold tabular-nums leading-tight">{stats.totalConvs}</div>
          <div className="text-xs text-text-tertiary mt-1">Conversaciones auditadas</div>
        </div>
        <div className="rounded-[18px] border border-border-primary bg-foreground/[0.03] p-5">
          <div className="text-[28px] font-semibold tabular-nums leading-tight">{stats.totalEvents}</div>
          <div className="text-xs text-text-tertiary mt-1">Eventos registrados</div>
        </div>
      </div>

      {/* Connected Pets / PCs Fleet */}
      {pets.length === 0 ? (
        <div className={PANEL_CLASS}>
          <EmptyState
            icon={<Sparkles className="w-6 h-6" />}
            title="Aún no hay mascotas conectadas"
            description="Instala la mascota en tu Mac o PC, abre Configuración → API Keys y conéctala con tu URL de NexusMind y tu API key."
          />
        </div>
      ) : (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h2 className="text-[15px] font-semibold">PCs y Mascotas Conectadas</h2>
            {selectedPetId && (
              <Button variant="ghost" size="sm" onClick={() => setSelectedPetId(null)}>
                Ver todas las mascotas
              </Button>
            )}
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {pets.map((pet) => {
              const online = isOnline(pet.last_seen_at)
              const selected = selectedPetId === pet.id
              return (
                <button
                  key={pet.id}
                  type="button"
                  onClick={() => setSelectedPetId(selected ? null : pet.id)}
                  className={`text-left rounded-[18px] border p-5 transition-colors ${
                    selected
                      ? 'border-accent-blue bg-foreground/[0.06]'
                      : 'border-border-primary bg-foreground/[0.03] hover:bg-foreground/[0.05]'
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2.5">
                      <span
                        className={`w-2.5 h-2.5 rounded-full ${
                          online ? 'bg-emerald-400' : 'bg-zinc-500'
                        }`}
                      />
                      <div>
                        <div className="text-[15px] font-semibold">{pet.pet_name}</div>
                        <div className="text-xs text-text-secondary flex items-center gap-1.5 mt-0.5">
                          <Laptop className="w-3.5 h-3.5" />
                          <span>{pet.hostname}</span>
                        </div>
                      </div>
                    </div>
                    <Badge variant={online ? 'success' : 'default'}>
                      {online ? 'Online' : 'Offline'}
                    </Badge>
                  </div>

                  <div className="mt-3 flex flex-wrap gap-1.5 text-[11px] text-text-secondary">
                    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-foreground/[0.05] border border-border-secondary">
                      <Cpu className="w-3 h-3" />
                      {pet.os_name} {pet.os_version} ({pet.arch || 'n/a'})
                    </span>
                    <span className="px-2 py-0.5 rounded-full bg-foreground/[0.05] border border-border-secondary">
                      {DECISION_LABEL[pet.decision_mode] ?? pet.decision_mode}
                    </span>
                    <span className="px-2 py-0.5 rounded-full bg-foreground/[0.05] border border-border-secondary">
                      {AUTONOMY_LABEL[pet.autonomy_level] ?? pet.autonomy_level}
                    </span>
                  </div>

                  {/* Outcome Tape (last 14 actions) */}
                  <div className="mt-4">
                    <div className="flex items-center justify-between text-[11px] text-text-tertiary mb-1.5">
                      <span>Outcome tape</span>
                      <span>
                        {pet.conversations_count} chats · {pet.events_count} logs
                      </span>
                    </div>
                    <div className="flex items-center gap-1" aria-label="Últimas 14 acciones de la mascota">
                      {Array.from({ length: 14 }).map((_, idx) => {
                        const out = pet.recent_outcomes[idx]
                        return (
                          <span
                            key={idx}
                            className={`w-3.5 h-3.5 rounded-[3px] ${outcomeColor(out)}`}
                            title={out ?? 'Sin ejecución'}
                          />
                        )
                      })}
                    </div>
                  </div>

                  <div className="mt-3 pt-2.5 border-t border-border-secondary flex items-center justify-between text-[11px] text-text-tertiary">
                    <span>Usuario: {pet.user_name || pet.user_id}</span>
                    <span>Visto: {formatTime(pet.last_seen_at)}</span>
                  </div>
                </button>
              )
            })}
          </div>
        </div>
      )}

      {/* Audit Tabs: Full Logs + Conversations */}
      <div className="rounded-[18px] border border-border-primary bg-foreground/[0.02] overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-3.5 border-b border-border-secondary">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setActiveTab('logs')}
              className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium transition-colors ${
                activeTab === 'logs'
                  ? 'bg-accent-blue text-white'
                  : 'text-text-secondary hover:bg-foreground/[0.06]'
              }`}
            >
              <ScrollText className="w-3.5 h-3.5" />
              Logs de Auditoría ({events.length})
            </button>
            <button
              type="button"
              onClick={() => setActiveTab('conversations')}
              className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium transition-colors ${
                activeTab === 'conversations'
                  ? 'bg-accent-blue text-white'
                  : 'text-text-secondary hover:bg-foreground/[0.06]'
              }`}
            >
              <MessageSquare className="w-3.5 h-3.5" />
              Conversaciones ({conversations.length})
            </button>
          </div>

          {activeTab === 'logs' && (
            <div className="flex items-center gap-1.5 text-xs">
              {['all', 'prompt', 'action', 'approval', 'decision', 'learning', 'blocked', 'kill_switch'].map((k) => (
                <button
                  key={k}
                  type="button"
                  onClick={() => setEventFilter(k)}
                  className={`px-2.5 py-1 rounded-full text-[11px] font-medium ${
                    eventFilter === k
                      ? 'bg-foreground/15 text-text-primary'
                      : 'text-text-tertiary hover:text-text-secondary'
                  }`}
                >
                  {k === 'all' ? 'Todos' : k}
                </button>
              ))}
            </div>
          )}
        </div>

        {activeTab === 'logs' ? (
          events.length === 0 ? (
            <div className="p-8 text-center text-sm text-text-tertiary">
              Sin eventos de auditoría todavía para este filtro.
            </div>
          ) : (
            <div className="divide-y divide-border-secondary">
              {events.map((ev) => {
                const expanded = expandedEventId === ev.id
                return (
                  <div key={ev.id} className="px-5 py-3 hover:bg-foreground/[0.02]">
                    <button
                      type="button"
                      onClick={() => setExpandedEventId(expanded ? null : ev.id)}
                      className="w-full text-left flex items-start gap-3"
                    >
                      <span className="mt-1 text-text-tertiary">
                        {expanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
                      </span>
                      <div className="flex-1 min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="text-[13px] font-medium text-text-primary">{ev.summary}</span>
                        </div>
                        <div className="flex flex-wrap items-center gap-2 mt-1 text-[11px] text-text-tertiary">
                          <span className="font-medium text-text-secondary">
                            {ev.pet_name} · {ev.hostname} ({ev.os_name})
                          </span>
                          <Badge variant={ev.outcome === 'error' || ev.outcome === 'blocked' ? 'error' : ev.outcome === 'held' ? 'warning' : 'default'}>
                            {ev.event_kind}
                          </Badge>
                          {ev.decision_source && <span>Motor: {ev.decision_source}</span>}
                          {typeof ev.risk === 'number' && <span>Riesgo: {Math.round(ev.risk * 100)}%</span>}
                          {ev.outcome && <span>Resultado: {ev.outcome}</span>}
                          <span>{formatTime(ev.created_at)}</span>
                        </div>
                      </div>
                    </button>
                    {expanded && (
                      <pre className="mt-2.5 ml-7 p-3 rounded-[11px] bg-black/40 border border-border-secondary text-[11px] font-mono text-text-secondary overflow-x-auto">
                        {prettyJson(ev.detail_json)}
                      </pre>
                    )}
                  </div>
                )
              })}
            </div>
          )
        ) : conversations.length === 0 ? (
          <div className="p-8 text-center text-sm text-text-tertiary">
            Sin conversaciones registradas todavía.
          </div>
        ) : (
          <div className="divide-y divide-border-secondary">
            {conversations.map((conv) => {
              const expanded = expandedConvId === conv.id
              const messages = parseTranscript(conv.transcript_json)
              return (
                <div key={conv.id} className="px-5 py-4 hover:bg-foreground/[0.02]">
                  <button
                    type="button"
                    onClick={() => setExpandedConvId(expanded ? null : conv.id)}
                    className="w-full text-left flex items-start gap-3"
                  >
                    <span className="mt-1 text-text-tertiary">
                      {expanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
                    </span>
                    <div className="flex-1 min-w-0">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <span className="text-[14px] font-semibold text-text-primary">{conv.objective}</span>
                        <Badge
                          variant={
                            conv.status === 'completed'
                              ? 'success'
                              : conv.status === 'waiting_approval'
                                ? 'warning'
                                : 'error'
                          }
                        >
                          {conv.status}
                        </Badge>
                      </div>
                      {conv.final_reply && (
                        <p className="text-[13px] text-text-secondary mt-1 line-clamp-2">{conv.final_reply}</p>
                      )}
                      <div className="flex flex-wrap items-center gap-3 mt-1.5 text-[11px] text-text-tertiary">
                        <span className="font-medium text-text-secondary">
                          {conv.pet_name} · {conv.hostname} ({conv.os_name})
                        </span>
                        <span>{conv.steps_executed} pasos</span>
                        {conv.tool_errors > 0 && (
                          <span className="inline-flex items-center gap-1 text-rose-400">
                            <ShieldAlert className="w-3 h-3" /> {conv.tool_errors} errores
                          </span>
                        )}
                        {conv.route && <span>Ruta: {conv.route}</span>}
                        <span>{formatTime(conv.updated_at)}</span>
                      </div>
                    </div>
                  </button>

                  {expanded && (
                    <div className="mt-3 ml-7 space-y-2">
                      {messages.length === 0 ? (
                        <pre className="p-3 rounded-[11px] bg-black/40 border border-border-secondary text-[11px] font-mono text-text-secondary overflow-x-auto">
                          {prettyJson(conv.transcript_json)}
                        </pre>
                      ) : (
                        messages.map((m, idx) => (
                          <div
                            key={idx}
                            className="p-3 rounded-[11px] bg-black/30 border border-border-secondary text-xs space-y-1"
                          >
                            <div className="text-[11px] font-semibold uppercase tracking-wide text-text-tertiary">
                              {m.role}
                              {m.tool_name ? ` · ${m.tool_name}` : ''}
                            </div>
                            {m.text && <div className="text-text-primary whitespace-pre-wrap">{m.text}</div>}
                            {m.content && (
                              <pre className="text-[11px] font-mono text-text-secondary whitespace-pre-wrap overflow-x-auto">
                                {m.content}
                              </pre>
                            )}
                            {Array.isArray(m.tool_calls) && m.tool_calls.length > 0 && (
                              <pre className="text-[11px] font-mono text-amber-300/90 overflow-x-auto">
                                {JSON.stringify(m.tool_calls, null, 2)}
                              </pre>
                            )}
                          </div>
                        ))
                      )}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2)
  } catch {
    return raw
  }
}

interface TranscriptMessage {
  role: string
  text?: string
  content?: string
  tool_name?: string
  tool_calls?: unknown[]
}

function parseTranscript(raw: string): TranscriptMessage[] {
  try {
    const parsed = JSON.parse(raw)
    if (Array.isArray(parsed)) return parsed
    if (parsed && Array.isArray(parsed.messages)) return parsed.messages
    return []
  } catch {
    return []
  }
}
