import { describe, it, expect, vi, beforeEach } from 'vitest'
import { type ReactElement } from 'react'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../../auth/AuthContext'
import { renderWithProviders } from '../../test/render'
import TaskDetail from './TaskDetail'
import type { Task, AuthSession } from '../../types'

// ── Fixture data ──────────────────────────────────────────────────────────────

const task: Task = {
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
  spec_links: ['team-tasks'],
  subtask_count: 1,
}

const users = [
  { id: 'user-1', org_id: 'org-test-1', email: 'sarah@acme.test', name: 'Sarah Chen', role: 'member', status: 'active' as const, created_at: '2026-01-01T00:00:00Z' },
  { id: 'user-2', org_id: 'org-test-1', email: 'raj@acme.test', name: 'Raj Patel', role: 'member', status: 'active' as const, created_at: '2026-01-01T00:00:00Z' },
]

const comments = [
  { id: 'c1', task_id: 't1', user_id: 'user-admin-1', author_name: 'Test Admin', body: 'Looking into it', created_at: '2026-07-01T00:00:00Z' },
]

const subtasks: Task[] = [
  { ...task, id: 't1a', title: 'Reproduce the bug', status: 'done', parent_id: 't1' },
]

const specLinks = ['team-tasks']

// A hydrated version of the task as returned by GET /v1/tasks/:id — this is
// what list_tasks does NOT include (assignees/labels are empty on the list).
const hydratedTask: Task = {
  ...task,
  assignees: [{ id: 'user-1', name: 'Sarah Chen', email: 'sarah@acme.test' }],
  labels: ['bug'],
}

// ── Mocks ─────────────────────────────────────────────────────────────────────

const {
  getTaskMock,
  updateTaskMock,
  listUsersMock,
  assignTaskMock,
  unassignTaskMock,
  listTaskCommentsMock,
  addTaskCommentMock,
  listTaskSubtasksMock,
  createTaskMock,
  addTaskLabelMock,
  removeTaskLabelMock,
  listTaskSpecLinksMock,
  linkTaskSpecMock,
  unlinkTaskSpecMock,
} = vi.hoisted(() => ({
  getTaskMock: vi.fn(),
  updateTaskMock: vi.fn(),
  listUsersMock: vi.fn(),
  assignTaskMock: vi.fn(),
  unassignTaskMock: vi.fn(),
  listTaskCommentsMock: vi.fn(),
  addTaskCommentMock: vi.fn(),
  listTaskSubtasksMock: vi.fn(),
  createTaskMock: vi.fn(),
  addTaskLabelMock: vi.fn(),
  removeTaskLabelMock: vi.fn(),
  listTaskSpecLinksMock: vi.fn(),
  linkTaskSpecMock: vi.fn(),
  unlinkTaskSpecMock: vi.fn(),
}))

vi.mock('../../api/client', () => ({
  createClient: vi.fn(() => ({
    getTask: getTaskMock,
    updateTask: updateTaskMock,
    listUsers: listUsersMock,
    assignTask: assignTaskMock,
    unassignTask: unassignTaskMock,
    listTaskComments: listTaskCommentsMock,
    addTaskComment: addTaskCommentMock,
    listTaskSubtasks: listTaskSubtasksMock,
    createTask: createTaskMock,
    addTaskLabel: addTaskLabelMock,
    removeTaskLabel: removeTaskLabelMock,
    listTaskSpecLinks: listTaskSpecLinksMock,
    linkTaskSpec: linkTaskSpecMock,
    unlinkTaskSpec: unlinkTaskSpecMock,
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
  getTaskMock.mockResolvedValue(hydratedTask)
  updateTaskMock.mockResolvedValue(hydratedTask)
  listUsersMock.mockResolvedValue(users)
  assignTaskMock.mockResolvedValue([{ id: 'user-2', name: 'Raj Patel', email: 'raj@acme.test' }])
  unassignTaskMock.mockResolvedValue(undefined)
  listTaskCommentsMock.mockResolvedValue(comments)
  addTaskCommentMock.mockResolvedValue({ id: 'c2', task_id: 't1', user_id: 'user-admin-1', author_name: 'Test Admin', body: 'New comment', created_at: '2026-07-02T00:00:00Z' })
  listTaskSubtasksMock.mockResolvedValue(subtasks)
  createTaskMock.mockResolvedValue({ ...task, id: 't1b', title: 'New subtask' })
  addTaskLabelMock.mockResolvedValue(['bug', 'urgent'])
  removeTaskLabelMock.mockResolvedValue(undefined)
  listTaskSpecLinksMock.mockResolvedValue(specLinks)
  linkTaskSpecMock.mockResolvedValue(undefined)
  unlinkTaskSpecMock.mockResolvedValue(undefined)
})

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('TaskDetail — renders comments, labels, subtasks, spec links', () => {
  it('renders the linked spec change names', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('team-tasks')).toBeInTheDocument()
    })
  })

  it('renders subtasks with their own status distinct from the parent', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Reproduce the bug')).toBeInTheDocument()
    })

    const subtaskRow = screen.getByText('Reproduce the bug').closest('li')!
    expect(within(subtaskRow).getByText(/^done$/i)).toBeInTheDocument()
  })

  it('renders existing labels as chips', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('bug')).toBeInTheDocument()
    })
  })

  it('renders the comment thread', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Looking into it')).toBeInTheDocument()
    })
    expect(screen.getByText('Test Admin')).toBeInTheDocument()
  })
})

