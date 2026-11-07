import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { npmrcFromToken, RepoSecrets } from './RepoSecrets'

const client = {
  listRepoSecrets: vi.fn(),
  saveRepoSecret: vi.fn(),
  deleteRepoSecret: vi.fn(),
}
vi.mock('../shared/ui', async (original) => ({
  ...(await original<typeof import('../shared/ui')>()),
  useFactory: () => ({ client, can: () => true }),
}))

describe('RepoSecrets', () => {
  it('builds a scoped .npmrc from a token', () => {
    expect(npmrcFromToken('@Kasymir', 'https://npm.pkg.github.com/', 'ghp_x')).toBe(
      '@kasymir:registry=https://npm.pkg.github.com/\n//npm.pkg.github.com/:_authToken=ghp_x\n',
    )
    expect(npmrcFromToken('', 'https://registry.example.com', 't')).toBe(
      'registry=https://registry.example.com/\n//registry.example.com/:_authToken=t\n',
    )
  })

  it('shows saved metadata, never values, and saves write-only content', async () => {
    client.listRepoSecrets.mockResolvedValue({
      encryption_configured: true,
      secrets: [{ repository: 'api', kind: 'env', summary: { keys: ['DB_URL'] }, updated_at: '2026-11-07 14:00:00' }],
    })
    client.saveRepoSecret.mockResolvedValue({ encryption_configured: true, secrets: [] })
    render(
      <QueryClientProvider client={new QueryClient()}>
        <RepoSecrets workspaceId="w1" repositories={['api', 'web']} disabled={false} />
      </QueryClientProvider>,
    )
    expect(await screen.findByText(/DB_URL/)).toBeTruthy()
    expect((screen.getByLabelText('Contenido .env') as HTMLTextAreaElement).value).toBe('')
    fireEvent.change(screen.getByPlaceholderText('ghp_… (read:packages)'), { target: { value: 'ghp_secret' } })
    fireEvent.change(screen.getByPlaceholderText('@kasymir'), { target: { value: '@kasymir' } })
    fireEvent.click(screen.getByText('Guardar autenticación'))
    await waitFor(() =>
      expect(client.saveRepoSecret).toHaveBeenCalledWith(
        'w1', 'api', 'npmrc',
        '@kasymir:registry=https://npm.pkg.github.com/\n//npm.pkg.github.com/:_authToken=ghp_secret\n',
      ),
    )
  })
})
