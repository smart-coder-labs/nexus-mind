import type { AutonomousAgentRun } from '../../../types'
import { parseUtc, runOutcome, type Outcome } from '../shared/format'

export const TAPE_LENGTH = 14

// The outcome tape (DESIGN_DIRECTION §11): the factory's one distinctive visual.
// State is the only color; "in progress" and "no run" are told apart by shape.
const CELL: Record<Outcome | 'empty', string> = {
  success: 'bg-status-success',
  warning: 'bg-status-warning',
  error: 'bg-status-error',
  neutral: 'bg-text-tertiary',
  active: 'border border-text-secondary bg-transparent',
  empty: 'border border-border-primary bg-white/[0.03]',
}

const WORD: Record<Outcome, string> = {
  success: 'succeeded',
  warning: 'partly done or out of budget',
  error: 'failed or blocked',
  neutral: 'cancelled',
  active: 'in progress',
}

/** The agent's last N runs, oldest first (left), padded on the left with empty slots. */
export function tapeFor(runs: AutonomousAgentRun[], length = TAPE_LENGTH): Array<Outcome | 'empty'> {
  const recent = [...runs]
    .sort((a, b) => parseUtc(a.created_at) - parseUtc(b.created_at))
    .slice(-length)
    .map(run => runOutcome(run.status))
  return [...Array<'empty'>(Math.max(0, length - recent.length)).fill('empty'), ...recent]
}

export function tapeLabel(cells: Array<Outcome | 'empty'>): string {
  const outcomes = cells.filter((c): c is Outcome => c !== 'empty')
  if (!outcomes.length) return 'No runs yet'
  const empty = cells.length - outcomes.length
  return `Last ${outcomes.length} run${outcomes.length === 1 ? '' : 's'}, oldest first: ${outcomes.map(o => WORD[o]).join(', ')}${empty ? `. ${empty} slot${empty === 1 ? '' : 's'} without a run` : ''}.`
}

export function OutcomeTape({ runs }: { runs: AutonomousAgentRun[] }) {
  const cells = tapeFor(runs)
  return (
    <div role="img" aria-label={tapeLabel(cells)} className="flex items-center gap-[3px]">
      {cells.map((cell, index) => (
        <span key={index} className={`h-2.5 w-2.5 shrink-0 rounded-[2px] ${CELL[cell]}`} aria-hidden />
      ))}
    </div>
  )
}

const LEGEND: Array<{ cell: Outcome | 'empty'; label: string }> = [
  { cell: 'success', label: 'Succeeded' },
  { cell: 'warning', label: 'Partly done or out of budget' },
  { cell: 'error', label: 'Failed or blocked' },
  { cell: 'neutral', label: 'Cancelled' },
  { cell: 'active', label: 'In progress' },
  { cell: 'empty', label: 'No run' },
]

export function OutcomeLegend() {
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-[12px] text-text-tertiary">
      <span className="text-text-secondary">Outcome tape shows each agent’s last {TAPE_LENGTH} runs, oldest on the left:</span>
      {LEGEND.map(item => (
        <span key={item.cell} className="inline-flex items-center gap-1.5">
          <span className={`h-2.5 w-2.5 rounded-[2px] ${CELL[item.cell]}`} aria-hidden />
          {item.label}
        </span>
      ))}
    </div>
  )
}
