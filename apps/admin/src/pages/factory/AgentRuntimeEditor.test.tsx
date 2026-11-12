import { useState } from 'react'
import { beforeEach, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AgentRuntimeEditor } from './AgentRuntimeEditor'
import { WorkflowEditor } from './workflows/WorkflowEditor'
import type { AgentRuntimeConfig } from '../../types'

const api = vi.hoisted(() => ({ listHarnesses: vi.fn(), getHarnessVersion: vi.fn(), approveHarnessInstall: vi.fn(), listAutonomousAgents: vi.fn(), getAutonomousAgent: vi.fn() }))
vi.mock('../../api/client', () => ({ createClient: () => api }))

function Editor({ executor = 'pi', initial = {} }: { executor?: string; initial?: AgentRuntimeConfig }) {
  const [value, setValue] = useState(initial)
  return <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><AgentRuntimeEditor value={value} executor={executor} onChange={setValue} /></QueryClientProvider>
}
beforeEach(() => {
  vi.clearAllMocks()
  api.listHarnesses.mockResolvedValue([])
  api.listAutonomousAgents.mockResolvedValue([])
  api.approveHarnessInstall.mockResolvedValue({})
})

function WorkflowDraft() {
  const [value, setValue] = useState(JSON.stringify({ name: 'Review', workspace_id: 'w', steps: [{ id: 'review', name: 'Review', kind: 'review', adapter: 'pi-reviewer', depends_on: [], write_repositories: [], options: {} }] }))
  return <WorkflowEditor value={value} onChange={setValue} adapters={['pi-reviewer']} repositories={[]} />
}

it('inherits an agent runtime and scopes edits to the workflow step', async () => {
  const original = { executor: 'pi', runtime: { nexusmind_profile: 'essential', components: [] } }
  api.listAutonomousAgents.mockResolvedValue([{ id: 'agent1', name: 'Backend reviewer', status: 'enabled' }])
  api.getAutonomousAgent.mockResolvedValue({ revision: { config: original } })
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><WorkflowDraft /></QueryClientProvider>)
  const user = userEvent.setup()
  await screen.findByRole('option', { name: 'Backend reviewer' })
  await user.selectOptions(screen.getByLabelText('Agent configuration'), 'agent1')
  await waitFor(() => expect(screen.getByLabelText('Runtime executor')).toHaveValue('pi'))
  await user.click(screen.getByLabelText('Customize runtime for this step'))
  await user.selectOptions(screen.getByLabelText('NexusMind profile (always connected)'), 'only_context')
  const draft = JSON.parse((screen.getByLabelText('Definición JSON') as HTMLTextAreaElement).value)
  expect(draft.steps[0].options.agent_id).toBe('agent1')
  expect(draft.steps[0].options.runtime.nexusmind_profile).toBe('only_context')
  expect(original.runtime.nexusmind_profile).toBe('essential')
})

it('keeps NexusMind mandatory and offers only native executor resources', async () => {
  render(<Editor />)
  const user = userEvent.setup()
  const profile = screen.getByLabelText('NexusMind profile (always connected)')
  expect(profile).toHaveValue('essential')
  await user.selectOptions(profile, 'only_context')
  expect(profile).toHaveValue('only_context')
  const types = screen.getByLabelText('Component type')
  expect(types).toHaveTextContent('extension')
  expect(types).not.toHaveTextContent('plugin')
  expect(screen.queryByLabelText(/Disable NexusMind/)).not.toBeInTheDocument()
})

it('retains incompatible resources visibly and lets a workflow disable them', async () => {
  render(<Editor executor="codex" initial={{ components: [{ id: 'browser', kind: 'extension', files: { 'index.ts': '' } }] }} />)
  expect(screen.getByRole('alert')).toHaveTextContent('Unsupported by codex: browser')
  await userEvent.setup().click(screen.getByLabelText('browser · extension'))
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  expect((screen.getByLabelText('Component configuration JSON') as HTMLTextAreaElement).value).toContain('"enabled": false')
})

