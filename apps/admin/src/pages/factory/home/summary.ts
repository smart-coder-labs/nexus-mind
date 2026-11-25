import type { AutonomousAgentRun } from '../../../types'
import { isActiveRun, parseUtc, runOutcome } from '../shared/format'

export interface DayOutcomes {
  day: string
  success: number
  warning: number
  error: number
  neutral: number
}

const DAY_MS = 86_400_000
const isoDay = (ms: number) => new Date(ms).toISOString().slice(0, 10)

/** Finished runs per UTC day by outcome, the last `days` days oldest first. Empty days stay as real zeros. */
export function dailyOutcomes(runs: AutonomousAgentRun[], days: number, now = Date.now()): DayOutcomes[] {
  const buckets = new Map<string, DayOutcomes>()
  for (let i = days - 1; i >= 0; i--) {
    const day = isoDay(now - i * DAY_MS)
    buckets.set(day, { day, success: 0, warning: 0, error: 0, neutral: 0 })
  }
  for (const run of runs) {
    const outcome = runOutcome(run.status)
    if (outcome === 'active') continue
    const bucket = buckets.get(isoDay(parseUtc(run.finished_at ?? run.created_at) || 0))
    if (bucket) bucket[outcome]++
  }
  return [...buckets.values()]
}

/** Success share of runs finished in the last `days` days; `null` when none finished (shown as "—"). */
export function successRate(runs: AutonomousAgentRun[], days: number, now = Date.now()) {
  const since = now - days * DAY_MS
  const finished = runs.filter(
    run => !isActiveRun(run.status) && parseUtc(run.finished_at ?? run.created_at) >= since,
  )
  const succeeded = finished.filter(run => run.status === 'succeeded').length
  return { finished: finished.length, succeeded, rate: finished.length ? succeeded / finished.length : null }
}
