import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Button } from '../../../components/ui/Button'
import type { AutonomousAgentEvent } from '../../../types'
import { TranscriptView } from '../runs/Transcript'
import { asDict, asNum, asStr } from '../shared/format'
import type { WorkflowEvent, WorkflowEventPage, WorkflowEventQuery, WorkflowRun } from './types'

export const LOG_KINDS = new Set(['log.chunk', 'service.log', 'dependency.log'])
const POLL_MS = 3000
const MAX_LINES = 5000

type Status = 'loading' | 'ready' | 'forbidden' | 'offline' | 'error'
type Line = { key: string; seq: number; at: string; source: string; severity: string; text: string; control: boolean }

/** Terminal output is untrusted data: drop escape sequences and control bytes. */
export function sanitize(text: string): string {
  return text
    .replace(/\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)/g, '')
    .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, '')
    .replace(/\u001b[@-_]/g, '')
    .replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, '')
}

const utc = (at: string) => `${at} UTC`
const str = (v: unknown) => (v == null ? '' : String(v))

function describeControl(e: WorkflowEvent): string {
  const p = e.payload
  const data = asDict(p.data) ?? {}
  switch (e.kind) {
    case 'command.finished': {
      const code = data.exit_code
      const parts = [code == null ? (data.timed_out ? 'tiempo agotado' : 'interrumpido') : `exit ${str(code)}`]
      if (asNum(data.duration_ms) != null) parts.push(`${((data.duration_ms as number) / 1000).toFixed(1)} s`)
      if (data.hint === 'killed_or_out_of_memory') parts.push('proceso terminado (posible falta de memoria)')
      if (data.output_exceeded) parts.push('límite de salida superado')
      return `Proceso terminado: ${parts.join(' · ')}`
    }
    case 'backend.rejected':
      return `El backend rechazó ${str(p.route)} con ${str(p.status)} ${str(p.code)}${p.request_id ? ` (request ${str(p.request_id)})` : ''}`
    case 'runner.connectivity':
      return `${str(p.message)}${asStr(data.last_code) ? ` · último error ${str(data.last_code)}` : ''}`
    case 'runner.diagnostic':
      return `Diagnóstico del runner: ${str(p.message)}${data.stage ? ` en ${str(data.stage)}` : ''}`
    case 'infra.incident':
      return `Incidente de infraestructura coincidente: ${str(p.message)}${asStr(data.message) ? ` — ${str(data.message)}` : ''}`
    case 'capture.truncated':
      return `Se alcanzó el límite de ${p.class === 'logs' ? 'salida' : 'eventos de control'} (${str(p.limit)}); lo posterior no se registró aquí`
    case 'capture.pruned':
      return `Retención: se eliminaron ${str(p.deleted)} fragmentos de salida tras ${str(p.retention_days)} días`
    default:
      return `${e.kind}${p.message ? `: ${str(p.message)}` : ''}`
  }
}

function toLines(events: WorkflowEvent[], withControl: boolean): Line[] {
  const lines: Line[] = []
  for (const e of events) {
    const p = e.payload
    const source = asStr(p.source) ?? (LOG_KINDS.has(e.kind) ? 'runner' : 'control')
    const severity = asStr(p.severity) ?? 'info'
    if (LOG_KINDS.has(e.kind)) {
      const parts = sanitize(asStr(p.message) ?? '').split('\n')
      if (parts[parts.length - 1] === '') parts.pop()
      parts.forEach((text, i) => lines.push({ key: `${e.seq}-${i}`, seq: e.seq, at: e.created_at, source, severity, text, control: false }))
    } else if (withControl) {
      lines.push({ key: `${e.seq}`, seq: e.seq, at: e.created_at, source, severity, text: sanitize(describeControl(e)), control: true })
    }
  }
  return lines
}

/** Agent transcripts arrive as JSON lines in the streamed output (Claude stream-json, Pi JSONL). */
export function transcriptTurns(events: WorkflowEvent[]): AutonomousAgentEvent[] {
  const turns: AutonomousAgentEvent[] = []
  for (const e of events) {
    if (!LOG_KINDS.has(e.kind)) continue
    for (const raw of (asStr(e.payload.message) ?? '').split('\n')) {
      const line = raw.trim()
      if (!line.startsWith('{')) continue
      try {
        const payload = asDict(JSON.parse(line))
        if (payload) turns.push({ sequence: turns.length + 1, kind: asStr(payload.type) ?? 'raw', payload, created_at: e.created_at })
      } catch { /* a non-JSON line is console output, not conversation */ }
    }
  }
  return turns
}

