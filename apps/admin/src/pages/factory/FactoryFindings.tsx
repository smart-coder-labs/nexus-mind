import { useState } from 'react'
import { InlineError, PageHeader, useFactory } from './shared/ui'
import FindingsPage from './findings/FindingsPage'
export default function FactoryFindings() {
  const { can } = useFactory()
  const [domain, setDomain] = useState<'software' | 'business' | 'all'>('software')
  return (
    <div className="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Hallazgos"
        subtitle="Resultados de agentes. Cada hallazgo mantiene el enlace a su ejecución y sus evidencias."
      />
      <nav
        className="flex flex-wrap gap-1 border-b border-border pb-2"
        aria-label="Ámbito de los hallazgos"
      >
        {(
          [
            ['software', 'Software y QA'],
            ['business', 'Contenido y leads'],
            ['all', 'Todos'],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            aria-current={domain === id ? 'page' : undefined}
            className={`rounded-lg px-3 py-2 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring ${domain === id ? 'bg-muted font-medium' : 'text-muted-foreground'}`}
            onClick={() => setDomain(id)}
          >
            {label}
          </button>
        ))}
      </nav>
      {can('autonomous_agent:read') ? (
        <FindingsPage embedded domain={domain} />
      ) : (
        <InlineError message="Necesitas permiso para consultar hallazgos de Factory." />
      )}
    </div>
  )
}
