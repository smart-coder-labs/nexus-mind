import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Activity, CheckCircle2, CircleAlert, FlaskConical, Play, RefreshCw } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { cn } from '../lib/utils'
import type { FactoryEvaluationRun, FactoryEvaluationSet } from '../types'
import { InlineAlert, PageHeader, PANEL_CLASS, PermissionDenied, SectionHeading } from './factory/govern/ui'

const number = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1, style: 'percent' })
const money = new Intl.NumberFormat(undefined, { style: 'currency', currency: 'USD', maximumFractionDigits: 2 })

function statusVariant(status: string): 'default' | 'success' | 'warning' | 'error' {
  if (status === 'complete') return 'success'
  if (status === 'failed' || status === 'missing') return 'error'
  if (status === 'running' || status === 'queued') return 'warning'
  return 'default'
}

function timestamp(value: string | null): string {
  if (!value) return '—'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString()
}

function SetResult({ set }: { set: FactoryEvaluationSet }) {
  const summary = set.summary
  const baseline = summary?.arms.find(arm => arm.arm === 'baseline')
  const specialist = summary?.arms.find(arm => arm.arm === 'specialist')
  return (
    <article className={cn(PANEL_CLASS, 'space-y-4')}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-text-primary">{set.name}</h3>
          <p className="mt-1 text-xs text-text-tertiary">
            {set.task_count === null ? 'Task count unavailable' : `${set.completed_tasks} of ${set.task_count} tasks complete`}
            {set.base_commit && <><span aria-hidden="true"> · </span><code>{set.base_commit.slice(0, 12)}</code></>}
          </p>
        </div>
        <Badge role="status" variant={statusVariant(set.status)}>{set.status === 'ready' ? 'Ready' : 'Dataset missing'}</Badge>
      </div>
      {summary ? (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2" aria-label={`${set.name} results`}>
          {[baseline, specialist].map((arm, index) => (
            <div key={arm?.arm ?? index} className="rounded-lg border border-border-secondary bg-foreground/[0.02] p-3">
              <div className="text-xs font-medium uppercase tracking-wide text-text-tertiary">{arm?.arm ?? (index ? 'Specialist' : 'Baseline')}</div>
              {arm ? <div className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
                <span className="text-lg font-semibold tabular-nums text-text-primary">{arm.accepted}/{arm.tasks}</span>
                <span className="text-sm text-text-secondary">{number.format(arm.acceptance_rate)} accepted</span>
                <span className="ml-auto text-xs text-text-tertiary">{arm.cost_usd === null ? 'Cost unavailable' : money.format(arm.cost_usd)}</span>
              </div> : <p className="mt-2 text-sm text-text-tertiary">No result yet</p>}
            </div>
          ))}
        </div>
      ) : <p className="text-sm text-text-secondary">No evaluation summary is available yet.</p>}
      {summary && <p className="text-xs text-text-secondary">
        Specialist vs baseline: {summary.specialist_beats_baseline === null ? 'not enough completed results' : summary.specialist_beats_baseline ? 'specialist ahead' : 'specialist has not surpassed baseline'}
      </p>}
    </article>
  )
}

function RunRow({ run }: { run: FactoryEvaluationRun }) {
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3 sm:px-5">
      {run.status === 'complete' ? <CheckCircle2 aria-hidden="true" className="h-4 w-4 text-status-success" /> : run.status === 'failed' ? <CircleAlert aria-hidden="true" className="h-4 w-4 text-status-error" /> : <Activity aria-hidden="true" className="h-4 w-4 text-text-secondary" />}
      <code className="font-mono text-xs text-text-secondary">{run.id}</code>
      <Badge role="status" variant={statusVariant(run.status)}>{run.status}</Badge>
      <span className="text-xs text-text-tertiary">{run.set_ids.join(', ') || 'No sets'}</span>
      <time className="ml-auto text-xs text-text-tertiary" dateTime={run.updated_at}>{timestamp(run.updated_at)}</time>
      {run.error && <p className="basis-full text-sm text-status-error">{run.error}</p>}
    </li>
  )
}

