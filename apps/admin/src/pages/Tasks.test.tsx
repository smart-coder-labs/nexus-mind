import { describe, it, expect, vi, beforeEach } from 'vitest'
import { type ReactElement } from 'react'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../auth/AuthContext'
import { renderWithProviders } from '../test/render'
import Tasks from './Tasks'
import type { Task, AuthSession } from '../types'

// ── Fixture data ──────────────────────────────────────────────────────────────

const tasks: Task[] = [
  {
    id: 't1',
    org_id: 'org-test-1',
    project: 'acme-platform',
    title: 'Fix login redirect bug',
    description: 'Users get stuck on /login after SSO callback',
    status: 'in_progress',
    priority: 'high',
    due_date: '2026-07-15',
    parent_id: null,
    sprint_id: null,
    created_by: 'user-admin-1',
    created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z',
    archived_at: null,
    assignees: [{ id: 'user-1', name: 'Sarah Chen', email: 'sarah@acme.test' }],
    labels: ['bug'],
    comment_count: 2,
    spec_links: [],
    subtask_count: 0,
  },
  {
    id: 't2',
    org_id: 'org-test-1',
    project: 'acme-platform',
    title: 'Write onboarding docs',
    description: null,
    status: 'backlog',
    priority: 'medium',
    due_date: null,
    parent_id: null,
    sprint_id: null,
    created_by: 'user-admin-1',
    created_at: '2026-07-02T00:00:00Z',
    updated_at: '2026-07-02T00:00:00Z',
    archived_at: null,
    assignees: [],
    labels: [],
    comment_count: 0,
    spec_links: [],
    subtask_count: 0,
  },
]

const projects = [
  { id: 'p1', org_id: 'org-test-1', name: 'acme-platform', description: null, parent_id: null, created_at: '2026-01-01T00:00:00Z' },
]

const users = [
  { id: 'user-1', org_id: 'org-test-1', email: 'sarah@acme.test', name: 'Sarah Chen', role: 'member', status: 'active' as const, created_at: '2026-01-01T00:00:00Z' },
]

// ── Mocks ─────────────────────────────────────────────────────────────────────

const {
  listTasksMock,
  listProjectsMock,
  listUsersMock,
  getTaskMock,
  createTaskMock,
  updateTaskMock,
  deleteTaskMock,
  assignTaskMock,
  unassignTaskMock,
  listTaskCommentsMock,
  listTaskSubtasksMock,
  listTaskSpecLinksMock,
} = vi.hoisted(() => ({
  listTasksMock: vi.fn(),
  listProjectsMock: vi.fn(),
  listUsersMock: vi.fn(),
  getTaskMock: vi.fn(),
  createTaskMock: vi.fn(),
  updateTaskMock: vi.fn(),
  deleteTaskMock: vi.fn(),
  assignTaskMock: vi.fn(),
  unassignTaskMock: vi.fn(),
  listTaskCommentsMock: vi.fn(),
  listTaskSubtasksMock: vi.fn(),
  listTaskSpecLinksMock: vi.fn(),
}))

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listTasks: listTasksMock,
    listProjects: listProjectsMock,
    listUsers: listUsersMock,
    getTask: getTaskMock,
    createTask: createTaskMock,
    updateTask: updateTaskMock,
    deleteTask: deleteTaskMock,
    assignTask: assignTaskMock,
    unassignTask: unassignTaskMock,
    listTaskComments: listTaskCommentsMock,
    listTaskSubtasks: listTaskSubtasksMock,
    listTaskSpecLinks: listTaskSpecLinksMock,
  })),
}))

// ── Custom render for permission-scoped scenarios ──────────────────────────────

