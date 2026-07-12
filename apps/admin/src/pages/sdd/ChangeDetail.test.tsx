import { describe, it, expect, vi, beforeEach } from 'vitest'
import { type ReactElement } from 'react'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../../auth/AuthContext'
import ChangeDetail from './ChangeDetail'
import clientSource from '../../api/client.ts?raw'
import changeDetailSource from './ChangeDetail.tsx?raw'
import type {
  AuthSession, Memory, SddArtifact, SddArtifactDetail, SddChange, SddRevisionMeta, Task,
} from '../../types'

// ── Fixture data ──────────────────────────────────────────────────────────────

const artifact = (id: string, kind: SddArtifact['kind'], capability = '', latest = 1): SddArtifact => ({
  id,
  change_id: 'c1',
  kind,
  capability,
  path: null,
  latest_revision: latest,
  created_at: '2026-07-01T00:00:00Z',
  updated_at: '2026-07-05T00:00:00Z',
})

// NB: these titles must NOT collide with any line of TASKS_MD below — the drawer
// renders both the linked-task list and the tasks.md artifact on the same screen.
const linkedTasks: Task[] = [
  {
    id: 't1', org_id: 'org-test-1', project: 'acme-platform', title: 'Land the migration PR',
    description: null, status: 'in_progress', priority: 'high', due_date: null, parent_id: null,
    sprint_id: null, created_by: 'user-admin-1', created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z', archived_at: null, assignees: [], labels: [],
    comment_count: 0, spec_links: ['sdd-artifacts'], subtask_count: 0,
  },
  {
    id: 't2', org_id: 'org-test-1', project: 'acme-platform', title: 'Land the store layer PR',
    description: null, status: 'done', priority: 'medium', due_date: null, parent_id: null,
    sprint_id: null, created_by: 'user-admin-1', created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z', archived_at: null, assignees: [], labels: [],
    comment_count: 0, spec_links: ['sdd-artifacts'], subtask_count: 0,
  },
]

const linkedMemory: Memory = {
  id: 'm1',
  org_id: 'org-test-1',
  user_id: 'user-admin-1',
  project: 'acme-platform',
  tool: 'claude-code',
  content: 'Chose SQLite over Postgres for the artifact store',
  tags: [],
  created_at: '2026-07-01T00:00:00Z',
  title: 'Artifact store engine decision',
  type: 'decision',
}

const unlinkedMemory: Memory = {
  ...linkedMemory,
  id: 'm2',
  title: 'Revision hashing gotcha',
  type: 'discovery',
}

/** proposal + 3 specs + design + tasks. Deliberately NO verify-report artifact. */
const change: SddChange = {
  id: 'c1',
  org_id: 'org-test-1',
  project: 'acme-platform',
  name: 'sdd-artifacts',
  title: 'SDD artifacts in NexusMind',
  status: 'active',
  phase: 'tasks',
  repo_url: null,
  repo_ref: null,
  sprint_id: null,
  created_by: 'user-admin-1',
  created_at: '2026-07-01T00:00:00Z',
  updated_at: '2026-07-05T00:00:00Z',
  archived_at: null,
  artifacts: [
    artifact('a-proposal', 'proposal'),
    artifact('a-spec-store', 'spec', 'sdd-artifact-store'),
    artifact('a-spec-api', 'spec', 'sdd-artifact-api'),
    artifact('a-spec-admin', 'spec', 'sdd-artifact-admin'),
    artifact('a-design', 'design'),
    artifact('a-tasks', 'tasks', '', 3),
  ],
  task_links: linkedTasks,
  memory_links: [linkedMemory],
}

/** proposal + design only — the "only existing kinds get tabs" fixture. */
const sparseChange: SddChange = {
  ...change,
  artifacts: [artifact('a-proposal', 'proposal'), artifact('a-design', 'design')],
}

const TASKS_MD = [
  '## Checklist',
  '',
  '- [x] Write the spec',
  '- [ ] Write the migration',
  '',
  '| PR | Est. lines |',
  '|----|-----------|',
  '| 8  | 340       |',
  '',
].join('\n')

