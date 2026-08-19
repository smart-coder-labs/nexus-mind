import { useMemo, useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Navigate } from 'react-router-dom'
import { AlertTriangle, Archive, Ban, Bot, Camera, CheckCircle2, Clock, Copy, ExternalLink, GitPullRequest, Loader2, Pencil, Play, Plus, RefreshCw, ShieldCheck, X, XCircle } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/ui/Button'
import { Badge } from '../components/ui/Badge'
import { Switch } from '../components/ui/Switch'
import { SegmentedControl } from '../components/ui/SegmentedControl'
import { EmptyState } from '../components/ui/EmptyState'
import { Modal, ModalHeader, ModalTitle, ModalContent } from '../components/ui/Modal'
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

// The star of the redesign: turns run.finished's raw JSON into a readable story.
function RunDetail({ run, events, agentName, templateKey, onOpenFindings }: { run: AutonomousAgentRun; events: AutonomousAgentEvent[]; agentName?: string; templateKey?: string; onOpenFindings?: () => void }) {
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
  const findings = asArr(result.findings)
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
        <Badge variant={meta.variant} dot>{meta.label}{code ? ` · ${code}` : ''}</Badge>
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

      {/* QA result */}
      {(findings?.length || screenshots) && (
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary mb-2">Result · QA</p>
          <div className="rounded-[12px] border border-border-primary bg-white/[0.02] p-4">
            <div className="flex items-center gap-2 mb-3 text-[13px] font-semibold text-text-primary"><Camera className="w-4 h-4 text-accent-blue" />{findings?.length ?? 0} finding{(findings?.length ?? 0) === 1 ? '' : 's'}{screenshots ? ` · ${Object.keys(screenshots).length} screenshot${Object.keys(screenshots).length === 1 ? '' : 's'}` : ''}</div>
            {screenshots && (
              <div className="flex gap-2.5 flex-wrap">
                {Object.entries(screenshots).slice(0, 6).map(([name, url]) => (
                  <a key={name} href={typeof url === 'string' ? url : undefined} target="_blank" rel="noreferrer" className="w-28">
                    {typeof url === 'string'
                      ? <img src={url} alt={name} className="h-[70px] w-full object-cover rounded-lg border border-border-primary" />
                      : <div className="h-[70px] rounded-lg border border-border-primary bg-white/[0.03]" />}
                    <div className="text-[10.5px] text-text-tertiary mt-1 text-center truncate">{name}</div>
                  </a>
                ))}
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
  const runs = useQuery({ queryKey: ['autonomous-runs'], queryFn: () => client.listAutonomousAgentRuns(), enabled: can('autonomous_agent:read') && tab === 'runs' })
  const events = useQuery({ queryKey: ['autonomous-run-events', selectedRun?.id], queryFn: () => client.listAutonomousAgentRunEvents(selectedRun!.id), enabled: can('autonomous_agent:read') && Boolean(selectedRun) })
  const runtime = useQuery({ queryKey: ['autonomous-runtime'], queryFn: () => client.getAutonomousRuntimeHealth(), enabled: can('autonomous_agent:read') && tab === 'runtime' })
  const settings = useQuery({ queryKey: ['autonomous-settings'], queryFn: () => client.getAutonomousAgentSettings(), enabled: can('autonomous_agent:read') && tab === 'runtime' })
  const metrics = useQuery({ queryKey: ['autonomous-metrics'], queryFn: () => client.getAutonomousAgentMetrics(), enabled: can('autonomous_agent:read') && tab === 'runtime', refetchInterval: 30_000 })
  const findings = useQuery({ queryKey: ['autonomous-findings'], queryFn: () => client.listAutonomousAgentFindings(), enabled: can('autonomous_agent:read') && tab === 'findings' })
  const deliveries = useQuery({ queryKey: ['autonomous-deliveries'], queryFn: () => client.listAutonomousAgentDeliveries(), enabled: can('autonomous_agent:read') && tab === 'findings' })

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
  const runNow = useMutation({ mutationFn: (id: string) => client.runAutonomousAgent(id), onSuccess: () => invalidate('autonomous-runs') })
  const cancelRun = useMutation({ mutationFn: (id: string) => client.cancelAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs') })
  const checkRuntime = useMutation({ mutationFn: () => client.checkAutonomousRuntimeHealth(), onSuccess: () => invalidate('autonomous-runtime') })
  const toggleOrg = useMutation({ mutationFn: (enabled: boolean) => client.patchAutonomousAgentSettings({ enabled }), onSuccess: () => invalidate('autonomous-settings', 'autonomous-runs') })
  const saveRetention = useMutation({ mutationFn: (days: number) => client.patchAutonomousAgentSettings({ retention_days: days }), onSuccess: () => invalidate('autonomous-settings') })
  const retryDelivery = useMutation({ mutationFn: (id: string) => client.retryAutonomousAgentDelivery(id), onSuccess: () => invalidate('autonomous-deliveries') })
  const resolveFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'resolved'), onSuccess: () => invalidate('autonomous-findings') })

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
                {can('autonomous_agent:run') && agent.status === 'enabled' && <Button size="sm" variant="primary" leftIcon={<Play className="w-3.5 h-3.5" />} onClick={() => runNow.mutate(agent.id)}>Run</Button>}
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

      {tab === 'runs' && (
        <div className="grid gap-4 lg:grid-cols-[minmax(280px,340px)_1fr]">
          <div className="rounded-[14px] border border-border-primary overflow-hidden self-start">
            <div className="flex items-center justify-between px-4 py-3 border-b border-border-primary">
              <span className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">Recent runs</span>
              <span className="text-xs text-text-tertiary">{runs.data?.length ?? 0}</span>
            </div>
            <div className="divide-y divide-border-secondary">
              {runs.data?.map(run => {
                const meta = runStatusMeta(run.status)
                const active = selectedRun?.id === run.id
                return (
                  <button type="button" key={run.id} onClick={() => setSelectedRun(run)} className={`w-full px-4 py-3 flex flex-col gap-1.5 text-left transition-colors ${active ? 'bg-accent-blue/[0.10] shadow-[inset_3px_0_0_var(--color-accent-blue)]' : 'hover:bg-white/[0.02]'}`}>
                    <div className="flex justify-between gap-2 items-center">
                      <span className="text-[13px] font-semibold text-text-primary truncate">{allAgents.find(a => a.id === run.definition_id)?.name ?? `${titleCase(run.trigger_kind)} run`}</span>
                      <Badge size="sm" variant={meta.variant} dot={run.status !== 'running'}>{run.status === 'running' ? <Loader2 className="w-3 h-3 animate-spin" /> : null}{meta.label}</Badge>
                    </div>
                    <div className="flex items-center gap-2 flex-wrap text-[11px] text-text-tertiary tabular-nums">
                      <span>{titleCase(run.trigger_kind)}</span>
                      <span>·</span>
                      <span>{new Date(`${run.created_at}Z`).toLocaleString()}</span>
                      {can('autonomous_agent:cancel') && ['queued', 'leased', 'running'].includes(run.status) && (
                        <span onClick={event => { event.stopPropagation(); cancelRun.mutate(run.id) }} className="ml-auto text-status-error font-semibold">Cancel</span>
                      )}
                    </div>
                  </button>
                )
              })}
              {runs.data?.length === 0 && <div className="p-4"><EmptyState title="No runs yet" description="Runs appear here once an enabled agent is triggered." /></div>}
            </div>
          </div>
          <aside className="rounded-[14px] border border-border-primary p-5">
            {selectedRun ? (
              <RunDetail run={runs.data?.find(r => r.id === selectedRun.id) ?? selectedRun} events={events.data ?? []} agentName={runAgent?.name} templateKey={runAgent?.template_key} onOpenFindings={() => setTab('findings')} />
            ) : <EmptyState title="Select a run" description="See the outcome, budget consumption, a readable timeline, and what the agent produced." />}
          </aside>
        </div>
      )}

      {tab === 'findings' && (
        <div className="space-y-3">
          {findings.data?.map(finding => {
            const ev = (finding.evidence ?? {}) as Dict
            const shot = asStr(ev.screenshot_url) ?? asStr(ev.screenshot)
            const location = ev.location
            const locDict = asDict(location)
            const locStr = asStr(location)
            const steps = asArr(ev.steps)
            const repro = asStr(ev.repro) ?? asStr(ev.excerpt) ?? asStr(ev.code)
            const findingDeliveries = deliveries.data?.filter(item => item.finding_id === finding.id) ?? []
            const hasStructured = shot || locDict || locStr || (steps && steps.length) || repro
            return (
              <article key={finding.id} className="rounded-[14px] border border-border-primary bg-white/[0.02] overflow-hidden grid grid-cols-[4px_1fr]">
                <div className={sevRail(finding.severity)} aria-hidden />
                <div className="p-4">
                  <div className="flex justify-between gap-3 items-start flex-wrap">
                    <h2 className="text-sm font-semibold text-text-primary">{finding.title}</h2>
                    <div className="flex items-center gap-2">
                      <Badge size="sm" variant={sevVariant(finding.severity)} dot>{finding.severity}</Badge>
                      <Badge size="sm" variant={finding.status === 'resolved' ? 'success' : 'default'}>{finding.status}</Badge>
                      {can('autonomous_agent:update') && finding.status === 'open' && <button type="button" onClick={() => resolveFinding.mutate(finding.id)} className="text-xs text-accent-blue font-medium">Resolve</button>}
                    </div>
                  </div>
                  <p className="text-sm text-text-tertiary mt-2 leading-relaxed">{finding.summary}</p>

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
          {findings.data?.length === 0 && <EmptyState title="No findings yet" description="Confirmed findings from QA and review agents will land here — with the screenshot, the reproduction, and where they were delivered." />}
        </div>
      )}

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
    </div>
  )
}
