import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor, fireEvent } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import type { CodeProject } from '../types'

// Mock WebGL-based library — jsdom has no real canvas/WebGL
vi.mock('react-force-graph-3d', () => ({
  default: vi.fn(() => <div data-testid="force-graph" />),
}))

const mockGetCodeGraph = vi.fn()
const mockGetCodeSnippet = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    getCodeGraph: mockGetCodeGraph,
    getCodeSnippet: mockGetCodeSnippet,
  })),
}))

import CodeGraph from './CodeGraph'

const repo: CodeProject = {
  id: 'p1',
  org_id: 'org1',
  name: 'my-repo',
  root_path: '/repo',
  repo_url: null,
  file_count: 10,
  chunk_count: 100,
  last_indexed: '2026-06-29T00:00:00Z',
  created_at: '2026-01-01T00:00:00Z',
}

const unindexedRepo: CodeProject = { ...repo, id: 'p2', name: 'never-indexed', last_indexed: null }

function renderGraph(props: Partial<React.ComponentProps<typeof CodeGraph>> = {}) {
  return renderWithProviders(
    <CodeGraph
      projects={[repo, unindexedRepo]}
      selectedRepo="my-repo"
      onSelectRepo={vi.fn()}
      storageKey="test"
      title="Graph"
      {...props}
    />,
  )
}

describe('CodeGraph', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
  })

  it('lists only indexed repositories in the selector', async () => {
    mockGetCodeGraph.mockResolvedValue({
      project: 'my-repo', node_count: 0, edge_count: 0, nodes: [], edges: [],
    })
    renderGraph()

    const select = (await screen.findByLabelText('Select repository')) as HTMLSelectElement
    const optionTexts = Array.from(select.options).map(o => o.text)
    expect(optionTexts).toContain('my-repo')
    expect(optionTexts).not.toContain('never-indexed')
  })

  it('renders the empty state when the repository has no indexed symbols', async () => {
    mockGetCodeGraph.mockResolvedValue({
      project: 'my-repo', node_count: 0, edge_count: 0, nodes: [], edges: [],
    })
    renderGraph()

    await waitFor(() => {
      expect(screen.getByText(/No graph data/i)).toBeInTheDocument()
    })
  })

  it('renders the graph and its type filter chips when the repo has symbols', async () => {
    mockGetCodeGraph.mockResolvedValue({
      project: 'my-repo',
      node_count: 2,
      edge_count: 1,
      nodes: [
        { id: 1, type: 'File', name: 'a.ts', qualified_name: 'a.ts', file_path: 'a.ts' },
        { id: 2, type: 'Function', name: 'doThing', qualified_name: 'a.ts::doThing', file_path: 'a.ts', start_line: 1, end_line: 3 },
      ],
      edges: [{ id: 1, from_id: 1, to_id: 2, type: 'defines' }],
    })
    renderGraph()

    expect(await screen.findByTestId('force-graph')).toBeInTheDocument()
    expect(screen.getByLabelText('Toggle Function nodes')).toBeInTheDocument()
    // External is hidden by default (LOD), so its chip is off.
    expect(screen.getByLabelText('Toggle External nodes')).toHaveAttribute('aria-pressed', 'false')
  })

  it('persists a node-type toggle so the filter survives a reload', async () => {
    mockGetCodeGraph.mockResolvedValue({
      project: 'my-repo',
      node_count: 1,
      edge_count: 0,
      nodes: [{ id: 1, type: 'File', name: 'a.ts', qualified_name: 'a.ts', file_path: 'a.ts' }],
      edges: [],
    })
    renderGraph()

    fireEvent.click(await screen.findByLabelText('Toggle File nodes'))

    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem('nexusmind-code-graph-types-test') ?? 'null')
      expect(stored).not.toBeNull()
      expect(stored).not.toContain('File')
    })
  })

  it('prompts for a repository when none is selected', async () => {
    renderGraph({ selectedRepo: '' })
    // Scoped to the empty-state copy — the select placeholder shares the wording.
    expect(await screen.findByText(/Choose a repository from the dropdown/i)).toBeInTheDocument()
    expect(mockGetCodeGraph).not.toHaveBeenCalled()
  })
})
