import { useState, type ReactNode } from 'react'
import { AlertTriangle, Archive, ArchiveRestore, Ban, Camera, CheckCircle2, Circle, ExternalLink, GitPullRequest, Loader2, MessagesSquare, RefreshCw, XCircle } from 'lucide-react'
import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import { ConfirmModal } from '../../../components/ConfirmModal'
import type { AutonomousAgentEvent, AutonomousAgentRun, StoredVerificationReport } from '../../../types'
import { VerificationReports } from '../VerificationReports'
import {
  CONTINUABLE_RUN_STATUSES, RESULT_CODE, SANDBOX_FAILURE_CODES, asArr, asDict, asNum, asStr, dur, fmtDateTime, fmtTime, humanize,
  isActiveRun, parseLenient, round, runDurationSeconds, runStatusMeta, templateName, triggerName, usd, type Dict, type Tone,
} from '../shared/format'
import { OverflowMenu, RawJson, RunStatusPill, SectionTitle, TEXT_LINK, type MenuItem } from '../shared/ui'
import { TranscriptView } from './Transcript'

const outcomeToneClass: Record<Tone, string> = {
  ok: 'border-status-success/25 bg-status-success/[0.06]',
  warn: 'border-status-warning/25 bg-status-warning/[0.06]',
  bad: 'border-status-error/25 bg-status-error/[0.06]',
  info: 'border-status-info/30 bg-status-info/[0.08]',
  neutral: 'border-border-primary bg-white/[0.02]',
}

// ── Budget meter ──

/** One budget gauge: consumed vs. the configured limit, green → amber → red. */
function Meter({ label, used, max, format }: { label: string; used?: number; max: number; format: (n: number) => string }) {
  const pct = max > 0 && used != null ? Math.min(100, Math.round((used / max) * 100)) : 0
  const fill = pct >= 95 ? 'bg-status-error' : pct >= 70 ? 'bg-status-warning' : 'bg-status-success'
  const valColor = used == null ? 'text-text-tertiary' : pct >= 95 ? 'text-status-error' : pct >= 70 ? 'text-status-warning' : 'text-text-primary'
  return (
    <div className="min-w-0">
      <div className="mb-1.5 flex items-baseline justify-between gap-2">
        <span className="text-[12px] text-text-tertiary">{label}</span>
        <span className={`text-[13px] font-semibold tabular-nums ${valColor}`}>{used != null ? format(used) : '—'}</span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-white/[0.06]" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct} aria-valuetext={used != null ? `${format(used)} of ${format(max)}` : 'Not started'}>
        <div className={`h-full rounded-full ${fill}`} style={{ width: `${pct}%` }} />
      </div>
      <div className="mt-1 text-[12px] text-text-tertiary tabular-nums">of {format(max)} max{used != null ? ` · ${pct}% used` : ''}</div>
    </div>
  )
}

// ── Stage strip: Queued → Sandbox → Agent → Verification → Publish ──

type StageState = 'done' | 'current' | 'failed' | 'warn' | 'cancelled' | 'pending' | 'skipped'
type Stage = { name: string; state: StageState; note?: string }

const STAGE_STYLE: Record<StageState, { bar: string; icon: ReactNode; word: string }> = {
  done: { bar: 'bg-status-success', icon: <CheckCircle2 className="h-3.5 w-3.5 text-status-success" aria-hidden />, word: 'Done' },
  current: { bar: 'bg-status-info', icon: <Loader2 className="h-3.5 w-3.5 animate-spin text-text-primary motion-reduce:animate-none" aria-hidden />, word: 'In progress' },
  failed: { bar: 'bg-status-error', icon: <XCircle className="h-3.5 w-3.5 text-status-error" aria-hidden />, word: 'Failed' },
  warn: { bar: 'bg-status-warning', icon: <AlertTriangle className="h-3.5 w-3.5 text-status-warning" aria-hidden />, word: 'Stopped early' },
  cancelled: { bar: 'bg-text-tertiary', icon: <Ban className="h-3.5 w-3.5 text-text-tertiary" aria-hidden />, word: 'Cancelled' },
  pending: { bar: 'bg-white/[0.08]', icon: <Circle className="h-3.5 w-3.5 text-text-tertiary" aria-hidden />, word: 'Not reached' },
  skipped: { bar: 'bg-white/[0.08]', icon: <Circle className="h-3.5 w-3.5 text-text-tertiary" aria-hidden />, word: 'Not run' },
}