/** Frozen F4 specialist-vs-baseline evaluations. The backend owns execution and persisted output. */
export default function FactoryEvaluations() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const [selected, setSelected] = useState<string[]>([])
  const evaluations = useQuery({
    queryKey: ['factory-evaluations'],
    queryFn: () => client.listFactoryEvaluations(),
    enabled: canRead,
    refetchInterval: query => query.state.data?.runs.some(run => run.status === 'running' || run.status === 'queued') ? 10_000 : false,
  })
  const start = useMutation({
    mutationFn: () => client.startFactoryEvaluations({ set_ids: selected }),
    onSuccess: () => {
      setSelected([])
      queryClient.invalidateQueries({ queryKey: ['factory-evaluations'] })
    },
  })

  if (!canRead) return <PermissionDenied title="Evaluaciones" permission="factory_policy:read" what="see the factory specialist evaluations" />

  const sets = evaluations.data?.sets ?? []
  const ready = sets.filter(set => set.status === 'ready')
  const toggle = (id: string) => setSelected(current => current.includes(id) ? current.filter(value => value !== id) : [...current, id])

  return (
    <div className="mx-auto min-w-0 max-w-7xl space-y-6 p-6 md:p-8">
      <PageHeader
        title="Evaluaciones"
        aside={<Badge role="note" variant="default">F4 · eval congelado</Badge>}
        subtitle="Compara cada specialist con el baseline en los conjuntos congelados. Las ejecuciones y sus resultados se guardan en el runner del backend."
        action={<Button variant="secondary" onClick={() => evaluations.refetch()} disabled={evaluations.isFetching}><RefreshCw className="mr-2 h-4 w-4" aria-hidden="true" />Actualizar</Button>}
      />

      {evaluations.isError && <InlineAlert onRetry={() => evaluations.refetch()}>No se pudieron cargar las evaluaciones. Comprueba que el backend tenga implementado GET /v1/factory/evaluations.</InlineAlert>}
      {start.isError && <InlineAlert onRetry={() => start.reset()}>No se pudo iniciar la evaluación: {(start.error as Error).message}. Comprueba POST /v1/factory/evaluations.</InlineAlert>}

      <section aria-labelledby="frozen-sets-title" className="space-y-3">
        <SectionHeading id="frozen-sets-title" title="Conjuntos congelados" description="Selecciona los conjuntos preparados para iniciar o reanudar una comparación. El backend informa cuáles están disponibles." />
        {evaluations.isLoading ? <div className="grid gap-3 md:grid-cols-3" aria-label="Cargando conjuntos"><div className={cn(PANEL_CLASS, 'h-32 animate-pulse')} /><div className={cn(PANEL_CLASS, 'h-32 animate-pulse')} /><div className={cn(PANEL_CLASS, 'h-32 animate-pulse')} /></div> : sets.length === 0 ? (
          <EmptyState icon={<FlaskConical />} title="No hay conjuntos disponibles" description="El backend aún no ha publicado los conjuntos congelados de docs, tests y UI." />
        ) : (
          <div className="grid gap-3 md:grid-cols-3">
            {sets.map(set => {
              const checked = selected.includes(set.id)
              return <label key={set.id} className={cn(PANEL_CLASS, 'flex cursor-pointer items-start gap-3 transition-colors', set.status === 'missing' && 'cursor-not-allowed opacity-60', checked && 'border-accent/50')}>
                <input type="checkbox" className="mt-1 accent-accent" checked={checked} disabled={!canWrite || set.status !== 'ready' || start.isPending} onChange={() => toggle(set.id)} aria-label={`Seleccionar ${set.name}`} />
                <span className="min-w-0 flex-1"><span className="flex flex-wrap items-center justify-between gap-2"><span className="text-sm font-semibold text-text-primary">{set.name}</span><Badge role="status" variant={statusVariant(set.status)}>{set.status === 'ready' ? 'Ready' : 'Missing'}</Badge></span>
                  <span className="mt-2 block text-sm text-text-secondary">{set.task_count === null ? 'El backend no informó cuántas tareas hay.' : `${set.task_count} tareas congeladas`}</span>
                  {set.base_commit && <code className="mt-2 block text-xs text-text-tertiary">Base {set.base_commit.slice(0, 12)}</code>}
                </span>
              </label>
            })}
          </div>
        )}
        {canWrite && <div className="flex flex-wrap items-center gap-3">
          <Button onClick={() => start.mutate()} disabled={!selected.length || start.isPending || !ready.length}><Play className="mr-2 h-4 w-4" aria-hidden="true" />{start.isPending ? 'Iniciando…' : 'Iniciar / reanudar'}</Button>
          <span className="text-xs text-text-tertiary">La ejecución la controla el backend y puede continuar tras límites del proveedor.</span>
        </div>}
      </section>

      <section aria-labelledby="results-title" className="space-y-3">
        <SectionHeading id="results-title" title="Resultados por specialist" description="La salida F4 solo se cumple cuando cada specialist supera al baseline según la regla de coste y aceptación." />
        {evaluations.isLoading ? <div className={cn(PANEL_CLASS, 'h-28 animate-pulse')} aria-label="Cargando resultados" /> : sets.some(set => set.summary) ? <div className="grid gap-3 md:grid-cols-2">{sets.filter(set => set.summary).map(set => <SetResult key={set.id} set={set} />)}</div> : <EmptyState icon={<FlaskConical />} title="Todavía no hay resultados" description="Cuando el runner publique resúmenes, la comparación de baseline y specialist aparecerá aquí." />}
      </section>

      <section aria-labelledby="history-title" className="space-y-3">
        <SectionHeading id="history-title" title="Ejecuciones recientes" description="Estado del runner y conjuntos solicitados." />
        {evaluations.isLoading ? <div className={cn(PANEL_CLASS, 'h-20 animate-pulse')} aria-label="Cargando ejecuciones" /> : (evaluations.data?.runs.length ?? 0) ? <ul className={cn(PANEL_CLASS, 'm-0 list-none divide-y divide-border-secondary p-0')}>{evaluations.data!.runs.map(run => <RunRow key={run.id} run={run} />)}</ul> : <EmptyState icon={<Activity />} title="Sin ejecuciones" description="Las ejecuciones iniciadas aparecerán aquí." />}
      </section>
    </div>
  )
}
