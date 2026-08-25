import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AuthContext } from '../auth/AuthContext'
import type { AuthSession } from '../types'
import AutonomousAgents from './AutonomousAgents'

const api = vi.hoisted(() => ({
  listAutonomousAgentTemplates: vi.fn(),
  listAutonomousAgents: vi.fn(),
  getAutonomousRuntimeHealth: vi.fn(),
  getAutonomousAgentSettings: vi.fn(),
  getAutonomousAgentMetrics: vi.fn(),
}))

vi.mock('../api/client', () => ({ createClient: () => api }))

function renderPage(permissions: string[]) {
  const session: AuthSession = {
    org: { id: 'o1', name: 'Acme', slug: 'acme', created_at: '2026-01-01' },
    user: {
      id: 'u1', org_id: 'o1', email: 'admin@acme.test', name: 'Admin', role: 'member',
      status: 'active', created_at: '2026-01-01', permissions,
    },
  }
  return render(
    <MemoryRouter>
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <AuthContext.Provider value={{ session, loading: false, setSession: () => undefined, logout: () => undefined }}>
          <AutonomousAgents />
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

describe('AutonomousAgents permission and runtime states', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.listAutonomousAgentTemplates.mockResolvedValue([])
    api.listAutonomousAgents.mockResolvedValue([])
    api.getAutonomousRuntimeHealth.mockResolvedValue({ status: 'reauth_required', reason_code: 'claude_auth_required' })
    api.getAutonomousAgentSettings.mockResolvedValue({ enabled: true, retention_days: 90 })
    api.getAutonomousAgentMetrics.mockResolvedValue({ queued_runs: 2 })
  })

  it('does not infer access from the role name', () => {
    renderPage([])
    expect(screen.queryByText('Autonomous agents')).not.toBeInTheDocument()
  })

  it('shows durable reauthentication guidance to a custom permitted role', async () => {
    renderPage(['autonomous_agent:read'])
    expect(await screen.findByText('Autonomous agents')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /create agent/i })).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Runtime' }))
    expect(await screen.findByText(/authenticate claude code again/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /check again/i })).not.toBeInTheDocument()
  }, 20_000)
})