export function runStages(run: AutonomousAgentRun, events: AutonomousAgentEvent[], reports: StoredVerificationReport[], code?: string): Stage[] {
  const names = ['Queued', 'Sandbox', 'Agent', 'Verification', 'Publish']
  const active = isActiveRun(run.status)
  const s: Stage[] = names.map(name => ({ name, state: 'pending' as StageState, note: active ? 'Waiting' : undefined }))
  const set = (i: number, state: StageState, note?: string) => { s[i] = { name: names[i], state, note } }
  const doneThrough = (n: number) => { for (let i = 0; i < n; i += 1) set(i, 'done') }
  const verification: StageState = reports.length ? (reports.every(r => r.report.passed) ? 'done' : 'failed') : 'skipped'
  const started = Boolean(run.started_at) || events.some(e => e.kind === 'run.started')
  const sandboxFailed = run.status === 'blocked_runtime' || (code != null && SANDBOX_FAILURE_CODES.has(code))

  switch (run.status) {
    case 'queued': set(0, 'current'); break
    case 'leased': doneThrough(1); set(1, 'current', 'Starting'); break
    case 'running': doneThrough(2); set(2, 'current'); break
    case 'succeeded': doneThrough(3); set(3, verification); set(4, 'done'); break
    case 'partial': doneThrough(3); set(3, verification); set(4, 'warn', 'Partly done'); break
    case 'budget_exhausted': doneThrough(2); set(2, 'warn', 'Out of budget'); break
    case 'blocked_policy': doneThrough(3); set(3, verification); set(4, 'failed', 'Held by policy'); break
    case 'cancelled':
      if (started) { doneThrough(2); set(2, 'cancelled') } else set(0, 'cancelled')
      break
    default: // failed, blocked_runtime, dead_letter, anything new
      if (sandboxFailed) { doneThrough(1); set(1, 'failed', run.status === 'blocked_runtime' ? 'Runtime unavailable' : undefined) }
      else if (verification === 'failed') { doneThrough(3); set(3, 'failed') }
      else { doneThrough(2); set(2, 'failed') }
  }
  return s
}

function StageStrip({ stages }: { stages: Stage[] }) {
  return (
    <ol aria-label="Run stages" className="grid list-none grid-cols-2 gap-2 p-0 sm:grid-cols-5">
      {stages.map(stage => {
        const style = STAGE_STYLE[stage.state]
        const word = stage.note ?? style.word
        return (
          <li key={stage.name} className="min-w-0" aria-label={`${stage.name}: ${word}`}>
            <div className={`h-1 rounded-full ${style.bar}`} aria-hidden />
            <div className="mt-2 flex items-center gap-1.5 text-[13px] font-medium text-text-primary">{style.icon}{stage.name}</div>
            <div className="mt-0.5 text-[12px] text-text-tertiary">{word}</div>
          </li>
        )
      })}
    </ol>
  )
}

// ── Event log ──

function eventLine(e: AutonomousAgentEvent, run: AutonomousAgentRun, code?: string): string {
  const p = asDict(e.payload) ?? {}
  switch (e.kind) {
    case 'run.started': {
      const worker = asStr(p.worker) ?? 'local'
      return `Started on the ${worker} worker${run.snapshot_sha ? ` at snapshot ${run.snapshot_sha.slice(0, 7)}` : ''}`
    }
    case 'model.selected': {
      const model = asStr(p.model); const tier = asStr(p.tier)
      return `Chose ${model ?? 'a model'}${tier ? ` (${humanize(tier).toLowerCase()} tier)` : ''}`
    }
    case 'run.requeued': return `Put back in the queue${asStr(p.reason) ? `: ${humanize(asStr(p.reason)!).toLowerCase()}` : ''}`
    case 'run.cancelled': return 'Cancelled on request; partial work discarded'
    case 'run.finished': return code && RESULT_CODE[code] ? `Agent ${RESULT_CODE[code]}` : `Finished: ${runStatusMeta(run.status).label.toLowerCase()}`
    default: return humanize(e.kind.replace(/\./g, ' '))
  }
}

// ── Result cards ──

function Stat({ value, label, tone }: { value: ReactNode; label: string; tone?: 'ok' | 'bad' }) {
  return (
    <div className="rounded-[11px] border border-border-primary bg-white/[0.02] px-3 py-2">
      <div className={`text-[15px] font-semibold tabular-nums ${tone === 'ok' ? 'text-status-success' : tone === 'bad' ? 'text-status-error' : 'text-text-primary'}`}>{value}</div>
      <div className="text-[12px] text-text-tertiary">{label}</div>
    </div>
  )
}