const detail = (a: SddArtifact, content: string): SddArtifactDetail => ({
  // Flattened on the wire: the artifact's fields are INLINE, not nested under
  // `artifact`. A nested shape here would silently yield `undefined` content.
  ...a,
  change_name: 'sdd-artifacts',
  project: 'acme-platform',
  content,
  content_hash: `hash-${a.id}`,
})

const ARTIFACT_CONTENT: Record<string, string> = {
  'a-proposal': '# Proposal\n\nWhy SDD artifacts belong in NexusMind.',
  'a-design': '# Design\n\nThe store layer.',
  'a-tasks': TASKS_MD,
  'a-spec-store': '# Store spec\n\nThe store capability.',
  'a-spec-api': '# API spec\n\nThe api capability.',
  'a-spec-admin': '# Admin spec\n\nThe admin capability.',
}

const revisions: SddRevisionMeta[] = [
  { id: 'r3', artifact_id: 'a-tasks', revision: 3, content_hash: 'h3', byte_size: 900, git_commit: null, git_path: null, source: 'agent', created_by: 'user-admin-1', created_at: '2026-07-11T00:00:00Z' },
  { id: 'r2', artifact_id: 'a-tasks', revision: 2, content_hash: 'h2', byte_size: 800, git_commit: null, git_path: null, source: 'agent', created_by: 'user-admin-1', created_at: '2026-07-10T00:00:00Z' },
  { id: 'r1', artifact_id: 'a-tasks', revision: 1, content_hash: 'h1', byte_size: 700, git_commit: null, git_path: null, source: 'import', created_by: 'user-admin-1', created_at: '2026-07-09T00:00:00Z' },
]

const sprints = [
  { id: 's1', org_id: 'org-test-1', project: 'acme-platform', name: 'Sprint 12', goal: null, starts_at: null, ends_at: null, status: 'active' as const, created_by: 'user-admin-1', created_at: '2026-07-01T00:00:00Z', archived_at: null, task_count: 3 },
]

// ── Mocks ─────────────────────────────────────────────────────────────────────

const {
  getSddChangeMock,
  getSddChangeTasksMock,
  getSddArtifactMock,
  listSddArtifactRevisionsMock,
  getSddArtifactRevisionMock,
  getSddChangeSpecsMock,
  patchSddChangeMock,
  linkSddChangeMemoryMock,
  unlinkSddChangeMemoryMock,
  listMemoriesMock,
  listSprintsMock,
} = vi.hoisted(() => ({
  getSddChangeMock: vi.fn(),
  getSddChangeTasksMock: vi.fn(),
  getSddArtifactMock: vi.fn(),
  listSddArtifactRevisionsMock: vi.fn(),
  getSddArtifactRevisionMock: vi.fn(),
  getSddChangeSpecsMock: vi.fn(),
  patchSddChangeMock: vi.fn(),
  linkSddChangeMemoryMock: vi.fn(),
  unlinkSddChangeMemoryMock: vi.fn(),
  listMemoriesMock: vi.fn(),
  listSprintsMock: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  createClient: vi.fn(() => ({
    getSddChange: getSddChangeMock,
    getSddChangeTasks: getSddChangeTasksMock,
    getSddArtifact: getSddArtifactMock,
    listSddArtifactRevisions: listSddArtifactRevisionsMock,
    getSddArtifactRevision: getSddArtifactRevisionMock,
    getSddChangeSpecs: getSddChangeSpecsMock,
    patchSddChange: patchSddChangeMock,
    linkSddChangeMemory: linkSddChangeMemoryMock,
    unlinkSddChangeMemory: unlinkSddChangeMemoryMock,
    listMemories: listMemoriesMock,
    listSprints: listSprintsMock,
  })),
}))

// ── Renders ───────────────────────────────────────────────────────────────────

