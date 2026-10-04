import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Scale } from 'lucide-react'
import type { NexusMindClient } from '../../api/client'
import type { ShadowDecision, ShadowOutcome } from '../../types'
import { Button } from '../../components/ui/Button'

const OUTCOME_LABEL: Record<ShadowOutcome, string> = {
  pending: 'Pending',
  clean: 'Clean',
  high_risk: 'High risk',
  not_merged: 'Not merged',
  superseded: 'Another head was merged',
  unresolvable: 'Could not be read',
}

/** Outcomes the report counts; only those are worth a person's label. */
const COUNTED: ShadowOutcome[] = ['pending', 'clean', 'high_risk']

/** `reverted_by:1a2b3c` → `reverted by 1a2b3c`. */
function signalLabel(signal: string): string {
  const [kind, detail] = signal.split(':')
  const text: Record<string, string> = {
    reverted_by: 'reverted by',
    follow_up_fix: 'fixed by',
    merge_commit_ci_failed: 'CI failed on merge',
  }
  return detail ? `${text[kind] ?? kind} ${detail}` : (text[kind] ?? kind)
}

function percent(value: number): string {
  return `${Math.round(value * 100)}%`
}

/**
 * The decision model in shadow (factory F3): Jev's merge verdict on every
 * reviewed PR, its real outcome, and how close each task class is to the bar
 * that allows automatic routing (OD-5). A person can say whether a change was
 * actually risky; that label overrides the automatic signals.
 */
export function ShadowRouterPanel({ client, canWrite }: { client: NexusMindClient; canWrite: boolean }) {
  const queryClient = useQueryClient()
  const report = useQuery({ queryKey: ['factory-shadow-report'], queryFn: () => client.getShadowReport() })
  const decisions = useQuery({ queryKey: ['factory-shadow-decisions'], queryFn: () => client.listShadowDecisions(50) })

  const label = useMutation({
    mutationFn: ({ id, value }: { id: string; value: 'low' | 'high' | null }) => client.labelShadowDecision(id, value),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['factory-shadow-report'] })
      queryClient.invalidateQueries({ queryKey: ['factory-shadow-decisions'] })
    },
  })

  const bar = report.data
  const classes = bar?.classes ?? []
  const rows = decisions.data ?? []

  return (
    <section aria-labelledby="shadow-router-title" className="rounded-xl border border-border-primary p-4 space-y-4">
      <div>
        <h2 id="shadow-router-title" className="text-sm font-semibold text-text-primary flex items-center gap-2">
          <Scale className="h-4 w-4" />Decision model · shadow
        </h2>
        <p className="mt-1 max-w-2xl text-xs text-text-tertiary">
          The decision model judges every reviewed pull request without changing what the factory does. A task class
          can be routed automatically only after {bar?.min_settled_allows ?? 50} settled “allow” decisions with at most{' '}
          {percent(bar?.max_false_low_rate ?? 0.02)} that turned out to be risky.
        </p>
      </div>

      {(report.isLoading || decisions.isLoading) && <p className="text-xs text-text-tertiary">Loading…</p>}
      {(report.isError || decisions.isError) && (
        <p role="alert" className="text-xs text-text-primary">Could not load the shadow decisions.</p>
      )}

      {report.isSuccess && decisions.isSuccess && classes.length === 0 && rows.length === 0 && (
        <p className="text-xs text-text-tertiary">
          No shadow decisions yet. They are recorded after each pull request review, or in bulk with{' '}
          <code>factory-shadow backfill</code>.
        </p>
      )}

      {classes.length > 0 && (
        <table className="w-full text-xs" aria-label="Shadow results by task class">
          <thead>
            <tr className="text-left text-text-tertiary">
              <th className="py-1 font-medium">Class</th>
              <th className="py-1 font-medium">Decisions</th>
              <th className="py-1 font-medium">Allowed</th>
              <th className="py-1 font-medium">Settled allows</th>
              <th className="py-1 font-medium">Turned out risky</th>
              <th className="py-1 font-medium">Status</th>
            </tr>
          </thead>
          <tbody>
            {classes.map(row => (
              <tr key={row.task_class} className="border-t border-border-primary">
                <td className="py-1.5 font-mono text-text-primary">{row.task_class}</td>
                <td className="py-1.5">{row.decisions}</td>
                <td className="py-1.5">{row.allowed}</td>
                <td className="py-1.5">
                  {row.settled_allowed} / {bar?.min_settled_allows ?? 50}
                </td>
                <td className="py-1.5">
                  {row.false_low}
                  {row.false_low_rate !== null && <span className="text-text-tertiary"> ({percent(row.false_low_rate)})</span>}
                </td>
                <td className="py-1.5">{row.meets_od5 ? 'Ready for automatic routing' : 'Collecting evidence'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {rows.length > 0 && (
        <ul aria-label="Recent shadow decisions" className="m-0 p-0 list-none divide-y divide-border-primary">
          {rows.map(row => (
            <DecisionRow
              key={row.id}
              row={row}
              canWrite={canWrite}
              // One label write at a time: concurrent clicks could land out of order.
              busy={label.isPending}
              onLabel={value => label.mutate({ id: row.id, value })}
            />
          ))}
        </ul>
      )}
      {label.isError && (
        <p role="alert" className="text-xs text-text-primary">The label was not saved. Try again.</p>
      )}
    </section>
  )
}

function DecisionRow({
  row,
  canWrite,
  busy,
  onLabel,
}: {
  row: ShadowDecision
  canWrite: boolean
  busy: boolean
  onLabel: (value: 'low' | 'high' | null) => void
}) {
  const name = `${row.repository}#${row.pull_number}`
  const url = `https://github.com/${row.repository}/pull/${row.pull_number}`
  return (
    <li className="py-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
      <a href={url} target="_blank" rel="noreferrer" className="font-mono text-text-primary underline-offset-2 hover:underline">
        {row.repository}#{row.pull_number}
      </a>
      <span className="font-mono text-text-tertiary">{row.task_class}</span>
      <span className="text-text-tertiary">risk {percent(row.risk)}</span>
      <span className="rounded border border-border-primary px-1.5 py-0.5 font-medium">{row.verdict === 'allow' ? 'Would allow' : 'Would hold'}</span>
      <span className="text-text-secondary">
        {OUTCOME_LABEL[row.outcome]}
        {row.outcome_signals.length > 0 && <span className="text-text-tertiary"> · {row.outcome_signals.map(signalLabel).join(', ')}</span>}
      </span>
      <span className="ml-auto flex items-center gap-1.5">
        {row.human_label && (
          <span className="text-text-tertiary">Labelled {row.human_label === 'high' ? 'risky' : 'fine'}</span>
        )}
        {canWrite && COUNTED.includes(row.outcome) && (
          <>
            <Button size="sm" variant="secondary" aria-label={`${name} was risky`} disabled={busy || row.human_label === 'high'} onClick={() => onLabel('high')}>
              Was risky
            </Button>
            <Button size="sm" variant="secondary" aria-label={`${name} was fine`} disabled={busy || row.human_label === 'low'} onClick={() => onLabel('low')}>
              Was fine
            </Button>
            {row.human_label && (
              <Button size="sm" variant="ghost" aria-label={`Clear the label of ${name}`} disabled={busy} onClick={() => onLabel(null)}>
                Clear
              </Button>
            )}
          </>
        )}
      </span>
    </li>
  )
}
