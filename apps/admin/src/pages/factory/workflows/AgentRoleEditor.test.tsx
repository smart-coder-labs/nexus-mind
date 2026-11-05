import { useState } from 'react'
import { describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { AgentRoleEditor } from './AgentRoleEditor'
import type { AutonomousAgentDefinition, AutonomousAgentDetail } from '../../../types'
import type { RuntimeAgent } from './runtime'
const agent = {
  id: 'profile',
  name: 'Kasymir resolver',
  template_key: 'github_issue_resolver',
  status: 'enabled',
  validation_status: 'valid',
  current_revision: 2,
} as AutonomousAgentDefinition
const detail = {
  ...agent,
  revision: {
    revision: 2,
    config_hash: 'hash',
    config: { model: 'sonnet', executor: 'claude', target_password: 'fixture-never-copy' },
    budgets: {},
  },
} as unknown as AutonomousAgentDetail
function Row({
  load = vi.fn().mockResolvedValue(detail),
}: {
  load?: (id: string) => Promise<AutonomousAgentDetail>
}) {
  const [value, setValue] = useState<RuntimeAgent>({
    role: 'implementation',
    model: 'haiku',
    image: 'sha256:' + 'a'.repeat(64),
    model_connection: 'proxy',
    network: 'internal',
  })
  return (
    <>
      <AgentRoleEditor
        label="Implementación"
        value={value}
        agents={[agent]}
        availableHarnesses={['claude']}
        disabled={false}
        loadAgent={load}
        onChange={(p) => setValue((v) => ({ ...v, ...p }))}
      />
      <output data-testid="selection">{JSON.stringify(value)}</output>
    </>
  )
}
describe('catalog agent and harness selection', () => {
  it('reuses a versioned profile without copying credentials or changing its permissions', async () => {
    const load = vi.fn().mockResolvedValue(detail)
    render(<Row load={load} />)
    const user = userEvent.setup()
    await user.selectOptions(screen.getByLabelText('Perfil de agente · Implementación'), 'profile')
    await waitFor(() =>
      expect(screen.getByLabelText('Modelo · Implementación')).toHaveValue('sonnet'),
    )
    expect(load).toHaveBeenCalledWith('profile')
    expect(screen.getByLabelText('Modelo · Implementación')).toBeDisabled()
    expect(screen.getByTestId('selection')).toHaveTextContent('"revision":2')
    expect(screen.getByTestId('selection')).not.toHaveTextContent('fixture-never-copy')
  })
  it('selects a different harness explicitly without silently installing or falling back', async () => {
    render(<Row />)
    const user = userEvent.setup()
    await user.selectOptions(screen.getByLabelText('Harness · Implementación'), 'pi')
    expect(screen.getByTestId('selection')).toHaveTextContent('"harness":"pi"')
    expect(screen.getByText(/no tienes que escribir una imagen/i)).toBeInTheDocument()
    expect(screen.getByLabelText('Modelo · Implementación')).toHaveValue('haiku')
  })
  it('lets a phase override the inherited model while retaining the agent reference', async () => {
    render(<Row />)
    const user = userEvent.setup()
    await user.selectOptions(screen.getByLabelText('Perfil de agente · Implementación'), 'profile')
    const model = await screen.findByLabelText('Modelo · Implementación')
    await waitFor(() => expect(model).toBeDisabled())
    await user.click(screen.getByLabelText('Heredar el modelo del agente · Implementación'))
    await user.clear(model)
    await user.type(model, 'gpt-example')
    expect(screen.getByTestId('selection')).toHaveTextContent('"id":"profile"')
    expect(screen.getByTestId('selection')).toHaveTextContent('"inherit_model":false')
    expect(model).toHaveValue('gpt-example')
  })
  it('does not replace the existing role when loading a profile fails', async () => {
    render(<Row load={vi.fn().mockRejectedValue(new Error('Perfil no disponible'))} />)
    await userEvent
      .setup()
      .selectOptions(screen.getByLabelText('Perfil de agente · Implementación'), 'profile')
    expect(await screen.findByRole('alert')).toHaveTextContent('Perfil no disponible')
    expect(screen.getByLabelText('Modelo · Implementación')).toHaveValue('haiku')
  })
})
