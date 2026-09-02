import { useEffect, useMemo, useState } from 'react'
import { formatDistanceToNow } from 'date-fns'
import {
  Layers,
  Brain,
  BookMarked,
  ListTodo,
  FileText,
  GitBranch,
  Database,
  MessageSquare,
  Sparkles,
  SlidersHorizontal,
  ShieldCheck,
  ShieldX,
  Eye,
  Check,
  Copy,
  Clock,
  X,
  GitMerge,
  AlertCircle,
  CheckCircle2,
  Inbox,
  Trash2,
  type LucideIcon,
} from 'lucide-react'
import { createClient } from '../api/client'
import type {
  MigrationRun,
  MigrationCandidate,
  MigrationVerdict,
  MigrationCommitResponse,
  MigrationRunReport,
} from '../types'

/**
 * The human gate of the knowledge-migration pipeline.
 *
 * Everything a connector produces lands here as a candidate and stays there
 * until somebody says yes. The screen exists to make that "yes" an informed
 * one, which is why every candidate shows the verbatim source excerpt beside
 * the proposal: a reviewer must never have to open the original file to judge
 * what is being claimed on its behalf.
 */

// Same glass recipe used across Sessions/Sdd — inlined to keep pages independent.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

const DESTINATION_LABELS: Record<string, string> = {
  memory: 'Memory',
  convention: 'Convention',
  task: 'Task',
  sdd_artifact: 'SDD artifact',
  harness: 'Harness',
  harness_config_review: 'Config review',
}

const DESTINATION_ICONS: Record<string, LucideIcon> = {
  memory: Brain,
  convention: BookMarked,
  task: ListTodo,
  sdd_artifact: FileText,
  harness: Sparkles,
  harness_config_review: SlidersHorizontal,
}

/** A run is one connector execution against a source. The raw `source_kind`
 *  (`repo-docs`, `git-history`, …) is machine jargon — pair it with a human
 *  label and an icon so the card reads without a decoder ring. */
const SOURCE_LABELS: Record<string, string> = {
  'repo-docs': 'Repository docs',
  'git-history': 'Git history',
  'claude-memories': 'Claude memories',
  'db-schema': 'Database schema',
  noop: 'No-op',
}

const SOURCE_ICONS: Record<string, LucideIcon> = {
  'repo-docs': FileText,
  'git-history': GitBranch,
  'claude-memories': MessageSquare,
  'db-schema': Database,
  noop: Layers,
}

/** Semantic colour for a run's lifecycle status. Kept substring-based because
 *  the backend status string is free-form (`staging`, `committed`, …). */
function statusTone(s: string): string {
  const v = s.toLowerCase()
  if (v.includes('commit')) return 'bg-status-success/15 text-status-success'
  if (v.includes('stag')) return 'bg-accent-blue/15 text-accent-blue'
  if (v.includes('fail') || v.includes('error')) return 'bg-status-error/15 text-status-error'
  if (v.includes('cancel')) return 'bg-white/[0.06] text-text-tertiary'
  return 'bg-white/[0.06] text-text-tertiary'
}

function relTime(iso: string): string {
  try {
    return formatDistanceToNow(new Date(iso), { addSuffix: true })
  } catch {
    return iso
  }
}

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

/** Minimal JSON syntax highlighter — no dependency, no eval. HTML is escaped
 *  first so a string value like `"<script>"` can never inject markup; the only
 *  tags added afterwards are our own class-bearing spans. Colours literal so
 *  Tailwind's JIT keeps them. */
function highlightJson(value: unknown): string {
  const json = escapeHtml(JSON.stringify(value, null, 2) ?? 'null')
  return json.replace(
    /("(?:\\.|[^"\\])*"(\s*:)?)|(\b-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b)|\b(true|false|null)\b/g,
    (m: string, str: string, colon: string, num: string, lit: string) => {
      if (str !== undefined) {
        const cls = colon ? 'text-[#79c0ff]' : 'text-[#a5d6ff]'
        return `<span class="${cls}">${str}</span>`
      }
      if (num !== undefined) return `<span class="text-[#f0883e]">${num}</span>`
      if (lit !== undefined) return `<span class="text-[#ff7b72]">${lit}</span>`
      return m
    },
  )
}

