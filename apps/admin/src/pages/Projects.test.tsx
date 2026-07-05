import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Projects from './Projects'
import type { Project } from '../types'

// ── Fixture data ──────────────────────────────────────────────────────────────

const projects: Project[] = [
  {
    id: 'p1',
    org_id: 'org-test-1',
    name: 'acme-platform',
    description: 'Top-level platform project',
    parent_id: null,
    created_at: '2026-01-01T00:00:00Z',
  },
  {
    id: 'p2',
    org_id: 'org-test-1',
    name: 'acme-payments',
    description: 'Payments service under the platform',
    parent_id: 'p1',
    created_at: '2026-01-02T00:00:00Z',
  },
  {
    id: 'p3',
    org_id: 'org-test-1',
    name: 'acme-billing',
    description: 'Billing service under payments',
    parent_id: 'p2',
    created_at: '2026-01-03T00:00:00Z',
  },
  {
    id: 'p4',
    org_id: 'org-test-1',
    name: 'standalone',
    description: 'Leaf project without any children',
    parent_id: null,
    created_at: '2026-01-04T00:00:00Z',
  },
]

// ── Mocks ─────────────────────────────────────────────────────────────────────

const listProjectsMock = vi.fn().mockResolvedValue(projects)

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listProjects: listProjectsMock,
    listUsers: vi.fn().mockResolvedValue([]),
    listRoles: vi.fn().mockResolvedValue([]),
    listConventions: vi.fn().mockResolvedValue([]),
    listProjectMembers: vi.fn().mockResolvedValue([]),
    deleteProjectMember: vi.fn().mockResolvedValue(undefined),
    getProjectSettings: vi.fn().mockResolvedValue({}),
    updateProjectSettings: vi.fn().mockResolvedValue({}),
    getProjectStats: vi.fn().mockResolvedValue({
      total_memories: 0,
      memories_this_week: 0,
      last_memory_at: null,
      top_tags: [],
    }),
    createProject: vi.fn().mockResolvedValue({}),
    updateProject: vi.fn().mockResolvedValue(undefined),
    archiveProject: vi.fn().mockResolvedValue(undefined),
    restoreProject: vi.fn().mockResolvedValue(undefined),
    deleteProject: vi.fn().mockResolvedValue(undefined),
    listMemories: vi.fn().mockResolvedValue([]),
  })),
}))

beforeEach(() => {
  vi.clearAllMocks()
  listProjectsMock.mockResolvedValue(projects)
  // Avoid confirm() dialogs from archive interactions blocking tests
  vi.spyOn(window, 'confirm').mockReturnValue(true)
})

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('Projects — child-project tree expand/collapse', () => {
  it('renders root projects with a chevron only when they have children', async () => {
    renderWithProviders(<Projects />)

    await waitFor(() => {
      expect(screen.getByText('acme-platform')).toBeInTheDocument()
    })

    // acme-platform has children → chevron button present
    expect(
      screen.getByRole('button', { name: /expand child projects of acme-platform/i }),
    ).toBeInTheDocument()

    // standalone is a leaf → no chevron button
    expect(
      screen.queryByRole('button', { name: /expand child projects of standalone/i }),
    ).not.toBeInTheDocument()
  })

  it('children are hidden by default', async () => {
    renderWithProviders(<Projects />)

    await waitFor(() => {
      expect(screen.getByText('acme-platform')).toBeInTheDocument()
    })

    // Direct child not visible until expanded
    expect(screen.queryByText('acme-payments')).not.toBeInTheDocument()
  })

  it('clicking the chevron expands the children inline', async () => {
    renderWithProviders(<Projects />)

    await waitFor(() => {
      expect(screen.getByText('acme-platform')).toBeInTheDocument()
    })

    fireEvent.click(
      screen.getByRole('button', { name: /expand child projects of acme-platform/i }),
    )

    await waitFor(() => {
      expect(screen.getByText('acme-payments')).toBeInTheDocument()
    })
  })

  it('expands all the way down — grandchildren appear when parent is expanded', async () => {
    renderWithProviders(<Projects />)

    await waitFor(() => {
      expect(screen.getByText('acme-platform')).toBeInTheDocument()
    })

    fireEvent.click(
      screen.getByRole('button', { name: /expand child projects of acme-platform/i }),
    )

    // Grandchild only appears after expanding the intermediate child too
    await waitFor(() => {
      expect(screen.getByText('acme-payments')).toBeInTheDocument()
    })
    expect(screen.queryByText('acme-billing')).not.toBeInTheDocument()

    fireEvent.click(
      screen.getByRole('button', { name: /expand child projects of acme-payments/i }),
    )

    await waitFor(() => {
      expect(screen.getByText('acme-billing')).toBeInTheDocument()
    })
  })

  it('clicking the chevron again collapses the children', async () => {
    renderWithProviders(<Projects />)

    await waitFor(() => {
      expect(screen.getByText('acme-platform')).toBeInTheDocument()
    })

    const toggle = screen.getByRole('button', {
      name: /expand child projects of acme-platform/i,
    })

    fireEvent.click(toggle)
    await waitFor(() => {
      expect(screen.getByText('acme-payments')).toBeInTheDocument()
    })

    fireEvent.click(
      screen.getByRole('button', { name: /collapse child projects of acme-platform/i }),
    )

    await waitFor(() => {
      expect(screen.queryByText('acme-payments')).not.toBeInTheDocument()
    })
  })

  it('members expand/collapse still works independently of the tree expand', async () => {
    renderWithProviders(<Projects />)

    await waitFor(() => {
      expect(screen.getByText('acme-platform')).toBeInTheDocument()
    })

    // Open the members panel for acme-platform — button aria-label flips
    const membersToggle = screen.getByRole('button', {
      name: /expand members for acme-platform/i,
    })
    fireEvent.click(membersToggle)

    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: /collapse members for acme-platform/i }),
      ).toBeInTheDocument()
    })

    // Tree expand still works while the members panel is open
    fireEvent.click(
      screen.getByRole('button', { name: /expand child projects of acme-platform/i }),
    )
    await waitFor(() => {
      expect(screen.getByText('acme-payments')).toBeInTheDocument()
    })

    // Tree expand must not have closed the members panel
    expect(
      screen.getByRole('button', { name: /collapse members for acme-platform/i }),
    ).toBeInTheDocument()
  })
})
