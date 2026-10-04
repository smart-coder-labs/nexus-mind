import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, within, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import FactoryIntake from './FactoryIntake'
import type { FactoryIntakeItem, FactoryIntakeSource } from '../types'

const api = vi.hoisted(() => ({
  listFactoryIntakeSources: vi.fn(),
  listFactoryIntakeItems: vi.fn(),
  getFactoryWatchdog: vi.fn(),
  listAutonomousAgentConnectors: vi.fn(),
  listAutonomousAgents: vi.fn(),
  putAutonomousAgentConnector: vi.fn(),
  createFactoryIntakeSource: vi.fn(),
  updateFactoryIntakeSource: vi.fn(),
  setFactoryIntakeSourceEnabled: vi.fn(),
  deleteFactoryIntakeSource: vi.fn(),
  putFactoryWatchdog: vi.fn(),
}))
vi.mock('../api/client', () => ({ createClient: () => api }))

const auth = vi.hoisted(() => ({ permissions: [] as string[] }))
vi.mock('../auth/AuthContext', () => ({
  useAuth: () => ({ session: { user: { permissions: auth.permissions } } }),
}))

const source: FactoryIntakeSource = {
  id: 's1',
  kind: 'slack',
  name: 'Bugs channel',
  project: 'app',
  resolver_definition_id: 'def-1',
  repository: 'acme/app',
  base_ref: 'main',
  privacy_class: 'internal',
  connector_id: 'conn-secret',
  config: { channel_id: 'C0123ABCD', allowed_reactors: ['ULEAD0001'] },
  enabled: true,
  last_polled_at: '2026-10-04 10:00:00',
  last_error: 'slack_error:not_in_channel',
  created_by: 'u1',
  created_at: '2026-10-04',
  updated_at: '2026-10-04',
}
const items: FactoryIntakeItem[] = [
  {
    task_id: 't1', source_id: 's1', source_ref: 'C0123ABCD:1.1', nexus_task_id: 'n1', task_class: 'docs', origin_trust: 'untrusted',
    repository: 'acme/app', start_decision: 'started', start_reason: 'decision_model_allow', jev: null, issue_number: 12, run_id: 'r1', created_at: '2026-10-04 10:00:00',
  },
  {
    task_id: 't2', source_id: 's1', source_ref: 'C0123ABCD:2.2', nexus_task_id: 'n2', task_class: 'security', origin_trust: 'untrusted',
    repository: 'acme/app', start_decision: 'backlog', start_reason: 'class_not_auto_startable:security', jev: null, issue_number: null, run_id: null, created_at: '2026-10-04 11:00:00',
  },
]

