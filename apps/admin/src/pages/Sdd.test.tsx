import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../auth/AuthContext'
import Sdd from './Sdd'
import type {
  AuthSession, SddChange, SddSpec, SddSpecDetail, SddSpecMerge, SddSpecRevisionMeta,
} from '../types'

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

// ── The OTHER tree: openspec/specs/{capability}/spec.md ───────────────────────
//
// The living specifications. Not artifacts of a change — their own entity, with
// their own revision history and the change that last merged into each of them.

const harnessLibrary: SddSpec = {
  id: 's1',
  org_id: 'org-test-1',
  project: 'acme-platform',
  capability: 'harness-library',
  title: 'Harness Library',
  path: 'openspec/specs/harness-library/spec.md',
  latest_revision: 3,
  created_by: 'user-admin-1',
  created_at: '2026-05-01T00:00:00Z',
  updated_at: '2026-07-05T00:00:00Z',
  archived_at: null,
  last_merged_from_change_id: 'c1',
  last_merged_from_change_name: 'sdd-artifacts',
}

/** No provenance: imported from disk, where "which change last merged" is not recorded. */
const policyEngine: SddSpec = {
  id: 's2',
  org_id: 'org-test-1',
  project: 'acme-platform',
  capability: 'policy-engine',
  title: null,
  path: 'openspec/specs/policy-engine/spec.md',
  latest_revision: 1,
  created_by: 'user-admin-1',
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-01T00:00:00Z',
  archived_at: null,
  last_merged_from_change_id: null,
  last_merged_from_change_name: null,
}

const specs: SddSpec[] = [harnessLibrary, policyEngine]

const specRevisions: SddSpecRevisionMeta[] = [
  { id: 'sr3', spec_id: 's1', revision: 3, content_hash: 'h3', byte_size: 300, merged_from_change_id: 'c1', merged_from_change_name: 'sdd-artifacts', git_commit: null, git_path: null, source: 'agent', created_by: 'user-admin-1', created_at: '2026-07-05T00:00:00Z' },
  { id: 'sr2', spec_id: 's1', revision: 2, content_hash: 'h2', byte_size: 200, merged_from_change_id: null, merged_from_change_name: null, git_commit: null, git_path: null, source: 'import', created_by: 'user-admin-1', created_at: '2026-06-01T00:00:00Z' },
  { id: 'sr1', spec_id: 's1', revision: 1, content_hash: 'h1', byte_size: 100, merged_from_change_id: null, merged_from_change_name: null, git_commit: null, git_path: null, source: 'import', created_by: 'user-admin-1', created_at: '2026-05-01T00:00:00Z' },
]

const harnessLibraryDetail: SddSpecDetail = {
  ...harnessLibrary,
  content: '# Harness Library\n\nThe library MUST be versioned.',
  content_hash: 'h3',
}

/** Which specs the change `sdd-artifacts` has merged into. */
const mergedSpecs: SddSpecMerge[] = [{ ...harnessLibrary, merged_revision: 3 }]

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
  listSddSpecsMock,
  getSddSpecMock,
  listSddSpecRevisionsMock,
  getSddSpecRevisionMock,
  getSddChangeSpecsMock,
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
  listSddSpecsMock: vi.fn(),
  getSddSpecMock: vi.fn(),
  listSddSpecRevisionsMock: vi.fn(),
  getSddSpecRevisionMock: vi.fn(),
  getSddChangeSpecsMock: vi.fn(),
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
    listSddSpecs: listSddSpecsMock,
    getSddSpec: getSddSpecMock,
    listSddSpecRevisions: listSddSpecRevisionsMock,
    getSddSpecRevision: getSddSpecRevisionMock,
    getSddChangeSpecs: getSddChangeSpecsMock,
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
  listSddSpecsMock.mockResolvedValue(specs)
  getSddSpecMock.mockResolvedValue(harnessLibraryDetail)
  listSddSpecRevisionsMock.mockResolvedValue(specRevisions)
  getSddSpecRevisionMock.mockImplementation((_id: string, rev: number) =>
    Promise.resolve({
      ...specRevisions.find(r => r.revision === rev)!,
      content: `# Revision ${rev}\n\nThe contract as it stood at revision ${rev}.`,
    }),
  )
  getSddChangeSpecsMock.mockResolvedValue(mergedSpecs)
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

// ── Specs — the living specification ─────────────────────────────────────────
//
// The OTHER openspec tree. These assert the thing that was missing: the platform
// centralised the drafts but never the contract.

