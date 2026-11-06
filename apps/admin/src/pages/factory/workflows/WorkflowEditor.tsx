import type { WorkflowDefinition } from './types'
import { WorkflowAgentRuntime } from './WorkflowAgentRuntime'
import { Input } from '../../../components/ui/Input'
export function WorkflowEditor({
  value,
  onChange,
  adapters,
  repositories,
}: {
  value: string
  onChange: (value: string) => void
  adapters: string[]
  repositories: string[]
}) {
  let definition: WorkflowDefinition | null = null
  try {
    const parsed = JSON.parse(value) as WorkflowDefinition
    if (
      parsed &&
      typeof parsed.name === 'string' &&
      typeof parsed.workspace_id === 'string' &&
      Array.isArray(parsed.steps) &&
      parsed.steps.every(
        (s) =>
          s &&
          typeof s.id === 'string' &&
          typeof s.name === 'string' &&
          typeof s.adapter === 'string' &&
          Array.isArray(s.depends_on) &&
          Array.isArray(s.write_repositories),
      )
    )
      definition = parsed
  } catch {
    /* Invalid drafts remain editable below. */
  }
  function update(change: (d: WorkflowDefinition) => void) {
    if (definition) {
      const next = structuredClone(definition)
      change(next)
      onChange(JSON.stringify(next, null, 2))
    }
  }
  return (
    <div className="space-y-5">
      {definition && (
        <>
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="space-y-1.5 text-sm sm:col-span-2">
              Nombre del workflow
              <Input
                value={definition.name}
                required
                onChange={(e) =>
                  update((d) => {
                    d.name = e.target.value
                  })
                }
              />
            </label>
            <label className="space-y-1.5 text-sm">
              Presupuesto máximo (USD)
              <Input
                type="number"
                min="0.01"
                max="1000"
                step="0.01"
                value={definition.max_cost_usd ?? 20}
                onChange={(e) =>
                  update((d) => {
                    d.max_cost_usd = Number(e.target.value)
                  })
                }
              />
            </label>
            <label className="space-y-1.5 text-sm">
              Ciclos automáticos de reparación
              <Input
                type="number"
                min="0"
                max="2"
                value={definition.max_repairs ?? 2}
                onChange={(e) =>
                  update((d) => {
                    d.max_repairs = Number(e.target.value)
                  })
                }
              />
            </label>
          </div>
          <div>
            <h3 className="text-sm font-medium">Fases del workflow</h3>
            <p className="mt-1 text-sm text-muted-foreground">
              Abre una fase para ajustar sus instrucciones, límites y alcance. El revisor
              independiente es de solo lectura.
            </p>
          </div>
          <ol className="divide-y divide-border rounded-xl border border-border">
            {definition.steps.map((step, index) => (
              <li key={step.id}>
                <details className="group p-4" open={step.kind === 'publish' || undefined}>
                  <summary className="cursor-pointer text-sm font-medium">
                    <span className="mr-2 text-muted-foreground">{index + 1}.</span>
                    {step.name}
                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                      {step.write_repositories.length
                        ? `${step.write_repositories.length} repositorios con escritura`
                        : 'Sin escritura de código'}
                    </span>
                  </summary>
                  <div className="mt-4 space-y-4">
                    <p className="text-xs text-muted-foreground">
                      Después de: {step.depends_on.join(', ') || 'inicio del workflow'}
                    </p>
                    {(step.kind === 'agent' || step.kind === 'review') && <WorkflowAgentRuntime options={step.options ?? {}} onChange={options => update(d => { d.steps[index].options = options })} />}
                    <label className="block space-y-1.5 text-sm">
                      Instrucciones de esta fase
                      <textarea
                        className="min-h-20 w-full rounded-lg border border-input bg-background p-3 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
                        value={String(step.options?.instructions ?? '')}
                        placeholder="Contexto adicional; las políticas y pruebas siguen siendo obligatorias"
                        onChange={(e) =>
                          update((d) => {
                            d.steps[index].options = {
                              ...step.options,
                              instructions: e.target.value,
                            }
                          })
                        }
                      />
                    </label>
                    {step.kind === 'publish' && step.options?.github_auth === 'server_gh_cli' && (
                      <p className="text-sm text-muted-foreground">
                        Publicación con la cuenta del backend:{' '}
                        <strong>{String(step.options.github_login ?? 'sin autenticar')}</strong>. No
                        se transfieren credenciales al runner.
                      </p>
                    )}
                    {step.kind === 'publish' && step.options?.github_auth !== 'server_gh_cli' && (
                      <label className="block space-y-1.5 text-sm">
                        ID del conector GitHub autorizado
                        <Input
                          value={String(step.options?.connector_id ?? '')}
                          placeholder="Referencia cifrada, nunca un token"
                          required
                          onChange={(e) =>
                            update((d) => {
                              d.steps[index].options = {
                                ...step.options,
                                connector_id: e.target.value,
                              }
                            })
                          }
                        />
                      </label>
                    )}
                    {step.kind === 'agent' && (
                      <details className="text-sm">
                        <summary className="cursor-pointer">
                          Editar alcance de escritura ({step.write_repositories.length})
                        </summary>
                        <div className="mt-3 grid max-h-60 gap-2 overflow-auto sm:grid-cols-2">
                          {repositories.map((path) => (
                            <label key={path} className="flex min-w-0 items-start gap-2 text-xs">
                              <input
                                className="mt-0.5 shrink-0"
                                type="checkbox"
                                checked={step.write_repositories.includes(path)}
                                onChange={(e) =>
                                  update((d) => {
                                    d.steps[index].write_repositories = e.target.checked
                                      ? [...step.write_repositories, path]
                                      : step.write_repositories.filter((p) => p !== path)
                                  })
                                }
                              />
                              <span className="break-all">{path}</span>
                            </label>
                          ))}
                        </div>
                      </details>
                    )}
                    <details className="space-y-3 text-sm">
                      <summary className="cursor-pointer text-muted-foreground">
                        Adaptador y límites técnicos
                      </summary>
                      <div className="grid gap-4 pt-3 sm:grid-cols-2">
                        <label className="space-y-1.5 text-sm sm:col-span-2">
                          Adaptador
                          <select
                            className="h-9 w-full min-w-0 rounded-lg border border-input bg-background px-3 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
                            value={step.adapter}
                            onChange={(e) =>
                              update((d) => {
                                d.steps[index].adapter = e.target.value
                              })
                            }
                          >
                            {[...new Set([step.adapter, ...adapters])].map((a) => (
                              <option key={a}>{a}</option>
                            ))}
                          </select>
                        </label>
                        <label className="space-y-1.5 text-sm">
                          Intentos máximos
                          <Input
                            type="number"
                            min="1"
                            max="3"
                            value={step.max_attempts ?? 1}
                            onChange={(e) =>
                              update((d) => {
                                d.steps[index].max_attempts = Number(e.target.value)
                              })
                            }
                          />
                        </label>
                        <label className="space-y-1.5 text-sm">
                          Tiempo máximo (segundos)
                          <Input
                            type="number"
                            min="10"
                            max="7200"
                            value={step.timeout_seconds ?? 900}
                            onChange={(e) =>
                              update((d) => {
                                d.steps[index].timeout_seconds = Number(e.target.value)
                              })
                            }
                          />
                        </label>
                      </div>
                      {!adapters.includes(step.adapter) && (
                        <p className="text-xs text-muted-foreground">
                          El runner todavía no anuncia este adaptador. La ejecución esperará hasta
                          que esté disponible.
                        </p>
                      )}
                      {step.kind === 'check' && (
                        <label className="block space-y-1.5 text-sm">
                          Comprobaciones requeridas (separadas por coma)
                          <Input
                            value={(Array.isArray(step.options?.required_checks)
                              ? step.options.required_checks
                              : []
                            ).join(', ')}
                            onChange={(e) =>
                              update((d) => {
                                d.steps[index].options = {
                                  ...step.options,
                                  required_checks: e.target.value
                                    .split(',')
                                    .map((s) => s.trim())
                                    .filter(Boolean),
                                }
                              })
                            }
                          />
                        </label>
                      )}
                    </details>
                  </div>
                </details>
              </li>
            ))}
          </ol>
        </>
      )}
      <details className="rounded-lg bg-muted/50 p-4">
        <summary className="cursor-pointer text-sm">JSON de definición · avanzado</summary>
        <label className="mt-3 block space-y-1 text-xs">
          Definición JSON
          <textarea
            className="h-80 w-full rounded-lg border border-input bg-background p-3 font-mono text-xs focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
            value={value}
            onChange={(e) => onChange(e.target.value)}
          />
        </label>
      </details>
      {!definition && (
        <p role="alert" className="text-sm text-destructive">
          La definición JSON no es válida. Corrígela en el editor avanzado antes de guardar.
        </p>
      )}
    </div>
  )
}
