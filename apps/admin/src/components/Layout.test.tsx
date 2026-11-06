import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
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

    const toggle = screen.getByRole('button', { name: 'Knowledge' })
    const contentId = toggle.getAttribute('aria-controls')
    const sddLink = screen.getByRole('link', { name: /^sdd$/i })
    expect(contentId).toBeTruthy()
    expect(document.getElementById(contentId!)?.contains(sddLink)).toBe(true)
  })

  it('nav_item_sdd_hidden_without_sdd_read', async () => {
    renderLayout('member', ['task:read'])

    await waitFor(() => {
      expect(screen.getAllByRole('link', { name: /^tasks$/i }).length).toBeGreaterThan(0)
    })

    expect(screen.queryByRole('link', { name: /^sdd$/i })).not.toBeInTheDocument()
  })

  it('toggles navigation groups and their links accessibly', async () => {
    renderLayout('admin', ['sdd:read'])

    const knowledge = screen.getByRole('button', { name: 'Knowledge' })
    expect(knowledge).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByRole('link', { name: /^sdd$/i })).toBeInTheDocument()

    fireEvent.click(knowledge)
    expect(knowledge).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByRole('link', { name: /^sdd$/i })).not.toBeInTheDocument()

    fireEvent.click(knowledge)
    expect(knowledge).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByRole('link', { name: /^sdd$/i })).toBeInTheDocument()
  })

  it('places the announcement in page flow and the theme control in the sidebar footer', async () => {
    getOrgSettingsMock.mockResolvedValue({ announcement: 'A new feature is available', announcement_type: 'info' })
    renderLayout('admin', ['user:read', 'sdd:read'])

    const announcement = await screen.findByRole('status', { name: 'Organization announcement' })
    expect(announcement.closest('[data-testid="announcement-placement"]')).toBeInTheDocument()
    expect(announcement).toHaveClass('border-l-[3px]', 'bg-action-primary/5')
    expect(screen.getByRole('region', { name: 'Page content' })).toContainElement(announcement)
    expect(screen.queryByRole('banner')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /search…/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Open account menu' })).not.toBeInTheDocument()
    const themeButton = screen.getByRole('button', { name: /^Switch to (light|dark) mode$/ })
    expect(document.querySelector('.audit-sidebar')).toContainElement(themeButton)
    expect(screen.getByRole('button', { name: 'Keyboard shortcuts' })).toBeInTheDocument()
  })
})