describe('Sdd — specs view', () => {
  it('sdd_specs_tab_lists_one_row_per_capability_with_its_revision_and_last_merge', async () => {
    renderAsAdmin()

    await waitFor(() => expect(screen.getByText('sdd-artifacts')).toBeInTheDocument())

    // The Changes view is the default — the specs are not showing yet.
    expect(screen.queryByText('harness-library')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: 'Specs' }))

    await waitFor(() => expect(screen.getByText('harness-library')).toBeInTheDocument())

    const row = screen.getByText('harness-library').closest('tr')!
    expect(within(row).getByText('Harness Library')).toBeInTheDocument()
    expect(within(row).getByText('rev 3')).toBeInTheDocument()
    // The payoff, on the list: which change last merged into this contract.
    expect(within(row).getByText('sdd-artifacts')).toBeInTheDocument()

    // A spec with no recorded provenance says so rather than inventing one.
    const imported = screen.getByText('policy-engine').closest('tr')!
    expect(within(imported).getByText('rev 1')).toBeInTheDocument()
    expect(within(imported).getByText('—')).toBeInTheDocument()

    expect(listSddSpecsMock).toHaveBeenCalled()
  })

  it('sdd_specs_list_never_asks_for_content', async () => {
    renderAsAdmin()
    fireEvent.click(screen.getByRole('tab', { name: 'Specs' }))
    await waitFor(() => expect(screen.getByText('harness-library')).toBeInTheDocument())

    // The list is metadata only: no detail read fires until a row is clicked.
    expect(getSddSpecMock).not.toHaveBeenCalled()
  })

  it('sdd_specs_shows_skeleton_while_loading_then_the_table', async () => {
    let resolve!: (v: SddSpec[]) => void
    listSddSpecsMock.mockReturnValue(new Promise<SddSpec[]>(r => { resolve = r }))

    const { container } = renderAsAdmin('/sdd?tab=specs')

    expect(container.querySelector('[data-testid="sdd-specs-skeleton"]')).not.toBeNull()

    resolve(specs)
    await waitFor(() => expect(screen.getByText('harness-library')).toBeInTheDocument())
    expect(container.querySelector('[data-testid="sdd-specs-skeleton"]')).toBeNull()
  })

  it('sdd_specs_empty_state_when_the_project_has_no_contract_yet', async () => {
    listSddSpecsMock.mockResolvedValue([])
    renderAsAdmin('/sdd?tab=specs')

    expect(await screen.findByText('No specifications found')).toBeInTheDocument()
  })

  it('sdd_specs_filters_by_project', async () => {
    renderAsAdmin('/sdd?tab=specs')
    await waitFor(() => expect(listSddSpecsMock).toHaveBeenCalledWith({ project: undefined }))
  })

  it('sdd_specs_denied_without_sdd_read_redirects_and_never_calls_the_api', async () => {
    // An UNGATED 403 trips the client's global handler and redirects the WHOLE app to
    // /401 — so the query must not fire at all for a caller without the grant.
    renderAsMember(['task:read'], '/sdd?tab=specs')

    await waitFor(() => {
      expect(listSddSpecsMock).not.toHaveBeenCalled()
    })
    expect(screen.queryByText('harness-library')).not.toBeInTheDocument()
  })

  it('sdd_specs_readable_with_sdd_read_alone', async () => {
    renderAsMember(['sdd:read'], '/sdd?tab=specs')
    await waitFor(() => expect(screen.getByText('harness-library')).toBeInTheDocument())
  })
})