function renderDetail(permissions: string[] | null, ui?: ReactElement): ReturnType<typeof render> {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const session: AuthSession = {
    org: { id: 'org-test-1', name: 'Test Org', slug: 'test-org', created_at: '2026-01-01T00:00:00Z' },
    user: {
      id: 'user-1',
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
    <MemoryRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider
          value={{ session, loading: false, setSession: () => undefined, logout: () => undefined }}
        >
          {ui ?? <ChangeDetail changeId="c1" onClose={() => undefined} />}
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

const renderAsAdmin = () => renderDetail(null)
const renderAsMember = (permissions: string[]) => renderDetail(permissions)

beforeEach(() => {
  vi.clearAllMocks()
  getSddChangeMock.mockResolvedValue(change)
  getSddChangeTasksMock.mockResolvedValue(linkedTasks)
  getSddArtifactMock.mockImplementation((id: string) => {
    const a = change.artifacts.find(x => x.id === id) ?? change.artifacts[0]
    return Promise.resolve(detail(a, ARTIFACT_CONTENT[id] ?? '# Untitled'))
  })
  listSddArtifactRevisionsMock.mockResolvedValue(revisions)
  getSddChangeSpecsMock.mockResolvedValue([])
  getSddArtifactRevisionMock.mockImplementation((id: string, rev: number) =>
    Promise.resolve({
      id: `r${rev}`, artifact_id: id, revision: rev,
      content: `# Revision ${rev}\n\nThe old body of revision ${rev}.`,
      content_hash: `h${rev}`, byte_size: 100, git_commit: null, git_path: null,
      source: 'agent', created_by: 'user-admin-1', created_at: '2026-07-09T00:00:00Z',
    }),
  )
  patchSddChangeMock.mockResolvedValue({ ...change, phase: 'verify' })
  linkSddChangeMemoryMock.mockResolvedValue([linkedMemory, unlinkedMemory])
  unlinkSddChangeMemoryMock.mockResolvedValue(undefined)
  listMemoriesMock.mockResolvedValue([linkedMemory, unlinkedMemory])
  listSprintsMock.mockResolvedValue(sprints)
})

// ── Tabs ──────────────────────────────────────────────────────────────────────

describe('ChangeDetail — artifact tabs', () => {
  it('change_detail_renders_one_tab_per_existing_artifact_kind', async () => {
    getSddChangeMock.mockResolvedValue(sparseChange)
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^proposal$/i })).toBeInTheDocument()
    })

    expect(screen.getByRole('tab', { name: /^design$/i })).toBeInTheDocument()
    // No `tasks` artifact ⇒ no Tasks tab. The inventory drives the tab strip.
    expect(screen.queryByRole('tab', { name: /^tasks$/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('tab', { name: /^specs$/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('tab', { name: /^verify$/i })).not.toBeInTheDocument()
  })

  it('does not render a Verify tab when the change has no verify-report artifact', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^proposal$/i })).toBeInTheDocument()
    })

    expect(screen.getByRole('tab', { name: /^specs$/i })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: /^tasks$/i })).toBeInTheDocument()
    expect(screen.queryByRole('tab', { name: /^verify$/i })).not.toBeInTheDocument()
  })
})

// ── Markdown rendering ────────────────────────────────────────────────────────

describe('ChangeDetail — markdown rendering', () => {
  it('change_detail_renders_the_selected_artifact_as_rendered_markdown', async () => {
    const { container } = renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^tasks$/i })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('tab', { name: /^tasks$/i }))

    await waitFor(() => {
      expect(getSddArtifactMock).toHaveBeenCalledWith('a-tasks')
    })

    // Real checkboxes, not literal `- [ ]` — the PR-7 primitive, end to end.
    await waitFor(() => {
      expect(container.querySelectorAll('input[type="checkbox"]')).toHaveLength(2)
    })
    const boxes = container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')
    expect(boxes[0].checked).toBe(true)
    expect(boxes[1].checked).toBe(false)
    // Read-only: an artifact checkbox is never an editable control (A7).
    expect(boxes[0].disabled).toBe(true)

    // …and a real table.
    const table = container.querySelector('table')
    expect(table).not.toBeNull()
    expect(within(table as HTMLElement).getByRole('columnheader', { name: 'PR' })).toBeInTheDocument()

    expect(container.textContent).not.toContain('- [ ]')
  })
})