describe('TaskDetail — adding a comment', () => {
  it('calls addTaskComment and refetches the thread when task:write is held', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Looking into it')).toBeInTheDocument()
    })

    const input = screen.getByLabelText(/add a comment/i)
    fireEvent.change(input, { target: { value: 'New comment' } })
    fireEvent.click(screen.getByRole('button', { name: /post/i }))

    await waitFor(() => {
      expect(addTaskCommentMock).toHaveBeenCalledWith('t1', 'New comment')
    })
    await waitFor(() => {
      expect(listTaskCommentsMock).toHaveBeenCalledTimes(2)
    })
  })

  it('hides the comment form without task:write', async () => {
    renderAsMember(<TaskDetail task={task} onClose={() => undefined} />, ['task:read'])

    await waitFor(() => {
      expect(screen.getByText('Looking into it')).toBeInTheDocument()
    })

    expect(screen.queryByLabelText(/add a comment/i)).not.toBeInTheDocument()
  })
})

describe('TaskDetail — labels', () => {
  it('calls addTaskLabel when submitting a new label', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('bug')).toBeInTheDocument()
    })

    const labelInput = screen.getByLabelText(/add label/i)
    fireEvent.change(labelInput, { target: { value: 'urgent' } })
    fireEvent.submit(labelInput.closest('form')!)

    await waitFor(() => {
      expect(addTaskLabelMock).toHaveBeenCalledWith('t1', 'urgent')
    })
  })

  it('calls removeTaskLabel when removing an existing label chip', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('bug')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /remove label bug/i }))

    await waitFor(() => {
      expect(removeTaskLabelMock).toHaveBeenCalledWith('t1', 'bug')
    })
  })

  it('hides label editing without task:write', async () => {
    renderAsMember(<TaskDetail task={task} onClose={() => undefined} />, ['task:read'])

    await waitFor(() => {
      expect(screen.getByText('bug')).toBeInTheDocument()
    })

    expect(screen.queryByLabelText(/add label/i)).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /remove label bug/i })).not.toBeInTheDocument()
  })
})

describe('TaskDetail — subtasks', () => {
  it('calls createTask with parent_id when creating a subtask', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Reproduce the bug')).toBeInTheDocument()
    })

    const subtaskInput = screen.getByLabelText(/new subtask title/i)
    fireEvent.change(subtaskInput, { target: { value: 'Write regression test' } })
    fireEvent.submit(subtaskInput.closest('form')!)

    await waitFor(() => {
      expect(createTaskMock).toHaveBeenCalledWith(
        expect.objectContaining({ title: 'Write regression test', parent_id: 't1', project: 'acme-platform' }),
      )
    })
  })

  it('hides subtask creation without task:write', async () => {
    renderAsMember(<TaskDetail task={task} onClose={() => undefined} />, ['task:read'])

    await waitFor(() => {
      expect(screen.getByText('Reproduce the bug')).toBeInTheDocument()
    })

    expect(screen.queryByLabelText(/new subtask title/i)).not.toBeInTheDocument()
  })
})

describe('TaskDetail — spec links', () => {
  it('calls linkTaskSpec when linking a new spec change', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('team-tasks')).toBeInTheDocument()
    })

    const specInput = screen.getByLabelText(/link spec change/i)
    fireEvent.change(specInput, { target: { value: 'another-change' } })
    fireEvent.submit(specInput.closest('form')!)

    await waitFor(() => {
      expect(linkTaskSpecMock).toHaveBeenCalledWith('t1', 'another-change')
    })
  })

  it('calls unlinkTaskSpec when unlinking an existing spec change', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('team-tasks')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /unlink team-tasks/i }))

    await waitFor(() => {
      expect(unlinkTaskSpecMock).toHaveBeenCalledWith('t1', 'team-tasks')
    })
  })

  it('hides spec-link editing without task:write', async () => {
    renderAsMember(<TaskDetail task={task} onClose={() => undefined} />, ['task:read'])

    await waitFor(() => {
      expect(screen.getByText('team-tasks')).toBeInTheDocument()
    })

    expect(screen.queryByLabelText(/link spec change/i)).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /unlink team-tasks/i })).not.toBeInTheDocument()
  })
})

