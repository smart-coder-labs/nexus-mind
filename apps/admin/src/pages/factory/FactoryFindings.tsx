import { Link } from 'react-router-dom'
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { InlineError, PageHeader, useFactory } from './shared/ui'
import FindingsPage from './findings/FindingsPage'
export default function FactoryFindings() {
  const { can, client } = useFactory()
  const read = can('factory_policy:read')
  const catalog = useQuery({
    queryKey: ['workflow-catalog'],
    queryFn: () => client.getWorkflowCatalog(),
    enabled: read,
    refetchInterval: 10000,
  })
  const observations = catalog.data?.findings ?? []
  const [domain, setDomain] = useState<'software' | 'business' | 'all'>('software')
  return (
    <div className="mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8">
      <PageHeader
        title="Hallazgos"
        subtitle="Observaciones de workflows y resultados de agentes. Cada hallazgo mantiene el enlace a su ejecución y sus evidencias."
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
      {read && domain !== 'business' && (
        <section className="space-y-4" aria-label="Observaciones de workflows">
          <div>
            <h2 className="text-base font-semibold">Observaciones de workflows</h2>
            <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
              Últimas 100 observaciones de la evidencia vigente por fase y revisión. Son informes,
              no tickets independientes: el bloqueo o reparación se gestiona en su ejecución.
            </p>
          </div>
          {catalog.isPending ? (
            <p role="status">Cargando observaciones…</p>
          ) : catalog.isError ? (
            <InlineError
              message="No se pudieron cargar los hallazgos de workflows."
              onRetry={() => {
                void catalog.refetch()
              }}
            />
          ) : catalog.data?.findings === undefined ? (
            <p className="text-sm text-muted-foreground">
              El backend todavía no publica esta vista. Las evidencias originales siguen en
              Ejecuciones.
            </p>
          ) : observations.length ? (
            <ul className="divide-y divide-border rounded-xl border border-border bg-card">
              {observations.map((item, index) => (
                <li key={`${item.run_id}:${item.step_id}:${index}`} className="space-y-2 p-4">
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <h3 className="text-sm font-medium">{item.title}</h3>
                    <span className="text-xs text-muted-foreground">
                      {item.severity} · revisión {item.revision}
                    </span>
                  </div>
                  <p className="whitespace-pre-wrap break-words text-sm text-muted-foreground">
                    {item.summary}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {item.definition_name} · {item.step_id} · {item.run_status}
                  </p>
                  <Link
                    className="inline-block text-sm underline underline-offset-4"
                    to={`/factory/runs?kind=workflow&run=${encodeURIComponent(item.run_id)}`}
                  >
                    Ver fase y evidencia
                  </Link>
                </li>
              ))}
            </ul>
          ) : (
            <p className="rounded-xl border border-dashed border-border p-5 text-sm text-muted-foreground">
              No hay observaciones en la evidencia vigente. Esto no implica que todas las
              ejecuciones hayan completado su QA.
            </p>
          )}
        </section>
      )}
      {can('autonomous_agent:read') ? (
        <FindingsPage embedded domain={domain} />
      ) : (
        !read && <InlineError message="Necesitas permiso para consultar hallazgos de Factory." />
      )}
    </div>
  )
}
