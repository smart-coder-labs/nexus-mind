// Pure helpers shared by the factory "operate" pages (agents, runs, findings,
// templates, settings). They turn raw run/finding payloads and enum values into
// words, so no raw enum ever reaches the UI (DESIGN_DIRECTION §11).

export type Dict = Record<string, unknown>
export type Tone = 'ok' | 'warn' | 'bad' | 'info' | 'neutral'
export type BadgeTone = 'default' | 'success' | 'warning' | 'error' | 'info' | 'purple' | 'primary'

export const asDict = (v: unknown): Dict | undefined => (v && typeof v === 'object' && !Array.isArray(v) ? (v as Dict) : undefined)
export const asNum = (v: unknown): number | undefined => (typeof v === 'number' && Number.isFinite(v) ? v : undefined)
export const asStr = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined)
export const asArr = (v: unknown): unknown[] | undefined => (Array.isArray(v) ? v : undefined)

/** `blocked_policy` → `Blocked policy`; the fallback for enums with no explicit label. */
export function humanize(value: string): string {
  const spaced = value.replace(/[_-]+/g, ' ').trim()
  return spaced ? spaced.charAt(0).toUpperCase() + spaced.slice(1).toLowerCase() : value
}

/**
 * Whether a finding is a LinkedIn post draft.
 *
 * The agent is asked for `kind: "post"` but the field is model-written and it
 * does omit it, while the `post` object it also emits is what actually carries
 * the body, destination and images. The backend already treats either as a post
 * (worker.rs `auto_publish_posts`), so a post could be published and still render
 * here as a generic bug — no `post` badge, no Published link, and "Create issue"
 * offered instead. Both sides must answer this question the same way.
 */
export function isPostEvidence(ev: Dict): boolean {
  return asStr(ev.kind) === 'post' || !!asDict(ev.post)
}

// Mirror the backend's `structured_result`: a QA run's findings live inside the
// model's final message (`result.result`, a JSON string that may be wrapped in
// ```json fences or prose), not as a direct property of the result object.
export function parseLenient(text: string): Dict | undefined {
  const unfenced = text.trim().replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '').trim()
  try { return asDict(JSON.parse(unfenced)) } catch { /* fall through */ }
  const start = unfenced.indexOf('{')
  const end = unfenced.lastIndexOf('}')
  if (start >= 0 && end > start) {
    try { return asDict(JSON.parse(unfenced.slice(start, end + 1))) } catch { /* give up */ }
  }
  return undefined
}

// ── Templates ──

const TEMPLATE_NAMES: Record<string, string> = {
  qa: 'QA',
  github_issue_resolver: 'Issue resolver',
  github_pr_reviewer: 'PR reviewer',
  lead_generation: 'Lead generation',
  judge: 'Judge',
  ai_content_manager: 'Content manager',
  security_scan: 'Security scan',
  security_dast: 'Live security scan',
}
export const templateName = (key: string) => TEMPLATE_NAMES[key] ?? humanize(key)

/** Template workflow step ids → readable step names. */
const STEP_NAMES: Record<string, string> = {
  qa: 'QA',
  sast_scan: 'Static analysis',
  sca_audit: 'Dependency audit',
  draft_pr: 'Open draft PR',
  pin_head: 'Pin the PR head',
  head_recheck: 'Recheck the PR head',
  select_pr_issue: 'Select PR or issue',
  drive_live_app: 'Drive the live app',
  comment_or_request_changes: 'Comment or request changes',
}
export const stepName = (step: string) => STEP_NAMES[step] ?? humanize(step)

// ── Runs ──

export const ACTIVE_RUN_STATUSES = ['queued', 'leased', 'running']
export const isActiveRun = (status: string) => ACTIVE_RUN_STATUSES.includes(status)
export const CONTINUABLE_RUN_STATUSES = ['budget_exhausted', 'partial', 'blocked_policy', 'failed', 'cancelled']

