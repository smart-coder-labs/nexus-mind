import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../../../test/render'
import type { AutonomousAgentDefinition } from '../../../types'
import { JudgeRunDialog } from './RunDialogs'

const api = vi.hoisted(() => ({
  getAutonomousAgent: vi.fn(async () => ({
    revision: { config: { repositories: ['kasymir/kasymir-app-ui'] } },
    judge_repositories: ['kasymir/kasymir-app-ui', 'kasymir/kasymir-api'],
  })),
}))
vi.mock('../../../api/client', () => ({ createClient: () => api }))

describe('JudgeRunDialog', () => {
  it('offers the repositories linked to the project target, not only the configured ones', async () => {
    const agent = { id: 'judge-1', name: 'Kasymir Judge' } as AutonomousAgentDefinition
    renderWithProviders(<JudgeRunDialog agent={agent} pending={false} onClose={() => {}} onRun={() => {}} />)
    const [repository] = await screen.findAllByRole('combobox')
    await waitFor(() => expect(repository).toHaveTextContent('kasymir/kasymir-app-ui'))
    await userEvent.click(repository)
    expect(await screen.findByRole('option', { name: 'kasymir/kasymir-api' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'kasymir/kasymir-app-ui' })).toBeInTheDocument()
  })
})
