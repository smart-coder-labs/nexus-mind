import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AuthContext } from '../../../auth/AuthContext'
import type { AuthSession, AutonomousAgentEvent, AutonomousAgentRun } from '../../../types'
import RunsPage from './RunsPage'

const api = vi.hoisted(() => ({
  listAutonomousAgents: vi.fn(), listAutonomousAgentRuns: vi.fn(), listAutonomousAgentRunEvents: vi.fn(),
  listAutonomousAgentRunTranscript: vi.fn(), listRunVerificationReports: vi.fn(),
}))
vi.mock('../../../api/client', () => ({ createClient: () => api }))
vi.mock('./RunDetail', () => ({ RunDetail: ({ events }: { events: AutonomousAgentEvent[] }) => <p>Loaded lifecycle events: {events.length}</p> }))

const run: AutonomousAgentRun = {
  id: 'r1', definition_id: 'a1', revision_id: 'v1', trigger_kind: 'manual', occurrence_key: 'manual-1',
  scheduled_for: null, snapshot_sha: null, status: 'running', budget: {},
  started_at: '2026-10-31T04:00:00Z', finished_at: null, created_at: '2026-10-31T04:00:00Z', archived_at: null,
}
const event = (sequence: number): AutonomousAgentEvent => ({ sequence, kind: 'model.selected', payload: { model: 'gpt-6.1-sol' }, created_at: '2026-10-31T04:00:00Z' })

function renderPage() {
  const session: AuthSession = {
    org: { id: 'o1', name: 'Acme', slug: 'acme', created_at: '2026-01-01' },
    user: { id: 'u1', org_id: 'o1', email: 'test@acme.test', name: 'Test', role: 'member', status: 'active', created_at: '2026-01-01', permissions: ['autonomous_agent:read'] },
  }
  render(<MemoryRouter initialEntries={[{ pathname: '/factory/runs', state: { runId: 'r1' } }]}>
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <AuthContext.Provider value={{ session, loading: false, setSession: () => undefined, logout: () => undefined }}><RunsPage /></AuthContext.Provider>
    </QueryClientProvider>
  </MemoryRouter>)
}

beforeEach(() => {
  vi.clearAllMocks()
  api.listAutonomousAgents.mockResolvedValue([])
  api.listAutonomousAgentRuns.mockResolvedValue([run])
  api.listAutonomousAgentRunEvents.mockResolvedValue([event(1)])
  api.listAutonomousAgentRunTranscript.mockResolvedValue([])
  api.listRunVerificationReports.mockResolvedValue({ reports: [] })
})

describe('run log fetching', () => {
  it('refreshes lifecycle logs while the run is still running', async () => {
    api.listAutonomousAgentRunEvents.mockResolvedValueOnce([event(1)]).mockResolvedValue([event(1), event(2)])
    renderPage()
    expect(await screen.findByText('Loaded lifecycle events: 1')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('Loaded lifecycle events: 2')).toBeInTheDocument(), { timeout: 4500 })
    expect(api.listAutonomousAgentRunEvents).toHaveBeenCalledWith('r1')
  }, 7000)

  it('shows fetch failures instead of silently presenting empty logs', async () => {
    api.listAutonomousAgentRunEvents.mockRejectedValue(new Error('Unavailable'))
    api.listAutonomousAgentRunTranscript.mockRejectedValue(new Error('Unavailable'))
    renderPage()
    expect(await screen.findByText('Could not load the event log.')).toBeInTheDocument()
    expect(await screen.findByText('Could not load conversation and tool logs.')).toBeInTheDocument()
  })
})