describe('TaskDetail — assignee editing', () => {
  it('calls assignTask when selecting a user and clicking Add', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Sarah Chen')).toBeInTheDocument()
    })

    const assigneeSelect = screen.getByRole('button', { name: /^assignee$/i })
    fireEvent.click(assigneeSelect)
    const rajOption = await screen.findByRole('option', { name: /raj patel/i })
    fireEvent.click(rajOption)

    const addButton = screen.getByRole('button', { name: /^add assignee$/i })
    expect(addButton).not.toBeDisabled()
    fireEvent.click(addButton)

    await waitFor(() => {
      expect(assignTaskMock).toHaveBeenCalledWith('t1', ['user-2'])
    })
  })

  it('does not assign immediately on select — only after clicking Add', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Sarah Chen')).toBeInTheDocument()
    })

    const assigneeSelect = screen.getByRole('button', { name: /^assignee$/i })
    fireEvent.click(assigneeSelect)
    const rajOption = await screen.findByRole('option', { name: /raj patel/i })
    fireEvent.click(rajOption)

    expect(assignTaskMock).not.toHaveBeenCalled()
  })

  it('calls unassignTask when removing an existing assignee', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Sarah Chen')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByRole('button', { name: /unassign sarah chen/i }))

    await waitFor(() => {
      expect(unassignTaskMock).toHaveBeenCalledWith('t1', 'user-1')
    })
  })

  it('hides assignee editing without task:assign', async () => {
    renderAsMember(<TaskDetail task={task} onClose={() => undefined} />, ['task:read', 'task:write'])

    await waitFor(() => {
      expect(screen.getByText('Sarah Chen')).toBeInTheDocument()
    })

    expect(screen.queryByRole('button', { name: /^assignee$/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /unassign sarah chen/i })).not.toBeInTheDocument()
  })
})

describe('TaskDetail — hydrated task via getTask', () => {
  it('fetches the full task via getTask and renders assignees/labels from it, not the list item', async () => {
    const listItem: Task = { ...task, assignees: [], labels: [] }
    renderWithProviders(<TaskDetail task={listItem} onClose={() => undefined} />)

    await waitFor(() => {
      expect(getTaskMock).toHaveBeenCalledWith('t1')
    })

    await waitFor(() => {
      expect(screen.getByText('Sarah Chen')).toBeInTheDocument()
    })
    expect(screen.getByText('bug')).toBeInTheDocument()
  })

  it('does not crash when getTask has not resolved yet and the list item has no assignees/labels', async () => {
    getTaskMock.mockImplementation(() => new Promise(() => undefined))
    const listItem: Task = { ...task, assignees: [], labels: [] }
    renderWithProviders(<TaskDetail task={listItem} onClose={() => undefined} />)

    await waitFor(() => {
      expect(screen.getByText('Unassigned')).toBeInTheDocument()
    })
  })

  it('re-renders assignees after assigning because the getTask query is invalidated', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(getTaskMock).toHaveBeenCalledTimes(1)
    })

    const assigneeSelect = screen.getByRole('button', { name: /^assignee$/i })
    fireEvent.click(assigneeSelect)
    const rajOption = await screen.findByRole('option', { name: /raj patel/i })
    fireEvent.click(rajOption)
    fireEvent.click(screen.getByRole('button', { name: /^add assignee$/i }))

    await waitFor(() => {
      expect(assignTaskMock).toHaveBeenCalledWith('t1', ['user-2'])
    })

    await waitFor(() => {
      expect(getTaskMock).toHaveBeenCalledTimes(2)
    })
  })
})

describe('TaskDetail — editable fields form', () => {
  it('initializes the form from the hydrated task and calls updateTask on Save', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(getTaskMock).toHaveBeenCalledWith('t1')
    })

    const titleInput = await screen.findByLabelText(/^title$/i)
    expect(titleInput).toHaveValue('Fix login redirect bug')

    fireEvent.change(titleInput, { target: { value: 'Fix login redirect bug — updated' } })

    const saveButton = screen.getByRole('button', { name: /^save$/i })
    fireEvent.click(saveButton)

    await waitFor(() => {
      expect(updateTaskMock).toHaveBeenCalledWith(
        't1',
        expect.objectContaining({ title: 'Fix login redirect bug — updated' }),
      )
    })
  })

  it('hides the editable form without task:write and shows read-only fields instead', async () => {
    renderAsMember(<TaskDetail task={task} onClose={() => undefined} />, ['task:read'])

    await waitFor(() => {
      expect(screen.getByText('Fix login redirect bug')).toBeInTheDocument()
    })

    expect(screen.queryByLabelText(/^title$/i)).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^save$/i })).not.toBeInTheDocument()
  })

  // FIX 2: Save previously sat inline between the "Due date" field and the "Assignees"
  // section (top of the modal). It now renders in a footer after Comments (the last
  // section) and submits the edit form via the HTML5 form-association attribute
  // (`form="task-edit-form"`) rather than being nested inside the <form> itself.
  it('renders the Save button after the Comments section, not inside the edit fields form', async () => {
    renderWithProviders(<TaskDetail task={task} onClose={() => undefined} />)

    await waitFor(() => {
      expect(getTaskMock).toHaveBeenCalledWith('t1')
    })
    await screen.findByLabelText(/^title$/i)

    const saveButton = screen.getByRole('button', { name: /^save$/i })
    const commentsHeading = screen.getByText('Comments')

    // DOM order: Comments heading must precede the Save button.
    expect(
      commentsHeading.compareDocumentPosition(saveButton) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()

    // Save must NOT be inside the edit-fields <form> — it's associated via `form=`.
    expect(saveButton.closest('form')).toBeNull()
    expect(saveButton).toHaveAttribute('form', 'task-edit-form')
  })
})
