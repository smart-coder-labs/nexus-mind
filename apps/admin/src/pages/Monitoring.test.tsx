import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import Monitoring, { pushEndpoint, setupCommands } from './Monitoring'
import type { MonitorGroup, MonitorSource } from '../types'

const api = vi.hoisted(() => ({
  listMonitorSources: vi.fn(),
  listMonitorGroups: vi.fn(),
  listAutonomousAgentConnectors: vi.fn(),
  createMonitorSource: vi.fn(),
  updateMonitorSource: vi.fn(),
  deleteMonitorSource: vi.fn(),
  testMonitorSource: vi.fn(),
  setMonitorGroupStatus: vi.fn(),
}))
vi.mock('../api/client', () => ({ createClient: () => api }))
vi.mock('../auth/AuthContext', () => ({ useAuth: () => ({ session: { user: { permissions: ['settings:write'] } } }) }))

const source: MonitorSource = {
  id: 'src-1',
  org_id: 'o1',
  project: 'kasymir',
  name: 'Kasymir beta',
  service_account: 'nexusmind-push@my-proj-1.iam.gserviceaccount.com',
  slack_connector_id: null,
  github_connector_id: 'gh-1',
  github_repo: 'acme/app',
  throttle_minutes: 30,
  enabled: true,
  created_at: '2026-11-20',
  audience: 'nexusmind-monitor:src-1',
}
const group: MonitorGroup = {
  id: 'g1',
  source_id: 'src-1',
  service: 'sales',
  title: 'Order <n> not found',
  severity: 'ERROR',
  count: 12,
  first_seen: '2026-11-20T10:00:00+00:00',
  last_seen: '2026-11-20T10:05:00+00:00',
  sample: '{"message":"Order 42 not found"}',
  log_url: 'https://console.cloud.google.com/logs/query;query=x?project=my-proj-1',
  issue_number: 7,
  status: 'open',
}

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter><Monitoring /></MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('setupCommands', () => {
  it('fills the project, endpoint, account and audience', () => {
    const text = setupCommands(source, 'my-proj-1', 'https://api.example.com/v1/monitor/gcp/src-1')
    expect(text).toContain('gcloud pubsub topics create nexusmind-errors --project=my-proj-1')
    expect(text).toContain("--log-filter='severity>=ERROR AND NOT logName:\"cloudaudit.googleapis.com\"'")
    expect(text).toContain('--push-endpoint=https://api.example.com/v1/monitor/gcp/src-1')
    expect(text).toContain('--push-auth-service-account=nexusmind-push@my-proj-1.iam.gserviceaccount.com')
    expect(text).toContain('--push-auth-token-audience=nexusmind-monitor:src-1')
    expect(text).toContain('gcloud iam service-accounts create nexusmind-push --project=my-proj-1')
  })

  it('never pastes an invalid project id into a shell command', () => {
    const text = setupCommands(source, 'x; rm -rf /', 'https://api.example.com/e')
    expect(text).not.toContain('rm -rf')
    expect(text).toContain('--project=PROJECT_ID')
  })

  it('builds the push endpoint from the API base', () => {
    expect(pushEndpoint('src-1', 'https://api.example.com/')).toBe('https://api.example.com/v1/monitor/gcp/src-1')
  })
})

describe('Monitoring page', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.listMonitorSources.mockResolvedValue([source])
    api.listMonitorGroups.mockResolvedValue([group])
    api.listAutonomousAgentConnectors.mockResolvedValue([])
    api.setMonitorGroupStatus.mockResolvedValue(undefined)
  })

  it('lists error groups with issue and log links, and resolves one', async () => {
    renderPage()
    expect(await screen.findByText('Order <n> not found')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Issue #7' })).toHaveAttribute('href', 'https://github.com/acme/app/issues/7')
    await userEvent.click(screen.getByRole('button', { name: 'Resolve Order <n> not found' }))
    expect(api.setMonitorGroupStatus).toHaveBeenCalledWith('g1', 'resolved')
  })

  it('shows the setup commands for a source', async () => {
    renderPage()
    await userEvent.click(await screen.findByRole('button', { name: 'Setup commands for Kasymir beta' }))
    await userEvent.type(screen.getByLabelText('GCP project ID'), 'my-proj-1')
    expect(screen.getByLabelText('Setup commands').textContent).toContain('--project=my-proj-1')
  })
})
