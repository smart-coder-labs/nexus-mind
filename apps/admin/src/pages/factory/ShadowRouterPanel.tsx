import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Scale } from 'lucide-react'
import type { NexusMindClient } from '../../api/client'
import type { ShadowClassReport, ShadowDecision, ShadowOutcome, ShadowReport } from '../../types'
import { Badge } from '../../components/ui/Badge'
import { Button } from '../../components/ui/Button'
import { EmptyState } from '../../components/ui/EmptyState'
import { SegmentedControl } from '../../components/ui/SegmentedControl'
import { Skeleton } from '../../components/ui/Skeleton'
import { cn } from '../../lib/utils'
import { InlineAlert, LINK_CLASS, SectionHeading } from './govern/ui'
import { taskClassLabel } from './govern/words'

const OUTCOME_LABEL: Record<ShadowOutcome, string> = {
  pending: 'Not settled yet',
  clean: 'Clean',
  high_risk: 'Turned out risky',
  not_merged: 'Not merged',
  superseded: 'Another head was merged',
  unresolvable: 'Could not be read',
}

const OUTCOME_VARIANT: Record<ShadowOutcome, 'default' | 'success' | 'error'> = {
  pending: 'default',
  clean: 'success',
  high_risk: 'error',
  not_merged: 'default',
  superseded: 'default',
  unresolvable: 'default',
}

/** Outcomes the report counts; only those are worth a person's label. */
const COUNTED: ShadowOutcome[] = ['pending', 'clean', 'high_risk']

const needsLabel = (row: ShadowDecision) => COUNTED.includes(row.outcome) && !row.human_label

/** `reverted_by:1a2b3c` → `reverted by 1a2b3c`. */
function signalLabel(signal: string): string {
  const [kind, detail] = signal.split(':')
  const text: Record<string, string> = {
    reverted_by: 'reverted by',
    follow_up_fix: 'fixed by',
    merge_commit_ci_failed: 'CI failed on merge',
  }
  return detail ? `${text[kind] ?? kind.replace(/_/g, ' ')} ${detail}` : (text[kind] ?? kind.replace(/_/g, ' '))
}

function percent(value: number): string {
  return `${Math.round(value * 100)}%`
}

type ClassStatus = 'ready' | 'over' | 'collecting'

