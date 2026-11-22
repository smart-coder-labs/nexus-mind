import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import Monitoring, { drainEndpoint, drainSteps, pushEndpoint, setupCommands, setupSteps, suggestedAccount } from './Monitoring'
import type { MonitorGroup, MonitorSource } from '../types'
import { chooseSelectOption } from '../test/select'

const api = vi.hoisted(() => ({
  getMonitorOptions: vi.fn(),
  listMonitorSources: vi.fn(),
  listMonitorGroups: vi.fn(),
  listAutonomousAgentConnectors: vi.fn(),
  createMonitorSource: vi.fn(),
  updateMonitorSource: vi.fn(),
  deleteMonitorSource: vi.fn(),
  setMonitorSourceEnabled: vi.fn(),
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
  slack_channel: null,
  github_connector_id: 'gh-1',
  github_repo: 'acme/app',
  throttle_minutes: 30,
  enabled: true,
  created_at: '2026-11-20',
  audience: 'nexusmind-monitor:src-1',
  audit_executor: null,
  audit_model: null,
  audit_agent_id: null,
  provider: 'gcp',
  provider_ref: null,
  autofix: false,
  autofix_agent_id: null,
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
  audit_run_id: null,
  audit_status: null,
  audit_summary: null,
  issue_repo: null,
  slack_ts: null,
  fix_run_id: null,
  fix_status: null,
  fix_pr_url: null,
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

  it('numbers the steps and keeps the service account step only for its own project', () => {
    const own = setupSteps(source, 'my-proj-1', 'https://e')
    expect(own.map(step => step.title)).toContain('Create the push service account')
    expect(own.at(-2)?.command).toContain('gcloud logging write nexusmind-test')
    const other = setupSteps(source, 'another-proj', 'https://e')
    expect(other.map(step => step.title)).not.toContain('Create the push service account')
    expect(setupCommands(source, 'my-proj-1', 'https://e')).toMatch(/^# 1\. Sign in and pick the project/)
  })

  it('suggests the push account from a valid project id only', () => {
    expect(suggestedAccount('my-proj-1')).toBe('nexusmind-push@my-proj-1.iam.gserviceaccount.com')
    expect(suggestedAccount('Bad Project')).toBe('')
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
    api.getMonitorOptions.mockResolvedValue({ server_slack_mcp: false, server_github: false })
  })

  it('lists errors as terminal lines that open to every detail, and resolves one', async () => {
    renderPage()
    const line = await screen.findByText('Order <n> not found')
    const errors = screen.getByRole('list', { name: 'Errors' })
    expect(errors.textContent).toContain('ERROR')
    expect(errors.textContent).toContain('×12')
    expect(errors.textContent).toContain('[open]')
    await userEvent.click(line)
    expect(screen.getByText('Order 42 not found')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Issue #7' })).toHaveAttribute('href', 'https://github.com/acme/app/issues/7')
    await userEvent.click(screen.getByRole('button', { name: 'Resolve Order <n> not found' }))
    expect(api.setMonitorGroupStatus).toHaveBeenCalledWith('g1', 'resolved')
  })

  it('shows the audit conclusion, the routed issue and the audit run', async () => {
    api.listMonitorGroups.mockResolvedValue([{
      ...group,
      issue_repo: 'acme/locations',
      audit_run_id: 'run-9',
      audit_status: 'done',
      audit_summary: 'The occupancy endpoint times out on the locations service',
    }])
    renderPage()
    await userEvent.click(await screen.findByText('Order <n> not found'))
    expect(screen.getByText(/The occupancy endpoint times out on the locations service/)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'locations #7' })).toHaveAttribute('href', 'https://github.com/acme/locations/issues/7')
    expect(screen.getByRole('link', { name: 'audit run' })).toHaveAttribute('href', '/factory/runs?agent_run=run-9')
  })

  it('sends the audit executor and model only with GitHub issues on', async () => {
    api.listMonitorSources.mockResolvedValue([])
    api.updateMonitorSource.mockResolvedValue(source)
    api.listMonitorSources.mockResolvedValue([source])
    renderPage()
    await userEvent.click(await screen.findByRole('button', { name: 'Edit Kasymir beta' }))
    await userEvent.click(screen.getByLabelText('Audit new errors before opening the issue'))
    expect(screen.getByLabelText('Default repository for issues')).toHaveValue('acme/app')
    await chooseSelectOption(userEvent.setup(), screen.getByLabelText('Task executor'), /Codex/)
    await userEvent.type(screen.getByLabelText('Model (optional)'), 'gpt-5.5')
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.updateMonitorSource).toHaveBeenCalledWith('src-1', expect.objectContaining({
      github_connector_id: 'gh-1',
      audit_executor: 'codex',
      audit_model: 'gpt-5.5',
    }))
  })

  it('pauses and resumes a project from its card', async () => {
    api.setMonitorSourceEnabled.mockResolvedValue({ ...source, enabled: false })
    renderPage()
    await userEvent.click(await screen.findByRole('button', { name: 'Pause Kasymir beta' }))
    expect(api.setMonitorSourceEnabled).toHaveBeenCalledWith('src-1', false)
  })

  it('opens the setup steps with the project taken from the service account', async () => {
    renderPage()
    await userEvent.click(await screen.findByRole('button', { name: 'Setup commands for Kasymir beta' }))
    expect(screen.getByLabelText('GCP project ID')).toHaveValue('my-proj-1')
    const steps = screen.getByRole('list', { name: 'Setup steps' })
    expect(steps.textContent).toContain('1. Sign in and pick the project')
    expect(steps.textContent).toContain('--push-auth-token-audience=nexusmind-monitor:src-1')
    expect(screen.getByRole('button', { name: 'Copy step 1' })).toBeInTheDocument()
  })

  it('fills the service account from the GCP project and offers server accounts only when allowed', async () => {
    api.listMonitorSources.mockResolvedValue([])
    renderPage()
    await userEvent.click(await screen.findByRole('button', { name: 'Connect project' }))
    expect(screen.getByRole('heading', { name: 'How it works' })).toBeInTheDocument()
    await userEvent.type(screen.getByLabelText('GCP project ID'), 'my-proj-1')
    expect(screen.getByLabelText('Push service account')).toHaveValue('nexusmind-push@my-proj-1.iam.gserviceaccount.com')
    await waitFor(() => expect(api.getMonitorOptions).toHaveBeenCalled())
    const user = userEvent.setup()
    await chooseSelectOption(user, screen.getByLabelText('Slack'), 'No Slack alerts')
    await user.click(screen.getByLabelText('Slack'))
    expect(await screen.findByRole('option', { name: 'No Slack alerts' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: /Server Slack/ })).not.toBeInTheDocument()
  })

  it('sends the channel when Slack goes through the server MCP', async () => {
    api.listMonitorSources.mockResolvedValue([])
    api.getMonitorOptions.mockResolvedValue({ server_slack_mcp: true, server_github: true })
    api.createMonitorSource.mockResolvedValue({ ...source, id: 'src-2' })
    renderPage()
    await userEvent.click(await screen.findByRole('button', { name: 'Connect project' }))
    await userEvent.type(screen.getByLabelText('Name'), 'Beta')
    await userEvent.type(screen.getByLabelText('NexusMind project'), 'kasymir')
    await userEvent.type(screen.getByLabelText('GCP project ID'), 'my-proj-1')
    await waitFor(() => expect(api.getMonitorOptions).toHaveBeenCalled())
    await chooseSelectOption(userEvent.setup(), screen.getByLabelText('Slack'), /Server Slack/)
    await userEvent.type(screen.getByLabelText('Slack channel ID'), 'c0123abcde')
    await chooseSelectOption(userEvent.setup(), screen.getByLabelText('GitHub issues'), /Server GitHub/)
    await userEvent.type(screen.getByLabelText('Repository for issues'), 'acme/app')
    await userEvent.click(screen.getByRole('button', { name: 'Connect' }))
    expect(api.createMonitorSource).toHaveBeenCalledWith(expect.objectContaining({
      service_account: 'nexusmind-push@my-proj-1.iam.gserviceaccount.com',
      slack_connector_id: 'server_slack_mcp',
      slack_channel: 'C0123ABCDE',
      github_connector_id: 'server_gh_cli',
      github_repo: 'acme/app',
    }))
    expect(await screen.findByRole('list', { name: 'Setup steps' })).toBeInTheDocument()
  })

  it('turns auto-fix on only with the server gh, and links the fix PR', async () => {
    const serverSource = { ...source, github_connector_id: 'server_gh_cli' }
    api.listMonitorSources.mockResolvedValue([serverSource])
    api.getMonitorOptions.mockResolvedValue({ server_slack_mcp: true, server_github: true })
    api.listMonitorGroups.mockResolvedValue([{ ...group, fix_status: 'pr_open', fix_run_id: 'run-4', fix_pr_url: 'https://github.com/acme/app/pull/9' }])
    api.updateMonitorSource.mockResolvedValue({ ...serverSource, autofix: true })
    renderPage()
    await userEvent.click(await screen.findByText('Order <n> not found'))
    expect(screen.getByRole('link', { name: 'Fix PR' })).toHaveAttribute('href', 'https://github.com/acme/app/pull/9')
    await userEvent.click(screen.getByRole('button', { name: 'Edit Kasymir beta' }))
    await userEvent.click(screen.getByLabelText('Open a draft PR for each issue'))
    await userEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(api.updateMonitorSource).toHaveBeenCalledWith('src-1', expect.objectContaining({ github_connector_id: 'server_gh_cli', autofix: true }))
  })

  it('connects a Vercel drain and shows its token once, with no GCP fields', async () => {
    api.listMonitorSources.mockResolvedValue([])
    api.createMonitorSource.mockResolvedValue({ ...source, id: 'src-3', provider: 'vercel', provider_ref: 'acme', service_account: '', ingest_token: 'nmd_secret' })
    renderPage()
    await userEvent.click(await screen.findByRole('button', { name: 'Connect project' }))
    await chooseSelectOption(userEvent.setup(), screen.getByLabelText('Errors come from'), /Vercel/)
    expect(screen.queryByLabelText('GCP project ID')).not.toBeInTheDocument()
    await userEvent.type(screen.getByLabelText('Name'), 'Shop')
    await userEvent.type(screen.getByLabelText('NexusMind project'), 'shop')
    await userEvent.type(screen.getByLabelText('Vercel team slug (optional)'), 'acme')
    await userEvent.click(screen.getByRole('button', { name: 'Connect' }))
    expect(api.createMonitorSource).toHaveBeenCalledWith(expect.objectContaining({ provider: 'vercel', provider_ref: 'acme', service_account: '' }))
    const steps = await screen.findByRole('list', { name: 'Setup steps' })
    expect(steps.textContent).toContain('/v1/monitor/drain/src-3')
    expect(steps.textContent).toContain('Authorization: Bearer nmd_secret')
  })
})

describe('drain setup', () => {
  it('builds the drain endpoint and never invents a token', () => {
    expect(drainEndpoint('s1', 'https://api.example.com/')).toBe('https://api.example.com/v1/monitor/drain/s1')
    const steps = drainSteps('supabase', 'https://api.example.com/v1/monitor/drain/s1', undefined)
    expect(steps[0].title).toBe('Add a log drain in Supabase')
    expect(steps[1].command).toBe('Authorization: Bearer TOKEN_SHOWN_WHEN_CONNECTED')
    expect(steps[1].detail).toMatch(/delete the source and connect it again/)
  })
})
