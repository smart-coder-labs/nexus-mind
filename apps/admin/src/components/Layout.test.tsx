import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../auth/AuthContext'
import { Layout } from './Layout'
import type { AuthSession } from '../types'

const { getNotificationsMock, getOrgSettingsMock, globalSearchMock } = vi.hoisted(() => ({
  getNotificationsMock: vi.fn(),
  getOrgSettingsMock: vi.fn(),
  globalSearchMock: vi.fn(),
}))

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    getNotifications: getNotificationsMock,
    getOrgSettings: getOrgSettingsMock,
    globalSearch: globalSearchMock,
  })),
}))

/** Permissions are authoritative even for built-in admin users. */
function renderLayout(role: 'admin' | 'member', permissions: string[] | null) {
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
      role,
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
          <Layout><div>page</div></Layout>
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

beforeEach(() => {
  vi.clearAllMocks()
  getNotificationsMock.mockResolvedValue([])
  getOrgSettingsMock.mockResolvedValue({})
  globalSearchMock.mockResolvedValue({ memories: [], users: [], projects: [], policies: [], conventions: [], sdd_changes: [] })
})

describe('Layout — SDD nav entry', () => {
  it('nav_item_sdd_visible_with_sdd_read', async () => {
    renderLayout('member', ['sdd:read'])

    await waitFor(() => {
      expect(screen.getAllByRole('link', { name: /^sdd$/i }).length).toBeGreaterThan(0)
    })

    const link = screen.getAllByRole('link', { name: /^sdd$/i })[0]
    expect(link).toHaveAttribute('href', '/sdd')
  })

  it('nav_item_sdd_sits_in_the_knowledge_group', async () => {
    renderLayout('admin', ['sdd:read'])

    await waitFor(() => {
      expect(screen.getAllByRole('link', { name: /^sdd$/i }).length).toBeGreaterThan(0)
    })

    // The group heading and the item share a container (the group <div>).
    const heading = screen.getAllByText('Knowledge')[0]
    const group = heading.parentElement as HTMLElement
    expect(within(group).getByRole('link', { name: /^sdd$/i })).toBeInTheDocument()
  })

  it('nav_item_sdd_hidden_without_sdd_read', async () => {
    renderLayout('member', ['task:read'])

    await waitFor(() => {
      expect(screen.getAllByRole('link', { name: /^tasks$/i }).length).toBeGreaterThan(0)
    })

    expect(screen.queryByRole('link', { name: /^sdd$/i })).not.toBeInTheDocument()
  })
})
