import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { AutonomousAgentRun } from '../../../types'
import { OutcomeTape, TAPE_LENGTH, tapeFor } from './OutcomeTape'

const run = (status: string, created_at: string): AutonomousAgentRun => ({
  id: `${status}-${created_at}`, definition_id: 'a', revision_id: 'r', trigger_kind: 'manual', occurrence_key: 'k',
  scheduled_for: null, snapshot_sha: null, status, budget: {}, started_at: null, finished_at: null, created_at, archived_at: null,
})

describe('OutcomeTape', () => {
  it('orders oldest first and pads missing runs on the left', () => {
    const cells = tapeFor([run('failed', '2026-10-02 10:00:00'), run('succeeded', '2026-10-01 10:00:00'), run('running', '2026-10-03 10:00:00')])
    expect(cells).toHaveLength(TAPE_LENGTH)
    expect(cells.slice(-3)).toEqual(['success', 'error', 'active'])
    expect(cells.slice(0, TAPE_LENGTH - 3).every(c => c === 'empty')).toBe(true)
  })

  it('keeps only the last 14 runs', () => {
    const runs = Array.from({ length: 20 }, (_, i) => run(i < 6 ? 'failed' : 'succeeded', `2026-09-${String(i + 1).padStart(2, '0')} 10:00:00`))
    expect(tapeFor(runs).every(c => c === 'success')).toBe(true)
  })

  it('reads the outcomes in words', () => {
    render(<OutcomeTape runs={[run('succeeded', '2026-10-01 10:00:00'), run('budget_exhausted', '2026-10-02 10:00:00'), run('cancelled', '2026-10-03 10:00:00')]} />)
    expect(screen.getByRole('img')).toHaveAccessibleName(/oldest first: succeeded, partly done or out of budget, cancelled/i)
  })
})
