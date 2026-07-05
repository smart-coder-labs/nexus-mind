import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Graph from './Graph'

// ForceGraph3D needs WebGL — replace with a stub in jsdom
vi.mock('react-force-graph-3d', () => ({
  default: () => <div data-testid="force-graph" />,
}))

const mockListProjects = vi.fn()
const mockGetMemoryGraphForFamily = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listProjects: mockListProjects,
    getMemoryGraphForFamily: mockGetMemoryGraphForFamily,
    getMemory: vi.fn(),
  })),
}))

const projects = [
  { id: 'p1', org_id: 'org-test-1', name: 'alpha', description: null, parent_id: null,   created_at: '2026-01-01T00:00:00Z', archived_at: null },
  { id: 'p2', org_id: 'org-test-1', name: 'beta',  description: null, parent_id: 'p1',  created_at: '2026-01-01T00:00:00Z', archived_at: null },
  { id: 'p3', org_id: 'org-test-1', name: 'gamma', description: null, parent_id: null,   created_at: '2026-01-01T00:00:00Z', archived_at: null },
  { id: 'p4', org_id: 'org-test-1', name: 'old',   description: null, parent_id: null,   created_at: '2026-01-01T00:00:00Z', archived_at: '2026-06-01T00:00:00Z' },
]

function familyResponse(rootId: string) {
  return {
    project: 'alpha',
    node_count: 1,
    edge_count: 0,
    nodes: [{ id: `memory:${rootId}-m1`, type: 'Memory', label: 'a memory' }],
    edges: [],
    projects: [
      { id: 'p1', name: 'alpha', color: '#2997ff', parent_id: null },
      { id: 'p2', name: 'beta',  color: '#34d399', parent_id: 'p1' },
    ],
  }
}

describe('Graph page', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    mockListProjects.mockResolvedValue(projects)
    mockGetMemoryGraphForFamily.mockImplementation((id: string) => Promise.resolve(familyResponse(id)))
  })

  it('shows the empty state when no project is selected', async () => {
    // Persisted selection starts empty → empty state is shown.
    renderWithProviders(<Graph />)
    expect(await screen.findByText(/Select a project/i)).toBeInTheDocument()
  })

  it('auto-selects the first non-archived project on first load', async () => {
    renderWithProviders(<Graph />)
    const select = (await screen.findByLabelText('Select project')) as HTMLSelectElement
    await waitFor(() => {
      expect(select.value).toBe('p1')
    })
  })

  it('excludes archived projects from the dropdown', async () => {
    renderWithProviders(<Graph />)
    const select = (await screen.findByLabelText('Select project')) as HTMLSelectElement
    await waitFor(() => expect(select.value).toBe('p1'))
    const optionTexts = Array.from(select.options).map(o => o.text)
    expect(optionTexts.some(t => t.includes('old'))).toBe(false)
    expect(optionTexts.some(t => t.includes('alpha'))).toBe(true)
    expect(optionTexts.some(t => t.includes('beta'))).toBe(true)
  })

  it('renders the legend swatches for the resolved family after a project is selected', async () => {
    localStorage.setItem('nexusmind-graph-page-project', JSON.stringify({ __v: 1, value: 'p1' }))
    renderWithProviders(<Graph />)

    // The legend's "Project family: alpha + 1 descendant" label confirms the
    // BFS walk resolved to {alpha, beta}.
    expect(await screen.findByText(/Project family: alpha \+ 1 descendant/i)).toBeInTheDocument()
    // The list of swatches is exposed via role="list" with aria-label "Project legend".
    const list = await screen.findByRole('list', { name: 'Project legend' })
    expect(list.textContent).toContain('alpha')
    expect(list.textContent).toContain('beta')
  })

  it('persists the selected project to localStorage so reloads restore it', async () => {
    renderWithProviders(<Graph />)
    const select = (await screen.findByLabelText('Select project')) as HTMLSelectElement
    await waitFor(() => expect(select.value).toBe('p1'))

    fireEvent.change(select, { target: { value: 'p3' } })
    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem('nexusmind-graph-page-project') ?? 'null')
      // Hook writes either raw or { __v, value } depending on version config.
      const value = stored && typeof stored === 'object' && '__v' in stored ? stored.value : stored
      expect(value).toBe('p3')
    })
  })

  it('renders the empty state when the selected project has no data', async () => {
    mockGetMemoryGraphForFamily.mockResolvedValueOnce({
      project: 'gamma',
      node_count: 0,
      edge_count: 0,
      nodes: [],
      edges: [],
      projects: [{ id: 'p3', name: 'gamma', color: '#fb923c', parent_id: null }],
    })
    localStorage.setItem('nexusmind-graph-page-project', JSON.stringify({ __v: 1, value: 'p3' }))
    renderWithProviders(<Graph />)
    expect(await screen.findByText(/No data for this project/i)).toBeInTheDocument()
  })
})
