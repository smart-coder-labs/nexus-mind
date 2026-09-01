import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Navigate } from 'react-router-dom'
import { AlertTriangle, Archive, Ban, Bot, Camera, CheckCircle2, Clock, Copy, ExternalLink, GitPullRequest, Loader2, MessagesSquare, Pencil, Play, Plus, RefreshCw, ShieldCheck, Wrench, X, XCircle } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/ui/Button'
import { Input } from '../components/ui/Input'
import { Badge } from '../components/ui/Badge'
import { Switch } from '../components/ui/Switch'
import { SegmentedControl } from '../components/ui/SegmentedControl'
import { EmptyState } from '../components/ui/EmptyState'
import { Modal, ModalHeader, ModalTitle, ModalContent } from '../components/ui/Modal'
import { Markdown } from '../components/ui/Markdown'
import AutonomousAgentWizard from './AutonomousAgentWizard'
import type { AutonomousAgentDefinition, AutonomousAgentDetail, AutonomousAgentEvent, AutonomousAgentRun, AutonomousAgentTemplate } from '../types'

type Tab = 'agents' | 'templates' | 'runs' | 'findings' | 'runtime'

const TABS: { value: Tab; label: string }[] = [
  { value: 'agents', label: 'Agents' },
  { value: 'templates', label: 'Templates' },
  { value: 'runs', label: 'Runs' },
  { value: 'findings', label: 'Findings' },
  { value: 'runtime', label: 'Runtime' },
]

const statusVariant: Record<string, 'success' | 'default' | 'warning'> = { enabled: 'success', disabled: 'default', archived: 'warning' }

// ── Render helpers (pure; translate raw run/finding payloads into readable UI) ──
type Tone = 'ok' | 'warn' | 'bad' | 'info' | 'neutral'
type BV = 'default' | 'success' | 'warning' | 'error' | 'info' | 'purple' | 'primary'
type Dict = Record<string, unknown>

const asDict = (v: unknown): Dict | undefined => (v && typeof v === 'object' && !Array.isArray(v) ? (v as Dict) : undefined)
const asNum = (v: unknown): number | undefined => (typeof v === 'number' && Number.isFinite(v) ? v : undefined)
const asStr = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined)
const asArr = (v: unknown): unknown[] | undefined => (Array.isArray(v) ? v : undefined)
const titleCase = (s: string) => s.replace(/_/g, ' ')

// Mirror the backend's `structured_result`: a QA run's findings live inside the
// model's final message (`result.result`, a JSON string that may be wrapped in
// ```json fences or prose), not as a direct property of the result object.
const parseLenient = (text: string): Dict | undefined => {
  const unfenced = text.trim().replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '').trim()
  try { return asDict(JSON.parse(unfenced)) } catch { /* fall through */ }
  const start = unfenced.indexOf('{')
  const end = unfenced.lastIndexOf('}')
  if (start >= 0 && end > start) {
    try { return asDict(JSON.parse(unfenced.slice(start, end + 1))) } catch { /* give up */ }
  }
  return undefined
}

// Run status → readable label, Badge variant, and a color tone for accents.
const RUN_STATUS: Record<string, { label: string; variant: BV; tone: Tone }> = {
  queued: { label: 'Queued', variant: 'default', tone: 'neutral' },
  leased: { label: 'Leased', variant: 'info', tone: 'info' },
  running: { label: 'Running', variant: 'info', tone: 'info' },
  succeeded: { label: 'Succeeded', variant: 'success', tone: 'ok' },
  partial: { label: 'Partial', variant: 'warning', tone: 'warn' },
  budget_exhausted: { label: 'Budget exhausted', variant: 'warning', tone: 'warn' },
  blocked_runtime: { label: 'Blocked · runtime', variant: 'error', tone: 'bad' },
  blocked_policy: { label: 'Blocked · policy', variant: 'error', tone: 'bad' },
  failed: { label: 'Failed', variant: 'error', tone: 'bad' },
  cancelled: { label: 'Cancelled', variant: 'default', tone: 'neutral' },
}
const runStatusMeta = (s: string) => RUN_STATUS[s] ?? { label: titleCase(s), variant: 'default' as BV, tone: 'neutral' as Tone }

// run.finished result `code` → a plain-language clause completing "This run …".
const RESULT_CODE: Record<string, string> = {
  completed: 'completed cleanly',
  cost_limit_exceeded: 'stopped after reaching the cost limit',
  wall_time_exceeded: 'stopped after reaching the time limit',
  completed_nonzero_exit: 'finished with a non-zero exit code',
  claude_failed: 'failed inside Claude Code',
  cancelled_by_operator: 'was cancelled by an operator',
  sandbox_create_failed: 'could not create its sandbox',
  sandbox_environment_failed: 'failed to prepare its sandbox environment',
  claude_auth_required: 'needs Claude Code re-authentication',
  claude_runtime_unavailable: 'could not reach the Claude Code runtime',
  unsupported_template: 'used an unsupported template',
}

const usd = (n: number) => `$${n.toFixed(2)}`
const dur = (s: number) => { const m = Math.floor(s / 60); const sec = Math.round(s % 60); return m > 0 ? `${m}m ${sec}s` : `${sec}s` }
const round = (n: number) => String(Math.round(n))
const parseUtc = (iso?: string | null) => (iso ? Date.parse(`${iso}Z`) : NaN)
const fmtTime = (iso?: string | null) => (iso ? new Date(`${iso}Z`).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }) : '')

const toneDotClass: Record<Tone, string> = {
  ok: 'bg-status-success/15 border-status-success/40 text-status-success',
  warn: 'bg-status-warning/15 border-status-warning/40 text-status-warning',
  bad: 'bg-status-error/15 border-status-error/40 text-status-error',
  info: 'bg-accent-blue/15 border-accent-blue/40 text-accent-blue',
  neutral: 'bg-white/[0.06] border-border-primary text-text-tertiary',
}
const outcomeToneClass: Record<Tone, string> = {
  ok: 'border-status-success/25 bg-status-success/[0.06]',
  warn: 'border-status-warning/25 bg-status-warning/[0.06]',
  bad: 'border-status-error/25 bg-status-error/[0.06]',
  info: 'border-accent-blue/25 bg-accent-blue/[0.06]',
  neutral: 'border-border-primary bg-white/[0.02]',
}
const sevVariant = (s: string): BV => (s === 'critical' || s === 'high' ? 'error' : s === 'medium' ? 'warning' : s === 'low' ? 'info' : 'default')
const sevRail = (s: string) => (s === 'critical' || s === 'high' ? 'bg-status-error' : s === 'medium' ? 'bg-status-warning' : s === 'low' ? 'bg-accent-blue' : 'bg-text-tertiary')
const deliveryDot = (s: string) => (s === 'sent' ? 'bg-status-success' : s === 'pending' ? 'bg-status-warning' : 'bg-status-error')

// A single budget gauge: consumed vs. the configured limit, green → amber → red.
function Meter({ label, used, max, format }: { label: string; used?: number; max: number; format: (n: number) => string }) {
  const pct = max > 0 && used != null ? Math.min(100, Math.round((used / max) * 100)) : 0
  const fill = pct >= 95 ? 'bg-status-error' : pct >= 70 ? 'bg-status-warning' : 'bg-status-success'
  const valColor = used == null ? 'text-text-tertiary' : pct >= 95 ? 'text-status-error' : pct >= 70 ? 'text-status-warning' : 'text-text-primary'
  return (
    <div>
      <div className="flex items-baseline justify-between text-xs mb-1.5">
        <span className="text-text-tertiary">{label}</span>
        <span className={`font-semibold tabular-nums ${valColor}`}>{used != null ? format(used) : '—'}</span>
      </div>
      <div className="h-2 rounded-md bg-white/[0.06] border border-white/[0.04] overflow-hidden">
        <div className={`h-full rounded-md ${fill}`} style={{ width: `${pct}%` }} />
      </div>
      <div className="text-[11px] text-text-quaternary mt-1 tabular-nums">of {format(max)} max · {used != null ? `${pct}% used` : 'not started'}</div>
    </div>
  )
}

function TimelineRow({ tone, icon, title, time, body, last }: { tone: Tone; icon: ReactNode; title: string; time?: string; body?: ReactNode; last?: boolean }) {
  return (
    <li className="relative pl-8 pb-4 last:pb-0">
      {!last && <span className="absolute left-[11px] top-6 bottom-0 w-px bg-border-primary" aria-hidden />}
      <span className={`absolute left-0 top-0.5 grid h-6 w-6 place-items-center rounded-full border ${toneDotClass[tone]}`}>{icon}</span>
      <div className="flex items-baseline gap-2 flex-wrap">
        <span className="text-[13px] font-semibold text-text-primary">{title}</span>
        {time && <span className="text-[11px] text-text-quaternary tabular-nums">{time}</span>}
      </div>
      {body && <div className="text-xs text-text-tertiary mt-1 leading-relaxed">{body}</div>}
    </li>
  )
}

// ── Conversation transcript → readable chat ──
// Distils the raw Claude stream-json turns into human-readable chat messages:
// assistant prose, a one-line note per tool call, tool errors, and the final
// result. System/init turns and non-error tool outputs are dropped as noise.
function toolResultText(content: unknown): string {
  if (typeof content === 'string') return content
  const arr = asArr(content)
  if (arr) return arr.map(x => { const d = asDict(x); return asStr(d?.text) ?? (d?.type === 'image' ? '[image]' : JSON.stringify(x)) }).join('\n')
  return content == null ? '' : JSON.stringify(content, null, 2)
}

type ChatMsg = { key: string; kind: 'assistant' | 'tool' | 'tool-error'; tool?: string; markdown?: string; pretty?: string; plain?: string; badge?: string }

// The agent's messages are often the JSON output contract (e.g.
// {"no_op":true,"comment":"…"} or {"summary":…,"findings":[…]}). Rendered raw
// they're unreadable, so we lift the human field (comment/summary/result) and
// render it as markdown; JSON without one is pretty-printed, plain prose is kept.
function decodeAssistantText(text: string): Pick<ChatMsg, 'markdown' | 'pretty' | 'plain' | 'badge'> {
  const trimmed = text.trim()
  if (trimmed.startsWith('{') || trimmed.startsWith('```')) {
    const unfenced = trimmed.replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '').trim()
    try {
      const obj = asDict(JSON.parse(unfenced))
      if (obj) {
        const md = asStr(obj.comment) ?? asStr(obj.summary) ?? asStr(obj.result)
        const findings = asArr(obj.findings)
        const badge = obj.no_op === true ? 'No changes'
          : findings ? `${findings.length} finding${findings.length === 1 ? '' : 's'}`
          : asStr(obj.title) ? 'Proposed change'
          : undefined
        if (md && md.trim()) return { markdown: md, badge }
        return { pretty: JSON.stringify(obj, null, 2), badge }
      }
    } catch { /* not JSON after all — fall through to plain */ }
  }
  return { plain: text }
}

