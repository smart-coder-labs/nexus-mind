import { useEffect, useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useLocation, useNavigate } from 'react-router-dom'
import { Archive, PlayCircle } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { EmptyState } from '../../../components/ui/EmptyState'
import { Skeleton } from '../../../components/ui/Skeleton'
import { Switch } from '../../../components/ui/Switch'
import { ConfirmModal } from '../../../components/ConfirmModal'
import type { AutonomousAgentEvent, AutonomousAgentRun } from '../../../types'
import { dayLabel, dur, fmtTime, isActiveRun, parseUtc, runDurationSeconds, templateName, triggerName } from '../shared/format'
import { FilterChip, InlineError, PageHeader, RunStatusPill, errorMessage, useFactory } from '../shared/ui'
import { RunDetail } from './RunDetail'

type RunFilter = 'all' | 'active' | 'succeeded' | 'failed' | 'blocked' | 'cancelled'
const FILTERS: Array<{ value: RunFilter; label: string }> = [
  { value: 'all', label: 'All' }, { value: 'active', label: 'Active' }, { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' }, { value: 'blocked', label: 'Blocked' }, { value: 'cancelled', label: 'Cancelled' },
]
// Collapse the many raw statuses into the filter's coarse buckets.
const bucket = (s: string): RunFilter => s.startsWith('blocked') ? 'blocked' : isActiveRun(s) ? 'active' : (s === 'succeeded' || s === 'failed' || s === 'cancelled') ? s : 'all'

/** /factory/runs — run list grouped by day, run story on the right. */
export default function RunsPage() {
  const { can, client, queryClient, invalidate } = useFactory()
  const navigate = useNavigate()
  const location = useLocation()
  const [selectedRun, setSelectedRun] = useState<AutonomousAgentRun | null>(null)
  const [runStatusFilter, setRunStatusFilter] = useState<RunFilter>('all')
  const [showArchivedRuns, setShowArchivedRuns] = useState(false)
  const [confirmArchiveAll, setConfirmArchiveAll] = useState(false)
  const [actionError, setActionError] = useState('')
  const preselect = (location.state as { runId?: string } | null)?.runId

  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents() })
  // Poll while the selected run is still active so the run list, timeline, and
  // conversation update live. Reads the freshest run status from the cache to
  // avoid ordering issues with the runs query below.
  const selectedRunActive = () => {
    const list = queryClient.getQueryData<AutonomousAgentRun[]>(['autonomous-runs'])
    const r = list?.find(x => x.id === selectedRun?.id) ?? selectedRun
    return Boolean(r && isActiveRun(r.status))
  }
  const runs = useQuery({ queryKey: ['autonomous-runs'], queryFn: () => client.listAutonomousAgentRuns(), refetchInterval: () => (selectedRunActive() ? 2500 : false) })
  // Lifecycle events barely change mid-run; the terminal-status effect below
  // refetches them once the run finishes, so no need to poll them live.
  const events = useQuery({ queryKey: ['autonomous-run-events', selectedRun?.id], queryFn: () => client.listAutonomousAgentRunEvents(selectedRun!.id), enabled: Boolean(selectedRun) })
  // Reports appear when the run finishes; the terminal-status refetch covers them.
  const verification = useQuery({ queryKey: ['autonomous-run-verification', selectedRun?.id], queryFn: () => client.listRunVerificationReports(selectedRun!.id), enabled: Boolean(selectedRun) })
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
    enabled: Boolean(selectedRun),
    refetchInterval: () => (selectedRunActive() ? 2000 : false),
  })
  // When a run stops streaming, pull the final turns once (the last poll may have
  // fired just before the run's closing turns were written).
  const selectedLiveStatus = (runs.data?.find(r => r.id === selectedRun?.id) ?? selectedRun)?.status
  useEffect(() => {
    if (selectedRun && selectedLiveStatus && !isActiveRun(selectedLiveStatus)) {
      void transcript.refetch()
      void events.refetch()
      void verification.refetch()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedLiveStatus, selectedRun?.id])
  // Arriving from "Resolve with agent" (or any link carrying a run id): open it.
  useEffect(() => {
    if (preselect && !selectedRun) {
      const found = runs.data?.find(r => r.id === preselect)
      if (found) setSelectedRun(found)
    }
  }, [preselect, runs.data, selectedRun])

  const onError = (fallback: string) => (value: unknown) => setActionError(errorMessage(value, fallback))
  const cancelRun = useMutation({ mutationFn: (id: string) => client.cancelAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs'), onError: onError('Could not cancel the run.') })
  const continueRun = useMutation({ mutationFn: (id: string) => client.continueAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs'), onError: onError('Could not continue the run.') })
  const archiveRun = useMutation({ mutationFn: (id: string) => client.archiveAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs'), onError: onError('Could not archive the run.') })
  const unarchiveRun = useMutation({ mutationFn: (id: string) => client.unarchiveAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs'), onError: onError('Could not restore the run.') })
  const archiveAllRuns = useMutation({ mutationFn: () => client.archiveAllAutonomousAgentRuns(), onSuccess: () => invalidate('autonomous-runs'), onError: onError('Could not archive finished runs.') })

  const allAgents = agents.data ?? []
  const agentById = new Map(allAgents.map(a => [a.id, a]))
  const allRuns = runs.data ?? []
  const archivedCount = allRuns.filter(r => r.archived_at).length
  const scoped = allRuns.filter(r => showArchivedRuns || !r.archived_at)
  const counts = Object.fromEntries(FILTERS.map(f => [f.value, f.value === 'all' ? scoped.length : scoped.filter(r => bucket(r.status) === f.value).length])) as Record<RunFilter, number>
  const visibleRuns = scoped
    .filter(r => runStatusFilter === 'all' || bucket(r.status) === runStatusFilter)
    .sort((a, b) => parseUtc(b.created_at) - parseUtc(a.created_at))
  const archivableCount = allRuns.filter(r => !r.archived_at && !isActiveRun(r.status)).length

  // "Running now" first, then one group per local day.
  const groups: Array<{ label: string; runs: AutonomousAgentRun[] }> = []
  const activeRuns = visibleRuns.filter(r => isActiveRun(r.status))
  if (activeRuns.length) groups.push({ label: 'Running now', runs: activeRuns })
  for (const run of visibleRuns.filter(r => !isActiveRun(r.status))) {
    const label = dayLabel(run.created_at)
    const last = groups[groups.length - 1]
    if (last && last.label === label) last.runs.push(run)
    else groups.push({ label, runs: [run] })
  }

  const current = selectedRun ? (runs.data?.find(r => r.id === selectedRun.id) ?? selectedRun) : null
  const runAgent = current ? agentById.get(current.definition_id) : undefined

  return (
    <div className="mx-auto max-w-7xl space-y-6 p-6 md:p-8">
      <PageHeader title="Runs" subtitle="Every agent run, newest first. Select one to read what happened." />

      <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Filter runs by status">
        {FILTERS.map(f => <FilterChip key={f.value} label={f.label} count={runs.isSuccess ? counts[f.value] : undefined} active={runStatusFilter === f.value} onClick={() => setRunStatusFilter(f.value)} />)}
      </div>

      {actionError && <InlineError message={actionError} onDismiss={() => setActionError('')} />}

      {/* Fixed-height board: the list and the run detail each scroll on their own; the page itself stays put. */}
      <div className="grid gap-4 lg:h-[calc(100vh-16rem)] lg:min-h-[520px] lg:grid-cols-[minmax(300px,360px)_1fr]">
        <section aria-label="Run list" className="flex max-h-[70vh] min-h-0 flex-col overflow-hidden rounded-[18px] border border-border-primary lg:max-h-none">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border-primary px-4 py-3">
            <span className="text-[13px] text-text-secondary"><span className="tabular-nums">{visibleRuns.length}</span> run{visibleRuns.length === 1 ? '' : 's'}</span>
            <div className="flex items-center gap-2">
              {archivedCount > 0 && <Switch size="sm" checked={showArchivedRuns} onCheckedChange={setShowArchivedRuns} label={`Archived (${archivedCount})`} />}
              {can('autonomous_agent:update') && archivableCount > 0 && (
                <Button size="sm" variant="ghost" leftIcon={<Archive className="h-3.5 w-3.5" />} loading={archiveAllRuns.isPending} onClick={() => setConfirmArchiveAll(true)}>Archive all finished</Button>
              )}
            </div>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {runs.isLoading && (
              <div className="space-y-3 p-4" aria-busy="true" aria-label="Loading runs">
                {[0, 1, 2, 3, 4].map(i => <div key={i} className="space-y-2"><Skeleton className="h-3.5 w-40" /><Skeleton className="h-3 w-24" /></div>)}
              </div>
            )}
            {runs.isError && <div className="p-4"><InlineError message="Could not load runs." onRetry={() => void runs.refetch()} /></div>}
            {groups.map(group => (
              <div key={group.label}>
                <h3 className="sticky top-0 z-10 border-b border-border-secondary bg-background-primary px-4 py-2 text-[12px] font-medium text-text-tertiary">{group.label}</h3>
                <ul className="list-none divide-y divide-border-secondary p-0">
                  {group.runs.map(run => {
                    const selected = selectedRun?.id === run.id
                    const agent = agentById.get(run.definition_id)
                    const seconds = runDurationSeconds(run)
                    return (
                      <li key={run.id}>
                        <button
                          type="button"
                          aria-current={selected ? 'true' : undefined}
                          onClick={() => setSelectedRun(run)}
                          className={`flex w-full flex-col gap-1.5 px-4 py-3 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus-ring ${selected ? 'bg-white/[0.06] shadow-[inset_3px_0_0_var(--color-accent-blue)]' : 'hover:bg-white/[0.03]'} ${run.archived_at ? 'opacity-60' : ''}`}
                        >
                          <span className="flex items-center justify-between gap-2">
                            <span className="truncate text-[13px] font-semibold text-text-primary">{agent?.name ?? `${triggerName(run.trigger_kind)} run`}</span>
                            <RunStatusPill status={run.status} />
                          </span>
                          <span className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[12px] text-text-tertiary tabular-nums">
                            <span>{fmtTime(run.created_at)}</span>
                            <span>{triggerName(run.trigger_kind)}</span>
                            {agent && <span>{templateName(agent.template_key)}</span>}
                            {seconds != null && <span>{dur(seconds)}</span>}
                            {run.archived_at && <span>Archived</span>}
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              </div>
            ))}
            {runs.isSuccess && visibleRuns.length === 0 && (
              <div className="p-4">
                <EmptyState
                  icon={<PlayCircle className="h-6 w-6" />}
                  title={allRuns.length ? 'No runs match this filter' : 'No runs yet'}
                  description={allRuns.length ? 'Pick another status above.' : 'Runs appear here once an enabled agent runs, on its schedule or when you start it.'}
                  action={allRuns.length ? <Button size="sm" variant="secondary" onClick={() => { setRunStatusFilter('all'); setShowArchivedRuns(true) }}>Show all runs</Button> : <Button size="sm" variant="primary" onClick={() => navigate('/factory')}>Go to agents</Button>}
                />
              </div>
            )}
          </div>
        </section>

        <section aria-label="Run detail" className="min-h-0 overflow-y-auto rounded-[18px] border border-border-primary p-5 md:p-6">
          {current ? (
            <RunDetail
              run={current}
              events={events.data ?? []}
              transcript={transcript.data ?? []}
              verificationReports={verification.data?.reports ?? []}
              agentName={runAgent?.name}
              templateKey={runAgent?.template_key}
              onOpenFindings={() => navigate('/factory/findings')}
              onContinue={can('autonomous_agent:run') ? id => continueRun.mutate(id) : undefined}
              continuing={continueRun.isPending}
              onCancel={can('autonomous_agent:cancel') ? id => cancelRun.mutate(id) : undefined}
              cancelling={cancelRun.isPending}
              onArchive={can('autonomous_agent:update') ? id => archiveRun.mutate(id) : undefined}
              onRestore={can('autonomous_agent:update') ? id => unarchiveRun.mutate(id) : undefined}
            />
          ) : (
            <EmptyState title="Select a run" description="Read the outcome, which stages it reached, the budget it used, and the agent’s conversation." />
          )}
        </section>
      </div>

      <ConfirmModal
        open={confirmArchiveAll}
        title={`Archive ${archivableCount} finished run${archivableCount === 1 ? '' : 's'}?`}
        description="Archived runs leave the list but are kept. Turn on “Archived” to see or restore them."
        confirmLabel="Archive all finished"
        loading={archiveAllRuns.isPending}
        onClose={() => setConfirmArchiveAll(false)}
        onConfirm={() => { archiveAllRuns.mutate(); setConfirmArchiveAll(false) }}
      />
    </div>
  )
}
