import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, expect, it, vi } from 'vitest'
import Wizard from './AutonomousAgentWizard'

const { fixture } = vi.hoisted(() => ({ fixture: { client: null as any } }))
vi.mock('../api/client', () => ({ createClient: () => fixture.client }))
vi.mock('../auth/AuthContext', () => ({ useAuth: () => ({ session: null }) }))
vi.mock('./factory/AgentRuntimeEditor', () => ({ AgentRuntimeEditor: () => null, runtimeKinds: { claude: [] } }))
vi.mock('../components/ui/Select/StyledSelect', () => ({ StyledSelect: (props: any) => <select {...props} /> }))
afterEach(cleanup)

function fixtureWizard(editing?: any) {
  const client = {
    listProjects: vi.fn().mockResolvedValue([{ id: 'p', name: 'QA Project' }]),
    listQaProjectRepositories: vi.fn().mockResolvedValue({ candidates: ['acme/web', 'acme/api', 'acme/index-only'], grants: [{ repository: 'acme/web', can_read: true }, { repository: 'acme/api', can_read: true }, { repository: 'acme/index-only', can_read: false }, { repository: 'foreign/repo', can_read: true }] }),
    createAutonomousAgent: vi.fn().mockResolvedValue({ id: 'a' }),
    updateAutonomousAgent: vi.fn().mockResolvedValue({ id: 'a' }),
    putAutonomousAgentTarget: vi.fn().mockResolvedValue({}),
    putAutonomousAgentSchedule: vi.fn().mockResolvedValue({}),
    getAutonomousAgentSchedule: vi.fn().mockResolvedValue({ kind: 'manual' }),
    listAutonomousAgentTargets: vi.fn().mockResolvedValue([{ kind: 'web_application', name: 'App', enabled: true, config: { url: 'https://app.invalid' } }]),
  }
  fixture.client = client
  const query = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={query}><Wizard open onClose={() => {}} editing={editing} templates={[{ key: 'qa', name: 'QA', description: 'QA', capabilities: [], default_budgets: {}, config_schema: {}, workflow: [] } as any]} /></QueryClientProvider>)
  return client
}
const next = () => fireEvent.click(screen.getByRole('button', { name: 'Continue' }))

it('names each context checkbox and preserves editable diagnostic routing', async () => {
  const client = fixtureWizard({ id: 'a', name: 'QA edit', template_key: 'qa', revision: { config: { project_id: 'p', context_repos: ['acme/api'], outputs: ['nexusmind'] }, budgets: {} } })
  next()
  expect(await screen.findByRole('checkbox', { name: 'acme/api' })).toBeChecked()
  expect(screen.getByRole('checkbox', { name: 'acme/web' })).not.toBeChecked()
  expect(screen.queryByRole('checkbox', { name: 'acme/index-only' })).toBeNull()
  expect(screen.queryByRole('checkbox', { name: 'foreign/repo' })).toBeNull()
  fireEvent.change(screen.getByLabelText(/^Test adapter/), { target: { value: 'allowlisted_command' } })
  const repository = screen.getByLabelText(/^Repository to check out/)
  expect([...repository.querySelectorAll('option')].map(option => (option as HTMLOptionElement).value)).toEqual(['', 'acme/api', 'acme/web'])
  fireEvent.change(repository, { target: { value: 'acme/api' } })
  fireEvent.change(screen.getByPlaceholderText('npx playwright test'), { target: { value: 'npm test' } })
  next()
  next()
  fireEvent.click(screen.getByRole('button', { name: 'Save revision' }))
  await waitFor(() => expect(client.updateAutonomousAgent).toHaveBeenCalled())
  expect(client.updateAutonomousAgent.mock.calls[0][1].config).toMatchObject({ project_id: 'p', repository: 'acme/api', context_repos: ['acme/api'], test_commands: [['npm', 'test']] })
})

it('creates project and URL without a primary repo while preserving selected diagnostic repositories', async () => {
  const client = fixtureWizard()
  next()
  fireEvent.change(screen.getByPlaceholderText('https://staging.example.com'), { target: { value: 'https://app.invalid' } })
  next()
  await screen.findByRole('option', { name: 'QA Project' })
  fireEvent.change(screen.getByLabelText(/^Project/), { target: { value: 'p' } })
  fireEvent.click(await screen.findByRole('checkbox', { name: 'acme/api' }))
  fireEvent.click(screen.getByRole('checkbox', { name: 'acme/web' }))
  next()
  next()
  fireEvent.change(screen.getByPlaceholderText('Nightly QA'), { target: { value: 'Browser QA' } })
  fireEvent.click(screen.getByRole('button', { name: 'Create (disabled)' }))
  await waitFor(() => expect(client.createAutonomousAgent).toHaveBeenCalled())
  const config = client.createAutonomousAgent.mock.calls[0][0].config
  expect(config).toMatchObject({ project_id: 'p', context_repos: ['acme/api', 'acme/web'] })
  expect(config.repository).toBeUndefined()
  expect(client.putAutonomousAgentTarget).toHaveBeenCalledWith('a', expect.objectContaining({ config: expect.objectContaining({ url: 'https://app.invalid' }) }))
})
