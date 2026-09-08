import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../auth/AuthContext'
import { Layout } from './Layout'
import type { AuthSession } from '../types'

// The only-context build, without rebuilding: the config module is swapped for
// the same catalog resolved for that profile.
vi.mock('../config/disabled-sections', async () => {
  const actual = await vi.importActual<typeof import('../config/disabled-sections')>('../config/disabled-sections')
  // Every export that closes over the build's sets is re-derived, so a call
  // site using isSectionEnabled/isSectionKeptByProfile sees the cut too.
  const profileHidden = actual.profileHiddenHrefsFor('only-context')
  const disabled = actual.disabledNavHrefsFor('only-context')
  return {
    ...actual,
    ADMIN_PROFILE: 'only-context',
    PROFILE_HIDDEN_HREFS: profileHidden,
    DISABLED_NAV_HREFS: disabled,
    isSectionEnabled: (href: string) => actual.isSectionEnabledIn(href, disabled),
    isSectionKeptByProfile: (href: string) => actual.isSectionEnabledIn(href, profileHidden),
    WEBHOOKS_ENABLED: false,
  }
})

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

// Every permission the nav can ask for, so what is missing is missing because
// of the profile and not because of a permission.
const ALL_PERMISSIONS = [
  'collection:read', 'tag:read', 'convention:read', 'migration:read', 'task:read', 'sdd:read',
  'client:read', 'project:read', 'code:read', 'harness:read', 'autonomous_agent:read',
  'policy:read', 'settings:write', 'audit:read',
]

function renderLayout() {
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
      role: 'admin',
      status: 'active',
      created_at: '2026-01-01T00:00:00Z',
      permissions: ALL_PERMISSIONS,
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

describe('Layout — only-context profile', () => {
  it('shows_exactly_the_context_sections_to_a_fully_permitted_admin', async () => {
    renderLayout()

    await waitFor(() => {
      expect(screen.getAllByRole('link', { name: /^memories$/i }).length).toBeGreaterThan(0)
    })

    const kept = ['Search', 'Dashboard', 'Memories', 'Graph', 'Tags', 'Conventions', 'Migration',
      'Clients', 'Projects', 'Code', 'Users', 'Roles', 'Settings']
    for (const label of kept) {
      expect(screen.getAllByRole('link', { name: new RegExp(`^${label}$`, 'i') }).length, label).toBeGreaterThan(0)
    }

    const hidden = ['Usage', 'Collections', 'Sessions', 'Tasks', 'SDD', 'Harnesses', 'Automation',
      'API Keys', 'Agent identities', 'Policies', 'Webhooks', 'Audit Log', 'Backups']
    for (const label of hidden) {
      expect(screen.queryByRole('link', { name: new RegExp(`^${label}$`, 'i') }), label).not.toBeInTheDocument()
    }
  })

  it('keeps_a_group_heading_only_while_it_still_has_an_item', async () => {
    renderLayout()

    await waitFor(() => {
      expect(screen.getAllByRole('link', { name: /^users$/i }).length).toBeGreaterThan(0)
    })

    // Access keeps Users + Roles, System keeps Settings — both headings stay.
    expect(screen.getAllByText('Access').length).toBeGreaterThan(0)
    expect(screen.getAllByText('System').length).toBeGreaterThan(0)
  })
})
