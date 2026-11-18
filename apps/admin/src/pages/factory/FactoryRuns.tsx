import { InlineError, PageHeader, useFactory } from './shared/ui'
import RunsPage from './runs/RunsPage'
export default function FactoryRuns() {
  const { can } = useFactory()
  if (!can('autonomous_agent:read'))
    return (
      <div className="p-6">
        <InlineError message="Necesitas autonomous_agent:read para consultar ejecuciones de Factory." />
      </div>
    )
  return (
    <div className="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Ejecuciones"
        subtitle="Ejecuciones de agentes individuales, con sus logs, evidencias y controles de seguridad."
      />
      <RunsPage embedded />
    </div>
  )
}
