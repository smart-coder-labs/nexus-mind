import { describe, expect, it } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import { VerificationReports } from './VerificationReports'
import type { StoredVerificationReport } from '../../types'

function stored(passed: boolean): StoredVerificationReport {
  return {
    head_sha: '0123456789abcdef0123456789abcdef01234567',
    created_at: '2026-10-02 04:26:44',
    report: {
      schema_version: 1,
      task_id: '73c1e1a7-8751-487e-a1d0-58146f5b4e22',
      head_sha: '0123456789abcdef0123456789abcdef01234567',
      passed,
      checks: passed
        ? [{ name: 'cmd:npm test', status: 'PASS', duration_ms: 120765 }]
        : [
            { name: 'cmd:npm test', status: 'FAIL', duration_ms: 4200 },
            { name: 'ci:build', status: 'ERROR' },
          ],
      blocking_failures: passed ? [] : ['cmd:npm test', 'ci:build'],
      eligible_for_merge: passed,
      human_approval_required: !passed,
    },
  }
}

describe('VerificationReports', () => {
  it('renders nothing when the run has no report', () => {
    const { container } = render(<VerificationReports reports={[]} />)
    expect(container).toBeEmptyDOMElement()
  })

  it('shows a passing report with its checks and the short head', () => {
    render(<VerificationReports reports={[stored(true)]} />)
    const section = screen.getByRole('region', { name: /verification/i })
    expect(within(section).getByText('Passed')).toBeInTheDocument()
    expect(within(section).getByText('0123456789ab')).toBeInTheDocument()
    expect(within(section).getByText('npm test')).toBeInTheDocument()
    expect(within(section).getByText('PASS')).toBeInTheDocument()
    expect(within(section).getByText(/eligible for merge/i)).toBeInTheDocument()
  })

  it('names what blocks a failing report', () => {
    render(<VerificationReports reports={[stored(false)]} />)
    const section = screen.getByRole('region', { name: /verification/i })
    expect(within(section).getByText('Blocked')).toBeInTheDocument()
    expect(within(section).getByText('FAIL')).toBeInTheDocument()
    expect(within(section).getByText('ERROR')).toBeInTheDocument()
    expect(within(section).getByText(/needs a human/i)).toBeInTheDocument()
    expect(within(section).getByText('CI · build')).toBeInTheDocument()
  })
})