function failureKind(error: unknown): Status {
  const status = (error as { status?: number })?.status
  if (status === 403 || status === 401) return 'forbidden'
  if (status == null) return 'offline'
  return 'error'
}

export function useStepHistory(
  load: (q: WorkflowEventQuery) => Promise<WorkflowEventPage>,
  step: string,
  attempt: number,
  live: boolean,
) {
  const [events, setEvents] = useState<WorkflowEvent[]>([])
  const [page, setPage] = useState<Omit<WorkflowEventPage, 'events'> | null>(null)
  const [status, setStatus] = useState<Status>('loading')
  const last = useRef(0)
  const first = useRef<number | null>(null)
  const merge = useCallback((incoming: WorkflowEvent[]) => {
    setEvents((current) => {
      const seen = new Set(current.map((e) => e.seq))
      return [...current, ...incoming.filter((e) => !seen.has(e.seq))].sort((a, b) => a.seq - b.seq)
    })
    for (const e of incoming) {
      last.current = Math.max(last.current, e.seq)
      first.current = first.current == null ? e.seq : Math.min(first.current, e.seq)
    }
  }, [])
  useEffect(() => {
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    last.current = 0
    first.current = null
    setEvents([])
    setPage(null)
    setStatus('loading')
    const poll = async (initial: boolean) => {
      try {
        let result = await load(initial ? { step, attempt, limit: 300 } : { step, attempt, after: last.current, limit: 500 })
        if (cancelled) return
        merge(result.events)
        while (!initial && result.has_newer && !cancelled) {
          result = await load({ step, attempt, after: last.current, limit: 500 })
          merge(result.events)
        }
        if (cancelled) return
        const { events: _ignored, ...rest } = result
        setPage((previous) => (initial ? rest : { ...rest, has_older: previous?.has_older ?? rest.has_older }))
        setStatus('ready')
      } catch (error) {
        if (cancelled) return
        setStatus(failureKind(error))
      }
      if (!cancelled && live) timer = setTimeout(() => void poll(false), POLL_MS)
    }
    void poll(true)
    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
    }
  }, [load, step, attempt, live, merge])
  const older = useCallback(async () => {
    if (first.current == null) return
    try {
      const result = await load({ step, attempt, before: first.current, limit: 300 })
      merge(result.events)
      setPage((previous) => (previous ? { ...previous, has_older: result.has_older } : previous))
    } catch (error) {
      setStatus(failureKind(error))
    }
  }, [load, step, attempt, merge])
  return { events, page, status, older }
}

async function download(load: (q: WorkflowEventQuery) => Promise<WorkflowEventPage>, runId: string, step: string, attempt: number) {
  const all: WorkflowEvent[] = []
  let after = 0
  for (;;) {
    const result = await load({ step, attempt, after, limit: 500 })
    all.push(...result.events)
    if (!result.has_newer || !result.events.length) break
    after = result.events[result.events.length - 1].seq
  }
  const header = `# run ${runId} · fase ${step} · intento ${attempt} · ${all.length} eventos (redactados en origen)\n`
  const body = toLines(all, true).map((l) => `${utc(l.at)} [${l.source}${l.control ? ':control' : ''}] ${l.text}`).join('\n')
  const url = URL.createObjectURL(new Blob([header, body, '\n'], { type: 'text/plain' }))
  const link = document.createElement('a')
  link.href = url
  link.download = `workflow-${runId}-${step}-intento-${attempt}.log`
  link.click()
  URL.revokeObjectURL(url)
}

function Coverage({ page }: { page: Omit<WorkflowEventPage, 'events'> | null }) {
  const c = page?.coverage
  if (!c) return null
  if (c.complete) return <p className="text-xs text-muted-foreground">Registro completo: sin pérdidas ni truncado.</p>
  const reasons = [
    c.truncated.length ? `límite alcanzado (${c.truncated.map((t) => (t === 'logs' ? 'salida' : 'control')).join(', ')})` : '',
    c.dropped_events ? `${c.dropped_events} eventos no registrados por caída del backend` : '',
    c.gap_bytes ? `${c.gap_bytes} bytes de salida sin registrar` : '',
    c.pruned_events ? `${c.pruned_events} fragmentos eliminados por retención` : '',
  ].filter(Boolean)
  return (
    <p role="note" className="text-xs text-status-warning">
      Registro incompleto: {reasons.join(' · ')}. Los logs completos de la fase están en sus artefactos.
    </p>
  )
}

