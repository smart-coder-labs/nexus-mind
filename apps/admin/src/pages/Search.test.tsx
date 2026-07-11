import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor, within, fireEvent } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Search from './Search'
import type { GlobalSearchResult, Memory, SddChangeSummary } from '../types'

// ── Fixture data ──────────────────────────────────────────────────────────────

const memory: Memory = {
  id: 'm1',
  org_id: 'org-test-1',
  user_id: 'user-admin-1',
  project: 'acme-platform',
  tool: 'claude-code',
  content: 'The artifact store hashes content to dedupe revisions',
  tags: ['sdd'],
  created_at: '2026-07-01T00:00:00Z',
  title: 'Artifact revision hashing',
  type: 'discovery',
}

const sddChanges: SddChangeSummary[] = [
  { id: 'c1', project: 'acme-platform', name: 'sdd-artifacts', title: 'SDD artifacts', phase: 'design', status: 'active' },
  { id: 'c2', project: 'acme-platform', name: 'team-tasks', title: 'Team tasks', phase: 'verify', status: 'active' },
]

const EMPTY: GlobalSearchResult = {
  memories: [], users: [], projects: [], policies: [], conventions: [], sdd_changes: [],
}

const { globalSearchMock } = vi.hoisted(() => ({ globalSearchMock: vi.fn() }))

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({ globalSearch: globalSearchMock })),
}))

beforeEach(() => {
  vi.clearAllMocks()
  globalSearchMock.mockResolvedValue({ ...EMPTY, memories: [memory], sdd_changes: sddChanges })
})

function typeQuery(q = 'artifact') {
  fireEvent.change(screen.getByPlaceholderText(/search everything/i), { target: { value: q } })
}

describe('Search — SDD result group', () => {
  it('global_search_renders_an_sdd_changes_result_group', async () => {
    renderWithProviders(<Search />)
    typeQuery()

    const group = await screen.findByTestId('sdd-results', undefined, { timeout: 3000 })

    // Both changes, each with its name and phase.
    const first = within(group).getByText('sdd-artifacts').closest('a')!
    expect(within(first).getByText('design')).toBeInTheDocument()
    expect(first).toHaveAttribute('href', '/sdd?change=sdd-artifacts')

    const second = within(group).getByText('team-tasks').closest('a')!
    expect(within(second).getByText('verify')).toBeInTheDocument()
    expect(second).toHaveAttribute('href', '/sdd?change=team-tasks')
  })

  it('no_sdd_results_means_no_sdd_group', async () => {
    globalSearchMock.mockResolvedValue({ ...EMPTY, memories: [memory], sdd_changes: [] })
    renderWithProviders(<Search />)
    typeQuery()

    // The memory results render normally…
    await waitFor(() => {
      expect(screen.getByText(/artifact store hashes content/i)).toBeInTheDocument()
    }, { timeout: 3000 })

    // …and the SDD group is omitted entirely, not rendered empty.
    expect(screen.queryByTestId('sdd-results')).not.toBeInTheDocument()
  })

  it('search_ui_tolerates_a_response_without_the_sdd_changes_key', async () => {
    // An older backend that predates the additive facet: the key is simply absent.
    const legacy = { memories: [memory], users: [], projects: [], policies: [], conventions: [] }
    globalSearchMock.mockResolvedValue(legacy as unknown as GlobalSearchResult)

    renderWithProviders(<Search />)
    typeQuery()

    await waitFor(() => {
      expect(screen.getByText(/artifact store hashes content/i)).toBeInTheDocument()
    }, { timeout: 3000 })

    expect(screen.queryByTestId('sdd-results')).not.toBeInTheDocument()
  })
})