it('lists and approves Markdown from another target while still pinning the hash and version', async () => {
  api.listHarnesses.mockResolvedValue([{ id: 'h1', name: 'Rust review', status: 'published', latest_version: { version: '1.0.0' } }])
  api.getHarnessVersion.mockResolvedValue({ harness_id: 'h1', version: '1.0.0', manifest_hash: 'sha256:pinned', targets: ['claude'], manifest: { format: 'agent', components: [{ kind: 'file', path: 'rust/SKILL.md', content: 'Review Rust' }] } })
  render(<Editor />)
  const user = userEvent.setup()
  await screen.findByRole('option', { name: 'Rust review · 1.0.0' })
  expect(api.listHarnesses).toHaveBeenCalledWith()
  await user.selectOptions(screen.getByLabelText('Add a registered harness'), 'h1')
  await screen.findByText(/Pinned version 1.0.0/)
  expect(api.approveHarnessInstall).not.toHaveBeenCalled()
  await user.click(screen.getByLabelText('rust/SKILL.md'))
  await user.click(screen.getByRole('button', { name: 'Approve selected files for this runtime' }))
  await waitFor(() => expect(api.approveHarnessInstall).toHaveBeenCalledWith('h1', '1.0.0', expect.objectContaining({ target_tool: 'pi', target_scope: 'agent-runtime', manifest_hash: 'sha256:pinned' })))
  expect(await screen.findByRole('button', { name: 'Remove harness' })).toBeInTheDocument()
})

it('does not turn a foreign-target executable into a portable Markdown harness', async () => {
  api.listHarnesses.mockResolvedValue([{ id: 'h1', name: 'Shell hook', status: 'published', latest_version: { version: '1.0.0', targets: ['claude'] } }])
  api.getHarnessVersion.mockResolvedValue({ harness_id: 'h1', version: '1.0.0', manifest_hash: 'sha256:pinned', targets: ['claude'], manifest: { format: 'hook', components: [{ kind: 'file', path: 'run.sh', content: 'exit 0' }], security: { executable: true } } })
  render(<Editor />)
  const user = userEvent.setup()
  await screen.findByRole('option', { name: 'Shell hook · 1.0.0' })
  await user.selectOptions(screen.getByLabelText('Add a registered harness'), 'h1')
  await user.click(await screen.findByLabelText('run.sh'))
  expect(screen.getByRole('button', { name: 'Approve selected files for this runtime' })).toBeDisabled()
  expect(api.approveHarnessInstall).not.toHaveBeenCalled()
})

it('re-approves assigned harnesses when the executor changes', async () => {
  api.listHarnesses.mockResolvedValue([{ id: 'h1', name: 'Rust review', status: 'published', latest_version: { version: '1.0.0' } }])
  const initial = { harnesses: [{ id: 'h1', version: '1.0.0', manifest_hash: 'sha256:pinned', components: ['rust/SKILL.md'] }] }
  const wrap = (executor: string) => <QueryClientProvider client={client}><Switchable executor={executor} initial={initial} /></QueryClientProvider>
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const { rerender } = render(wrap('claude'))
  expect(await screen.findByText(/Rust review · 1.0.0/)).toBeInTheDocument()
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  rerender(wrap('pi'))
  expect(screen.getByRole('alert')).toHaveTextContent('approved for pi before saving')
  await userEvent.setup().click(screen.getByRole('button', { name: 'Approve assigned harnesses for pi' }))
  await waitFor(() => expect(api.approveHarnessInstall).toHaveBeenCalledWith('h1', '1.0.0', expect.objectContaining({ target_tool: 'pi', target_scope: 'agent-runtime', manifest_hash: 'sha256:pinned' })))
  expect(await screen.findByText('Approved 1 assigned harness for pi.')).toBeInTheDocument()
  expect(screen.queryByRole('alert')).not.toBeInTheDocument()
})

function Switchable({ executor, initial }: { executor: string; initial: AgentRuntimeConfig }) {
  const [value, setValue] = useState(initial)
  return <AgentRuntimeEditor value={value} executor={executor} onChange={setValue} />
}
