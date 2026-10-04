import { describe, expect, it, vi } from 'vitest'
import { render, screen, within, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ShadowRouterPanel } from './ShadowRouterPanel'
import type { NexusMindClient } from '../../api/client'
import type { ShadowDecision, ShadowReport } from '../../types'

const report: ShadowReport = {
  min_settled_allows: 50,
  max_false_low_rate: 0.02,
  classes: [
    { task_class: 'docs', decisions: 60, allowed: 55, settled_allowed: 52, false_low: 1, false_low_rate: 1 / 52, meets_od5: true },
    { task_class: 'ui', decisions: 9, allowed: 4, settled_allowed: 2, false_low: 1, false_low_rate: 0.5, meets_od5: false },
  ],
}

const decision: ShadowDecision = {
  id: 'd1',
  repository: 'acme/app',
  pull_number: 42,
  head_sha: '0'.repeat(40),
  task_class: 'ui',
  risk: 0.1,
  risk_confidence: 0.95,
  verdict: 'allow',
  floor: 'path_not_allowlisted',
  outcome: 'high_risk',
  outcome_signals: ['reverted_by:1a2b3c', 'merge_commit_ci_failed'],
  merged_at: '2026-09-01T00:00:00Z',
  human_label: null,
  created_at: '2026-09-01 00:00:00',
}

function setup(overrides: Partial<NexusMindClient> = {}, canWrite = true) {
  const client = {
    getShadowReport: vi.fn().mockResolvedValue(report),
    listShadowDecisions: vi.fn().mockResolvedValue([decision]),
    labelShadowDecision: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as NexusMindClient
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <ShadowRouterPanel client={client} canWrite={canWrite} />
    </QueryClientProvider>,
  )
  return client
}

describe('ShadowRouterPanel', () => {
  it('shows each class against the OD-5 bar', async () => {
    setup()
    const table = await screen.findByRole('table', { name: /shadow results by task class/i })
    const docs = within(table).getByText('docs').closest('tr')!
    expect(within(docs).getByText('52 / 50')).toBeInTheDocument()
    expect(within(docs).getByText(/ready for automatic routing/i)).toBeInTheDocument()
    const ui = within(table).getByText('ui').closest('tr')!
    expect(within(ui).getByText(/collecting evidence/i)).toBeInTheDocument()
    expect(within(ui).getByText('(50%)')).toBeInTheDocument()
  })

  it('lists decisions with their outcome signals and lets a person label them', async () => {
    const client = setup()
    const list = await screen.findByRole('list', { name: /recent shadow decisions/i })
    expect(within(list).getByRole('link', { name: 'acme/app#42' })).toHaveAttribute('href', 'https://github.com/acme/app/pull/42')
    expect(within(list).getByText(/reverted by 1a2b3c, CI failed on merge/)).toBeInTheDocument()
    expect(within(list).getByText('Would allow')).toBeInTheDocument()
    await userEvent.click(within(list).getByRole('button', { name: 'acme/app#42 was fine' }))
    await waitFor(() => expect(client.labelShadowDecision).toHaveBeenCalledWith('d1', 'low'))
  })

  it('is read-only without the write permission', async () => {
    setup({}, false)
    await screen.findByRole('list', { name: /recent shadow decisions/i })
    expect(screen.queryByRole('button', { name: /was risky/i })).not.toBeInTheDocument()
  })

  it('explains how decisions arrive when there are none', async () => {
    setup({
      getShadowReport: vi.fn().mockResolvedValue({ ...report, classes: [] }),
      listShadowDecisions: vi.fn().mockResolvedValue([]),
    } as Partial<NexusMindClient>)
    expect(await screen.findByText(/no shadow decisions yet/i)).toBeInTheDocument()
  })

  it('offers no label for decisions the report does not count', async () => {
    setup({ listShadowDecisions: vi.fn().mockResolvedValue([{ ...decision, outcome: 'superseded', outcome_signals: [] }]) } as Partial<NexusMindClient>)
    const list = await screen.findByRole('list', { name: /recent shadow decisions/i })
    expect(within(list).getByText('Another head was merged')).toBeInTheDocument()
    expect(within(list).queryByRole('button')).not.toBeInTheDocument()
  })
})