function Summary({ state, events, page, live }: {
  state?: WorkflowRun['steps'][number]
  events: WorkflowEvent[]
  page: Omit<WorkflowEventPage, 'events'> | null
  live: boolean
}) {
  const output = asDict(asDict(state?.result)?.output) ?? {}
  const finished = [...events].reverse().find((e) => e.kind === 'command.finished')
  const incidents = events.filter((e) => ['backend.rejected', 'runner.connectivity', 'infra.incident', 'runner.diagnostic', 'capture.truncated', 'capture.pruned'].includes(e.kind))
  const context = page?.context
  const restarted = context && context.backend_started_at > context.run_created_at
  const lastSeen = context?.runner?.last_seen
  const stale = live && lastSeen != null && Date.now() - Date.parse(`${lastSeen.replace(' ', 'T')}Z`) > 90_000
  const facts: [string, unknown][] = [
    ['Etapa', output.stage],
    ['Código', output.code ?? output.backend_code],
    ['Llamada', output.request],
    ['HTTP', output.http_status],
    ['Request ID', output.request_id],
    ['Reintentar es seguro', output.retryable == null ? undefined : output.retryable ? 'sí' : 'no'],
  ]
  return (
    <div className="space-y-3 text-sm">
      {facts.some(([, v]) => v != null && v !== '') && (
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
          {facts.filter(([, v]) => v != null && v !== '').map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-muted-foreground">{k}</dt>
              <dd className="break-all font-mono text-xs">{str(v)}</dd>
            </div>
          ))}
        </dl>
      )}
      {finished && <p>{describeControl(finished)}</p>}
      {stale && <p role="alert">El runner no envía heartbeat desde {utc(lastSeen!)}: la fase no avanza aunque figure en ejecución.</p>}
      {restarted && (
        <p className="text-xs text-muted-foreground">
          El backend se reinició a las {utc(context!.backend_started_at)}, durante esta ejecución (coincidente, no necesariamente la causa).
        </p>
      )}
      {!!incidents.length && (
        <ul aria-label="Incidentes y diagnósticos" className="space-y-1 text-xs">
          {incidents.map((e) => (
            <li key={e.seq}><span className="text-muted-foreground">{utc(e.created_at)} · </span>{describeControl(e)}</li>
          ))}
        </ul>
      )}
      <Coverage page={page} />
    </div>
  )
}

function Console({ events, live, older, hasOlder }: { events: WorkflowEvent[]; live: boolean; older: () => void; hasOlder: boolean }) {
  const [withControl, setWithControl] = useState(true)
  const [source, setSource] = useState('')
  const [query, setQuery] = useState('')
  const [following, setFollowing] = useState(true)
  const box = useRef<HTMLDivElement>(null)
  const all = useMemo(() => toLines(events, withControl), [events, withControl])
  const sources = useMemo(() => [...new Set(all.map((l) => l.source))].sort(), [all])
  const shown = useMemo(() => {
    const needle = query.toLowerCase()
    return all.filter((l) => (!source || l.source === source) && (!needle || l.text.toLowerCase().includes(needle))).slice(-MAX_LINES)
  }, [all, source, query])
  useEffect(() => {
    if (following && box.current) box.current.scrollTop = box.current.scrollHeight
  }, [shown, following])
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-3 text-xs">
        <label className="flex items-center gap-1">Origen
          <select className="rounded border border-border bg-background px-1 py-0.5" value={source} onChange={(e) => setSource(e.target.value)}>
            <option value="">Todos</option>
            {sources.map((s) => <option key={s} value={s}>{s}</option>)}
          </select>
        </label>
        <input aria-label="Buscar en la consola" placeholder="Buscar…" className="rounded border border-border bg-background px-2 py-0.5" value={query} onChange={(e) => setQuery(e.target.value)} />
        <label className="flex items-center gap-1"><input type="checkbox" checked={withControl} onChange={(e) => setWithControl(e.target.checked)} />Eventos del runtime</label>
        {live && <label className="flex items-center gap-1"><input type="checkbox" checked={following} onChange={(e) => setFollowing(e.target.checked)} />Seguir en vivo</label>}
        {hasOlder && <button type="button" className="underline" onClick={() => { setFollowing(false); older() }}>Cargar anteriores</button>}
      </div>
      {shown.length ? (
        <div
          ref={box}
          role="log"
          aria-label="Consola de la fase"
          onScroll={(e) => {
            const el = e.currentTarget
            if (el.scrollHeight - el.scrollTop - el.clientHeight > 40) setFollowing(false)
          }}
          className="max-h-[480px] overflow-auto rounded-md bg-muted/60 p-2 font-mono text-[11px] leading-relaxed"
        >
          {shown.map((l) => (
            <div key={l.key} className={`whitespace-pre-wrap break-words ${l.control ? 'text-muted-foreground' : ''} ${l.severity === 'error' ? 'text-status-error' : ''}`}>
              {l.control && <span>▸ </span>}{l.text}
            </div>
          ))}
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">{all.length ? 'Ninguna línea coincide con el filtro.' : 'No hay salida registrada para este intento.'}</p>
      )}
    </div>
  )
}

