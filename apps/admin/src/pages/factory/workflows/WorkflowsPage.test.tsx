import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import WorkflowsPage from './WorkflowsPage'
import { deliveryWorkflow, preparationWorkflow, type Workspace } from './types'

const api = vi.hoisted(() => ({ getWorkflowCatalog: vi.fn(), confirmFactoryWorkspace: vi.fn(), registerFactoryRunner: vi.fn() }))
const auth = vi.hoisted(() => ({ permissions: [] as string[] }))
vi.mock('../../../api/client', () => ({ createClient: () => api }))
vi.mock('../../../auth/AuthContext', () => ({ useAuth: () => ({ session: { user: { permissions: auth.permissions } } }) }))
const workspace: Workspace = { id: 'w', runner_id: 'r', name: 'Kasymir', root_path: '/work/kasymir', discovered: [{ path: 'api', remote: '', head_sha: 'a'.repeat(40), writable: false }], repositories: [], inventory_revision: 1, confirmed_revision: null }
function show(permissions: string[]) {
  auth.permissions = permissions
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><WorkflowsPage /></QueryClientProvider>)
}
beforeEach(() => {
  vi.clearAllMocks()
  api.getWorkflowCatalog.mockResolvedValue({ runners: [{ id: 'r', name: 'Local', revoked: false, capabilities: [], last_seen: null }], workspaces: [workspace], definitions: [], runs: [] })
  api.confirmFactoryWorkspace.mockResolvedValue(workspace)
})
describe('workspace workflows', () => {
  it('requires explicit read permission without making API calls', () => {
    show([])
    expect(screen.getByRole('alert')).toHaveTextContent('factory_policy:read')
    expect(api.getWorkflowCatalog).not.toHaveBeenCalled()
  })
  it('shows the folder and isolation requirements, read-only without write permission', async () => {
    show(['factory_policy:read'])
    expect(await screen.findByText('/work/kasymir')).toBeInTheDocument()
    expect(screen.getByRole('note')).toHaveTextContent('Missing adapters wait visibly')
    expect(screen.getByLabelText('Allow implementation writes')).toBeDisabled()
    expect(screen.queryByRole('button', { name: 'Register runner' })).not.toBeInTheDocument()
  })
  it('defaults discovered repositories to read-only and confirms exact revision', async () => {
    show(['factory_policy:read', 'factory_policy:write'])
    const user = userEvent.setup()
    const writable = await screen.findByLabelText('Allow implementation writes')
    expect(writable).not.toBeChecked()
    await user.click(screen.getByRole('button', { name: 'Confirm selected repositories' }))
    await waitFor(() => expect(api.confirmFactoryWorkspace).toHaveBeenCalledWith('w', 1, workspace.discovered))
  })
  it('hides a newly issued credential after acknowledgment', async () => {
    api.registerFactoryRunner.mockResolvedValue({ id: 'new', token: 'nmr_secret' })
    show(['factory_policy:read', 'factory_policy:write'])
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Runner name'), 'Laptop')
    await user.click(screen.getByRole('button', { name: 'Register runner' }))
    expect(await screen.findByText('nmr_secret')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /I saved it/ }))
    expect(screen.queryByText('nmr_secret')).not.toBeInTheDocument()
  })
  it('makes independent review read-only and QA parallel after environment setup', () => {
    const w = { ...workspace, repositories: [{ ...workspace.discovered[0], writable: true }] }
    const d = deliveryWorkflow(w)
    expect(d.steps.find(s => s.id === 'review')?.write_repositories).toEqual([])
    expect(d.steps.find(s => s.id === 'gate')?.depends_on).toEqual(['functional', 'design', 'security'])
    expect(d.steps.find(s => s.id === 'implement')?.write_repositories).toEqual(['api'])
    expect(preparationWorkflow(w).steps).toHaveLength(1)
  })
})
