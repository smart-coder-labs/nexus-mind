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
  it('states merge eligibility only when CI checks are part of the report', () => {
    const report = stored(true)
    report.report.checks.push({ name: 'ci:build', status: 'PASS' })
    render(<VerificationReports reports={[report]} />)
    expect(screen.getByText(/eligible for merge/i)).toBeInTheDocument()
  })

  it('keeps repeated commands as separate rows', () => {
    const report = stored(true)
    report.report.checks.push({ name: 'cmd:npm test', status: 'FAIL' })
    render(<VerificationReports reports={[report]} />)
    expect(screen.getAllByText('npm test')).toHaveLength(2)
  })

  it('renders nothing when the run has no report', () => {
    const { container } = render(<VerificationReports reports={[]} />)
    expect(container).toBeEmptyDOMElement()
  })

  it('shows a passing report with its checks and the short head', () => {
    render(<VerificationReports reports={[stored(true)]} />)
    const section = screen.getByRole('region', { name: /verification/i })
    expect(within(section).getByText('All checks passed')).toBeInTheDocument()
    expect(within(section).getByText('0123456789ab')).toBeInTheDocument()
    expect(within(section).getByText('npm test')).toBeInTheDocument()
    expect(within(section).getByText('Passed')).toBeInTheDocument()
    // Without CI checks the report speaks for the sandbox commands only.
    expect(within(section).getByText(/sandbox commands only/i)).toBeInTheDocument()
    expect(within(section).queryByText(/eligible for merge/i)).not.toBeInTheDocument()
  })

  it('names what blocks a failing report', () => {
    render(<VerificationReports reports={[stored(false)]} />)
    const section = screen.getByRole('region', { name: /verification/i })
    expect(within(section).getByText('Blocked')).toBeInTheDocument()
    expect(within(section).getByText('Failed')).toBeInTheDocument()
    expect(within(section).getByText('Errored')).toBeInTheDocument()
    expect(within(section).getByText(/needs a human/i)).toBeInTheDocument()
    expect(within(section).getByText('CI · build')).toBeInTheDocument()
  })
})