// ── Raw / Preview ─────────────────────────────────────────────────────────────

describe('ChangeDetail — raw/preview toggle', () => {
  it('change_detail_raw_preview_toggle_switches_between_source_and_render', async () => {
    const { container } = renderAsAdmin()

    await waitFor(() => {
      expect(getSddArtifactMock).toHaveBeenCalledWith('a-proposal')
    })
    await waitFor(() => {
      expect(container.querySelector('h1')).not.toBeNull()
    })

    // Preview is the default: markdown is rendered, the `#` heading marker is gone.
    expect(screen.getByRole('heading', { name: /^proposal$/i })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /^raw$/i }))

    // Raw: the source verbatim, unrendered.
    const raw = await screen.findByTestId('artifact-raw')
    expect(raw.textContent).toBe(ARTIFACT_CONTENT['a-proposal'])
    expect(raw.textContent).toContain('# Proposal')
    expect(raw.querySelector('h1')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /^preview$/i }))

    await waitFor(() => {
      expect(screen.queryByTestId('artifact-raw')).not.toBeInTheDocument()
    })
    expect(screen.getByRole('heading', { name: /^proposal$/i })).toBeInTheDocument()
  })
})

// ── Specs tab / capabilities ──────────────────────────────────────────────────

describe('ChangeDetail — specs tab', () => {
  it('change_detail_specs_tab_lists_one_entry_per_capability', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^specs$/i })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('tab', { name: /^specs$/i }))

    const list = await screen.findByTestId('spec-capabilities')
    expect(within(list).getByRole('button', { name: 'sdd-artifact-store' })).toBeInTheDocument()
    expect(within(list).getByRole('button', { name: 'sdd-artifact-api' })).toBeInTheDocument()
    expect(within(list).getByRole('button', { name: 'sdd-artifact-admin' })).toBeInTheDocument()

    // Selecting a capability renders that capability's spec content.
    fireEvent.click(within(list).getByRole('button', { name: 'sdd-artifact-api' }))

    await waitFor(() => {
      expect(getSddArtifactMock).toHaveBeenCalledWith('a-spec-api')
    })
    await waitFor(() => {
      expect(screen.getByText(/the api capability/i)).toBeInTheDocument()
    })
  })
})

// ── Revisions ─────────────────────────────────────────────────────────────────

describe('ChangeDetail — revision selector', () => {
  it('change_detail_revision_selector_refetches_and_renders_the_selected_revision', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^tasks$/i })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole('tab', { name: /^tasks$/i }))

    await waitFor(() => {
      expect(listSddArtifactRevisionsMock).toHaveBeenCalledWith('a-tasks')
    })
    // The latest revision (3) is shown by default, from the artifact detail read.
    await waitFor(() => {
      expect(screen.getByText('Write the migration')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /revision/i }))
    fireEvent.click(await screen.findByRole('option', { name: /rev 1/i }))

    await waitFor(() => {
      expect(getSddArtifactRevisionMock).toHaveBeenCalledWith('a-tasks', 1)
    })
    // Revision 1's content replaces revision 3's.
    await waitFor(() => {
      expect(screen.getByText(/the old body of revision 1/i)).toBeInTheDocument()
    })
    expect(screen.queryByText('Write the migration')).not.toBeInTheDocument()
  })

  it('change_detail_revision_selector_shows_timestamp_and_source_per_revision', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^tasks$/i })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole('tab', { name: /^tasks$/i }))

    await waitFor(() => {
      expect(listSddArtifactRevisionsMock).toHaveBeenCalledWith('a-tasks')
    })

    fireEvent.click(screen.getByRole('button', { name: /revision/i }))

    const option = await screen.findByRole('option', { name: /rev 1/i })
    // `rev 1 · import · <date>` — source and timestamp, per revision.
    expect(option.textContent).toMatch(/import/)
    expect(option.textContent).toMatch(/2026/)

    const agentOption = screen.getByRole('option', { name: /rev 2/i })
    expect(agentOption.textContent).toMatch(/agent/)

    // Diff UI is explicitly out of scope (proposal §4).
    expect(screen.queryByText(/diff/i)).not.toBeInTheDocument()
  })
})

