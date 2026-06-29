import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor, fireEvent } from '@testing-library/react'
import { renderWithProviders } from '../../test/render'
import type { CodeProject } from '../../types'

// Mock canvas-based library — jsdom has no real canvas
vi.mock('react-force-graph-2d', () => ({
  default: vi.fn(() => <div data-testid="force-graph" />),
}))

// Mock the API client
const mockGetCodeGraph = vi.fn()

vi.mock('../../api/client', () => ({
  createClient: vi.fn(() => ({
    getCodeGraph: mockGetCodeGraph,
  })),
}))

// Import pure functions after mocks are established
import {
  mapGraphData,
  filterNodesByTypes,
  computeExternalAggregate,
  DEFAULT_VISIBLE_TYPES,
  EXTERNAL_COLLAPSE_THRESHOLD,
} from './graphUtils'
import GraphTab from './GraphTab'
import type { CodeGraph, GraphNode, GraphEdge } from '../../types'

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeNode(id: number, type: string, name = `node-${id}`): GraphNode {
  return { id, type, name, qualified_name: `qn::${id}`, file_path: null }
}

function makeEdge(id: number, from_id: number, to_id: number, type = 'defines'): GraphEdge {
  return { id, from_id, to_id, type }
}

function makeGraph(nodes: GraphNode[], edges: GraphEdge[]): CodeGraph {
  return { project: 'test', node_count: nodes.length, edge_count: edges.length, nodes, edges }
}

const mockProject: CodeProject = {
  id: 'p1',
  org_id: 'org1',
  name: 'my-project',
  root_path: '/repo',
  repo_url: null,
  file_count: 10,
  chunk_count: 100,
  last_indexed: '2026-06-29T00:00:00Z',
  created_at: '2026-01-01T00:00:00Z',
}

// ── Pure-function tests ───────────────────────────────────────────────────────

describe('mapGraphData — from_id/to_id → source/target', () => {
  it('maps from_id to source and to_id to target', () => {
    const graph = makeGraph(
      [makeNode(1, 'File'), makeNode(2, 'Function')],
      [makeEdge(10, 1, 2, 'defines')],
    )
    const { links } = mapGraphData(graph)
    expect(links).toHaveLength(1)
    expect(links[0].source).toBe(1)
    expect(links[0].target).toBe(2)
    expect(links[0].type).toBe('defines')
  })

  it('preserves node id, type, and name', () => {
    const graph = makeGraph([makeNode(5, 'Function', 'myFn')], [])
    const { nodes } = mapGraphData(graph)
    expect(nodes[0].id).toBe(5)
    expect(nodes[0].type).toBe('Function')
    expect(nodes[0].name).toBe('myFn')
  })

  it('returns empty arrays for empty graph', () => {
    const graph = makeGraph([], [])
    const { nodes, links } = mapGraphData(graph)
    expect(nodes).toHaveLength(0)
    expect(links).toHaveLength(0)
  })
})

describe('default LOD filter — excludes Folder and External', () => {
  it('excludes Folder nodes by default', () => {
    const nodes = [
      { id: 1, type: 'File', name: 'index.ts', fp: null },
      { id: 2, type: 'Folder', name: 'src', fp: null },
      { id: 3, type: 'Function', name: 'fn', fp: null },
    ]
    const visible = filterNodesByTypes(nodes, DEFAULT_VISIBLE_TYPES)
    expect(visible.map(n => n.type)).not.toContain('Folder')
  })

  it('excludes External nodes by default', () => {
    const nodes = [
      { id: 1, type: 'File', name: 'index.ts', fp: null },
      { id: 2, type: 'External', name: 'react', fp: null },
    ]
    const visible = filterNodesByTypes(nodes, DEFAULT_VISIBLE_TYPES)
    expect(visible.map(n => n.type)).not.toContain('External')
  })

  it('includes File, Function, Class, Struct, Interface, Type, Enum, Method, Module, Project', () => {
    const expectedVisible = ['File', 'Function', 'Class', 'Struct', 'Interface', 'Type', 'Enum', 'Method', 'Module', 'Project']
    for (const t of expectedVisible) {
      expect(DEFAULT_VISIBLE_TYPES.has(t)).toBe(true)
    }
  })

  it('does NOT include Folder or External in the default set', () => {
    expect(DEFAULT_VISIBLE_TYPES.has('Folder')).toBe(false)
    expect(DEFAULT_VISIBLE_TYPES.has('External')).toBe(false)
  })
})

