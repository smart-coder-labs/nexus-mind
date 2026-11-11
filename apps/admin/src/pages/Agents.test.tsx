import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../test/render'
import type { ApiKeyWithUser } from '../types'
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

describe('Agents accessibility affordances', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.listOrgKeys.mockResolvedValue([agentKey])
    api.listRoles.mockResolvedValue([])
    api.getAgentActivity.mockResolvedValue([])
  })

  it('announces the selected status filter via aria-pressed', async () => {
    renderWithProviders(<Agents />)

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
    renderWithProviders(<Agents />)

    const rotate = await screen.findByRole('button', { name: 'Rotate key' })
    const revoke = screen.getByRole('button', { name: 'Revoke key' })
    // Let the activity query settle so nothing resolves outside act().
    expect(await screen.findByText('No recent agent activity.')).toBeInTheDocument()

    const actions = rotate.parentElement
    expect(actions).not.toBeNull()
    // Both actions share the one visibility container.
    expect(actions).toBe(revoke.parentElement)

    // jsdom has no computed styles here (css: false), so the class list is the
    // only proxy. Assert just the two properties this fix is about.
    const className = actions!.className
    // Revealed when keyboard focus moves into the card.
    expect(className).toContain('sm:group-focus-within:opacity-100')
    // No unprefixed opacity-0: that would hide the actions on touch devices.
    expect(className).not.toMatch(/(^|\s)opacity-0(\s|$)/)
  })
})
