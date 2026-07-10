import { describe, it, expect } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import TasksBoard from './TasksBoard'
import type { Task } from '../../types'

const tasks: Task[] = [
  {
    id: 't1',
    org_id: 'org-test-1',
    project: 'acme-platform',
    title: 'Fix login redirect bug',
    description: null,
    status: 'in_progress',
    priority: 'high',
    due_date: null,
    parent_id: null,
    sprint_id: null,
    created_by: 'user-admin-1',
    created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z',
    archived_at: null,
    assignees: [],
    labels: [],
    comment_count: 0,
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
  {
    id: 't3',
    org_id: 'org-test-1',
    project: 'acme-platform',
    title: 'Ship release notes',
    description: null,
    status: 'done',
    priority: 'low',
    due_date: null,
    parent_id: null,
    sprint_id: null,
    created_by: 'user-admin-1',
    created_at: '2026-07-03T00:00:00Z',
    updated_at: '2026-07-03T00:00:00Z',
    archived_at: null,
    assignees: [],
    labels: [],
    comment_count: 0,
    spec_links: [],
    subtask_count: 0,
  },
  {
    id: 't4',
    org_id: 'org-test-1',
    project: 'acme-platform',
    title: 'Retire legacy export job',
    description: null,
    status: 'cancelled',
    priority: 'low',
    due_date: null,
    parent_id: null,
    sprint_id: null,
    created_by: 'user-admin-1',
    created_at: '2026-07-04T00:00:00Z',
    updated_at: '2026-07-04T00:00:00Z',
    archived_at: null,
    assignees: [],
    labels: [],
    comment_count: 0,
    spec_links: [],
    subtask_count: 0,
  },
]

describe('TasksBoard — columns by status', () => {
  it('renders one column per board status', () => {
    render(<TasksBoard tasks={tasks} onTaskClick={() => undefined} />)

    expect(screen.getByText('Backlog')).toBeInTheDocument()
    expect(screen.getByText('To Do')).toBeInTheDocument()
    expect(screen.getByText('In Progress')).toBeInTheDocument()
    expect(screen.getByText('In Review')).toBeInTheDocument()
    expect(screen.getByText('Done')).toBeInTheDocument()
    expect(screen.getByText('Cancelled')).toBeInTheDocument()
  })

  it('renders a cancelled task under the Cancelled column instead of dropping it', () => {
    render(<TasksBoard tasks={tasks} onTaskClick={() => undefined} />)

    const cancelledColumn = screen.getByTestId('board-column-cancelled')
    expect(within(cancelledColumn).getByText('Retire legacy export job')).toBeInTheDocument()
  })

  it('places each task card under its status column', () => {
    render(<TasksBoard tasks={tasks} onTaskClick={() => undefined} />)

    const backlogColumn = screen.getByTestId('board-column-backlog')
    expect(within(backlogColumn).getByText('Write onboarding docs')).toBeInTheDocument()

    const inProgressColumn = screen.getByTestId('board-column-in_progress')
    expect(within(inProgressColumn).getByText('Fix login redirect bug')).toBeInTheDocument()

    const doneColumn = screen.getByTestId('board-column-done')
    expect(within(doneColumn).getByText('Ship release notes')).toBeInTheDocument()
  })

  it('invokes onTaskClick when a card is clicked', () => {
    let clicked: Task | null = null
    render(<TasksBoard tasks={tasks} onTaskClick={(t) => { clicked = t }} />)

    screen.getByText('Fix login redirect bug').click()

    expect(clicked).not.toBeNull()
    expect((clicked as unknown as Task)?.id).toBe('t1')
  })

  it('renders an empty-column message when a status has no tasks', () => {
    render(<TasksBoard tasks={[]} onTaskClick={() => undefined} />)

    const backlogColumn = screen.getByTestId('board-column-backlog')
    expect(within(backlogColumn).getByText(/no tasks/i)).toBeInTheDocument()
  })
})
