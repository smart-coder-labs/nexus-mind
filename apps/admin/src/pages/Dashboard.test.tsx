import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderWithProviders } from '../test/render'
import Dashboard from './Dashboard'

const getDashboard = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({ getDashboard })),
}))

describe('Dashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getDashboard.mockResolvedValue({
      stats: { total_memories: 0, active_users_24h: 0, searches_today: 0, top_tools: [] },
      usage: null,
      trends: { daily_counts: [{ date: '2026-07-20', count: 2 }], by_type: [], by_project: [], total: 2, this_week: 2, this_month: 2 },
      activity: [], agent_activity: null, heatmap: null, contributors: null, health: null, users: null,
      onboarding: null, conventions: null,
      availability: { usage: false, users: false, onboarding: false, agent_activity: false, health: false, contributors: false, heatmap: false, conventions: false },
    })
  })

  it('requests the selected period and renders restricted scoped widgets', async () => {
    renderWithProviders(<Dashboard />)

    await waitFor(() => expect(getDashboard).toHaveBeenCalledWith(30))
    expect((await screen.findAllByText('Unavailable for scoped administrators.')).length).toBeGreaterThan(0)
    expect(screen.getByText('2 total')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Export config' })).not.toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: '7d' }))
    await waitFor(() => expect(getDashboard).toHaveBeenCalledWith(7))
  })
})
