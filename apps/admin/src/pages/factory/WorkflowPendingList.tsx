import { Link } from 'react-router-dom'
import type { WorkflowPending } from './workflows/types'
export function WorkflowPendingList({ items }: { items: WorkflowPending[] }) {
  if (!items.length) return null
  return (
    <section className="space-y-3" aria-label="Workflows pendientes">
      <div>
        <h2 className="text-base font-semibold">Workflows que necesitan atención</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Revisa el bloqueo y la evidencia de la fase antes de autorizar o reintentar. Estar
          bloqueado no significa que solo falte una aprobación.
        </p>
      </div>
      <ul className="divide-y divide-border rounded-xl border border-border bg-card">
        {items.map((r) => (
          <li key={r.id} className="flex flex-wrap items-start justify-between gap-3 p-4">
            <div className="min-w-0">
              <h3 className="break-words text-sm font-medium">{r.title ?? r.source_key}</h3>
              <p className="mt-1 text-xs text-muted-foreground">
                {r.workspace_name} · {r.definition_name}
              </p>
              <p className="mt-1 break-all text-xs text-muted-foreground">
                {r.last_event?.includes('adapter') && /missing|unavailable/.test(r.last_event)
                  ? 'El runner no tiene el adaptador requerido.'
                  : 'Consulta la fase y sus evidencias para identificar el bloqueo.'}
              </p>
              {r.last_event && (
                <details className="mt-1 text-xs text-muted-foreground">
                  <summary className="cursor-pointer">Evento técnico</summary>
                  <p className="mt-1 break-all font-mono">{r.last_event}</p>
                </details>
              )}
            </div>
            <Link
              className="rounded-md px-2 py-1 text-sm underline underline-offset-4 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
              to={`/factory/runs?kind=workflow&run=${encodeURIComponent(r.id)}`}
            >
              Revisar bloqueo
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
