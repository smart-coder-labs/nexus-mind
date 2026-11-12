import { Link, useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { InlineError, PageHeader, useFactory } from './shared/ui'
import FactorySettings from './FactorySettings'
import FactoryPolicies from '../FactoryPolicies'
import WorkflowsPage from './workflows/WorkflowsPage'
import { GithubReviewHooks } from './workflows/GithubReviewHooks'
const tabs = [
  ['infrastructure', 'Infraestructura'],
  ['execution', 'Agentes, harnesses y modelos'],
  ['integrations', 'GitHub y conectores'],
  ['permissions', 'Permisos y aprobaciones'],
] as const
export default function FactoryConfiguration() {
  const { can, client } = useFactory()
  const capabilities = useQuery({
    queryKey: ['workflow-catalog'],
    queryFn: () => client.getWorkflowCatalog(),
    enabled: can('factory_policy:read'),
    refetchInterval: 5000,
  })
  const [params, setParams] = useSearchParams()
  const tab = tabs.some(([id]) => id === params.get('tab')) ? params.get('tab')! : 'infrastructure'
  if (!can('factory_policy:read') && !can('autonomous_agent:read'))
    return (
      <div className="p-6">
        <InlineError message="Necesitas permiso para consultar la configuración de Factory." />
      </div>
    )
  return (
    <div className="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Configuración de Factory"
        subtitle="Infraestructura, identidades y límites. Las tareas y sus evidencias se gestionan desde Workflows y Ejecuciones."
      />
      <nav
        aria-label="Secciones de configuración de Factory"
        className="flex gap-1 overflow-x-auto border-b border-border pb-2"
      >
        {tabs.map(([id, label]) => (
          <button
            key={id}
            type="button"
            aria-current={tab === id ? 'page' : undefined}
            className={`shrink-0 rounded-lg px-3 py-2 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring ${tab === id ? 'bg-muted font-medium' : 'text-muted-foreground'}`}
            onClick={() =>
              setParams((previous) => {
                const next = new URLSearchParams(previous)
                next.set('tab', id)
                return next
              })
            }
          >
            {label}
          </button>
        ))}
      </nav>
      {tab === 'infrastructure' && (
        <div className="space-y-8">
          {can('autonomous_agent:read') && (
            <FactorySettings
              embedded
              sharedPauseReady={typeof capabilities.data?.factory_enabled === 'boolean'}
            />
          )}
          <section className="space-y-4 border-t border-border pt-6">
            <div>
              <h2 className="text-lg font-semibold">Runners y carpetas</h2>
              <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
                El worker del backend y los runners remotos son máquinas distintas. Una sesión de Pi
                o Claude en una no autentica automáticamente la otra. La pausa de Factory detiene
                nuevos inicios en ambos motores y revoca las fases activas.
              </p>
            </div>
            {can('factory_policy:read') ? (
              <WorkflowsPage embedded area="connections" />
            ) : (
              <p className="text-sm text-muted-foreground">
                Necesitas factory_policy:read para consultar runners remotos.
              </p>
            )}
          </section>
        </div>
      )}
      {tab === 'execution' && (
        <section className="max-w-4xl space-y-6">
          <div>
            <h2 className="text-lg font-semibold">Una configuración efectiva por ejecución</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              El agente guarda su perfil y modelo por defecto. El workflow asigna un perfil
              compatible y un harness a cada fase; permite heredar el modelo o elegir uno explícito.
              El backend fija revisión, modelo e instrucciones al iniciar el trabajo.
            </p>
          </div>
          <ol className="divide-y divide-border rounded-xl border border-border bg-card">
            <li className="space-y-2 p-5">
              <Link className="font-medium underline underline-offset-4" to="/factory/agents">
                Crear o editar un agente
              </Link>
              <p className="text-sm text-muted-foreground">
                Su plantilla completa se ejecuta como tarea individual. Solo los perfiles
                compatibles se pueden reutilizar en una fase de código.
              </p>
            </li>
            <li className="space-y-2 p-5">
              <Link
                className="font-medium underline underline-offset-4"
                to="/factory/workflows?tab=settings&section=agents"
              >
                Asignar agentes y harnesses por fase
              </Link>
              <p className="text-sm text-muted-foreground">
                Pi, Codex, Claude Code y otros adaptadores deben estar instalados y autorizados
                donde se ejecutan. No hay cambio silencioso a otro harness ni copia de sesiones
                OAuth.
              </p>
            </li>
            <li className="space-y-2 p-5">
              <Link className="font-medium underline underline-offset-4" to="/factory/runs">
                Consultar lo que realmente se ejecutó
              </Link>
              <p className="text-sm text-muted-foreground">
                La evidencia y los costes pertenecen a una ejecución, no al estado actual del
                formulario.
              </p>
            </li>
          </ol>
          <p className="rounded-lg bg-muted p-4 text-sm">
            Las preferencias globales históricas de «Modelos y roles» se conservan en la base de
            datos, pero no son una pasarela ejecutable. Ya no se presentan como configuración
            activa. Los agentes anteriores mantienen su ruta y no se reactivan automáticamente: al
            editarlos puedes autorizar explícitamente el modelo del agente/harness. Un pedido de la
            pasarela histórica sin conexión sigue bloqueado.
          </p>
          <p className="text-sm text-muted-foreground">
            Los checks y servicios ejecutan comandos reales en aislamiento; elegir un modelo no
            sustituye tests, build ni QA.
          </p>
        </section>
      )}
      {tab === 'integrations' && (
        <div className="space-y-6">
          <section className="max-w-3xl space-y-2">
            <h2 className="text-lg font-semibold">Conectores con un propósito claro</h2>
            <p className="text-sm text-muted-foreground">
              Entrada, publicación y webhook son permisos distintos. El token cifrado del control
              plane no se entrega al runner. Las fuentes se administran en{' '}
              <Link className="underline underline-offset-4" to="/factory/intake">
                Entradas
              </Link>
              ; el conector de publicación se selecciona en el workflow.
            </p>
          </section>
          <p className="max-w-3xl text-sm text-muted-foreground">
            Las reviews solicitadas en GitHub usan la cuenta del backend y no dependen del runner.
            GitHub no admite autoaprobación del autor; un workflow puede hacer una revisión interna
            independiente sin confundirla con una aprobación formal.
          </p>
          <GithubReviewHooks />
        </div>
      )}
      {tab === 'permissions' && <FactoryPolicies embedded />}
      <footer className="border-t border-border pt-4 text-sm text-muted-foreground">
        Herramientas opcionales:{' '}
        <Link className="underline underline-offset-4" to="/factory/laboratory">
          Laboratorio de evaluaciones y decisión
        </Link>
        . No son un requisito para ejecutar una tarea normal.
      </footer>
    </div>
  )
}
