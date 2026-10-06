import { useQuery } from '@tanstack/react-query'
import { Archive, Copy, Pencil, Play, PowerOff, ShieldCheck } from 'lucide-react'
import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import type { AutonomousAgentDefinition, AutonomousAgentRun, AutonomousAgentSchedule } from '../../../types'
import { fmtDateTime, parseUtc, relativeTime, runStatusMeta, templateName, type BadgeTone } from '../shared/format'
import { OverflowMenu, useFactory, type MenuItem } from '../shared/ui'
import { OutcomeTape } from './OutcomeTape'

export function agentStatus(agent: AutonomousAgentDefinition): { label: string; variant: BadgeTone } {
  if (agent.status === 'archived') return { label: 'Archived', variant: 'default' }
  if (agent.status === 'enabled') return { label: 'Enabled', variant: 'success' }
  if (agent.validation_status === 'invalid') return { label: 'Invalid', variant: 'error' }
  if (agent.validation_status === 'pending') return { label: 'Needs validation', variant: 'warning' }
  return { label: 'Disabled', variant: 'default' }
}

function intervalWords(minutes: number): string {
  if (minutes % 1440 === 0) { const d = minutes / 1440; return d === 1 ? 'Every day' : `Every ${d} days` }
  if (minutes % 60 === 0) { const h = minutes / 60; return h === 1 ? 'Every hour' : `Every ${h} hours` }
  return `Every ${minutes} minutes`
}

export function scheduleWords(schedule?: AutonomousAgentSchedule | null): string {
  if (!schedule || schedule.kind === 'manual') return 'Runs on demand'
  if (!schedule.enabled) return 'Schedule paused'
  const base = schedule.kind === 'daily'
    ? `Daily at ${schedule.expression ?? '—'}${schedule.timezone ? ` (${schedule.timezone})` : ''}`
    : schedule.kind === 'interval' && Number(schedule.expression) > 0
      ? intervalWords(Number(schedule.expression))
      : 'Scheduled'
  return base
}

export type AgentActions = {
  onValidate: (agent: AutonomousAgentDefinition) => void
  onEnable: (agent: AutonomousAgentDefinition) => void
  onDisable: (agent: AutonomousAgentDefinition) => void
  onRun: (agent: AutonomousAgentDefinition) => void
  onEdit: (agent: AutonomousAgentDefinition) => void
  onClone: (agent: AutonomousAgentDefinition) => void
  onArchive: (agent: AutonomousAgentDefinition) => void
  busy: { validating?: string; enabling?: string; running?: string }
}

export function AgentCard({ agent, runs, actions }: { agent: AutonomousAgentDefinition; runs: AutonomousAgentRun[]; actions: AgentActions }) {
  const { can, client } = useFactory()
  const status = agentStatus(agent)
  // The schedule lives on its own endpoint (not in the list payload); a 404 means
  // the agent has no schedule, i.e. it runs on demand.
  const schedule = useQuery({
    queryKey: ['autonomous-agent-schedule', agent.id],
    queryFn: () => client.getAutonomousAgentSchedule(agent.id),
    enabled: agent.status !== 'archived',
    retry: false,
    staleTime: 60_000,
  })
  const last = [...runs].sort((a, b) => parseUtc(b.created_at) - parseUtc(a.created_at))[0]
  const lastMeta = last ? runStatusMeta(last.status) : undefined

  const canEnable = can('autonomous_agent:enable')
  const canRun = can('autonomous_agent:run')
  const canUpdate = can('autonomous_agent:update')
  const canCreate = can('autonomous_agent:create')

  // One primary action, chosen by what the agent needs next.
  let primary: { label: string; icon?: React.ReactNode; onClick: () => void; loading?: boolean } | null = null
  if (agent.status === 'disabled' && canEnable) {
    primary = agent.validation_status === 'valid'
      ? { label: 'Enable', onClick: () => actions.onEnable(agent), loading: actions.busy.enabling === agent.id }
      : { label: 'Validate', icon: <ShieldCheck className="h-3.5 w-3.5" />, onClick: () => actions.onValidate(agent), loading: actions.busy.validating === agent.id }
  } else if (agent.status === 'enabled' && canRun) {
    primary = { label: 'Run', icon: <Play className="h-3.5 w-3.5" />, onClick: () => actions.onRun(agent), loading: actions.busy.running === agent.id }
  }

  const menu: MenuItem[] = []
  if (agent.status === 'disabled' && canEnable && agent.validation_status === 'valid') menu.push({ label: 'Validate again', icon: <ShieldCheck className="h-3.5 w-3.5" />, onSelect: () => actions.onValidate(agent) })
  if (canCreate) menu.push({ label: 'Clone', icon: <Copy className="h-3.5 w-3.5" />, onSelect: () => actions.onClone(agent) })
  if (agent.status === 'enabled' && canEnable) menu.push({ label: 'Disable', icon: <PowerOff className="h-3.5 w-3.5" />, onSelect: () => actions.onDisable(agent) })
  if (agent.status === 'disabled' && canUpdate) menu.push({ label: 'Archive', icon: <Archive className="h-3.5 w-3.5" />, danger: true, onSelect: () => actions.onArchive(agent) })

  const nextRun = schedule.data?.enabled && schedule.data.kind !== 'manual' && agent.status === 'enabled' ? fmtDateTime(schedule.data.next_run_at) : ''

  return (
    <article aria-labelledby={`agent-${agent.id}`} className="flex flex-col gap-4 rounded-[18px] border border-border-primary bg-white/[0.02] p-5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 id={`agent-${agent.id}`} className="truncate text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{agent.name}</h2>
          <p className="mt-0.5 text-[12px] text-text-tertiary">{templateName(agent.template_key)}</p>
        </div>
        <Badge size="sm" variant={status.variant} dot className="shrink-0">{status.label}</Badge>
      </div>

      {agent.description && <p className="line-clamp-2 text-[13px] leading-relaxed text-text-secondary">{agent.description}</p>}

      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[13px]">
        <dt className="text-text-tertiary">Schedule</dt>
        <dd className="min-w-0 text-text-secondary">
          {schedule.isLoading ? <span className="text-text-tertiary">Loading…</span> : scheduleWords(schedule.data)}
          {nextRun && <span className="block text-[12px] text-text-tertiary">Next run {nextRun}</span>}
        </dd>
        <dt className="text-text-tertiary">Last run</dt>
        <dd className="text-text-secondary">
          {last && lastMeta ? <>{lastMeta.label} <span className="text-text-tertiary">{relativeTime(last.created_at)}</span></> : <span className="text-text-tertiary">No runs yet</span>}
        </dd>
      </dl>

      <OutcomeTape runs={runs} />

      {(primary || (canUpdate && agent.status !== 'archived') || menu.length > 0) && (
        <div className="mt-auto flex flex-wrap items-center gap-2 border-t border-border-secondary pt-4">
          {primary && <Button size="sm" variant="primary" leftIcon={primary.icon} loading={primary.loading} onClick={primary.onClick}>{primary.label}</Button>}
          {canUpdate && agent.status !== 'archived' && <Button size="sm" variant="secondary" leftIcon={<Pencil className="h-3.5 w-3.5" />} onClick={() => actions.onEdit(agent)}>Edit</Button>}
          <div className="ml-auto"><OverflowMenu label={`More actions for ${agent.name}`} items={menu} /></div>
        </div>
      )}
    </article>
  )
}
