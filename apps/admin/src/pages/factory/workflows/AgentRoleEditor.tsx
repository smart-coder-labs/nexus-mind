import { useEffect, useRef, useState } from 'react'
import type { AutonomousAgentDefinition } from '../../../types'
import { Input } from '../../../components/ui/Input'
import { InlineError, errorMessage } from '../shared/ui'
import type { RuntimeAgent, WorkflowAgentProfile } from './runtime'
const select =
  'h-9 w-full min-w-0 rounded-lg border border-input bg-background px-3 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring'
export function AgentRoleEditor({
  label,
  value,
  agents,
  availableHarnesses,
  disabled,
  onChange,
  loadAgent,
  onLoadingChange,
}: {
  label: string
  value: RuntimeAgent
  agents: AutonomousAgentDefinition[]
  availableHarnesses: string[]
  disabled: boolean
  onChange: (patch: Partial<RuntimeAgent>) => void
  loadAgent?: (id: string) => Promise<WorkflowAgentProfile>
  onLoadingChange?: (loading: boolean) => void
}) {
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [profileHasModel, setProfileHasModel] = useState<boolean | undefined>()
  const generation = useRef(0)
  useEffect(
    () => () => {
      generation.current++
      onLoadingChange?.(false)
    },
    [],
  )
  async function choose(id: string) {
    const current = ++generation.current
    setError('')
    setLoading(false)
    onLoadingChange?.(false)
    if (!id) {
      setProfileHasModel(undefined)
      onChange({ binding: null, inherit_model: false, instructions: '' })
      return
    }
    if (!loadAgent) {
      setError('Necesitas permiso para leer el catálogo de agentes.')
      return
    }
    setLoading(true)
    onLoadingChange?.(true)
    try {
      const detail = await loadAgent(id)
      if (current !== generation.current) return
      if (
        detail.status !== 'enabled' ||
        detail.validation_status !== 'valid' ||
        !['github_issue_resolver', 'github_pr_reviewer'].includes(detail.template_key)
      )
        throw new Error('Este perfil no es compatible o ya no está habilitado y validado.')
      const hasModel =
        typeof detail.revision.config.model === 'string' && !!detail.revision.config.model.trim()
      const harness =
        typeof detail.revision.config.executor === 'string'
          ? detail.revision.config.executor
          : 'claude'
      setProfileHasModel(hasModel)
      onChange({
        binding: {
          id: detail.id,
          revision: detail.revision.revision,
          name: detail.name,
          config_hash: detail.revision.config_hash,
        },
        inherit_model: hasModel,
        model: hasModel
          ? (detail.revision.config.model as string)
          : harness === (value.harness ?? 'claude')
            ? value.model
            : '',
        harness,
      })
    } catch (e) {
      if (current === generation.current) setError(errorMessage(e, 'No se pudo cargar el perfil.'))
    } finally {
      if (current === generation.current) {
        setLoading(false)
        onLoadingChange?.(false)
      }
    }
  }
  const harness = value.harness ?? 'claude'
  const supported = availableHarnesses.includes(harness)
  const selected = agents.find((a) => a.id === value.binding?.id)
  const stale =
    !!value.binding &&
    (!selected || selected.status !== 'enabled' || selected.validation_status !== 'valid')
  return (
    <div className="space-y-3">
      <div className="grid gap-3 @xl:grid-cols-2">
        <label className="space-y-1 text-xs">
          Perfil de agente · {label}
          <select
            aria-label={`Perfil de agente · ${label}`}
            className={select}
            disabled={disabled || loading || !loadAgent}
            value={value.binding?.id ?? ''}
            onChange={(e) => {
              void choose(e.target.value)
            }}
          >
            <option value="">Rol propio del workflow</option>
            {value.binding && !selected && (
              <option value={value.binding.id}>{value.binding.name} · no disponible</option>
            )}
            {agents.map((a) => {
              const compatible =
                ['github_issue_resolver', 'github_pr_reviewer'].includes(a.template_key) &&
                a.status === 'enabled' &&
                a.validation_status === 'valid'
              return (
                <option key={a.id} value={a.id} disabled={!compatible}>
                  {a.name}
                  {!compatible ? ' · no compatible / no validado' : ''}
                </option>
              )
            })}
          </select>
        </label>
        <label className="space-y-1 text-xs">
          Harness · {label}
          <select
            aria-label={`Harness · ${label}`}
            className={select}
            disabled={disabled || loading}
            value={harness}
            onChange={(e) => onChange({ harness: e.target.value })}
          >
            {[...new Set(['claude', 'pi', 'codex', 'nexus', ...availableHarnesses, harness])].map(
              (h) => (
                <option key={h} value={h}>
                  {(
                    { claude: 'Claude Code', pi: 'Pi', codex: 'Codex', nexus: 'Nexus' } as Record<
                      string,
                      string
                    >
                  )[h] ?? h}
                  {!availableHarnesses.includes(h) ? h === 'pi' ? ' · runtime / sesión pendientes' : ' · requiere adaptador local' : ''}
                </option>
              ),
            )}
          </select>
        </label>
      </div>
      {value.binding && (
        <p className="text-xs text-muted-foreground">
          {value.binding.name} · revisión mostrada {value.binding.revision}. Se fijará la revisión
          validada vigente al iniciar cada ejecución.
        </p>
      )}
      {stale && (
        <p role="alert" className="text-sm text-status-warning">
          El perfil vinculado ya no está habilitado y validado. El backend bloqueará nuevas
          ejecuciones.
        </p>
      )}
      {value.binding && (
        <label className="flex items-center gap-2 text-xs">
          <input
            type="checkbox"
            checked={value.inherit_model ?? false}
            disabled={disabled || loading || profileHasModel === false}
            onChange={(e) => onChange({ inherit_model: e.target.checked })}
          />
          Heredar el modelo del agente · {label}
        </label>
      )}
      <label className="block space-y-1 text-xs">
        Modelo · {label}
        <Input
          value={value.model}
          disabled={disabled || loading || !!value.inherit_model}
          list="agent-models"
          placeholder="Modelo compatible con el harness"
          onChange={(e) => onChange({ model: e.target.value, inherit_model: false })}
        />
      </label>
      {!supported && (
        <p className="text-xs text-muted-foreground">
          {harness === 'pi'
            ? 'Pi administrado requiere que el SDK automático y la sesión OAuth del backend estén disponibles. No tienes que escribir una imagen ni copiar una sesión al runner.'
            : 'Este harness aún no está anunciado por el runner. Puedes guardar el perfil, pero no se ejecutará ni cambiará a otro harness como fallback. Su imagen y proxy deben estar autorizados.'}
        </p>
      )}
      {value.binding && profileHasModel === false && (
        <p className="text-xs text-muted-foreground">
          Este perfil usa selección automática en su ejecución individual. Aquí se utiliza el modelo
          visible de esta fase; puedes fijar un modelo en el catálogo para heredarlo.
        </p>
      )}
      {!value.model && (
        <p className="text-xs text-muted-foreground">
          El perfil no define un modelo explícito. Desactiva la herencia y selecciona uno
          compatible.
        </p>
      )}
      {harness === 'pi' && value.model_connection === 'backend' && (
        <p className="text-xs text-muted-foreground">Runtime administrado · sandbox sin red · OAuth solo en el backend · sesión nueva por fase.</p>
      )}
      <details className="text-xs text-muted-foreground">
        <summary className="cursor-pointer">Infraestructura de esta fase · avanzado</summary>
        <div className="mt-3 space-y-3">
          <label className="block space-y-1">
            Imagen · {label}
            <Input
              aria-label={`Imagen · ${label}`}
              value={value.image}
              disabled={disabled || loading}
              list="runtime-images"
              onChange={(e) => onChange({ image: e.target.value })}
            />
          </label>
          <label className="block space-y-1">
            Conexión de modelo · {label}
            <Input
              value={value.model_connection}
              disabled={disabled || loading}
              list="model-connections"
              onChange={(e) => onChange({ model_connection: e.target.value })}
            />
          </label>
          <label className="block space-y-1">
            Red interna · {label}
            <Input
              value={value.network}
              disabled={disabled || loading}
              list="model-networks"
              onChange={(e) => onChange({ network: e.target.value })}
            />
          </label>
        </div>
      </details>
      {loading && (
        <p role="status" className="text-xs">
          Cargando perfil…
        </p>
      )}
      {error && <InlineError message={error} />}
    </div>
  )
}
