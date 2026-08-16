import { useEffect, useMemo, useState } from 'react'
import { createClient } from '../api/client'
import type {
  MigrationRun,
  MigrationCandidate,
  MigrationVerdict,
  MigrationCommitResponse,
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

const DESTINATION_LABELS: Record<string, string> = {
  memory: 'Memory',
  convention: 'Convention',
  task: 'Task',
  sdd_artifact: 'SDD artifact',
  harness: 'Harness',
  harness_config_review: 'Config review',
}

function confidenceLabel(c?: number | null): string {
  if (c === null || c === undefined) return '—'
  return `${Math.round(c * 100)}%`
}

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

  const open = candidates.find((c) => c.id === openCandidate) ?? null
  const approvedCount = candidates.filter((c) => c.status === 'approved').length

  return (
    <div className="p-6 space-y-6">
      <header>
        <h1 className="text-2xl font-semibold">Knowledge migration</h1>
        <p className="text-sm text-muted-foreground">
          Nothing here has entered the company brain yet. Every candidate is waiting for a
          decision.
        </p>
      </header>

      {error && (
        <div role="alert" className="rounded border border-destructive p-3 text-sm">
          {error}
        </div>
      )}
      {notice && (
        <div role="status" className="rounded border p-3 text-sm">
          {notice}
        </div>
      )}

      <section aria-label="Runs" className="space-y-2">
        <h2 className="text-lg font-medium">Runs</h2>
        {runs.length === 0 && <p className="text-sm text-muted-foreground">No runs yet.</p>}
        <ul className="space-y-1">
          {runs.map((r) => (
            <li key={r.id}>
              <button
                type="button"
                onClick={() => loadCandidates(r.id)}
                aria-pressed={selectedRun === r.id}
                className="text-left underline"
              >
                {r.source_kind} — {r.status}
                {r.client_id ? ` — client ${r.client_id}` : ' — internal'}
              </button>
            </li>
          ))}
        </ul>
      </section>

      {selectedRun && (
        <section aria-label="Review queue" className="space-y-3">
          <h2 className="text-lg font-medium">
            Review queue <span className="text-sm font-normal">({staged.length} staged)</span>
          </h2>

          <ul className="space-y-2">
            {staged.map((c) => (
              <li key={c.id} className="rounded border p-3">
                <label className="flex items-start gap-2">
                  <input
                    type="checkbox"
                    checked={selectedIds.has(c.id)}
                    onChange={() => toggle(c.id)}
                    aria-label={`Select candidate ${c.source_identity}`}
                  />
                  <span className="flex-1">
                    <span className="font-medium">
                      {DESTINATION_LABELS[c.destination_kind] ?? c.destination_kind}
                    </span>{' '}
                    <span className="text-sm text-muted-foreground">
                      confidence {confidenceLabel(c.confidence)} · {c.provenance_kind}
                    </span>
                    <br />
                    <span className="text-sm">{c.source_identity}</span>
                  </span>
                  <button type="button" onClick={() => setOpenCandidate(c.id)} className="underline">
                    Inspect
                  </button>
                </label>
              </li>
            ))}
          </ul>

          {batchBlocked && (
            <p role="alert" className="text-sm">
              {attestedInSelection.length} of the selected candidates carry client-attested
              provenance. Those rest on somebody&apos;s word rather than a verified manifest, so
              they must be approved one at a time.
            </p>
          )}

          {selectionIncludesHarness && (
            <p role="note" className="text-sm">
              This selection includes a harness. Approving it here decides that it becomes a tool
              of the team — it does <strong>not</strong> install it anywhere. Whoever receives it
              approves the install separately, on their own machine.
            </p>
          )}

          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => submit('approved')}
              disabled={loading || selected.length === 0 || batchBlocked}
            >
              Approve {selected.length > 0 ? `(${selected.length})` : ''}
            </button>
            <button
              type="button"
              onClick={() => submit('rejected')}
              disabled={loading || selected.length === 0}
            >
              Reject {selected.length > 0 ? `(${selected.length})` : ''}
            </button>
            <button type="button" onClick={commit} disabled={loading || approvedCount === 0}>
              Commit {approvedCount} approved
            </button>
          </div>
        </section>
      )}

      {open && (
        <section aria-label="Candidate detail" className="rounded border p-4 space-y-3">
          <h2 className="text-lg font-medium">
            {DESTINATION_LABELS[open.destination_kind] ?? open.destination_kind}
          </h2>
          <dl className="text-sm">
            <dt className="font-medium">Source</dt>
            <dd>{open.source_identity}</dd>
            <dt className="font-medium mt-2">Proposed content</dt>
            <dd className="whitespace-pre-wrap">{open.content}</dd>
            {open.source_excerpt && (
              <>
                <dt className="font-medium mt-2">Verbatim from the source</dt>
                <dd className="whitespace-pre-wrap italic">{open.source_excerpt}</dd>
              </>
            )}
            <dt className="font-medium mt-2">Destination hint</dt>
            <dd>
              <pre className="text-xs overflow-x-auto">
                {JSON.stringify(open.destination_hint, null, 2)}
              </pre>
            </dd>
          </dl>
        </section>
      )}

      {commitResult && (
        <section aria-label="Commit result" className="rounded border p-4 text-sm">
          <p>
            {commitResult.committed} committed, {commitResult.skipped} skipped,{' '}
            {commitResult.failed} failed.
          </p>
          {commitResult.pending_index > 0 && (
            <p>
              {commitResult.pending_index} artifact(s) are stored but not yet searchable by
              similarity. They are safe; indexing catches up separately.
            </p>
          )}
          <ul>
            {commitResult.results
              .filter((r) => r.outcome !== 'committed')
              .map((r) => (
                <li key={r.candidate_id}>
                  {r.candidate_id}: {r.outcome}
                  {r.reason ? ` — ${r.reason}` : ''}
                </li>
              ))}
          </ul>
        </section>
      )}
    </div>
  )
}
