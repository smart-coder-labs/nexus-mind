import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import { InlineError, SectionTitle, errorMessage, useFactory } from '../shared/ui'
export function GithubReviewHooks() {
  const { client, can } = useFactory()
  const readable = can('autonomous_agent:read')
  const writable = can('factory_policy:write') && can('autonomous_agent:manage_connectors')
  const hooks = useQuery({
    queryKey: ['github-review-hooks'],
    queryFn: () => client.getGithubReviewHooks(),
    enabled: readable,
  })
  const agents = useQuery({
    queryKey: ['github-review-hook-agents'],
    queryFn: () => client.listAutonomousAgents(),
    enabled: readable,
  })
  const reviewers =
    agents.data?.filter(
      (a) =>
        a.template_key === 'github_pr_reviewer' &&
        a.status === 'enabled' &&
        a.validation_status === 'valid',
    ) ?? []
  const [agent, setAgent] = useState('')
  const [repository, setRepository] = useState('')
  const [base, setBase] = useState(import.meta.env.VITE_API_URL ?? '')
  const [administrationToken, setAdministrationToken] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  async function action(work: () => Promise<unknown>) {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      await work()
      await hooks.refetch()
    } catch (e) {
      setError(errorMessage(e, 'No se pudo configurar el webhook de GitHub.'))
      await hooks.refetch()
    } finally {
      setAdministrationToken('')
      setBusy(false)
    }
  }
  if (!readable)
    return (
      <p className="text-sm text-muted-foreground">
        Necesitas autonomous_agent:read para consultar esta integración.
      </p>
    )
  return (
    <section
      className="space-y-5 rounded-xl border border-border bg-card p-5 sm:p-6"
      aria-label="Solicitudes de revisión de GitHub"
    >
      <div className="space-y-2">
        <SectionTitle>Webhooks por repositorio</SectionTitle>
        <p className="max-w-2xl text-sm text-muted-foreground">
          Usa la cuenta autenticada en el CLI gh del backend, no tu sesión del navegador ni la del
          modelo. Cada commit nuevo recibe otra revisión vinculada a su SHA.
        </p>
        <p className="text-sm">
          Cuenta revisora:{' '}
          <strong>{hooks.data?.login ?? 'No disponible · autentica gh en el worker'}</strong>
        </p>
      </div>
      {(error || hooks.isError || agents.isError) && (
        <InlineError
          message={
            error ||
            errorMessage(
              hooks.error ?? agents.error,
              'No se pudo cargar la integración con GitHub.',
            )
          }
          onRetry={() => {
            void hooks.refetch()
            void agents.refetch()
          }}
        />
      )}
      {notice && (
        <p role="status" className="rounded-lg bg-muted p-3 text-sm">
          {notice}
        </p>
      )}
      {writable && <div className="space-y-2">
        <label className="block space-y-1.5 text-sm">
          Token de administración del webhook (opcional)
          <Input type="password" autoComplete="new-password" value={administrationToken}
            onChange={(e) => setAdministrationToken(e.target.value)} disabled={busy}
            placeholder="Webhooks: Read and write en GitHub" aria-describedby="webhook-token-help" />
        </label>
        <p id="webhook-token-help" className="max-w-2xl text-xs text-muted-foreground">
          Se guarda cifrado por organización y repositorio para conectar, reconectar y borrar hooks.
          Déjalo vacío para reutilizar el token guardado; si no existe, se usa gh del backend.
          Nunca llega a los agentes ni cambia la cuenta revisora. Para proyectos, selecciona un
          repositorio si el token solo tiene acceso a ese repositorio.
        </p>
      </div>}
      {!!hooks.data?.hooks.length && (
        <ul className="divide-y divide-border">
          {hooks.data.hooks.map((h) => (
            <li key={h.id} className="flex flex-wrap items-center justify-between gap-3 py-3">
              <div className="min-w-0 text-sm">
                <p>
                  {h.metadata.repository} · {h.health === 'revoked' ? 'Revocado' : h.health}
                </p>
                <p className="mt-1 break-all text-xs text-muted-foreground">
                  {h.metadata.callback_url}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">{h.management_credential_id ? 'Token de administración guardado y cifrado' : 'Sin token de administración guardado'}</p>
                {h.health === 'revoked' && h.metadata.remote_deleted !== true && <p className="mt-1 text-xs">Desconectado localmente; falta eliminarlo en GitHub.</p>}
              </div>
              {writable && h.management_credential_id && <Button size="sm" variant="secondary" disabled={busy} onClick={() => {
                const id = h.management_credential_id
                if (id && window.confirm('¿Olvidar el token guardado? Esto no revoca el PAT en GitHub.')) void action(async () => {
                  await client.revokeAutonomousAgentConnector(id)
                  setNotice('Token guardado eliminado. Revoca también el PAT en GitHub si ya no lo necesitas.')
                })
              }}>Olvidar token</Button>}
              {writable && (h.health !== 'revoked' || h.metadata.remote_deleted !== true) && (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => {
                    if (window.confirm(`¿Desconectar y eliminar el webhook de ${h.metadata.repository}? El token introducido se guardará para este repositorio.`))
                      void action(async () => {
                        const r = await client.disconnectGithubReviewHook(h.id, administrationToken.trim() || undefined)
                        setNotice(r.remote_deleted
                          ? 'Webhook desconectado y eliminado en GitHub. El token de administración se conserva para reconectar.'
                          : `Webhook local revocado, pero no se pudo eliminar en GitHub. ${r.cleanup_error?.error ?? 'Comprueba el token de administración.'} Puedes reintentar la eliminación.`)
                      })
                  }}
                >
                  {h.health === 'revoked' ? 'Reintentar eliminación en GitHub' : 'Desconectar'}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {writable && (
        <form
          className="max-w-3xl space-y-4 border-t border-border pt-5"
          onSubmit={(e) => {
            e.preventDefault()
            void action(async () => {
              const r = await client.registerGithubReviewHook(
                agent,
                base,
                administrationToken.trim() || undefined,
                repository.trim() || undefined,
              )
              setNotice(
                `${r.status === 'configured' ? 'Webhook ya configurado' : 'Webhook conectado'} para ${r.login}. Solicita revisión a esa cuenta en GitHub.`,
              )
            })
          }}
        >
          <label className="block space-y-1.5 text-sm">
            Agente reviewer habilitado
            <select
              className="h-9 w-full min-w-0 rounded-lg border border-input bg-background px-3 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
              required
              value={agent}
              onChange={(e) => { setAgent(e.target.value); setRepository('') }}
            >
              <option value="">
                Seleccionar agente · repositorio fijo o proyecto
              </option>
              {reviewers.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
          </label>
          {!agents.isPending && !reviewers.length && (
            <p className="text-sm text-muted-foreground">
              No hay reviewers habilitados con configuración válida.{' '}
              <Link to="/factory/agents" className="underline underline-offset-4">
                Configura un agente github_pr_reviewer
              </Link>{' '}
              antes de conectar el repositorio.
            </p>
          )}
          <label className="block space-y-1.5 text-sm">
            Repositorio del proyecto (opcional)
            <Input value={repository} onChange={(e) => setRepository(e.target.value)} placeholder="owner/repo" />
          </label>
          <p className="text-xs text-muted-foreground">Vacío conecta todos los repositorios vinculados al proyecto con el mismo reviewer. Indica owner/repo solo para conectar uno; el backend comprueba el alcance.</p>
          <label className="block space-y-1.5 text-sm">
            URL pública HTTPS del backend
            <Input
              type="url"
              value={base}
              onChange={(e) => setBase(e.target.value)}
              placeholder="https://api.nexusmind.smartcoderlabs.com"
              required
            />
          </label>
          <p className="max-w-2xl text-xs text-muted-foreground">
            Conectar de nuevo actualiza el hook existente y sus eventos Issues y Pull requests.
            Ser superusuario de NexusMind no concede permisos en GitHub.
          </p>
          <details className="text-sm text-muted-foreground">
            <summary className="cursor-pointer">Si un administrador ya instaló el webhook</summary>
            <p className="mt-2 max-w-2xl">
              Puedes reutilizar el callback exacto registrado, sin dar acceso de administrador al
              bot. El secreto de firma se genera y cifra en el servidor. Los forks y los PRs del
              propio autor quedan fuera de esta integración.
            </p>
          </details>
          <Button type="submit" disabled={busy || !hooks.data?.login || !agent}>
            Conectar webhook de GitHub
          </Button>
        </form>
      )}
    </section>
  )
}
