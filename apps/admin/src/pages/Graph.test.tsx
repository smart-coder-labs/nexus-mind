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
const mockListCodeProjects = vi.fn()
const mockGetCodeGraph = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listProjects: mockListProjects,
    getMemoryGraphForFamily: mockGetMemoryGraphForFamily,
    getMemory: vi.fn(),
    listCodeProjects: mockListCodeProjects,
    getCodeGraph: mockGetCodeGraph,
    getCodeSnippet: vi.fn(),
  })),
}))

const codeRepos = [
  { id: 'c1', org_id: 'org-test-1', name: 'beta',  root_path: '/beta',  repo_url: null, file_count: 3, chunk_count: 9,  last_indexed: '2026-06-01T00:00:00Z', created_at: '2026-01-01T00:00:00Z' },
  { id: 'c2', org_id: 'org-test-1', name: 'other', root_path: '/other', repo_url: null, file_count: 1, chunk_count: 2,  last_indexed: '2026-06-01T00:00:00Z', created_at: '2026-01-01T00:00:00Z' },
]

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
    mockListCodeProjects.mockResolvedValue(codeRepos)
    mockGetCodeGraph.mockResolvedValue({
      project: 'beta', node_count: 0, edge_count: 0, nodes: [], edges: [],
    })
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

  // ── Code graph tab ──────────────────────────────────────────────────────
  //
  // `/graph` is NOT behind AdminRoute (unlike `/code`), so the code tab must be
  // gated on `code:read` by the page itself.

  it('hides the code graph tab from a user without code:read', async () => {
    renderWithProviders(<Graph />)
    await screen.findByLabelText('Select project', {}, { timeout: 5000 })
    expect(screen.queryByRole('button', { name: 'Code' })).not.toBeInTheDocument()
    expect(mockListCodeProjects).not.toHaveBeenCalled()
  })

  it('does not resurrect a persisted code tab for a user who lost code:read', async () => {
    localStorage.setItem('nexusmind-graph-page-tab', JSON.stringify({ __v: 1, value: 'code' }))
    renderWithProviders(<Graph />)

    // Falls back to the knowledge graph — its project selector, not the
    // repository selector, is what renders.
    expect(await screen.findByLabelText('Select project', {}, { timeout: 5000 })).toBeInTheDocument()
    expect(screen.queryByLabelText('Select repository')).not.toBeInTheDocument()
    expect(mockListCodeProjects).not.toHaveBeenCalled()
  })

  it('shows the code graph tab to a user with code:read', async () => {
    renderWithProviders(<Graph />, { permissions: ['code:read'] })
    expect(await screen.findByRole('button', { name: 'Code' }, { timeout: 5000 })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Knowledge' })).toBeInTheDocument()
  })

  it('preselects the repository whose name matches the active project', async () => {
    // Project p2 is named "beta", and so is one of the indexed repositories.
    localStorage.setItem('nexusmind-graph-page-project', JSON.stringify({ __v: 1, value: 'p2' }))
    localStorage.setItem('nexusmind-graph-page-tab', JSON.stringify({ __v: 1, value: 'code' }))
    renderWithProviders(<Graph />, { permissions: ['code:read'] })

    const select = (await screen.findByLabelText('Select repository', {}, { timeout: 5000 })) as HTMLSelectElement
    await waitFor(() => expect(select.value).toBe('beta'))
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
