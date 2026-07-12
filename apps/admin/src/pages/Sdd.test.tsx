import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../auth/AuthContext'
import Sdd from './Sdd'
import type { AuthSession, SddChange } from '../types'

// ── Fixture data ──────────────────────────────────────────────────────────────

/** The load-bearing fixture: `phase` says `spec`, but a `design` and a `tasks`
 *  artifact both exist. The pipeline must believe the inventory, not the field. */
const staleChange: SddChange = {
  id: 'c1',
  org_id: 'org-test-1',
  project: 'acme-platform',
  name: 'sdd-artifacts',
  title: 'SDD artifacts in NexusMind',
  status: 'active',
  phase: 'spec',
  repo_url: null,
  repo_ref: null,
  sprint_id: null,
  created_by: 'user-admin-1',
  created_at: '2026-07-01T00:00:00Z',
  updated_at: '2026-07-05T00:00:00Z',
  archived_at: null,
  artifacts: [
    { id: 'a1', change_id: 'c1', kind: 'proposal', capability: '', path: null, latest_revision: 1, created_at: '2026-07-01T00:00:00Z', updated_at: '2026-07-01T00:00:00Z' },
    { id: 'a2', change_id: 'c1', kind: 'design',   capability: '', path: null, latest_revision: 2, created_at: '2026-07-02T00:00:00Z', updated_at: '2026-07-03T00:00:00Z' },
    { id: 'a3', change_id: 'c1', kind: 'tasks',    capability: '', path: null, latest_revision: 3, created_at: '2026-07-02T00:00:00Z', updated_at: '2026-07-05T00:00:00Z' },
  ],
  task_links: [],
  memory_links: [],
}

const secondChange: SddChange = {
  id: 'c2',
  org_id: 'org-test-1',
  project: 'nexusmind-admin',
  name: 'team-tasks',
  title: 'Team tasks',
  status: 'archived',
  phase: 'verify',
  repo_url: null,
  repo_ref: null,
  sprint_id: null,
  created_by: 'user-admin-1',
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-10T00:00:00Z',
  archived_at: null,
  artifacts: [
    { id: 'b1', change_id: 'c2', kind: 'proposal', capability: '', path: null, latest_revision: 1, created_at: '2026-06-01T00:00:00Z', updated_at: '2026-06-01T00:00:00Z' },
  ],
  task_links: [],
  memory_links: [],
}

const changes: SddChange[] = [staleChange, secondChange]

const projects = [
  { id: 'p1', org_id: 'org-test-1', name: 'acme-platform', description: null, parent_id: null, created_at: '2026-01-01T00:00:00Z' },
  { id: 'p2', org_id: 'org-test-1', name: 'nexusmind-admin', description: null, parent_id: null, created_at: '2026-01-01T00:00:00Z' },
]

// ── Mocks ─────────────────────────────────────────────────────────────────────

const {
  listSddChangesMock,
  getSddChangeMock,
  getSddChangeTasksMock,
  getSddArtifactMock,
  listSddArtifactRevisionsMock,
  getSddArtifactRevisionMock,
  patchSddChangeMock,
  linkSddChangeMemoryMock,
  unlinkSddChangeMemoryMock,
  listProjectsMock,
  listSprintsMock,
  listMemoriesMock,
} = vi.hoisted(() => ({
  listSddChangesMock: vi.fn(),
  getSddChangeMock: vi.fn(),
  getSddChangeTasksMock: vi.fn(),
  getSddArtifactMock: vi.fn(),
  listSddArtifactRevisionsMock: vi.fn(),
  getSddArtifactRevisionMock: vi.fn(),
  patchSddChangeMock: vi.fn(),
  linkSddChangeMemoryMock: vi.fn(),
  unlinkSddChangeMemoryMock: vi.fn(),
  listProjectsMock: vi.fn(),
  listSprintsMock: vi.fn(),
  listMemoriesMock: vi.fn(),
}))

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listSddChanges: listSddChangesMock,
    getSddChange: getSddChangeMock,
    getSddChangeTasks: getSddChangeTasksMock,
    getSddArtifact: getSddArtifactMock,
    listSddArtifactRevisions: listSddArtifactRevisionsMock,
    getSddArtifactRevision: getSddArtifactRevisionMock,
    patchSddChange: patchSddChangeMock,
    linkSddChangeMemory: linkSddChangeMemoryMock,
    unlinkSddChangeMemory: unlinkSddChangeMemoryMock,
    listProjects: listProjectsMock,
    listSprints: listSprintsMock,
    listMemories: listMemoriesMock,
  })),
}))

