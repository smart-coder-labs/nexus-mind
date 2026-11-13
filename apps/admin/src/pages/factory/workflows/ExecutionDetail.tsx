import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import { RawJson } from '../shared/ui'
import { useState } from 'react'
import { Link } from 'react-router-dom'
import { StepObservability } from './StepObservability'
import type { WorkflowEventPage, WorkflowEventQuery, WorkflowRun } from './types'
const status = (s: string) =>
  (
    ({
      succeeded: 'Completada',
      running: 'En ejecución',
      queued: 'En cola',
      blocked: 'Bloqueada',
      cancelled: 'Cancelada',
      pending: 'Pendiente',
    }) as Record<string, string>
  )[s] ?? s
function StepFailure({ result }: { result: unknown }) {
  if (!result || typeof result !== 'object') return null
  const data = result as Record<string, unknown>
  if (data.qa_report_only === true) return <p role="status" className="mt-3 text-sm text-muted-foreground">QA detectó hallazgos no bloqueantes. Se incluirán en el PR; las comprobaciones fallidas no se marcaron como aprobadas.</p>
  const output = data.output && typeof data.output === 'object' ? data.output as Record<string, unknown> : {}
  const error = typeof output.error === 'string' ? output.error : typeof data.reason === 'string' ? data.reason : ''
  if (!error) return null
  const solution = typeof output.solution === 'string' ? output.solution : error === 'Oversized or hard-linked workspace output'
    ? 'El runner antiguo no guardó la ruta del archivo. Actualiza el runner y reintenta para identificarlo. Los archivos de más de 10 MiB deben reducirse o retirarse de la entrada/salida del sandbox; los enlaces duros deben sustituirse por copias independientes. Conserva los originales. Una aprobación no permite omitir este control.'
    : 'Revisa los logs y artefactos de la fase, corrige la causa indicada y reintenta tras reconciliar los efectos externos. Una aprobación no corrige errores de ejecución.'
  return <div role="alert" className="mt-3 space-y-1 rounded-md border border-border p-3 text-sm">
    <p><strong>Error:</strong> {error}</p>
    {typeof output.path === 'string' && <p className="break-all"><strong>Archivo:</strong> <code>{output.path}</code></p>}
    {typeof output.actual === 'number' && typeof output.limit === 'number' && <p>Detectado: {output.actual} · permitido: {output.limit}{error === 'workspace_file_too_large' ? ' bytes' : error === 'workspace_hard_link' ? ' enlaces' : ''}</p>}
    <p><strong>Solución:</strong> {solution}</p>
  </div>
}

