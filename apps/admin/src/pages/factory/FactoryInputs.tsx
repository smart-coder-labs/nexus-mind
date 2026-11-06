import { useSearchParams } from 'react-router-dom'
import { PageHeader } from './shared/ui'
import FactoryIntake from '../FactoryIntake'
import WorkflowsPage from './workflows/WorkflowsPage'
export default function FactoryInputs() {
  const [params, setParams] = useSearchParams()
  const target = params.get('target') === 'agent' ? 'agent' : 'workflow'
  return (
    <div className="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Entradas"
        subtitle="Conecta el origen una sola vez y elige quién recibe el trabajo. Cada fuente conserva su deduplicación, políticas y credenciales cifradas."
      />
      <div className="space-y-3 border-b border-border pb-4">
        <p className="text-sm font-medium">Destino del trabajo</p>
        <nav className="flex flex-wrap gap-1" aria-label="Destino de las entradas">
          {[
            ['workflow', 'Workflow de un workspace'],
            ['agent', 'Agente individual'],
          ].map(([value, label]) => (
            <button
              type="button"
              key={value}
              aria-current={target === value ? 'page' : undefined}
              className={`rounded-lg px-3 py-2 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring ${target === value ? 'bg-muted font-medium' : 'text-muted-foreground'}`}
              onClick={() =>
                setParams((previous) => {
                  const next = new URLSearchParams(previous)
                  next.set('target', value)
                  return next
                })
              }
            >
              {label}
            </button>
          ))}
        </nav>
        <p className="max-w-3xl text-sm text-muted-foreground">
          {target === 'workflow'
            ? 'El origen inicia una definición guardada: agentes por fase, tests, servicios, QA y publicación. GitHub Issues entra por esta vía.'
            : 'Conserva las fuentes existentes que crean una tarea y la derivan a un resolver. Se ejecuta su plantilla completa, no un DAG del runner.'}
        </p>
      </div>
      {target === 'workflow' ? (
        <WorkflowsPage embedded area="sources" />
      ) : (
        <FactoryIntake embedded />
      )}
    </div>
  )
}