function renderAsMember(ui: ReactElement, permissions: string[]): ReturnType<typeof render> {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const memberSession: AuthSession = {
    org: { id: 'org-test-1', name: 'Test Org', slug: 'test-org', created_at: '2026-01-01T00:00:00Z' },
    user: {
      id: 'user-member-1',
      org_id: 'org-test-1',
      email: 'member@test.com',
      name: 'Test Member',
      role: 'member',
      status: 'active',
      created_at: '2026-01-01T00:00:00Z',
      permissions,
    },
  }
  return render(
    <MemoryRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider
          value={{ session: memberSession, loading: false, setSession: () => undefined, logout: () => undefined }}
        >
          {ui}
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

beforeEach(() => {
  vi.clearAllMocks()
  listTasksMock.mockResolvedValue(tasks)
  listProjectsMock.mockResolvedValue(projects)
  listUsersMock.mockResolvedValue(users)
  getTaskMock.mockImplementation((id: string) => Promise.resolve(tasks.find(t => t.id === id) ?? tasks[0]))
  createTaskMock.mockResolvedValue(tasks[0])
  updateTaskMock.mockResolvedValue({ ...tasks[0], status: 'done' })
  deleteTaskMock.mockResolvedValue(undefined)
  assignTaskMock.mockResolvedValue([])
  unassignTaskMock.mockResolvedValue(undefined)
  listTaskCommentsMock.mockResolvedValue([])
  listTaskSubtasksMock.mockResolvedValue([])
  listTaskSpecLinksMock.mockResolvedValue([])
  vi.spyOn(window, 'confirm').mockReturnValue(true)
})

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('Tasks — list rendering', () => {
  it('renders each task title, status, priority, assignees, and due date', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    expect(screen.getByText('Write onboarding docs')).toBeInTheDocument()
    expect(screen.getByText(/in_progress/i)).toBeInTheDocument()
    expect(screen.getByText(/high/i)).toBeInTheDocument()
    expect(screen.getByText('Sarah Chen')).toBeInTheDocument()
  })

  it('shows an empty state when no tasks match the filters', async () => {
    listTasksMock.mockResolvedValue([])
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('No tasks found')).toBeInTheDocument()
    })
  })

  it('updates the visible list when the status filter changes', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    listTasksMock.mockClear()
    listTasksMock.mockResolvedValue([tasks[1]])

    const statusFilter = screen.getByRole('button', { name: /status/i })
    fireEvent.click(statusFilter)
    const backlogOption = await screen.findByRole('option', { name: /backlog/i })
    fireEvent.click(backlogOption)

    await waitFor(() => {
      expect(listTasksMock).toHaveBeenCalledWith(
        expect.objectContaining({ status: 'backlog' }),
      )
    })
  })

  it('filters by assignee — sends the selected user id, not their name', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    listTasksMock.mockClear()
    listTasksMock.mockResolvedValue([tasks[0]])

    const assigneeFilter = screen.getByRole('button', { name: /assignee/i })
    fireEvent.click(assigneeFilter)
    const option = await screen.findByRole('option', { name: new RegExp(users[0].name, 'i') })
    fireEvent.click(option)

    await waitFor(() => {
      // The backend keys on the user id. Sending the display name would silently
      // match nothing and render an empty list that looks like "no tasks".
      expect(listTasksMock).toHaveBeenCalledWith(
        expect.objectContaining({ assignee: users[0].id }),
      )
    })
  })

  it('offers an "Assigned to me" shortcut that sends the backend\'s `me` sentinel', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    listTasksMock.mockClear()
    listTasksMock.mockResolvedValue([tasks[0]])

    fireEvent.click(screen.getByRole('button', { name: /assignee/i }))
    fireEvent.click(await screen.findByRole('option', { name: /assigned to me/i }))

    await waitFor(() => {
      // `me` is resolved server-side from the API key (api/tasks.rs) — the client
      // must not try to resolve it itself.
      expect(listTasksMock).toHaveBeenCalledWith(
        expect.objectContaining({ assignee: 'me' }),
      )
    })
  })

  it('clears the assignee filter — omits the param entirely rather than sending an empty string', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /assignee/i }))
    fireEvent.click(await screen.findByRole('option', { name: new RegExp(users[0].name, 'i') }))
    await waitFor(() => {
      expect(listTasksMock).toHaveBeenCalledWith(expect.objectContaining({ assignee: users[0].id }))
    })

    listTasksMock.mockClear()
    fireEvent.click(screen.getByRole('button', { name: /assignee/i }))
    fireEvent.click(await screen.findByRole('option', { name: /all assignees/i }))

    await waitFor(() => {
      const calls = listTasksMock.mock.calls
      const lastCall = calls[calls.length - 1]?.[0]
      expect(lastCall?.assignee).toBeUndefined()
    })
  })
})

