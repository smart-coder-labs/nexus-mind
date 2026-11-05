import { beforeEach, describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { GithubReviewHooks } from './GithubReviewHooks'
const api = vi.hoisted(() => ({
  getGithubReviewHooks: vi.fn(),
  listAutonomousAgents: vi.fn(),
  registerGithubReviewHook: vi.fn(),
  disconnectGithubReviewHook: vi.fn(),
}))
const auth = vi.hoisted(() => ({ permissions: [] as string[] }))
vi.mock('../../../api/client', () => ({ createClient: () => api }))
vi.mock('../../../auth/AuthContext', () => ({
  useAuth: () => ({ session: { user: { permissions: auth.permissions } } }),
}))
function show(permissions: string[]) {
  auth.permissions = permissions
  render(
    <MemoryRouter>
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <GithubReviewHooks />
      </QueryClientProvider>
    </MemoryRouter>,
  )
}
beforeEach(() => {
  vi.clearAllMocks()
  api.getGithubReviewHooks.mockResolvedValue({ login: 'cli-account', hooks: [] })
  api.listAutonomousAgents.mockResolvedValue([
    {
      id: 'reviewer',
      name: 'API reviewer',
      template_key: 'github_pr_reviewer',
      status: 'enabled',
      validation_status: 'valid',
    },
  ])
  api.registerGithubReviewHook.mockResolvedValue({ id: 'hook', login: 'cli-account' })
})
const permissions = [
  'autonomous_agent:read',
  'factory_policy:write',
  'autonomous_agent:manage_connectors',
]
async function fill() {
  const user = userEvent.setup()
  await screen.findByText('cli-account')
  await user.selectOptions(screen.getByLabelText('Agente reviewer habilitado'), 'reviewer')
  await user.clear(screen.getByLabelText('URL pública HTTPS del backend'))
  await user.type(screen.getByLabelText('URL pública HTTPS del backend'), 'https://api.example.com')
  return user
}
describe('GitHub review hooks', () => {
  it('uses a setup-only credential without changing identity and clears it', async () => {
    show(permissions)
    const user = await fill()
    const input = screen.getByLabelText('Token de administración del webhook (opcional)')
    expect(input).toHaveAttribute('type', 'password')
    await user.type(input, 'setup-only-fixture')
    await user.click(screen.getByRole('button', { name: 'Conectar webhook de GitHub' }))
    await waitFor(() =>
      expect(api.registerGithubReviewHook).toHaveBeenCalledWith(
        'reviewer',
        'https://api.example.com',
        'setup-only-fixture',
        undefined,
      ),
    )
    await waitFor(() => expect(input).toHaveValue(''))
    expect(screen.getByText('cli-account')).toBeInTheDocument()
  })
  it('does not contact the CLI without permission', () => {
    show([])
    expect(api.getGithubReviewHooks).not.toHaveBeenCalled()
  })
  it('does not grant setup to a read-only user', async () => {
    show(['autonomous_agent:read'])
    expect(await screen.findByText('cli-account')).toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: 'Conectar webhook de GitHub' }),
    ).not.toBeInTheDocument()
  })
  it('registers the selected repository without requiring an administrator token', async () => {
    show(permissions)
    const user = await fill()
    await user.click(screen.getByRole('button', { name: 'Conectar webhook de GitHub' }))
    await waitFor(() =>
      expect(api.registerGithubReviewHook).toHaveBeenCalledWith(
        'reviewer',
        'https://api.example.com',
        undefined,
        undefined,
      ),
    )
    expect(
      screen.getByLabelText('Token de administración del webhook (opcional)'),
    ).not.toBeRequired()
  })
  it('passes an explicit linked repository for a project reviewer', async () => {
    show(permissions)
    const user = await fill()
    await user.type(screen.getByLabelText('Repositorio del proyecto (opcional)'), 'acme/web')
    await user.click(screen.getByRole('button', { name: 'Conectar webhook de GitHub' }))
    await waitFor(() => expect(api.registerGithubReviewHook).toHaveBeenCalledWith('reviewer', 'https://api.example.com', undefined, 'acme/web'))
  })
  it('explains how to configure a missing reviewer instead of showing an unusable selector', async () => {
    api.listAutonomousAgents.mockResolvedValue([])
    show(permissions)
    expect(await screen.findByRole('link', { name: /Configura un agente/ })).toHaveAttribute(
      'href',
      '/factory/agents',
    )
    expect(screen.getByRole('button', { name: 'Conectar webhook de GitHub' })).toBeDisabled()
  })
})