// ── Renders ───────────────────────────────────────────────────────────────────

function renderSdd(permissions: string[] | null, initialEntry = '/sdd'): ReturnType<typeof render> {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const session: AuthSession = {
    org: { id: 'org-test-1', name: 'Test Org', slug: 'test-org', created_at: '2026-01-01T00:00:00Z' },
    user: {
      id: permissions === null ? 'user-admin-1' : 'user-member-1',
      org_id: 'org-test-1',
      email: 'u@test.com',
      name: 'Test User',
      role: permissions === null ? 'admin' : 'member',
      status: 'active',
      created_at: '2026-01-01T00:00:00Z',
      ...(permissions === null ? {} : { permissions }),
    },
  }
  return render(
    <MemoryRouter initialEntries={[initialEntry]} future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider
          value={{ session, loading: false, setSession: () => undefined, logout: () => undefined }}
        >
          <Sdd />
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

/** Admin (privileged) render — the default caller. */
const renderAsAdmin = (entry?: string) => renderSdd(null, entry)
/** Member with exactly `permissions` — the permission-gating precedent from Tasks.test.tsx. */
const renderAsMember = (permissions: string[], entry?: string) => renderSdd(permissions, entry)

beforeEach(() => {
  vi.clearAllMocks()
  listSddChangesMock.mockResolvedValue(changes)
  getSddChangeMock.mockResolvedValue(staleChange)
  getSddChangeTasksMock.mockResolvedValue([])
  getSddArtifactMock.mockResolvedValue({ ...staleChange.artifacts[0], change_name: 'sdd-artifacts', project: 'acme-platform', content: '# Proposal', content_hash: 'h1' })
  listSddArtifactRevisionsMock.mockResolvedValue([])
  listProjectsMock.mockResolvedValue(projects)
  listSprintsMock.mockResolvedValue([])
  listMemoriesMock.mockResolvedValue([])
})

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('Sdd — change list', () => {
  it('sdd_list_renders_every_change_across_all_projects', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByText('sdd-artifacts')).toBeInTheDocument()
    })

    // name, title, project, status — for both changes, across both projects.
    expect(screen.getByText('SDD artifacts in NexusMind')).toBeInTheDocument()
    expect(screen.getByText('acme-platform')).toBeInTheDocument()
    expect(screen.getByText('team-tasks')).toBeInTheDocument()
    expect(screen.getByText('Team tasks')).toBeInTheDocument()
    expect(screen.getByText('nexusmind-admin')).toBeInTheDocument()

    const row = screen.getByText('sdd-artifacts').closest('tr')!
    expect(within(row).getByText('active')).toBeInTheDocument()
  })

  it('sdd_list_shows_skeleton_while_loading_then_the_table', async () => {
    let resolve!: (v: SddChange[]) => void
    listSddChangesMock.mockReturnValue(new Promise<SddChange[]>(r => { resolve = r }))

    const { container } = renderAsAdmin()

    // In flight: a skeleton, no table.
    expect(container.querySelector('[data-testid="sdd-skeleton"]')).not.toBeNull()
    expect(screen.queryByRole('table')).not.toBeInTheDocument()

    resolve(changes)

    await waitFor(() => {
      expect(screen.getByRole('table')).toBeInTheDocument()
    })
    expect(container.querySelector('[data-testid="sdd-skeleton"]')).toBeNull()
  })

  it('sdd_list_renders_empty_state_when_no_changes_match_filters', async () => {
    listSddChangesMock.mockResolvedValue([])
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByText('No changes found')).toBeInTheDocument()
    })
    expect(screen.queryByRole('table')).not.toBeInTheDocument()
  })
})

describe('Sdd — filter bar', () => {
  it('sdd_list_filter_bar_by_project_phase_and_status_refetches', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByText('sdd-artifacts')).toBeInTheDocument()
    })

    // Project
    listSddChangesMock.mockClear()
    fireEvent.click(screen.getByRole('button', { name: /^project$/i }))
    fireEvent.click(await screen.findByRole('option', { name: /^acme-platform$/i }))
    await waitFor(() => {
      expect(listSddChangesMock).toHaveBeenCalledWith(
        expect.objectContaining({ project: 'acme-platform' }),
      )
    })

    // Phase
    listSddChangesMock.mockClear()
    listSddChangesMock.mockResolvedValue([staleChange])
    fireEvent.click(screen.getByRole('button', { name: /^phase$/i }))
    fireEvent.click(await screen.findByRole('option', { name: /^design$/i }))
    await waitFor(() => {
      expect(listSddChangesMock).toHaveBeenCalledWith(
        expect.objectContaining({ phase: 'design' }),
      )
    })
    await waitFor(() => {
      expect(screen.queryByText('team-tasks')).not.toBeInTheDocument()
    })

    // Status
    listSddChangesMock.mockClear()
    fireEvent.click(screen.getByRole('button', { name: /^status$/i }))
    fireEvent.click(await screen.findByRole('option', { name: /^active$/i }))
    await waitFor(() => {
      expect(listSddChangesMock).toHaveBeenCalledWith(
        expect.objectContaining({ status: 'active' }),
      )
    })
  })
})