/** Run status → word, badge tone and color tone. */
const RUN_STATUS: Record<string, { label: string; variant: BadgeTone; tone: Tone }> = {
  queued: { label: 'Queued', variant: 'default', tone: 'neutral' },
  leased: { label: 'Starting', variant: 'info', tone: 'info' },
  running: { label: 'Running', variant: 'info', tone: 'info' },
  succeeded: { label: 'Succeeded', variant: 'success', tone: 'ok' },
  partial: { label: 'Partly done', variant: 'warning', tone: 'warn' },
  budget_exhausted: { label: 'Out of budget', variant: 'warning', tone: 'warn' },
  blocked_runtime: { label: 'Blocked by runtime', variant: 'error', tone: 'bad' },
  blocked_policy: { label: 'Blocked by policy', variant: 'error', tone: 'bad' },
  failed: { label: 'Failed', variant: 'error', tone: 'bad' },
  cancelled: { label: 'Cancelled', variant: 'default', tone: 'neutral' },
  dead_letter: { label: 'Gave up after retries', variant: 'error', tone: 'bad' },
}
export const runStatusMeta = (s: string) => RUN_STATUS[s] ?? { label: humanize(s), variant: 'default' as BadgeTone, tone: 'neutral' as Tone }

/** The five outcome classes of the outcome tape (§11), plus in-progress. */
export type Outcome = 'success' | 'warning' | 'error' | 'neutral' | 'active'
export function runOutcome(status: string): Outcome {
  if (isActiveRun(status)) return 'active'
  if (status === 'succeeded') return 'success'
  if (status === 'partial' || status === 'budget_exhausted') return 'warning'
  if (status === 'failed' || status === 'dead_letter' || status.startsWith('blocked')) return 'error'
  return 'neutral'
}

const TRIGGERS: Record<string, string> = {
  manual: 'Manual',
  schedule: 'Scheduled',
  github_webhook: 'GitHub event',
  webhook: 'Webhook',
  intake: 'Intake',
}
export const triggerName = (kind: string) => TRIGGERS[kind] ?? humanize(kind)

/** run.finished result `code` → a plain-language clause completing "This run …". */
export const RESULT_CODE: Record<string, string> = {
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
  nexus_executor_not_ready: 'needs its Nexus OpenShell executor and Claude login provisioned',
  nexus_auth_required: 'needs Claude login inside its OpenShell sandbox',
  nexus_gateway_unavailable: 'could not reach the OpenShell gateway',
  nexus_cli_unavailable: 'could not start the Nexus worker executable',
  nexus_sync_failed: 'could not safely synchronize OpenShell changes',
  nexus_runtime_failed: 'Nexus execution failed inside OpenShell',
  unsupported_template: 'used an unsupported template',
}

/** Result codes that mean the run never got a working sandbox/runtime. */
export const SANDBOX_FAILURE_CODES = new Set([
  'sandbox_create_failed', 'sandbox_environment_failed', 'claude_auth_required', 'claude_runtime_unavailable',
  'nexus_executor_not_ready', 'nexus_auth_required', 'nexus_gateway_unavailable', 'nexus_cli_unavailable',
])

// ── Runtime / settings ──

const RUNTIME_STATUS: Record<string, { label: string; variant: BadgeTone }> = {
  ready: { label: 'Ready', variant: 'success' },
  degraded: { label: 'Degraded', variant: 'warning' },
  reauth_required: { label: 'Needs re-authentication', variant: 'warning' },
  unavailable: { label: 'Unavailable', variant: 'error' },
}
export const runtimeStatusMeta = (s?: string | null) => (s ? RUNTIME_STATUS[s] ?? { label: humanize(s), variant: 'default' as BadgeTone } : { label: 'Not checked yet', variant: 'default' as BadgeTone })

// ── Findings ──

export const SEVERITY_ORDER = ['critical', 'high', 'medium', 'low', 'info']
export const severityLabel = (s: string) => humanize(s)
export const severityVariant = (s: string): BadgeTone => (s === 'critical' || s === 'high' ? 'error' : s === 'medium' ? 'warning' : s === 'low' ? 'info' : 'default')
export const severityDot = (s: string) => (s === 'critical' || s === 'high' ? 'bg-status-error' : s === 'medium' ? 'bg-status-warning' : s === 'low' ? 'bg-status-info' : 'bg-text-tertiary')