describe('Sdd — spec detail drawer', () => {
  async function openSpecDrawer() {
    renderAsAdmin('/sdd?tab=specs')
    await waitFor(() => expect(screen.getByText('harness-library')).toBeInTheDocument())
    fireEvent.click(screen.getByText('harness-library'))
    await waitFor(() => expect(getSddSpecMock).toHaveBeenCalledWith('s1'))
    return screen.findByRole('dialog')
  }

  it('spec_drawer_renders_the_contract_as_markdown_by_default', async () => {
    await openSpecDrawer()

    // Preview is the default: the markdown is rendered, the `#` marker is gone.
    const panel = await screen.findByTestId('spec-panel')
    await waitFor(() => {
      expect(within(panel).getByText('Harness Library')).toBeInTheDocument()
    })
    expect(within(panel).queryByText('# Harness Library')).not.toBeInTheDocument()
    expect(screen.queryByTestId('spec-raw')).not.toBeInTheDocument()
  })

  it('spec_drawer_raw_toggle_shows_the_source_verbatim', async () => {
    await openSpecDrawer()
    await screen.findByTestId('spec-panel')

    fireEvent.click(screen.getByText('Raw'))

    const raw = await screen.findByTestId('spec-raw')
    expect(raw.textContent).toContain('# Harness Library')
    expect(raw.textContent).toContain('The library MUST be versioned.')

    // …and back.
    fireEvent.click(screen.getByText('Preview'))
    await waitFor(() => {
      expect(screen.queryByTestId('spec-raw')).not.toBeInTheDocument()
    })
  })

  it('spec_drawer_revision_selector_fetches_an_older_revision', async () => {
    await openSpecDrawer()

    await waitFor(() => {
      expect(listSddSpecRevisionsMock).toHaveBeenCalledWith('s1')
    })

    // The latest revision arrived inline with the detail read — no extra fetch for it.
    expect(getSddSpecRevisionMock).not.toHaveBeenCalled()

    fireEvent.click(screen.getByLabelText('Revision'))
    fireEvent.click(await screen.findByText(/rev 1 · import/))

    await waitFor(() => {
      expect(getSddSpecRevisionMock).toHaveBeenCalledWith('s1', 1)
    })
    const panel = await screen.findByTestId('spec-panel')
    await waitFor(() => {
      expect(within(panel).getByText(/The contract as it stood at revision 1/)).toBeInTheDocument()
    })
  })

  it('spec_drawer_revision_labels_name_the_change_that_merged_each_one', async () => {
    await openSpecDrawer()
    await waitFor(() => expect(listSddSpecRevisionsMock).toHaveBeenCalledWith('s1'))

    fireEvent.click(screen.getByLabelText('Revision'))

    // Revision 3 came from a change and says so. (It matches twice — the trigger shows
    // the selected option's label as well as the list does.)
    expect(await screen.findAllByText(/rev 3 · agent · .* · ← sdd-artifacts/)).not.toHaveLength(0)

    // Revisions 1 and 2 were imported: they name no change, and none is invented.
    expect(screen.getByText(/rev 1 · import/)).toBeInTheDocument()
    expect(screen.queryByText(/rev 1 .* ← /)).not.toBeInTheDocument()
    expect(screen.queryByText(/rev 2 .* ← /)).not.toBeInTheDocument()
  })

  it('spec_drawer_shows_the_change_that_last_merged_into_the_contract', async () => {
    await openSpecDrawer()

    const provenance = await screen.findByTestId('spec-provenance')
    expect(within(provenance).getByText('sdd-artifacts')).toBeInTheDocument()
  })

  it('spec_drawer_is_read_only_over_content', async () => {
    const dialog = await openSpecDrawer()
    await screen.findByTestId('spec-panel')

    // A7: the contract is authored by the harness and by git. No editor, no save.
    expect(within(dialog).queryByRole('textbox')).not.toBeInTheDocument()
    expect(within(dialog).queryByRole('button', { name: /save/i })).not.toBeInTheDocument()
    expect(within(dialog).queryByRole('button', { name: /delete/i })).not.toBeInTheDocument()
  })

  it('opens the spec drawer for a spec arrived at via ?spec=<id>', async () => {
    renderAsAdmin('/sdd?spec=s1')

    await waitFor(() => expect(getSddSpecMock).toHaveBeenCalledWith('s1'))
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
  })
})

describe('Sdd — a change reports the specs it merged into', () => {
  it('change_drawer_lists_the_specs_this_change_merged_into', async () => {
    renderAsAdmin()
    await waitFor(() => expect(screen.getByText('sdd-artifacts')).toBeInTheDocument())

    fireEvent.click(screen.getByText('sdd-artifacts'))
    await waitFor(() => expect(getSddChangeSpecsMock).toHaveBeenCalledWith('c1'))

    const merged = await screen.findByTestId('merged-specs')
    expect(within(merged).getByText('harness-library')).toBeInTheDocument()
    expect(within(merged).getByText('rev 3')).toBeInTheDocument()
  })

  it('change_drawer_says_so_when_the_change_has_merged_into_nothing', async () => {
    getSddChangeSpecsMock.mockResolvedValue([])
    renderAsAdmin()
    await waitFor(() => expect(screen.getByText('sdd-artifacts')).toBeInTheDocument())

    fireEvent.click(screen.getByText('sdd-artifacts'))

    const merged = await screen.findByTestId('merged-specs')
    await waitFor(() => {
      expect(
        within(merged).getByText(/has not been merged into any specification yet/),
      ).toBeInTheDocument()
    })
  })
})
