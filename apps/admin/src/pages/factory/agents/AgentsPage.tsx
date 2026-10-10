import { useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { Bot, Plus } from 'lucide-react'
import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import { EmptyState } from '../../../components/ui/EmptyState'
import { Skeleton } from '../../../components/ui/Skeleton'
import { Switch } from '../../../components/ui/Switch'
import { ConfirmModal } from '../../../components/ConfirmModal'
import AutonomousAgentWizard from '../../AutonomousAgentWizard'
import type { AutonomousAgentDefinition, AutonomousAgentDetail, AutonomousAgentRun } from '../../../types'
import { runtimeStatusMeta, usd } from '../shared/format'
import { PauseAllControl, useFactorySettings, useRuntimeHealth } from '../shared/fleet'
import { InlineError, PageHeader, TEXT_LINK, errorMessage, useFactory } from '../shared/ui'
import { AgentCard, type AgentActions } from './AgentCard'
import { OutcomeLegend } from './OutcomeTape'
import { JudgeRunDialog, ReviewerRunDialog, type RunTarget } from './RunDialogs'

const LINK = TEXT_LINK

function Count({ value, label, alert }: { value?: number; label: string; alert?: 'warn' | 'bad' }) {
  const color = value && value > 0 && alert === 'bad' ? 'text-status-error' : value && value > 0 && alert === 'warn' ? 'text-status-warning' : 'text-text-primary'
  return (
    <div className="inline-flex min-w-0 items-center gap-2 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className={`rounded-md bg-muted px-2 py-0.5 font-semibold tabular-nums ${color}`}>{value ?? '—'}</span>
    </div>
  )
}

function Cell({ title, children, footer }: { title: string; children: React.ReactNode; footer?: React.ReactNode }) {
  return (
    <section aria-label={title} className="flex min-w-0 flex-col gap-2 bg-card p-3">
      <h2 className="text-[12px] font-medium text-text-tertiary">{title}</h2>
      <div className="flex-1">{children}</div>
      {footer && <div className="flex flex-wrap items-center gap-2">{footer}</div>}
    </section>
  )
}

/** /factory — fleet health strip and agent cards. */
export default function AgentsPage() {
  const { can, client, invalidate } = useFactory()
  const [showArchived, setShowArchived] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [editing, setEditing] = useState<AutonomousAgentDetail | null>(null)
  const [actionError, setActionError] = useState('')
  const [notice, setNotice] = useState('')
  const [runJudge, setRunJudge] = useState<AutonomousAgentDefinition | null>(null)
  const [runReviewer, setRunReviewer] = useState<AutonomousAgentDefinition | null>(null)
  const [archiving, setArchiving] = useState<AutonomousAgentDefinition | null>(null)

  const templates = useQuery({ queryKey: ['autonomous-templates'], queryFn: () => client.listAutonomousAgentTemplates() })
  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents() })
  const runs = useQuery({ queryKey: ['autonomous-runs'], queryFn: () => client.listAutonomousAgentRuns() })
  const metrics = useQuery({ queryKey: ['autonomous-metrics'], queryFn: () => client.getAutonomousAgentMetrics(), refetchInterval: 30_000 })
  const { settings, toggle } = useFactorySettings()
  const { runtime } = useRuntimeHealth()

  const fail = (fallback: string) => (value: unknown) => { setNotice(''); setActionError(errorMessage(value, fallback)) }
  const friendly = (message: string) => message === 'validation_required' ? 'Validate the agent before enabling it.' : message
  const useLifecycle = (fn: (id: string) => Promise<unknown>, fallback: string) => useMutation({
    mutationFn: fn,
    onSuccess: () => { setActionError(''); return invalidate('autonomous-agents') },
    onError: (value: unknown) => setActionError(friendly(errorMessage(value, fallback))),
  })
  const enable = useLifecycle(id => client.enableAutonomousAgent(id), 'Could not enable the agent.')
  const disable = useLifecycle(id => client.disableAutonomousAgent(id), 'Could not disable the agent.')
  const archive = useLifecycle(id => client.archiveAutonomousAgent(id), 'Could not archive the agent.')
  const validate = useMutation({
    mutationFn: (id: string) => client.validateAutonomousAgent(id),
    onSuccess: detail => {
      void invalidate('autonomous-agents')
      if (detail.revision.validation_status === 'valid') { setActionError(''); setNotice(`“${detail.name}” is valid and can be enabled.`); return }
      const errors = Array.isArray((detail.revision.validation as { errors?: unknown } | null)?.errors) ? ((detail.revision.validation as { errors: string[] }).errors) : []
      setActionError(errors.length ? `“${detail.name}” did not validate: ${errors.join(', ')}. Edit it and validate again.` : `“${detail.name}” did not validate. Review its configuration and validate again.`)
    },
    onError: fail('Validation failed. Try again.'),
  })
  const clone = useMutation({
    mutationFn: async (id: string) => { const source = await client.getAutonomousAgent(id); return client.createAutonomousAgent({ name: `${source.name} copy`, description: source.description ?? undefined, template_key: source.template_key, config: source.revision.config, budgets: source.revision.budgets }) },
    onSuccess: () => { setActionError(''); return invalidate('autonomous-agents') },
    onError: fail('Could not clone the agent.'),
  })
  const runNow = useMutation({
    mutationFn: (vars: { id: string; targets?: RunTarget[] }) => client.runAutonomousAgent(vars.id, vars.targets ? { targets: vars.targets } : undefined),
    onSuccess: () => { invalidate('autonomous-runs'); setRunJudge(null); setRunReviewer(null); setActionError(''); setNotice('Run queued. Follow it in Runs.') },
    onError: fail('Could not start the run.'),
  })

  const openEdit = async (agent: AutonomousAgentDefinition) => {
    try { setEditing(await client.getAutonomousAgent(agent.id)) } catch (value) { fail('Could not open the agent for editing.')(value) }
  }

  const actions: AgentActions = {
    onValidate: agent => validate.mutate(agent.id),
    onEnable: agent => enable.mutate(agent.id),
    onDisable: agent => disable.mutate(agent.id),
    onRun: agent => agent.template_key === 'judge' ? setRunJudge(agent) : agent.template_key === 'github_pr_reviewer' ? setRunReviewer(agent) : runNow.mutate({ id: agent.id }),
    onEdit: agent => void openEdit(agent),
    onClone: agent => clone.mutate(agent.id),
    onArchive: agent => setArchiving(agent),
    busy: {
      validating: validate.isPending ? validate.variables : undefined,
      enabling: enable.isPending ? enable.variables : undefined,
      running: runNow.isPending ? runNow.variables?.id : undefined,
    },
  }

  const allAgents = agents.data ?? []
  const archivedCount = allAgents.filter(agent => agent.status === 'archived').length
  const visibleAgents = showArchived ? allAgents : allAgents.filter(agent => agent.status !== 'archived')
  const enabledCount = allAgents.filter(agent => agent.status === 'enabled').length
  const runsByAgent = new Map<string, AutonomousAgentRun[]>()
  for (const run of runs.data ?? []) runsByAgent.set(run.definition_id, [...(runsByAgent.get(run.definition_id) ?? []), run])

  const m = metrics.data
  const runtimeMeta = runtimeStatusMeta(runtime.data?.status)
  const fleetOn = settings.data?.enabled

  return (
    <div className="mx-auto min-w-0 max-w-7xl space-y-6 p-6 md:p-8">
      <PageHeader
        title="Agents"
        subtitle="Claude Code agents that run on this server, on a schedule or when you start them."
        actions={<>
          <Link to="/factory/templates" className="inline-flex h-8 items-center rounded-md border border-border-primary bg-background px-3 text-sm font-medium text-text-secondary transition-colors hover:bg-foreground/5 hover:text-text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">Browse templates</Link>
          {can('autonomous_agent:create') && <Button size="sm" variant="primary" leftIcon={<Plus className="h-4 w-4" />} onClick={() => setShowCreate(true)}>New agent</Button>}
        </>}
      />

      {/* Fleet strip: one bordered row of four cells. */}
      <div className="grid gap-px overflow-hidden rounded-xl border border-border-primary bg-border-primary sm:grid-cols-2 xl:grid-cols-4">
        <Cell
          title="Fleet"
          footer={can('autonomous_agent:enable') && settings.isSuccess
            ? <PauseAllControl enabled={Boolean(fleetOn)} pending={toggle.isPending} onToggle={value => toggle.mutate(value)} error={toggle.error ?? undefined} />
            : undefined}
        >
          {settings.isLoading ? <Skeleton className="h-7 w-24" /> : settings.isError ? <p className="text-sm text-text-secondary">Could not load the fleet state.</p> : (
            <>
              <p className={`text-sm font-semibold leading-tight ${fleetOn ? 'text-text-primary' : 'text-status-warning'}`}>{fleetOn ? 'Running' : 'Paused'}</p>
              <p className="mt-1 text-[12px] text-text-tertiary">{agents.isSuccess ? `${enabledCount} of ${allAgents.length - archivedCount} agents enabled` : ' '}</p>
            </>
          )}
        </Cell>
        <Cell title="Right now" footer={<Link to="/factory/runs" className={LINK}>Open runs</Link>}>
          {metrics.isLoading ? <Skeleton className="h-10 w-40" /> : (
            <>
              <div className="flex flex-wrap gap-x-3 gap-y-2">
                <Count value={m?.queued} label="Queued" />
                <Count value={m?.running} label="Running" />
                <Count value={m?.blocked} label="Blocked" alert="bad" />
              </div>
              {typeof m?.estimated_cost_usd === 'number' && <p className="mt-2 text-[12px] text-text-tertiary">Spend to date {usd(m.estimated_cost_usd)}</p>}
            </>
          )}
        </Cell>
        <Cell title="Needs attention" footer={<Link to="/factory/findings" className={LINK}>Open findings</Link>}>
          {metrics.isLoading ? <Skeleton className="h-10 w-40" /> : (
            <>
              <div className="flex flex-wrap gap-x-3 gap-y-2">
                <Count value={m?.open_findings} label="Open findings" alert="warn" />
                <Count value={m?.failed_deliveries} label="Failed deliveries" alert="bad" />
              </div>
              {m && m.dead_letters > 0 && <p className="mt-2 text-[12px] text-status-error">{m.dead_letters} gave up after retries</p>}
            </>
          )}
        </Cell>
        <Cell title="Runtime" footer={<Link to="/factory/settings" className={LINK}>Factory settings</Link>}>
          {runtime.isLoading ? <Skeleton className="h-7 w-32" /> : runtime.isError ? <p className="text-sm text-text-secondary">Could not check the runtime.</p> : (
            <>
              <Badge size="md" variant={runtimeMeta.variant} dot>{runtimeMeta.label}</Badge>
              <p className="mt-2 text-[12px] text-text-tertiary">{runtime.data?.claude_version ? `Claude Code ${runtime.data.claude_version}` : 'Claude Code runtime on this server'}</p>
            </>
          )}
        </Cell>
      </div>
      {metrics.isError && <InlineError message="Could not load fleet metrics." onRetry={() => void metrics.refetch()} />}

      {actionError && <InlineError message={actionError} onDismiss={() => setActionError('')} />}
      {notice && !actionError && (
        <div role="status" className="flex flex-wrap items-center gap-3 rounded-md border border-border-primary bg-foreground/[0.03] px-4 py-3 text-sm text-text-primary">
          <span className="flex-1">{notice}</span>
          {notice.startsWith('Run queued') && <Link to="/factory/runs" className={LINK}>Open runs</Link>}
          <Button size="sm" variant="ghost" onClick={() => setNotice('')}>Dismiss</Button>
        </div>
      )}

      <section aria-labelledby="agents-list-title" className="space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 id="agents-list-title" className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">
            {showArchived ? 'All agents' : 'Active agents'}{agents.isSuccess && <span className="ml-2 text-sm font-normal text-text-tertiary tabular-nums">{visibleAgents.length}</span>}
          </h2>
          {archivedCount > 0 && <Switch size="sm" checked={showArchived} onCheckedChange={setShowArchived} label={`Show archived (${archivedCount})`} />}
        </div>

        {agents.isLoading && (
          <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,280px),1fr))] gap-4" aria-busy="true" aria-label="Loading agents">
            {[0, 1, 2].map(i => (
              <div key={i} className="space-y-4 rounded-xl border border-border-primary p-5">
                <Skeleton className="h-4 w-40" /><Skeleton className="h-3 w-24" /><Skeleton className="h-10 w-full" /><Skeleton className="h-2.5 w-48" />
              </div>
            ))}
          </div>
        )}
        {agents.isError && <InlineError message="Could not load agents." onRetry={() => void agents.refetch()} />}
        {runs.isError && <InlineError message="Could not load recent runs, so outcome tapes are empty." onRetry={() => void runs.refetch()} />}

        {agents.isSuccess && visibleAgents.length > 0 && (
          <>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,280px),1fr))] gap-4">
              {visibleAgents.map(agent => <AgentCard key={agent.id} agent={agent} runs={runsByAgent.get(agent.id) ?? []} actions={actions} />)}
            </div>
            <OutcomeLegend />
          </>
        )}
        {agents.isSuccess && visibleAgents.length === 0 && (
          <EmptyState
            icon={<Bot className="h-6 w-6" />}
            title={allAgents.length === 0 ? 'No agents yet' : 'No active agents'}
            description={allAgents.length === 0 ? 'An agent runs a managed template — QA, issue resolver, PR reviewer and more — against your repositories.' : 'Every agent is archived. Turn on “Show archived” to see them.'}
            action={can('autonomous_agent:create') && allAgents.length === 0 ? <Button size="sm" variant="primary" leftIcon={<Plus className="h-4 w-4" />} onClick={() => setShowCreate(true)}>New agent</Button> : undefined}
          />
        )}
      </section>

      {(showCreate || editing) && (
        <AutonomousAgentWizard
          open={showCreate || Boolean(editing)}
          editing={editing}
          templates={templates.data ?? []}
          onClose={() => { setShowCreate(false); setEditing(null) }}
        />
      )}

      {runJudge && <JudgeRunDialog agent={runJudge} pending={runNow.isPending} onClose={() => setRunJudge(null)} onRun={targets => runNow.mutate({ id: runJudge.id, targets })} />}
      {runReviewer && <ReviewerRunDialog agent={runReviewer} pending={runNow.isPending} onClose={() => setRunReviewer(null)} onRun={target => runNow.mutate({ id: runReviewer.id, targets: [target] })} />}

      <ConfirmModal
        open={Boolean(archiving)}
        danger
        title={`Archive “${archiving?.name ?? ''}”?`}
        description="It leaves the fleet and cannot be enabled again; you can still clone it. Its runs and findings are kept."
        confirmLabel="Archive agent"
        loading={archive.isPending}
        onClose={() => setArchiving(null)}
        onConfirm={() => { if (archiving) archive.mutate(archiving.id); setArchiving(null) }}
      />
    </div>
  )
}