const FINDING_STATUS: Record<string, { label: string; variant: BadgeTone }> = {
  open: { label: 'Open', variant: 'default' },
  resolved: { label: 'Resolved', variant: 'success' },
  ignored: { label: 'Archived', variant: 'warning' },
}
export const findingStatusMeta = (s: string) => FINDING_STATUS[s] ?? { label: humanize(s), variant: 'default' as BadgeTone }

/** Evidence classes (`sast`, `dast`, `functional`…) — short acronyms stay upper case. */
export const evidenceKindLabel = (k: string) => (k.length <= 4 ? k.toUpperCase() : humanize(k))

const CHANNELS: Record<string, string> = { slack: 'Slack', github_issue: 'GitHub issue', linkedin: 'LinkedIn', email: 'Email', webhook: 'Webhook' }
export const channelName = (c: string) => CHANNELS[c] ?? humanize(c)
const DELIVERY_STATUS: Record<string, { label: string; tone: Tone }> = {
  sent: { label: 'Sent', tone: 'ok' },
  delivered: { label: 'Delivered', tone: 'ok' },
  pending: { label: 'Pending', tone: 'warn' },
  failed: { label: 'Failed', tone: 'bad' },
  dead_letter: { label: 'Gave up after retries', tone: 'bad' },
}
export const deliveryStatusMeta = (s: string) => DELIVERY_STATUS[s] ?? { label: humanize(s), tone: 'neutral' as Tone }

// ── Numbers and time ──

export const usd = (n: number) => `$${n.toFixed(2)}`
export const dur = (s: number) => {
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = Math.round(s % 60)
  if (h > 0) return `${h}h ${m}m`
  return m > 0 ? `${m}m ${sec}s` : `${sec}s`
}
export const round = (n: number) => String(Math.round(n))
/** Backend timestamps are UTC without a zone suffix. */
export const parseUtc = (iso?: string | null) => (iso ? Date.parse(`${iso.replace(' ', 'T')}Z`) : NaN)
export const fmtTime = (iso?: string | null) => {
  const t = parseUtc(iso)
  return Number.isFinite(t) ? new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : ''
}
export const fmtDateTime = (iso?: string | null) => {
  const t = parseUtc(iso)
  return Number.isFinite(t) ? new Date(t).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : ''
}

export function relativeTime(iso?: string | null, now = Date.now()): string {
  const t = parseUtc(iso)
  if (!Number.isFinite(t)) return ''
  const diff = Math.max(0, now - t) / 1000
  if (diff < 60) return 'just now'
  if (diff < 3600) { const m = Math.floor(diff / 60); return `${m} minute${m === 1 ? '' : 's'} ago` }
  if (diff < 86_400) { const h = Math.floor(diff / 3600); return `${h} hour${h === 1 ? '' : 's'} ago` }
  const d = Math.floor(diff / 86_400)
  if (d < 30) return `${d} day${d === 1 ? '' : 's'} ago`
  return new Date(t).toLocaleDateString([], { month: 'short', day: 'numeric' })
}

/** Local-day bucket label for grouping lists: "Today", "Yesterday", or a date. */
export function dayLabel(iso?: string | null, now = new Date()): string {
  const t = parseUtc(iso)
  if (!Number.isFinite(t)) return 'Unknown date'
  const d = new Date(t)
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const days = Math.round((startOf(now) - startOf(d)) / 86_400_000)
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  return d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric', ...(d.getFullYear() !== now.getFullYear() ? { year: 'numeric' } : {}) })
}

export function runDurationSeconds(run: { started_at: string | null; finished_at: string | null }): number | undefined {
  const s = parseUtc(run.started_at)
  const f = parseUtc(run.finished_at)
  return Number.isFinite(s) && Number.isFinite(f) ? Math.max(0, (f - s) / 1000) : undefined
}

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`