// ── Linked tasks & memories ───────────────────────────────────────────────────

describe('ChangeDetail — linked tasks and memories', () => {
  it('change_detail_renders_linked_tasks_and_memories', async () => {
    renderAsAdmin()

    await waitFor(() => {
      expect(getSddChangeTasksMock).toHaveBeenCalledWith('c1')
    })

    const tasksSection = await screen.findByTestId('linked-tasks')
    await waitFor(() => {
      expect(within(tasksSection).getByText('Land the migration PR')).toBeInTheDocument()
    })

    const t1 = within(tasksSection).getByText('Land the migration PR').closest('li')!
    expect(within(t1).getByText('in_progress')).toBeInTheDocument()
    expect(within(t1).getByRole('link', { name: /land the migration pr/i }))
      .toHaveAttribute('href', '/tasks?task=t1')

    const t2 = within(tasksSection).getByText('Land the store layer PR').closest('li')!
    expect(within(t2).getByText('done')).toBeInTheDocument()

    const memSection = await screen.findByTestId('linked-memories')
    await waitFor(() => {
      expect(within(memSection).getByText('Artifact store engine decision')).toBeInTheDocument()
    })

    const m1 = within(memSection).getByText('Artifact store engine decision').closest('li')!
    expect(within(m1).getByText('decision')).toBeInTheDocument()
    expect(within(m1).getByRole('link', { name: /artifact store engine decision/i }))
      .toHaveAttribute('href', '/memories?id=m1')
  })
})

// ── A7: read-only over artifact CONTENT ───────────────────────────────────────

describe('ChangeDetail — the admin is read-only over artifacts', () => {
  it('change_detail_presents_no_artifact_edit_save_or_delete_control', async () => {
    // A user who HOLDS sdd:write still gets no artifact-content editor.
    renderAsMember(['sdd:read', 'sdd:write'])

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^tasks$/i })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole('tab', { name: /^tasks$/i }))

    await waitFor(() => {
      expect(getSddArtifactMock).toHaveBeenCalledWith('a-tasks')
    })

    const panel = await screen.findByTestId('artifact-panel')
    // No editable control bound to artifact content.
    expect(panel.querySelector('textarea')).toBeNull()
    expect(panel.querySelector('input:not([type="checkbox"])')).toBeNull()
    expect(panel.querySelectorAll('input[type="checkbox"]:not([disabled])')).toHaveLength(0)
    expect(panel.getAttribute('contenteditable')).toBeNull()

    // No edit / save / delete affordance for the artifact.
    expect(within(panel).queryByRole('button', { name: /^edit/i })).not.toBeInTheDocument()
    expect(within(panel).queryByRole('button', { name: /^save/i })).not.toBeInTheDocument()
    expect(within(panel).queryByRole('button', { name: /delete/i })).not.toBeInTheDocument()
  })

  it('admin_issues_no_artifact_save_request', async () => {
    renderAsMember(['sdd:read', 'sdd:write'])

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^tasks$/i })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole('tab', { name: /^tasks$/i }))
    await waitFor(() => {
      expect(getSddArtifactMock).toHaveBeenCalledWith('a-tasks')
    })
    fireEvent.click(screen.getByRole('button', { name: /revision/i }))
    fireEvent.click(await screen.findByRole('option', { name: /rev 1/i }))
    await waitFor(() => {
      expect(getSddArtifactRevisionMock).toHaveBeenCalledWith('a-tasks', 1)
    })

    // The client exposes NO artifact-save method at all — the capability does not
    // exist in the admin, so no code path can reach `PUT /v1/sdd/artifacts`.
    expect(clientSource).not.toMatch(/saveSddArtifact|putSddArtifact|upsertSddArtifact/)
    expect(clientSource).not.toMatch(/'\/v1\/sdd\/artifacts',\s*\{\s*method:/)
    expect(changeDetailSource).not.toMatch(/method:\s*'?"?(PUT|POST)/i)
    expect(changeDetailSource).not.toMatch(/\/v1\/sdd\/artifacts/)
  })
})

