import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import { InlineError, errorMessage, useFactory } from '../shared/ui'

export type RepoSecretKind = 'env' | 'npmrc'
/** Repository value for a secret that applies to every repository without its own. */
export const ALL_REPOSITORIES = '*'
export interface RepoSecretSummary {
  repository: string
  kind: RepoSecretKind
  summary: { keys?: string[]; registries?: string[]; scopes?: string[]; authenticated?: string[] }
  updated_at: string
}
export interface RepoSecretList {
  secrets: RepoSecretSummary[]
  encryption_configured: boolean
}

const field =
  'w-full min-w-0 rounded-lg border border-input bg-background px-3 py-2 font-mono text-xs focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring'
const select =
  'h-9 w-full min-w-0 rounded-lg border border-input bg-background px-3 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring'

/** Builds an .npmrc from a scope, registry and token. Exported for tests. */
export function npmrcFromToken(scope: string, registry: string, token: string) {
  const url = registry.trim().replace(/\/+$/, '') + '/'
  const host = url.replace(/^https:\/\//, '')
  const lines = scope.trim() ? [`${scope.trim().toLowerCase()}:registry=${url}`] : [`registry=${url}`]
  lines.push(`//${host}:_authToken=${token.trim()}`)
  return lines.join('\n') + '\n'
}

export function RepoSecrets({ workspaceId, repositories, disabled }: { workspaceId: string; repositories: string[]; disabled: boolean }) {
  const { client } = useFactory()
  const list = useQuery({ queryKey: ['repo-secrets', workspaceId], queryFn: () => client.listRepoSecrets(workspaceId) })
  const [repository, setRepository] = useState(repositories[0] ?? '')
  const [env, setEnv] = useState('')
  const [mode, setMode] = useState<'token' | 'file'>('token')
  const [scope, setScope] = useState('')
  const [registry, setRegistry] = useState('https://npm.pkg.github.com')
  const [token, setToken] = useState('')
  const [npmrc, setNpmrc] = useState('')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)

  const current = (kind: RepoSecretKind) => list.data?.secrets.find((s) => s.repository === repository && s.kind === kind)
  // The workspace default the runner uses when this repository has no entry of its own.
  const inherited = (kind: RepoSecretKind) =>
    repository === ALL_REPOSITORIES ? undefined : list.data?.secrets.find((s) => s.repository === ALL_REPOSITORIES && s.kind === kind)
  const act = async (work: () => Promise<RepoSecretList>, done: string) => {
    setBusy(true); setError(''); setNotice('')
    try {
      await work()
      await list.refetch()
      setNotice(done)
    } catch (e) {
      setError(errorMessage(e, 'No se pudo guardar el secreto.'))
    } finally {
      setBusy(false)
    }
  }
  const saveEnv = () => act(() => client.saveRepoSecret(workspaceId, repository, 'env', env), '.env guardado y cifrado.').then(() => setEnv(''))
  const saveNpm = () => {
    const content = mode === 'token' ? npmrcFromToken(scope, registry, token) : npmrc
    return act(() => client.saveRepoSecret(workspaceId, repository, 'npmrc', content), 'Autenticación npm guardada y cifrada.').then(() => { setToken(''); setNpmrc('') })
  }
  const readFile = async (file: File | undefined) => { if (file) setNpmrc(await file.text()) }
  const locked = disabled || busy || !repository || list.data?.encryption_configured === false
  const envSaved = current('env')
  const npmSaved = current('npmrc')

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-base font-semibold">Secretos por repositorio</h2>
        <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
          Se guardan cifrados y no se vuelven a mostrar. El <code>.env</code> se escribe en la copia temporal del
          repositorio solo para tests, build y QA; nunca llega a los agentes de IA ni a los commits. El{' '}
          <code>.npmrc</code> lo usa el runner para descargar dependencias privadas verificadas por el lockfile.
        </p>
      </div>
      {list.isError && <InlineError message="No se pudieron cargar los secretos." onRetry={() => { void list.refetch() }} />}
      {list.data?.encryption_configured === false && (
        <InlineError message="El backend no tiene NEXUSMIND_TOKEN_ENCRYPTION_KEY configurada; no se pueden guardar secretos." />
      )}
      {error && <InlineError message={error} onDismiss={() => setError('')} />}
      {notice && <p role="status" className="text-sm text-muted-foreground">{notice}</p>}
      <label className="block space-y-1.5 text-sm">
        Repositorio
        <select className={select} value={repository} onChange={(e) => setRepository(e.target.value)} aria-label="Repositorio">
          <option value={ALL_REPOSITORIES}>Todos los repositorios</option>
          {repositories.map((r) => <option key={r} value={r}>{r}</option>)}
        </select>
        {repository === ALL_REPOSITORIES && (
          <span className="block text-xs text-muted-foreground">
            Se aplica a todos los repositorios del workspace; un repositorio con su propio valor lo reemplaza.
          </span>
        )}
      </label>

      <section className="space-y-2" aria-label="Archivo .env">
        <h3 className="text-sm font-semibold">.env</h3>
        <p className="text-xs text-muted-foreground">
          {envSaved
            ? `Guardado · ${envSaved.summary.keys?.join(', ') ?? ''} · ${envSaved.updated_at}`
            : inherited('env') ? 'Usa el .env de todos los repositorios.' : 'Sin .env para este repositorio.'}
        </p>
        <textarea className={field} rows={6} spellCheck={false} autoComplete="off" aria-label="Contenido .env"
          placeholder={'DATABASE_URL=postgres://...\nAPI_KEY=...'} value={env} disabled={locked} onChange={(e) => setEnv(e.target.value)} />
        <div className="flex gap-2">
          <Button size="sm" disabled={locked || !env.trim()} onClick={() => { void saveEnv() }}>{envSaved ? 'Reemplazar .env' : 'Guardar .env'}</Button>
          {envSaved && <Button size="sm" variant="ghost" disabled={locked} onClick={() => { void act(() => client.deleteRepoSecret(workspaceId, repository, 'env'), '.env eliminado.') }}>Eliminar</Button>}
        </div>
      </section>

      <section className="space-y-2" aria-label="Autenticación npm">
        <h3 className="text-sm font-semibold">Autenticación npm</h3>
        <p className="text-xs text-muted-foreground">
          {npmSaved
            ? `Guardado · registros: ${npmSaved.summary.registries?.join(', ') || '—'} · scopes: ${npmSaved.summary.scopes?.join(', ') || '—'} · ${npmSaved.updated_at}`
            : inherited('npmrc')
              ? 'Usa la autenticación npm de todos los repositorios.'
              : 'Sin autenticación npm: solo se aceptan paquetes de registry.npmjs.org.'}
        </p>
        <div className="flex gap-4 text-sm" role="radiogroup" aria-label="Modo de autenticación npm">
          <label className="flex items-center gap-2"><input type="radio" checked={mode === 'token'} onChange={() => setMode('token')} />Token</label>
          <label className="flex items-center gap-2"><input type="radio" checked={mode === 'file'} onChange={() => setMode('file')} />Archivo .npmrc</label>
        </div>
        {mode === 'token' ? (
          <div className="grid gap-3 sm:grid-cols-3">
            <label className="space-y-1.5 text-sm">Scope (opcional)<Input value={scope} placeholder="@kasymir" disabled={locked} onChange={(e) => setScope(e.target.value)} /></label>
            <label className="space-y-1.5 text-sm">Registro<Input value={registry} disabled={locked} onChange={(e) => setRegistry(e.target.value)} /></label>
            <label className="space-y-1.5 text-sm">Token<Input type="password" autoComplete="off" value={token} placeholder="ghp_… (read:packages)" disabled={locked} onChange={(e) => setToken(e.target.value)} /></label>
          </div>
        ) : (
          <>
            <input type="file" aria-label="Subir .npmrc" disabled={locked} onChange={(e) => { void readFile(e.target.files?.[0]) }} className="text-xs" />
            <textarea className={field} rows={5} spellCheck={false} autoComplete="off" aria-label="Contenido .npmrc"
              placeholder={'@kasymir:registry=https://npm.pkg.github.com\n//npm.pkg.github.com/:_authToken=ghp_...'} value={npmrc} disabled={locked} onChange={(e) => setNpmrc(e.target.value)} />
          </>
        )}
        <div className="flex gap-2">
          <Button size="sm" disabled={locked || (mode === 'token' ? !token.trim() || !registry.trim() : !npmrc.trim())} onClick={() => { void saveNpm() }}>
            {npmSaved ? 'Reemplazar autenticación' : 'Guardar autenticación'}
          </Button>
          {npmSaved && <Button size="sm" variant="ghost" disabled={locked} onClick={() => { void act(() => client.deleteRepoSecret(workspaceId, repository, 'npmrc'), 'Autenticación npm eliminada.') }}>Eliminar</Button>}
        </div>
      </section>
    </div>
  )
}
