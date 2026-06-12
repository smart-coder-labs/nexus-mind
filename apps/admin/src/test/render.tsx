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

export function renderWithProviders(ui: ReactElement): RenderResult {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })

  return render(
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider
          value={{
            session: mockAdminSession,
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
