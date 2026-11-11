import { beforeEach, describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import FactoryRuns from './FactoryRuns'
import FactoryHome from './FactoryHome'
import FactoryInputs from './FactoryInputs'
import { WorkflowConnectorForm } from './workflows/WorkflowConnectorForm'
const api = vi.hoisted(() => ({
  getWorkflowCatalog: vi.fn(),
  getFactoryWorkflowRun: vi.fn(),
  listAutonomousAgents: vi.fn(),
  listAutonomousAgentConnectors: vi.fn(),
  createWorkflowIntake: vi.fn(),
  putAutonomousAgentConnector: vi.fn(),
  controlFactoryWorkflowRun: vi.fn(),
  approveWorkflowAction: vi.fn(),
}))
const auth = vi.hoisted(() => ({ permissions: [] as string[] }))
vi.mock('../../api/client', () => ({ createClient: () => api }))
vi.mock('../../auth/AuthContext', () => ({
  useAuth: () => ({ session: { user: { permissions: auth.permissions } } }),
}))
const repo = {
  path: 'web',
  remote: 'https://github.com/acme/web.git',
  head_sha: 'a'.repeat(40),
  writable: true,
}
const workspace = {
  id: 'w',
  runner_id: 'runner',
  name: 'Kasymir',
  root_path: '/work/kasymir',
  discovered: [repo],
  repositories: [repo],
  inventory_revision: 1,
  confirmed_revision: 1,
}
const definition = {
  name: 'Preparation',
  workspace_id: 'w',
  steps: [
    {
      id: 'prepare',
      name: 'Prepare',
      kind: 'prepare',
      adapter: 'prepare-workspace',
      depends_on: [],
      write_repositories: [],
      max_attempts: 2,
      timeout_seconds: 60,
    },
  ],
}
const run = {
  id: 'r',
  status: 'blocked',
  definition_id: 'd',
  source_key: 'github:acme/web#42',
  created_at: '2026-01-01',
  snapshot: { workspace, definition },
  steps: [],
  events: [],
}
function show(element: React.ReactNode, permissions = ['factory_policy:read'], url = '/factory') {
  auth.permissions = permissions
  render(
    <MemoryRouter initialEntries={[url]}>
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        {element}
      </QueryClientProvider>
    </MemoryRouter>,
  )
}
beforeEach(() => {
  vi.clearAllMocks()
  api.getWorkflowCatalog.mockResolvedValue({
    runners: [],
    workspaces: [workspace],
    definitions: [{ id: 'd', definition }],
    runs: [run],
    pending_runs: [],
    factory_enabled: true,
  })
  api.getFactoryWorkflowRun.mockResolvedValue(run)
  api.listAutonomousAgents.mockResolvedValue([])
  api.listAutonomousAgentConnectors.mockResolvedValue([
    {
      id: 'connector',
      kind: 'target_secret',
      health: 'healthy',
      name: 'GitHub issues reader',
      secret_configured: true,
      scopes: ['target:use', 'github:read'],
      metadata: {
        purpose: 'factory_workflow_intake',
        source_kind: 'github',
        repositories: ['acme/web'],
      },
    },
  ])
  api.createWorkflowIntake.mockResolvedValue({ id: 'intake' })
})
describe('connected Factory operation', () => {
  it('offers only authorized entry paths to an agent-only viewer', async () => {
    show(<FactoryHome />, ['autonomous_agent:read'])
    expect(await screen.findByRole('link', { name: /Abrir agentes/ })).toHaveAttribute(
      'href',
      '/factory/agents',
    )
    expect(api.getWorkflowCatalog).not.toHaveBeenCalled()
    expect(screen.queryByRole('link', { name: 'Conectar workspace' })).not.toBeInTheDocument()
  })
  it('does not call a discovered workspace QA-ready without applied runtime profiles', async () => {
    show(<FactoryHome />)
    expect(
      await screen.findByText(
        '1 workspaces · 0 con perfiles aplicados · Inicio de trabajo habilitado',
      ),
    ).toBeInTheDocument()
    expect(screen.getByText('Configuración pendiente', { exact: false })).toBeInTheDocument()
  })
  it('opens a workflow from the unified run history and keeps approvals read-only', async () => {
    show(<FactoryRuns />, ['factory_policy:read'], '/factory/runs')
    const user = userEvent.setup()
    await user.click(await screen.findByRole('link', { name: /Preparation/ }))
    expect(await screen.findByRole('region', { name: 'Workflow seleccionado' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Autorizar implementación' })).toBeDisabled()
    expect(api.approveWorkflowAction).not.toHaveBeenCalled()
  })
  it('creates a GitHub Issues source using an authorized repo, filters and a dedicated connector', async () => {
    show(
      <FactoryInputs />,
      ['factory_policy:read', 'factory_policy:write', 'autonomous_agent:read'],
      '/factory/intake',
    )
    const user = userEvent.setup()
    await user.selectOptions(await screen.findByLabelText('Workflow destino'), 'd')
    await user.selectOptions(screen.getByLabelText('Aplicación'), 'github')
    await user.click(screen.getByRole('checkbox', { name: 'acme/web' }))
    await user.selectOptions(screen.getByLabelText('Conector dedicado'), 'connector')
    await user.type(
      screen.getByLabelText('Labels adicionales (separadas por coma)'),
      'bug, frontend',
    )
    await user.click(screen.getByRole('button', { name: 'Conectar entrada' }))
    await waitFor(() =>
      expect(api.createWorkflowIntake).toHaveBeenCalledWith({
        definition_id: 'd',
        kind: 'github',
        connector_id: 'connector',
        config: { repositories: ['acme/web'], labels: ['bug', 'frontend'] },
        enabled: false,
      }),
    )
    expect(screen.getByText(/Solo issues abiertos con label factory/)).toBeInTheDocument()
  })
  it('clears a new connector credential even when creation fails', async () => {
    api.putAutonomousAgentConnector.mockRejectedValue(new Error('Conector rechazado'))
    show(
      <WorkflowConnectorForm
        kind="github"
        repositories={['acme/web']}
        onCreated={vi.fn()}
        onCancel={vi.fn()}
      />,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Nombre del conector'), 'Issues')
    const input = screen.getByLabelText('Token dedicado')
    await user.type(input, 'fixture-private-token')
    await user.click(screen.getByRole('button', { name: 'Crear conector cifrado' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Conector rechazado')
    expect(input).toHaveValue('')
    expect(api.putAutonomousAgentConnector).toHaveBeenCalledWith(
      expect.objectContaining({
        metadata: {
          purpose: 'factory_workflow_intake',
          source_kind: 'github',
          repositories: ['acme/web'],
        },
        scopes: ['target:use', 'github:read'],
      }),
    )
  })
})