function toChatMessages(turns: AutonomousAgentEvent[]): ChatMsg[] {
  const out: ChatMsg[] = []
  for (const turn of turns) {
    const p = asDict(turn.payload) ?? {}
    const type = asStr(p.type) ?? turn.kind
    if (type !== 'assistant' && type !== 'user') continue // drop system/init and result (dup of the final assistant message)
    const content = asArr(asDict(p.message)?.content) ?? []
    content.forEach((block, i) => {
      const b = asDict(block) ?? {}
      const bt = asStr(b.type)
      const key = `${turn.sequence}-${i}`
      if (bt === 'text' && asStr(b.text)?.trim()) out.push({ key, kind: 'assistant', ...decodeAssistantText(asStr(b.text)!) })
      else if (bt === 'tool_use') out.push({ key, kind: 'tool', tool: asStr(b.name) ?? 'tool' })
      else if (bt === 'tool_result' && b.is_error === true) out.push({ key, kind: 'tool-error', plain: toolResultText(b.content).slice(0, 800) })
    })
  }
  return out
}

function ChatRow({ m }: { m: ChatMsg }) {
  if (m.kind === 'assistant') return (
    <div className="rounded-[10px] border border-white/[0.07] bg-white/[0.04] px-3 py-2">
      {m.badge && <span className="mb-1.5 inline-block rounded bg-white/[0.06] px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-text-tertiary">{m.badge}</span>}
      {m.markdown != null
        ? <div className="text-[13px] leading-relaxed text-text-primary [&_*]:!my-1 [&_h1]:text-sm [&_h2]:text-sm [&_h3]:text-[13px] [&_table]:text-[12px]"><Markdown content={m.markdown} /></div>
        : m.pretty != null
          ? <pre className="overflow-x-auto rounded bg-black/30 p-2 font-mono text-[11px] leading-relaxed text-text-secondary">{m.pretty}</pre>
          : <div className="text-[13px] leading-relaxed text-text-primary whitespace-pre-wrap">{m.plain}</div>}
    </div>
  )
  if (m.kind === 'tool') return <div className="flex items-center gap-1.5 pl-1 text-[11.5px] text-text-tertiary"><Wrench className="h-3 w-3" /> used <span className="font-mono text-text-secondary">{m.tool}</span></div>
  if (m.kind === 'tool-error') return <div className="rounded-[10px] border border-status-error/25 bg-status-error/[0.07] px-3 py-2 font-mono text-[12px] text-status-error whitespace-pre-wrap">{m.plain}</div>
  return null
}