// ── A7: curation IS permitted ─────────────────────────────────────────────────

describe('ChangeDetail — curation of change metadata (A7)', () => {
  it('change_detail_allows_curation_of_phase_status_and_sprint', async () => {
    renderAsMember(['sdd:read', 'sdd:write'])

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /^phase$/i })).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /^phase$/i }))
    fireEvent.click(await screen.findByRole('option', { name: /^verify$/i }))

    await waitFor(() => {
      expect(patchSddChangeMock).toHaveBeenCalledWith('c1', { phase: 'verify' })
    })

    fireEvent.click(screen.getByRole('button', { name: /^status$/i }))
    fireEvent.click(await screen.findByRole('option', { name: /^abandoned$/i }))
    await waitFor(() => {
      expect(patchSddChangeMock).toHaveBeenCalledWith('c1', { status: 'abandoned' })
    })

    fireEvent.click(screen.getByRole('button', { name: /^sprint$/i }))
    fireEvent.click(await screen.findByRole('option', { name: /sprint 12/i }))
    await waitFor(() => {
      expect(patchSddChangeMock).toHaveBeenCalledWith('c1', { sprint_id: 's1' })
    })

    // The detail read is invalidated, so the drawer reflects the new phase.
    await waitFor(() => {
      expect(getSddChangeMock.mock.calls.length).toBeGreaterThan(1)
    })
  })

  it('never sends project or name in the patch body (the backend denies unknown fields)', async () => {
    renderAsMember(['sdd:read', 'sdd:write'])

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /^phase$/i })).toBeInTheDocument()
    })
    fireEvent.click(screen.getByRole('button', { name: /^phase$/i }))
    fireEvent.click(await screen.findByRole('option', { name: /^verify$/i }))

    await waitFor(() => {
      expect(patchSddChangeMock).toHaveBeenCalled()
    })
    const body = patchSddChangeMock.mock.calls[0][1]
    expect(body).not.toHaveProperty('project')
    expect(body).not.toHaveProperty('name')
  })

  it('hides the curation controls without sdd:write', async () => {
    renderAsMember(['sdd:read'])

    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /^proposal$/i })).toBeInTheDocument()
    })

    expect(screen.queryByRole('button', { name: /^phase$/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^status$/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^sprint$/i })).not.toBeInTheDocument()
  })
})

describe('ChangeDetail — memory links (A7)', () => {
  it('change_detail_allows_linking_and_unlinking_memories', async () => {
    renderAsMember(['sdd:read', 'sdd:write'])

    const memSection = await screen.findByTestId('linked-memories')

    // Link
    fireEvent.click(within(memSection).getByRole('button', { name: /link memory/i }))
    fireEvent.click(await screen.findByRole('option', { name: /revision hashing gotcha/i }))
    fireEvent.click(within(memSection).getByRole('button', { name: /^link$/i }))

    await waitFor(() => {
      expect(linkSddChangeMemoryMock).toHaveBeenCalledWith('c1', { memory_id: 'm2' })
    })

    // Unlink
    fireEvent.click(within(memSection).getByRole('button', { name: /unlink artifact store engine decision/i }))
    await waitFor(() => {
      expect(unlinkSddChangeMemoryMock).toHaveBeenCalledWith('c1', 'm1')
    })

    // Both invalidate the hydrated change read.
    await waitFor(() => {
      expect(getSddChangeMock.mock.calls.length).toBeGreaterThan(1)
    })
  })

  it('hides the memory link/unlink controls without sdd:write', async () => {
    renderAsMember(['sdd:read'])

    const memSection = await screen.findByTestId('linked-memories')
    // The memory is still *listed* — reading links is an sdd:read affordance.
    await waitFor(() => {
      expect(within(memSection).getByText('Artifact store engine decision')).toBeInTheDocument()
    })

    expect(within(memSection).queryByRole('button', { name: /link memory/i })).not.toBeInTheDocument()
    expect(within(memSection).queryByRole('button', { name: /unlink/i })).not.toBeInTheDocument()
  })
})