export function StepObservability({ runId, stepId, agent, state, live, artifacts, openArtifact, load }: {
  runId: string
  stepId: string
  agent: boolean
  state?: WorkflowRun['steps'][number]
  live: boolean
  artifacts: NonNullable<WorkflowRun['artifacts']>
  openArtifact: (id: string) => void
  load: (q: WorkflowEventQuery) => Promise<WorkflowEventPage>
}) {
  const attempts = Math.max(state?.attempt ?? 0, 1)
  const [attempt, setAttempt] = useState(attempts)
  useEffect(() => setAttempt(attempts), [attempts])
  const [tab, setTab] = useState<'summary' | 'conversation' | 'console' | 'artifacts'>('summary')
  const executed = (state?.attempt ?? 0) > 0
  const running = live && state?.status === 'running' && attempt === attempts
  const { events, page, status, older } = useStepHistory(load, stepId, attempt, running)
  const turns = useMemo(() => (agent ? transcriptTurns(events) : []), [events, agent])
  const own = artifacts.filter((a) => a.step_id === stepId && a.attempt === attempt)
  if (!executed) return <p className="mt-3 text-sm text-muted-foreground">Fase no ejecutada todavía.</p>
  const tabs = [['summary', 'Resumen'], ['conversation', 'Conversación'], ['console', 'Consola'], ['artifacts', `Artefactos (${own.length})`]] as const
  return (
    <section aria-label={`Ejecución de ${stepId}`} className="mt-3 space-y-3 rounded-lg border border-border p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div role="tablist" className="flex flex-wrap gap-1">
          {tabs.map(([id, label]) => (
            <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => setTab(id)}
              className={`rounded-md px-2 py-1 text-xs ${tab === id ? 'bg-muted font-medium' : 'text-muted-foreground'}`}>{label}</button>
          ))}
        </div>
        <div className="flex items-center gap-2 text-xs">
          {attempts > 1 && (
            <label className="flex items-center gap-1">Intento
              <select className="rounded border border-border bg-background px-1 py-0.5" value={attempt} onChange={(e) => setAttempt(Number(e.target.value))}>
                {Array.from({ length: attempts }, (_, i) => i + 1).map((n) => <option key={n} value={n}>{n}</option>)}
              </select>
            </label>
          )}
          <Button size="sm" variant="ghost" onClick={() => void download(load, runId, stepId, attempt).catch(() => undefined)}>Descargar</Button>
        </div>
      </div>
      {status === 'loading' && <p role="status" className="text-sm text-muted-foreground">Cargando historial…</p>}
      {status === 'forbidden' && <p role="alert" className="text-sm">No tienes permiso para ver los logs de esta ejecución.</p>}
      {status === 'offline' && <p role="alert" className="text-sm">Conexión con el backend perdida; reintentando. Lo ya cargado sigue visible.</p>}
      {status === 'error' && <p role="alert" className="text-sm">No se pudo cargar el historial de la fase.</p>}
      {status !== 'loading' && status !== 'forbidden' && (
        <>
          {tab === 'summary' && <Summary state={state} events={events} page={page} live={running} />}
          {tab === 'conversation' && (agent
            ? (turns.length || running ? <TranscriptView turns={turns} live={running} /> : <p className="text-sm text-muted-foreground">No registrado en esta versión: esta fase no guardó la conversación en su historial. Revisa sus artefactos.</p>)
            : <p className="text-sm text-muted-foreground">Esta fase no ejecuta un agente.</p>)}
          {tab === 'console' && <Console events={events} live={running} older={() => void older()} hasOlder={!!page?.has_older} />}
          {tab === 'artifacts' && (own.length ? (
            <ul className="divide-y divide-border text-sm">
              {own.map((a) => (
                <li key={a.id} className="flex items-center justify-between gap-2 py-2">
                  <button type="button" className="text-left underline underline-offset-4" onClick={() => openArtifact(a.id)}>{a.name}</button>
                  <span className="text-xs text-muted-foreground">{Math.ceil(a.size / 1024)} KB</span>
                </li>
              ))}
            </ul>
          ) : <p className="text-sm text-muted-foreground">Este intento no tiene artefactos.</p>)}
        </>
      )}
    </section>
  )
}
