import { describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, useLocation } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import App from './App'
import { AuthContext } from './auth/AuthContext'
import type { AuthSession } from './types'

// The only-context build, without rebuilding.
vi.mock('./config/disabled-sections', async () => {
  const actual = await vi.importActual<typeof import('./config/disabled-sections')>('./config/disabled-sections')
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

// Every page renders in its loading state: the API client answers with
// promises that never settle, so no page can throw on a shape it did not get.
vi.mock('./api/client', () => ({
  createClient: () => new Proxy({}, { get: () => () => new Promise(() => undefined) }),
}))

// Hoisted: vi.mock factories run before module-level code.
const { session } = vi.hoisted(() => {
  const session: AuthSession = {
    org: { id: 'org-test-1', name: 'Test Org', slug: 'test-org', created_at: '2026-01-01T00:00:00Z' },
    user: {
      id: 'user-1',
      org_id: 'org-test-1',
      email: 'u@test.com',
      name: 'Test User',
      role: 'super_user',
      status: 'active',
      created_at: '2026-01-01T00:00:00Z',
      permissions: ['task:read', 'sdd:read', 'harness:read', 'audit:read', 'migration:read'],
    },
  }
  return { session }
})

// A signed-in super user: the strongest session the panel knows, so a blocked
// route is blocked by the profile and by nothing else.
vi.mock('./auth/AuthContext', async () => {
  const actual = await vi.importActual<typeof import('./auth/AuthContext')>('./auth/AuthContext')
  const value = { session, loading: false, setSession: () => undefined, logout: () => undefined }
  return {
    ...actual,
    AuthProvider: ({ children }: { children: ReactNode }) => (
      <actual.AuthContext.Provider value={value}>{children}</actual.AuthContext.Provider>
    ),
    useAuth: () => value,
  }
})

function LocationProbe() {
  const location = useLocation()
  return <div data-testid="location">{location.pathname}</div>
}

function renderAt(path: string) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <MemoryRouter initialEntries={[path]} future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
      <QueryClientProvider client={queryClient}>
        <LocationProbe />
        <App />
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

// Silence the unused-import check: AuthContext is what the mock above provides.
void AuthContext

describe('App — only-context profile blocks hidden routes', () => {
  it.each(['/tasks', '/sdd', '/harnesses', '/audit', '/backups', '/usage', '/collections', '/autonomous-agents'])(
    'typing %s by hand lands on the dashboard',
    async (path) => {
      renderAt(path)
      await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent(/^\/$/))
    },
  )

  it.each(['/migrations', '/memories', '/graph', '/clients', '/roles'])(
    'a kept section such as %s stays reachable',
    async (path) => {
      renderAt(path)
      await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent(path))
    },
  )
})
