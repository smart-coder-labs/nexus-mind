import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, expect, it, vi } from 'vitest'
import RunsPage from './RunsPage'

const { data } = vi.hoisted(() => ({ data: { factory: null as any } }))
vi.mock('../shared/ui', async importOriginal => ({ ...await importOriginal<any>(), useFactory: () => data.factory }))
vi.mock('./RunDetail', () => ({ RunDetail: () => <div>Fixture selected run detail</div> }))
afterEach(cleanup)

it('shows each inventory scenario last-tested timestamp in retained QA history', async () => {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const client = {
    listAutonomousAgents: vi.fn().mockResolvedValue([{ id: 'qa1', name: 'Project QA', template_key: 'qa' }]),
    getAutonomousAgent: vi.fn().mockResolvedValue({ revision: { config: { project_id: 'p1' } } }),
    listProjects: vi.fn().mockResolvedValue([{ id: 'p1', name: 'Current project' }]),
    listAutonomousAgentQACoverage: vi.fn().mockResolvedValue({ scenarios: [], checkpoints: [], plans: [], inventory: [{ module: 'auth', key: 'login', description: 'Login works', last_status: 'passed', last_tested_at: '2026-10-01 12:34:56', blocker_changed: false }], inventory_summary: [] }),
    listAutonomousAgentRuns: vi.fn().mockResolvedValue([]),
    listAutonomousAgentRunEvents: vi.fn().mockResolvedValue([]),
    listRunVerificationReports: vi.fn().mockResolvedValue({ reports: [] }),
    listAutonomousRunQACoverage: vi.fn().mockResolvedValue({ scenarios: [], plans: [] }),
    listAutonomousAgentRunTranscript: vi.fn().mockResolvedValue([]),
  }
  data.factory = { can: () => false, client, queryClient, invalidate: vi.fn() }
  render(<QueryClientProvider client={queryClient}><MemoryRouter initialEntries={['/factory/runs']}><RunsPage /></MemoryRouter></QueryClientProvider>)
  await screen.findByRole('option', { name: 'Project QA' })
  fireEvent.change(screen.getByLabelText('Retained QA history'), { target: { value: 'qa1' } })
  expect(await screen.findByText(/last tested 2026-10-01 12:34:56/)).toBeTruthy()
})
