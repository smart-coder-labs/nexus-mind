import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { Activity, AlertTriangle, CheckCircle2, DollarSign, ListChecks, Play, Settings2 } from 'lucide-react'
import { ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/shadcn/chart'
import { MetricSummary } from '../../components/ui/MetricSummary'
import { Badge } from '../../components/ui/Badge'
import { addDaysIso, formatUsd, todayIso } from '../usage/format'
import { InlineError, PageHeader, RunStatusPill, SectionTitle, TEXT_LINK, useFactory } from './shared/ui'
import { useRuntimeHealth } from './shared/fleet'
import { dur, plural, relativeTime, runDurationSeconds, runtimeStatusMeta } from './shared/format'
import { dailyOutcomes, successRate } from './home/summary'

const DAYS = 14
const SEVERITIES = ['critical', 'high', 'medium', 'low'] as const
const SEVERITY_WORD: Record<string, string> = { critical: 'críticos', high: 'altos', medium: 'medios', low: 'bajos' }
const chartConfig = {
  success: { label: 'Correctas', color: 'var(--color-status-success)' },
  warning: { label: 'Parciales o sin presupuesto', color: 'var(--color-status-warning)' },
  error: { label: 'Fallidas o bloqueadas', color: 'var(--color-status-error)' },
  neutral: { label: 'Canceladas', color: 'var(--color-text-tertiary)' },
}
const dash = '—'
const dayLabel = (day: string) =>
  new Date(`${day}T00:00:00Z`).toLocaleDateString([], { day: 'numeric', month: 'short', timeZone: 'UTC' })

export default function FactoryHome() {
  const { can, client } = useFactory()
  const agentRead = can('autonomous_agent:read')
  const native = can('factory_policy:read')
  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents(), enabled: agentRead })
  const runs = useQuery({ queryKey: ['autonomous-runs'], queryFn: () => client.listAutonomousAgentRuns(), enabled: agentRead })
  const metrics = useQuery({ queryKey: ['autonomous-metrics'], queryFn: () => client.getAutonomousAgentMetrics(), enabled: agentRead })
  const findings = useQuery({ queryKey: ['autonomous-findings'], queryFn: () => client.listAutonomousAgentFindings(), enabled: agentRead })
  const to = todayIso()
  const usage = useQuery({
    queryKey: ['usage-summary', 'agent', 'factory-30d', to],
    queryFn: () => client.getUsageSummary({ level: 'agent', from: addDaysIso(to, -29), to }),
    enabled: agentRead,
    retry: false,
  })
  const { runtime } = useRuntimeHealth()

  if (!native && !agentRead)
    return (
      <div className="p-6">
        <InlineError message="Necesitas permiso para consultar Factory." />
      </div>
    )

  const allRuns = runs.data ?? []
  const names = new Map((agents.data ?? []).map(agent => [agent.id, agent.name]))
  const week = successRate(allRuns, 7)
  const days = dailyOutcomes(allRuns, DAYS)
  const finishedInWindow = days.reduce((sum, d) => sum + d.success + d.warning + d.error + d.neutral, 0)
  const recent = allRuns.filter(run => !run.archived_at).slice(0, 8)
  const open = (findings.data ?? []).filter(f => f.status === 'open')
  const m = metrics.data
  const attention = m ? m.blocked + m.failed_deliveries + m.dead_letters : 0
  const costRows = (usage.data?.rows ?? []).filter(row => row.key_id && row.cost_usd > 0).sort((a, b) => b.cost_usd - a.cost_usd).slice(0, 6)
  const maxCost = costRows[0]?.cost_usd ?? 0
  const enabledAgents = (agents.data ?? []).filter(a => a.status === 'enabled').length
  const rt = runtimeStatusMeta(runtime.data?.status)

  return (
    <div className="mx-auto max-w-7xl space-y-8 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Factory"
        subtitle="De una tarea a cambios verificados: qué está corriendo, cómo va y cuánto cuesta."
        actions={
          <>
            {runtime.isSuccess && <Badge variant={rt.variant}>Runtime · {rt.label}</Badge>}
            <Link className="inline-flex h-9 items-center gap-2 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring" to="/factory/runs">
              <Play className="size-4" aria-hidden />
              Ver ejecuciones
            </Link>
          </>
        }
      />

      {agentRead && (
        <>
          {(runs.isError || metrics.isError) && (
            <InlineError message="No se pudieron cargar los datos de Factory." onRetry={() => { void runs.refetch(); void metrics.refetch() }} />
          )}

          <section aria-label="Indicadores" className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <MetricSummary label="En curso" icon={Activity} accent="var(--d2)" value={m ? m.running + m.queued : dash}>
              <span>{m ? `${m.running} corriendo · ${m.queued} en cola` : 'Cargando…'}</span>
            </MetricSummary>
            <MetricSummary label="Éxito · 7 días" icon={CheckCircle2} accent="var(--d1)" value={week.rate === null ? dash : `${Math.round(week.rate * 100)}%`}>
              <span>{runs.isSuccess ? `${week.succeeded} de ${plural(week.finished, 'ejecución terminada', 'ejecuciones terminadas')}` : 'Cargando…'}</span>
            </MetricSummary>
            <MetricSummary label="Coste · 30 días" icon={DollarSign} accent="var(--d4)" value={usage.isSuccess ? formatUsd(usage.data.totals.cost_usd) : dash}>
              <span>
                {usage.isSuccess
                  ? `${usage.data.totals.tokens_total.toLocaleString()} tokens`
                  : m ? `${formatUsd(m.estimated_cost_usd)} desde el inicio` : 'Cargando…'}
              </span>
            </MetricSummary>
            <MetricSummary label="Findings abiertos" icon={ListChecks} accent="var(--d3)" value={findings.isSuccess ? open.length : m ? m.open_findings : dash}>
              <span>
                {findings.isSuccess
                  ? SEVERITIES.map(s => [s, open.filter(f => f.severity === s).length] as const).filter(([, n]) => n > 0).map(([s, n]) => `${n} ${SEVERITY_WORD[s]}`).join(' · ') || 'Sin severidad alta'
                  : 'Cargando…'}
              </span>
            </MetricSummary>
          </section>

          {attention > 0 && m && (
            <Link to="/factory/inbox" className="flex flex-wrap items-center gap-3 rounded-xl border border-status-warning/30 bg-status-warning/[0.08] px-4 py-3 text-sm text-text-primary hover:bg-status-warning/[0.12] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
              <AlertTriangle className="size-4 shrink-0 text-status-warning" aria-hidden />
              <span className="min-w-0 flex-1">
                Necesita atención: {[
                  m.blocked && plural(m.blocked, 'ejecución bloqueada', 'ejecuciones bloqueadas'),
                  m.failed_deliveries && plural(m.failed_deliveries, 'entrega fallida', 'entregas fallidas'),
                  m.dead_letters && plural(m.dead_letters, 'abandono tras reintentos', 'abandonos tras reintentos'),
                ].filter(Boolean).join(' · ')}
              </span>
              <span className="font-medium">Abrir pendientes →</span>
            </Link>
          )}

          <section className="glass-1 rounded-2xl p-4 sm:p-6" aria-labelledby="factory-activity">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
              <SectionTitle id="factory-activity">Ejecuciones terminadas · últimos {DAYS} días</SectionTitle>
              <span className="text-[12px] text-text-tertiary tabular-nums">{runs.isSuccess ? plural(finishedInWindow, 'ejecución', 'ejecuciones') : ''}</span>
            </div>
            {runs.isSuccess ? (
              <div role="figure" aria-label={`Ejecuciones terminadas por día y resultado, ${DAYS} días, ${finishedInWindow} en total.`} className="mt-4">
                <ChartContainer config={chartConfig} className="h-[220px] w-full aspect-auto">
                  <BarChart accessibilityLayer data={days} margin={{ left: 0, right: 4, top: 8, bottom: 0 }}>
                    <CartesianGrid vertical={false} />
                    <XAxis dataKey="day" tickFormatter={dayLabel} axisLine={false} tickLine={false} tickMargin={10} minTickGap={28} />
                    <YAxis width={32} axisLine={false} tickLine={false} allowDecimals={false} />
                    <ChartTooltip content={<ChartTooltipContent labelFormatter={(_, payload) => dayLabel(payload[0]?.payload.day ?? '')} />} />
                    {(['success', 'warning', 'error', 'neutral'] as const).map((key, i, all) => (
                      <Bar key={key} dataKey={key} stackId="runs" fill={`var(--color-${key})`} maxBarSize={28} isAnimationActive={false} radius={i === all.length - 1 ? [4, 4, 0, 0] : 0} />
                    ))}
                  </BarChart>
                </ChartContainer>
                <ul aria-label="Leyenda" className="mt-3 flex list-none flex-wrap justify-center gap-x-6 gap-y-1 p-0 text-xs text-text-secondary">
                  {Object.entries(chartConfig).map(([key, item]) => (
                    <li key={key} className="inline-flex items-center gap-2">
                      <span aria-hidden className="h-2.5 w-2.5 rounded-[2px]" style={{ backgroundColor: item.color }} />
                      {item.label}
                    </li>
                  ))}
                </ul>
              </div>
            ) : (
              <div className="mt-4 h-[220px] animate-pulse rounded-lg bg-muted motion-reduce:animate-none" aria-hidden />
            )}
          </section>

          <div className="grid gap-6 lg:grid-cols-5">
            <section className="glass-1 rounded-2xl p-4 sm:p-6 lg:col-span-3" aria-labelledby="factory-recent">
              <div className="flex items-baseline justify-between gap-2">
                <SectionTitle id="factory-recent">Últimas ejecuciones</SectionTitle>
                <Link className={TEXT_LINK} to="/factory/runs">Ver todas</Link>
              </div>
              {runs.isSuccess && recent.length === 0 && <p className="mt-4 text-sm text-text-secondary">Todavía no hay ejecuciones.</p>}
              <ul className="mt-3 divide-y divide-border-secondary">
                {recent.map(run => {
                  const seconds = runDurationSeconds(run)
                  return (
                    <li key={run.id}>
                      <Link
                        to={`/factory/runs?agent_run=${encodeURIComponent(run.id)}`}
                        className="flex items-center gap-3 rounded-md py-2.5 hover:bg-foreground/[0.03] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring"
                      >
                        <RunStatusPill status={run.status} />
                        <span className="min-w-0 flex-1 truncate text-sm text-text-primary">{names.get(run.definition_id) ?? 'Agente eliminado'}</span>
                        <span className="hidden text-[12px] tabular-nums text-text-tertiary sm:inline">{seconds === undefined ? '' : dur(seconds)}</span>
                        <span className="w-24 shrink-0 text-right text-[12px] text-text-tertiary">{relativeTime(run.created_at)}</span>
                      </Link>
                    </li>
                  )
                })}
              </ul>
            </section>

            <section className="glass-1 rounded-2xl p-4 sm:p-6 lg:col-span-2" aria-labelledby="factory-cost">
              <div className="flex items-baseline justify-between gap-2">
                <SectionTitle id="factory-cost">Coste por agente · 30 días</SectionTitle>
                <Link className={TEXT_LINK} to="/usage">Usage</Link>
              </div>
              {usage.isError && <p className="mt-4 text-sm text-text-secondary">El desglose de coste requiere acceso a Usage.</p>}
              {usage.isSuccess && costRows.length === 0 && <p className="mt-4 text-sm text-text-secondary">Sin coste registrado en 30 días.</p>}
              <ul className="mt-4 space-y-3">
                {costRows.map(row => (
                  <li key={row.key_id}>
                    <div className="flex items-baseline justify-between gap-3 text-sm">
                      <span className="min-w-0 truncate text-text-primary">{row.key_name}</span>
                      <span className="shrink-0 tabular-nums text-text-secondary">{formatUsd(row.cost_usd)}</span>
                    </div>
                    <div className="mt-1.5 h-1.5 rounded-full bg-foreground/[0.06]" aria-hidden>
                      <div className="sig-bar h-full rounded-full" style={{ width: `${Math.max(2, (row.cost_usd / maxCost) * 100)}%` }} />
                    </div>
                  </li>
                ))}
              </ul>
            </section>
          </div>
        </>
      )}

      <footer className="flex flex-wrap items-center gap-x-6 gap-y-3 border-t border-border pt-5 text-sm">
        {native && (
          <>
            <Link className={TEXT_LINK} to="/factory/intake">Entradas</Link>
            <Link className={TEXT_LINK} to="/factory/inbox">Pendientes</Link>
          </>
        )}
        {agentRead && (
          <Link className={TEXT_LINK} to="/factory/agents">
            Catálogo de agentes{agents.isSuccess ? ` · ${enabledAgents} activos de ${agents.data.length}` : ''}
          </Link>
        )}
        <Link className="ml-auto inline-flex items-center gap-2 text-text-secondary underline underline-offset-4" to="/factory/settings">
          <Settings2 className="size-4" aria-hidden />
          Infraestructura y permisos
        </Link>
      </footer>
    </div>
  )
}
