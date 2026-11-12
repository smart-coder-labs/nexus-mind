import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import { RepositoryInventory } from './RepositoryInventory'

it('shows and confirms the source branch without granting write permission', async () => {
  const repo = { path: 'accounting', remote: 'https://github.com/acme/accounting.git', head_sha: 'a'.repeat(40), branch: 'master', writable: false }
  const save = vi.fn()
  render(<RepositoryInventory disabled={false} onSave={save} workspace={{
    id: 'workspace', runner_id: 'runner', name: 'Product', root_path: '/workspace',
    discovered: [repo], repositories: [], inventory_revision: 2, confirmed_revision: null,
  }} />)
  expect(screen.getByText('master · actualizar antes de cada ejecución')).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: 'Guardar selección y permisos' }))
  expect(save).toHaveBeenCalledWith([repo])
})
