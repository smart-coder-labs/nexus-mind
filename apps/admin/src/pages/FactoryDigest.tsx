import { useMemo } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, GitMerge, Inbox, ListTodo, Scale, TriangleAlert } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { cn } from '../lib/utils'
import type { FactoryEconomics, FactoryHeldMerge } from '../types'
import { CardListSkeleton, InlineAlert, LINK_CLASS, PageHeader, PANEL_CLASS, PermissionDenied, SectionHeading, when } from './factory/govern/ui'
import { MetricSummary } from '../components/ui/MetricSummary'
import { accentFor } from './dashboard/colors'
import { holdReason, holdSource, readable, runStatusLabel } from './factory/govern/words'

/** `owner/repo#7@sha` → GitHub pull URL. */
function pullUrl(subject: string): string | null {
  const match = /^([\w.-]+\/[\w.-]+)#(\d+)@/.exec(subject)
  return match ? `https://github.com/${match[1]}/pull/${match[2]}` : null
}

function usd(value: number): string {
  return `$${value.toFixed(value < 1 ? 3 : 2)}`
}

/**
 * Needs a human (factory F3): what the factory cannot finish without a person.
 * Held merges are decided here one at a time. Approving one starts its soak:
 * when it ends the factory re-runs every check on that exact commit and merges
 * only if they all still pass. Rejecting cancels any pending soak.
 */
