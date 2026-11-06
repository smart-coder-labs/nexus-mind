import { useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Button } from '../../components/ui/Button'
import { Badge } from '../../components/ui/Badge'
import { InlineError, PageHeader, errorMessage, useFactory } from './shared/ui'
import RunsPage from './runs/RunsPage'
import { ExecutionDetail } from './workflows/ExecutionDetail'
export default function FactoryRuns() {
  const { client, can } = useFactory()
  const [params, setParams] = useSearchParams()
  const kind = params.get('kind') ?? 'all'
  const id = kind === 'workflow' ? (params.get('run') ?? '') : ''
  const workspace = params.get('workspace') ?? ''
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [artifact, setArtifact] = useState<{ mime: string; content_base64: string } | null>(null)
  const read = can('factory_policy:read')
  const write = can('factory_policy:write')
  const catalog = useQuery({
    queryKey: ['workflow-catalog'],
    queryFn: () => client.getWorkflowCatalog(),
    enabled: read,
    refetchInterval: 5000,
  })
  const detail = useQuery({
    queryKey: ['workflow-run', id],
    queryFn: () => client.getFactoryWorkflowRun(id),
    enabled: read && !!id,
    refetchInterval: 2500,
  })
  const definitions = catalog.data?.definitions ?? []
  const runs =
    catalog.data?.runs.filter(
      (r) =>
        !workspace ||
        definitions.find((d) => d.id === r.definition_id)?.definition.workspace_id === workspace,
    ) ?? []
  async function perform(work: () => Promise<unknown>) {
    setBusy(true)
    setError('')
    try {
      await work()
      await detail.refetch()
      await catalog.refetch()
    } catch (e) {
      setError(errorMessage(e, 'No se pudo completar la operación.'))
    } finally {
      setBusy(false)
    }
  }
  function tab(value: string) {
    setArtifact(null)
    setParams((previous) => {
      const next = new URLSearchParams(previous)
      next.set('kind', value)
      next.delete('run')
      return next
    })
  }
  function close() {
    setArtifact(null)
    setParams((previous) => {
      const next = new URLSearchParams(previous)
      next.delete('run')
      return next
    })
  }
  if (!read && !can('autonomous_agent:read'))
    return (
      <div className="p-6">
        <InlineError message="Necesitas permiso para consultar ejecuciones de Factory." />
      </div>
    )
  return (
    <div className="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Ejecuciones"
        subtitle="Workflows y agentes individuales en un solo lugar. Cada tipo conserva sus logs, evidencias y controles de seguridad."
      />
      <nav
        aria-label="Tipo de ejecución"
        className="flex flex-wrap gap-1 border-b border-border pb-2"
      >
        {[
          ['all', 'Todas'],
          ['workflow', 'Workflows'],
          ['agent', 'Agentes individuales'],
        ].map(([value, label]) => (
          <button
            key={value}
            type="button"
            aria-current={kind === value ? 'page' : undefined}
            className={`rounded-lg px-3 py-2 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring ${kind === value ? 'bg-muted font-medium' : 'text-muted-foreground'}`}
            onClick={() => tab(value)}
          >
            {label}
          </button>
        ))}
      </nav>
      {error && <InlineError message={error} />}
      {id && read && (
        <section
          className="space-y-5 rounded-xl border border-border bg-card p-5 sm:p-6"
          aria-label="Workflow seleccionado"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="font-medium">Detalle del workflow</h2>
            <Button variant="ghost" size="sm" onClick={close}>
              Cerrar ejecución
            </Button>
          </div>
          {detail.isPending && (
            <p role="status" className="text-sm">
              Cargando ejecución…
            </p>
          )}
          {detail.isError && (
            <InlineError
              message={errorMessage(detail.error, 'No se pudo abrir la ejecución.')}
              onRetry={() => {
                void detail.refetch()
              }}
            />
          )}
          {detail.data && (
            <ExecutionDetail
              run={detail.data}
              disabled={!write || busy}
              control={(action, step) => {
                void perform(() => client.controlFactoryWorkflowRun(id, action, step))
              }}
              approve={(action, revision) => {
                void perform(() => client.approveWorkflowAction(id, action, revision))
              }}
              artifact={(key) => {
                void perform(async () => setArtifact(await client.getWorkflowArtifact(key)))
              }}
            />
          )}
          {artifact && (
            <section
              className="space-y-3 border-t border-border pt-4"
              aria-label="Artefacto seleccionado"
            >
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium">Contenido del artefacto</h3>
                <Button size="sm" variant="ghost" onClick={() => setArtifact(null)}>
                  Cerrar artefacto
                </Button>
              </div>
              {artifact.mime.startsWith('image/') ? (
                <img
                  className="max-w-full"
                  alt="Evidencia de QA"
                  src={`data:${artifact.mime};base64,${artifact.content_base64}`}
                />
              ) : (
                <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words text-xs">
                  {new TextDecoder().decode(
                    Uint8Array.from(atob(artifact.content_base64), (c) => c.charCodeAt(0)),
                  )}
                </pre>
              )}
            </section>
          )}
        </section>
      )}
      {kind !== 'agent' && (
        <section className="space-y-4" aria-label="Historial de workflows">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-base font-semibold">
              Workflows{workspace ? ' del workspace' : ''}
            </h2>
            <Link className="text-sm underline underline-offset-4" to="/factory/workflows?tab=runs">
              Ejecutar una tarea
            </Link>
          </div>
          {!read ? (
            <p className="text-sm text-muted-foreground">
              Necesitas factory_policy:read para consultar workflows.
            </p>
          ) : catalog.isPending ? (
            <p role="status">Cargando workflows…</p>
          ) : catalog.isError ? (
            <InlineError
              message="No se pudo cargar el historial de workflows."
              onRetry={() => {
                void catalog.refetch()
              }}
            />
          ) : !runs.length ? (
            <p className="rounded-xl border border-dashed border-border p-5 text-sm text-muted-foreground">
              No hay ejecuciones de workflows en este alcance.
            </p>
          ) : (
            <ul className="divide-y divide-border rounded-xl border border-border bg-card">
              {runs.map((r) => (
                <li key={r.id}>
                  <Link
                    className="flex flex-wrap items-center justify-between gap-3 p-4 hover:bg-muted/50 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
                    to={`/factory/runs?kind=workflow&run=${encodeURIComponent(r.id)}${workspace ? `&workspace=${encodeURIComponent(workspace)}` : ''}`}
                  >
                    <div className="min-w-0">
                      <h3 className="text-sm font-medium">
                        {definitions.find((d) => d.id === r.definition_id)?.definition.name ??
                          'Workflow'}
                      </h3>
                      <p className="mt-1 break-all text-xs text-muted-foreground">
                        {r.source_key} · {new Date(r.created_at).toLocaleString('es')}
                      </p>
                    </div>
                    <span className="flex items-center gap-3">
                      <Badge
                        variant={
                          r.status === 'succeeded'
                            ? 'success'
                            : r.status === 'blocked'
                              ? 'warning'
                              : r.status === 'failed'
                                ? 'error'
                                : 'default'
                        }
                      >
                        {(
                          {
                            queued: 'En cola',
                            running: 'En curso',
                            succeeded: 'Completada',
                            blocked: 'Bloqueada',
                            failed: 'Fallida',
                            cancelled: 'Cancelada',
                          } as Record<string, string>
                        )[r.status] ?? r.status}
                      </Badge>
                      <span className="text-sm underline underline-offset-4">Abrir ejecución</span>
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
      {kind !== 'workflow' && (
        <section
          className="space-y-4 border-t border-border pt-6"
          aria-label="Historial de agentes individuales"
        >
          <h2 className="text-base font-semibold">Agentes individuales</h2>
          {can('autonomous_agent:read') ? (
            <RunsPage embedded />
          ) : (
            <p className="text-sm text-muted-foreground">
              Necesitas autonomous_agent:read para consultar este historial.
            </p>
          )}
        </section>
      )}
    </div>
  )
}