const REVIEW_EVENT: Record<string, { label: string; variant: 'warning' | 'success' | 'default' }> = {
  REQUEST_CHANGES: { label: 'Requested changes', variant: 'warning' },
  APPROVE: { label: 'Approved', variant: 'success' },
  COMMENT: { label: 'Commented', variant: 'default' },
}

export type RunDetailProps = {
  run: AutonomousAgentRun
  events: AutonomousAgentEvent[]
  transcript: AutonomousAgentEvent[]
  verificationReports?: StoredVerificationReport[]
  agentName?: string
  templateKey?: string
  onOpenFindings?: () => void
  onContinue?: (id: string) => void
  continuing?: boolean
  onCancel?: (id: string) => void
  cancelling?: boolean
  onArchive?: (id: string) => void
  onRestore?: (id: string) => void
}

/** The run's story: header, outcome sentence, stages, budget, evidence, conversation, raw payload last. */
export function RunDetail({ run, events, transcript, verificationReports = [], agentName, templateKey, onOpenFindings, onContinue, continuing, onCancel, cancelling, onArchive, onRestore }: RunDetailProps) {
  const [confirmCancel, setConfirmCancel] = useState(false)
  const active = isActiveRun(run.status)
  const canContinue = Boolean(onContinue) && CONTINUABLE_RUN_STATUSES.includes(run.status)
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
  const durationSec = runDurationSeconds(run)

  const pr = asDict(pub.draft_pull_request)
  const prNumber = asNum(pr?.number)
  const prUrl = asStr(pr?.html_url)
  const review = asDict(pub.github_review)
  const reviewUrl = asStr(review?.html_url)
  const reviewEvent = asStr(pub.event) ?? asStr(review?.event)
  const reviewMeta = reviewEvent ? REVIEW_EVENT[reviewEvent] ?? { label: humanize(reviewEvent), variant: 'default' as const } : undefined

  // Outcome sentence, assembled only from fields that are actually present.
  const lead = agentName || `${triggerName(run.trigger_kind)} run`
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
  if (prNumber != null) tail.push(`Opened draft PR #${prNumber}.`)
  if (reviewMeta) tail.push(`Review posted: ${reviewMeta.label.toLowerCase()}.`)
  if (run.status === 'budget_exhausted' || code === 'cost_limit_exceeded' || code === 'wall_time_exceeded') tail.push('Raise the budget or narrow the scope, then continue or run again.')
  if (run.status === 'blocked_runtime' || code === 'claude_runtime_unavailable' || code === 'claude_auth_required') tail.push('No budget was spent. Fix the runtime in Factory settings and this run is picked up again automatically.')

  const meters = [
    maxCost ? <Meter key="cost" label="Cost" used={cost} max={maxCost} format={usd} /> : null,
    wall ? <Meter key="time" label="Wall time" used={durationSec} max={wall} format={dur} /> : null,
    maxFiles ? <Meter key="files" label="Files changed" used={filesChanged} max={maxFiles} format={round} /> : null,
  ].filter(Boolean)

  const lc = asDict(pub.lines_changed)
  const linesAdded = asNum(lc?.added)
  const linesRemoved = asNum(lc?.removed)
  const linesTotal = asNum(pub.lines_changed)
  const verification = asStr(pub.verification) ?? (asDict(pub.verification) ? 'Recorded' : undefined)

  // One primary action: open the PR it produced, else continue, else cancel.
  let primary: ReactNode = null
  if (pr && prUrl) primary = <a href={prUrl} target="_blank" rel="noreferrer" className="inline-flex h-8 items-center gap-2 rounded-full bg-accent-blue px-4 text-[12px] font-semibold text-white transition-colors hover:bg-accent-blue-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring"><GitPullRequest className="h-3.5 w-3.5" aria-hidden />{prNumber != null ? `Open PR #${prNumber}` : 'Open draft PR'}<ExternalLink className="h-3 w-3" aria-hidden /></a>
  else if (canContinue) primary = <Button size="sm" variant="primary" leftIcon={<RefreshCw className="h-3.5 w-3.5" />} loading={continuing} onClick={() => onContinue?.(run.id)}>Continue</Button>
  else if (active && onCancel) primary = <Button size="sm" variant="destructive" leftIcon={<Ban className="h-3.5 w-3.5" />} loading={cancelling} onClick={() => setConfirmCancel(true)}>Cancel run</Button>

  const menu: MenuItem[] = []
  if (pr && prUrl && canContinue) menu.push({ label: 'Continue run', icon: <RefreshCw className="h-3.5 w-3.5" />, onSelect: () => onContinue?.(run.id) })
  if (!active && run.archived_at && onRestore) menu.push({ label: 'Restore run', icon: <ArchiveRestore className="h-3.5 w-3.5" />, onSelect: () => onRestore(run.id) })
  if (!active && !run.archived_at && onArchive) menu.push({ label: 'Archive run', icon: <Archive className="h-3.5 w-3.5" />, onSelect: () => onArchive(run.id) })

  const label = 'text-[12px] font-medium text-text-tertiary'

  return (
    <div className="space-y-6">
      {/* header */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{agentName ?? `${triggerName(run.trigger_kind)} run`}</h2>
            <RunStatusPill status={run.status} />
            {run.archived_at && <Badge size="sm" variant="default">Archived</Badge>}
          </div>
          <dl className="mt-2 flex flex-wrap gap-x-5 gap-y-1 text-[12px] tabular-nums">
            {templateKey && <div className="flex gap-1.5"><dt className="text-text-tertiary">Template</dt><dd className="text-text-secondary">{templateName(templateKey)}</dd></div>}
            <div className="flex gap-1.5"><dt className="text-text-tertiary">Trigger</dt><dd className="text-text-secondary">{triggerName(run.trigger_kind)}</dd></div>
            {run.started_at && <div className="flex gap-1.5"><dt className="text-text-tertiary">Started</dt><dd className="text-text-secondary">{fmtDateTime(run.started_at)}</dd></div>}
            {durationSec != null && <div className="flex gap-1.5"><dt className="text-text-tertiary">Ran</dt><dd className="text-text-secondary">{dur(durationSec)}</dd></div>}
            {run.snapshot_sha && <div className="flex gap-1.5"><dt className="text-text-tertiary">Snapshot</dt><dd className="font-mono text-text-secondary">{run.snapshot_sha.slice(0, 7)}</dd></div>}
          </dl>
        </div>
        <div className="flex items-center gap-2">
          {primary}
          <OverflowMenu label="More run actions" items={menu} />
        </div>
      </div>

      {/* outcome sentence */}
      <p className={`rounded-[11px] border px-4 py-3 text-[13px] leading-relaxed text-text-secondary ${outcomeToneClass[meta.tone]}`}>
        <span className="font-semibold text-text-primary">{lead}</span> {phrase}
        {bits.length ? <> and used {bits.join(', ')}</> : null}. {tail.join(' ')}
        {code && !RESULT_CODE[code] && <> Result: {humanize(code).toLowerCase()}.</>}
      </p>

      <StageStrip stages={runStages(run, events, verificationReports, code)} />

      {meters.length > 0 && (
        <section aria-label="Budget used" className="space-y-3">
          <p className={label}>Budget used</p>
          <div className="grid gap-5 sm:grid-cols-3">{meters}</div>
        </section>
      )}

      {/* F1 verification gate: commands run in the sandbox + CI checks, per head */}
      <VerificationReports reports={verificationReports} />

      {/* QA result */}
      {(findings?.length || screenshots) && (
        <section aria-labelledby="run-result-qa" className="space-y-2">
          <SectionTitle id="run-result-qa">Result</SectionTitle>
          <div className="rounded-[18px] border border-border-primary bg-white/[0.02] p-5">
            <div className="mb-3 flex items-center gap-2 text-[13px] font-semibold text-text-primary">
              <Camera className="h-4 w-4 text-text-secondary" aria-hidden />
              {findings?.length ?? 0} finding{(findings?.length ?? 0) === 1 ? '' : 's'}
              {screenshots && <span className="font-normal text-text-tertiary">and {Object.keys(screenshots).length} screenshot{Object.keys(screenshots).length === 1 ? '' : 's'}</span>}
            </div>
            {screenshots && (
              <div className="flex flex-wrap gap-2.5">
                {Object.entries(screenshots).slice(0, 6).map(([name, url]) => {
                  // Prefer the stable re-signing endpoint (durable) over the raw
                  // presigned URL baked into the result, which expires after 7 days.
                  const src = run.id
                    ? `${import.meta.env.VITE_API_URL ?? ''}/evidence/${encodeURIComponent(run.id)}/${encodeURIComponent(name)}`
                    : (typeof url === 'string' ? url : undefined)
                  return (
                    <a key={name} href={src} target="_blank" rel="noreferrer" className="w-28 rounded-[8px] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
                      {src
                        ? <img src={src} alt={`Screenshot ${name}`} className="h-[70px] w-full rounded-[8px] border border-border-primary object-cover" />
                        : <div className="h-[70px] rounded-[8px] border border-border-primary bg-white/[0.03]" />}
                      <div className="mt-1 truncate text-center text-[11px] text-text-tertiary">{name}</div>
                    </a>
                  )
                })}
              </div>
            )}
            {onOpenFindings && <button type="button" onClick={onOpenFindings} className={`mt-3 ${TEXT_LINK}`}>Open in Findings</button>}
          </div>
        </section>
      )}

      {/* github_issue_resolver result */}
      {pr && (
        <section aria-labelledby="run-result-pr" className="space-y-2">
          <SectionTitle id="run-result-pr">Pull request</SectionTitle>
          <div className="space-y-3 rounded-[18px] border border-border-primary bg-white/[0.02] p-5">
            <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
              {filesChanged != null && <Stat value={filesChanged} label="Files changed" />}
              {linesAdded != null && <Stat value={`+${linesAdded}`} label="Lines added" tone="ok" />}
              {linesRemoved != null && <Stat value={`-${linesRemoved}`} label="Lines removed" tone="bad" />}
              {linesTotal != null && <Stat value={linesTotal} label="Lines changed" />}
              {verification && <Stat value={humanize(verification)} label="Verification" />}
            </div>
            <p className="flex flex-wrap items-center gap-2 text-[13px] text-text-secondary">
              <GitPullRequest className="h-4 w-4 text-text-secondary" aria-hidden />
              {prUrl ? <a href={prUrl} target="_blank" rel="noreferrer" className={TEXT_LINK}>{prNumber != null ? `Draft PR #${prNumber}` : 'Draft PR'}</a> : <span>{prNumber != null ? `Draft PR #${prNumber}` : 'Draft PR'}</span>}
              <Badge size="sm" variant="default">Draft</Badge>
            </p>
          </div>
        </section>
      )}

      {/* github_pr_reviewer result */}
      {(review || reviewEvent) && !pr && (
        <section aria-labelledby="run-result-review" className="space-y-2">
          <SectionTitle id="run-result-review">Review</SectionTitle>
          <div className="flex flex-wrap items-center gap-2 rounded-[18px] border border-border-primary bg-white/[0.02] p-5 text-[13px]">
            <GitPullRequest className="h-4 w-4 text-text-secondary" aria-hidden />
            {reviewUrl ? <a href={reviewUrl} target="_blank" rel="noreferrer" className={TEXT_LINK}>Review posted on GitHub</a> : <span className="text-text-primary">Review posted</span>}
            {reviewMeta && <Badge size="sm" variant={reviewMeta.variant}>{reviewMeta.label}</Badge>}
          </div>
        </section>
      )}

      {/* full agent conversation (streamed) */}
      <section aria-labelledby="run-conversation" className="space-y-3">
        <SectionTitle id="run-conversation" className="flex items-center gap-2">
          <MessagesSquare className="h-4 w-4 text-text-secondary" aria-hidden />Conversation
          {transcript.length > 0 && <span className="text-[12px] font-normal text-text-tertiary">{transcript.length} turns</span>}
        </SectionTitle>
        <TranscriptView turns={transcript} live={active} />
      </section>

      {/* event log */}
      <details className="border-t border-border-secondary pt-3">
        <summary className="cursor-pointer rounded-[8px] text-[12px] text-text-tertiary hover:text-text-secondary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">Event log ({events.length})</summary>
        {events.length ? (
          <ol className="mt-3 list-none space-y-1.5 p-0">
            {events.map(e => (
              <li key={e.sequence} className="flex gap-3 text-[13px]">
                <span className="w-12 shrink-0 text-[12px] text-text-tertiary tabular-nums">{fmtTime(e.created_at)}</span>
                <span className="text-text-secondary">{eventLine(e, run, code)}</span>
              </li>
            ))}
          </ol>
        ) : <p className="mt-2 text-[13px] text-text-tertiary">No events recorded yet.</p>}
      </details>

      {finished && <RawJson label="Raw result (JSON)" value={finished.payload} />}

      <ConfirmModal
        open={confirmCancel}
        danger
        title="Cancel this run?"
        description="The agent stops now and its partial work is discarded. You can continue it afterwards."
        confirmLabel="Cancel run"
        loading={cancelling}
        onClose={() => setConfirmCancel(false)}
        onConfirm={() => { onCancel?.(run.id); setConfirmCancel(false) }}
      />
    </div>
  )
}
