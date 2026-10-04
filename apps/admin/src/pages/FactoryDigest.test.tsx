import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, within, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import FactoryDigest from './FactoryDigest'
import type { FactoryDigest as Digest, FactoryEconomics } from '../types'

const api = vi.hoisted(() => ({
  getFactoryDigest: vi.fn(),
  getFactoryEconomics: vi.fn(),
  decideFactoryMerge: vi.fn(),
}))
vi.mock('../api/client', () => ({ createClient: () => api }))

const auth = vi.hoisted(() => ({ permissions: [] as string[] }))
vi.mock('../auth/AuthContext', () => ({
  useAuth: () => ({ session: { user: { permissions: auth.permissions } } }),
}))

const SUBJECT = 'acme/app#7@0123456789abcdef0123456789abcdef01234567'
const digest: Digest = {
  held_merges: [{ subject: SUBJECT, reason: 'decision_model_not_configured', source: 'policy', created_at: '2026-10-04 01:00:00' }],
  blocked_runs: [{ run_id: 'r1', agent: 'Kasymir resolver', template_key: 'github_issue_resolver', status: 'blocked_policy', reason: 'judge_targets_required', finished_at: '2026-10-03T20:00:00Z' }],
  factory_tasks: [{ id: 't1', project: 'app', title: 'Document the refund flow', status: 'backlog', created_at: '2026-10-03' }],
  unlabeled_shadow: 120,
  unlabeled_shadow_allows: 4,
}
const economics: FactoryEconomics = {
  days: 30,
  runs: 12,
  cost_usd: 3.2,
  by_model: [{ model: 'claude-opus-5[1m]', runs: 7, cost_usd: 1.9, input_tokens: 1, output_tokens: 1 }],
  tier_choices: [['frontier', 2], ['standard', 6]],
  frontier_avoidance: 0.75,
  proposed_changes: 4,
  cost_per_proposed_change: 0.8,
  accepted_changes_tracked: false,
}

function renderPage(permissions: string[]) {
  auth.permissions = permissions
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter><FactoryDigest /></MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('FactoryDigest', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.getFactoryDigest.mockResolvedValue(digest)
    api.getFactoryEconomics.mockResolvedValue(economics)
    api.decideFactoryMerge.mockResolvedValue({ id: 'd1' })
  })

  it('lists held merges, blocked runs, waiting tasks and labels to give', async () => {
    renderPage(['factory_policy:read'])
    const held = await screen.findByRole('region', { name: /merges held for a person/i })
    expect(within(held).getByRole('link', { name: 'acme/app#7' })).toHaveAttribute('href', 'https://github.com/acme/app/pull/7')
    expect(within(held).queryByRole('button')).not.toBeInTheDocument()
    expect(screen.getByText('judge_targets_required')).toBeInTheDocument()
    expect(screen.getByText('Document the refund flow')).toBeInTheDocument()
    expect(screen.getByText(/120 shadow decisions have no human label/)).toBeInTheDocument()
  })

  it('lets a person with write access approve one merge', async () => {
    renderPage(['factory_policy:read', 'factory_policy:write'])
    await userEvent.click(await screen.findByRole('button', { name: `Approve merging ${SUBJECT}` }))
    await waitFor(() => expect(api.decideFactoryMerge).toHaveBeenCalledWith(SUBJECT, true))
  })

  it('shows spend and says accepted changes are not measured yet', async () => {
    renderPage(['factory_policy:read'])
    const card = await screen.findByRole('region', { name: /economics/i })
    expect(within(card).getByText('75%')).toBeInTheDocument()
    expect(within(card).getByText('$0.800')).toBeInTheDocument()
    expect(within(card).getByText(/not measured yet/i)).toBeInTheDocument()
  })

  it('says when nothing waits and explains the missing permission', async () => {
    api.getFactoryDigest.mockResolvedValue({ held_merges: [], blocked_runs: [], factory_tasks: [], unlabeled_shadow: 0, unlabeled_shadow_allows: 0 })
    renderPage(['factory_policy:read'])
    expect(await screen.findByText(/nothing is waiting on a person/i)).toBeInTheDocument()
  })

  it('needs the factory read permission', () => {
    renderPage([])
    expect(screen.getByText(/factory_policy:read permission/i)).toBeInTheDocument()
    expect(api.getFactoryDigest).not.toHaveBeenCalled()
  })
})