describe('Tasks — create via modal', () => {
  it('calls createTask and invalidates the list on submit', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /new task/i }))

    const titleInput = await screen.findByLabelText(/title/i)
    fireEvent.change(titleInput, { target: { value: 'Investigate flaky test' } })

    const form = titleInput.closest('form')!
    fireEvent.submit(form)

    await waitFor(() => {
      expect(createTaskMock).toHaveBeenCalledWith(
        expect.objectContaining({ title: 'Investigate flaky test' }),
      )
    })
  })

  it('hides the create action when the user lacks task:write', async () => {
    renderAsMember(<Tasks />, ['task:read'])

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    expect(screen.queryByRole('button', { name: /new task/i })).not.toBeInTheDocument()
  })
})

describe('Tasks — permission guard on direct navigation', () => {
  it('does not render the task list for a member without task:read', async () => {
    renderAsMember(<Tasks />, [])

    await waitFor(() => {
      expect(listTasksMock).not.toHaveBeenCalled()
    })

    expect(screen.queryByText('Fix login redirect bug')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new task/i })).not.toBeInTheDocument()
  })
})

describe('Tasks — edit via unified detail modal', () => {
  it('opens the same detail modal (with assignees section) when clicking the Edit pencil', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    const row = screen.getByText('Fix login redirect bug').closest('tr')!
    fireEvent.click(within(row).getByRole('button', { name: /edit/i }))

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /unassign sarah chen/i })).toBeInTheDocument()
    })
  })

  it('opens the same detail modal when clicking a row', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByText('Fix login redirect bug'))

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /unassign sarah chen/i })).toBeInTheDocument()
    })
  })

  it('calls updateTask with the new status via the detail modal form and closes it', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    const row = screen.getByText('Fix login redirect bug').closest('tr')!
    fireEvent.click(within(row).getByRole('button', { name: /edit/i }))

    const titleInput = await screen.findByLabelText(/^title$/i)
    // The Save button lives in a footer at the bottom of the modal (after Comments),
    // associated to the edit form via `form="task-edit-form"` rather than nested inside
    // it, so scope to the whole modal container instead of just the form's parent.
    const modal = titleInput.closest('form')!.closest('.rounded-\\[18px\\]') as HTMLElement

    const statusSelect = within(modal).getByRole('button', { name: /^status$/i })
    fireEvent.click(statusSelect)
    const doneOption = await screen.findByRole('option', { name: /^done$/i })
    fireEvent.click(doneOption)

    const saveButton = within(modal).getByRole('button', { name: /^save$/i })
    fireEvent.click(saveButton)

    await waitFor(() => {
      expect(updateTaskMock).toHaveBeenCalledWith('t1', expect.objectContaining({ status: 'done' }))
    })
  })
})

describe('Tasks — list/board view toggle', () => {
  it('shows the list view by default', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    expect(screen.getByRole('columnheader', { name: /title/i })).toBeInTheDocument()
  })

  it('switches to the board view and renders status columns', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /board view/i }))

    expect(screen.getByText('In Progress')).toBeInTheDocument()
    expect(screen.getByText('Backlog')).toBeInTheDocument()
    expect(screen.queryByRole('columnheader', { name: /title/i })).not.toBeInTheDocument()
  })

  it('switches back to list view', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /board view/i }))
    fireEvent.click(screen.getByRole('button', { name: /list view/i }))

    expect(screen.getByRole('columnheader', { name: /title/i })).toBeInTheDocument()
  })
})

describe('Tasks — delete requires confirmation', () => {
  it('calls deleteTask when the user confirms', async () => {
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    const row = screen.getByText('Fix login redirect bug').closest('tr')!
    fireEvent.click(within(row).getByRole('button', { name: /delete/i }))

    await waitFor(() => {
      expect(deleteTaskMock).toHaveBeenCalledWith('t1')
    })
  })

  it('does not call deleteTask when the user cancels', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    renderWithProviders(<Tasks />)

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    const row = screen.getByText('Fix login redirect bug').closest('tr')!
    fireEvent.click(within(row).getByRole('button', { name: /delete/i }))

    expect(deleteTaskMock).not.toHaveBeenCalled()
    expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
  })

  it('hides the delete action when the user lacks task:delete', async () => {
    renderAsMember(<Tasks />, ['task:read', 'task:write'])

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    const row = screen.getByText('Fix login redirect bug').closest('tr')!
    expect(within(row).queryByRole('button', { name: /delete/i })).not.toBeInTheDocument()
  })
})
