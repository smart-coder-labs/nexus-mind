import { useMemo } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Inbox } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import type { FactoryEconomics } from '../types'

/** `owner/repo#7@sha` → GitHub pull URL. */
function pullUrl(subject: string): string | null {
  const match = /^([\w.-]+\/[\w.-]+)#(\d+)@/.exec(subject)
  return match ? `https://github.com/${match[1]}/pull/${match[2]}` : null
}

function usd(value: number): string {
  return `$${value.toFixed(value < 1 ? 3 : 2)}`
}

function when(value: string | null): string {
  return value ? value.replace('T', ' ').slice(0, 16) : '—'
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
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()

  const digest = useQuery({ queryKey: ['factory-digest'], queryFn: () => client.getFactoryDigest(), enabled: canRead })
  const economics = useQuery({ queryKey: ['factory-economics'], queryFn: () => client.getFactoryEconomics(30), enabled: canRead })
  const decide = useMutation({
    mutationFn: ({ subject, approve }: { subject: string; approve: boolean }) => client.decideFactoryMerge(subject, approve),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['factory-digest'] }),
  })

  if (!canRead) {
    return (
      <div className="p-6 md:p-8 max-w-7xl mx-auto">
        <EmptyState
          title="Needs a human"
          description="You need the factory_policy:read permission to see what the factory is waiting on. Ask an organization owner to grant it."
        />
      </div>
    )
  }

  const data = digest.data
  const blocked = data?.blocked_runs ?? []
  const nothingWaiting =
    data && !data.held_merges.length && !data.approved_merges.length && !blocked.length && !data.factory_tasks.length && !data.unlabeled_shadow
  const failedSubject = decide.isError ? decide.variables?.subject : undefined

  return (
    <div className="p-6 md:p-8 max-w-7xl mx-auto space-y-6">
      <header>
        <h1 className="text-xl font-semibold text-text-primary flex items-center gap-2">
          <Inbox className="h-5 w-5" />Needs a human
        </h1>
        <p className="mt-1 max-w-2xl text-sm text-text-tertiary">
          What the software factory cannot finish on its own. Approving a merge applies to the exact commit shown: after a
          short wait the factory re-runs every check and merges only if they all still pass.
        </p>
      </header>

      {digest.isLoading && <p className="text-sm text-text-tertiary">Loading…</p>}
      {digest.isError && <p role="alert" className="text-sm text-text-primary">Could not load what is waiting.</p>}
      {nothingWaiting && <p className="text-sm text-text-tertiary">Nothing is waiting on a person right now.</p>}

      {data && data.held_merges.length > 0 && (
        <section aria-labelledby="held-title" className="rounded-xl border border-border-primary p-4 space-y-2">
          <h2 id="held-title" className="text-sm font-semibold text-text-primary">Merges held for a person</h2>
          <ul className="m-0 p-0 list-none divide-y divide-border-primary">
            {data.held_merges.map(merge => {
              const url = pullUrl(merge.subject)
              const busy = decide.isPending
              return (
                <li key={merge.subject} className="py-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                  {url ? (
                    <a href={url} target="_blank" rel="noreferrer" className="font-mono text-text-primary hover:underline">{merge.subject.split('@')[0]}</a>
                  ) : (
                    <span className="font-mono">{merge.subject}</span>
                  )}
                  <code className="text-text-tertiary">{merge.subject.split('@')[1]?.slice(0, 12)}</code>
                  <span className="text-text-secondary">{merge.reason}</span>
                  {canWrite && (
                    <span className="ml-auto flex gap-1.5">
                      <Button size="sm" variant="secondary" aria-label={`Approve merging ${merge.subject}`} disabled={busy} onClick={() => decide.mutate({ subject: merge.subject, approve: true })}>Approve</Button>
                      <Button size="sm" variant="ghost" aria-label={`Reject merging ${merge.subject}`} disabled={busy} onClick={() => decide.mutate({ subject: merge.subject, approve: false })}>Reject</Button>
                    </span>
                  )}
                </li>
              )
            })}
          </ul>
          {failedSubject && (
            <p role="alert" className="text-xs text-text-primary">
              The decision on {failedSubject.split('@')[0]} was not saved: {(decide.error as { message?: string })?.message ?? 'unknown error'}.
            </p>
          )}
        </section>
      )}

      {data && data.approved_merges.length > 0 && (
        <section aria-labelledby="approved-title" className="rounded-xl border border-border-primary p-4 space-y-2">
          <h2 id="approved-title" className="text-sm font-semibold text-text-primary">Approved merges</h2>
          <ul className="m-0 p-0 list-none divide-y divide-border-primary">
            {data.approved_merges.map(merge => (
              <li key={merge.subject} className="py-2 flex flex-wrap items-center gap-x-3 text-xs">
                <span className="font-mono text-text-primary">{merge.subject.split('@')[0]}</span>
                <code className="text-text-tertiary">{merge.subject.split('@')[1]?.slice(0, 12)}</code>
                <span className="ml-auto text-text-secondary">
                  {merge.merges_after ? `Re-checks and merges after ${when(merge.merges_after)}` : 'Checked: merged or declined'}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {blocked.length > 0 && (
        <section aria-labelledby="blocked-title" className="rounded-xl border border-border-primary p-4 space-y-2">
          <h2 id="blocked-title" className="text-sm font-semibold text-text-primary">Runs that stopped short (last 7 days)</h2>
          <ul className="m-0 p-0 list-none divide-y divide-border-primary">
            {blocked.map(run => (
              <li key={run.run_id} className="py-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                <Link to="/factory" className="text-text-primary hover:underline">{run.agent}</Link>
                <span className="rounded border border-border-primary px-1.5 py-0.5 font-mono">{run.status}</span>
                <span className="text-text-secondary">{run.reason ?? 'no reason recorded'}</span>
                <span className="ml-auto text-text-tertiary">{when(run.finished_at)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {data && data.factory_tasks.length > 0 && (
        <section aria-labelledby="tasks-title" className="rounded-xl border border-border-primary p-4 space-y-2">
          <h2 id="tasks-title" className="text-sm font-semibold text-text-primary">Factory tasks not started</h2>
          <ul className="m-0 p-0 list-none divide-y divide-border-primary">
            {data.factory_tasks.map(task => (
              <li key={task.id} className="py-2 flex flex-wrap items-center gap-x-3 text-xs">
                <Link to="/tasks" className="text-text-primary hover:underline">{task.title}</Link>
                <span className="text-text-tertiary">{task.project}</span>
                <span className="ml-auto text-text-tertiary">{task.status}</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {data && data.unlabeled_shadow > 0 && (
        <section aria-labelledby="labels-title" className="rounded-xl border border-border-primary p-4">
          <h2 id="labels-title" className="text-sm font-semibold text-text-primary">Decision model labels</h2>
          <p className="mt-1 text-xs text-text-secondary">
            {data.unlabeled_shadow} shadow decisions have no human label ({data.unlabeled_shadow_allows} of them “allow”, which
            are the ones the routing bar counts). <Link to="/factory/decision-model" className="underline">Label them</Link>.
          </p>
        </section>
      )}

      {economics.isError && <p role="alert" className="text-sm text-text-primary">Could not load the economics.</p>}
      {economics.data && <EconomicsCard data={economics.data} />}
    </div>
  )
}

function EconomicsCard({ data }: { data: FactoryEconomics }) {
  return (
    <section aria-labelledby="economics-title" className="rounded-xl border border-border-primary p-4 space-y-3">
      <h2 id="economics-title" className="text-sm font-semibold text-text-primary">Economics · last {data.days} days</h2>
      <dl className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs">
        <div><dt className="text-text-tertiary">Runs</dt><dd className="text-text-primary">{data.runs}</dd></div>
        <div><dt className="text-text-tertiary">Spend (list price)</dt><dd className="text-text-primary">{usd(data.cost_usd)}</dd></div>
        <div>
          <dt className="text-text-tertiary">Runs below the frontier tier</dt>
          <dd className="text-text-primary">{data.frontier_avoidance === null ? '—' : `${Math.round(data.frontier_avoidance * 100)}%`}</dd>
        </div>
        <div>
          <dt className="text-text-tertiary">Cost per proposed change</dt>
          <dd className="text-text-primary">{data.cost_per_proposed_change === null ? '—' : usd(data.cost_per_proposed_change)}</dd>
        </div>
      </dl>
      {!data.accepted_changes_tracked && (
        <p className="text-xs text-text-tertiary">Merged (accepted) changes are not measured yet, so cost per accepted change is not shown.</p>
      )}
      {data.by_model.length > 0 && (
        <table className="w-full text-xs" aria-label="Spend by model">
          <thead><tr className="text-left text-text-tertiary"><th className="py-1 font-medium">Model</th><th className="py-1 font-medium">Runs</th><th className="py-1 font-medium">Spend</th></tr></thead>
          <tbody>
            {data.by_model.map(row => (
              <tr key={row.model ?? 'unknown'} className="border-t border-border-primary">
                <td className="py-1.5 font-mono">{row.model ?? 'unknown'}</td>
                <td className="py-1.5">{row.runs}</td>
                <td className="py-1.5">{usd(row.cost_usd)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
