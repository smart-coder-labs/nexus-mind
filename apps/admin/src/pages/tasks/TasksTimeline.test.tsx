import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import TasksTimeline from './TasksTimeline'
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
    due_date: '2026-07-20',
    parent_id: null,
    sprint_id: null,
    created_by: 'user-admin-1',
    created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z',
    archived_at: null,
    assignees: [{ id: 'user-1', name: 'Sarah Chen', email: 'sarah@acme.test' }],
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
    due_date: '2026-07-10',
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
]

describe('TasksTimeline — groups tasks by due date', () => {
  it('renders a date group header for each distinct due date, earliest first', () => {
    render(<TasksTimeline tasks={tasks} onTaskClick={() => undefined} />)

    const headers = screen.getAllByText(/2026/i).map(el => el.textContent)
    const shipIndex = headers.findIndex(h => h?.includes('Jul 10'))
    const fixIndex = headers.findIndex(h => h?.includes('Jul 20'))
    expect(shipIndex).toBeGreaterThanOrEqual(0)
    expect(fixIndex).toBeGreaterThan(shipIndex)
  })

  it('groups tasks with no due date under a trailing "No due date" bucket', () => {
    render(<TasksTimeline tasks={tasks} onTaskClick={() => undefined} />)

    expect(screen.getByText('No due date')).toBeInTheDocument()
    expect(screen.getByText('Write onboarding docs')).toBeInTheDocument()
  })

  it('invokes onTaskClick when a row is clicked', () => {
    let clicked: Task | null = null
    render(<TasksTimeline tasks={tasks} onTaskClick={(t) => { clicked = t }} />)

    screen.getByText('Fix login redirect bug').click()

    expect(clicked).not.toBeNull()
    expect((clicked as unknown as Task)?.id).toBe('t1')
  })

  it('renders an empty state when there are no tasks', () => {
    render(<TasksTimeline tasks={[]} onTaskClick={() => undefined} />)

    expect(screen.getByText('No tasks')).toBeInTheDocument()
  })
})
