import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ArrowRight, Play, Settings2 } from 'lucide-react'
import { InlineError, PageHeader, useFactory } from './shared/ui'
import { WorkflowPendingList } from './WorkflowPendingList'
export default function FactoryHome() {
  const { can, client } = useFactory()
  const catalog = useQuery({
    queryKey: ['workflow-catalog'],
    queryFn: () => client.getWorkflowCatalog(),
    enabled: can('factory_policy:read'),
    refetchInterval: 10000,
  })
  const agents = useQuery({
    queryKey: ['autonomous-agents'],
    queryFn: () => client.listAutonomousAgents(),
    enabled: can('autonomous_agent:read'),
  })
  const workspaces = catalog.data?.workspaces ?? []
  const configured = workspaces.filter((w) => w.runtime?.ready).length
  const native = can('factory_policy:read')
  const agentRead = can('autonomous_agent:read')
  if (!native && !agentRead)
    return (
      <div className="p-6">
        <InlineError message="Necesitas permiso para consultar Factory." />
      </div>
    )
  return (
    <div className="mx-auto max-w-7xl space-y-7 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Factory"
        subtitle="De una tarea a cambios verificados. Elige qué ejecutar y sigue sus evidencias, sin configurar lo mismo en varios sitios."
      />
      <section className="space-y-5 rounded-xl border border-border bg-card p-5 sm:p-6">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="max-w-2xl">
            <h2 className="text-lg font-semibold">
              {!native
                ? 'Ejecuta un agente individual'
                : workspaces.length
                  ? 'Continúa con tu workspace'
                  : 'Prepara tu primer workflow'}
            </h2>
            <p className="mt-2 text-sm text-muted-foreground">
              {!native
                ? 'Abre el catálogo y selecciona una plantilla habilitada. Tu rol permite consultar agentes individuales; la orquestación de workspaces requiere factory_policy:read.'
                : workspaces.length
                  ? 'Configura qué repositorios pueden cambiar, reutiliza perfiles de agentes y selecciona un harness por fase. Los tests, QA y publicación quedan vinculados a cada ejecución.'
                  : 'Conecta la máquina que contiene tus clones, confirma el alcance y configura los perfiles. No se ejecuta código ni se consumen modelos al registrar la carpeta.'}
            </p>
          </div>
          <Link
            className="inline-flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
            to={
              !native
                ? '/factory/agents'
                : workspaces.length
                  ? '/factory/workflows'
                  : '/factory/workflows?tab=connections'
            }
          >
            {!native
              ? 'Abrir agentes'
              : workspaces.length
                ? 'Abrir workflows'
                : 'Conectar workspace'}
            <ArrowRight className="size-4" />
          </Link>
        </div>
        {catalog.isPending && can('factory_policy:read') ? (
          <p role="status" className="text-sm text-muted-foreground">
            Comprobando workspaces…
          </p>
        ) : catalog.isError ? (
          <InlineError
            message="No se pudo comprobar el estado de los workspaces. No se asumirá que están listos."
            onRetry={() => {
              void catalog.refetch()
            }}
          />
        ) : (
          catalog.data && (
            <p className="text-sm text-muted-foreground">
              {workspaces.length} workspaces · {configured} con perfiles aplicados ·{' '}
              {catalog.data.factory_enabled === false
                ? 'Factory pausada'
                : catalog.data.factory_enabled === true
                  ? 'Inicio de trabajo habilitado'
                  : 'Estado de pausa global no disponible en esta versión'}
            </p>
          )
        )}
        {workspaces.length > 0 && (
          <ul className="divide-y divide-border">
            {workspaces.slice(0, 5).map((w) => (
              <li key={w.id} className="flex flex-wrap items-center justify-between gap-3 py-3">
                <div>
                  <h3 className="text-sm font-medium">{w.name}</h3>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {w.repositories.length} repositorios ·{' '}
                    {w.runtime?.ready
                      ? 'Perfiles aplicados; QA se verifica en cada ejecución'
                      : 'Configuración pendiente'}
                  </p>
                </div>
                <Link
                  className="text-sm underline underline-offset-4"
                  to={`/factory/workflows?workspace=${encodeURIComponent(w.id)}`}
                >
                  Abrir workspace
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>
      {catalog.data?.pending_runs && (
        <WorkflowPendingList items={catalog.data.pending_runs.slice(0, 5)} />
      )}
      {native && (
        <section className="space-y-4">
          <h2 className="text-base font-semibold">Un recorrido, con dos formas de ejecutar</h2>
          <ol className="grid gap-4 border-y border-border py-5 sm:grid-cols-3">
            <li>
              <Link
                className="text-sm font-medium underline underline-offset-4"
                to="/factory/intake"
              >
                Entradas
              </Link>
              <p className="mt-2 text-sm text-muted-foreground">
                Conecta el origen y elige destino: workflow o agente individual.
              </p>
            </li>
            <li>
              <Link className="text-sm font-medium underline underline-offset-4" to="/factory/runs">
                Ejecuciones y evidencias
              </Link>
              <p className="mt-2 text-sm text-muted-foreground">
                Historial conjunto, con detalle y controles propios de cada tipo.
              </p>
            </li>
            <li>
              <Link
                className="text-sm font-medium underline underline-offset-4"
                to="/factory/inbox"
              >
                Pendientes
              </Link>
              <p className="mt-2 text-sm text-muted-foreground">
                Bloqueos y aprobaciones. Ninguna autorización permite omitir las pruebas.
              </p>
            </li>
          </ol>
        </section>
      )}
      <section className="flex flex-wrap items-start justify-between gap-4">
        <div className="max-w-2xl">
          <h2 className="text-base font-semibold">Agentes reutilizables</h2>
          <p className="mt-2 text-sm text-muted-foreground">
            {agents.isSuccess ? `${agents.data.length} agentes en el catálogo. ` : ''}Ejecuta una
            plantilla completa para tareas individuales, o reutiliza su perfil compatible dentro de
            un workflow. Las credenciales y herramientas no se transfieren entre máquinas.
          </p>
          {agents.isError && (
            <InlineError
              message="No se pudo cargar el catálogo de agentes."
              onRetry={() => {
                void agents.refetch()
              }}
            />
          )}
        </div>
        {agentRead ? (
          <Link className="text-sm underline underline-offset-4" to="/factory/agents">
            Abrir catálogo
          </Link>
        ) : (
          <span className="text-xs text-muted-foreground">
            El catálogo requiere autonomous_agent:read. Puedes usar roles propios del workflow.
          </span>
        )}
      </section>
      <footer className="flex flex-wrap gap-3 border-t border-border pt-4">
        <Link
          className="inline-flex items-center rounded-lg bg-muted px-3 py-2 text-sm font-medium"
          to="/factory/runs"
        >
          <Play className="mr-2 size-4" />
          Ver ejecuciones
        </Link>
        <Link
          className="inline-flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground underline underline-offset-4"
          to="/factory/settings"
        >
          <Settings2 className="size-4" />
          Configurar infraestructura y permisos
        </Link>
      </footer>
    </div>
  )
}