function classStatus(row: ShadowClassReport, bar: ShadowReport): ClassStatus {
  if (row.meets_od5) return 'ready'
  if (row.false_low_rate !== null && row.false_low_rate > bar.max_false_low_rate) return 'over'
  return 'collecting'
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
  const [filter, setFilter] = useState<'all' | 'unlabelled'>('all')

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
  const unlabelled = rows.filter(needsLabel)
  const visible = filter === 'unlabelled' ? unlabelled : rows
  const loading = report.isLoading || decisions.isLoading
  const failed = report.isError || decisions.isError
  const empty = report.isSuccess && decisions.isSuccess && classes.length === 0 && rows.length === 0

  const minAllows = bar?.min_settled_allows ?? 50
  const maxRate = bar?.max_false_low_rate ?? 0.02
  const ready = classes.filter(row => row.meets_od5).length

  return (
    <div className="space-y-8">
      <p className="max-w-3xl text-[13px] leading-normal text-text-secondary">
        A task class may be routed automatically only after <span className="text-text-primary tabular-nums">{minAllows}</span> settled
        “allow” decisions, with at most <span className="text-text-primary tabular-nums">{percent(maxRate)}</span> of them turning out
        risky.
        {classes.length > 0 && (
          <> {ready === 0 ? 'No task class is there yet.' : `${ready} of ${classes.length} task classes ${ready === 1 ? 'is' : 'are'} there.`}</>
        )}
      </p>

      {failed && (
        <InlineAlert onRetry={() => { report.refetch(); decisions.refetch() }}>Could not load the shadow decisions.</InlineAlert>
      )}

      {loading && (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3" aria-hidden="true">
          {[0, 1, 2].map(index => <Skeleton key={index} className="h-44 rounded-[18px]" />)}
        </div>
      )}

      {empty && (
        <EmptyState
          icon={<Scale />}
          title="No shadow decisions yet"
          description="They are recorded after each pull request review, or in bulk with the factory-shadow backfill command."
        />
      )}

      {bar && classes.length > 0 && (
        <section aria-labelledby="readiness-title" className="space-y-3">
          <SectionHeading id="readiness-title" title="Readiness by task class" />
          <ul aria-label="Readiness by task class" className="m-0 grid list-none gap-4 p-0 sm:grid-cols-2 xl:grid-cols-3">
            {classes.map(row => <ReadinessCard key={row.task_class} row={row} bar={bar} />)}
          </ul>
        </section>
      )}

      {rows.length > 0 && (
        <section aria-labelledby="decisions-title" className="space-y-3">
          <SectionHeading
            id="decisions-title"
            title="Recent decisions"
            description="Your label overrides the automatic signals. Only decisions the bar counts can be labelled."
            action={
              <SegmentedControl
                size="sm"
                value={filter}
                onChange={setFilter}
                options={[
                  { value: 'all', label: `All (${rows.length})` },
                  { value: 'unlabelled', label: `Needs a label (${unlabelled.length})` },
                ]}
              />
            }
          />
          {visible.length === 0 ? (
            <p className="rounded-[18px] border border-border-primary px-5 py-6 text-center text-[13px] text-text-tertiary">
              Every recent decision the bar counts has a label.
            </p>
          ) : (
            <div className="overflow-x-auto rounded-[18px] border border-border-primary">
              <table className="w-full min-w-[56rem] text-[13px]" aria-label="Recent shadow decisions">
                <thead>
                  <tr className="border-b border-border-secondary text-left text-[12px] text-text-tertiary">
                    <th scope="col" className="px-4 py-2.5 font-medium">Pull request</th>
                    <th scope="col" className="px-4 py-2.5 font-medium">Task class</th>
                    <th scope="col" className="px-4 py-2.5 font-medium">Risk</th>
                    <th scope="col" className="px-4 py-2.5 font-medium">Model said</th>
                    <th scope="col" className="px-4 py-2.5 font-medium">What happened</th>
                    <th scope="col" className="px-4 py-2.5 font-medium">Your label</th>
                  </tr>
                </thead>
                <tbody>
                  {visible.map(row => (
                    <DecisionRow
                      key={row.id}
                      row={row}
                      canWrite={canWrite}
                      // One label write at a time: concurrent clicks could land out of order.
                      busy={label.isPending}
                      onLabel={value => label.mutate({ id: row.id, value })}
                    />
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {label.isError && <InlineAlert>The label was not saved. Try again.</InlineAlert>}
        </section>
      )}
    </div>
  )
}

const STATUS_COPY: Record<ClassStatus, { text: string; variant: 'success' | 'error' | 'default' }> = {
  ready: { text: 'Ready for automatic routing', variant: 'success' },
  over: { text: 'Over the risk limit', variant: 'error' },
  collecting: { text: 'Collecting evidence', variant: 'default' },
}

function ReadinessCard({ row, bar }: { row: ShadowClassReport; bar: ShadowReport }) {
  const status = classStatus(row, bar)
  const missing = Math.max(0, bar.min_settled_allows - row.settled_allowed)
  const progress = Math.min(1, row.settled_allowed / Math.max(1, bar.min_settled_allows))
  // Scale the risk bar so the limit sits at a quarter, unless the observed rate is higher.
  const scale = Math.min(1, Math.max(bar.max_false_low_rate * 4, row.false_low_rate ?? 0, 0.01))
  const rateWidth = row.false_low_rate === null ? 0 : Math.min(1, row.false_low_rate / scale)
  const limitAt = Math.min(1, bar.max_false_low_rate / scale)
  const nameId = `readiness-${row.task_class}`

  return (
    <li aria-labelledby={nameId} className="rounded-[18px] border border-border-primary bg-white/[0.02] p-5 space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 id={nameId} className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{taskClassLabel(row.task_class)}</h3>
          <p className="mt-0.5 text-[12px] text-text-tertiary">
            {row.decisions} decisions, {row.allowed} would allow
          </p>
        </div>
        <Badge role="none" size="sm" variant={STATUS_COPY[status].variant}>{STATUS_COPY[status].text}</Badge>
      </div>

      <div className="space-y-1.5">
        <div className="flex items-baseline justify-between gap-2 text-[12px]">
          <span className="text-text-secondary">Settled allows</span>
          <span className="tabular-nums text-text-primary">{row.settled_allowed} / {bar.min_settled_allows}</span>
        </div>
        <div className="h-1.5 overflow-hidden rounded-full bg-white/[0.06]" aria-hidden="true">
          <div className="h-full rounded-full bg-text-secondary" style={{ width: `${progress * 100}%` }} />
        </div>
        {status !== 'ready' && missing > 0 && (
          <p className="text-[12px] text-text-tertiary">{missing} more needed</p>
        )}
      </div>

      <div className="space-y-1.5">
        <div className="flex items-baseline justify-between gap-2 text-[12px]">
          <span className="text-text-secondary">Turned out risky</span>
          <span className="tabular-nums text-text-primary">
            {row.false_low}
            {row.false_low_rate !== null && <span className="text-text-tertiary"> ({percent(row.false_low_rate)})</span>}
          </span>
        </div>
        <div className="relative h-1.5 rounded-full bg-white/[0.06]" aria-hidden="true">
          <div
            className={cn('h-full rounded-full', status === 'over' ? 'bg-status-error' : 'bg-text-secondary')}
            style={{ width: `${rateWidth * 100}%` }}
          />
          <div className="absolute -top-1 h-3.5 w-0.5 rounded-full bg-text-primary" style={{ left: `calc(${limitAt * 100}% - 1px)` }} />
        </div>
        <p className="text-[12px] text-text-tertiary">Limit {percent(bar.max_false_low_rate)}</p>
      </div>
    </li>
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
    <tr className="border-b border-border-secondary align-middle last:border-0 hover:bg-white/[0.03]">
      <td className="px-4 py-2.5">
        <a href={url} target="_blank" rel="noreferrer" className={cn('font-mono', LINK_CLASS)}>
          {row.repository}#{row.pull_number}
        </a>
      </td>
      <td className="px-4 py-2.5 text-text-secondary">{taskClassLabel(row.task_class)}</td>
      <td className="px-4 py-2.5 tabular-nums text-text-secondary">{percent(row.risk)}</td>
      <td className="px-4 py-2.5 text-text-primary">{row.verdict === 'allow' ? 'Would allow' : 'Would hold'}</td>
      <td className="px-4 py-2.5">
        <div className="flex flex-col items-start gap-1">
          <Badge role="none" size="sm" variant={OUTCOME_VARIANT[row.outcome]}>{OUTCOME_LABEL[row.outcome]}</Badge>
          {row.outcome_signals.length > 0 && (
            <span className="text-[12px] text-text-tertiary">{row.outcome_signals.map(signalLabel).join(', ')}</span>
          )}
        </div>
      </td>
      <td className="px-4 py-2.5">
        <div className="flex flex-wrap items-center gap-1.5">
          {row.human_label && (
            <span className="mr-1 text-[12px] text-text-secondary">Labelled {row.human_label === 'high' ? 'risky' : 'fine'}</span>
          )}
          {!row.human_label && !COUNTED.includes(row.outcome) && <span className="text-[12px] text-text-tertiary">Not counted</span>}
          {!row.human_label && COUNTED.includes(row.outcome) && !canWrite && <span className="text-[12px] text-text-tertiary">No label</span>}
          {canWrite && COUNTED.includes(row.outcome) && (
            <>
              <Button size="sm" variant="secondary" aria-label={`${name} was fine`} disabled={busy || row.human_label === 'low'} onClick={() => onLabel('low')}>
                Was fine
              </Button>
              <Button size="sm" variant="secondary" aria-label={`${name} was risky`} disabled={busy || row.human_label === 'high'} onClick={() => onLabel('high')}>
                Was risky
              </Button>
              {row.human_label && (
                <Button size="sm" variant="ghost" aria-label={`Clear the label of ${name}`} disabled={busy} onClick={() => onLabel(null)}>
                  Clear label
                </Button>
              )}
            </>
          )}
        </div>
      </td>
    </tr>
  )
}
