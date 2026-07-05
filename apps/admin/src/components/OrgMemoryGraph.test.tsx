import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import OrgMemoryGraph from './OrgMemoryGraph'

// ForceGraph3D needs WebGL — replace with a stub in jsdom
vi.mock('react-force-graph-3d', () => ({
  default: () => <div data-testid="force-graph" />,
}))

const mockListProjects = vi.fn()
const mockGetMemoryGraph = vi.fn()
const mockGetMemory = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listProjects: mockListProjects,
    getMemoryGraph: mockGetMemoryGraph,
    getMemory: mockGetMemory,
  })),
}))

const projects = [
  { id: 'p1', org_id: 'org-test-1', name: 'alpha', description: null, parent_id: null, created_at: '2026-01-01T00:00:00Z', archived_at: null },
  { id: 'p2', org_id: 'org-test-1', name: 'beta', description: null, parent_id: null, created_at: '2026-01-01T00:00:00Z', archived_at: null },
]

function graphResponse(project: string, nodeCount = 1) {
  return {
    project,
    node_count: nodeCount,
    edge_count: 0,
    nodes: [{ id: `memory:${project}-m1`, type: 'Memory', label: `${project} memory` }],
    edges: [],
  }
}

describe('OrgMemoryGraph filter persistence', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    mockListProjects.mockResolvedValue(projects)
    mockGetMemoryGraph.mockImplementation((name: string) => Promise.resolve(graphResponse(name)))
  })

  it('restores hidden projects from localStorage on mount (reload survival)', async () => {
    localStorage.setItem('nexusmind-org-graph-projects-test', JSON.stringify(['beta']))

    renderWithProviders(<OrgMemoryGraph storageKey="test" />)

    const betaPill = await screen.findByRole('button', { name: 'Toggle beta project' })
    const alphaPill = screen.getByRole('button', { name: 'Toggle alpha project' })

    await waitFor(() => {
      expect(betaPill).toHaveAttribute('aria-pressed', 'false')
      expect(alphaPill).toHaveAttribute('aria-pressed', 'true')
    })
  })

  it('restores hidden types from localStorage on mount', async () => {
    localStorage.setItem('nexusmind-org-graph-types-test', JSON.stringify(['Memory', 'Project']))

    renderWithProviders(<OrgMemoryGraph storageKey="test" />)

    const tagPill = await screen.findByRole('button', { name: 'Toggle Tag nodes' })
    const memoryPill = screen.getByRole('button', { name: 'Toggle Memory nodes' })

    expect(tagPill).toHaveAttribute('aria-pressed', 'false')
    expect(memoryPill).toHaveAttribute('aria-pressed', 'true')
  })

  it('persists a project toggle to localStorage', async () => {
    renderWithProviders(<OrgMemoryGraph storageKey="test" />)

    const betaPill = await screen.findByRole('button', { name: 'Toggle beta project' })
    fireEvent.click(betaPill)

    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem('nexusmind-org-graph-projects-test') ?? '[]')
      expect(stored).toEqual(['beta'])
    })
  })

  it('prunes projects that no longer exist from stored hidden set', async () => {
    localStorage.setItem(
      'nexusmind-org-graph-projects-test',
      JSON.stringify(['beta', 'deleted-project']),
    )

    renderWithProviders(<OrgMemoryGraph storageKey="test" />)

    await screen.findByRole('button', { name: 'Toggle beta project' })

    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem('nexusmind-org-graph-projects-test') ?? '[]')
      expect(stored).toEqual(['beta'])
    })
  })
})
