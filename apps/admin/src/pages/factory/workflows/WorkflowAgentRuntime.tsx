import { useQuery } from '@tanstack/react-query'
import { createClient } from '../../../api/client'
import type { AgentRuntimeConfig } from '../../../types'
import { AgentRuntimeEditor } from '../AgentRuntimeEditor'

export function WorkflowAgentRuntime({ options, onChange }: { options: Record<string, unknown>; onChange: (value: Record<string, unknown>) => void }) {
  const client = createClient()
  const agents = useQuery({ queryKey: ['workflow-runtime-agents'], queryFn: () => client.listAutonomousAgents() })
  const agentId = typeof options.agent_id === 'string' ? options.agent_id : ''
  const agent = useQuery({ queryKey: ['workflow-runtime-agent', agentId], queryFn: () => client.getAutonomousAgent(agentId), enabled: Boolean(agentId) })
  const executor = String(options.executor ?? agent.data?.revision.config.executor ?? 'claude')
  const runtime = (options.runtime ?? agent.data?.revision.config.runtime ?? { nexusmind_profile: 'essential' }) as AgentRuntimeConfig
  return <div className="space-y-2">
    <label className="block text-xs">Agent configuration<select className="mt-1 w-full rounded-md border border-input bg-background p-2 text-sm" value={agentId} onChange={e => {
      const next: Record<string, unknown> = { ...options, agent_id: e.target.value }; delete next.runtime; delete next.executor; delete next.runtime_snapshot; onChange(next)
    }}><option value="">Adapter defaults</option>{agents.data?.filter(a => a.status !== 'archived').map(a => <option key={a.id} value={a.id}>{a.name}</option>)}</select></label>
    {(agents.isError || agent.isError) && <p role="alert" className="text-xs">Cannot load agent configuration.</p>}
    <label className="block text-xs">Runtime executor<select className="mt-1 w-full rounded-md border border-input bg-background p-2 text-sm" value={executor} onChange={e => onChange({ ...options, executor: e.target.value, runtime: structuredClone(runtime) })}>{['claude', 'pi', 'codex', 'nexus'].map(e => <option key={e}>{e}</option>)}</select></label>
    <p className="text-xs text-text-tertiary">Executor: {executor}. The agent revision is pinned when saving. Runtime overrides affect this step only; the runner adapter must support this executor.</p>
    <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={options.runtime !== undefined} disabled={agent.isLoading} onChange={e => { const next = { ...options }; if (e.target.checked) next.runtime = structuredClone(runtime); else delete next.runtime; onChange(next) }} />Customize runtime for this step</label>
    {options.runtime !== undefined && <AgentRuntimeEditor value={runtime} executor={executor} onChange={runtime => onChange({ ...options, runtime })} />}
  </div>
}