function renderPage(permissions: string[]) {
  auth.permissions = permissions
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter><FactoryIntake /></MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('FactoryIntake', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.listFactoryIntakeSources.mockResolvedValue([source])
    api.listFactoryIntakeItems.mockResolvedValue(items)
    api.getFactoryWatchdog.mockResolvedValue(null)
    api.listAutonomousAgentConnectors.mockResolvedValue([
      { id: 'conn-secret', kind: 'target_secret', name: 'intake: Bugs', health: 'ready', metadata: { purpose: 'factory_intake', source_kind: 'slack' } },
      { id: 'conn-qa', kind: 'target_secret', name: 'QA login', health: 'ready', metadata: {} },
      { id: 'conn-hook', kind: 'slack', name: 'Team channel', health: 'ready', metadata: {} },
    ])
    api.listAutonomousAgents.mockResolvedValue([
      { id: 'def-1', name: 'App resolver', template_key: 'github_issue_resolver', status: 'enabled' },
      { id: 'def-2', name: 'Reviewer', template_key: 'github_pr_reviewer', status: 'enabled' },
    ])
    api.putAutonomousAgentConnector.mockResolvedValue({ id: 'conn-new' })
    api.createFactoryIntakeSource.mockResolvedValue(source)
    api.updateFactoryIntakeSource.mockResolvedValue(source)
    api.setFactoryIntakeSourceEnabled.mockResolvedValue({ ...source, enabled: false })
    api.putFactoryWatchdog.mockResolvedValue({ slack_connector_id: 'conn-hook', enabled: true, daily_hour_utc: 13, last_sent_at: null, last_daily_on: null })
  })

  it('shows sources, their last error and what came in, read-only without write access', async () => {
    renderPage(['factory_policy:read'])
    const sources = await screen.findByRole('region', { name: 'Sources' })
    expect(within(sources).getByText('acme/app')).toBeInTheDocument()
    expect(within(sources).getByText(/last poll failed: slack_error:not_in_channel/i)).toBeInTheDocument()
    expect(within(sources).queryByRole('button')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Add source' })).not.toBeInTheDocument()
    const table = await screen.findByRole('table', { name: 'Intake items' })
    expect(within(table).getByRole('link', { name: /issue #12/ })).toHaveAttribute('href', 'https://github.com/acme/app/issues/12')
    expect(within(table).getByText(/class_not_auto_startable:security/)).toBeInTheDocument()
  })

  it('creates a Slack source with a new token stored as a secret connector', async () => {
    api.listFactoryIntakeSources.mockResolvedValue([])
    renderPage(['factory_policy:read', 'factory_policy:write', 'autonomous_agent:manage_connectors'])
    const user = userEvent.setup()
    await screen.findByText(/no intake sources yet/i)
    await waitFor(() => expect(api.listAutonomousAgents).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Add source' }))
    const save = screen.getByRole('button', { name: 'Save source' })
    expect(save).toBeDisabled()
    await user.type(screen.getByLabelText('Name'), 'Bugs')
    await user.type(screen.getByLabelText('NexusMind project for the tasks'), 'app')
    await user.selectOptions(screen.getByLabelText(/issue resolver/i), 'def-1')
    expect(within(screen.getByLabelText(/issue resolver/i)).queryByRole('option', { name: 'Reviewer' })).not.toBeInTheDocument()
    await user.type(screen.getByLabelText('Channel ID'), 'C0123ABCD')
    await user.type(screen.getByLabelText(/who can send a message/i), 'ULEAD0001, UOPS00002')
    expect(within(screen.getByLabelText('Token')).queryByRole('option', { name: 'QA login' })).not.toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText('Token'), '__new__')
    expect(save).toBeDisabled()
    await user.type(screen.getByLabelText(/slack bot token/i), 'xoxb-secret')
    await user.click(save)
    await waitFor(() => expect(api.createFactoryIntakeSource).toHaveBeenCalled())
    expect(api.putAutonomousAgentConnector).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'target_secret', secret: 'xoxb-secret', scopes: ['target:use'], metadata: { purpose: 'factory_intake', source_kind: 'slack' },
    }))
    expect(api.createFactoryIntakeSource).toHaveBeenCalledWith(expect.objectContaining({
      kind: 'slack',
      connector_id: 'conn-new',
      resolver_definition_id: 'def-1',
      config: { channel_id: 'C0123ABCD', allowed_reactors: ['ULEAD0001', 'UOPS00002'] },
    }))
  })

  it('pauses a source and says when saving fails', async () => {
    api.setFactoryIntakeSourceEnabled.mockRejectedValueOnce(new Error('connector_id must be a secret connector'))
    renderPage(['factory_policy:read', 'factory_policy:write'])
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Pause Bugs channel' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('connector_id must be a secret connector')
    expect(api.setFactoryIntakeSourceEnabled).toHaveBeenCalledWith('s1', false)
  })

  it('offers no new token without the permission to manage connectors', async () => {
    api.listFactoryIntakeSources.mockResolvedValue([])
    renderPage(['factory_policy:read', 'factory_policy:write'])
    const user = userEvent.setup()
    await screen.findByText(/no intake sources yet/i)
    await waitFor(() => expect(api.listAutonomousAgentConnectors).toHaveBeenCalled())
    await user.click(screen.getByRole('button', { name: 'Add source' }))
    const token = screen.getByLabelText('Token')
    expect(within(token).queryByRole('option', { name: 'Paste a new token' })).not.toBeInTheDocument()
    expect(screen.queryByLabelText(/slack bot token/i)).not.toBeInTheDocument()
  })

  it('turns Slack notifications on with a Slack webhook connector', async () => {
    renderPage(['factory_policy:read', 'factory_policy:write'])
    const user = userEvent.setup()
    const card = await screen.findByRole('region', { name: 'Slack notifications' })
    await waitFor(() => expect(within(card).getByRole('option', { name: 'Team channel' })).toBeInTheDocument())
    expect(within(card).queryByRole('option', { name: 'intake: Bugs' })).not.toBeInTheDocument()
    expect(within(card).getByRole('button', { name: 'Turn on' })).toBeDisabled()
    await user.selectOptions(within(card).getByLabelText('Slack webhook'), 'conn-hook')
    await user.click(within(card).getByRole('button', { name: 'Turn on' }))
    await waitFor(() => expect(api.putFactoryWatchdog).toHaveBeenCalledWith({ slack_connector_id: 'conn-hook', enabled: true, daily_hour_utc: 13 }))
  })
})