export default function FactoryDigest() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  const canReadLookback = canRead && permissions.includes('task:read') && permissions.includes('memory:read')
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()

  const digest = useQuery({ queryKey: ['factory-digest'], queryFn: () => client.getFactoryDigest(), enabled: canRead })
  const feedback = useQuery({ queryKey: ['factory-trajectory-feedback'], queryFn: () => client.listFactoryTrajectoryFeedback(), enabled: canRead })
  const lookback = useQuery({ queryKey: ['factory-lookback'], queryFn: () => client.getFactoryLookback(), enabled: canReadLookback })
  const economics = useQuery({ queryKey: ['factory-economics'], queryFn: () => client.getFactoryEconomics(30), enabled: canRead })
  const decide = useMutation({
    mutationFn: ({ subject, approve }: { subject: string; approve: boolean }) => client.decideFactoryMerge(subject, approve),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['factory-digest'] }),
  })
  const labelRun = useMutation({
    mutationFn: ({ runId, outcome }: { runId: string; outcome: 'accepted' | 'rejected' }) =>
      client.recordFactoryTrajectoryFeedback(runId, outcome),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['factory-trajectory-feedback'] }),
  })

  if (!canRead) {
    return <PermissionDenied title="Needs a human" permission="factory_policy:read" what="see what the factory is waiting on" />
  }

  const data = digest.data
  const blocked = data?.blocked_runs ?? []
  const reviewItems = feedback.data ?? []
  const noLookbackRecommendations = !canReadLookback || lookback.isError || (lookback.isSuccess && lookback.data.recommendations.length === 0)
  const nothingWaiting =
    data && feedback.isSuccess && !data.held_merges.length && !data.approved_merges.length && !blocked.length &&
    !data.factory_tasks.length && !data.unlabeled_shadow && !reviewItems.length && noLookbackRecommendations
  const failedSubject = decide.isError ? decide.variables?.subject : undefined
  const waiting = data ? data.held_merges.length + blocked.length + data.factory_tasks.length + reviewItems.length : 0

  return (
    <div className="p-6 md:p-8 min-w-0 max-w-7xl mx-auto space-y-6">
      <PageHeader
        title="Needs a human"
        aside={data && waiting > 0 ? <Badge role="none" variant="warning">{waiting} waiting</Badge> : undefined}
        subtitle="What the factory cannot finish on its own. Each item is a decision: what is held, why, and the evidence to decide on."
      />

      {digest.isError && <InlineAlert onRetry={() => digest.refetch()}>Could not load what is waiting.</InlineAlert>}
      {digest.isLoading && <CardListSkeleton count={3} height="h-32" />}

      {nothingWaiting && (
        <EmptyState
          icon={<Inbox />}
          title="Nothing is waiting on a person right now"
          description="Held merges, runs that stopped short and new factory tasks appear here as soon as they need a decision."
        />
      )}

      {data && data.held_merges.length > 0 && (
        <section aria-labelledby="held-title" className="space-y-3">
          <SectionHeading
            id="held-title"
            title="Merges held for a person"
            description="Approving applies to the exact commit shown. After a short wait (the soak) the factory re-runs every check on that commit and merges only if they all still pass. Rejecting cancels any pending soak."
          />
          <ul className="m-0 list-none space-y-3 p-0">
            {data.held_merges.map(merge => (
              <HeldMergeCard
                key={merge.subject}
                merge={merge}
                canWrite={canWrite}
                busy={decide.isPending}
                onDecide={approve => decide.mutate({ subject: merge.subject, approve })}
                error={
                  failedSubject === merge.subject
                    ? `The decision on ${merge.subject.split('@')[0]} was not saved: ${(decide.error as { message?: string })?.message ?? 'unknown error'}.`
                    : undefined
                }
              />
            ))}
          </ul>
        </section>
      )}

      {data && data.approved_merges.length > 0 && (
        <section aria-labelledby="approved-title" className="space-y-3">
          <SectionHeading id="approved-title" title="Approved merges" description="Decided by a person; the factory re-checks each one before it merges." />
          <ul className={cn(PANEL_CLASS, 'm-0 list-none divide-y divide-border-secondary p-0')}>
            {data.approved_merges.map(merge => (
              <li key={merge.subject} className="flex flex-wrap items-center gap-x-4 gap-y-1 px-5 py-3 text-sm">
                <CheckCircle2 className="h-4 w-4 shrink-0 text-status-success" aria-hidden="true" />
                <span className="font-mono text-text-primary">{merge.subject.split('@')[0]}</span>
                <code className="font-mono text-[12px] text-text-tertiary">{merge.subject.split('@')[1]?.slice(0, 12)}</code>
                <span className="ml-auto text-text-secondary">
                  {merge.merges_after ? `Re-checks and merges after ${when(merge.merges_after)}` : 'Checked: merged or declined'}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {blocked.length > 0 && (
        <section aria-labelledby="blocked-title" className="space-y-3">
          <SectionHeading id="blocked-title" title="Runs that stopped short" description="Last 7 days. Open the run to see what stopped it and continue it." />
          <ul className="m-0 list-none space-y-3 p-0">
            {blocked.map(run => (
              <li key={run.run_id} className={cn(PANEL_CLASS, 'flex flex-wrap items-start gap-x-4 gap-y-3')}>
                <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0 text-status-error" aria-hidden="true" />
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium text-text-primary">{run.agent}</span>
                    <Badge role="none" size="sm" variant="error">{runStatusLabel(run.status)}</Badge>
                  </div>
                  <p className="text-sm text-text-secondary">
                    {run.reason ? readable(run.reason) : 'No reason recorded.'}
                    {run.reason && <> <code className="ml-1 break-all font-mono text-[12px] text-text-tertiary">{run.reason}</code></>}
                  </p>
                  <p className="text-[12px] text-text-tertiary">Stopped {when(run.finished_at)}</p>
                </div>
                <Link to="/factory/runs" className="inline-flex h-8 items-center rounded-md border border-border-primary bg-background px-3 text-sm font-medium text-text-secondary transition-apple hover:bg-foreground/5 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
                  Open runs
                </Link>
              </li>
            ))}
          </ul>
        </section>
      )}

      {feedback.isError && <InlineAlert onRetry={() => feedback.refetch()}>Could not load runs awaiting an outcome.</InlineAlert>}
      {feedback.isLoading && <CardListSkeleton count={2} height="h-20" />}
      {reviewItems.length > 0 && (
        <section aria-labelledby="trajectory-feedback-title" className="space-y-3">
          <SectionHeading
            id="trajectory-feedback-title"
            title="Agent outcomes awaiting review"
            description="Mark whether each completed run produced a change worth keeping. This feedback is stored for evaluation and does not change policy or rerun the agent."
          />
          <ul className={cn(PANEL_CLASS, 'm-0 list-none divide-y divide-border-secondary p-0')}>
            {reviewItems.map(item => {
              const failedRun = labelRun.isError && labelRun.variables?.runId === item.run_id
              return (
                <li key={item.run_id} className="flex flex-wrap items-center gap-x-4 gap-y-3 px-5 py-4">
                  <Scale className="h-4 w-4 shrink-0 text-text-tertiary" aria-hidden="true" />
                  <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium text-text-primary">{item.agent_name}</span>
                      <Badge role="none" size="sm" variant="default">{runStatusLabel(item.run_status)}</Badge>
                      <Badge role="none" size="sm" variant={item.outcome === 'unknown' || !item.outcome ? 'warning' : 'info'}>
                        {item.outcome ?? 'unlabeled'}
                      </Badge>
                    </div>
                    <p className="text-sm text-text-secondary">Did this run produce an accepted change?</p>
                    <p className="font-mono text-[12px] text-text-tertiary">Run {item.run_id} · {when(item.created_at)}</p>
                    {failedRun && <p role="alert" className="text-sm text-status-error">Could not save this outcome: {(labelRun.error as { message?: string })?.message ?? 'unknown error'}.</p>}
                  </div>
                  {canWrite && (
                    <div className="flex shrink-0 gap-2">
                      <Button
                        size="sm"
                        variant="destructive"
                        disabled={labelRun.isPending}
                        loading={labelRun.isPending}
                        onClick={() => labelRun.mutate({ runId: item.run_id, outcome: 'rejected' })}
                      >
                        Mark rejected
                      </Button>
                      <Button
                        size="sm"
                        disabled={labelRun.isPending}
                        loading={labelRun.isPending}
                        onClick={() => labelRun.mutate({ runId: item.run_id, outcome: 'accepted' })}
                      >
                        Mark accepted
                      </Button>
                    </div>
                  )}
                </li>
              )
            })}
          </ul>
        </section>
      )}

      {data && data.factory_tasks.length > 0 && (
        <section aria-labelledby="tasks-title" className="space-y-3">
          <SectionHeading id="tasks-title" title="Factory tasks not started" description="Work that came in but did not start on its own. Start or dismiss it from Tasks." />
          <ul className={cn(PANEL_CLASS, 'm-0 list-none divide-y divide-border-secondary p-0')}>
            {data.factory_tasks.map(task => (
              <li key={task.id} className="flex flex-wrap items-center gap-x-4 gap-y-1 px-5 py-3 text-sm">
                <ListTodo className="h-4 w-4 shrink-0 text-text-tertiary" aria-hidden="true" />
                <Link to="/tasks" className={LINK_CLASS}>{task.title}</Link>
                <span className="text-text-tertiary">{task.project}</span>
                <span className="ml-auto text-text-secondary">{readable(task.status)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {canReadLookback && lookback.isError && <InlineAlert onRetry={() => lookback.refetch()}>Could not load recurring fix recommendations.</InlineAlert>}
      {canReadLookback && lookback.isLoading && <CardListSkeleton count={1} height="h-24" />}
      {canReadLookback && lookback.data && (
        <section aria-labelledby="lookback-title" className="space-y-3">
          <SectionHeading
            id="lookback-title"
            title="Recurring fixes to review"
            description="Read-only matches between completed tasks and NexusMind bugfix memories. These suggestions do not change tasks, memories, policy, or agent runs."
          />
          <p className="text-[12px] text-text-tertiary">
            Scanned {lookback.data.completed_tasks_scanned} completed tasks and {lookback.data.bugfix_memories_scanned} bugfix memories.
            {lookback.data.history_truncated && ` History is capped at ${lookback.data.history_limit} records; some matches may be omitted.`}
          </p>
          {lookback.data.recommendations.length === 0 ? (
            <EmptyState icon={<Scale />} title="No recurring fixes found" description="No repeated completed-task terms matched a bugfix memory in the scanned history." />
          ) : (
            <ul className={cn(PANEL_CLASS, 'm-0 list-none divide-y divide-border-secondary p-0')}>
              {lookback.data.recommendations.map(recommendation => (
                <li key={recommendation.memory_id} className="space-y-3 px-5 py-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium text-text-primary">{recommendation.memory_title}</span>
                    <Badge role="none" variant="default">{recommendation.memory_project}</Badge>
                    <Badge role="none" variant="warning">{recommendation.recurrence_count} recurring tasks</Badge>
                  </div>
                  <p className="text-sm text-text-secondary">{recommendation.explanation}</p>
                  <div className="flex flex-wrap gap-1.5" aria-label="Recurring terms">
                    {recommendation.recurring_terms.map(term => <Badge key={term} role="none" variant="info">{term}</Badge>)}
                  </div>
                  <details className="text-sm text-text-secondary">
                    <summary className="cursor-pointer">Evidence ({recommendation.task_evidence.length} completed tasks)</summary>
                    <ul className="mt-2 list-disc space-y-1 pl-5">
                      {recommendation.task_evidence.map(task => (
                        <li key={task.id}>
                          <span className="text-text-primary">{task.title}</span>
                          <code className="ml-2 text-[11px] text-text-tertiary">{task.id}</code>
                        </li>
                      ))}
                    </ul>
                  </details>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {data && data.unlabeled_shadow > 0 && (
        <section aria-labelledby="labels-title" className={cn(PANEL_CLASS, 'flex flex-wrap items-center gap-4')}>
          <Scale className="h-5 w-5 shrink-0 text-text-secondary" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <h2 id="labels-title" className="text-base font-semibold tracking-[-0.2px] text-text-primary">Decision model labels</h2>
            <p className="mt-1 text-sm text-text-secondary">
              {data.unlabeled_shadow} shadow decisions have no human label ({data.unlabeled_shadow_allows} of them “allow”, which
              are the ones the routing bar counts).
            </p>
          </div>
          <Link to="/factory/decision-model" className="inline-flex h-8 items-center rounded-md border border-border-primary bg-background px-3 text-sm font-medium text-text-secondary transition-apple hover:bg-foreground/5 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
            Label them
          </Link>
        </section>
      )}

      {economics.isError && <InlineAlert onRetry={() => economics.refetch()}>Could not load the economics.</InlineAlert>}
      {economics.data && <Economics data={economics.data} />}
    </div>
  )
}

function HeldMergeCard({
  merge,
  canWrite,
  busy,
  onDecide,
  error,
}: {
  merge: FactoryHeldMerge
  canWrite: boolean
  busy: boolean
  onDecide: (approve: boolean) => void
  error?: string
}) {
  const url = pullUrl(merge.subject)
  const [pull, sha] = merge.subject.split('@')
  return (
    <li className={cn(PANEL_CLASS, 'space-y-4')}>
      <div className="flex flex-wrap items-start gap-x-4 gap-y-3">
        <GitMerge className="mt-0.5 h-4 w-4 shrink-0 text-status-warning" aria-hidden="true" />
        <div className="min-w-0 flex-1 space-y-1.5">
          <p className="text-sm text-text-secondary">
            Merge{' '}
            {url ? (
              <a href={url} target="_blank" rel="noreferrer" className={cn('font-mono', LINK_CLASS)}>{pull}</a>
            ) : (
              <span className="font-mono text-text-primary">{merge.subject}</span>
            )}{' '}
            at commit <code className="font-mono text-[12px] text-text-primary">{sha?.slice(0, 12)}</code>
          </p>
          <dl className="grid gap-x-4 gap-y-1 text-sm sm:grid-cols-[7rem_1fr]">
            <dt className="text-text-tertiary">Why</dt>
            <dd className="text-text-primary">
              <span className="text-text-secondary">{holdSource(merge.source)}.</span> {holdReason(merge.reason)}
            </dd>
            <dt className="text-text-tertiary">Held since</dt>
            <dd className="text-text-secondary">{when(merge.created_at)}</dd>
            <dt className="text-text-tertiary">Evidence</dt>
            <dd className="text-text-secondary">{url ? 'The pull request, its review and checks on GitHub.' : 'No pull request link for this item.'}</dd>
          </dl>
        </div>
        {canWrite && (
          <div className="flex shrink-0 flex-wrap gap-2">
            <Button size="sm" variant="secondary" aria-label={`Reject merging ${merge.subject}`} disabled={busy} onClick={() => onDecide(false)}>
              Reject
            </Button>
            <Button size="sm" aria-label={`Approve merging ${merge.subject}`} disabled={busy} onClick={() => onDecide(true)}>
              Approve merge
            </Button>
          </div>
        )}
      </div>
      {error && <InlineAlert>{error}</InlineAlert>}
    </li>
  )
}

function Economics({ data }: { data: FactoryEconomics }) {
  return (
    <section aria-labelledby="economics-title" className="space-y-3 border-t border-border-primary pt-6">
      <SectionHeading id="economics-title" title={`Economics, last ${data.days} days`} description="What the factory cost at list price." />
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,220px),1fr))] gap-3" role="list" aria-label="Factory economics statistics">
        <div role="listitem"><MetricSummary label="Runs" value={data.runs} icon={ListTodo} accent={accentFor(0)} /></div>
        <div role="listitem"><MetricSummary label="Spend (list price)" value={usd(data.cost_usd)} icon={Scale} accent={accentFor(1)} /></div>
        <div role="listitem"><MetricSummary label="Runs below the frontier tier" value={data.frontier_avoidance === null ? '—' : `${Math.round(data.frontier_avoidance * 100)}%`} icon={CheckCircle2} accent={accentFor(2)} /></div>
        <div role="listitem"><MetricSummary label="Cost per proposed change" value={data.cost_per_proposed_change === null ? '—' : usd(data.cost_per_proposed_change)} icon={GitMerge} accent={accentFor(3)} /></div>
      </div>
      {!data.accepted_changes_tracked && (
        <p className="text-[12px] text-text-tertiary">Merged (accepted) changes are not measured yet, so cost per accepted change is not shown.</p>
      )}
      {data.by_model.length > 0 && (
        <details className="group rounded-xl border border-border-primary">
          <summary className="cursor-pointer list-none rounded-xl px-5 py-3 text-sm text-text-secondary hover:text-text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
            Spend by model ({data.by_model.length})
          </summary>
          <div className="overflow-x-auto border-t border-border-secondary">
            <table className="admin-data-table w-full text-sm" aria-label="Spend by model">
              <thead>
                <tr className="border-b border-border-secondary text-left text-[12px] text-text-tertiary">
                  <th scope="col" className="px-5 py-2.5 font-medium">Model</th>
                  <th scope="col" className="px-5 py-2.5 font-medium text-right">Runs</th>
                  <th scope="col" className="px-5 py-2.5 font-medium text-right">Spend</th>
                </tr>
              </thead>
              <tbody>
                {data.by_model.map(row => (
                  <tr key={row.model ?? 'unknown'} className="border-b border-border-secondary last:border-0">
                    <td className="px-5 py-2.5 font-mono text-text-primary">{row.model ?? 'Unknown model'}</td>
                    <td className="px-5 py-2.5 text-right tabular-nums text-text-secondary">{row.runs}</td>
                    <td className="px-5 py-2.5 text-right tabular-nums text-text-secondary">{usd(row.cost_usd)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      )}
    </section>
  )
}
