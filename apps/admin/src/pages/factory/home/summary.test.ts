import { describe, expect, it } from 'vitest'
import type { AutonomousAgentRun } from '../../../types'
import { dailyOutcomes, successRate } from './summary'

const NOW = Date.parse('2026-11-25T12:00:00Z')
const run = (status: string, finished_at: string | null, created_at = '2026-11-25 08:00:00') =>
  ({ id: status + finished_at, status, finished_at, created_at }) as AutonomousAgentRun

const runs = [
  run('succeeded', '2026-11-25 09:00:00'),
  run('failed', '2026-11-25 10:00:00'),
  run('blocked_policy', '2026-11-24 10:00:00'),
  run('partial', '2026-11-24 11:00:00'),
  run('running', null),
  run('succeeded', '2026-10-01 10:00:00'),
]

describe('factory summary', () => {
  it('buckets finished runs by day and outcome, skipping active and out-of-range runs', () => {
    const days = dailyOutcomes(runs, 3, NOW)
    expect(days.map(d => d.day)).toEqual(['2026-11-23', '2026-11-24', '2026-11-25'])
    expect(days[0]).toMatchObject({ success: 0, warning: 0, error: 0, neutral: 0 })
    expect(days[1]).toMatchObject({ error: 1, warning: 1 })
    expect(days[2]).toMatchObject({ success: 1, error: 1 })
  })

  it('computes the success rate over finished runs in the window, null when none', () => {
    expect(successRate(runs, 7, NOW)).toEqual({ finished: 4, succeeded: 1, rate: 0.25 })
    expect(successRate([run('running', null)], 7, NOW).rate).toBeNull()
  })
})
