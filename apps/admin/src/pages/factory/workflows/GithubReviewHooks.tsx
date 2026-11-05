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
              </div>
              {writable && h.health !== 'revoked' && (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => {
                    if (window.confirm('¿Desconectar el webhook firmado de este repositorio?'))
                      void action(async () => {
                        const r = await client.disconnectGithubReviewHook(h.id)
                        if (!r.remote_deleted)
                          setNotice(
                            'Webhook local revocado. Elimina el hook anterior en los ajustes de GitHub si no se pudo borrar automáticamente.',
                          )
                      })
                  }}
                >
                  Desconectar
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
          <p className="text-xs text-muted-foreground">Si el proyecto tiene varios repositorios vinculados, elige cuál conectar. Cada webhook es por repositorio; el backend comprueba que pertenezca al proyecto.</p>
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
          <label className="block space-y-1.5 text-sm">
            Token de administración del webhook (opcional)
            <Input
              type="password"
              autoComplete="off"
              value={administrationToken}
              onChange={(e) => setAdministrationToken(e.target.value)}
              placeholder="Solo si la cuenta revisora no puede administrar hooks"
            />
          </label>
          <p className="max-w-2xl text-xs text-muted-foreground">
            Ser superusuario de NexusMind no concede permisos en GitHub. Este token es solo para el
            alta: no se guarda ni llega a los agentes. La cuenta revisora no cambia.
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
