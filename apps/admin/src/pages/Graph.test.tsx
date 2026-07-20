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

  it('auto-selects a project and renders the graph instead of a blank prompt', async () => {
    // The redesigned page always resolves to a selected project (there is no
    // deselect affordance), so with no persisted selection it auto-selects the
    // first project and renders the graph rather than a "pick a project" state.
    renderWithProviders(<Graph />)
    // First test in the file pays the lazy-chunk cold-load cost (OrgMemoryGraph
    // is React.lazy) — under full-suite load that can exceed findBy's 1s
    // default, so give only this initial lookup a longer timeout.
    const select = (await screen.findByLabelText('Select project', {}, { timeout: 5000 })) as HTMLSelectElement
    await waitFor(() => expect(select.value).toBe('p1'))
    expect(await screen.findByTestId('force-graph')).toBeInTheDocument()
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

  it('renders the family legend chips for the resolved family after a project is selected', async () => {
    localStorage.setItem('nexusmind-graph-page-project', JSON.stringify({ __v: 1, value: 'p1' }))
    renderWithProviders(<Graph />)

    // The family legend exposes one clickable chip per project in the resolved
    // BFS family ({alpha, beta}), via role="list" aria-label "Project family legend".
    const list = await screen.findByRole('list', { name: 'Project family legend' })
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