/** Terminal-style JSON viewer: an inset dark panel with a mono label bar and a
 *  copy button. This is the "formatted / terminal" treatment the raw
 *  `JSON.stringify(<pre>)` never gave — legible without pretending to be an
 *  interactive shell the data has no use for. */
function JsonBlock({ label, value }: { label: string; value: unknown }) {
  const [copied, setCopied] = useState(false)
  const isEmpty =
    value == null ||
    (typeof value === 'object' &&
      !Array.isArray(value) &&
      Object.keys(value as Record<string, unknown>).length === 0)

  function copy() {
    void navigator.clipboard?.writeText(JSON.stringify(value, null, 2) ?? 'null')
    setCopied(true)
    setTimeout(() => setCopied(false), 1200)
  }

  return (
    <div className="overflow-hidden rounded-[12px] border border-white/[0.07] bg-[#0a0c10]">
      <div className="flex items-center justify-between border-b border-white/[0.06] px-3 py-1.5">
        <span className="flex items-center gap-1.5 font-mono text-[10px] text-text-tertiary">
          <span className="h-2 w-2 rounded-full bg-white/[0.12]" />
          {label}
        </span>
        {!isEmpty && (
          <button
            type="button"
            onClick={copy}
            aria-label={`Copy ${label}`}
            className="flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] text-text-tertiary transition-colors hover:bg-white/[0.06] hover:text-text-secondary"
          >
            {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            {copied ? 'Copied' : 'Copy'}
          </button>
        )}
      </div>
      {isEmpty ? (
        <p className="px-3 py-3 font-mono text-[11px] text-text-quaternary">{'{ }'} · empty</p>
      ) : (
        <pre
          className="overflow-x-auto px-3 py-3 font-mono text-[11px] leading-relaxed text-text-secondary"
          dangerouslySetInnerHTML={{ __html: highlightJson(value) }}
        />
      )}
    </div>
  )
}

function confidenceLabel(c?: number | null): string {
  if (c === null || c === undefined) return '—'
  return `${Math.round(c * 100)}%`
}

/** The classifier's score only orders the queue — it never authorizes. The
 *  colour is a reading aid, not a verdict. */
function confidenceTone(c?: number | null): string {
  if (c === null || c === undefined) return 'bg-white/[0.06] text-text-tertiary'
  if (c >= 0.8) return 'bg-status-success/15 text-status-success'
  if (c >= 0.5) return 'bg-accent-blue/15 text-accent-blue'
  return 'bg-status-warning/15 text-status-warning'
}

const PILL_BASE =
  'inline-flex items-center gap-1.5 rounded-full px-4 py-1.5 text-xs font-semibold transition-colors disabled:opacity-40 disabled:cursor-not-allowed'

export default function Migrations() {
  const api = useMemo(() => createClient(), [])
  const [runs, setRuns] = useState<MigrationRun[]>([])
  const [selectedRun, setSelectedRun] = useState<string | null>(null)
  const [candidates, setCandidates] = useState<MigrationCandidate[]>([])
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [openCandidate, setOpenCandidate] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [commitResult, setCommitResult] = useState<MigrationCommitResponse | null>(null)
  const [report, setReport] = useState<MigrationRunReport | null>(null)
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    api
      .listMigrationRuns({ limit: 50 })
      .then(setRuns)
      .catch((e: unknown) => setError(String(e)))
  }, [api])

  /** `reset` clears the previous run's messages. It is off when reloading after
   *  a review or a commit, because the caller has just written a result the
   *  reviewer needs to read — wiping it here is how that result disappears. */
  async function loadCandidates(runId: string, reset = true) {
    setLoading(true)
    if (reset) {
      setError(null)
      setNotice(null)
      setCommitResult(null)
      setReport(null)
    }
    try {
      const list = await api.listMigrationCandidates(runId, { limit: 200 })
      setCandidates(list)
      setSelectedRun(runId)
      setSelectedIds(new Set())
      setOpenCandidate(null)
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
    // The aggregate counts (staged/approved/committed/…) are what make a run
    // legible. Fetched separately and failure-tolerant: a missing report must
    // never blank the review queue the reviewer came here to act on.
    const pending = api.getMigrationReport?.(runId)
    if (pending) pending.then(setReport).catch(() => setReport(null))
  }

  /** Highest confidence first — the classifier's score orders the queue and
   *  never authorizes anything. Unscored candidates sink to the bottom. */
  const staged = useMemo(
    () =>
      candidates
        .filter((c) => c.status === 'staged')
        .slice()
        .sort((a, b) => (b.confidence ?? -1) - (a.confidence ?? -1)),
    [candidates],
  )

  const selected = useMemo(
    () => staged.filter((c) => selectedIds.has(c.id)),
    [staged, selectedIds],
  )

  /** Batch approval is refused by the backend when any selected candidate is
   *  `client_attested`. Surfacing that here means the reviewer learns it before
   *  clicking, not from a 409. */
  const attestedInSelection = useMemo(
    () => selected.filter((c) => c.provenance_kind === 'client_attested'),
    [selected],
  )
  const batchBlocked = selected.length > 1 && attestedInSelection.length > 0

  const selectionIncludesHarness = useMemo(
    () => selected.some((c) => c.destination_kind === 'harness'),
    [selected],
  )

  function toggle(id: string) {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  async function submit(verdict: MigrationVerdict) {
    if (!selectedRun || selected.length === 0) return
    setLoading(true)
    setError(null)
    setNotice(null)
    try {
      const resp = await api.reviewMigrationCandidates(
        selectedRun,
        selected.map((c) => ({
          candidate_id: c.id,
          action: verdict,
          expected_version: c.version,
        })),
      )
      if (resp.conflicts > 0) {
        // Someone else acted on the same candidate while this queue was open.
        // Reloading is the only honest resolution: the reviewer has to see what
        // it says now before deciding again.
        setError(
          `${resp.conflicts} candidate(s) changed while you were reviewing. ` +
            'The queue has been reloaded — please look again before deciding.',
        )
      } else {
        setNotice(`${resp.applied} candidate(s) ${verdict}.`)
      }
      await loadCandidates(selectedRun, false)
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  async function commit() {
    if (!selectedRun) return
    setLoading(true)
    setError(null)
    try {
      const resp = await api.commitMigrationRun(selectedRun)
      await loadCandidates(selectedRun, false)
      setCommitResult(resp)
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  // Hard-deletes the selected run. The backend refuses (409) any run that
  // committed knowledge, so the destructive reach here is limited to aborted
  // scans and test runs — exactly what needs clearing out.
  async function deleteRun() {
    if (!selectedRun) return
    const run = runs.find((r) => r.id === selectedRun)
    const label = run ? (SOURCE_LABELS[run.source_kind] ?? run.source_kind) : 'this run'
    const ok = window.confirm(
      `Delete ${label}${run?.source_ref ? ` (${run.source_ref})` : ''}?\n\n` +
        'This removes the run and its staged candidates. A run that already ' +
        'committed knowledge cannot be deleted — cancel it instead.',
    )
    if (!ok) return
    setLoading(true)
    setError(null)
    setNotice(null)
    try {
      await api.deleteMigrationRun(selectedRun)
      setSelectedRun(null)
      setCandidates([])
      setReport(null)
      setCommitResult(null)
      setNotice('Run deleted.')
      const list = await api.listMigrationRuns({ limit: 50 })
      setRuns(list)
    } catch (e: unknown) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  const open = candidates.find((c) => c.id === openCandidate) ?? null
  const approvedCount = candidates.filter((c) => c.status === 'approved').length

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6">
      {/* Header */}
      <header className="flex items-start gap-3">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[12px] bg-accent-blue/[0.12] text-accent-blue">
          <Layers className="h-5 w-5" />
        </div>
        <div>
          <h1 className="text-base font-semibold text-text-primary">Knowledge migration</h1>
          <p className="text-xs text-text-quaternary mt-0.5">
            Nothing here has entered the company brain yet. Every candidate is waiting for a
            decision.
          </p>
        </div>
      </header>

      {error && (
        <div
          role="alert"
          className="flex items-start gap-2.5 rounded-[14px] border border-status-error/30 bg-status-error/[0.08] p-3.5 text-xs text-status-error"
        >
          <AlertCircle className="h-4 w-4 shrink-0 mt-px" />
          <span className="leading-relaxed">{error}</span>
        </div>
      )}
      {notice && (
        <div
          role="status"
          className="flex items-start gap-2.5 rounded-[14px] border border-status-success/30 bg-status-success/[0.08] p-3.5 text-xs text-status-success"
        >
          <CheckCircle2 className="h-4 w-4 shrink-0 mt-px" />
          <span className="leading-relaxed">{notice}</span>
        </div>
      )}

      {/* Runs */}
      <section aria-label="Runs" className="space-y-3">
        <div className="flex items-baseline gap-2">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-text-tertiary">Runs</h2>
          <span className="text-[10px] text-text-quaternary">{runs.length}</span>
        </div>

        {runs.length === 0 ? (
          <div
            className={`flex flex-col items-center gap-2 rounded-[18px] py-12 text-center ${GLASS_PANEL}`}
          >
            <Inbox className="h-6 w-6 text-text-quaternary" />
            <p className="text-xs text-text-quaternary">
              No migration runs yet. A connector run will appear here once it scans a source.
            </p>
          </div>
        ) : (
          <div className="grid gap-2.5 sm:grid-cols-2">
            {runs.map((r) => {
              const active = selectedRun === r.id
              const SrcIcon = SOURCE_ICONS[r.source_kind] ?? Layers
              const label = SOURCE_LABELS[r.source_kind] ?? r.source_kind
              return (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => loadCandidates(r.id)}
                  aria-pressed={active}
                  aria-label={`Migration run ${r.source_kind}${r.source_ref ? ` (${r.source_ref})` : ''}, ${r.status}`}
                  className={`group flex flex-col gap-2.5 rounded-[16px] border p-4 text-left transition-colors ${
                    active
                      ? 'border-accent-blue/50 bg-accent-blue/[0.08]'
                      : `${GLASS_PANEL} hover:bg-white/[0.05]`
                  }`}
                >
                  <div className="flex items-start gap-3">
                    <span
                      className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] ${
                        active ? 'bg-accent-blue/20 text-accent-blue' : 'bg-white/[0.06] text-text-secondary'
                      }`}
                    >
                      <SrcIcon className="h-4 w-4" />
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-sm font-semibold text-text-primary">{label}</span>
                        <span
                          className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium capitalize ${statusTone(r.status)}`}
                        >
                          {r.status}
                        </span>
                      </div>
                      {r.source_ref && (
                        <p className="mt-1 truncate font-mono text-[11px] text-text-tertiary">
                          {r.source_ref}
                        </p>
                      )}
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-[10px] text-text-quaternary">
                    <span className="inline-flex items-center gap-1">
                      <Clock className="h-3 w-3" />
                      {relTime(r.created_at)}
                    </span>
                    <span className="text-text-tertiary/40">·</span>
                    <span
                      className={`rounded-full px-1.5 py-0.5 ${
                        r.client_id
                          ? 'bg-accent-purple/15 text-accent-purple'
                          : 'bg-white/[0.06] text-text-tertiary'
                      }`}
                    >
                      {r.client_id ? `client ${r.client_id}` : 'internal'}
                    </span>
                    {r.runner_version && (
                      <>
                        <span className="text-text-tertiary/40">·</span>
                        <span className="font-mono">runner {r.runner_version}</span>
                      </>
                    )}
                  </div>
                </button>
              )
            })}
          </div>
        )}
      </section>

      {selectedRun && (
        <section aria-label="Review queue" className="space-y-3">
          <div className="flex items-baseline gap-2">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-text-tertiary">
              Review queue
            </h2>
            <span className="text-[10px] text-text-quaternary">{staged.length} staged</span>
            <button
              type="button"
              onClick={deleteRun}
              disabled={loading}
              className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-status-error/30 bg-status-error/[0.08] px-2.5 py-1 text-[11px] font-medium text-status-error transition-colors hover:bg-status-error/[0.14] disabled:opacity-50"
              title="Delete this run and its candidates. A run that committed knowledge cannot be deleted."
            >
              <Trash2 className="h-3.5 w-3.5" />
              Delete run
            </button>
          </div>

          {/* Run summary — the aggregate counts that tell the reviewer where this
              run stands at a glance. Only rendered once the report loads. */}
          {report && (
            <div className={`flex flex-wrap gap-2 rounded-[14px] p-3 ${GLASS_PANEL}`}>
              {[
                { label: 'staged', value: report.staged, tone: 'text-accent-blue' },
                { label: 'approved', value: report.approved, tone: 'text-status-success' },
                { label: 'rejected', value: report.rejected, tone: 'text-status-error' },
                { label: 'committed', value: report.committed, tone: 'text-status-success' },
                { label: 'skipped', value: report.skipped, tone: 'text-text-tertiary' },
                { label: 'failed', value: report.failed, tone: 'text-status-error' },
              ].map((s) => (
                <div
                  key={s.label}
                  className="flex min-w-[64px] flex-col items-center rounded-[10px] bg-white/[0.03] px-3 py-1.5"
                >
                  <span className={`text-base font-semibold tabular-nums ${s.tone}`}>{s.value}</span>
                  <span className="text-[10px] uppercase tracking-wide text-text-quaternary">
                    {s.label}
                  </span>
                </div>
              ))}
              {report.pending_index > 0 && (
                <div className="flex items-center gap-1.5 rounded-[10px] bg-status-warning/10 px-3 py-1.5 text-[11px] text-status-warning">
                  <Clock className="h-3.5 w-3.5" />
                  {report.pending_index} indexing
                </div>
              )}
            </div>
          )}

          {loading && staged.length === 0 ? (
            <div className="space-y-2">
              {[...Array(3)].map((_, i) => (
                <div key={i} className={`h-16 rounded-[16px] animate-pulse ${GLASS_PANEL}`} />
              ))}
            </div>
          ) : staged.length === 0 ? (
            <div
              className={`flex flex-col items-center gap-2 rounded-[18px] py-12 text-center ${GLASS_PANEL}`}
            >
              <Inbox className="h-6 w-6 text-text-quaternary" />
              <p className="text-xs text-text-quaternary">Nothing staged for review in this run.</p>
            </div>
          ) : (
            <ul className="space-y-2">
              {staged.map((c) => {
                const Icon = DESTINATION_ICONS[c.destination_kind] ?? Layers
                const isSelected = selectedIds.has(c.id)
                const attested = c.provenance_kind === 'client_attested'
                return (
                  <li
                    key={c.id}
                    className={`rounded-[16px] transition-colors ${
                      isSelected ? 'border border-accent-blue/40 bg-accent-blue/[0.06]' : GLASS_PANEL
                    }`}
                  >
                    <label className="flex cursor-pointer items-start gap-3 p-4">
                      <input
                        type="checkbox"
                        checked={isSelected}
                        onChange={() => toggle(c.id)}
                        aria-label={`Select candidate ${c.source_identity}`}
                        className="mt-0.5 h-4 w-4 shrink-0 rounded accent-[#0066cc]"
                      />
                      <span className="flex flex-1 min-w-0 items-start gap-2.5">
                        <span className="mt-px flex h-7 w-7 shrink-0 items-center justify-center rounded-[9px] bg-white/[0.06] text-text-secondary">
                          <Icon className="h-3.5 w-3.5" />
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="flex flex-wrap items-center gap-2">
                            <span className="text-xs font-semibold text-text-primary">
                              {DESTINATION_LABELS[c.destination_kind] ?? c.destination_kind}
                            </span>
                            <span
                              className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${confidenceTone(c.confidence)}`}
                            >
                              {confidenceLabel(c.confidence)}
                            </span>
                            <span
                              className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium ${
                                attested
                                  ? 'bg-status-warning/15 text-status-warning'
                                  : 'bg-white/[0.06] text-text-tertiary'
                              }`}
                            >
                              {attested ? (
                                <ShieldX className="h-3 w-3" />
                              ) : (
                                <ShieldCheck className="h-3 w-3" />
                              )}
                              {c.provenance_kind}
                            </span>
                          </span>
                          <span className="mt-1 block truncate font-mono text-[11px] text-text-tertiary">
                            {c.source_identity}
                          </span>
                        </span>
                      </span>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.preventDefault()
                          setOpenCandidate(c.id)
                        }}
                        className="flex shrink-0 items-center gap-1 rounded-full bg-white/[0.06] px-2.5 py-1 text-[11px] font-medium text-text-secondary transition-colors hover:bg-white/[0.10] hover:text-text-primary"
                      >
                        <Eye className="h-3 w-3" />
                        Inspect
                      </button>
                    </label>
                  </li>
                )
              })}
            </ul>
          )}

          {batchBlocked && (
            <p
              role="alert"
              className="flex items-start gap-2.5 rounded-[14px] border border-status-warning/30 bg-status-warning/[0.08] p-3.5 text-xs leading-relaxed text-status-warning"
            >
              <ShieldX className="h-4 w-4 shrink-0 mt-px" />
              <span>
                {attestedInSelection.length} of the selected candidates carry client-attested
                provenance. Those rest on somebody&apos;s word rather than a verified manifest, so
                they must be approved one at a time.
              </span>
            </p>
          )}

          {selectionIncludesHarness && (
            <p
              role="note"
              className="flex items-start gap-2.5 rounded-[14px] border border-accent-blue/30 bg-accent-blue/[0.07] p-3.5 text-xs leading-relaxed text-text-secondary"
            >
              <Sparkles className="h-4 w-4 shrink-0 mt-px text-accent-blue" />
              <span>
                This selection includes a harness. Approving it here decides that it becomes a tool
                of the team — it does <strong className="text-text-primary">not</strong> install it
                anywhere. Whoever receives it approves the install separately, on their own machine.
              </span>
            </p>
          )}

          {/* Action bar */}
          <div
            className={`flex flex-wrap items-center gap-2 rounded-[16px] p-3 ${GLASS_PANEL}`}
          >
            {selected.length > 0 && (
              <span className="mr-auto text-xs text-text-quaternary">
                {selected.length} selected
              </span>
            )}
            <button
              type="button"
              onClick={() => submit('approved')}
              disabled={loading || selected.length === 0 || batchBlocked}
              className={`${PILL_BASE} bg-accent-blue text-white hover:bg-accent-blue-hover`}
            >
              <Check className="h-3.5 w-3.5" />
              Approve {selected.length > 0 ? `(${selected.length})` : ''}
            </button>
            <button
              type="button"
              onClick={() => submit('rejected')}
              disabled={loading || selected.length === 0}
              className={`${PILL_BASE} bg-white/[0.06] text-text-secondary hover:bg-white/[0.10]`}
            >
              <X className="h-3.5 w-3.5" />
              Reject {selected.length > 0 ? `(${selected.length})` : ''}
            </button>
            <button
              type="button"
              onClick={commit}
              disabled={loading || approvedCount === 0}
              className={`${PILL_BASE} border border-status-success/30 bg-status-success/[0.12] text-status-success hover:bg-status-success/[0.20]`}
            >
              <GitMerge className="h-3.5 w-3.5" />
              Commit {approvedCount} approved
            </button>
          </div>
        </section>
      )}

      {open && (
        <section
          aria-label="Candidate detail"
          className={`space-y-4 rounded-[18px] p-5 ${GLASS_PANEL}`}
        >
          <div className="flex items-center justify-between gap-3">
            <h2 className="flex items-center gap-2 text-sm font-semibold text-text-primary">
              {(() => {
                const Icon = DESTINATION_ICONS[open.destination_kind] ?? Layers
                return <Icon className="h-4 w-4 text-accent-blue" />
              })()}
              {DESTINATION_LABELS[open.destination_kind] ?? open.destination_kind}
            </h2>
            <button
              type="button"
              onClick={() => setOpenCandidate(null)}
              aria-label="Close detail"
              className="flex h-7 w-7 items-center justify-center rounded-full text-text-quaternary transition-colors hover:bg-white/[0.06] hover:text-text-primary"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          <dl className="space-y-4">
            <div>
              <dt className="text-[10px] font-semibold uppercase tracking-wide text-text-tertiary">
                Source
              </dt>
              <dd className="mt-1 break-all font-mono text-xs text-text-secondary">
                {open.source_identity}
              </dd>
            </div>
            <div>
              <dt className="text-[10px] font-semibold uppercase tracking-wide text-text-tertiary">
                Proposed content
              </dt>
              <dd className="mt-1 whitespace-pre-wrap rounded-[12px] bg-white/[0.04] p-3 text-xs leading-relaxed text-text-primary">
                {open.content}
              </dd>
            </div>
            {open.source_excerpt && (
              <div>
                <dt className="text-[10px] font-semibold uppercase tracking-wide text-text-tertiary">
                  Verbatim from the source
                </dt>
                <dd className="mt-1 whitespace-pre-wrap rounded-[12px] border-l-2 border-accent-blue/40 bg-white/[0.02] p-3 text-xs italic leading-relaxed text-text-secondary">
                  {open.source_excerpt}
                </dd>
              </div>
            )}
            <div>
              <dt className="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-text-tertiary">
                Structured metadata
              </dt>
              <dd className="space-y-2">
                <JsonBlock label="destination_hint" value={open.destination_hint} />
                {open.normalized_metadata &&
                  Object.keys(open.normalized_metadata).length > 0 && (
                    <JsonBlock label="normalized_metadata" value={open.normalized_metadata} />
                  )}
                {open.attestation && Object.keys(open.attestation).length > 0 && (
                  <JsonBlock label="attestation" value={open.attestation} />
                )}
              </dd>
            </div>
          </dl>
        </section>
      )}

      {commitResult && (
        <section
          aria-label="Commit result"
          className={`space-y-3 rounded-[18px] p-5 ${GLASS_PANEL}`}
        >
          <div className="flex flex-wrap gap-2">
            <span className="inline-flex items-center gap-1.5 rounded-full bg-status-success/15 px-3 py-1 text-xs font-medium text-status-success">
              <CheckCircle2 className="h-3.5 w-3.5" />
              {commitResult.committed} committed
            </span>
            <span className="inline-flex items-center rounded-full bg-white/[0.06] px-3 py-1 text-xs font-medium text-text-tertiary">
              {commitResult.skipped} skipped
            </span>
            <span
              className={`inline-flex items-center rounded-full px-3 py-1 text-xs font-medium ${
                commitResult.failed > 0
                  ? 'bg-status-error/15 text-status-error'
                  : 'bg-white/[0.06] text-text-tertiary'
              }`}
            >
              {commitResult.failed} failed
            </span>
          </div>

          {commitResult.pending_index > 0 && (
            <p className="flex items-start gap-2 text-xs leading-relaxed text-text-secondary">
              <AlertCircle className="h-3.5 w-3.5 shrink-0 mt-px text-status-warning" />
              <span>
                {commitResult.pending_index} artifact(s) are stored but not yet searchable by
                similarity. They are safe; indexing catches up separately.
              </span>
            </p>
          )}

          {commitResult.results.filter((r) => r.outcome !== 'committed').length > 0 && (
            <ul className="space-y-1 border-t border-border-primary pt-3">
              {commitResult.results
                .filter((r) => r.outcome !== 'committed')
                .map((r) => (
                  <li
                    key={r.candidate_id}
                    className="flex items-center gap-2 font-mono text-[11px] text-text-tertiary"
                  >
                    <span className="rounded-full bg-status-error/15 px-1.5 py-0.5 text-status-error">
                      {r.outcome}
                    </span>
                    <span className="truncate">{r.candidate_id}</span>
                    {r.reason ? <span className="text-text-quaternary">— {r.reason}</span> : null}
                  </li>
                ))}
            </ul>
          )}
        </section>
      )}
    </div>
  )
}
