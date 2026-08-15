import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderWithProviders } from '../test/render'
import Usage from './Usage'

const getUsageSummary = vi.fn()
const getUsageTimeseries = vi.fn()
const runUsageBackfill = vi.fn()
const listClients = vi.fn()
const listProjects = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    getUsageSummary,
    getUsageTimeseries,
    runUsageBackfill,
    listClients,
    listProjects,
  })),
}))

vi.mock('../auth/AuthContext', async () => {
  const actual = await vi.importActual<typeof import('../auth/AuthContext')>('../auth/AuthContext')
  return {
    ...actual,
    useAuth: () => ({ session: { user: { role: 'admin', id: 'u1' } } }),
  }
})

function summary(rows: unknown[], totals: Record<string, number>) {
  return { rows, totals }
}

const EMPTY_TOTALS = {
  tokens_in: 0, tokens_out: 0, tokens_total: 0, duration_ms: 0, event_count: 0,
}

describe('Usage', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    listClients.mockResolvedValue([])
    listProjects.mockResolvedValue([])
    getUsageTimeseries.mockResolvedValue({ bucket: 'day', buckets: [] })
    getUsageSummary.mockResolvedValue(summary([], EMPTY_TOTALS))
  })

  it('defaults to a bounded 30-day window and requests a matching trend', async () => {
    renderWithProviders(<Usage />)

    await waitFor(() => expect(getUsageTimeseries).toHaveBeenCalled())

    const call = getUsageTimeseries.mock.calls[0][0]
    expect(call.bucket).toBe('day')
    expect(call.from).toMatch(/^\d{4}-\d{2}-\d{2}$/)
    expect(call.to).toMatch(/^\d{4}-\d{2}-\d{2}$/)

    // The summary is scoped to the same window — the KPI totals and the chart
    // must never describe different ranges.
    const summaryCall = getUsageSummary.mock.calls.find(c => c[0].level === 'project')![0]
    expect(summaryCall.from).toBe(call.from)
    expect(summaryCall.to).toBe(call.to)
  })

  it('shows an actionable empty state and can widen the range to all time', async () => {
    renderWithProviders(<Usage />)

    expect(await screen.findByText('No usage in this range')).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: 'Show all time' }))

    await waitFor(() => {
      const last = getUsageTimeseries.mock.calls[getUsageTimeseries.mock.calls.length - 1][0]
      expect(last.from).toBeUndefined()
      expect(last.to).toBeUndefined()
    })
  })

  const ROW = {
    key_id: 'p1', key_name: 'acme-web',
    tokens_in: 60, tokens_out: 40, tokens_total: 100, duration_ms: 500, event_count: 2,
  }
  const ROW_TOTALS = {
    tokens_in: 60, tokens_out: 40, tokens_total: 100, duration_ms: 500, event_count: 2,
  }
  const ONE_BUCKET = {
    bucket: 'day',
    buckets: [{ bucket_ts: '2026-08-12', ...ROW_TOTALS }],
  }

  /** Wires every query to return one project, one model and one day of usage. */
  function seedUsage() {
    getUsageSummary.mockImplementation(({ level }: { level: string }) =>
      Promise.resolve(
        level === 'model'
          ? summary([{ ...ROW, key_id: 'opus', key_name: 'opus' }], ROW_TOTALS)
          : summary([ROW], ROW_TOTALS),
      ),
    )
    getUsageTimeseries.mockResolvedValue(ONE_BUCKET)
  }

  it('renders the panel with charts once usage exists', async () => {
    seedUsage()
    renderWithProviders(<Usage />)

    expect(await screen.findByText('Usage over time')).toBeInTheDocument()
    expect(screen.getByText('By model')).toBeInTheDocument()

    // The chart carries an accessible summary — its numbers are reachable
    // without seeing the marks. The bucket count is the gap-filled window, not
    // the single bucket the backend returned: idle days are plotted as zeroes.
    expect(
      await screen.findByRole('img', { name: /^Tokens by day, 30 buckets, / }),
    ).toBeInTheDocument()

    // The KPI total comes from the summary rollup, so it is range-exact.
    expect(screen.getAllByText('Total tokens').length).toBeGreaterThan(0)
    expect(screen.getAllByText('100').length).toBeGreaterThan(0)

    // Both series are named in a legend rather than relying on the two shades
    // alone. Scoped to the chart: the detail table reuses the same words.
    const chartCard = screen.getByText('Usage over time').closest('section')!
    expect(within(chartCard).getByText('Tokens in')).toBeInTheDocument()
    expect(within(chartCard).getByText('Tokens out')).toBeInTheDocument()

    // The detail table keeps the auditable per-row numbers.
    expect(screen.getByText('Detail by project')).toBeInTheDocument()
    expect(screen.getAllByText('acme-web').length).toBeGreaterThan(0)
  })

  it('switching the trend metric re-plots from data already fetched', async () => {
    seedUsage()
    renderWithProviders(<Usage />)
    await screen.findByText('Usage over time')
    const callsBefore = getUsageTimeseries.mock.calls.length

    await userEvent.click(screen.getByRole('button', { name: 'Events' }))

    // Every metric rides on the same response, so the toggle costs no request.
    expect(getUsageTimeseries.mock.calls.length).toBe(callsBefore)
    expect(await screen.findByRole('img', { name: /Events by day/ })).toBeInTheDocument()

    // A single series carries no legend — the heading already names it.
    const chartCard = screen.getByText('Usage over time').closest('section')!
    expect(within(chartCard).queryByText('Tokens in')).not.toBeInTheDocument()
  })

  it('changing the bucket refetches at the new granularity', async () => {
    seedUsage()
    renderWithProviders(<Usage />)
    await screen.findByText('Usage over time')

    await userEvent.click(screen.getByRole('button', { name: 'Week' }))

    await waitFor(() =>
      expect(
        getUsageTimeseries.mock.calls[getUsageTimeseries.mock.calls.length - 1][0].bucket,
      ).toBe('week'),
    )
  })
})
