import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, useNavigate } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import WorkflowsPage from './WorkflowsPage'
import { deliveryWorkflow, preparationWorkflow, type Workspace } from './types'
const api = vi.hoisted(() => ({
  getWorkflowCatalog: vi.fn(),
  getWorkflowPublicationIdentity: vi.fn(),
  getFactoryWorkflowRun: vi.fn(),
  confirmFactoryWorkspace: vi.fn(),
  registerFactoryRunner: vi.fn(),
  saveWorkflowRuntime: vi.fn(),
}))
const auth = vi.hoisted(() => ({ permissions: [] as string[] }))
vi.mock('../../../api/client', () => ({ createClient: () => api }))
vi.mock('../../../auth/AuthContext', () => ({
  useAuth: () => ({ session: { user: { permissions: auth.permissions } } }),
}))
const workspace: Workspace = {
  id: 'w',
  runner_id: 'r',
  name: 'Kasymir',
  root_path: '/work/kasymir',
  discovered: [{ path: 'api', remote: '', head_sha: 'a'.repeat(40), writable: false }],
  repositories: [],
  inventory_revision: 1,
  confirmed_revision: null,
}
const definition = preparationWorkflow(workspace)
const detail = {
  id: 'run1',
  status: 'succeeded',
  source_key: 'manual:prueba',
  definition_id: 'd',
  created_at: '2026-01-01',
  task: {},
  snapshot: { workspace, definition },
  steps: [{ id: 'prepare', status: 'succeeded', attempt: 1, result: { success: true } }],
  events: [],
}
function Back() {
  const navigate = useNavigate()
  return <button onClick={() => navigate(-1)}>Atrás del navegador</button>
}
function show(permissions: string[], url = '/factory/workflows') {
  auth.permissions = permissions
  render(
    <MemoryRouter initialEntries={[url]}>
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <Back />
        <WorkflowsPage />
      </QueryClientProvider>
    </MemoryRouter>,
  )
}
beforeEach(() => {
  vi.clearAllMocks()
  api.getWorkflowCatalog.mockResolvedValue({
    runners: [{ id: 'r', name: 'Local', revoked: false, capabilities: [], last_seen: null }],
    workspaces: [workspace],
    definitions: [{ id: 'd', definition }],
    runs: [
      {
        id: 'run1',
        status: 'succeeded',
        source_key: 'manual:prueba',
        definition_id: 'd',
        created_at: '2026-01-01',
      },
    ],
  })
  api.getWorkflowPublicationIdentity.mockResolvedValue({
    enabled: true,
    login: 'test-bot',
    error: null,
  })
  api.getFactoryWorkflowRun.mockResolvedValue(detail)
  api.confirmFactoryWorkspace.mockResolvedValue(workspace)
})
const read = ['factory_policy:read']
const write = [...read, 'factory_policy:write']
describe('guided workspace workflows', () => {
  it('uses the authenticated backend account without requiring a publication connector', async () => {
    const confirmed = {
      ...workspace,
      confirmed_revision: 1,
      repositories: [
        {
          path: 'api',
          remote: 'https://github.com/acme/api.git',
          head_sha: 'a'.repeat(40),
          writable: true,
        },
      ],
    }
    api.getWorkflowCatalog.mockResolvedValue({
      runners: [],
      workspaces: [confirmed],
      definitions: [],
      runs: [],
    })
    show(write, '/factory/workflows?tab=settings&section=delivery')
    const account = await screen.findByRole('combobox', { name: /Cuenta para publicar/ })
    expect(account).toHaveValue('server_gh_cli')
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Crear workflow completo' })).toBeEnabled(),
    )
    expect(
      screen.queryByText(/No hay un conector de publicación autorizado/),
    ).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Crear workflow completo' }))
    expect(screen.getByText('test-bot', { exact: true })).toBeInTheDocument()
  })
  it('requires explicit read permission without API calls', () => {
    show([])
    expect(screen.getByRole('alert')).toHaveTextContent('factory_policy:read')
    expect(api.getWorkflowCatalog).not.toHaveBeenCalled()
  })
  it('shows a useful next step instead of fifty repository cards', async () => {
    show(read)
    expect(await screen.findByText('/work/kasymir')).toBeInTheDocument()
    expect(
      screen.getByRole('heading', { name: 'Completa la configuración del workspace' }),
    ).toBeInTheDocument()
    expect(screen.getByText('Vista de solo lectura.', { exact: false })).toBeInTheDocument()
    expect(screen.queryByLabelText('Escritura')).not.toBeInTheDocument()
  })
  it('keeps repository controls disabled for a read-only operator', async () => {
    show(read, '/factory/workflows?tab=settings&section=repositories')
    expect(await screen.findByLabelText('Escritura')).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Guardar selección y permisos' })).toBeDisabled()
  })
  it('defaults discovered repositories to read-only and confirms the exact revision', async () => {
    show(write, '/factory/workflows?tab=settings&section=repositories')
    const user = userEvent.setup()
    expect(await screen.findByLabelText('Escritura')).not.toBeChecked()
    await user.click(screen.getByRole('button', { name: 'Guardar selección y permisos' }))
    await waitFor(() =>
      expect(api.confirmFactoryWorkspace).toHaveBeenCalledWith('w', 1, workspace.discovered),
    )
  })
  it('hides a newly issued credential after acknowledgment', async () => {
    api.registerFactoryRunner.mockResolvedValue({ id: 'new', token: 'nmr_secret' })
    show(write, '/factory/workflows?tab=connections')
    const user = userEvent.setup()
    await user.type(await screen.findByLabelText('Nombre del runner'), 'Laptop')
    await user.click(screen.getByRole('button', { name: 'Registrar runner' }))
    expect(await screen.findByText('nmr_secret')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Ya la guardé/ }))
    expect(screen.queryByText('nmr_secret')).not.toBeInTheDocument()
  })
  it('opens a real shareable execution link and supports browser back', async () => {
    show(read, '/factory/workflows?tab=runs')
    const user = userEvent.setup()
    const link = await screen.findByRole('link', { name: /manual:prueba/ })
    expect(link).toHaveAttribute('href', '/factory/workflows?tab=runs&run=run1')
    await user.click(link)
    expect(await screen.findByRole('region', { name: 'Detalle de ejecución' })).toBeInTheDocument()
    expect(api.getFactoryWorkflowRun).toHaveBeenCalledWith('run1')
    await user.click(screen.getByRole('button', { name: 'Atrás del navegador' }))
    await waitFor(() =>
      expect(
        screen.queryByRole('region', { name: 'Detalle de ejecución' }),
      ).not.toBeInTheDocument(),
    )
  })
  it('loads a deep-linked execution without clicking its list entry', async () => {
    show(read, '/factory/workflows?run=run1')
    expect(await screen.findByRole('region', { name: 'Detalle de ejecución' })).toBeInTheDocument()
    expect(api.getFactoryWorkflowRun).toHaveBeenCalledWith('run1')
  })
  it('shows pending state and can close the execution before it loads', async () => {
    api.getFactoryWorkflowRun.mockReturnValue(new Promise(() => {}))
    show(read, '/factory/workflows?run=run1')
    expect(await screen.findByText('Cargando ejecución…')).toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Cerrar ejecución' }))
    expect(screen.queryByText('Cargando ejecución…')).not.toBeInTheDocument()
  })
  it('reports detail failures with an explicit retry', async () => {
    api.getFactoryWorkflowRun
      .mockRejectedValueOnce(new Error('No se pudo abrir la ejecución.'))
      .mockResolvedValue(detail)
    show(read, '/factory/workflows?run=run1')
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('No se pudo abrir')
    await userEvent.setup().click(within(alert).getByRole('button'))
    expect(await screen.findByRole('region', { name: 'Detalle de ejecución' })).toBeInTheDocument()
  })
  it('does not offer full execution before runtime configuration is ready', async () => {
    show(write, '/factory/workflows?tab=runs')
    expect(await screen.findByText(/Completa agentes, QA y servicios/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Iniciar ejecución' })).toBeDisabled()
  })
  it('explains same-author review limitations without promising a GitHub self-approval', async () => {
    show(read)
    expect(await screen.findByText(/El autor no puede aprobarse a sí mismo/)).toBeInTheDocument()
  })
  it('reviews a definition without duplicate builders and can discard it safely', async () => {
    api.getWorkflowCatalog.mockResolvedValue({
      runners: [
        {
          id: 'r',
          name: 'Oracle',
          revoked: false,
          capabilities: ['adapter:prepare-workspace'],
          last_seen: null,
        },
      ],
      workspaces: [{ ...workspace, confirmed_revision: 1, repositories: workspace.discovered }],
      definitions: [],
      runs: [],
    })
    show(write)
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Probar solo la preparación' }))
    expect(await screen.findByLabelText('Nombre del workflow')).toBeInTheDocument()
    expect(
      screen.queryByRole('navigation', { name: 'Pasos de configuración' }),
    ).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Descartar' }))
    expect(
      await screen.findByRole('button', { name: 'Crear prueba de preparación' }),
    ).toBeInTheDocument()
  })
  it('keeps review read-only and QA parallel after environment setup', () => {
    const w = { ...workspace, repositories: [{ ...workspace.discovered[0], writable: true }] }
    const d = deliveryWorkflow(w)
    expect(d.steps.find((s) => s.id === 'review')?.write_repositories).toEqual([])
    expect(d.steps.find((s) => s.id === 'gate')?.depends_on).toEqual([
      'functional',
      'design',
      'security',
    ])
    expect(d.steps.find((s) => s.id === 'implement')?.write_repositories).toEqual(['api'])
    expect(preparationWorkflow(w).steps).toHaveLength(1)
  })
})
