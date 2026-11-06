import { render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MemoryHealthCard } from './MemoryHealthCard'
import { MemoryTrendChart } from './MemoryTrendChart'
import { UsageTrendChart } from '../usage/UsageTrendChart'

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class {
    observe() {}
    unobserve() {}
    disconnect() {}
  })
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 640, height: 268, top: 0, left: 0, right: 640, bottom: 268, x: 0, y: 0, toJSON() {} })
})
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })

describe('chart data and accessible alternatives', () => {
  it('preserves zero-valued days instead of dropping them from a trend', () => {
    render(<MemoryTrendChart data={[{ date: '2026-10-08', count: 0 }, { date: '2026-10-09', count: 17 }]} />)
    const table = screen.getByRole('table', { name: 'Memories created per day' })
    expect(within(table).getByRole('row', { name: '2026-10-08 0' })).toBeInTheDocument()
    expect(within(table).getByRole('row', { name: '2026-10-09 17' })).toBeInTheDocument()
  })
  it('reports overlapping health counts separately and does not invent a healthy percentage', () => {
    render(<MemoryHealthCard total={10} duplicates={8} stale={7} untagged={undefined} />)
    const table = screen.getByRole('table', { name: 'Memory health indicators' })
    expect(within(table).getByRole('row', { name: 'Duplicates 8' })).toBeInTheDocument()
    expect(within(table).getByRole('row', { name: 'Stale (>30d) 7' })).toBeInTheDocument()
    expect(within(table).queryByText('Untagged')).not.toBeInTheDocument()
    expect(screen.getByRole('status')).toHaveTextContent('Some health indicators are unavailable')
    expect(screen.queryByText(/healthy/i)).not.toBeInTheDocument()
  })
  it('retains both token series and changes units when the selected metric changes', () => {
    const buckets = [{ bucket_ts: '2026-10-09', tokens_in: 20, tokens_out: 12, tokens_total: 32, duration_ms: 2000, event_count: 3 }]
    const { rerender } = render(<UsageTrendChart buckets={buckets} size="day" metric="tokens" />)
    let table = screen.getByRole('table', { name: /Tokens by day/ })
    expect(within(table).getByRole('cell', { name: '20' })).toBeInTheDocument()
    expect(within(table).getByRole('cell', { name: '12' })).toBeInTheDocument()
    expect(within(table).getByRole('cell', { name: '32' })).toBeInTheDocument()
    rerender(<UsageTrendChart buckets={buckets} size="day" metric="duration" />)
    table = screen.getByRole('table', { name: /Execution time by day/ })
    expect(within(table).getByRole('cell', { name: '2s' })).toBeInTheDocument()
    expect(within(table).queryByText('Tokens in')).not.toBeInTheDocument()
  })
})