describe('Sdd — phase pipeline', () => {
  it('sdd_list_renders_a_phase_pipeline_driven_by_which_artifacts_exist', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByText('sdd-artifacts')).toBeInTheDocument()
    })

    const row = screen.getByText('sdd-artifacts').closest('tr')!
    const pipeline = within(row).getByTestId('phase-pipeline')

    // The change's advisory `phase` is `spec`, but a design AND a tasks artifact
    // exist — the inventory is the ground truth, so both are present.
    expect(within(pipeline).getByTestId('phase-step-design')).toHaveAttribute('data-present', 'true')
    expect(within(pipeline).getByTestId('phase-step-tasks')).toHaveAttribute('data-present', 'true')
    expect(within(pipeline).getByTestId('phase-step-propose')).toHaveAttribute('data-present', 'true')

    // …and the steps with no artifact are not claimed.
    expect(within(pipeline).getByTestId('phase-step-spec')).toHaveAttribute('data-present', 'false')
    expect(within(pipeline).getByTestId('phase-step-apply')).toHaveAttribute('data-present', 'false')
    expect(within(pipeline).getByTestId('phase-step-verify')).toHaveAttribute('data-present', 'false')

    // All six steps of the pipeline are rendered, not just the ones reached.
    expect(within(pipeline).getAllByTestId(/^phase-step-/)).toHaveLength(6)
  })
})

describe('Sdd — permission guard on direct navigation', () => {
  it('sdd_page_redirects_to_401_without_sdd_read', async () => {
    renderAsMember([])

    await waitFor(() => {
      expect(listSddChangesMock).not.toHaveBeenCalled()
    })

    expect(screen.queryByText('sdd-artifacts')).not.toBeInTheDocument()
    expect(screen.queryByRole('table')).not.toBeInTheDocument()
  })

  it('renders the list for a member holding sdd:read', async () => {
    renderAsMember(['sdd:read'])

    await waitFor(() => {
      expect(screen.getByText('sdd-artifacts')).toBeInTheDocument()
    })
    expect(listSddChangesMock).toHaveBeenCalled()
  })
})

describe('Sdd — deep link', () => {
  it('sdd_list_deep_links_a_change_by_query_param', async () => {
    renderAsAdmin('/sdd?change=sdd-artifacts')

    await waitFor(() => {
      expect(screen.getByText('sdd-artifacts')).toBeInTheDocument()
    })

    // `?change=<name>` selects that row — the target PR-9's cross-links and
    // search results point at.
    const selected = screen.getByText('sdd-artifacts').closest('tr')!
    expect(selected).toHaveAttribute('aria-selected', 'true')

    const other = screen.getByText('team-tasks').closest('tr')!
    expect(other).toHaveAttribute('aria-selected', 'false')
  })

  it('does not select anything when the ?change= name matches no row', async () => {
    renderAsAdmin('/sdd?change=no-such-change')

    await waitFor(() => {
      expect(screen.getByText('sdd-artifacts')).toBeInTheDocument()
    })

    expect(screen.getByText('sdd-artifacts').closest('tr')!).toHaveAttribute('aria-selected', 'false')
    expect(getSddChangeMock).not.toHaveBeenCalled()
  })
})

// ── Drawer wiring (PR-9) ──────────────────────────────────────────────────────

describe('Sdd — change detail drawer', () => {
  it('opens the ChangeDetail drawer when a row is clicked', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByText('sdd-artifacts')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByText('sdd-artifacts'))

    await waitFor(() => {
      expect(getSddChangeMock).toHaveBeenCalledWith('c1')
    })
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
  })

  it('opens the drawer for a change arrived at via ?change=<name>', async () => {
    renderAsAdmin('/sdd?change=sdd-artifacts')

    await waitFor(() => {
      expect(getSddChangeMock).toHaveBeenCalledWith('c1')
    })
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
  })
})
