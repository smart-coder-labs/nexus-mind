import { type ReactElement } from 'react'
import { render, type RenderResult } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../auth/AuthContext'
import type { AuthSession } from '../types'

const mockAdminSession: AuthSession = {
  org: {
    id: 'org-test-1',
    name: 'Test Org',
    slug: 'test-org',
    created_at: '2026-01-01T00:00:00Z',
  },
  user: {
    id: 'user-admin-1',
    org_id: 'org-test-1',
    email: 'admin@test.com',
    name: 'Test Admin',
    role: 'admin',
    status: 'active',
    created_at: '2026-01-01T00:00:00Z',
  },
}

export interface RenderOptions {
  /**
   * Permissions carried by the mocked session's user. Defaults to none, so a
   * permission-gated feature stays hidden unless a test opts into it —
   * mirroring how `/me` returns permissions derived from the user's role.
   */
  permissions?: string[]
}

export function renderWithProviders(
  ui: ReactElement,
  { permissions }: RenderOptions = {},
): RenderResult {
  const session: AuthSession = permissions
    ? { ...mockAdminSession, user: { ...mockAdminSession.user, permissions } }
    : mockAdminSession

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })

  return render(
    <MemoryRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider
          value={{
            session,
            loading: false,
            setSession: () => undefined,
            logout: () => undefined,
          }}
        >
          {ui}
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}