describe('computeExternalAggregate — external-node volume control', () => {
  it('produces exactly 1 aggregate node when external count exceeds threshold', () => {
    const externalNodes = Array.from({ length: EXTERNAL_COLLAPSE_THRESHOLD + 1 }, (_, i) => ({
      id: i + 100,
      type: 'External',
      name: `ext-${i}`,
      fp: null,
    }))
    const fileNode = { id: 1, type: 'File', name: 'main.ts', fp: null }
    const links = externalNodes.map(n => ({ source: 1, target: n.id, type: 'imports' }))

    const { nodes } = computeExternalAggregate([fileNode, ...externalNodes], links, false)
    const externals = nodes.filter(n => n.type === 'External')
    expect(externals).toHaveLength(1)
    expect(externals[0].name).toMatch(/External dependencies \(\d+\)/)
  })

  it('does NOT collapse when external count is exactly at threshold', () => {
    const externalNodes = Array.from({ length: EXTERNAL_COLLAPSE_THRESHOLD }, (_, i) => ({
      id: i + 100,
      type: 'External',
      name: `ext-${i}`,
      fp: null,
    }))
    const links = externalNodes.map(n => ({ source: 1, target: n.id, type: 'imports' }))
    const { nodes } = computeExternalAggregate(externalNodes, links, false)
    const externals = nodes.filter(n => n.type === 'External')
    expect(externals).toHaveLength(EXTERNAL_COLLAPSE_THRESHOLD)
  })

  it('does NOT collapse when expandExternals=true, even above threshold', () => {
    const externalNodes = Array.from({ length: EXTERNAL_COLLAPSE_THRESHOLD + 10 }, (_, i) => ({
      id: i + 100,
      type: 'External',
      name: `ext-${i}`,
      fp: null,
    }))
    const { nodes } = computeExternalAggregate(externalNodes, [], true)
    expect(nodes.filter(n => n.type === 'External')).toHaveLength(EXTERNAL_COLLAPSE_THRESHOLD + 10)
  })

  it('remaps import links to the aggregate node', () => {
    const externalNodes = Array.from({ length: EXTERNAL_COLLAPSE_THRESHOLD + 1 }, (_, i) => ({
      id: i + 100,
      type: 'External',
      name: `ext-${i}`,
      fp: null,
    }))
    const fileNode = { id: 1, type: 'File', name: 'a.ts', fp: null }
    const links = externalNodes.map(n => ({ source: 1, target: n.id, type: 'imports' }))

    const { links: remapped } = computeExternalAggregate([fileNode, ...externalNodes], links, false)
    // All links must point to the aggregate (id = -1)
    expect(remapped.every(l => l.target === -1)).toBe(true)
  })
})

// ── Rendering test ────────────────────────────────────────────────────────────

describe('GraphTab rendering', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders "No graph data" when API returns node_count=0, without JS error', async () => {
    mockGetCodeGraph.mockResolvedValue({
      project: 'my-project',
      node_count: 0,
      edge_count: 0,
      nodes: [],
      edges: [],
    })

    renderWithProviders(<GraphTab projects={[mockProject]} />)

    // Select the project to trigger the query
    const select = screen.getByRole('combobox')
    fireEvent.change(select, { target: { value: 'my-project' } })

    await waitFor(() => {
      expect(screen.getByText(/No graph data/i)).toBeInTheDocument()
    })
  })
})
