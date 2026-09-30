import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AuthContext } from '../auth/AuthContext'
import type { AuthSession, FactoryPolicy } from '../types'
import FactoryPolicies from './FactoryPolicies'

const api = vi.hoisted(() => ({
  listFactoryPolicies: vi.fn(),
  putFactoryPolicy: vi.fn(),
  deleteFactoryPolicy: vi.fn(),
  getFactoryBot: vi.fn(),
  rotateFactoryBotKey: vi.fn(),
}))

vi.mock('../api/client', () => ({ createClient: () => api }))

const docsMerge: FactoryPolicy = {
  id: 'p1',
  schema_version: 1,
  action: 'merge',
  mode: 'criteria',
  scope: { task_class: 'docs' },
  allow: ['docs/tests paths only'],
  stop: [],
  version: 3,
  updated_by: 'u1',
  updated_at: '2026-09-29 20:00:00',
}

function renderPage(permissions: string[]) {
  const session: AuthSession = {
    org: { id: 'o1', name: 'Acme', slug: 'acme', created_at: '2026-01-01' },
    user: {
      id: 'u1', org_id: 'o1', email: 'owner@acme.test', name: 'Owner', role: 'admin',
      status: 'active', created_at: '2026-01-01', permissions,
    },
  }
  return render(
    <MemoryRouter>
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <AuthContext.Provider value={{ session, loading: false, setSession: () => undefined, logout: () => undefined }}>
          <FactoryPolicies />
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

describe('FactoryPolicies', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.listFactoryPolicies.mockResolvedValue([])
    api.getFactoryBot.mockResolvedValue({ bot: null })
  })

  it('does not infer access from the role name', () => {
    renderPage([])
    expect(screen.getByText(/you need the factory_policy:read permission/i)).toBeInTheDocument()
    expect(api.listFactoryPolicies).not.toHaveBeenCalled()
  })

  it('explains that no policy means every action waits for a person', async () => {
    renderPage(['factory_policy:read'])
    expect(await screen.findByText(/every action waits for a person/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new policy/i })).not.toBeInTheDocument()
  })

  it('creates a new policy at version 1', async () => {
    api.putFactoryPolicy.mockResolvedValue({ ...docsMerge, version: 1 })
    renderPage(['factory_policy:read', 'factory_policy:write'])
    await userEvent.click(await screen.findByRole('button', { name: /new policy/i }))
    await userEvent.selectOptions(screen.getByLabelText('Action'), 'merge')
    await userEvent.selectOptions(screen.getByLabelText('Mode'), 'criteria')
    await userEvent.selectOptions(screen.getByLabelText('Task class'), 'docs')
    await userEvent.type(screen.getByLabelText(/allow when/i), 'docs/tests paths only')
    await userEvent.click(screen.getByRole('button', { name: /save policy/i }))
    expect(api.putFactoryPolicy).toHaveBeenCalledWith({
      schema_version: 1,
      action: 'merge',
      mode: 'criteria',
      scope: { task_class: 'docs' },
      allow: ['docs/tests paths only'],
      stop: [],
      version: 1,
    })
  })

  it('edits an existing policy at the next version', async () => {
    api.listFactoryPolicies.mockResolvedValue([docsMerge])
    api.putFactoryPolicy.mockResolvedValue({ ...docsMerge, mode: 'manual', version: 4 })
    renderPage(['factory_policy:read', 'factory_policy:write'])
    const row = (await screen.findByText('merge')).closest('tr')!
    await userEvent.click(within(row).getByRole('button', { name: /edit/i }))
    await userEvent.selectOptions(screen.getByLabelText('Mode'), 'manual')
    await userEvent.click(screen.getByRole('button', { name: /save policy/i }))
    expect(api.putFactoryPolicy).toHaveBeenCalledWith(expect.objectContaining({ mode: 'manual', version: 4 }))
  })

  it('tells the user to reload on a version conflict', async () => {
    api.listFactoryPolicies.mockResolvedValue([docsMerge])
    api.putFactoryPolicy.mockRejectedValue(Object.assign(new Error('conflict'), { status: 409, code: 'policy_version_conflict' }))
    renderPage(['factory_policy:read', 'factory_policy:write'])
    const row = (await screen.findByText('merge')).closest('tr')!
    await userEvent.click(within(row).getByRole('button', { name: /edit/i }))
    await userEvent.click(screen.getByRole('button', { name: /save policy/i }))
    expect(await screen.findByText(/someone changed this policy/i)).toBeInTheDocument()
  })

  it('reports a failed delete instead of silently keeping the row', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    api.listFactoryPolicies.mockResolvedValue([docsMerge])
    api.deleteFactoryPolicy.mockRejectedValue(Object.assign(new Error('Insufficient permissions'), { status: 403, code: 'forbidden' }))
    renderPage(['factory_policy:read', 'factory_policy:write'])
    const row = (await screen.findByText('merge')).closest('tr')!
    await userEvent.click(within(row).getByRole('button', { name: /delete/i }))
    expect(await screen.findByRole('alert')).toHaveTextContent(/could not delete the merge policy/i)
  })

  it('warns that a project-scoped merge policy holds every merge until runs carry a project', async () => {
    renderPage(['factory_policy:read', 'factory_policy:write'])
    await userEvent.click(await screen.findByRole('button', { name: /new policy/i }))
    await userEvent.selectOptions(screen.getByLabelText('Action'), 'merge')
    expect(screen.queryByText(/holds every autonomous merge/i)).not.toBeInTheDocument()
    await userEvent.type(screen.getByLabelText('Project'), 'web')
    expect(screen.getByText(/holds every autonomous merge/i)).toBeInTheDocument()
  })

  it('shows the sandbox bot state without offering keys to readers', async () => {
    renderPage(['factory_policy:read'])
    expect(await screen.findByText(/no sandbox bot yet/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /generate bot key/i })).not.toBeInTheDocument()
  })

  it('shows a new bot key exactly once', async () => {
    api.rotateFactoryBotKey.mockResolvedValue({
      bot: { user_id: 'b1', role: 'factory-bot', role_permissions: ['memory:read'], status: 'active', key_created_at: '2026-09-30T12:00:00Z' },
      api_key: 'nm_secret_once',
    })
    renderPage(['factory_policy:read', 'factory_policy:write'])
    await userEvent.click(await screen.findByRole('button', { name: /generate bot key/i }))
    expect(await screen.findByText('nm_secret_once')).toBeInTheDocument()
    expect(screen.getByText(/will not be shown again/i)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: /i stored it/i }))
    expect(screen.queryByText('nm_secret_once')).not.toBeInTheDocument()
  })

  it('never offers to rotate while the bot state is unknown', async () => {
    api.getFactoryBot.mockRejectedValue(new Error('network down'))
    renderPage(['factory_policy:read', 'factory_policy:write'])
    expect(await screen.findByText(/could not load the sandbox bot/i)).toBeInTheDocument()
    const button = screen.getByRole('button', { name: /bot key/i })
    expect(button).toBeDisabled()
    expect(api.rotateFactoryBotKey).not.toHaveBeenCalled()
  })

  it('asks before rotating an active bot key', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    api.getFactoryBot.mockResolvedValue({
      bot: { user_id: 'b1', role: 'factory-bot', role_permissions: ['memory:read'], status: 'active', key_created_at: '2026-09-30T12:00:00Z' },
    })
    renderPage(['factory_policy:read', 'factory_policy:write'])
    await userEvent.click(await screen.findByRole('button', { name: /rotate bot key/i }))
    expect(confirm).toHaveBeenCalled()
    expect(api.rotateFactoryBotKey).not.toHaveBeenCalled()
  })

  it('is read-only without the write permission', async () => {
    api.listFactoryPolicies.mockResolvedValue([docsMerge])
    renderPage(['factory_policy:read'])
    const row = (await screen.findByText('merge')).closest('tr')!
    expect(within(row).queryByRole('button', { name: /edit/i })).not.toBeInTheDocument()
    expect(within(row).queryByRole('button', { name: /delete/i })).not.toBeInTheDocument()
  })
})
