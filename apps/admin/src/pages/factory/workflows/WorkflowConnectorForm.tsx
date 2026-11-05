import { useState, type FormEvent } from 'react'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import { InlineError, errorMessage, useFactory } from '../shared/ui'
export function WorkflowConnectorForm({
  kind,
  repositories,
  baseUrl,
  onCreated,
  onCancel,
}: {
  kind: string
  repositories: string[]
  baseUrl?: string
  onCreated: (id: string) => Promise<void>
  onCancel: () => void
}) {
  const { client } = useFactory()
  const [name, setName] = useState('')
  const [secret, setSecret] = useState('')
  const [selected, setSelected] = useState<string[]>(
    kind === 'github' ? repositories.slice(0, 1) : [],
  )
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const publication = kind === 'publication'
  const needsRepo = publication || kind === 'github'
  async function save(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    setError('')
    try {
      const metadata: Record<string, unknown> = {
        purpose: publication ? 'factory_workflow_github' : 'factory_workflow_intake',
      }
      if (!publication) metadata.source_kind = kind
      if (needsRepo) metadata.repositories = selected
      if (kind === 'sentry') {
        const url = new URL(baseUrl ?? '')
        if (url.protocol !== 'https:')
          throw new Error('Configura primero una URL HTTPS válida de Sentry.')
        metadata.base_host = url.host.toLowerCase()
      }
      const result = await client.putAutonomousAgentConnector({
        name: `workflow: ${name.trim()} · ${crypto.randomUUID()}`,
        kind: 'target_secret',
        metadata,
        scopes: publication ? ['target:use', 'github:write'] : ['target:use', `${kind}:read`],
        secret: secret.trim(),
      })
      setSecret('')
      await onCreated(result.id)
    } catch (e) {
      setError(errorMessage(e, 'No se pudo crear el conector.'))
    } finally {
      setSecret('')
      setBusy(false)
    }
  }
  return (
    <form
      className="space-y-4 rounded-xl border border-border bg-muted/30 p-4"
      aria-label="Nuevo conector de workflow"
      onSubmit={(e) => {
        void save(e)
      }}
    >
      <div>
        <h3 className="text-sm font-semibold">
          {publication ? 'Conector de publicación GitHub' : `Conector de entrada · ${kind}`}
        </h3>
        <p className="mt-1 max-w-3xl text-xs text-muted-foreground">
          El secreto se cifra en el control plane. No llega a los agentes ni al runner y se limpia
          de este formulario tras cada intento. El nombre es único para no sobrescribir credenciales
          existentes.
        </p>
      </div>
      <fieldset disabled={busy} className="space-y-4">
        <label className="block space-y-1 text-sm">
          Nombre del conector
          <Input
            value={name}
            required
            maxLength={80}
            onChange={(e) => setName(e.target.value)}
            placeholder="Kasymir · issues de frontend"
          />
        </label>
        <label className="block space-y-1 text-sm">
          Token dedicado
          <Input
            type="password"
            autoComplete="off"
            required
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            placeholder={
              publication
                ? 'Permisos de contenidos y pull requests solo en los repositorios elegidos'
                : 'Solo lectura de la fuente y repositorios elegidos'
            }
          />
        </label>
        {needsRepo && (
          <div className="space-y-2">
            <p className="text-sm font-medium">Repositorios permitidos en este conector</p>
            {!repositories.length && (
              <p className="text-xs text-muted-foreground">
                Selecciona antes un repositorio autorizado del workspace.
              </p>
            )}
            <ul className="max-h-48 space-y-2 overflow-auto">
              {repositories.map((repo) => (
                <li key={repo}>
                  <label className="flex items-start gap-2 text-sm">
                    <input
                      className="mt-1"
                      type="checkbox"
                      checked={selected.includes(repo)}
                      onChange={(e) =>
                        setSelected((s) =>
                          e.target.checked ? [...s, repo] : s.filter((r) => r !== repo),
                        )
                      }
                    />
                    <span className="break-all">{repo}</span>
                  </label>
                </li>
              ))}
            </ul>
          </div>
        )}
        <div className="flex gap-2">
          <Button
            type="submit"
            disabled={!secret.trim() || !name.trim() || (needsRepo && !selected.length)}
          >
            Crear conector cifrado
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={() => {
              setSecret('')
              onCancel()
            }}
          >
            Cancelar
          </Button>
        </div>
      </fieldset>
      {error && <InlineError message={error} />}
    </form>
  )
}
