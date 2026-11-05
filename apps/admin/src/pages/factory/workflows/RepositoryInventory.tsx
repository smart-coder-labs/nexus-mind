import { useState } from 'react'
import { Search } from 'lucide-react'
import { Input } from '../../../components/ui/Input'
import { Button } from '../../../components/ui/Button'
import type { Repository, Workspace } from './types'
export function RepositoryInventory({
  workspace,
  disabled,
  onSave,
}: {
  workspace: Workspace
  disabled: boolean
  onSave: (repos: Repository[]) => void
}) {
  const [rows, setRows] = useState(() =>
    workspace.discovered.map((r) => ({
      ...r,
      selected:
        workspace.confirmed_revision === null ||
        workspace.repositories.some((v) => v.path === r.path),
      writable: workspace.repositories.some((v) => v.path === r.path && v.writable),
    })),
  )
  const [query, setQuery] = useState('')
  const [limit, setLimit] = useState(12)
  const selected = rows.filter((r) => r.selected)
  const filtered = rows.filter((r) => r.path.toLowerCase().includes(query.toLowerCase()))
  return (
    <section className="space-y-4" aria-label="Repositorios del workspace">
      <div>
        <h2 className="text-base font-semibold">Repositorios y permisos</h2>
        <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
          Selecciona el alcance una vez para el workspace. Los agentes usan copias separadas; no
          reorganizan tus clones ni reciben sus archivos .env.
        </p>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          {selected.length} seleccionados · {selected.filter((r) => r.writable).length} con
          escritura · inventario {workspace.inventory_revision}
        </p>
        <label className="relative block w-full sm:w-72">
          <Search className="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground" />
          <Input
            aria-label="Buscar repositorio"
            className="pl-9"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value)
              setLimit(12)
            }}
            placeholder="Buscar repositorio"
          />
        </label>
      </div>
      {!rows.length ? (
        <div className="rounded-xl border border-dashed border-border p-6">
          <h3 className="font-medium">Esperando al runner</h3>
          <p className="mt-1 text-sm text-muted-foreground">
            La carpeta está registrada. El runner debe conectarse y descubrir sus repositorios; el
            navegador no puede escanear Oracle.
          </p>
        </div>
      ) : (
        <>
          <div className="flex flex-wrap gap-x-6 gap-y-3 border-y border-border py-3 text-sm">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                disabled={disabled}
                checked={selected.length === rows.length}
                onChange={(e) =>
                  setRows((c) => c.map((r) => ({ ...r, selected: e.target.checked })))
                }
              />
              Seleccionar todos ({rows.length})
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                disabled={disabled || !selected.length}
                checked={!!selected.length && selected.every((r) => r.writable)}
                onChange={(e) =>
                  setRows((c) =>
                    c.map((r) => (r.selected ? { ...r, writable: e.target.checked } : r)),
                  )
                }
              />
              Permitir escritura en todos los seleccionados
            </label>
          </div>
          <div className="divide-y divide-border">
            {filtered.slice(0, limit).map((repo) => (
              <div
                key={repo.path}
                className="grid gap-2 py-3 sm:grid-cols-[minmax(0,1fr)_110px_130px]"
              >
                <label className="flex min-w-0 items-start gap-2 text-sm">
                  <input
                    className="mt-1 shrink-0"
                    type="checkbox"
                    disabled={disabled}
                    checked={repo.selected}
                    onChange={(e) =>
                      setRows((c) =>
                        c.map((r) =>
                          r.path === repo.path ? { ...r, selected: e.target.checked } : r,
                        ),
                      )
                    }
                  />
                  <span className="break-words">{repo.path}</span>
                </label>
                <code className="pl-5 text-xs text-muted-foreground sm:pl-0">
                  {repo.head_sha.slice(0, 10)}
                </code>
                <label className="flex items-center gap-2 pl-5 text-sm sm:pl-0">
                  <input
                    type="checkbox"
                    checked={repo.writable}
                    disabled={disabled || !repo.selected}
                    onChange={(e) =>
                      setRows((c) =>
                        c.map((r) =>
                          r.path === repo.path ? { ...r, writable: e.target.checked } : r,
                        ),
                      )
                    }
                  />
                  Escritura
                </label>
              </div>
            ))}
          </div>
          {!filtered.length && (
            <p className="py-4 text-sm text-muted-foreground">
              No hay repositorios que coincidan con «{query}».
            </p>
          )}
          {filtered.length > limit && (
            <Button variant="ghost" onClick={() => setLimit((n) => n + 20)}>
              Mostrar más ({filtered.length - limit} restantes)
            </Button>
          )}
          <div className="flex flex-wrap items-center gap-3 border-t border-border pt-4">
            <Button
              disabled={disabled || !selected.length}
              onClick={() => onSave(selected.map(({ selected: _selected, ...r }) => r))}
            >
              Guardar selección y permisos
            </Button>
            <span className="text-xs text-muted-foreground">
              {workspace.confirmed_revision === workspace.inventory_revision
                ? 'Inventario confirmado'
                : 'Pendiente de confirmación'}
            </span>
          </div>
          <p className="max-w-2xl text-xs text-muted-foreground">
            Permitir escritura no autoriza merges, despliegues ni secretos de producción. Los
            cambios de alcance no modifican snapshots de ejecuciones anteriores.
          </p>
        </>
      )}
    </section>
  )
}
