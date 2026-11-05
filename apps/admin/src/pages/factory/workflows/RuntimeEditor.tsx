import { useEffect, useState } from 'react'
import { Plus, Trash2, ShieldCheck, Info } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import type { AutonomousAgentDefinition } from '../../../types'
import { AgentRoleEditor } from './AgentRoleEditor'
import { InlineError, errorMessage } from '../shared/ui'
import {
  agentRoles,
  checkTypes,
  emptyRuntime,
  formatCommand,
  parseCommand,
  type RuntimeCheck,
  type RuntimeConfig,
  type RuntimeService,
  type RuntimeState,
  type WorkflowAgentProfile,
} from './runtime'

const select =
  'h-9 w-full min-w-0 rounded-lg border border-input bg-background px-3 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring'
export type RuntimeDraft = {
  config: RuntimeConfig
  revision: number
  commands: Record<string, string>
}
export function RuntimeEditor({
  value,
  repositories,
  section,
  disabled: externalDisabled,
  onSave,
  draft,
  onDraft,
  catalog = [],
  loadAgent,
}: {
  value?: RuntimeState
  catalog?: AutonomousAgentDefinition[]
  loadAgent?: (id: string) => Promise<WorkflowAgentProfile>
  repositories: string[]
  section: 'agents' | 'checks' | 'services'
  disabled: boolean
  onSave: (revision: number, config: RuntimeConfig) => Promise<RuntimeState | void>
  draft?: RuntimeDraft
  onDraft?: (draft: RuntimeDraft) => void
}) {
  const runtime = value ?? emptyRuntime()
  const [config, setConfig] = useState<RuntimeConfig>(() =>
    structuredClone(draft?.config ?? runtime.config),
  )
  const [revision, setRevision] = useState(draft?.revision ?? runtime.revision)
  const [saving, setSaving] = useState(false)
  const [loadingRoles, setLoadingRoles] = useState<Set<string>>(() => new Set())
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [image, setImage] = useState(config.agents[0]?.image ?? '')
  const [connection, setConnection] = useState(config.agents[0]?.model_connection ?? '')
  const [network, setNetwork] = useState(config.agents[0]?.network ?? '')
  const nodeImage = runtime.report.managed_images?.node ?? ''
  const agentImage = runtime.report.managed_images?.agent ?? ''
  function useBackendPi() {
    setConfig((c) => ({ ...c, agents: agentRoles.map((r) => {
      const current = c.agents.find((a) => a.role === r.id)
      return { ...current, role: r.id, harness: 'pi', inherit_model: false,
        model: current?.harness === 'pi' && current.model ? current.model : ['investigation', 'implementation'].includes(r.id) ? 'gpt-6.1-sol' : 'gpt-6-luna',
        image: agentImage, model_connection: 'backend', network: 'none' }
    }) }))
    setImage(agentImage); setConnection('backend'); setNetwork('none')
  }
  const [qaImage, setQaImage] = useState(config.checks[0]?.image || nodeImage)
  useEffect(() => {
    if (!qaImage && nodeImage) setCheckImage(nodeImage)
  }, [nodeImage])
  function setCheckImage(next: string) {
    next =
      ['node', 'node:22', 'node:22-bookworm'].includes(next.trim()) && nodeImage
        ? nodeImage
        : next.trim()
    setQaImage(next)
    setConfig((c) => ({
      ...c,
      checks: c.checks.map((row) =>
        !row.image.trim() || row.image === qaImage ? { ...row, image: next } : row,
      ),
    }))
  }
  const [commandDrafts, setCommandDrafts] = useState<Record<string, string>>(draft?.commands ?? {})
  useEffect(() => {
    onDraft?.({ config, revision, commands: commandDrafts })
  }, [config, revision, commandDrafts])
  const disabled = externalDisabled || saving || loadingRoles.size > 0
  const stale = runtime.revision !== revision
  function agents(field: 'image' | 'model_connection' | 'network', text: string) {
    setConfig((c) => ({
      ...c,
      agents: agentRoles.map((r) => ({
        ...(c.agents.find((a) => a.role === r.id) ?? {
          role: r.id,
          model: r.model,
          image,
          model_connection: connection,
          network,
        }),
        [field]: text,
      })),
    }))
  }
  function command(key: string, initial: string[]) {
    return commandDrafts[key] ?? formatCommand(initial)
  }
  function removeDrafts(index: number, serviceRow: boolean) {
    const prefixes = serviceRow ? ['service-', 'health-'] : ['check-']
    setCommandDrafts((previous) =>
      Object.fromEntries(
        Object.entries(previous).flatMap(([key, value]) => {
          const prefix = prefixes.find((p) => key.startsWith(p))
          if (!prefix) return [[key, value]]
          const n = Number(key.slice(prefix.length))
          return n === index ? [] : [[prefix + (n > index ? n - 1 : n), value]]
        }),
      ),
    )
  }
  function check(index: number, patch: Partial<RuntimeCheck>) {
    setConfig((c) => ({
      ...c,
      checks: c.checks.map((s, i) => (i === index ? { ...s, ...patch } : s)),
    }))
  }
  function service(index: number, patch: Partial<RuntimeService>) {
    setConfig((c) => ({
      ...c,
      services: c.services.map((s, i) => (i === index ? { ...s, ...patch } : s)),
    }))
  }
  async function save() {
    setError('')
    setNotice('')
    setSaving(true)
    try {
      const updated = {
        ...config,
        checks: config.checks.map((c, i) => ({
          ...c,
          image: !c.image.trim()
            ? qaImage || nodeImage
            : ['node', 'node:22', 'node:22-bookworm'].includes(c.image.trim())
              ? nodeImage
              : c.image.trim(),
          command: parseCommand(command(`check-${i}`, c.command)),
        })),
        services: config.services.map((s, i) => ({
          ...s,
          command: parseCommand(command(`service-${i}`, s.command)),
          healthcheck: parseCommand(command(`health-${i}`, s.healthcheck)),
        })),
      }
      for (const [i, row] of updated.checks.entries()) {
        const label = `Comprobación ${i + 1} (${row.name})`
        if (!repositories.includes(row.cwd))
          throw new Error(`${label}: selecciona un repositorio autorizado.`)
        if (!/^(?:[a-zA-Z0-9._/:\-]+@)?sha256:[a-fA-F0-9]{64}$/.test(row.image))
          throw new Error(
            `${label}: elige el entorno Node.js. La imagen escrita no es un digest válido; no basta con un tag como node:22.`,
          )
        if (
          updated.checks.findIndex(
            (other) =>
              other.name === row.name &&
              other.cwd === row.cwd &&
              JSON.stringify(other.command) === JSON.stringify(row.command),
          ) !== i
        )
          throw new Error(`${label}: ya existe ese mismo comando y tipo para ese repositorio.`)
      }
      const saved = await onSave(revision, updated)
      setConfig(saved?.config ?? updated)
      setRevision(saved?.revision ?? revision + 1)
      setCommandDrafts({})
      setNotice(
        'Configuración guardada. El runner comprobará imágenes, conexiones y permisos antes de aplicarla.',
      )
    } catch (e) {
      setError(errorMessage(e, 'No se pudo guardar la configuración.'))
    } finally {
      setSaving(false)
    }
  }
  const refs = runtime.report?.environments ?? []
  return (
    <div className="@container space-y-6">
      <datalist id="runtime-images">
        {runtime.report?.images?.map((i) => <option key={i} value={i} />)}
      </datalist>
      {section === 'agents' && (
        <>
          <div>
            <h2 className="text-base font-semibold">Agentes y modelos</h2>
            <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
              Elige un perfil, un harness y un modelo por fase. Cada fase abre una sesión nueva; no
              hereda credenciales ni permisos.
            </p>
            {loadAgent ? (
              <a
                className="mt-2 inline-block text-sm underline underline-offset-4"
                href="/factory/agents"
                target="_blank"
                rel="noopener noreferrer"
              >
                Abrir catálogo o crear un perfil
              </a>
            ) : (
              <p className="mt-2 text-xs text-muted-foreground">
                El catálogo requiere autonomous_agent:read. Puedes configurar roles propios sin
                vincular un agente.
              </p>
            )}
          </div>
          <div className="space-y-3 rounded-lg bg-muted/50 p-3">
            <p className="text-sm font-medium">Pi administrado · cuenta del backend</p>
            <p className="text-xs text-muted-foreground">
              El SDK se prepara automáticamente en el runner. Pi trabaja sin acceso a internet;
              las solicitudes de modelo usan el OAuth del backend sin copiar credenciales ni conversaciones.
              Guardar no significa que el runtime esté aplicado ni que el QA haya pasado.
            </p>
            <Button variant="outline" className="h-auto whitespace-normal text-left" disabled={disabled} onClick={useBackendPi}>
              Usar Pi del backend en las cuatro fases
            </Button>
            <p className="text-xs text-muted-foreground">Cambia los harnesses y los modelos no compatibles con Pi. Conserva las comprobaciones y los servicios. Revisa y guarda los cambios.</p>
          </div>
          <details className="space-y-3 rounded-lg bg-muted/50 p-3">
            <summary className="cursor-pointer text-sm font-medium">
              Infraestructura personalizada · avanzado
            </summary>
            <p className="pt-3 text-xs text-muted-foreground">
              Pi administrado no necesita que escribas imágenes, proxies ni redes. Estos campos
              son solo para adaptadores personalizados autorizados por el operador.
            </p>
            <div className="grid gap-4 lg:grid-cols-3">
              <label className="space-y-1.5 text-sm">
                Imagen de agentes
                <Input
                  list="runtime-images"
                  value={image}
                  disabled={disabled}
                  placeholder="Imagen instalada @sha256:…"
                  onChange={(e) => {
                    setImage(e.target.value)
                    agents('image', e.target.value)
                  }}
                />
                <span className="block text-xs text-muted-foreground">
                  Se aplica a las cuatro fases. Solo imágenes instaladas y autorizadas.
                </span>
              </label>
              <label className="space-y-1.5 text-sm">
                Conexión de modelo
                <Input
                  aria-label="Conexión de modelo compartida"
                  list="model-connections"
                  value={connection}
                  disabled={disabled}
                  placeholder="Referencia del proxy, no una API key"
                  onChange={(e) => {
                    setConnection(e.target.value)
                    agents('model_connection', e.target.value)
                  }}
                />
                <datalist id="model-connections">
                  {runtime.report?.model_connections?.map((n) => <option key={n} value={n} />)}
                </datalist>
                <span className="block text-xs text-muted-foreground">
                  {runtime.report?.model_connections?.length
                    ? 'Conexiones verificadas por el runner.'
                    : 'Todavía no hay un proxy de modelo conectado.'}
                </span>
              </label>
              <label className="space-y-1.5 text-sm">
                Red interna del proxy
                <Input
                  list="model-networks"
                  value={network}
                  disabled={disabled}
                  placeholder="Nombre autorizado en el runner"
                  onChange={(e) => {
                    setNetwork(e.target.value)
                    agents('network', e.target.value)
                  }}
                />
                <datalist id="model-networks">
                  {runtime.report?.networks?.map((n) => <option key={n} value={n} />)}
                </datalist>
                <span className="block text-xs text-muted-foreground">
                  No se permite red del host ni acceso directo a internet.
                </span>
              </label>
            </div>
          </details>
          <div className="divide-y divide-border rounded-xl border border-border">
            {agentRoles.map((role) => {
              const current = config.agents.find((a) => a.role === role.id)
              return (
                <div
                  key={role.id}
                  className="grid gap-4 p-4 @3xl:grid-cols-[minmax(200px,1fr)_minmax(320px,1.3fr)]"
                >
                  <div>
                    <h3 className="text-sm font-medium">{role.label}</h3>
                    <p className="mt-1 max-w-xl text-sm text-muted-foreground">
                      {role.description}
                    </p>
                  </div>
                  <AgentRoleEditor
                    label={role.label}
                    value={
                      current ?? {
                        role: role.id,
                        image,
                        model: role.model,
                        model_connection: connection,
                        network,
                      }
                    }
                    agents={catalog}
                    onLoadingChange={(loading) =>
                      setLoadingRoles((previous) => {
                        const next = new Set(previous)
                        if (loading) next.add(role.id)
                        else next.delete(role.id)
                        return next
                      })
                    }
                    availableHarnesses={runtime.report.harnesses ?? []}
                    disabled={disabled}
                    loadAgent={loadAgent}
                    onChange={(patch) =>
                      setConfig((c) => ({
                        ...c,
                        agents: agentRoles.map((r) => ({
                          ...(c.agents.find((a) => a.role === r.id) ?? {
                            role: r.id,
                            image,
                            model: r.model,
                            model_connection: connection,
                            network,
                          }),
                          ...(r.id === role.id ? patch : {}),
                        })),
                      }))
                    }
                  />
                </div>
              )
            })}
          </div>
          <datalist id="agent-models">
            <option value="haiku" />
            <option value="sonnet" />
            <option value="opus" />
          </datalist>
          <p className="flex max-w-2xl gap-2 text-sm text-muted-foreground">
            <ShieldCheck className="mt-0.5 size-4 shrink-0" />
            El revisor es de solo lectura y no recibe entornos de la aplicación. Una revisión
            interna no es una aprobación formal de GitHub.
          </p>
        </>
      )}
      {section === 'checks' && (
        <>
          <div>
            <h2 className="text-base font-semibold">Tests, build y QA</h2>
            <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
              Comandos reales, por repositorio. Tests y build se ejecutan antes de levantar
              servicios; el QA funcional, visual y de seguridad utiliza el entorno integrado.
            </p>
          </div>
          <label className="block max-w-xl space-y-1.5 text-sm">
            Entorno de las comprobaciones
            <select
              className={select}
              aria-label="Entorno de las comprobaciones"
              disabled={disabled}
              value={nodeImage && qaImage === nodeImage ? 'node' : 'custom'}
              onChange={(e) => setCheckImage(e.target.value === 'node' ? nodeImage : '')}
            >
              <option value="node" disabled={!nodeImage}>
                Node.js 22 · preparación automática{!nodeImage ? ' (pendiente del runner)' : ''}
              </option>
              <option value="custom">Otro entorno · configuración avanzada</option>
            </select>
            <span className="block text-xs text-muted-foreground">
              El workflow descarga el entorno autorizado y prepara las dependencias del lockfile sin
              credenciales ni scripts de instalación. No necesitas instalar imágenes a mano.
            </span>
          </label>
          <details className="max-w-xl text-sm">
            <summary className="cursor-pointer text-muted-foreground">
              Imagen avanzada por digest
            </summary>
            <label className="mt-2 block space-y-1.5 text-sm">
              Imagen común para las comprobaciones
              <Input
                list="runtime-images"
                value={qaImage}
                disabled={disabled}
                placeholder="SDK y herramientas de pruebas @sha256:…"
                onChange={(e) => setCheckImage(e.target.value)}
              />
            </label>
          </details>
          {!config.checks.length && (
            <div className="rounded-xl border border-dashed border-border px-5 py-7">
              <h3 className="font-medium">Todavía no hay comprobaciones</h3>
              <p className="mt-1 max-w-xl text-sm text-muted-foreground">
                Añade tests y build para los repositorios que cambian, y después las tres revisiones
                de QA. Un nombre sin un comando no cuenta como evidencia.
              </p>
            </div>
          )}
          <div className="divide-y divide-border">
            {config.checks.map((row, i) => (
              <div key={i} className="space-y-3 py-4">
                <div className="grid gap-3 @xl:grid-cols-2 @3xl:grid-cols-[160px_220px_minmax(180px,1fr)_36px]">
                  <label className="space-y-1 text-xs">
                    Comprobación
                    <select
                      className={select}
                      value={row.name}
                      disabled={disabled}
                      onChange={(e) => check(i, { name: e.target.value })}
                    >
                      {checkTypes.map((t) => (
                        <option key={t.id} value={t.id}>
                          {t.label}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="space-y-1 text-xs">
                    Repositorio
                    <select
                      className={select}
                      value={row.cwd}
                      disabled={disabled}
                      onChange={(e) => check(i, { cwd: e.target.value })}
                    >
                      {repositories.map((r) => (
                        <option key={r}>{r}</option>
                      ))}
                    </select>
                  </label>
                  <label className="space-y-1 text-xs">
                    Comando
                    <Input
                      value={command(`check-${i}`, row.command)}
                      disabled={disabled}
                      placeholder="npm run test -- --run"
                      onChange={(e) =>
                        setCommandDrafts((c) => ({ ...c, [`check-${i}`]: e.target.value }))
                      }
                    />
                  </label>
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={disabled}
                    aria-label={`Eliminar comprobación ${i + 1}`}
                    onClick={() => {
                      setConfig((c) => ({ ...c, checks: c.checks.filter((_, n) => i !== n) }))
                      removeDrafts(i, false)
                    }}
                    className="self-end"
                  >
                    <Trash2 className="size-4" />
                  </Button>
                </div>
                <details className="text-sm">
                  <summary className="cursor-pointer text-muted-foreground">
                    Imagen y entorno de esta comprobación
                  </summary>
                  <div className="mt-3 grid gap-3 sm:grid-cols-2">
                    <label className="space-y-1 text-xs">
                      Imagen
                      <Input
                        list="runtime-images"
                        value={row.image}
                        disabled={disabled}
                        onChange={(e) => check(i, { image: e.target.value })}
                      />
                    </label>
                    <label className="space-y-1 text-xs">
                      Referencias de entorno, separadas por coma
                      <Input
                        value={row.environment_refs.join(', ')}
                        disabled={disabled}
                        placeholder={refs.join(', ') || 'Solo referencias privadas registradas'}
                        onChange={(e) =>
                          check(i, {
                            environment_refs: e.target.value
                              .split(',')
                              .map((s) => s.trim())
                              .filter(Boolean),
                          })
                        }
                      />
                    </label>
                  </div>
                </details>
              </div>
            ))}
          </div>
          <Button
            type="button"
            variant="secondary"
            disabled={disabled || !repositories.length}
            onClick={() =>
              setConfig((c) => ({
                ...c,
                checks: [
                  ...c.checks,
                  {
                    name: 'tests',
                    cwd: repositories[0],
                    image: qaImage,
                    command: [],
                    environment_refs: [],
                  },
                ],
              }))
            }
          >
            <Plus className="mr-2 size-4" />
            Añadir comprobación
          </Button>
        </>
      )}
      {section === 'services' && (
        <>
          <div>
            <h2 className="text-base font-semibold">Servicios y entornos de pruebas</h2>
            <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
              Bases de datos, backend y frontend desechables, en una red interna. No se conecta
              automáticamente ninguna base o archivo .env de producción.
            </p>
          </div>
          {!config.services.length && (
            <div className="rounded-xl border border-dashed border-border px-5 py-7">
              <h3 className="font-medium">Define el entorno que verificará el cambio</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Cada servicio necesita imagen, comando de inicio y healthcheck. El QA espera a que
                todos estén saludables.
              </p>
            </div>
          )}
          <div className="divide-y divide-border">
            {config.services.map((row, i) => (
              <fieldset key={i} className="space-y-3 py-4">
                <legend className="sr-only">Servicio {i + 1}</legend>
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-medium">{row.name || 'Nuevo servicio'}</h3>
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={disabled}
                    aria-label={`Eliminar servicio ${i + 1}`}
                    onClick={() => {
                      setConfig((c) => ({ ...c, services: c.services.filter((_, n) => n !== i) }))
                      removeDrafts(i, true)
                    }}
                  >
                    <Trash2 className="size-4" />
                  </Button>
                </div>
                <div className="grid gap-3 sm:grid-cols-2">
                  <label className="space-y-1 text-xs">
                    Nombre de red
                    <Input
                      value={row.name}
                      disabled={disabled}
                      placeholder="backend"
                      onChange={(e) => service(i, { name: e.target.value })}
                    />
                  </label>
                  <label className="space-y-1 text-xs">
                    Repositorio
                    <select
                      className={select}
                      value={row.cwd}
                      disabled={disabled}
                      onChange={(e) => service(i, { cwd: e.target.value })}
                    >
                      {repositories.map((r) => (
                        <option key={r}>{r}</option>
                      ))}
                    </select>
                  </label>
                  <label className="space-y-1 text-xs sm:col-span-2">
                    Imagen instalada
                    <Input
                      list="runtime-images"
                      value={row.image}
                      disabled={disabled}
                      placeholder="Imagen @sha256:…"
                      onChange={(e) => service(i, { image: e.target.value })}
                    />
                  </label>
                  <label className="space-y-1 text-xs">
                    Comando de inicio
                    <Input
                      value={command(`service-${i}`, row.command)}
                      disabled={disabled}
                      placeholder="npm run start"
                      onChange={(e) =>
                        setCommandDrafts((c) => ({ ...c, [`service-${i}`]: e.target.value }))
                      }
                    />
                  </label>
                  <label className="space-y-1 text-xs">
                    Healthcheck
                    <Input
                      value={command(`health-${i}`, row.healthcheck)}
                      disabled={disabled}
                      placeholder="python3 -m qa.check_health"
                      onChange={(e) =>
                        setCommandDrafts((c) => ({ ...c, [`health-${i}`]: e.target.value }))
                      }
                    />
                  </label>
                  <label className="space-y-1 text-xs sm:col-span-2">
                    Referencias de entorno, separadas por coma
                    <Input
                      value={row.environment_refs.join(', ')}
                      disabled={disabled}
                      placeholder={refs.join(', ') || 'Ejemplo de referencia: backend-dev'}
                      onChange={(e) =>
                        service(i, {
                          environment_refs: e.target.value
                            .split(',')
                            .map((s) => s.trim())
                            .filter(Boolean),
                        })
                      }
                    />
                  </label>
                </div>
              </fieldset>
            ))}
          </div>
          <Button
            type="button"
            variant="secondary"
            disabled={disabled || !repositories.length}
            onClick={() =>
              setConfig((c) => ({
                ...c,
                services: [
                  ...c.services,
                  {
                    name: '',
                    cwd: repositories[0],
                    image: '',
                    command: [],
                    healthcheck: [],
                    environment_refs: [],
                  },
                ],
              }))
            }
          >
            <Plus className="mr-2 size-4" />
            Añadir servicio
          </Button>
        </>
      )}
      <div className="flex flex-wrap items-center gap-3 border-t border-border pt-4">
        <Button
          type="button"
          disabled={disabled || saving || stale}
          onClick={() => {
            void save()
          }}
        >
          {saving ? 'Guardando…' : 'Guardar configuración'}
        </Button>
        <span className="text-xs text-muted-foreground">
          Revisión {revision} · aplicada por el runner: {runtime.report?.applied_revision ?? 0}
        </span>
      </div>
      {stale && (
        <InlineError message="Hay otra revisión guardada. Recarga la configuración antes de sobrescribirla." />
      )}
      {error && <InlineError message={error} />}
      {notice && (
        <p role="status" className="text-sm">
          {notice}
        </p>
      )}
      <details className="rounded-lg bg-muted/50 p-4 text-sm">
        <summary className="flex cursor-pointer items-center gap-2 font-medium">
          <Info className="size-4" />
          Preparación local y seguridad
        </summary>
        <div className="mt-3 max-w-2xl space-y-2 text-muted-foreground">
          <p>
            El administrador configura aquí modelos y comandos. El operador del runner debe
            autorizar una vez <code>allow_remote_configuration</code>, las imágenes por digest y las
            redes internas permitidas.
          </p>
          <p>
            Las conexiones de modelo referencian un proxy local autorizado; no admiten claves
            maestras del proveedor. Los archivos de entorno permanecen fuera del código, con
            permisos 0600. Aquí se guardan solo sus nombres de referencia.
          </p>
          <p>
            Guardar no equivale a que las pruebas hayan pasado: el estado operativo se confirma
            cuando el runner aplica la revisión. Los resultados de QA aparecen en cada ejecución.
          </p>
        </div>
      </details>
    </div>
  )
}
