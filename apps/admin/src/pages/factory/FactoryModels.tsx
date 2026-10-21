import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, KeyRound, LockKeyhole } from 'lucide-react'
import { createClient } from '../../api/client'
import { useAuth } from '../../auth/AuthContext'
import { Badge } from '../../components/ui/Badge'
import { Button } from '../../components/ui/Button'
import { Skeleton } from '../../components/ui/Skeleton'
import { cn } from '../../lib/utils'
import type { FactoryModelProvider, FactoryModelRole, FactoryModelRoleName, FactoryModelRoles } from '../../types'
import { PageHeader, PermissionDenied } from '../factory/govern/ui'

const roles: { key: FactoryModelRoleName; title: string; description: string }[] = [
  { key: 'planner', title: 'Plan', description: 'Diseña el enfoque y define el trabajo antes de implementarlo.' },
  { key: 'reviewer', title: 'Review', description: 'Revisa cambios, pruebas y criterios de aceptación.' },
  { key: 'implementer', title: 'Implement', description: 'Realiza los cambios siguiendo el plan aprobado.' },
]

const PROVIDERS: { id: FactoryModelProvider; label: string }[] = [
  { id: 'anthropic', label: 'Anthropic' },
  { id: 'openai_api', label: 'OpenAI API key' },
  { id: 'chatgpt_plan', label: 'ChatGPT plan (OAuth)' },
]

const MODELS: Record<FactoryModelProvider, string[]> = {
  anthropic: ['haiku', 'sonnet', 'opus'],
  openai_api: ['gpt-6-luna', 'gpt-6.1-sol', 'gpt-6-astra'],
  chatgpt_plan: ['gpt-6-luna', 'gpt-6.1-sol', 'gpt-6-astra'],
}

const FALLBACK_MODEL: Record<FactoryModelProvider, string> = {
  anthropic: 'sonnet',
  openai_api: 'gpt-6.1-sol',
  chatgpt_plan: 'gpt-6.1-sol',
}

function credentialLabel(value: string): { label: string; variant: 'success' | 'warning' | 'default' } {
  if (value === 'configured' || value === 'connected') return { label: 'Conectada', variant: 'success' }
  if (value === 'unknown') return { label: 'No conectada · estado no verificable', variant: 'warning' }
  return { label: 'No conectada', variant: 'warning' }
}

function StatusCard({ title, detail, status }: { title: string; detail: string; status: string }) {
  const meta = credentialLabel(status)
  return <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border-primary bg-card px-4 py-3">
    <div className="min-w-0"><h3 className="text-sm font-medium text-text-primary">{title}</h3><p className="mt-0.5 text-xs text-text-tertiary">{detail}</p></div>
    <Badge role="status" variant={meta.variant}>{meta.label}</Badge>
  </div>
}

function cloneRoles(value: FactoryModelRoles): FactoryModelRoles {
  return {
    planner: { ...value.planner },
    reviewer: { ...value.reviewer },
    implementer: { ...value.implementer },
  }
}

function equalRoles(a: FactoryModelRoles | null, b: FactoryModelRoles | null): boolean {
  return Boolean(a && b && roles.every(({ key }) => a[key].provider === b[key].provider && a[key].model === b[key].model))
}

