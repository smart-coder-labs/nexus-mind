import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor, fireEvent } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import { CommandPalette } from './CommandPalette'
import type { GlobalSearchResult, Memory, SddChangeSummary } from '../types'

const memory: Memory = {
  id: 'm1',
  org_id: 'org-test-1',
  user_id: 'user-admin-1',
  project: 'acme-platform',
  tool: 'claude-code',
  content: 'The artifact store hashes content to dedupe revisions',
  tags: [],
  created_at: '2026-07-01T00:00:00Z',
  title: 'Artifact revision hashing',
  type: 'discovery',
}

const sddChanges: SddChangeSummary[] = [
  { id: 'c1', project: 'acme-platform', name: 'sdd-artifacts', title: 'SDD artifacts', phase: 'design', status: 'active' },
]

const EMPTY: GlobalSearchResult = {
  memories: [], users: [], projects: [], policies: [], conventions: [], sdd_changes: [], sdd_specs: [],
}

const { globalSearchMock, navigateMock } = vi.hoisted(() => ({
  globalSearchMock: vi.fn(),
  navigateMock: vi.fn(),
}))

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({ globalSearch: globalSearchMock })),
}))

vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return { ...actual, useNavigate: () => navigateMock }
})

beforeEach(() => {
  vi.clearAllMocks()
  globalSearchMock.mockResolvedValue({ ...EMPTY, memories: [memory], sdd_changes: sddChanges })
})

describe('CommandPalette — SDD results', () => {
  it('command_palette_includes_sdd_changes_in_flattened_results', async () => {
    renderWithProviders(<CommandPalette open onClose={() => undefined} />)

    fireEvent.change(screen.getByPlaceholderText(/search memories, users, projects/i), { target: { value: 'artifact' } })

    const option = await screen.findByRole('option', { name: /sdd-artifacts/i }, { timeout: 3000 })
    fireEvent.click(option)

    // Selecting an SDD result navigates to the change in the SDD section.
    await waitFor(() => {
      expect(navigateMock).toHaveBeenCalledWith('/sdd?change=sdd-artifacts')
    })
  })

  it('search_ui_tolerates_a_response_without_the_sdd_changes_key', async () => {
    const legacy = { memories: [memory], users: [], projects: [], policies: [], conventions: [] }
    globalSearchMock.mockResolvedValue(legacy as unknown as GlobalSearchResult)

    renderWithProviders(<CommandPalette open onClose={() => undefined} />)
    fireEvent.change(screen.getByPlaceholderText(/search memories, users, projects/i), { target: { value: 'artifact' } })

    // The memory group still renders; nothing crashes on the missing key.
    await waitFor(() => {
      expect(screen.getByText('Artifact revision hashing')).toBeInTheDocument()
    }, { timeout: 3000 })

    expect(screen.queryByRole('option', { name: /sdd-artifacts/i })).not.toBeInTheDocument()
  })
})
