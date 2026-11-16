import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { GithubReviewHooks } from './GithubReviewHooks'

const api = vi.hoisted(() => ({ getGithubReviewHooks: vi.fn(), listAutonomousAgents: vi.fn(), registerGithubReviewHook: vi.fn(), disconnectGithubReviewHook: vi.fn(), revokeAutonomousAgentConnector: vi.fn() }))
const auth = vi.hoisted(() => ({ permissions: [] as string[] }))
vi.mock('../../../api/client', () => ({ createClient: () => api }))
vi.mock('../../../auth/AuthContext', () => ({ useAuth: () => ({ session: { user: { permissions: auth.permissions } } }) }))
const writePermissions = ['autonomous_agent:read', 'factory_policy:write', 'autonomous_agent:manage_connectors']
function show(permissions: string[]) {
  auth.permissions = permissions
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><GithubReviewHooks /></QueryClientProvider>)
}
const hook = { id: 'hook', health: 'ready', management_credential_id: 'credential', metadata: { repository: 'acme/api', callback_url: 'https://api.example.com/hook' } }
beforeEach(() => {
  vi.resetAllMocks()
  api.getGithubReviewHooks.mockResolvedValue({ login: 'cli-account', hooks: [] })
  api.listAutonomousAgents.mockResolvedValue([{ id: 'reviewer', name: 'API reviewer', template_key: 'github_pr_reviewer', status: 'enabled', validation_status: 'valid' }])
  api.registerGithubReviewHook.mockResolvedValue({ id: 'hook', login: 'cli-account' })
  vi.spyOn(window, 'confirm').mockReturnValue(true)
})
describe('GitHub requested review hooks', () => {
  it('does not contact the CLI without permission', () => {
    show([])
    expect(api.getGithubReviewHooks).not.toHaveBeenCalled()
  })
  it('does not expose credential controls to read-only users', async () => {
    show(['autonomous_agent:read'])
    expect(await screen.findByText('cli-account')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Connect GitHub webhook' })).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Webhook administration token (optional)')).not.toBeInTheDocument()
  })
  it('sends a write-only setup token and clears it after registration', async () => {
    show(writePermissions)
    const user = userEvent.setup()
    await screen.findByText('cli-account')
    await user.selectOptions(screen.getByLabelText('Enabled PR reviewer agent'), 'reviewer')
    await user.clear(screen.getByLabelText('Public HTTPS backend URL'))
    await user.type(screen.getByLabelText('Public HTTPS backend URL'), 'https://api.example.com')
    const token = screen.getByLabelText('Webhook administration token (optional)')
    expect(token).toHaveAttribute('type', 'password')
    await user.type(token, 'github_pat_setup')
    await user.click(screen.getByRole('button', { name: 'Connect GitHub webhook' }))
    await waitFor(() => expect(api.registerGithubReviewHook).toHaveBeenCalledWith('reviewer', 'https://api.example.com', 'github_pat_setup'))
    await waitFor(() => expect(token).toHaveValue(''))
  })
  it('allows retrying remote deletion after local revocation with a replacement token', async () => {
    api.getGithubReviewHooks.mockResolvedValue({ login: 'cli-account', hooks: [{ ...hook, health: 'revoked', metadata: { ...hook.metadata, remote_deleted: false } }] })
    api.disconnectGithubReviewHook.mockResolvedValue({ remote_deleted: false, cleanup_pending: true })
    show(writePermissions)
    const user = userEvent.setup()
    await screen.findByText('Disconnected locally; GitHub deletion is pending.')
    await user.type(screen.getByLabelText('Webhook administration token (optional)'), 'replacement')
    await user.click(screen.getByRole('button', { name: 'Retry GitHub deletion' }))
    await waitFor(() => expect(api.disconnectGithubReviewHook).toHaveBeenCalledWith('hook', 'replacement'))
    expect(await screen.findByRole('status')).toHaveTextContent('GitHub deletion failed')
    expect(screen.getByRole('button', { name: 'Retry GitHub deletion' })).toBeInTheDocument()
    expect(screen.getByLabelText('Webhook administration token (optional)')).toHaveValue('')
  })
  it('reuses saved credentials without sending their value to the browser', async () => {
    api.getGithubReviewHooks.mockResolvedValue({ login: 'cli-account', hooks: [hook] })
    api.disconnectGithubReviewHook.mockResolvedValue({ remote_deleted: true, cleanup_pending: false })
    show(writePermissions)
    await screen.findByText('Encrypted administration token saved')
    await userEvent.setup().click(screen.getByRole('button', { name: 'Disconnect' }))
    await waitFor(() => expect(api.disconnectGithubReviewHook).toHaveBeenCalledWith('hook', undefined))
    expect(await screen.findByRole('status')).toHaveTextContent('removed from GitHub')
  })
  it('lets the operator forget a saved management credential', async () => {
    api.getGithubReviewHooks.mockResolvedValue({ login: 'cli-account', hooks: [hook] })
    show(writePermissions)
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Forget administration token' }))
    await waitFor(() => expect(api.revokeAutonomousAgentConnector).toHaveBeenCalledWith('credential'))
  })
})