function RoleCard({
  roleKey, title, description, value, canWrite, onChange,
}: {
  roleKey: FactoryModelRoleName
  title: string
  description: string
  value: FactoryModelRole
  canWrite: boolean
  onChange: (next: FactoryModelRole) => void
}) {
  const modelOptions = MODELS[value.provider]
  const selectedModelExists = modelOptions.includes(value.model)
  return <section className="space-y-4 rounded-xl border border-border-primary bg-card p-4 sm:p-5" aria-labelledby={`role-${roleKey}`}>
    <div><h3 id={`role-${roleKey}`} className="text-[15px] font-semibold text-text-primary">{title}</h3><p className="mt-1 text-sm text-text-secondary">{description}</p></div>
    <div className="grid gap-4 sm:grid-cols-2">
      <div className="space-y-1.5">
        <label htmlFor={`${roleKey}-provider`} className="text-xs font-medium text-text-secondary">Proveedor</label>
        <select id={`${roleKey}-provider`} className={cn('h-9 w-full rounded-md border border-border-primary bg-background px-3 text-sm text-text-primary', 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:opacity-50')} value={value.provider} disabled={!canWrite} onChange={event => {
          const provider = event.target.value as FactoryModelProvider
          onChange({ provider, model: MODELS[provider].includes(value.model) ? value.model : FALLBACK_MODEL[provider] })
        }}>
          {PROVIDERS.map(provider => <option key={provider.id} value={provider.id}>{provider.label}</option>)}
        </select>
      </div>
      <div className="space-y-1.5">
        <label htmlFor={`${roleKey}-model`} className="text-xs font-medium text-text-secondary">Modelo</label>
        <select id={`${roleKey}-model`} className={cn('h-9 w-full rounded-md border border-border-primary bg-background px-3 text-sm text-text-primary', 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:opacity-50')} value={value.model} disabled={!canWrite} onChange={event => onChange({ ...value, model: event.target.value })}>
          {!selectedModelExists && <option value={value.model}>{value.model}</option>}
          {modelOptions.map(model => <option key={model} value={model}>{model}</option>)}
        </select>
      </div>
    </div>
  </section>
}

/** Provider and modmin role assignments. Credential readiness is visible but no secret or OAuth flow is exposed here. */
export default function FactoryModels() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState<FactoryModelRoles | null>(null)
  const config = useQuery({ queryKey: ['factory-model-config'], queryFn: () => client.getFactoryModelConfig(), enabled: canRead })
  const save = useMutation({
    mutationFn: (roles: FactoryModelRoles) => client.putFactoryModelConfig({ roles }),
    onSuccess: value => {
      setDraft(cloneRoles(value.roles))
      queryClient.setQueryData(['factory-model-config'], value)
    },
  })
  useEffect(() => { if (config.data && draft === null) setDraft(cloneRoles(config.data.roles)) }, [config.data, draft])

  if (!canRead) return <PermissionDenied title="Provider y modmin" permission="factory_policy:read" what="ver la configuración de modelos del factory" />

  const setRole = (key: FactoryModelRoleName, value: FactoryModelRole) => {
    setDraft(current => current ? { ...current, [key]: value } : current)
    save.reset()
  }
  const dirty = !equalRoles(draft, config.data?.roles ?? null)
  const chatGptSelected = draft ? roles.some(({ key }) => draft[key].provider === 'chatgpt_plan') : false
  const hasDisconnectedProvider = draft ? roles.some(({ key }) => {
    const provider = draft[key].provider
    return provider === 'chatgpt_plan'
      ? config.data?.credentials.chatgpt_plan === 'not_connected'
      : config.data?.credentials[provider] === 'unknown'
  }) : false

  return <div className="mx-auto min-w-0 max-w-5xl space-y-6 p-6 md:p-8">
    <PageHeader title="Provider y modmin" subtitle="Elige el proveedor y el modelo de cada rol. El default de modmin es Sol para plan y review, y Luna para implement." />

    {config.isLoading && <div className="space-y-3" aria-label="Cargando configuración"><Skeleton className="h-20 w-full" /><Skeleton className="h-32 w-full" /><Skeleton className="h-32 w-full" /></div>}
    {config.isError && <div role="alert" className="rounded-lg border border-status-error/25 bg-status-error/[0.08] p-4 text-sm text-text-primary">
      No se pudo cargar la configuración. Confirma que el backend implemente GET /v1/factory/model-config. <Button size="sm" variant="secondary" className="ml-2" onClick={() => void config.refetch()}>Reintentar</Button>
    </div>}

    {config.data && draft && <>
      <section className="space-y-3" aria-labelledby="provider-credentials-title">
        <div><h2 id="provider-credentials-title" className="text-[15px] font-semibold text-text-primary">Estado de credenciales</h2><p className="mt-1 text-sm text-text-secondary">El backend solo muestra disponibilidad. Esta página nunca lee ni almacena API keys o tokens.</p></div>
        <div className="grid gap-3 md:grid-cols-3">
          <StatusCard title="Anthropic" detail="Credencial del proxy de salida" status={config.data.credentials.anthropic} />
          <StatusCard title="OpenAI API key" detail="Credencial del proxy de salida" status={config.data.credentials.openai_api} />
          <StatusCard title="ChatGPT plan · OAuth" detail="Autorización de cuenta personal" status={config.data.credentials.chatgpt_plan} />
        </div>
      </section>

      <section className="space-y-3" aria-labelledby="modmin-title">
        <div><h2 id="modmin-title" className="text-[15px] font-semibold text-text-primary">Roles del agente</h2><p className="mt-1 text-sm text-text-secondary">Planifica, revisa e implementa con asignaciones independientes.</p></div>
        <div className="grid gap-3 lg:grid-cols-3">
          {roles.map(role => <RoleCard key={role.key} roleKey={role.key} title={role.title} description={role.description} value={draft[role.key]} canWrite={canWrite} onChange={value => setRole(role.key, value)} />)}
        </div>
      </section>

      {!config.data.execution_ready && <p className="flex gap-2 rounded-lg border border-status-warning/25 bg-status-warning/[0.08] p-4 text-sm text-text-primary" role="status">
        <LockKeyhole className="mt-0.5 h-4 w-4 shrink-0 text-status-warning" aria-hidden="true" />
        <span>Guardar preferencias no cambia todavía el proveedor que usa el runtime. Estas asignaciones no están habilitadas para ejecutar; si el proveedor seleccionado no está conectado, la ejecución debe bloquearse. No se hará fallback silencioso a Claude ni a otro proveedor.</span>
      </p>}
      {chatGptSelected && <p className="flex gap-2 rounded-lg border border-status-warning/25 bg-status-warning/[0.08] p-4 text-sm text-text-primary" role="status">
        <LockKeyhole className="mt-0.5 h-4 w-4 shrink-0 text-status-warning" aria-hidden="true" />
        <span>ChatGPT plan OAuth aún no está conectado y el runtime tampoco está integrado. La asignación puede guardarse, pero una conexión OAuth por sí sola no habilitaría ejecuciones. No hay flujo de autorización activo aquí.</span>
      </p>}
      {!chatGptSelected && hasDisconnectedProvider && <p className="flex gap-2 rounded-lg border border-status-warning/25 bg-status-warning/[0.08] p-4 text-sm text-text-primary" role="status">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-status-warning" aria-hidden="true" />
        <span>Falta una credencial para uno o más proveedores seleccionados. Las ejecuciones deben bloquearse sin fallback a otro proveedor.</span>
      </p>}

      <div className="flex flex-wrap items-center gap-3">
        {canWrite ? <Button onClick={() => draft && save.mutate(cloneRoles(draft))} disabled={!dirty || save.isPending} loading={save.isPending} leftIcon={<KeyRound className="h-4 w-4" aria-hidden="true" />}>Guardar configuración</Button> : <p className="text-xs text-text-tertiary">Necesitas `factory_policy:write` para cambiar la asignación.</p>}
        {save.isSuccess && <span role="status" className="text-sm text-status-success">Configuración guardada.</span>}
        {save.isError && <span role="alert" className="text-sm text-status-error">No se guardó la configuración: {(save.error as Error).message}.</span>}
        {canWrite && !dirty && !save.isPending && <span className="text-xs text-text-tertiary">Sin cambios pendientes.</span>}
      </div>
      {config.data.updated_at && <p className="text-xs text-text-tertiary">Actualizado {new Date(config.data.updated_at).toLocaleString()}{config.data.updated_by ? ` · ${config.data.updated_by}` : ''}</p>}
    </>}
  </div>
}
