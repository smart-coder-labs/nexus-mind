import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import OrgMemoryGraph from './OrgMemoryGraph'

// ForceGraph3D needs WebGL — replace with a stub in jsdom
vi.mock('react-force-graph-3d', () => ({
  default: () => <div data-testid="force-graph" />,
}))

const mockGetMemoryGraphForFamily = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    getMemoryGraphForFamily: mockGetMemoryGraphForFamily,
    getMemory: vi.fn(),
    listProjects: vi.fn(),
  })),
}))

// Project family passed in by the new Graph page (after the BFS walk).
// Colors are backend-provided — the legend and node-color resolver consume them.
const family = [
  { id: 'p1', name: 'alpha', color: '#2997ff', parent_id: null },
  { id: 'p2', name: 'beta',  color: '#34d399', parent_id: 'p1' },
]

const familyResponse = {
  project: 'alpha',
  node_count: 2,
  edge_count: 1,
  nodes: [
    { id: 'memory:m1', type: 'Memory', label: 'alpha memory' },
    { id: 'memory:m2', type: 'Memory', label: 'beta memory' },
    { id: 'project:p1', type: 'Project', label: 'alpha' },
    { id: 'project:p2', type: 'Project', label: 'beta' },
  ],
  edges: [
    { id: 'b1', from_id: 'memory:m1', to_id: 'project:p1', type: 'belongs_to' },
    { id: 'b2', from_id: 'memory:m2', to_id: 'project:p2', type: 'belongs_to' },
    { id: 'h1', from_id: 'project:p2', to_id: 'project:p1', type: 'child_of' },
  ],
  projects: family,
}

describe('OrgMemoryGraph — family-scoped contract', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    mockGetMemoryGraphForFamily.mockResolvedValue(familyResponse)
  })

  it('renders one legend swatch per project in the family, using backend colors', async () => {
    renderWithProviders(
      <OrgMemoryGraph family={family} familyId="p1" storageKey="test" height={400} />,
    )
    const items = await screen.findAllByRole('listitem')
    const legend = items.filter(el => el.textContent === 'alpha' || el.textContent === 'beta')
    expect(legend.length).toBe(2)
  })

  it('calls the new family-scoped endpoint exactly once per render', async () => {
    renderWithProviders(
      <OrgMemoryGraph family={family} familyId="p1" storageKey="test" />,
    )
    await screen.findAllByRole('listitem')
    await waitFor(() => {
      expect(mockGetMemoryGraphForFamily).toHaveBeenCalledTimes(1)
    })
    expect(mockGetMemoryGraphForFamily).toHaveBeenCalledWith('p1')
  })

  it('restores hidden node types from localStorage on mount (reload survival)', async () => {
    // The old test was about project toggles; in the new contract the family
    // is always fully visible — what survives the reload is the node-type
    // visibility filter.
    localStorage.setItem('nexusmind-org-graph-types-test', JSON.stringify(['Memory', 'Project']))

    renderWithProviders(
      <OrgMemoryGraph family={family} familyId="p1" storageKey="test" />,
    )

    const tagPill = await screen.findByRole('button', { name: 'Toggle Tag nodes' })
    const memoryPill = screen.getByRole('button', { name: 'Toggle Memory nodes' })

    expect(tagPill).toHaveAttribute('aria-pressed', 'false')
    expect(memoryPill).toHaveAttribute('aria-pressed', 'true')
  })

  it('persists a node-type toggle to localStorage', async () => {
    renderWithProviders(
      <OrgMemoryGraph family={family} familyId="p1" storageKey="test" />,
    )
    const tagPill = await screen.findByRole('button', { name: 'Toggle Tag nodes' })
    fireEvent.click(tagPill)

    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem('nexusmind-org-graph-types-test') ?? '[]')
      expect(stored).not.toContain('Tag')
    })
  })

  it('shows the empty state when the family response has no nodes', async () => {
    mockGetMemoryGraphForFamily.mockResolvedValueOnce({
      project: 'alpha',
      node_count: 0,
      edge_count: 0,
      nodes: [],
      edges: [],
      projects: family,
    })
    renderWithProviders(
      <OrgMemoryGraph family={family} familyId="p1" storageKey="test"
        emptyTitle="No data"
        emptyDescription="Try another project."
      />,
    )
    expect(await screen.findByText('No data')).toBeInTheDocument()
    expect(screen.getByText('Try another project.')).toBeInTheDocument()
  })
})
