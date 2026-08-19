import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Navigate } from 'react-router-dom'
import { Archive, Bot, Copy, Pencil, Play, Plus, RefreshCw, ShieldCheck, X } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/ui/Button'
import { Badge } from '../components/ui/Badge'
import { Switch } from '../components/ui/Switch'
import { SegmentedControl } from '../components/ui/SegmentedControl'
import { EmptyState } from '../components/ui/EmptyState'
import { Modal, ModalHeader, ModalTitle, ModalContent } from '../components/ui/Modal'
import AutonomousAgentWizard from './AutonomousAgentWizard'
import type { AutonomousAgentDefinition, AutonomousAgentDetail, AutonomousAgentRun, AutonomousAgentTemplate } from '../types'

type Tab = 'agents' | 'templates' | 'runs' | 'findings' | 'runtime'

const TABS: { value: Tab; label: string }[] = [
  { value: 'agents', label: 'Agents' },
  { value: 'templates', label: 'Templates' },
  { value: 'runs', label: 'Runs' },
  { value: 'findings', label: 'Findings' },
  { value: 'runtime', label: 'Runtime' },
]

const statusVariant: Record<string, 'success' | 'default' | 'warning'> = { enabled: 'success', disabled: 'default', archived: 'warning' }

export default function AutonomousAgents() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const can = (permission: string) => permissions.includes(permission)
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const [tab, setTab] = useState<Tab>('agents')
  const [showArchived, setShowArchived] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [editing, setEditing] = useState<AutonomousAgentDetail | null>(null)
  const [templateDetail, setTemplateDetail] = useState<AutonomousAgentTemplate | null>(null)
  const [selectedRun, setSelectedRun] = useState<AutonomousAgentRun | null>(null)
  const [actionError, setActionError] = useState('')

  const templates = useQuery({ queryKey: ['autonomous-templates'], queryFn: () => client.listAutonomousAgentTemplates(), enabled: can('autonomous_agent:read') })
  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents(), enabled: can('autonomous_agent:read') })
  const runs = useQuery({ queryKey: ['autonomous-runs'], queryFn: () => client.listAutonomousAgentRuns(), enabled: can('autonomous_agent:read') && tab === 'runs' })
  const events = useQuery({ queryKey: ['autonomous-run-events', selectedRun?.id], queryFn: () => client.listAutonomousAgentRunEvents(selectedRun!.id), enabled: can('autonomous_agent:read') && Boolean(selectedRun) })
  const runtime = useQuery({ queryKey: ['autonomous-runtime'], queryFn: () => client.getAutonomousRuntimeHealth(), enabled: can('autonomous_agent:read') && tab === 'runtime' })
  const settings = useQuery({ queryKey: ['autonomous-settings'], queryFn: () => client.getAutonomousAgentSettings(), enabled: can('autonomous_agent:read') && tab === 'runtime' })
  const metrics = useQuery({ queryKey: ['autonomous-metrics'], queryFn: () => client.getAutonomousAgentMetrics(), enabled: can('autonomous_agent:read') && tab === 'runtime', refetchInterval: 30_000 })
  const findings = useQuery({ queryKey: ['autonomous-findings'], queryFn: () => client.listAutonomousAgentFindings(), enabled: can('autonomous_agent:read') && tab === 'findings' })
  const deliveries = useQuery({ queryKey: ['autonomous-deliveries'], queryFn: () => client.listAutonomousAgentDeliveries(), enabled: can('autonomous_agent:read') && tab === 'findings' })

  const invalidate = (...keys: string[]) => Promise.all(keys.map(key => queryClient.invalidateQueries({ queryKey: [key] })))
  const friendly = (message: string) => message === 'validation_required' ? 'Validate the agent before enabling it.' : message
  const useLifecycle = (fn: (id: string) => Promise<unknown>) => useMutation({
    mutationFn: fn,
    onSuccess: () => { setActionError(''); return invalidate('autonomous-agents') },
    onError: (value: unknown) => setActionError(friendly(value instanceof Error ? value.message : 'Action failed')),
  })
  const enable = useLifecycle(id => client.enableAutonomousAgent(id))
  const disable = useLifecycle(id => client.disableAutonomousAgent(id))
  const archive = useLifecycle(id => client.archiveAutonomousAgent(id))
  const validate = useMutation({
    mutationFn: (id: string) => client.validateAutonomousAgent(id),
    onSuccess: detail => {
      void invalidate('autonomous-agents')
      if (detail.revision.validation_status === 'valid') { setActionError(''); return }
      const errors = Array.isArray((detail.revision.validation as { errors?: unknown } | null)?.errors) ? ((detail.revision.validation as { errors: string[] }).errors) : []
      setActionError(errors.length ? `Validation failed: ${errors.join(', ')}` : 'Validation failed — review the configuration.')
    },
    onError: (value: unknown) => setActionError(value instanceof Error ? value.message : 'Validation failed'),
  })
  const clone = useMutation({ mutationFn: async (id: string) => { const source = await client.getAutonomousAgent(id); return client.createAutonomousAgent({ name: `${source.name} copy`, description: source.description ?? undefined, template_key: source.template_key, config: source.revision.config, budgets: source.revision.budgets }) }, onSuccess: () => invalidate('autonomous-agents') })
  const runNow = useMutation({ mutationFn: (id: string) => client.runAutonomousAgent(id), onSuccess: () => invalidate('autonomous-runs') })
  const cancelRun = useMutation({ mutationFn: (id: string) => client.cancelAutonomousAgentRun(id), onSuccess: () => invalidate('autonomous-runs') })
  const checkRuntime = useMutation({ mutationFn: () => client.checkAutonomousRuntimeHealth(), onSuccess: () => invalidate('autonomous-runtime') })
  const toggleOrg = useMutation({ mutationFn: (enabled: boolean) => client.patchAutonomousAgentSettings({ enabled }), onSuccess: () => invalidate('autonomous-settings', 'autonomous-runs') })
  const saveRetention = useMutation({ mutationFn: (days: number) => client.patchAutonomousAgentSettings({ retention_days: days }), onSuccess: () => invalidate('autonomous-settings') })
  const retryDelivery = useMutation({ mutationFn: (id: string) => client.retryAutonomousAgentDelivery(id), onSuccess: () => invalidate('autonomous-deliveries') })
  const resolveFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'resolved'), onSuccess: () => invalidate('autonomous-findings') })

  const openEdit = async (agent: AutonomousAgentDefinition) => {
    const detail = await client.getAutonomousAgent(agent.id)
    setEditing(detail)
  }

  if (!can('autonomous_agent:read')) return <Navigate to="/401" replace />

  const loading = [agents, templates].some(query => query.isLoading)
  const loadError = [agents, templates, runs, findings, runtime].find(query => query.isError)
  const allAgents = agents.data ?? []
  const archivedCount = allAgents.filter(agent => agent.status === 'archived').length
  const visibleAgents = showArchived ? allAgents : allAgents.filter(agent => agent.status !== 'archived')

  return (
    <div className="p-6 md:p-8 space-y-6 max-w-7xl mx-auto">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary flex items-center gap-2"><Bot className="w-6 h-6" />Autonomous agents</h1>
          <p className="text-sm text-text-tertiary mt-1">Scheduled Claude Code agents running on this NexusMind server.</p>
        </div>
        {can('autonomous_agent:create') && (
          <Button variant="primary" leftIcon={<Plus className="w-4 h-4" />} onClick={() => setShowCreate(true)}>Create agent</Button>
        )}
      </header>

      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-primary pb-3">
        <SegmentedControl options={TABS} value={tab} onChange={setTab} />
        {tab === 'agents' && archivedCount > 0 && (
          <Switch size="sm" checked={showArchived} onCheckedChange={setShowArchived} label={`Show archived (${archivedCount})`} />
        )}
      </div>

      {loading && <p role="status" className="text-sm text-text-tertiary">Loading automation control plane…</p>}
      {loadError && <p role="alert" className="text-sm text-status-error">Could not load autonomous agent data.</p>}

      {tab === 'agents' && !loading && (
        <div className="grid gap-3">
          {actionError && (
            <div role="alert" className="flex items-start justify-between gap-3 rounded-[12px] border border-status-error/40 bg-status-error/[0.06] px-4 py-3 text-sm text-status-error">
              <span>{actionError}</span>
              <button type="button" onClick={() => setActionError('')} aria-label="Dismiss"><X className="w-4 h-4" /></button>
            </div>
          )}
          {visibleAgents.map(agent => (
            <article key={agent.id} className="rounded-[14px] border border-border-primary bg-white/[0.02] p-4 flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
              <div className="min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <h2 className="font-semibold text-text-primary truncate">{agent.name}</h2>
                  <Badge size="sm" variant={statusVariant[agent.status] ?? 'default'}>{agent.status}</Badge>
                  {agent.status === 'disabled' && agent.validation_status === 'invalid' && <Badge size="sm" variant="error">invalid</Badge>}
                  {agent.status === 'disabled' && agent.validation_status === 'valid' && <Badge size="sm" variant="success">validated</Badge>}
                </div>
                <p className="text-xs text-text-tertiary mt-1">{agent.template_key.replace(/_/g, ' ')} · revision {agent.current_revision}</p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {can('autonomous_agent:enable') && agent.status === 'disabled' && <>
                  <Button size="sm" variant={agent.validation_status === 'valid' ? 'outline' : 'secondary'} loading={validate.isPending && validate.variables === agent.id} onClick={() => validate.mutate(agent.id)}>Validate</Button>
                  {agent.validation_status === 'valid'
                    ? <Button size="sm" variant="primary" onClick={() => enable.mutate(agent.id)}>Enable</Button>
                    : <span className="text-xs text-text-tertiary px-1">Validate to enable</span>}
                </>}
                {can('autonomous_agent:enable') && agent.status === 'enabled' && <Button size="sm" variant="outline" onClick={() => disable.mutate(agent.id)}>Disable</Button>}
                {can('autonomous_agent:run') && agent.status === 'enabled' && <Button size="sm" variant="primary" leftIcon={<Play className="w-3.5 h-3.5" />} onClick={() => runNow.mutate(agent.id)}>Run</Button>}
                {can('autonomous_agent:update') && agent.status !== 'archived' && <Button size="sm" variant="ghost" leftIcon={<Pencil className="w-3.5 h-3.5" />} onClick={() => void openEdit(agent)}>Edit</Button>}
                {can('autonomous_agent:create') && <Button size="sm" variant="ghost" leftIcon={<Copy className="w-3.5 h-3.5" />} onClick={() => clone.mutate(agent.id)}>Clone</Button>}
                {can('autonomous_agent:update') && agent.status === 'disabled' && <Button size="sm" variant="ghost" leftIcon={<Archive className="w-3.5 h-3.5" />} onClick={() => archive.mutate(agent.id)}>Archive</Button>}
              </div>
            </article>
          ))}
          {visibleAgents.length === 0 && (
            <EmptyState
              icon={<Bot className="w-6 h-6" />}
              title={allAgents.length === 0 ? 'No autonomous agents yet' : 'No active agents'}
              description={allAgents.length === 0 ? 'Create your first agent from a managed template.' : 'All agents are archived. Toggle “Show archived” to see them.'}
              action={can('autonomous_agent:create') && allAgents.length === 0 ? <Button variant="primary" leftIcon={<Plus className="w-4 h-4" />} onClick={() => setShowCreate(true)}>Create agent</Button> : undefined}
            />
          )}
        </div>
      )}

      {tab === 'templates' && (
        <div className="grid gap-4 md:grid-cols-3">
          {templates.data?.map(item => (
            <button key={item.key} type="button" onClick={() => setTemplateDetail(item)} className="text-left rounded-[14px] border border-border-primary p-4 transition-colors hover:border-white/20">
              <div className="flex items-center justify-between">
                <h2 className="font-semibold text-text-primary">{item.name}</h2>
                <Badge size="sm" variant="default">v{item.version}</Badge>
              </div>
              <p className="text-sm text-text-tertiary mt-2">{item.description}</p>
              <div className="mt-3 flex flex-wrap gap-1">{item.capabilities.slice(0, 3).map(cap => <Badge key={cap} size="sm" variant="info">{cap}</Badge>)}</div>
              <p className="mt-3 text-xs text-accent-blue">View details →</p>
            </button>
          ))}
        </div>
      )}

      {tab === 'runs' && (
        <div className="grid gap-4 lg:grid-cols-2">
          <div className="rounded-[14px] border border-border-primary divide-y divide-border-primary">
            {runs.data?.map(run => (
              <button type="button" key={run.id} onClick={() => setSelectedRun(run)} className={`w-full p-4 flex justify-between text-left ${selectedRun?.id === run.id ? 'bg-white/[0.03]' : ''}`}>
                <div>
                  <div className="text-sm text-text-primary">{run.trigger_kind} run</div>
                  <div className="text-xs text-text-tertiary">{new Date(`${run.created_at}Z`).toLocaleString()}</div>
                </div>
                <div className="flex items-center gap-3">
                  <Badge size="sm" variant="default">{run.status.replace(/_/g, ' ')}</Badge>
                  {can('autonomous_agent:cancel') && ['queued', 'leased', 'running'].includes(run.status) && <span onClick={event => { event.stopPropagation(); cancelRun.mutate(run.id) }} className="text-xs text-status-error">Cancel</span>}
                </div>
              </button>
            ))}
            {runs.data?.length === 0 && <EmptyState title="No runs yet" description="Runs appear here once an enabled agent is triggered." />}
          </div>
          <aside className="rounded-[14px] border border-border-primary p-4">
            <h2 className="font-semibold text-text-primary">Run timeline</h2>
            {selectedRun ? (
              <>
                <dl className="mt-3 grid grid-cols-2 gap-2 text-xs">
                  <dt className="text-text-tertiary">Status</dt><dd className="text-text-primary">{selectedRun.status}</dd>
                  <dt className="text-text-tertiary">Budget</dt><dd className="font-mono break-all text-text-primary">{JSON.stringify(selectedRun.budget)}</dd>
                  <dt className="text-text-tertiary">Snapshot</dt><dd className="font-mono break-all text-text-primary">{selectedRun.snapshot_sha ?? 'n/a'}</dd>
                </dl>
                <ol className="mt-4 space-y-2">{events.data?.map(event => <li key={event.sequence} className="text-xs"><span className="font-mono text-text-tertiary">#{event.sequence}</span> {event.kind}<pre className="mt-1 whitespace-pre-wrap break-all text-text-tertiary">{JSON.stringify(event.payload, null, 2)}</pre></li>)}</ol>
              </>
            ) : <EmptyState title="Select a run" description="Inspect receipts, budget consumption, and events." />}
          </aside>
        </div>
      )}

      {tab === 'findings' && (
        <div className="space-y-3">
          {findings.data?.map(finding => (
            <article key={finding.id} className="rounded-[14px] border border-border-primary p-4">
              <div className="flex justify-between gap-3">
                <h2 className="text-sm font-semibold text-text-primary">{finding.title}</h2>
                <div className="flex items-center gap-2 text-xs">
                  <Badge size="sm" variant="warning">{finding.severity}</Badge>
                  <Badge size="sm" variant="default">{finding.status}</Badge>
                  {can('autonomous_agent:update') && finding.status === 'open' && <button type="button" onClick={() => resolveFinding.mutate(finding.id)} className="text-accent-blue">Resolve</button>}
                </div>
              </div>
              <p className="text-sm text-text-tertiary mt-2">{finding.summary}</p>
              <details className="mt-2 text-xs">
                <summary className="cursor-pointer text-text-secondary">Evidence and deliveries</summary>
                {typeof finding.evidence?.screenshot_url === 'string' && (
                  <a href={finding.evidence.screenshot_url as string} target="_blank" rel="noreferrer" className="mt-2 block">
                    <img src={finding.evidence.screenshot_url as string} alt="QA evidence screenshot" className="max-h-64 rounded-lg border border-border-primary" />
                  </a>
                )}
                <pre className="whitespace-pre-wrap break-all mt-2 text-text-tertiary">{JSON.stringify(finding.evidence, null, 2)}</pre>
                {deliveries.data?.filter(item => item.finding_id === finding.id).map(item => (
                  <div key={item.id} className="mt-2 flex gap-2">
                    <span className="text-text-secondary">{item.channel}: {item.status}</span>
                    {item.external_url && <a href={item.external_url} target="_blank" rel="noreferrer" className="text-accent-blue">Open</a>}
                    {can('autonomous_agent:run') && ['slack', 'github_issue'].includes(item.channel) && ['failed', 'dead_letter'].includes(item.status) && <button type="button" onClick={() => retryDelivery.mutate(item.id)} className="text-accent-blue">Retry delivery</button>}
                  </div>
                ))}
              </details>
              <p className="text-xs text-text-tertiary mt-2">Seen {finding.occurrence_count} time(s)</p>
            </article>
          ))}
          {findings.data?.length === 0 && <EmptyState title="No findings yet" description="Confirmed findings from QA and review agents will land here." />}
        </div>
      )}

      {tab === 'runtime' && (
        <div className="grid gap-4 md:grid-cols-2">
          <section className="rounded-[14px] border border-border-primary p-5 space-y-4">
            <div className="flex items-center gap-3">
              <ShieldCheck className="w-5 h-5 text-text-secondary" />
              <div>
                <h2 className="font-semibold text-text-primary capitalize">{runtime.data?.status?.replace(/_/g, ' ') ?? 'Loading'}</h2>
                <p className="text-xs text-text-tertiary">{runtime.data?.claude_version ?? runtime.data?.reason_code ?? 'Checking local Claude Code runtime'}</p>
              </div>
            </div>
            {runtime.data?.status === 'reauth_required' && <p className="text-sm text-status-warning">Authenticate Claude Code again as the backend OS account, then check again. Schedules remain durable and leasing is paused.</p>}
            {can('autonomous_agent:enable') && <Button size="sm" variant="outline" leftIcon={<RefreshCw className="w-4 h-4" />} onClick={() => checkRuntime.mutate()}>Check again</Button>}
          </section>
          <section className="rounded-[14px] border border-border-primary p-5 space-y-3">
            <h2 className="font-semibold text-text-primary">Organization kill switch</h2>
            <p className="text-sm text-text-tertiary">{settings.data?.enabled ? 'Enabled' : 'Disabled'} · retention {settings.data?.retention_days ?? '—'} days</p>
            {can('autonomous_agent:enable') && (
              <>
                <Button size="sm" variant={settings.data?.enabled ? 'destructive' : 'secondary'} onClick={() => toggleOrg.mutate(!settings.data?.enabled)}>{settings.data?.enabled ? 'Disable all agents' : 'Enable agents'}</Button>
                <label className="block text-xs text-text-secondary">Retention days<input type="number" min={7} max={3650} defaultValue={settings.data?.retention_days} onBlur={event => saveRetention.mutate(Number(event.target.value))} className="mt-1 block rounded-lg border border-border-primary bg-transparent px-3 py-2 text-text-primary" /></label>
              </>
            )}
          </section>
          {metrics.data && (
            <section aria-label="Automation metrics" className="md:col-span-2 grid grid-cols-2 md:grid-cols-4 gap-3">
              {Object.entries(metrics.data).map(([label, value]) => (
                <div key={label} className="rounded-[12px] border border-border-primary p-3">
                  <div className="text-xs capitalize text-text-tertiary">{label.replace(/_/g, ' ')}</div>
                  <div className="mt-1 text-lg font-semibold text-text-primary">{typeof value === 'number' ? value.toLocaleString() : value}</div>
                </div>
              ))}
            </section>
          )}
        </div>
      )}

      {(showCreate || editing) && (
        <AutonomousAgentWizard
          open={showCreate || Boolean(editing)}
          editing={editing}
          templates={templates.data ?? []}
          onClose={() => { setShowCreate(false); setEditing(null) }}
        />
      )}

      <Modal open={Boolean(templateDetail)} onOpenChange={value => { if (!value) setTemplateDetail(null) }} size="lg">
        {templateDetail && (
          <>
            <ModalHeader>
              <ModalTitle>{templateDetail.name} · v{templateDetail.version}</ModalTitle>
            </ModalHeader>
            <ModalContent className="max-h-[65vh] overflow-y-auto space-y-4">
              <p className="text-sm text-text-secondary">{templateDetail.description}</p>
              <div>
                <p className="text-xs font-medium text-text-secondary">Workflow</p>
                <ol className="mt-2 space-y-1">{templateDetail.workflow.map((stepName, index) => <li key={stepName} className="flex items-center gap-2 text-sm text-text-primary"><span className="grid h-5 w-5 place-items-center rounded-full bg-white/[0.06] text-[10px] text-text-tertiary">{index + 1}</span>{stepName.replace(/_/g, ' ')}</li>)}</ol>
              </div>
              <div>
                <p className="text-xs font-medium text-text-secondary">Capabilities</p>
                <div className="mt-2 flex flex-wrap gap-1">{templateDetail.capabilities.map(cap => <Badge key={cap} size="sm" variant="info">{cap}</Badge>)}</div>
              </div>
              <div>
                <p className="text-xs font-medium text-text-secondary">Default budgets</p>
                <pre className="mt-2 overflow-auto rounded-lg bg-black/30 p-3 font-mono text-[11px] text-text-tertiary">{JSON.stringify(templateDetail.default_budgets, null, 2)}</pre>
              </div>
              <div>
                <p className="text-xs font-medium text-text-secondary">Configuration schema</p>
                <pre className="mt-2 overflow-auto rounded-lg bg-black/30 p-3 font-mono text-[11px] text-text-tertiary">{JSON.stringify(templateDetail.config_schema, null, 2)}</pre>
              </div>
              <p className="text-[11px] text-text-tertiary flex items-center gap-1"><X className="w-3 h-3" />Managed templates are versioned in the server and cannot be edited here — upgrades create a new revision and require revalidation.</p>
            </ModalContent>
          </>
        )}
      </Modal>
    </div>
  )
}
