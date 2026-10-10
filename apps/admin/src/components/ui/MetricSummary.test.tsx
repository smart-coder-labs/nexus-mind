import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { MetricSummary } from './MetricSummary'

describe('static metric summaries', () => {
  it('keeps the count and supporting context visible without any disclosure controls', () => {
    render(<MetricSummary label="Memories" value="12,345"><p>28 new this week</p></MetricSummary>)
    expect(screen.getByText('12,345')).toBeVisible()
    expect(screen.getByText('28 new this week')).toBeVisible()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })
  it('keeps a zero count without adding interaction', () => {
    render(<MetricSummary label="Projects" value={0} />)
    expect(screen.getByText('0')).toBeVisible()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })
})