export function ExecutionDetail({
  run,
  disabled,
  control,
  approve,
  artifact,
  events,
}: {
  run: WorkflowRun
  disabled: boolean
  control: (action: 'cancel' | 'retry' | 'resume', step?: string) => void
  approve: (action: string, revision: number) => void
  artifact: (id: string) => void
  /** Paginated history; when present each phase offers its console and transcript. */
  events?: (q: WorkflowEventQuery) => Promise<WorkflowEventPage>
}) {
  const [opened, setOpened] = useState<string | null>(null)
  const live = ['queued', 'running', 'blocked'].includes(run.status)
  const refresh = run.events.find((event) => event.kind === 'run.base_refreshed')?.payload
  const replacement = refresh && typeof refresh === 'object' ? (refresh as Record<string, unknown>).replacement_run : undefined
  return (
    <section aria-label="Detalle de ejecución" className="space-y-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{run.snapshot.definition.name}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{run.source_key}</p>
        </div>
        <Badge>{status(run.status)}</Badge>
      </header>
      {typeof replacement === 'string' && (
        <p role="status" className="rounded-lg border border-border p-3 text-sm">
          La rama base cambió. Se inició una revalidación con fuentes actualizadas y el presupuesto restante; las evidencias anteriores se conservan, pero no acreditan el nuevo commit.{' '}
          <Link className="underline" to={`/factory/workflows?run=${encodeURIComponent(replacement)}`}>Ver revalidación automática</Link>
        </p>
      )}
      {run.execution && (
        <div className="flex flex-wrap gap-x-6 gap-y-2 rounded-lg bg-muted/50 p-3 text-sm">
          <span>
            Revisión de código <strong>{run.execution.revision}</strong>
          </span>
          <span>
            Reparaciones{' '}
            <strong>
              {run.execution.repairs} / {run.snapshot.definition.max_repairs ?? 2}
            </strong>
          </span>
          <span className="tabular-nums">
            Coste <strong>${run.execution.cost_usd.toFixed(3)}</strong> / $
            {run.snapshot.definition.max_cost_usd ?? 20}
          </span>
        </div>
      )}
      <ol
        aria-label="Fases del workflow"
        className="divide-y divide-border rounded-xl border border-border"
      >
        {run.snapshot.definition.steps.map((step) => {
          const state = run.steps.find((s) => s.id === step.id)
          const qaReported = !!state?.result && typeof state.result === 'object' && (state.result as Record<string, unknown>).qa_report_only === true
          const role = (
            {
              'isolated-investigator': 'investigation',
              'isolated-test-author': 'test_author',
              'isolated-implementer': 'implementation',
              'isolated-reviewer': 'reviewer',
            } as Record<string, string>
          )[step.adapter]
          const profile = run.snapshot.workspace.runtime?.config.agents.find((a) => a.role === role)
          return (
            <li key={step.id} className="p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-medium">{step.name}</h3>
                <Badge variant={qaReported ? 'warning' : 'default'}>{qaReported ? 'Completada con hallazgos QA' : status(state?.status ?? 'pending')}</Badge>
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                {step.adapter} · intento {state?.attempt ?? 0} de {step.max_attempts}
              </p>
              {profile && (
                <p className="mt-2 text-xs text-muted-foreground">
                  Perfil: {profile.binding?.name ?? 'Rol propio del workflow'}
                  {profile.binding ? ` · revisión ${profile.binding.revision}` : ''} · harness{' '}
                  {profile.harness ?? 'claude'} · modelo {profile.model}
                </p>
              )}
              <StepFailure result={state?.result} />
              {events && (state?.attempt ?? 0) > 0 && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="mt-2"
                  aria-expanded={opened === step.id}
                  onClick={() => setOpened(opened === step.id ? null : step.id)}
                >
                  {opened === step.id ? 'Ocultar ejecución' : 'Ver ejecución: conversación, consola y artefactos'}
                </Button>
              )}
              {events && opened === step.id && (
                <StepObservability
                  runId={run.id}
                  stepId={step.id}
                  agent={!!role}
                  state={state}
                  live={live}
                  artifacts={run.artifacts ?? []}
                  openArtifact={artifact}
                  load={events}
                />
              )}
              {state?.result != null && (
                <RawJson label="Ver resultado y evidencia" value={state.result} />
              )}
              {run.status === 'blocked' &&
                state?.status === 'blocked' &&
                state.attempt < step.max_attempts && (
                  <Button
                    size="sm"
                    variant="secondary"
                    className="mt-3"
                    disabled={disabled}
                    onClick={() => {
                      if (
                        window.confirm(
                          '¿Revisaste los efectos externos y detuviste al worker anterior?',
                        )
                      )
                        control('retry', step.id)
                    }}
                  >
                    Reintentar tras reconciliar
                  </Button>
                )}
            </li>
          )
        })}
      </ol>
      {run.status === 'blocked' && (
        <div className="space-y-3 rounded-xl border border-border p-4">
          <h3 className="font-medium">La ejecución necesita atención</h3>
          <p className="max-w-2xl text-sm text-muted-foreground">
            Revisa la fase y sus evidencias antes de autorizar. Una aprobación no permite omitir
            tests ni saltarse una política de prohibición.
          </p>
          <div className="flex flex-wrap gap-2">
            {['fix', 'publish', 'open_pr'].map((action) => (
              <Button
                key={action}
                size="sm"
                variant="secondary"
                disabled={disabled}
                onClick={() => {
                  if (window.confirm(`¿Autorizar ${action} para esta revisión exacta?`))
                    approve(action, action === 'fix' ? 0 : (run.execution?.revision ?? 0))
                }}
              >
                Autorizar{' '}
                {
                  (
                    {
                      fix: 'implementación',
                      publish: 'publicación',
                      open_pr: 'draft PR',
                    } as Record<string, string>
                  )[action]
                }
              </Button>
            ))}
            <Button size="sm" variant="ghost" disabled={disabled} onClick={() => control('resume')}>
              Reanudar cola reconciliada
            </Button>
          </div>
        </div>
      )}
      {!!run.execution?.publications.length && (
        <section className="space-y-2">
          <h3 className="font-medium">Pull requests</h3>
          {run.execution.publications.map((p) => (
            <p key={p.path} className="text-sm">
              {p.path} ·{' '}
              {p.result.url?.startsWith('https://github.com/') ? (
                <a
                  className="underline underline-offset-4"
                  href={p.result.url}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Abrir PR #{p.result.number}
                </a>
              ) : (
                p.state
              )}
            </p>
          ))}
        </section>
      )}
      <section className="space-y-3">
        <h3 className="font-medium">Logs y artefactos</h3>
        {!run.artifacts?.length ? (
          <p className="text-sm text-muted-foreground">
            Todavía no hay artefactos. Aparecerán cuando el runner ejecute una fase.
          </p>
        ) : (
          <ul className="divide-y divide-border">
            {run.artifacts.map((a) => (
              <li key={a.id} className="flex flex-wrap items-center justify-between gap-2 py-3">
                <button
                  type="button"
                  className="text-left text-sm underline underline-offset-4"
                  onClick={() => artifact(a.id)}
                >
                  {a.name}
                </button>
                <span className="text-xs text-muted-foreground">
                  {a.step_id} · {Math.ceil(a.size / 1024)} KB
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
      <details className="rounded-lg border border-border p-4">
        <summary className="cursor-pointer text-sm font-medium">Eventos y cambios fijados</summary>
        <div className="mt-4 space-y-3">
          {run.execution && <RawJson label="Commits por repositorio" value={run.execution.heads} />}
          <ol className="max-h-80 space-y-3 overflow-auto" aria-label="Eventos de ejecución">
            {run.events.map((e) => (
              <li key={e.seq} className="text-xs">
                <span className="text-muted-foreground">{e.created_at} · </span>
                {e.kind}
                <RawJson label="Detalles del evento" value={e.payload} />
              </li>
            ))}
          </ol>
          <code className="break-all text-xs">{run.id}</code>
        </div>
      </details>
      {['queued', 'running', 'blocked'].includes(run.status) && (
        <Button
          variant="secondary"
          disabled={disabled}
          onClick={() => {
            if (
              window.confirm(
                '¿Cancelar esta ejecución? No deshace efectos externos ya completados.',
              )
            )
              control('cancel')
          }}
        >
          Cancelar ejecución
        </Button>
      )}
    </section>
  )
}