function TranscriptView({ turns, live }: { turns: AutonomousAgentEvent[]; live: boolean }) {
  const messages = useMemo(() => toChatMessages(turns), [turns])
  const scrollRef = useRef<HTMLDivElement>(null)
  // Keep the newest message in view while the run streams.
  useEffect(() => {
    if (live && scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [messages.length, live])
  if (!turns.length) return <p className="text-xs text-text-tertiary">{live ? 'Waiting for the agent to start…' : 'No transcript recorded for this run.'}</p>
  return (
    <div ref={scrollRef} className="max-h-[520px] space-y-2 overflow-y-auto rounded-[12px] border border-border-primary bg-black/40 p-3">
      {messages.length ? messages.map(m => <ChatRow key={m.key} m={m} />) : <p className="text-xs text-text-tertiary">No readable messages yet.</p>}
      {live && <div className="flex items-center gap-2 pl-1 text-[11px] text-text-tertiary"><Loader2 className="h-3 w-3 animate-spin" /> streaming…</div>}
    </div>
  )
}

// The star of the redesign: turns run.finished's raw JSON into a readable story.
function RunDetail({ run, events, transcript, runActive, agentName, templateKey, onOpenFindings, onContinue, continuing }: { run: AutonomousAgentRun; events: AutonomousAgentEvent[]; transcript: AutonomousAgentEvent[]; runActive?: boolean; agentName?: string; templateKey?: string; onOpenFindings?: () => void; onContinue?: (id: string) => void; continuing?: boolean }) {
  const canContinue = Boolean(onContinue) && ['budget_exhausted', 'partial', 'blocked_policy', 'failed', 'cancelled'].includes(run.status)
  const meta = runStatusMeta(run.status)
  const finished = events.find(e => e.kind === 'run.finished')
  const payload = asDict(finished?.payload) ?? {}
  const result = asDict(payload.result) ?? {}
  // Template output (PR / review) is nested under `published` in the backend
  // payload; fall back to top-level for QA/older runs that don't nest.
  const pub = asDict(payload.published) ?? payload
  const code = asStr(payload.code)
  const cost = asNum(result.total_cost_usd)
  const turns = asNum(result.num_turns)
  // Findings come from the parsed model message (see parseLenient), falling back
  // to a direct `findings` array on the result for any structured-output runs.
  const structured = (asStr(result.result) ? parseLenient(asStr(result.result) as string) : undefined) ?? result
  const findings = asArr(structured.findings) ?? asArr(result.findings)
  const screenshots = asDict(payload.screenshots) ?? asDict(result.screenshots)
  const budget = (run.budget ?? {}) as Dict
  const maxCost = asNum(budget.max_cost_usd)
  const wall = asNum(budget.wall_time_seconds)
  const maxFiles = asNum(budget.max_changed_files)
  const filesChanged = asNum(pub.files_changed)
  const durationSec = Number.isFinite(parseUtc(run.started_at)) && Number.isFinite(parseUtc(run.finished_at))
    ? Math.max(0, (parseUtc(run.finished_at) - parseUtc(run.started_at)) / 1000)
    : undefined

  // Outcome sentence, assembled only from fields that are actually present.
  const lead = agentName || `${titleCase(run.trigger_kind)} run`
  let phrase: string
  if (code && RESULT_CODE[code]) phrase = RESULT_CODE[code]
  else if (run.status === 'running' || run.status === 'leased') phrase = 'is running now'
  else if (run.status === 'queued') phrase = 'is queued to start'
  else phrase = meta.label.toLowerCase()
  const bits: string[] = []
  if (cost != null) bits.push(usd(cost))
  if (turns != null) bits.push(`${turns} turn${turns === 1 ? '' : 's'}`)
  if (durationSec != null) bits.push(dur(durationSec))
  const tail: string[] = []
  if (findings && findings.length) tail.push(`Found ${findings.length} finding${findings.length === 1 ? '' : 's'}.`)
  const pr = asDict(pub.draft_pull_request)
  const prNumber = asNum(pr?.number)
  if (prNumber != null) tail.push(`Opened draft PR #${prNumber}.`)
  const reviewEvent = asStr(pub.event) ?? asStr(asDict(pub.github_review)?.event)
  if (reviewEvent) tail.push(`Left a ${reviewEvent} review.`)
  if (run.status === 'budget_exhausted' || code === 'cost_limit_exceeded' || code === 'wall_time_exceeded') tail.push('Consider raising the budget or narrowing scope, then re-run.')
  if (run.status === 'blocked_runtime' || code === 'claude_runtime_unavailable' || code === 'claude_auth_required') tail.push('No budget was spent — fix the runtime and this occurrence re-leases automatically.')

  const meters = [
    maxCost ? <Meter key="cost" label="Cost" used={cost} max={maxCost} format={usd} /> : null,
    wall ? <Meter key="time" label="Wall time" used={durationSec} max={wall} format={dur} /> : null,
    maxFiles ? <Meter key="files" label="Files changed" used={filesChanged} max={maxFiles} format={round} /> : null,
  ].filter(Boolean)

  // Timeline: one readable row per raw event.
  const rows = events.map((e, i) => {
    const last = i === events.length - 1
    const time = fmtTime(e.created_at)
    if (e.kind === 'run.started') {
      const worker = asStr(asDict(e.payload)?.worker)
      return <TimelineRow key={e.sequence} tone="info" icon={<Play className="w-3 h-3" />} title="Run started" time={time} last={last}
        body={<>Leased to worker <span className="text-text-secondary">{worker ?? 'local'}</span>{run.snapshot_sha ? <> · snapshot <span className="font-mono text-text-secondary">{run.snapshot_sha.slice(0, 7)}</span></> : null}.</>} />
    }
    if (e.kind === 'run.cancelled') {
      return <TimelineRow key={e.sequence} tone="neutral" icon={<Ban className="w-3 h-3" />} title="Cancelled by operator" time={time} last={last} body="The run was stopped on request. Partial work discarded." />
    }
    if (e.kind === 'run.finished') {
      const good = meta.tone === 'ok'
      const warnT = meta.tone === 'warn'
      const tone: Tone = good ? 'ok' : warnT ? 'warn' : 'bad'
      const icon = good ? <CheckCircle2 className="w-3.5 h-3.5" /> : warnT ? <AlertTriangle className="w-3.5 h-3.5" /> : <XCircle className="w-3.5 h-3.5" />
      const t = code && RESULT_CODE[code] ? `Agent ${RESULT_CODE[code]}` : good ? 'Agent finished' : `Run ${meta.label.toLowerCase()}`
      return <TimelineRow key={e.sequence} tone={tone} icon={icon} title={t} time={time} last={last}
        body={bits.length ? <>Used {bits.join(' · ')}{code ? <> · code <span className="font-mono text-text-secondary">{code}</span></> : null}.</> : (code ? <>Result code <span className="font-mono text-text-secondary">{code}</span>.</> : undefined)} />
    }
    return <TimelineRow key={e.sequence} tone="neutral" icon={<span className="text-[10px]">•</span>} title={titleCase(e.kind)} time={time} last={last} />
  })

  // Per-template result cards, chosen by which payload keys are present.
  const lc = asDict(pub.lines_changed)
  const linesAdded = asNum(lc?.added)
  const linesRemoved = asNum(lc?.removed)
  const linesTotal = asNum(pub.lines_changed)
  const verification = asStr(pub.verification) ?? (asDict(pub.verification) ? 'recorded' : undefined)
  const prUrl = asStr(pr?.html_url)
  const review = asDict(pub.github_review)
  const reviewUrl = asStr(review?.html_url)

  return (
    <div className="space-y-4">
      {/* header */}
      <div className="flex justify-between gap-4 flex-wrap items-start">
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <h2 className="text-[17px] font-semibold text-text-primary tracking-tight">{agentName ?? `${titleCase(run.trigger_kind)} run`}</h2>
            {templateKey && <Badge size="sm" variant="info">{titleCase(templateKey)}</Badge>}
          </div>
          <div className="flex gap-3.5 flex-wrap text-xs text-text-tertiary mt-2 tabular-nums">
            <span>{titleCase(run.trigger_kind)} trigger</span>
            {run.started_at && <span className="inline-flex items-center gap-1"><Clock className="w-3 h-3" />Started {fmtTime(run.started_at)}</span>}
            {durationSec != null && <span>Ran {dur(durationSec)}</span>}
            {run.snapshot_sha && <span>Snapshot <span className="font-mono">{run.snapshot_sha.slice(0, 7)}</span></span>}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={meta.variant} dot>{meta.label}{code ? ` · ${code}` : ''}</Badge>
          {canContinue && <Button size="sm" variant="secondary" leftIcon={<RefreshCw className="w-3.5 h-3.5" />} loading={continuing} onClick={() => onContinue?.(run.id)}>Continue</Button>}
        </div>
      </div>

      {/* outcome sentence */}
      <div className={`rounded-[12px] border px-4 py-3 text-[13px] leading-relaxed text-text-secondary ${outcomeToneClass[meta.tone]}`}>
        <span className="text-text-primary font-semibold">{lead}</span> {phrase}
        {bits.length ? <> — used {bits.join(' · ')}</> : null}. {tail.join(' ')}
      </div>

      {/* budget gauges */}
      {meters.length > 0 && (
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-2.5">Budget consumed</p>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">{meters}</div>
        </div>
      )}

      {/* human timeline */}
      <div>
        <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-3">What happened</p>
        {rows.length ? <ol className="m-0 p-0 list-none">{rows}</ol> : <p className="text-xs text-text-tertiary">No events recorded yet.</p>}
      </div>

      {/* full agent conversation (streamed) */}
      <div>
        <p className="mb-3 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-text-tertiary"><MessagesSquare className="w-3.5 h-3.5" />Conversation{transcript.length ? <span className="text-text-tertiary/70">· {transcript.length} turns</span> : null}</p>
        <TranscriptView turns={transcript} live={Boolean(runActive)} />
      </div>

      {/* QA result */}
      {(findings?.length || screenshots) && (
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-2">Result · QA</p>
          <div className="rounded-[12px] border border-border-primary bg-white/[0.02] p-4">
            <div className="flex items-center gap-2 mb-3 text-[13px] font-semibold text-text-primary"><Camera className="w-4 h-4 text-accent-blue" />{findings?.length ?? 0} finding{(findings?.length ?? 0) === 1 ? '' : 's'}{screenshots ? ` · ${Object.keys(screenshots).length} screenshot${Object.keys(screenshots).length === 1 ? '' : 's'}` : ''}</div>
            {screenshots && (
              <div className="flex gap-2.5 flex-wrap">
                {Object.entries(screenshots).slice(0, 6).map(([name, url]) => {
                  // Prefer the stable re-signing endpoint (durable) over the raw
                  // presigned URL baked into the result, which expires after 7 days.
                  const src = run.id
                    ? `${import.meta.env.VITE_API_URL ?? ''}/evidence/${encodeURIComponent(run.id)}/${encodeURIComponent(name)}`
                    : (typeof url === 'string' ? url : undefined)
                  return (
                  <a key={name} href={src} target="_blank" rel="noreferrer" className="w-28">
                    {src
                      ? <img src={src} alt={name} className="h-[70px] w-full object-cover rounded-lg border border-border-primary" />
                      : <div className="h-[70px] rounded-lg border border-border-primary bg-white/[0.03]" />}
                    <div className="text-[10.5px] text-text-tertiary mt-1 text-center truncate">{name}</div>
                  </a>
                  )
                })}
              </div>
            )}
            {onOpenFindings && <button type="button" onClick={onOpenFindings} className="mt-3 text-xs text-accent-blue">View in Findings →</button>}
          </div>
        </div>
      )}

      {/* github_issue_resolver result */}
      {pr && (
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-2">Result · Pull request</p>
          <div className="rounded-[12px] border border-border-primary bg-white/[0.02] p-4">
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
              {filesChanged != null && <div className="rounded-[10px] border border-white/[0.06] bg-white/[0.02] px-3 py-2"><div className="text-base font-semibold text-text-primary tabular-nums">{filesChanged}</div><div className="text-[10.5px] text-text-tertiary">Files changed</div></div>}
              {linesAdded != null && <div className="rounded-[10px] border border-white/[0.06] bg-white/[0.02] px-3 py-2"><div className="text-base font-semibold text-status-success tabular-nums">+{linesAdded}</div><div className="text-[10.5px] text-text-tertiary">Lines added</div></div>}
              {linesRemoved != null && <div className="rounded-[10px] border border-white/[0.06] bg-white/[0.02] px-3 py-2"><div className="text-base font-semibold text-status-error tabular-nums">-{linesRemoved}</div><div className="text-[10.5px] text-text-tertiary">Lines removed</div></div>}
              {linesTotal != null && <div className="rounded-[10px] border border-white/[0.06] bg-white/[0.02] px-3 py-2"><div className="text-base font-semibold text-text-primary tabular-nums">{linesTotal}</div><div className="text-[10.5px] text-text-tertiary">Lines changed</div></div>}
              {verification && <div className="rounded-[10px] border border-white/[0.06] bg-white/[0.02] px-3 py-2"><div className="text-sm font-semibold text-status-success">✓ {verification}</div><div className="text-[10.5px] text-text-tertiary">Verification</div></div>}
            </div>
            <a href={prUrl} target="_blank" rel="noreferrer" className="mt-3 inline-flex items-center gap-2 rounded-[10px] border border-border-primary bg-white/[0.02] px-3 py-2 text-[13px] hover:border-white/20">
              <GitPullRequest className="w-4 h-4 text-accent-purple" />
              <span className="text-accent-blue font-semibold">{prNumber != null ? `#${prNumber}` : 'Draft PR'}</span>
              <Badge size="sm" variant="purple">draft</Badge>
              {prUrl && <ExternalLink className="w-3.5 h-3.5 text-text-tertiary" />}
            </a>
          </div>
        </div>
      )}

      {/* github_pr_reviewer result */}
      {(review || reviewEvent) && !pr && (
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-2">Result · GitHub review</p>
          <div className="rounded-[12px] border border-border-primary bg-white/[0.02] p-4">
            <a href={reviewUrl} target="_blank" rel="noreferrer" className="inline-flex items-center gap-2 rounded-[10px] border border-border-primary bg-white/[0.02] px-3 py-2 text-[13px] hover:border-white/20">
              <GitPullRequest className="w-4 h-4 text-accent-purple" />
              <span className="text-accent-blue font-semibold">Review posted</span>
              {reviewEvent && <Badge size="sm" variant={reviewEvent === 'REQUEST_CHANGES' ? 'warning' : reviewEvent === 'APPROVE' ? 'success' : 'default'}>{reviewEvent}</Badge>}
              {reviewUrl && <ExternalLink className="w-3.5 h-3.5 text-text-tertiary" />}
            </a>
          </div>
        </div>
      )}

      {/* raw escape hatch */}
      {finished && (
        <details className="border-t border-border-secondary pt-3">
          <summary className="cursor-pointer text-xs text-text-tertiary">View raw result (JSON)</summary>
          <pre className="mt-2.5 overflow-auto rounded-lg bg-black/30 p-3 font-mono text-[11px] text-text-tertiary leading-relaxed">{JSON.stringify(finished.payload, null, 2)}</pre>
        </details>
      )}
    </div>
  )
}

export default function AutonomousAgents() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const can = (permission: string) => permissions.includes(permission)
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const [tab, setTab] = useState<Tab>('agents')
  const [showArchived, setShowArchived] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [editing, setEditing] = useState<AutonomousAgentDetail | null>(null)
  const [templateDetail, setTemplateDetail] = useState<AutonomousAgentTemplate | null>(null)
  const [selectedRun, setSelectedRun] = useState<AutonomousAgentRun | null>(null)
  const [actionError, setActionError] = useState('')

  const templates = useQuery({ queryKey: ['autonomous-templates'], queryFn: () => client.listAutonomousAgentTemplates(), enabled: can('autonomous_agent:read') })
  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents(), enabled: can('autonomous_agent:read') })
  // Poll while the selected run is still active so the run list, timeline, and
  // conversation update live. Reads the freshest run status from the cache to
  // avoid ordering issues with the runs query below.
  const selectedRunActive = () => {
    const list = queryClient.getQueryData<AutonomousAgentRun[]>(['autonomous-runs'])
    const r = list?.find(x => x.id === selectedRun?.id) ?? selectedRun
    return Boolean(r && ['queued', 'leased', 'running'].includes(r.status))
  }
  const runs = useQuery({ queryKey: ['autonomous-runs'], queryFn: () => client.listAutonomousAgentRuns(), enabled: can('autonomous_agent:read') && tab === 'runs', refetchInterval: () => (selectedRunActive() ? 2500 : false) })
  // Lifecycle events barely change mid-run; the terminal-status effect below
  // refetches them once the run finishes, so no need to poll them live.
  const events = useQuery({ queryKey: ['autonomous-run-events', selectedRun?.id], queryFn: () => client.listAutonomousAgentRunEvents(selectedRun!.id), enabled: can('autonomous_agent:read') && Boolean(selectedRun) })
  const transcript = useQuery({
    queryKey: ['autonomous-run-transcript', selectedRun?.id],
    // Incremental: only fetch turns after the last one we already hold, then
    // append. Avoids re-downloading the whole conversation on every poll.
    queryFn: async () => {
      const prev = queryClient.getQueryData<AutonomousAgentEvent[]>(['autonomous-run-transcript', selectedRun?.id]) ?? []
      const after = prev.length ? prev[prev.length - 1].sequence : 0
      const fresh = await client.listAutonomousAgentRunTranscript(selectedRun!.id, after)
      return after === 0 ? fresh : fresh.length ? [...prev, ...fresh] : prev
    },
    enabled: can('autonomous_agent:read') && Boolean(selectedRun),
    refetchInterval: () => (selectedRunActive() ? 2000 : false),
  })
  // When a run stops streaming, pull the final turns once (the last poll may have
  // fired just before the run's closing turns were written).
  const selectedLiveStatus = (runs.data?.find(r => r.id === selectedRun?.id) ?? selectedRun)?.status
  useEffect(() => {
    if (selectedRun && selectedLiveStatus && !['queued', 'leased', 'running'].includes(selectedLiveStatus)) {
      void transcript.refetch()
      void events.refetch()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedLiveStatus, selectedRun?.id])
  const runtime = useQuery({ queryKey: ['autonomous-runtime'], queryFn: () => client.getAutonomousRuntimeHealth(), enabled: can('autonomous_agent:read') && tab === 'runtime' })
  const settings = useQuery({ queryKey: ['autonomous-settings'], queryFn: () => client.getAutonomousAgentSettings(), enabled: can('autonomous_agent:read') && tab === 'runtime' })
  const metrics = useQuery({ queryKey: ['autonomous-metrics'], queryFn: () => client.getAutonomousAgentMetrics(), enabled: can('autonomous_agent:read') && tab === 'runtime', refetchInterval: 30_000 })
  const findings = useQuery({ queryKey: ['autonomous-findings'], queryFn: () => client.listAutonomousAgentFindings(), enabled: can('autonomous_agent:read') && tab === 'findings' })
  const deliveries = useQuery({ queryKey: ['autonomous-deliveries'], queryFn: () => client.listAutonomousAgentDeliveries(), enabled: can('autonomous_agent:read') && tab === 'findings' })
  const linkedinConnections = useQuery({ queryKey: ['linkedin-connections'], queryFn: () => client.listLinkedinConnections(), enabled: can('autonomous_agent:read') && tab === 'findings' })

  const invalidate = (...keys: string[]) => Promise.all(keys.map(key => queryClient.invalidateQueries({ queryKey: [key] })))
  const friendly = (message: string) => message === 'validation_required' ? 'Validate the agent before enabling it.' : message
  const useLifecycle = (fn: (id: string) => Promise<unknown>) => useMutation({
    mutationFn: fn,
    onSuccess: () => { setActionError(''); return invalidate('autonomous-agents') },
    onError: (value: unknown) => setActionError(friendly(value instanceof Error ? value.message : 'Action failed')),
  })
  const enable = useLifecycle(id => client.enableAutonomousAgent(id))
  const disable = useLifecycle(id => client.disableAutonomousAgent(id))
  const archive = useLifecycle(id => client.archiveAutonomousAgent(id))
  const validate = useMutation({
    mutationFn: (id: string) => client.validateAutonomousAgent(id),
    onSuccess: detail => {
      void invalidate('autonomous-agents')
      if (detail.revision.validation_status === 'valid') { setActionError(''); return }
      const errors = Array.isArray((detail.revision.validation as { errors?: unknown } | null)?.errors) ? ((detail.revision.validation as { errors: string[] }).errors) : []
      setActionError(errors.length ? `Validation failed: ${errors.join(', ')}` : 'Validation failed — review the configuration.')
    },
    onError: (value: unknown) => setActionError(value instanceof Error ? value.message : 'Validation failed'),
  })
  const clone = useMutation({ mutationFn: async (id: string) => { const source = await client.getAutonomousAgent(id); return client.createAutonomousAgent({ name: `${source.name} copy`, description: source.description ?? undefined, template_key: source.template_key, config: source.revision.config, budgets: source.revision.budgets }) }, onSuccess: () => invalidate('autonomous-agents') })
  const runNow = useMutation({ mutationFn: (vars: { id: string; targets?: Array<{ repository: string; type: 'pr' | 'issue'; number: number }> }) => client.runAutonomousAgent(vars.id, vars.targets ? { targets: vars.targets } : undefined), onSuccess: () => { invalidate('autonomous-runs'); setRunJudge(null); setRunReviewer(null) } })
  const [runJudge, setRunJudge] = useState<AutonomousAgentDefinition | null>(null)
  const [runReviewer, setRunReviewer] = useState<AutonomousAgentDefinition | null>(null)
  const cancelRun = useMutation({ mutationFn: (id: string) => client.cancelAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs') })
  const continueRun = useMutation({ mutationFn: (id: string) => client.continueAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs') })
  const archiveRun = useMutation({ mutationFn: (id: string) => client.archiveAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs') })
  const unarchiveRun = useMutation({ mutationFn: (id: string) => client.unarchiveAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs') })
  const archiveAllRuns = useMutation({ mutationFn: () => client.archiveAllAutonomousAgentRuns(), onSuccess: () => invalidate('autonomous-runs') })
  const [runStatusFilter, setRunStatusFilter] = useState<'all' | 'active' | 'succeeded' | 'failed' | 'blocked' | 'cancelled'>('all')
  const [showArchivedRuns, setShowArchivedRuns] = useState(false)
  const checkRuntime = useMutation({ mutationFn: () => client.checkAutonomousRuntimeHealth(), onSuccess: () => invalidate('autonomous-runtime') })
  const toggleOrg = useMutation({ mutationFn: (enabled: boolean) => client.patchAutonomousAgentSettings({ enabled }), onSuccess: () => invalidate('autonomous-settings', 'autonomous-runs') })
  const saveRetention = useMutation({ mutationFn: (days: number) => client.patchAutonomousAgentSettings({ retention_days: days }), onSuccess: () => invalidate('autonomous-settings') })
  const retryDelivery = useMutation({ mutationFn: (id: string) => client.retryAutonomousAgentDelivery(id), onSuccess: () => invalidate('autonomous-deliveries') })
  const resolveFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'resolved'), onSuccess: () => invalidate('autonomous-findings') })
  const archiveFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'ignored'), onSuccess: () => invalidate('autonomous-findings') })
  const restoreFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'open'), onSuccess: () => invalidate('autonomous-findings') })
  // Approve & publish a generated post draft to LinkedIn.
  const publishPost = useMutation({
    mutationFn: (vars: { id: string; destination: 'personal' | 'organization' }) => client.publishFindingLinkedin(vars.id, { destination: vars.destination }),
    onSuccess: result => { invalidate('autonomous-findings'); if (result.url) window.open(result.url, '_blank') },
    onError: (err: unknown) => window.alert(`Could not publish: ${(err as { message?: string })?.message ?? 'unknown error'}`),
  })
  const connectLinkedin = useMutation({
    mutationFn: (destination: 'personal' | 'organization') => client.linkedinAuthorize(destination),
    onSuccess: result => { if (result.url) window.open(result.url, '_blank', 'width=600,height=760') },
    onError: (err: unknown) => window.alert(`LinkedIn is not configured: ${(err as { message?: string })?.message ?? 'unknown error'}`),
  })
  const archiveAllFindings = useMutation({ mutationFn: () => client.archiveAllAutonomousAgentFindings(), onSuccess: () => invalidate('autonomous-findings') })
  // "Create issue" for a finding the agent did not file. Retries with an explicit
  // repository when the agent has none configured.
  const createFindingIssue = useMutation({
    mutationFn: (vars: { findingId: string; repository?: string }) => client.createFindingIssue(vars.findingId, vars.repository),
    onSuccess: () => invalidate('autonomous-findings', 'autonomous-deliveries'),
    onError: (err: unknown, vars) => {
      const code = (err as { code?: string })?.code
      if (code === 'repository_required' && !vars.repository) {
        const repository = window.prompt('Repository for this issue (owner/repo):')?.trim()
        if (repository) createFindingIssue.mutate({ findingId: vars.findingId, repository })
      }
    },
  })
  const [showArchivedFindings, setShowArchivedFindings] = useState(false)
  // "Resolve with agent": hand a single finding (and its linked GitHub issue, if
  // one was filed) to a chosen issue-resolver so it fixes ONLY that one thing.
  const [resolveWith, setResolveWith] = useState<{ findingId: string; title: string; finding: Dict; issue?: { repository: string; number: number } } | null>(null)
  const resolveWithAgent = useMutation({
    mutationFn: (vars: { agentId: string; findingId: string; finding: Dict; issue?: { repository: string; number: number } }) =>
      client.runAutonomousAgent(vars.agentId, { finding: vars.finding, finding_id: vars.findingId, ...(vars.issue ? { issue: vars.issue } : {}) }),
    onSuccess: () => { invalidate('autonomous-runs'); setResolveWith(null); setTab('runs') },
  })

  const openEdit = async (agent: AutonomousAgentDefinition) => {
    const detail = await client.getAutonomousAgent(agent.id)
    setEditing(detail)
  }

  if (!can('autonomous_agent:read')) return <Navigate to="/401" replace />

  const loading = [agents, templates].some(query => query.isLoading)
  const loadError = [agents, templates, runs, findings, runtime].find(query => query.isError)
  const allAgents = agents.data ?? []
  const archivedCount = allAgents.filter(agent => agent.status === 'archived').length
  const visibleAgents = showArchived ? allAgents : allAgents.filter(agent => agent.status !== 'archived')
  const runAgent = selectedRun ? allAgents.find(agent => agent.id === selectedRun.definition_id) : undefined

  const METRIC_TILES: { key: string; label: string; money?: boolean; alert?: 'warn' | 'bad' }[] = [
    { key: 'queued', label: 'Queued' },
    { key: 'running', label: 'Running' },
    { key: 'blocked', label: 'Blocked', alert: 'bad' },
    { key: 'open_findings', label: 'Open findings', alert: 'warn' },
    { key: 'failed_deliveries', label: 'Failed deliveries', alert: 'bad' },
    { key: 'dead_letters', label: 'Dead letters', alert: 'bad' },
    { key: 'estimated_cost_usd', label: 'Est. cost', money: true },
  ]

  return (
    <div className="p-6 md:p-8 space-y-6 max-w-7xl mx-auto">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary flex items-center gap-2"><Bot className="w-6 h-6" />Autonomous agents</h1>
          <p className="text-sm text-text-tertiary mt-1">Scheduled Claude Code agents running on this NexusMind server.</p>
        </div>
        {can('autonomous_agent:create') && (
          <Button variant="primary" leftIcon={<Plus className="w-4 h-4" />} onClick={() => setShowCreate(true)}>Create agent</Button>
        )}
      </header>

      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-primary pb-3">
        <SegmentedControl options={TABS} value={tab} onChange={setTab} />
        {tab === 'agents' && archivedCount > 0 && (
          <Switch size="sm" checked={showArchived} onCheckedChange={setShowArchived} label={`Show archived (${archivedCount})`} />
        )}
      </div>

      {loading && <p role="status" className="text-sm text-text-tertiary">Loading automation control plane…</p>}
      {loadError && <p role="alert" className="text-sm text-status-error">Could not load autonomous agent data.</p>}

      {tab === 'agents' && !loading && (
        <div className="grid gap-3">
          {actionError && (
            <div role="alert" className="flex items-start justify-between gap-3 rounded-[12px] border border-status-error/40 bg-status-error/[0.06] px-4 py-3 text-sm text-status-error">
              <span>{actionError}</span>
              <button type="button" onClick={() => setActionError('')} aria-label="Dismiss"><X className="w-4 h-4" /></button>
            </div>
          )}
          {visibleAgents.map(agent => (
            <article key={agent.id} className="rounded-[14px] border border-border-primary bg-white/[0.02] p-4 flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
              <div className="min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <h2 className="font-semibold text-text-primary truncate">{agent.name}</h2>
                  <Badge size="sm" variant={statusVariant[agent.status] ?? 'default'} dot>{agent.status}</Badge>
                  {agent.status === 'disabled' && agent.validation_status === 'invalid' && <Badge size="sm" variant="error">invalid</Badge>}
                  {agent.status === 'disabled' && agent.validation_status === 'valid' && <Badge size="sm" variant="success">validated</Badge>}
                </div>
                <p className="text-xs text-text-tertiary mt-1">{agent.template_key.replace(/_/g, ' ')} · revision {agent.current_revision}</p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {can('autonomous_agent:enable') && agent.status === 'disabled' && <>
                  <Button size="sm" variant={agent.validation_status === 'valid' ? 'outline' : 'secondary'} loading={validate.isPending && validate.variables === agent.id} onClick={() => validate.mutate(agent.id)}>Validate</Button>
                  {agent.validation_status === 'valid'
                    ? <Button size="sm" variant="primary" onClick={() => enable.mutate(agent.id)}>Enable</Button>
                    : <span className="text-xs text-text-tertiary px-1">Validate to enable</span>}
                </>}
                {can('autonomous_agent:enable') && agent.status === 'enabled' && <Button size="sm" variant="outline" onClick={() => disable.mutate(agent.id)}>Disable</Button>}
                {can('autonomous_agent:run') && agent.status === 'enabled' && <Button size="sm" variant="primary" leftIcon={<Play className="w-3.5 h-3.5" />} onClick={() => agent.template_key === 'judge' ? setRunJudge(agent) : agent.template_key === 'github_pr_reviewer' ? setRunReviewer(agent) : runNow.mutate({ id: agent.id })}>Run</Button>}
                {can('autonomous_agent:update') && agent.status !== 'archived' && <Button size="sm" variant="ghost" leftIcon={<Pencil className="w-3.5 h-3.5" />} onClick={() => void openEdit(agent)}>Edit</Button>}
                {can('autonomous_agent:create') && <Button size="sm" variant="ghost" leftIcon={<Copy className="w-3.5 h-3.5" />} onClick={() => clone.mutate(agent.id)}>Clone</Button>}
                {can('autonomous_agent:update') && agent.status === 'disabled' && <Button size="sm" variant="ghost" leftIcon={<Archive className="w-3.5 h-3.5" />} onClick={() => archive.mutate(agent.id)}>Archive</Button>}
              </div>
            </article>
          ))}
          {visibleAgents.length === 0 && (
            <EmptyState
              icon={<Bot className="w-6 h-6" />}
              title={allAgents.length === 0 ? 'No autonomous agents yet' : 'No active agents'}
              description={allAgents.length === 0 ? 'Create your first agent from a managed template.' : 'All agents are archived. Toggle “Show archived” to see them.'}
              action={can('autonomous_agent:create') && allAgents.length === 0 ? <Button variant="primary" leftIcon={<Plus className="w-4 h-4" />} onClick={() => setShowCreate(true)}>Create agent</Button> : undefined}
            />
          )}
        </div>
      )}

      {tab === 'templates' && (
        <div className="grid gap-4 md:grid-cols-3">
          {templates.data?.map(item => (
            <button key={item.key} type="button" onClick={() => setTemplateDetail(item)} className="text-left rounded-[14px] border border-border-primary bg-white/[0.02] p-5 transition-colors hover:border-white/20">
              <div className="flex items-center justify-between">
                <h2 className="font-semibold text-text-primary">{item.name}</h2>
                <Badge size="sm" variant="default">v{item.version}</Badge>
              </div>
              <p className="text-sm text-text-tertiary mt-2 leading-relaxed">{item.description}</p>
              {item.workflow?.length > 0 && (
                <div className="mt-3 flex flex-wrap items-center gap-1 text-[11px] text-text-tertiary">
                  {item.workflow.map((step, index) => (
                    <span key={`${step}-${index}`} className="flex items-center gap-1">
                      <span className="rounded-md border border-border-secondary bg-white/[0.03] px-1.5 py-0.5">{step.replace(/_/g, ' ')}</span>
                      {index < item.workflow.length - 1 && <span className="text-text-quaternary">→</span>}
                    </span>
                  ))}
                </div>
              )}
              <div className="mt-3 flex flex-wrap gap-1">{item.capabilities.slice(0, 3).map(cap => <Badge key={cap} size="sm" variant="info">{cap}</Badge>)}</div>
              <p className="mt-3 text-xs text-accent-blue">View details →</p>
            </button>
          ))}
        </div>
      )}

      {tab === 'runs' && (() => {
        const allRuns = runs.data ?? []
        // Collapse the many raw statuses into the filter's coarse buckets.
        const group = (s: string): typeof runStatusFilter => s.startsWith('blocked') ? 'blocked' : ['queued', 'leased', 'running'].includes(s) ? 'active' : (s === 'succeeded' || s === 'failed' || s === 'cancelled') ? s : 'all'
        const archivedCount = allRuns.filter(r => r.archived_at).length
        const visibleRuns = allRuns
          .filter(r => showArchivedRuns || !r.archived_at)
          .filter(r => runStatusFilter === 'all' || group(r.status) === runStatusFilter)
        const archivableCount = allRuns.filter(r => !r.archived_at && !['queued', 'leased', 'running'].includes(r.status)).length
        const FILTERS: Array<{ value: typeof runStatusFilter; label: string }> = [
          { value: 'all', label: 'All' }, { value: 'active', label: 'Active' }, { value: 'succeeded', label: 'Succeeded' }, { value: 'failed', label: 'Failed' }, { value: 'blocked', label: 'Blocked' }, { value: 'cancelled', label: 'Cancelled' },
        ]
        return (
        // Fixed-height board: the list and the run detail each scroll on their own;
        // the page itself stays put.
        <div className="grid gap-4 lg:grid-cols-[minmax(280px,340px)_1fr] h-[calc(100vh-15rem)] min-h-[520px]">
          <div className="rounded-[14px] border border-border-primary overflow-hidden flex flex-col min-h-0">
            <div className="px-4 py-3 border-b border-border-primary space-y-2.5">
              <div className="flex items-center justify-between">
                <span className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">Recent runs</span>
                <span className="text-xs text-text-tertiary">{visibleRuns.length}</span>
              </div>
              <div className="flex flex-wrap gap-1">
                {FILTERS.map(f => (
                  <button key={f.value} type="button" onClick={() => setRunStatusFilter(f.value)} className={`px-2 py-0.5 rounded-full text-[11px] font-medium transition-colors ${runStatusFilter === f.value ? 'bg-accent-blue/20 text-accent-blue' : 'text-text-tertiary hover:text-text-primary'}`}>{f.label}</button>
                ))}
              </div>
              {(archivedCount > 0 || archivableCount > 0) && (
                <div className="flex items-center justify-between gap-2">
                  {archivedCount > 0 ? <Switch size="sm" checked={showArchivedRuns} onCheckedChange={setShowArchivedRuns} label={`Archived (${archivedCount})`} /> : <span />}
                  {can('autonomous_agent:update') && archivableCount > 0 && (
                    <button type="button" onClick={() => { if (window.confirm(`Archive all ${archivableCount} finished run${archivableCount === 1 ? '' : 's'}?`)) archiveAllRuns.mutate() }} className="text-[11px] text-text-tertiary hover:text-text-primary font-medium">Archive all</button>
                  )}
                </div>
              )}
            </div>
            <div className="divide-y divide-border-secondary overflow-y-auto min-h-0 flex-1">
              {visibleRuns.map(run => {
                const meta = runStatusMeta(run.status)
                const active = selectedRun?.id === run.id
                const isRunning = ['queued', 'leased', 'running'].includes(run.status)
                return (
                  <button type="button" key={run.id} onClick={() => setSelectedRun(run)} className={`w-full px-4 py-3 flex flex-col gap-1.5 text-left transition-colors ${active ? 'bg-accent-blue/[0.10] shadow-[inset_3px_0_0_var(--color-accent-blue)]' : 'hover:bg-white/[0.02]'} ${run.archived_at ? 'opacity-60' : ''}`}>
                    <div className="flex justify-between gap-2 items-center">
                      <span className="text-[13px] font-semibold text-text-primary truncate">{allAgents.find(a => a.id === run.definition_id)?.name ?? `${titleCase(run.trigger_kind)} run`}</span>
                      <Badge size="sm" variant={meta.variant} dot={run.status !== 'running'}>{run.status === 'running' ? <Loader2 className="w-3 h-3 animate-spin" /> : null}{meta.label}</Badge>
                    </div>
                    <div className="flex items-center gap-2 flex-wrap text-[11px] text-text-tertiary tabular-nums">
                      <span>{titleCase(run.trigger_kind)}</span>
                      <span>·</span>
                      <span>{new Date(`${run.created_at}Z`).toLocaleString()}</span>
                      {can('autonomous_agent:cancel') && isRunning && (
                        <span onClick={event => { event.stopPropagation(); cancelRun.mutate(run.id) }} className="ml-auto text-status-error font-semibold">Cancel</span>
                      )}
                      {can('autonomous_agent:update') && !isRunning && (run.archived_at
                        ? <span onClick={event => { event.stopPropagation(); unarchiveRun.mutate(run.id) }} className="ml-auto text-accent-blue font-semibold">Restore</span>
                        : <span onClick={event => { event.stopPropagation(); archiveRun.mutate(run.id) }} className="ml-auto text-text-tertiary hover:text-text-primary font-semibold">Archive</span>
                      )}
                    </div>
                  </button>
                )
              })}
              {visibleRuns.length === 0 && <div className="p-4"><EmptyState title="No runs" description={allRuns.length ? 'No runs match this filter.' : 'Runs appear here once an enabled agent is triggered.'} /></div>}
            </div>
          </div>
          <aside className="rounded-[14px] border border-border-primary p-5 overflow-y-auto min-h-0">
            {selectedRun ? (
              <RunDetail run={runs.data?.find(r => r.id === selectedRun.id) ?? selectedRun} events={events.data ?? []} transcript={transcript.data ?? []} runActive={['queued', 'leased', 'running'].includes((runs.data?.find(r => r.id === selectedRun.id) ?? selectedRun).status)} agentName={runAgent?.name} templateKey={runAgent?.template_key} onOpenFindings={() => setTab('findings')} onContinue={can('autonomous_agent:run') ? id => continueRun.mutate(id) : undefined} continuing={continueRun.isPending} />
            ) : <EmptyState title="Select a run" description="See the outcome, budget consumption, a readable timeline, and what the agent produced." />}
          </aside>
        </div>
        )
      })()}

      {tab === 'findings' && (() => {
        const allFindings = findings.data ?? []
        const archivedFindingsCount = allFindings.filter(f => f.status === 'ignored').length
        const activeCount = allFindings.length - archivedFindingsCount
        const visibleFindings = showArchivedFindings ? allFindings : allFindings.filter(f => f.status !== 'ignored')
        return (
        <div className="space-y-3">
          {allFindings.length > 0 && (
            <div className="flex items-center justify-between gap-3 flex-wrap">
              <div className="flex items-center gap-3">
                {archivedFindingsCount > 0 && <Switch size="sm" checked={showArchivedFindings} onCheckedChange={setShowArchivedFindings} label={`Show archived (${archivedFindingsCount})`} />}
              </div>
              {can('autonomous_agent:update') && activeCount > 0 && (
                <Button size="sm" variant="ghost" leftIcon={<Archive className="w-3.5 h-3.5" />} loading={archiveAllFindings.isPending} onClick={() => { if (window.confirm(`Archive all ${activeCount} finding${activeCount === 1 ? '' : 's'}? You can restore them from “Show archived”.`)) archiveAllFindings.mutate() }}>Archive all</Button>
              )}
            </div>
          )}
          {(() => {
            const hasPosts = allFindings.some(f => asStr((f.evidence as Dict)?.kind) === 'post')
            if (!hasPosts) return null
            const connected = new Set((linkedinConnections.data ?? []).map(c => c.destination))
            return (
              <div className="rounded-[12px] border border-border-primary bg-white/[0.02] px-4 py-3 flex items-center justify-between gap-3 flex-wrap">
                <div className="text-[13px] text-text-secondary">
                  <span className="font-medium text-text-primary">LinkedIn</span> — {connected.size ? `connected: ${[...connected].join(', ')}` : 'not connected. Connect an account to publish approved posts.'}
                </div>
                {can('autonomous_agent:update') && (
                  <div className="flex items-center gap-2">
                    <Button size="sm" variant={connected.has('personal') ? 'ghost' : 'secondary'} loading={connectLinkedin.isPending} onClick={() => connectLinkedin.mutate('personal')}>{connected.has('personal') ? 'Reconnect personal' : 'Connect personal'}</Button>
                    <Button size="sm" variant={connected.has('organization') ? 'ghost' : 'secondary'} loading={connectLinkedin.isPending} onClick={() => connectLinkedin.mutate('organization')}>{connected.has('organization') ? 'Reconnect company' : 'Connect company'}</Button>
                    <button type="button" onClick={() => linkedinConnections.refetch()} className="text-xs text-accent-blue">Refresh</button>
                  </div>
                )}
              </div>
            )
          })()}
          {visibleFindings.map(finding => {
            const ev = (finding.evidence ?? {}) as Dict
            // Build the screenshot src from the stable re-signing endpoint using the
            // run id + the stored filename, so old evidence (whose baked-in presigned
            // URL has since expired) still renders. Fall back to the stored URL.
            const shotName = asStr(ev.screenshot)
            const shot = finding.run_id && shotName
              ? `${import.meta.env.VITE_API_URL ?? ''}/evidence/${encodeURIComponent(finding.run_id)}/${encodeURIComponent(shotName)}`
              : (asStr(ev.screenshot_url) ?? shotName)
            const location = ev.location
            const locDict = asDict(location)
            const locStr = asStr(location)
            const steps = asArr(ev.steps)
            const repro = asStr(ev.repro) ?? asStr(ev.excerpt) ?? asStr(ev.code)
            const findingDeliveries = deliveries.data?.filter(item => item.finding_id === finding.id) ?? []
            // The GitHub issue this finding was already filed as (if any).
            const linkedIssue = findingDeliveries.find(d => d.channel === 'github_issue' && d.external_url)
            const hasStructured = shot || locDict || locStr || (steps && steps.length) || repro
            return (
              <article key={finding.id} className="rounded-[14px] border border-border-primary bg-white/[0.02] overflow-hidden grid grid-cols-[4px_1fr]">
                <div className={sevRail(finding.severity)} aria-hidden />
                <div className="p-4">
                  <div className="flex justify-between gap-3 items-start flex-wrap">
                    <h2 className="text-sm font-semibold text-text-primary">{finding.title}</h2>
                    <div className="flex items-center gap-2">
                      {asStr(ev.kind) === 'feedback' && <Badge size="sm" variant="info">feedback</Badge>}
                      {asStr(ev.kind) === 'post' && <Badge size="sm" variant="info">post</Badge>}
                      <Badge size="sm" variant={sevVariant(finding.severity)} dot>{finding.severity}</Badge>
                      <Badge size="sm" variant={finding.status === 'resolved' ? 'success' : finding.status === 'ignored' ? 'warning' : 'default'}>{finding.status === 'ignored' ? 'archived' : finding.status}</Badge>
                      {can('autonomous_agent:run') && finding.status !== 'ignored' && asStr(ev.kind) === 'post' && (() => {
                        const connected = new Set((linkedinConnections.data ?? []).map(c => c.destination))
                        const postDest = asStr(asDict(ev.post)?.destination)
                        const dest = (postDest === 'organization' || postDest === 'personal') && connected.has(postDest) ? postDest : connected.has('personal') ? 'personal' : connected.has('organization') ? 'organization' : null
                        if (!dest) return null
                        return <button type="button" disabled={publishPost.isPending} onClick={() => { if (window.confirm(`Publish this post to LinkedIn (${dest})?`)) publishPost.mutate({ id: finding.id, destination: dest as 'personal' | 'organization' }) }} className="text-xs text-accent-blue font-medium disabled:opacity-50">Publish to LinkedIn</button>
                      })()}
                      {can('autonomous_agent:run') && finding.status !== 'ignored' && !asDict(ev.lead) && asStr(ev.kind) !== 'post' && !linkedIssue && (
                        <button type="button" disabled={createFindingIssue.isPending} onClick={() => createFindingIssue.mutate({ findingId: finding.id })} className="text-xs text-accent-blue font-medium disabled:opacity-50">Create issue</button>
                      )}
                      {can('autonomous_agent:run') && finding.status !== 'ignored' && asStr(ev.kind) !== 'post' && !asDict(ev.lead) && (() => {
                        // Link the GitHub issue this finding was filed as (if any), so
                        // the resolver both fixes the finding and closes the issue.
                        const parsed = linkedIssue?.external_url?.match(/github\.com\/([^/]+\/[^/]+)\/issues\/(\d+)/)
                        const issue = parsed ? { repository: parsed[1], number: Number(parsed[2]) } : undefined
                        return <button type="button" onClick={() => setResolveWith({ findingId: finding.id, title: finding.title, finding: { title: finding.title, summary: finding.summary, severity: finding.severity, evidence: ev }, issue })} className="text-xs text-accent-blue font-medium">Resolve with agent</button>
                      })()}
                      {can('autonomous_agent:update') && finding.status === 'open' && <button type="button" onClick={() => resolveFinding.mutate(finding.id)} className="text-xs text-accent-blue font-medium">Resolve</button>}
                      {can('autonomous_agent:update') && finding.status !== 'ignored' && <button type="button" onClick={() => archiveFinding.mutate(finding.id)} className="text-xs text-text-tertiary hover:text-text-primary font-medium">Archive</button>}
                      {can('autonomous_agent:update') && finding.status === 'ignored' && <button type="button" onClick={() => restoreFinding.mutate(finding.id)} className="text-xs text-accent-blue font-medium">Restore</button>}
                    </div>
                  </div>
                  <p className="text-sm text-text-tertiary mt-2 leading-relaxed">{finding.summary}</p>

                  {(() => {
                    const lead = asDict(ev.lead)
                    if (!lead) return null
                    const execs = (asArr(lead.executives) ?? []).map(asDict).filter(Boolean) as Dict[]
                    const sources = (asArr(lead.source_urls) ?? []).map(asStr).filter(Boolean) as string[]
                    const link = (label: string, url?: string, text?: string) => url ? <a key={label} href={url} target="_blank" rel="noreferrer" className="text-accent-blue hover:underline">{text ?? label}</a> : null
                    const socialChips = (arr: unknown, prefix: string) => (asArr(arr) ?? []).map(asDict).filter(Boolean).map((s, i) => {
                      const url = asStr((s as Dict).url); const platform = asStr((s as Dict).platform)
                      return url ? <a key={`${prefix}${i}`} href={url} target="_blank" rel="noreferrer" className="text-accent-blue hover:underline">{platform ? titleCase(platform) : 'Link'}</a> : null
                    }).filter(Boolean)
                    const contacts = [
                      link('Website', asStr(lead.website)),
                      asStr(lead.contact_email) ? <a key="email" href={`mailto:${asStr(lead.contact_email)}`} className="text-accent-blue hover:underline">{asStr(lead.contact_email)}</a> : null,
                      asStr(lead.contact_phone) ? <span key="phone" className="text-text-secondary">{asStr(lead.contact_phone)}</span> : null,
                      link('Contact page', asStr(lead.contact_page)),
                      link('LinkedIn', asStr(lead.company_linkedin)),
                      ...socialChips(lead.social_links, 'co-social-'),
                    ].filter(Boolean)
                    return (
                      <div className="mt-3 rounded-[12px] border border-border-primary p-3 space-y-2.5 text-xs">
                        {(asStr(lead.headquarters) || asStr(lead.industry)) && (
                          <p className="text-text-tertiary">{[asStr(lead.industry), asStr(lead.headquarters)].filter(Boolean).join(' · ')}</p>
                        )}
                        {contacts.length > 0 && <div className="flex flex-wrap gap-x-4 gap-y-1">{contacts}</div>}
                        {execs.length > 0 && (
                          <div>
                            <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-1.5">Decision-makers</p>
                            <div className="space-y-1">
                              {execs.map((e, i) => (
                                <div key={i} className="flex flex-wrap items-baseline gap-x-2">
                                  <span className="text-text-primary font-medium">{asStr(e.name) ?? '—'}</span>
                                  {asStr(e.title) && <span className="text-text-tertiary">{asStr(e.title)}</span>}
                                  {asStr(e.linkedin) && <a href={asStr(e.linkedin)} target="_blank" rel="noreferrer" className="text-accent-blue hover:underline">LinkedIn</a>}
                                  {asStr(e.public_email) && <a href={`mailto:${asStr(e.public_email)}`} className="text-accent-blue hover:underline">{asStr(e.public_email)}</a>}
                                  {asStr(e.direct_phone) && <span className="text-text-secondary">{asStr(e.direct_phone)}</span>}
                                  {socialChips(e.social_links, `ex${i}-social-`)}
                                </div>
                              ))}
                            </div>
                          </div>
                        )}
                        {(asStr(lead.email_subject) || asStr(lead.email_body)) && (
                          <details>
                            <summary className="cursor-pointer text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">Drafted email</summary>
                            {asStr(lead.email_subject) && <p className="mt-1.5 text-text-secondary"><span className="text-text-tertiary">Subject:</span> {asStr(lead.email_subject)}</p>}
                            {asStr(lead.email_body) && <pre className="mt-1 whitespace-pre-wrap text-text-secondary font-sans">{asStr(lead.email_body)}</pre>}
                          </details>
                        )}
                        {sources.length > 0 && <div className="flex flex-wrap gap-x-3 gap-y-1 text-text-quaternary">{sources.slice(0, 6).map((u, i) => <a key={i} href={u} target="_blank" rel="noreferrer" className="hover:text-text-secondary truncate max-w-[220px]">{u.replace(/^https?:\/\//, '')}</a>)}</div>}
                      </div>
                    )
                  })()}

                  {hasStructured && (
                    <div className="mt-3 grid gap-4 sm:grid-cols-[200px_1fr]">
                      {shot && (
                        <a href={shot} target="_blank" rel="noreferrer" className="block">
                          <img src={shot} alt="Finding evidence screenshot" className="w-full max-h-40 object-cover rounded-lg border border-border-primary" />
                        </a>
                      )}
                      <div className={`grid gap-3 ${shot ? '' : 'sm:col-span-2'}`}>
                        {(locDict || locStr) && (
                          <div>
                            <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-1.5">Where</p>
                            {locDict
                              ? <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">{Object.entries(locDict).map(([k, v]) => <div key={k} className="contents"><dt className="text-text-tertiary">{titleCase(k)}</dt><dd className="text-text-secondary font-mono break-all">{typeof v === 'string' || typeof v === 'number' ? String(v) : JSON.stringify(v)}</dd></div>)}</dl>
                              : <p className="text-xs text-text-secondary font-mono break-all">{locStr}</p>}
                          </div>
                        )}
                        {steps && steps.length > 0 && (
                          <div>
                            <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-1.5">Steps to reproduce</p>
                            <ol className="grid gap-1.5 list-none m-0 p-0">
                              {steps.map((s, i) => (
                                <li key={i} className="flex gap-2 text-xs text-text-secondary">
                                  <span className="grid h-4 w-4 shrink-0 place-items-center rounded-full bg-white/[0.05] border border-border-primary text-[10px] text-text-tertiary">{i + 1}</span>
                                  {typeof s === 'string' ? s : JSON.stringify(s)}
                                </li>
                              ))}
                            </ol>
                          </div>
                        )}
                        {repro && (
                          <div>
                            <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-1.5">Evidence excerpt</p>
                            <pre className="overflow-auto rounded-lg bg-black/30 p-2.5 font-mono text-[11.5px] text-text-secondary">{repro}</pre>
                          </div>
                        )}
                      </div>
                    </div>
                  )}

                  <div className="mt-3 flex items-center gap-2 flex-wrap pt-3 border-t border-border-secondary">
                    <span className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">Delivered</span>
                    {findingDeliveries.length === 0 && <span className="text-xs text-text-quaternary">No deliveries</span>}
                    {findingDeliveries.map(item => (
                      <span key={item.id} className="inline-flex items-center gap-1.5 rounded-lg border border-border-secondary bg-white/[0.02] px-2.5 py-1 text-xs text-text-secondary">
                        <span className={`w-1.5 h-1.5 rounded-full ${deliveryDot(item.status)}`} />
                        {item.channel}: {item.status}
                        {item.last_error_code && <span className="text-status-error font-mono break-all">({item.last_error_code})</span>}
                        {item.external_url && <a href={item.external_url} target="_blank" rel="noreferrer" className="text-accent-blue inline-flex"><ExternalLink className="w-3 h-3" /></a>}
                        {can('autonomous_agent:run') && ['slack', 'github_issue'].includes(item.channel) && ['failed', 'dead_letter'].includes(item.status) && (
                          <button type="button" onClick={() => retryDelivery.mutate(item.id)} className="inline-flex items-center gap-1 text-accent-blue font-medium"><RefreshCw className="w-3 h-3" />Retry</button>
                        )}
                      </span>
                    ))}
                    <span className="ml-auto text-xs text-text-tertiary">Seen {finding.occurrence_count} time(s)</span>
                  </div>

                  <details className="mt-2">
                    <summary className="cursor-pointer text-[11px] text-text-quaternary">View raw evidence (JSON)</summary>
                    <pre className="whitespace-pre-wrap break-all mt-2 text-[11px] text-text-tertiary font-mono">{JSON.stringify(finding.evidence, null, 2)}</pre>
                  </details>
                </div>
              </article>
            )
          })}
          {allFindings.length === 0 && <EmptyState title="No findings yet" description="Confirmed findings from QA and review agents will land here — with the screenshot, the reproduction, and where they were delivered." />}
          {allFindings.length > 0 && visibleFindings.length === 0 && <EmptyState title="All findings archived" description="Toggle “Show archived” to see them." />}
        </div>
        )
      })()}

      {tab === 'runtime' && (
        <div className="grid gap-4 md:grid-cols-2">
          <section className="rounded-[14px] border border-border-primary p-5 space-y-4">
            <div className="flex items-center gap-3">
              <ShieldCheck className="w-5 h-5 text-text-secondary" />
              <div>
                <h2 className="font-semibold text-text-primary capitalize">{runtime.data?.status?.replace(/_/g, ' ') ?? 'Loading'}</h2>
                <p className="text-xs text-text-tertiary">{runtime.data?.claude_version ?? runtime.data?.reason_code ?? 'Checking local Claude Code runtime'}</p>
              </div>
            </div>
            {runtime.data?.status === 'reauth_required' && (
              <p className="text-sm text-status-warning flex gap-2"><AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" />Authenticate Claude Code again as the backend OS account, then check again. Schedules remain durable and leasing is paused.</p>
            )}
            {can('autonomous_agent:enable') && <Button size="sm" variant="outline" leftIcon={<RefreshCw className="w-4 h-4" />} onClick={() => checkRuntime.mutate()}>Check again</Button>}
          </section>
          <section className="rounded-[14px] border border-border-primary p-5 space-y-3">
            <h2 className="font-semibold text-text-primary">Organization kill switch</h2>
            <p className="text-sm text-text-tertiary">{settings.data?.enabled ? 'Enabled' : 'Disabled'} · retention {settings.data?.retention_days ?? '—'} days</p>
            {can('autonomous_agent:enable') && (
              <>
                <Button size="sm" variant={settings.data?.enabled ? 'destructive' : 'secondary'} onClick={() => toggleOrg.mutate(!settings.data?.enabled)}>{settings.data?.enabled ? 'Disable all agents' : 'Enable agents'}</Button>
                <label className="block text-xs text-text-secondary">Retention days<input type="number" min={7} max={3650} defaultValue={settings.data?.retention_days} onBlur={event => saveRetention.mutate(Number(event.target.value))} className="mt-1 block rounded-lg border border-border-primary bg-transparent px-3 py-2 text-text-primary" /></label>
              </>
            )}
          </section>
          {metrics.data && (
            <section aria-label="Automation metrics" className="md:col-span-2 grid grid-cols-2 md:grid-cols-4 gap-3">
              {METRIC_TILES.map(tile => {
                const raw = (metrics.data as unknown as Record<string, unknown>)[tile.key]
                const value = typeof raw === 'number' ? raw : undefined
                const display = value == null ? '—' : tile.money ? usd(value) : value.toLocaleString()
                const color = value && value > 0 && tile.alert === 'bad' ? 'text-status-error' : value && value > 0 && tile.alert === 'warn' ? 'text-status-warning' : 'text-text-primary'
                return (
                  <div key={tile.key} className="rounded-[12px] border border-border-primary p-3">
                    <div className="text-xs text-text-tertiary">{tile.label}</div>
                    <div className={`mt-1 text-lg font-semibold tabular-nums ${color}`}>{display}</div>
                  </div>
                )
              })}
            </section>
          )}
        </div>
      )}

      {(showCreate || editing) && (
        <AutonomousAgentWizard
          open={showCreate || Boolean(editing)}
          editing={editing}
          templates={templates.data ?? []}
          onClose={() => { setShowCreate(false); setEditing(null) }}
        />
      )}

      <Modal open={Boolean(templateDetail)} onOpenChange={value => { if (!value) setTemplateDetail(null) }} size="lg">
        {templateDetail && (
          <>
            <ModalHeader>
              <ModalTitle>{templateDetail.name} · v{templateDetail.version}</ModalTitle>
            </ModalHeader>
            <ModalContent className="max-h-[65vh] overflow-y-auto space-y-4">
              <p className="text-sm text-text-secondary">{templateDetail.description}</p>
              <div>
                <p className="text-xs font-medium text-text-secondary">Workflow</p>
                <ol className="mt-2 space-y-1">{templateDetail.workflow.map((stepName, index) => <li key={stepName} className="flex items-center gap-2 text-sm text-text-primary"><span className="grid h-5 w-5 place-items-center rounded-full bg-white/[0.06] text-[10px] text-text-tertiary">{index + 1}</span>{stepName.replace(/_/g, ' ')}</li>)}</ol>
              </div>
              <div>
                <p className="text-xs font-medium text-text-secondary">Capabilities</p>
                <div className="mt-2 flex flex-wrap gap-1">{templateDetail.capabilities.map(cap => <Badge key={cap} size="sm" variant="info">{cap}</Badge>)}</div>
              </div>
              <div>
                <p className="text-xs font-medium text-text-secondary">Default budgets</p>
                <pre className="mt-2 overflow-auto rounded-lg bg-black/30 p-3 font-mono text-[11px] text-text-tertiary">{JSON.stringify(templateDetail.default_budgets, null, 2)}</pre>
              </div>
              <div>
                <p className="text-xs font-medium text-text-secondary">Configuration schema</p>
                <pre className="mt-2 overflow-auto rounded-lg bg-black/30 p-3 font-mono text-[11px] text-text-tertiary">{JSON.stringify(templateDetail.config_schema, null, 2)}</pre>
              </div>
              <p className="text-[11px] text-text-tertiary flex items-center gap-1"><X className="w-3 h-3" />Managed templates are versioned in the server and cannot be edited here — upgrades create a new revision and require revalidation.</p>
            </ModalContent>
          </>
        )}
      </Modal>

      {runJudge && (
        <JudgeRunDialog
          agent={runJudge}
          pending={runNow.isPending}
          onClose={() => setRunJudge(null)}
          onRun={targets => runNow.mutate({ id: runJudge.id, targets })}
        />
      )}

      {runReviewer && (
        <ReviewerRunDialog
          agent={runReviewer}
          pending={runNow.isPending}
          onClose={() => setRunReviewer(null)}
          onRun={target => runNow.mutate({ id: runReviewer.id, targets: [target] })}
        />
      )}

      {resolveWith && (
        <ResolveWithAgentDialog
          target={resolveWith}
          resolvers={allAgents.filter(a => a.template_key === 'github_issue_resolver' && a.status === 'enabled')}
          pending={resolveWithAgent.isPending}
          onClose={() => setResolveWith(null)}
          onRun={agentId => resolveWithAgent.mutate({ agentId, findingId: resolveWith.findingId, finding: resolveWith.finding, issue: resolveWith.issue })}
        />
      )}
    </div>
  )
}

/**
 * "Resolve with agent": pick ONE enabled issue-resolver to fix a single finding.
 * The finding content (and the GitHub issue it was filed as, when present) are
 * handed to that resolver, which resolves ONLY this — nothing else in the repo.
 */
function ResolveWithAgentDialog({ target, resolvers, pending, onClose, onRun }: { target: { title: string; issue?: { repository: string; number: number } }; resolvers: AutonomousAgentDefinition[]; pending: boolean; onClose: () => void; onRun: (agentId: string) => void }) {
  const [agentId, setAgentId] = useState('')
  useEffect(() => { if (!agentId && resolvers.length) setAgentId(resolvers[0].id) }, [resolvers, agentId])
  return (
    <Modal open onOpenChange={value => { if (!value) onClose() }} size="md">
      <ModalHeader>
        <ModalTitle>Resolve with agent</ModalTitle>
      </ModalHeader>
      <ModalContent className="space-y-4">
        <p className="text-[13px] text-text-secondary">Hand this finding to an issue-resolver. It will fix <span className="text-text-primary">only</span> this — nothing else in the repository.</p>
        <div className="rounded-lg border border-border-primary bg-white/[0.02] px-3 py-2 text-[13px] text-text-primary">{target.title}</div>
        {target.issue
          ? <p className="text-xs text-text-tertiary">Linked issue <span className="font-mono text-text-secondary">{target.issue.repository}#{target.issue.number}</span> — the pull request will close it.</p>
          : <p className="text-xs text-text-tertiary">No GitHub issue is linked; the finding itself is handed over as the task.</p>}
        {resolvers.length === 0
          ? <p className="text-xs text-status-warning">No enabled issue-resolver agent exists. Create and enable one first.</p>
          : (
            <label className="block">
              <span className="text-xs font-medium text-text-secondary">Resolver agent</span>
              <select value={agentId} onChange={e => setAgentId(e.target.value)} className="mt-1 block w-full rounded-lg border border-border-primary bg-transparent px-3 py-2 text-sm text-text-primary focus:border-accent-blue focus:outline-none">
                {resolvers.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
              </select>
            </label>
          )}
        <div className="flex justify-end gap-2 pt-2">
          <Button size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button size="sm" variant="primary" loading={pending} disabled={!agentId || resolvers.length === 0} onClick={() => agentId && onRun(agentId)}>Resolve</Button>
        </div>
      </ModalContent>
    </Modal>
  )
}

/**
 * Run dialog for the PR reviewer: the PR to review is chosen per run. Repository
 * defaults to the agent's configured `repository` (editable for multi-repo setups).
 */
function ReviewerRunDialog({ agent, pending, onClose, onRun }: { agent: AutonomousAgentDefinition; pending: boolean; onClose: () => void; onRun: (target: { repository: string; type: 'pr'; number: number }) => void }) {
  const client = useMemo(() => createClient(), [])
  const detail = useQuery({ queryKey: ['autonomous-agent-detail', agent.id], queryFn: () => client.getAutonomousAgent(agent.id) })
  const configRepo = (typeof detail.data?.revision?.config?.repository === 'string' ? detail.data?.revision?.config?.repository : '') as string
  const [repository, setRepository] = useState('')
  const [number, setNumber] = useState('')
  useEffect(() => { if (!repository && configRepo) setRepository(configRepo) }, [configRepo, repository])
  const parsed = Number(String(number).replace('#', '').trim())
  const valid = repository.trim().length > 0 && Number.isInteger(parsed) && parsed > 0
  return (
    <Modal open onOpenChange={value => { if (!value) onClose() }} size="md">
      <ModalHeader>
        <ModalTitle>Run “{agent.name}”</ModalTitle>
      </ModalHeader>
      <ModalContent className="space-y-4">
        <p className="text-[13px] text-text-secondary">Choose the pull request to review this run. Only this PR is reviewed; nothing is merged.</p>
        <label className="block">
          <span className="text-xs font-medium text-text-secondary">Repository</span>
          <Input inputSize="sm" className="mt-1 w-full" value={repository} onChange={e => setRepository(e.target.value)} placeholder="owner/repo" />
        </label>
        <label className="block">
          <span className="text-xs font-medium text-text-secondary">Pull request number</span>
          <Input inputSize="sm" className="mt-1 w-40" value={number} onChange={e => setNumber(e.target.value)} onKeyDown={e => { if (e.key === 'Enter' && valid) { e.preventDefault(); onRun({ repository: repository.trim(), type: 'pr', number: parsed }) } }} placeholder="123" />
        </label>
        <div className="flex justify-end gap-2 pt-2">
          <Button size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button size="sm" variant="primary" loading={pending} disabled={!valid} onClick={() => onRun({ repository: repository.trim(), type: 'pr', number: parsed })}>Review PR</Button>
        </div>
      </ModalContent>
    </Modal>
  )
}

/**
 * Run dialog for the Judge template: the PR/issue targets are chosen per run (not
 * baked into the agent), each scoped to one of the agent's configured repositories.
 */
function JudgeRunDialog({ agent, pending, onClose, onRun }: { agent: AutonomousAgentDefinition; pending: boolean; onClose: () => void; onRun: (targets: Array<{ repository: string; type: 'pr' | 'issue'; number: number }>) => void }) {
  const client = useMemo(() => createClient(), [])
  const detail = useQuery({ queryKey: ['autonomous-agent-detail', agent.id], queryFn: () => client.getAutonomousAgent(agent.id) })
  const repos = (Array.isArray(detail.data?.revision?.config?.repositories) ? detail.data?.revision?.config?.repositories : []) as string[]
  const [repository, setRepository] = useState('')
  const [type, setType] = useState<'pr' | 'issue'>('pr')
  const [number, setNumber] = useState('')
  const [targets, setTargets] = useState<Array<{ repository: string; type: 'pr' | 'issue'; number: number }>>([])
  useEffect(() => { if (!repository && repos.length) setRepository(repos[0]) }, [repos, repository])

  const add = () => {
    const n = Number(String(number).replace('#', '').trim())
    if (!repository || !Number.isInteger(n) || n <= 0) return
    if (targets.some(t => t.repository === repository && t.type === type && t.number === n)) { setNumber(''); return }
    setTargets(prev => [...prev, { repository, type, number: n }])
    setNumber('')
  }

  return (
    <Modal open onOpenChange={value => { if (!value) onClose() }} size="md">
      <ModalHeader>
        <ModalTitle>Run “{agent.name}”</ModalTitle>
      </ModalHeader>
      <ModalContent className="space-y-4">
        <p className="text-[13px] text-text-secondary">Choose the PRs / issues to judge this run. Each is verified against the live app, scoped to what it touches.</p>
        {repos.length === 0 && !detail.isLoading && <p className="text-xs text-status-warning">This agent has no configured repositories. Edit it to add some first.</p>}
        <div className="grid grid-cols-[1fr_auto_auto_auto] gap-2 items-end">
          <label className="block">
            <span className="text-xs font-medium text-text-secondary">Repository</span>
            <select value={repository} onChange={e => setRepository(e.target.value)} className="mt-1 block w-full rounded-lg border border-border-primary bg-transparent px-3 py-2 text-sm text-text-primary focus:border-accent-blue focus:outline-none">
              {repos.map(r => <option key={r} value={r}>{r}</option>)}
            </select>
          </label>
          <label className="block">
            <span className="text-xs font-medium text-text-secondary">Type</span>
            <select value={type} onChange={e => setType(e.target.value as 'pr' | 'issue')} className="mt-1 block rounded-lg border border-border-primary bg-transparent px-3 py-2 text-sm text-text-primary focus:border-accent-blue focus:outline-none">
              <option value="pr">PR</option>
              <option value="issue">Issue</option>
            </select>
          </label>
          <label className="block">
            <span className="text-xs font-medium text-text-secondary">Number</span>
            <Input inputSize="sm" className="w-24" value={number} onChange={e => setNumber(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); add() } }} placeholder="123" />
          </label>
          <Button size="sm" variant="secondary" onClick={add} disabled={!repository || !number.trim()}>Add</Button>
        </div>
        {targets.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {targets.map((t, i) => (
              <Badge key={`${t.repository}-${t.type}-${t.number}`} size="sm" variant="default">
                {t.repository} {t.type === 'pr' ? 'PR' : 'Issue'} #{t.number}
                <button type="button" className="ml-1.5 text-text-quaternary hover:text-text-primary" onClick={() => setTargets(prev => prev.filter((_, idx) => idx !== i))} aria-label="Remove target">×</button>
              </Badge>
            ))}
          </div>
        )}
        <div className="flex justify-end gap-2 pt-2">
          <Button size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button size="sm" variant="primary" loading={pending} disabled={targets.length === 0} onClick={() => onRun(targets)}>Run judge</Button>
        </div>
      </ModalContent>
    </Modal>
  )
}
