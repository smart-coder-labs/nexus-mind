import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Play, Settings2 } from 'lucide-react'
import { InlineError, PageHeader, useFactory } from './shared/ui'
export default function FactoryHome() {
  const { can, client } = useFactory()
  const agents = useQuery({
    queryKey: ['autonomous-agents'],
    queryFn: () => client.listAutonomousAgents(),
    enabled: can('autonomous_agent:read'),
  })
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
      {native && (
        <section className="space-y-4">
          <h2 className="text-base font-semibold">Un recorrido de la entrada a la evidencia</h2>
          <ol className="grid gap-4 border-y border-border py-5 sm:grid-cols-3">
            <li>
              <Link
                className="text-sm font-medium underline underline-offset-4"
                to="/factory/intake"
              >
                Entradas
              </Link>
              <p className="mt-2 text-sm text-muted-foreground">
                Conecta el origen y deriva el trabajo a un agente individual.
              </p>
            </li>
            <li>
              <Link className="text-sm font-medium underline underline-offset-4" to="/factory/runs">
                Ejecuciones y evidencias
              </Link>
              <p className="mt-2 text-sm text-muted-foreground">
                Historial con el detalle y los controles de cada ejecución.
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
            plantilla completa para tareas individuales. Las credenciales y herramientas no se transfieren entre máquinas.
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
            El catálogo requiere autonomous_agent:read.
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
