import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { AuthContext } from '../auth/AuthContext'
import type { ApiKeyWithUser, AuthSession } from '../types'
import Agents from './Agents'

const api = vi.hoisted(() => ({
  listOrgKeys: vi.fn(),
  listRoles: vi.fn(),
  revokeOrgKey: vi.fn(),
  resetUserKey: vi.fn(),
  getAgentActivity: vi.fn(),
  inviteUser: vi.fn(),
}))

vi.mock('../api/client', () => ({ createClient: () => api }))

const agentKey: ApiKeyWithUser = {
  id: 'k1',
  user_id: 'u2',
  user_name: 'CI Agent',
  user_email: 'ci-agent@agent.local',
  label: 'nm_abc123456',
  last_used: null,
  created_at: '2026-01-02 03:04:05',
  revoked: false,
  expires_at: null,
  times_used: 7,
  last_used_at: null,
}

function renderPage() {
  const session: AuthSession = {
    org: { id: 'o1', name: 'Acme', slug: 'acme', created_at: '2026-01-01' },
    user: {
      id: 'u1',
      org_id: 'o1',
      email: 'admin@acme.test',
      name: 'Admin',
      role: 'member',
      status: 'active',
      created_at: '2026-01-01',
      permissions: [],
    },
  }
  return render(
    <MemoryRouter>
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <AuthContext.Provider
          value={{ session, loading: false, setSession: () => undefined, logout: () => undefined }}
        >
          <Agents />
        </AuthContext.Provider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

describe('Agents accessibility affordances', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.listOrgKeys.mockResolvedValue([agentKey])
    api.listRoles.mockResolvedValue([])
    api.getAgentActivity.mockResolvedValue([])
  })

  it('announces the selected status filter via aria-pressed', async () => {
    renderPage()

    const group = await screen.findByRole('group', { name: 'Filter agents by status' })
    expect(group).toBeInTheDocument()
    // Let both page queries settle so nothing resolves outside act().
    expect(await screen.findByText('CI Agent')).toBeInTheDocument()
    expect(await screen.findByText('No recent agent activity.')).toBeInTheDocument()

    const all = screen.getByRole('button', { name: 'All' })
    const active = screen.getByRole('button', { name: 'Active' })
    const inactive = screen.getByRole('button', { name: 'Inactive' })
    const expired = screen.getByRole('button', { name: 'Expired' })

    expect(all).toHaveAttribute('aria-pressed', 'true')
    expect(active).toHaveAttribute('aria-pressed', 'false')
    expect(inactive).toHaveAttribute('aria-pressed', 'false')
    expect(expired).toHaveAttribute('aria-pressed', 'false')

    await userEvent.click(active)

    expect(active).toHaveAttribute('aria-pressed', 'true')
    expect(all).toHaveAttribute('aria-pressed', 'false')
    expect(inactive).toHaveAttribute('aria-pressed', 'false')
    expect(expired).toHaveAttribute('aria-pressed', 'false')
  })

  it('keeps card actions reachable on touch and keyboard focus', async () => {
    renderPage()

    const rotate = await screen.findByRole('button', { name: 'Rotate key' })
    const revoke = screen.getByRole('button', { name: 'Revoke key' })
    // Let the activity query settle so nothing resolves outside act().
    expect(await screen.findByText('No recent agent activity.')).toBeInTheDocument()

    const actions = rotate.parentElement
    expect(actions).not.toBeNull()
    // Both actions share the one visibility container.
    expect(actions).toBe(revoke.parentElement)

    const className = actions!.className
    expect(className).toContain('opacity-100')
    expect(className).toContain('sm:opacity-0')
    expect(className).toContain('sm:group-hover:opacity-100')
    expect(className).toContain('sm:group-focus-within:opacity-100')
    expect(className).toContain('transition-opacity')
    // No unprefixed opacity-0: that would hide the actions on touch devices.
    expect(className).not.toMatch(/(^|\s)opacity-0(\s|$)/)
  })
})
