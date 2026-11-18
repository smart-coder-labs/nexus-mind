import { PageHeader } from './shared/ui'
import FactoryIntake from '../FactoryIntake'
export default function FactoryInputs() {
  return (
    <div className="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Entradas"
        subtitle="Conecta el origen una sola vez y deriva el trabajo a un agente individual. Cada fuente conserva su deduplicación, políticas y credenciales cifradas."
      />
      <FactoryIntake embedded />
    </div>
  )
}
