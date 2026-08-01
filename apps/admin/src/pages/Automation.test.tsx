import { describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Automation from './Automation'

global.fetch = vi.fn().mockImplementation(() =>
  Promise.resolve({
    ok: true,
    json: () => Promise.resolve({
      profiles: [
        { id: '1', profile: 'read-only', provider: 'claude-code', model: 'claude-sonnet' },
        { id: '2', profile: 'implementation', provider: 'claude-code', model: 'claude-sonnet' },
        { id: '3', profile: 'qa-deploy', provider: 'claude-code', model: 'claude-sonnet' },
      ],
    }),
  } as Response)
)

describe('Automation Governance Page', () => {
  it('renders automation profiles and kill-switch controls', async () => {
    renderWithProviders(<Automation />)

    expect(screen.getByText('Automation Governance')).toBeTruthy()
    expect(screen.getByText('Managed Provider')).toBeTruthy()

    await waitFor(() => {
      expect(screen.getByText('read-only')).toBeTruthy()
      expect(screen.getByText('implementation')).toBeTruthy()
      expect(screen.getByText('qa-deploy')).toBeTruthy()
    })
  })
})
