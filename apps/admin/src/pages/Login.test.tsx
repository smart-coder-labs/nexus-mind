import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AuthContext } from '../auth/AuthContext'
import Login from './Login'

// Mock the client module
vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    getMe: vi.fn().mockResolvedValue({
      org: { id: 'org-1', name: 'Test', slug: 'test', created_at: '2026-01-01T00:00:00Z' },
      user: { id: 'u-1', org_id: 'org-1', email: 'admin@test.com', name: 'Admin', role: 'admin', status: 'active', created_at: '2026-01-01T00:00:00Z' },
    }),
  })),
  loginWithEmail: vi.fn(),
  loginWithApiKey: vi.fn(),
}))

// Mock react-router-dom navigate
const mockNavigate = vi.fn()
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  }
})

import * as clientModule from '../api/client'

function renderLogin() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const mockSetSession = vi.fn()

  render(
    <MemoryRouter>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider
          value={{ session: null, loading: false, setSession: mockSetSession, logout: vi.fn() }}
        >
          <Login />
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )

  return { mockSetSession }
}

describe('Login', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ;(clientModule.loginWithEmail as ReturnType<typeof vi.fn>).mockResolvedValue({
      org: { id: 'org-1', name: 'Test', slug: 'test', created_at: '2026-01-01T00:00:00Z' },
      user: { id: 'u-1', org_id: 'org-1', email: 'admin@test.com', name: 'Admin', role: 'admin', status: 'active', created_at: '2026-01-01T00:00:00Z' },
    })
  })

  it('renders login form and fills email + password, submit calls loginWithEmail with entered credentials', async () => {
    renderLogin()

    const emailInput = screen.getByPlaceholderText('admin@company.com')
    const passwordInput = screen.getByPlaceholderText('••••••••')
    const submitButton = screen.getByRole('button', { name: /sign in/i })

    expect(emailInput).toBeInTheDocument()
    expect(passwordInput).toBeInTheDocument()
    expect(submitButton).toBeInTheDocument()

    fireEvent.change(emailInput, { target: { value: 'test@example.com' } })
    fireEvent.change(passwordInput, { target: { value: 'secret123' } })
    fireEvent.click(submitButton)

    await waitFor(() => {
      expect(clientModule.loginWithEmail).toHaveBeenCalledWith('test@example.com', 'secret123')
    })
  })
})
