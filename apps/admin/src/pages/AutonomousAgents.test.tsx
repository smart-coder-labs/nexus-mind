import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AuthContext } from '../auth/AuthContext'
import type { AuthSession } from '../types'
import AutonomousAgents, { type AutonomousAgentsSection } from './AutonomousAgents'

const api = vi.hoisted(() => ({
  listAutonomousAgentTemplates: vi.fn(),
  listAutonomousAgents: vi.fn(),
  getAutonomousRuntimeHealth: vi.fn(),
  getAutonomousAgentSettings: vi.fn(),
  getAutonomousAgentMetrics: vi.fn(),
  listAutonomousAgentRuns: vi.fn(),
  getFactoryBot: vi.fn(),
  patchAutonomousAgentSettings: vi.fn(),
}))

vi.mock('../api/client', () => ({ createClient: () => api }))

function renderPage(permissions: string[], section?: AutonomousAgentsSection) {
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
          <AutonomousAgents section={section} />
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
    api.listAutonomousAgentRuns.mockResolvedValue([])
    api.getFactoryBot.mockResolvedValue({ bot: null })
  })

  it('does not infer access from the role name', () => {
    renderPage([])
    expect(screen.queryByRole('heading', { name: 'Agents' })).not.toBeInTheDocument()
    expect(api.listAutonomousAgents).not.toHaveBeenCalled()
  })

  it('shows the agents page without create rights to a custom permitted role', async () => {
    renderPage(['autonomous_agent:read'])
    expect(await screen.findByRole('heading', { name: 'Agents', level: 1 })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new agent/i })).not.toBeInTheDocument()
    // The route is the switcher: no in-page tab bar.
    expect(screen.queryByRole('button', { name: 'Runtime' })).not.toBeInTheDocument()
    // The fleet strip reports the runtime state in words and links to settings.
    expect(await screen.findByText('Needs re-authentication')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Factory settings' })).toHaveAttribute('href', '/factory/settings')
  }, 20_000)

  it('shows durable reauthentication guidance on the settings page to a custom permitted role', async () => {
    renderPage(['autonomous_agent:read'], 'runtime')
    expect(await screen.findByRole('heading', { name: 'Factory settings', level: 1 })).toBeInTheDocument()
    expect(await screen.findByText(/authenticate claude code again/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /check again/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /pause all agents/i })).not.toBeInTheDocument()
  }, 20_000)

  it('confirms before pausing every agent', async () => {
    api.patchAutonomousAgentSettings.mockResolvedValue({ enabled: false, retention_days: 90 })
    renderPage(['autonomous_agent:read', 'autonomous_agent:enable'], 'runtime')
    await userEvent.click(await screen.findByRole('button', { name: 'Pause all agents' }))
    expect(api.patchAutonomousAgentSettings).not.toHaveBeenCalled()
    const dialog = await screen.findByRole('dialog', { name: /pause all agents\?/i })
    await userEvent.click(within(dialog).getByRole('button', { name: 'Pause all agents' }))
    expect(api.patchAutonomousAgentSettings).toHaveBeenCalledWith({ enabled: false })
  }, 20_000)
})
