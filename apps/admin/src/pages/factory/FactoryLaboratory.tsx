import { Link, useSearchParams } from 'react-router-dom'
import FactoryEvaluations from '../FactoryEvaluations'
import DecisionModel from './DecisionModel'
export default function FactoryLaboratory() {
  const [params, setParams] = useSearchParams()
  const view = params.get('view') === 'decision' ? 'decision' : 'evaluations'
  return (
    <div className="min-w-0">
      <div className="mx-auto max-w-7xl space-y-3 px-4 pt-5 sm:px-6 md:px-8">
        <Link className="text-sm underline underline-offset-4" to="/factory/settings">
          Volver a configuración
        </Link>
        <p className="max-w-3xl text-sm text-muted-foreground">
          Laboratorio opcional. Las evaluaciones comparan especialistas; no sustituyen los tests y
          QA de cada ejecución. La integración del modelo de decisión puede habilitar inicios de
          bajo riesgo solo con consentimiento y políticas explícitas.
        </p>
        <nav
          aria-label="Herramientas de laboratorio"
          className="flex flex-wrap gap-1 border-b border-border pb-2"
        >
          {[
            ['evaluations', 'Evaluaciones'],
            ['decision', 'Modelo de decisión'],
          ].map(([id, label]) => (
            <button
              key={id}
              type="button"
              aria-current={view === id ? 'page' : undefined}
              className={`rounded-lg px-3 py-2 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring ${view === id ? 'bg-muted font-medium' : 'text-muted-foreground'}`}
              onClick={() => setParams({ view: id })}
            >
              {label}
            </button>
          ))}
        </nav>
      </div>
      {view === 'decision' ? <DecisionModel /> : <FactoryEvaluations />}
    </div>
  )
}
