import { describe, it, expect, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { RuntimeEditor } from './RuntimeEditor'
import { emptyRuntime, parseCommand, formatCommand } from './runtime'
describe('runtime settings', () => {
  it('selects managed Pi without asking for images or losing checks, services or existing Pi models', async () => {
    const value = emptyRuntime()
    value.revision = 8
    const image = 'sha256:' + 'a'.repeat(64)
    value.report.managed_images = { agent: image }
    value.config.agents = [
      { role: 'investigation', harness: 'claude', model: 'opus', image: '', model_connection: '', network: '' },
      { role: 'implementation', harness: 'pi', model: 'gpt-existing', image: '', model_connection: '', network: '' },
    ]
    value.config.checks = [{ name: 'tests', image, cwd: 'web', command: ['npm', 'test'], environment_refs: [] }]
    value.config.services = [{ name: 'web', image, cwd: 'web', command: ['npm', 'start'], healthcheck: ['node', 'health.js'], environment_refs: [] }]
    const onSave = vi.fn().mockResolvedValue(undefined)
    render(<RuntimeEditor value={value} repositories={['web']} section="agents" disabled={false} onSave={onSave} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Usar Pi del backend en las cuatro fases' }))
    await user.click(screen.getByRole('button', { name: 'Guardar configuración' }))
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce())
    const [revision, config] = onSave.mock.calls[0]
    expect(revision).toBe(8)
    expect(config.checks).toEqual(value.config.checks)
    expect(config.services).toEqual(value.config.services)
    expect(config.agents).toHaveLength(4)
    expect(config.agents.every((a: { harness: string; image: string; model_connection: string; network: string }) => a.harness === 'pi' && a.image === image && a.model_connection === 'backend' && a.network === 'none')).toBe(true)
    expect(config.agents.find((a: { role: string }) => a.role === 'implementation').model).toBe('gpt-existing')
  })
  it('parses argv without executing or interpolating a host shell', () => {
    const args = ['python3', '-c', 'print("hello world")']
    expect(parseCommand(formatCommand(args))).toEqual(args)
    expect(parseCommand('npm run test -- "a b"')).toEqual(['npm', 'run', 'test', '--', 'a b'])
    expect(() => parseCommand('npm "unterminated')).toThrow()
    expect(parseCommand('echo $TOKEN; rm -rf /')).toEqual(['echo', '$TOKEN;', 'rm', '-rf', '/'])
  })
  it('saves four independent roles, model choices and references instead of credentials', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    render(
      <RuntimeEditor repositories={['web']} section="agents" disabled={false} onSave={onSave} />,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText(/^Imagen de agentes/), 'sha256:' + 'a'.repeat(64))
    await user.type(screen.getByLabelText('Conexión de modelo compartida'), 'proxy')
    await user.type(screen.getByLabelText(/^Red interna del proxy/), 'model-net')
    await user.click(screen.getByRole('button', { name: 'Guardar configuración' }))
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce())
    const [revision, config] = onSave.mock.calls[0]
    expect(revision).toBe(0)
    expect(config.agents.map((a: { role: string }) => a.role)).toEqual([
      'investigation',
      'test_author',
      'implementation',
      'reviewer',
    ])
    expect(config.agents.every((a: { image: string }) => a.image.startsWith('sha256:'))).toBe(true)
    expect(JSON.stringify(config)).not.toMatch(/ANTHROPIC_AUTH_TOKEN|api_key/)
    expect(await screen.findByRole('status')).toHaveTextContent(/guardada/i)
  })
  it('does not silently adopt a concurrent runtime revision', async () => {
    const { rerender } = render(
      <RuntimeEditor repositories={['web']} section="agents" disabled={false} onSave={vi.fn()} />,
    )
    rerender(
      <RuntimeEditor
        value={{ ...emptyRuntime(), revision: 2 }}
        repositories={['web']}
        section="agents"
        disabled={false}
        onSave={vi.fn()}
      />,
    )
    expect(await screen.findByRole('alert')).toHaveTextContent('otra revisión')
    expect(screen.getByRole('button', { name: 'Guardar configuración' })).toBeDisabled()
  })
  it('reports invalid argv before sending a profile', async () => {
    const onSave = vi.fn()
    render(
      <RuntimeEditor repositories={['web']} section="checks" disabled={false} onSave={onSave} />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Añadir comprobación' }))
    await user.type(screen.getByLabelText('Comando'), 'npm "')
    await user.click(screen.getByRole('button', { name: 'Guardar configuración' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('comillas')
    expect(onSave).not.toHaveBeenCalled()
  })
  it('applies the selected Node environment to existing inherited rows, preserving explicit overrides', async () => {
    const value = emptyRuntime()
    const old = 'sha256:' + 'a'.repeat(64),
      node = 'sha256:' + 'b'.repeat(64),
      custom = 'sha256:' + 'c'.repeat(64)
    value.report.managed_images = { node }
    value.config.checks = ['tests', 'build', 'lint'].map((name, i) => ({
      name,
      cwd: 'web',
      image: i === 2 ? custom : old,
      command: ['npm', name === 'tests' ? 'test' : 'run', ...(name === 'tests' ? [] : [name])],
      environment_refs: [],
    }))
    const onSave = vi.fn().mockResolvedValue(undefined)
    render(
      <RuntimeEditor
        value={value}
        repositories={['web']}
        section="checks"
        disabled={false}
        onSave={onSave}
      />,
    )
    const user = userEvent.setup()
    await user.selectOptions(screen.getByLabelText('Entorno de las comprobaciones'), 'node')
    await user.click(screen.getByRole('button', { name: 'Guardar configuración' }))
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce())
    expect(onSave.mock.calls[0][1].checks.map((r: { image: string }) => r.image)).toEqual([
      node,
      node,
      custom,
    ])
  })
  it('explains the invalid image before saving rather than a generic four-field error', async () => {
    const value = emptyRuntime()
    value.config.checks = [
      {
        name: 'tests',
        image: 'node:22',
        cwd: 'web',
        command: ['npm', 'test'],
        environment_refs: [],
      },
    ]
    const onSave = vi.fn()
    render(
      <RuntimeEditor
        value={value}
        repositories={['web']}
        section="checks"
        disabled={false}
        onSave={onSave}
      />,
    )
    await userEvent.click(screen.getByRole('button', { name: 'Guardar configuración' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Comprobación 1')
    expect(onSave).not.toHaveBeenCalled()
  })
  it('keeps local drafts in memory when navigating away and back', async () => {
    const onDraft = vi.fn()
    const { unmount } = render(
      <RuntimeEditor
        repositories={['web']}
        section="agents"
        disabled={false}
        onSave={vi.fn()}
        onDraft={onDraft}
      />,
    )
    await userEvent
      .setup()
      .type(screen.getByLabelText('Conexión de modelo compartida'), 'private-alias')
    const draft = onDraft.mock.calls.at(-1)![0]
    unmount()
    render(
      <RuntimeEditor
        repositories={['web']}
        section="agents"
        disabled={false}
        onSave={vi.fn()}
        draft={draft}
      />,
    )
    expect(screen.getByLabelText('Conexión de modelo compartida')).toHaveValue('private-alias')
  })
})
