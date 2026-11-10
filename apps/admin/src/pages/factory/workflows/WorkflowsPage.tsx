import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ArrowRight, CheckCircle2, Circle, GitPullRequest, Play, Server, X } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { Badge } from '../../../components/ui/Badge'
import { Input } from '../../../components/ui/Input'
import { InlineError, PageHeader, errorMessage, useFactory } from '../shared/ui'
import { GithubReviewHooks } from './GithubReviewHooks'
import { RuntimeEditor, type RuntimeDraft } from './RuntimeEditor'
import { WorkflowConnectorForm } from './WorkflowConnectorForm'
import { RepositoryInventory } from './RepositoryInventory'
import { RepoSecrets } from './RepoSecrets'
import { ExecutionDetail } from './ExecutionDetail'
import { WorkflowEditor } from './WorkflowEditor'
import { blockerLabel, emptyRuntime } from './runtime'
import { deliveryWorkflow, preparationWorkflow, type WorkflowDefinition, type WorkflowEventQuery } from './types'

const select =
  'h-9 w-full min-w-0 rounded-lg border border-input bg-background px-3 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring'
const panel = 'rounded-xl border border-border bg-card p-5 sm:p-6'
const tabs = [
  ['overview', 'Resumen'],
  ['settings', 'Configuración'],
  ['runs', 'Ejecuciones'],
  ['github', 'GitHub'],
  ['sources', 'Entradas'],
  ['connections', 'Conexiones'],
] as const
const sections = [
  ['repositories', 'Repositorios'],
  ['secrets', 'Secretos por repositorio'],
  ['agents', 'Agentes y modelos'],
  ['checks', 'Tests y QA'],
  ['services', 'Servicios y entornos'],
  ['delivery', 'Publicación y límites'],
] as const
function connected(last: string | null | undefined) {
  if (!last) return false
  const value = last.includes('T') ? last : last.replace(' ', 'T') + 'Z'
  return Date.now() - Date.parse(value) < 120_000
}

export default function WorkflowsPage({
  embedded = false,
  area,
}: {
  embedded?: boolean
  area?: 'sources' | 'connections' | 'github'
}) {
  const { client, can } = useFactory()
  const readable = can('factory_policy:read')
  const writable = can('factory_policy:write')
  const [params, setParams] = useSearchParams()
  const runId = params.get('run') ?? ''
  const tab =
    area ?? (tabs.some(([id]) => id === params.get('tab')) ? params.get('tab')! : 'overview')
  const section = sections.some(([id]) => id === params.get('section'))
    ? params.get('section')!
    : 'repositories'
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const [runnerName, setRunnerName] = useState('')
  const [runnerId, setRunnerId] = useState('')
  const [token, setToken] = useState<string | null>(null)
  const [workspaceName, setWorkspaceName] = useState('')
  const [rootPath, setRootPath] = useState('')
  const [definitionId, setDefinitionId] = useState('')
  const [sourceKey, setSourceKey] = useState('')
  const [taskTitle, setTaskTitle] = useState('')
  const [criteria, setCriteria] = useState('')
  const [editor, setEditor] = useState('')
  const [connectorId, setConnectorId] = useState('server_gh_cli')
  const [budget, setBudget] = useState(20)
  const [repairs, setRepairs] = useState(2)
  const [intakeKind, setIntakeKind] = useState('linear')
  const [intakeConnector, setIntakeConnector] = useState('')
  const [team, setTeam] = useState('')
  const [project, setProject] = useState('')
  const [channel, setChannel] = useState('')
  const [reactors, setReactors] = useState('')
  const [sentryUrl, setSentryUrl] = useState('https://sentry.io')
  const [sentryOrg, setSentryOrg] = useState('')
  const [sentryProject, setSentryProject] = useState('')
  const [githubRepository, setGithubRepository] = useState('')
  const [githubLabels, setGithubLabels] = useState('')
  const [githubIncludeAssigned, setGithubIncludeAssigned] = useState(false)
  const [connectorForm, setConnectorForm] = useState<'publication' | 'intake' | null>(null)
  const [openedArtifact, setOpenedArtifact] = useState<{
    mime: string
    content_base64: string
  } | null>(null)
  const detail = useRef<HTMLDivElement>(null)
  const artifact = useRef<HTMLElement>(null)
  const runtimeDrafts = useRef<Record<string, RuntimeDraft>>({})
  const catalog = useQuery({
    queryKey: ['workflow-catalog'],
    queryFn: () => client.getWorkflowCatalog(),
    enabled: readable,
    refetchInterval: 5000,
  })
  const publicationIdentity = useQuery({
    queryKey: ['workflow-publication-identity'],
    queryFn: () => client.getWorkflowPublicationIdentity(),
    // Also on runs: "Nuevo workflow" there needs to know the publisher is ready.
    enabled: readable && (tab === 'runs' || (tab === 'settings' && section === 'delivery')),
    retry: false,
  })
  const backendPublisherReady =
    publicationIdentity.data?.enabled && !!publicationIdentity.data?.login
  // Stable across the detail's 2.5 s refetches so an open console keeps its cursor.
  const clientRef = useRef(client)
  clientRef.current = client
  const loadEvents = useCallback(
    (q: WorkflowEventQuery) => clientRef.current.getWorkflowRunEvents(runId, q),
    [runId],
  )
  const run = useQuery({
    queryKey: ['workflow-run', runId],
    queryFn: () => client.getFactoryWorkflowRun(runId),
    enabled: readable && !!runId,
    refetchInterval: 2500,
  })
  const agentCatalog = useQuery({
    queryKey: ['autonomous-agents'],
    queryFn: () => client.listAutonomousAgents(),
    enabled: readable && can('autonomous_agent:read'),
  })
  const connectors = useQuery({
    queryKey: ['workflow-connectors'],
    queryFn: () => client.listAutonomousAgentConnectors(),
    enabled: readable && can('autonomous_agent:read'),
  })
  const workspace =
    catalog.data?.workspaces.find(
      (w) => w.id === (params.get('workspace') ?? run.data?.snapshot.workspace.id),
    ) ?? catalog.data?.workspaces[0]
  const runner = catalog.data?.runners.find((r) => r.id === workspace?.runner_id)
  const runtime = workspace?.runtime ?? emptyRuntime()
  const definitions =
    catalog.data?.definitions.filter((d) => d.definition.workspace_id === workspace?.id) ?? []
  const selectedDefinition = definitions.find((d) => d.id === definitionId)
  const runs =
    catalog.data?.runs.filter((r) => definitions.some((d) => d.id === r.definition_id)) ?? []
  const publisherConnectors =
    connectors.data?.filter(
      (c) =>
        c.kind === 'target_secret' &&
        c.health !== 'revoked' &&
        c.metadata.purpose === 'factory_workflow_github',
    ) ?? []
  const githubRepositories = [
    ...new Set(
      (workspace?.repositories ?? []).flatMap((r) => {
        const path = r.remote
          .replace(/^git@github.com:/, 'https://github.com/')
          .match(/^https:\/\/github\.com\/([^?#]+)$/)?.[1]
          ?.replace(/\.git$/, '')
        return path && /^[a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+$/.test(path) ? [path.toLowerCase()] : []
      }),
    ),
  ].sort()
  const intakeConnectors =
    connectors.data?.filter(
      (c) =>
        c.kind === 'target_secret' &&
        c.health !== 'revoked' &&
        ((c.metadata.purpose === 'factory_workflow_intake' &&
          c.metadata.source_kind === intakeKind) ||
          (intakeKind === 'linear' && c.metadata.purpose === 'factory_workflow_linear')),
    ) ?? []
  const preparationOnly =
    selectedDefinition?.definition.steps.every((s) => s.kind === 'prepare') ?? false
  const localSteps =
    selectedDefinition?.definition.steps.filter((s) =>
      ['agent', 'check', 'review'].includes(s.kind),
    ) ?? []
  const locallyManagedReady =
    runtime.revision === 0 &&
    localSteps.length > 0 &&
    localSteps.every(
      (s) =>
        runner?.capabilities.includes(`adapter:${s.adapter}`) &&
        s.capabilities.every((c) => runner?.capabilities.includes(c)),
    )
  const factoryPaused = catalog.data?.factory_enabled === false
  const startAllowed =
    !factoryPaused &&
    !!selectedDefinition &&
    (preparationOnly || runtime.ready || locallyManagedReady)
  function navigate(values: Record<string, string>) {
    setParams((previous) => {
      const next = new URLSearchParams(previous)
      for (const [key, value] of Object.entries(values)) {
        if (value) next.set(key, value)
        else next.delete(key)
      }
      return next
    })
  }
  function runLink(id: string) {
    const next = new URLSearchParams(params)
    next.set('run', id)
    next.set('tab', 'runs')
    return `?${next}`
  }
  async function perform(work: () => Promise<unknown>) {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      await work()
      await catalog.refetch()
      if (runId) await run.refetch()
    } catch (e) {
      setError(errorMessage(e, 'No se pudo completar la operación.'))
    } finally {
      setBusy(false)
    }
  }
  function submit(e: FormEvent, work: () => Promise<unknown>) {
    e.preventDefault()
    void perform(work)
  }
  /** Why a full workflow cannot be created yet, in words the user can act on. */
  function fullWorkflowBlocker(): string {
    if (!workspace) return 'Selecciona o crea un workspace primero.'
    if (!writable) return 'Necesitas el permiso factory_policy:write para crear workflows.'
    if (workspace.confirmed_revision !== workspace.inventory_revision)
      return 'Confirma los repositorios en Configuración → Repositorios.'
    if (!workspace.repositories.some((r) => r.writable))
      return 'Marca al menos un repositorio con escritura en Configuración → Repositorios.'
    if (!connectorId) return 'Elige una cuenta de GitHub en Configuración → Publicación y límites.'
    if (connectorId === 'server_gh_cli' && !backendPublisherReady)
      return 'La cuenta de GitHub del backend no está lista para publicar. Revísala en Configuración → Publicación y límites.'
    if (!Number.isFinite(budget) || budget < 0.01 || budget > 1000)
      return 'El presupuesto debe estar entre 0.01 y 1000 USD.'
    if (!Number.isInteger(repairs) || repairs < 0 || repairs > 2)
      return 'Las reparaciones deben estar entre 0 y 2.'
    return ''
  }
  function startNewWorkflow() {
    const blocker = fullWorkflowBlocker()
    if (blocker) {
      setError(blocker)
      navigate({ tab: 'settings', section: 'delivery' })
      return
    }
    newDefinition(true)
  }
  function newDefinition(full: boolean) {
    if (!workspace) return
    const d = full ? deliveryWorkflow(workspace) : preparationWorkflow(workspace)
    if (full) {
      d.max_cost_usd = budget
      d.max_repairs = repairs
      const publish = d.steps.find((s) => s.kind === 'publish')
      if (publish)
        publish.options =
          connectorId === 'server_gh_cli'
            ? {
                ...publish.options,
                github_auth: 'server_gh_cli',
                github_login: publicationIdentity.data?.login,
              }
            : { ...publish.options, connector_id: connectorId }
    }
    setEditor(JSON.stringify(d, null, 2))
    navigate({ tab: 'settings', section: 'delivery' })
  }
  useEffect(() => {
    setOpenedArtifact(null)
    if (runId) {
      detail.current?.scrollIntoView?.({ behavior: 'smooth', block: 'start' })
      detail.current?.focus({ preventScroll: true })
    }
  }, [runId])
  useEffect(() => {
    if (openedArtifact) artifact.current?.scrollIntoView?.({ behavior: 'smooth', block: 'start' })
  }, [openedArtifact])
  useEffect(() => {
    setDefinitionId('')
    setGithubRepository('')
    setConnectorForm(null)
    setIntakeConnector('')
    setEditor('')
  }, [workspace?.id])
  if (!readable)
    return (
      <div className="p-6">
        <InlineError message="Necesitas el permiso factory_policy:read para ver los workflows." />
      </div>
    )
  const setup = [
    {
      label: 'Runner conectado',
      done: connected(runner?.last_seen) && !runner?.revoked,
      section: 'connections',
      detail: runner?.name ?? 'Registra el servidor que contiene tus clones',
    },
    {
      label: 'Repositorios autorizados',
      done:
        !!workspace?.repositories.length &&
        workspace.confirmed_revision === workspace.inventory_revision,
      section: 'repositories',
      detail: workspace
        ? `${workspace.repositories.length} repositorios · ${workspace.repositories.filter((r) => r.writable).length} con escritura`
        : 'Selecciona una carpeta',
    },
    {
      label: 'Agentes y modelos',
      done:
        runtime.config.agents.length === 4 &&
        !runtime.missing.some((s) => /^(agent:|model_connection:|network:)/.test(s)),
      section: 'agents',
      detail: 'Investigación, tests, implementación y revisor independiente',
    },
    {
      label: 'Tests, build y QA',
      done: !runtime.missing.some((s) => s.startsWith('check:')),
      section: 'checks',
      detail: 'Comprobaciones reales por repositorio',
    },
    {
      label: 'Servicios de pruebas',
      done: runtime.config.services.length > 0,
      section: 'services',
      detail: 'Entorno integrado desechable, sin producción',
    },
    {
      label: 'Configuración aplicada',
      done: runtime.ready,
      section: 'agents',
      detail: runtime.revision
        ? `Guardada: ${runtime.revision} · aplicada: ${runtime.report.applied_revision}`
        : 'El runner debe validar y aplicar los perfiles',
    },
  ]
  return (
    <div
      className={embedded ? 'min-w-0 space-y-5' : 'mx-auto max-w-7xl space-y-6 p-4 sm:p-6 md:p-8'}
    >
      <header className="flex flex-wrap items-start justify-between gap-4">
        {!embedded && (
          <PageHeader
            title="Workflows de desarrollo"
            subtitle="Configura un workspace, ejecuta una tarea y sigue sus evidencias."
          />
        )}
        <label className="block w-full space-y-1 text-xs sm:w-72">
          Workspace
          <select
            className={select}
            aria-label="Workspace activo"
            value={workspace?.id ?? ''}
            onChange={(e) => navigate({ workspace: e.target.value, run: '' })}
          >
            <option value="" disabled>
              Selecciona un workspace
            </option>
            {catalog.data?.workspaces.map((w) => (
              <option key={w.id} value={w.id}>
                {w.name}
              </option>
            ))}
          </select>
        </label>
      </header>
      {!embedded && (
        <nav
          className="flex gap-1 overflow-x-auto border-b border-border pb-1"
          aria-label="Secciones de workflows"
        >
          {tabs
            .filter(([id]) => ['overview', 'settings', 'runs'].includes(id))
            .map(([id, label]) => (
              <button
                type="button"
                key={id}
                aria-current={tab === id ? 'page' : undefined}
                onClick={() => navigate({ tab: id, run: '' })}
                className={`shrink-0 rounded-lg px-3 py-2 text-sm transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring ${tab === id ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground'}`}
              >
                {label}
                {id === 'runs' && !!runs.length && (
                  <span className="ml-2 tabular-nums text-xs">{runs.length}</span>
                )}
              </button>
            ))}
        </nav>
      )}
      {(error || catalog.isError) && (
        <InlineError
          message={error || errorMessage(catalog.error, 'No se pudo cargar el workspace.')}
          onRetry={() => {
            void catalog.refetch()
          }}
        />
      )}
      {factoryPaused && (
        <p role="status" className="rounded-lg border border-border bg-muted p-4 text-sm">
          Factory está pausada. No se iniciará trabajo nuevo en el worker ni en los runners.{' '}
          <Link className="underline underline-offset-4" to="/factory/settings">
            Abrir configuración
          </Link>
        </p>
      )}
      {notice && (
        <p role="status" className="rounded-lg bg-muted p-3 text-sm">
          {notice}
        </p>
      )}
      {catalog.isPending && (
        <div role="status" className="space-y-3 py-5">
          <p className="text-sm text-muted-foreground">Cargando workspaces…</p>
          <div className="h-16 rounded-lg bg-muted" />
          <div className="h-32 rounded-lg bg-muted" />
        </div>
      )}
      {runId && (
        <div
          ref={detail}
          tabIndex={-1}
          className={`${panel} scroll-mt-6 space-y-5`}
          aria-label="Ejecución seleccionada"
        >
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-sm font-medium">Detalle de ejecución</h2>
            <Button size="sm" variant="ghost" onClick={() => navigate({ run: '' })}>
              <X className="mr-1 size-4" />
              Cerrar ejecución
            </Button>
          </div>
          {run.isPending && <p role="status">Cargando ejecución…</p>}
          {run.isError && (
            <InlineError
              message={errorMessage(run.error, 'No se pudo abrir la ejecución.')}
              onRetry={() => {
                void run.refetch()
              }}
            />
          )}
          {run.data && (
            <ExecutionDetail
              run={run.data}
              disabled={busy || !writable}
              control={(action, step) => {
                void perform(() => client.controlFactoryWorkflowRun(run.data.id, action, step))
              }}
              approve={(action, revision) => {
                void perform(() => client.approveWorkflowAction(run.data.id, action, revision))
              }}
              artifact={(id) => {
                void perform(async () => setOpenedArtifact(await client.getWorkflowArtifact(id)))
              }}
              events={loadEvents}
            />
          )}
        </div>
      )}
      {openedArtifact && (
        <section
          ref={artifact}
          className={`${panel} scroll-mt-6 space-y-4`}
          aria-label="Contenido del artefacto"
        >
          <div className="flex items-center justify-between">
            <h2 className="font-medium">Contenido del artefacto</h2>
            <Button variant="ghost" onClick={() => setOpenedArtifact(null)}>
              Cerrar artefacto
            </Button>
          </div>
          {openedArtifact.mime.startsWith('image/') ? (
            <img
              className="max-w-full"
              alt="Evidencia de QA"
              src={`data:${openedArtifact.mime};base64,${openedArtifact.content_base64}`}
            />
          ) : (
            <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words text-xs">
              {new TextDecoder().decode(
                Uint8Array.from(atob(openedArtifact.content_base64), (c) => c.charCodeAt(0)),
              )}
            </pre>
          )}
        </section>
      )}
      {!catalog.isPending && !workspace && tab !== 'github' && tab !== 'connections' && (
        <section className={`${panel} space-y-3`}>
          <h2 className="text-lg font-semibold">Conecta la máquina donde están tus repositorios</h2>
          <p className="max-w-xl text-sm text-muted-foreground">
            Registra un runner y su carpeta. Después podrás seleccionar repositorios, configurar
            agentes y crear tu primer workflow.
          </p>
          <Button onClick={() => navigate({ tab: 'connections' })}>
            <Server className="mr-2 size-4" />
            Registrar runner y carpeta
          </Button>
        </section>
      )}
      {tab === 'overview' && workspace && (
        <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1.4fr)_minmax(280px,1fr)]">
          <section className={`${panel} space-y-5`}>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h2 className="text-lg font-semibold">
                  {runtime.ready
                    ? 'Workspace configurado'
                    : 'Completa la configuración del workspace'}
                </h2>
                <p className="mt-1 max-w-xl text-sm text-muted-foreground">
                  {runtime.ready
                    ? 'Los perfiles están aplicados. El QA se verificará en cada ejecución.'
                    : 'El runner y los permisos no bastan para ejecutar agentes. Aquí puedes ver exactamente qué falta.'}
                </p>
              </div>
              <Badge>{runtime.ready ? 'Configurado' : 'Preparación pendiente'}</Badge>
            </div>
            <Button
              onClick={() =>
                navigate(runtime.ready ? { tab: 'runs' } : { tab: 'settings', section: 'agents' })
              }
            >
              {runtime.ready ? 'Ejecutar una tarea' : 'Configurar agentes y QA'}
              <ArrowRight className="ml-2 size-4" />
            </Button>
            <ul className="divide-y divide-border">
              {setup.map((item) => (
                <li key={item.label} className="flex items-start gap-3 py-4">
                  {item.done ? (
                    <CheckCircle2 className="mt-0.5 size-5 shrink-0 text-status-success" />
                  ) : (
                    <Circle className="mt-0.5 size-5 shrink-0 text-muted-foreground" />
                  )}
                  <div className="min-w-0 flex-1">
                    <h3 className="text-sm font-medium">{item.label}</h3>
                    <p className="mt-1 text-sm text-muted-foreground">{item.detail}</p>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      navigate(
                        item.section === 'connections'
                          ? { tab: 'connections' }
                          : { tab: 'settings', section: item.section },
                      )
                    }
                  >
                    {item.done ? 'Revisar' : 'Configurar'}
                    <ArrowRight className="ml-1 size-4" />
                  </Button>
                </li>
              ))}
            </ul>
          </section>
          <aside className="space-y-5">
            <section className={`${panel} space-y-3`}>
              <div className="flex items-center gap-2">
                <Server className="size-4" />
                <h2 className="text-sm font-semibold">{runner?.name ?? 'Runner'}</h2>
                <Badge>
                  {connected(runner?.last_seen) && !runner?.revoked ? 'Conectado' : 'Sin conexión'}
                </Badge>
              </div>
              <p className="break-all text-sm text-muted-foreground">{workspace.root_path}</p>
              <p className="text-sm text-muted-foreground">
                {workspace.repositories.length} repositorios ·{' '}
                {workspace.repositories.filter((r) => r.writable).length} con escritura
              </p>
              <Button
                size="sm"
                variant="secondary"
                disabled={
                  busy || !writable || workspace.confirmed_revision !== workspace.inventory_revision
                }
                onClick={() => newDefinition(false)}
              >
                Probar solo la preparación
              </Button>
            </section>
            <section className="space-y-3 px-1">
              <h2 className="text-sm font-semibold">Dos formas de revisión</h2>
              <p className="text-sm text-muted-foreground">
                <strong className="font-medium text-foreground">Dentro del workflow:</strong> un
                agente nuevo revisa código y evidencias, incluso si el bot publicó el PR.
              </p>
              <p className="text-sm text-muted-foreground">
                <strong className="font-medium text-foreground">En GitHub:</strong> una aprobación
                formal requiere otra identidad. El autor no puede aprobarse a sí mismo.
              </p>
              <Button variant="ghost" size="sm" onClick={() => navigate({ tab: 'github' })}>
                <GitPullRequest className="mr-2 size-4" />
                Configurar reviews de GitHub
              </Button>
            </section>
          </aside>
        </div>
      )}
      {tab === 'settings' && workspace && (!editor || section !== 'delivery') && (
        <div className="grid items-start gap-6 lg:grid-cols-[210px_minmax(0,1fr)]">
          <nav
            aria-label="Pasos de configuración"
            className="flex gap-1 overflow-x-auto lg:flex-col"
          >
            {sections.map(([id, label]) => (
              <button
                key={id}
                type="button"
                onClick={() => navigate({ section: id })}
                aria-current={section === id ? 'step' : undefined}
                className={`shrink-0 rounded-lg px-3 py-2.5 text-left text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring ${section === id ? 'bg-muted font-medium text-foreground' : 'text-muted-foreground hover:bg-muted/50'}`}
              >
                {label}
              </button>
            ))}
          </nav>
          <div className={`${panel} min-w-0 space-y-6`}>
            {section === 'repositories' && (
              <RepositoryInventory
                key={`${workspace.id}:${workspace.inventory_revision}:${workspace.confirmed_revision}`}
                workspace={workspace}
                disabled={busy || !writable}
                onSave={(repos) => {
                  void perform(() =>
                    client.confirmFactoryWorkspace(
                      workspace.id,
                      workspace.inventory_revision,
                      repos,
                    ),
                  )
                }}
              />
            )}
            {section === 'secrets' && (
              <RepoSecrets
                key={workspace.id}
                workspaceId={workspace.id}
                repositories={workspace.repositories.map((r) => r.path)}
                disabled={busy || !writable}
              />
            )}
            {['agents', 'checks', 'services'].includes(section) && (
              <>
                {agentCatalog.isError && (
                  <InlineError
                    message="No se pudo cargar el catálogo de agentes. Los perfiles ya vinculados se conservan."
                    onRetry={() => {
                      void agentCatalog.refetch()
                    }}
                  />
                )}
                <RuntimeEditor
                  catalog={agentCatalog.data ?? []}
                  loadAgent={
                    can('autonomous_agent:read')
                      ? (id) => client.getWorkflowAgentProfile(id)
                      : undefined
                  }
                  key={workspace.id}
                  draft={runtimeDrafts.current[workspace.id]}
                  onDraft={(d) => {
                    runtimeDrafts.current[workspace.id] = d
                  }}
                  value={workspace.runtime}
                  repositories={workspace.repositories.map((r) => r.path)}
                  section={section as 'agents' | 'checks' | 'services'}
                  disabled={busy || !writable}
                  onSave={async (revision, config) => {
                    const saved = await client.saveWorkflowRuntime(workspace.id, revision, config)
                    await catalog.refetch()
                    return saved
                  }}
                />
              </>
            )}
            {section === 'delivery' && (
              <>
                <div>
                  <h2 className="text-base font-semibold">Publicación y límites</h2>
                  <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
                    Publica ramas y PRs draft con la cuenta de GitHub del backend. Sus credenciales
                    nunca llegan al runner. Se conservan verificaciones, permisos por repositorio,
                    presupuesto y aprobaciones; no se hacen merges ni despliegues.
                  </p>
                </div>
                <div className="grid gap-4 sm:grid-cols-2">
                  <label className="space-y-1.5 text-sm sm:col-span-2">
                    Cuenta para publicar en GitHub
                    <select
                      className={select}
                      value={connectorId}
                      disabled={!writable}
                      onChange={(e) => setConnectorId(e.target.value)}
                    >
                      <option value="server_gh_cli">
                        Cuenta autenticada del backend
                        {publicationIdentity.data?.login
                          ? ` · ${publicationIdentity.data.login}`
                          : ''}{' '}
                        · predeterminada
                      </option>
                      {publisherConnectors.map((c) => (
                        <option key={c.id} value={c.id}>
                          {c.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="space-y-1.5 text-sm">
                    Presupuesto máximo (USD)
                    <Input
                      type="number"
                      min="0.01"
                      max="1000"
                      step="0.01"
                      value={budget}
                      disabled={!writable}
                      onChange={(e) => setBudget(Number(e.target.value))}
                    />
                  </label>
                  <label className="space-y-1.5 text-sm">
                    Ciclos de reparación
                    <Input
                      type="number"
                      min="0"
                      max="2"
                      value={repairs}
                      disabled={!writable}
                      onChange={(e) => setRepairs(Number(e.target.value))}
                    />
                  </label>
                </div>
                <details className="space-y-3 text-sm">
                  <summary className="cursor-pointer text-muted-foreground">
                    Otra identidad de GitHub · avanzado
                  </summary>
                  <p className="mt-2 text-muted-foreground">
                    Solo necesitas un conector adicional para publicar con otra cuenta o una GitHub
                    App.
                  </p>
                  {can('autonomous_agent:manage_connectors') &&
                    can('autonomous_agent:read') &&
                    writable && (
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() => setConnectorForm('publication')}
                      >
                        Nuevo conector de publicación
                      </Button>
                    )}
                  {connectorForm === 'publication' && (
                    <WorkflowConnectorForm
                      kind="publication"
                      repositories={githubRepositories}
                      onCancel={() => setConnectorForm(null)}
                      onCreated={async (id) => {
                        await connectors.refetch()
                        setConnectorId(id)
                        setConnectorForm(null)
                        setNotice(
                          'Conector cifrado creado. Sus permisos efectivos también se comprobarán al publicar.',
                        )
                      }}
                    />
                  )}
                </details>
                {connectorId === 'server_gh_cli' && !backendPublisherReady && (
                  <p role="status" className="text-sm text-status-warning">
                    {publicationIdentity.isPending
                      ? 'Comprobando la cuenta del backend…'
                      : publicationIdentity.data?.enabled
                        ? 'La cuenta del backend no está autenticada. El operador debe iniciar sesión en GitHub.'
                        : 'La cuenta del backend aún no está autorizada para publicar en esta organización. El operador debe habilitarla; no necesitas crear un token en el formulario.'}
                  </p>
                )}
                <p className="text-sm text-muted-foreground">
                  Los PRs son draft. La revisión interna tiene contexto limpio y no equivale a una
                  aprobación de GitHub. No se fusiona ni despliega automáticamente.
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    disabled={busy || Boolean(fullWorkflowBlocker())}
                    onClick={() => newDefinition(true)}
                  >
                    Crear workflow completo
                  </Button>
                  <Button
                    variant="secondary"
                    disabled={
                      busy ||
                      !writable ||
                      workspace.confirmed_revision !== workspace.inventory_revision
                    }
                    onClick={() => newDefinition(false)}
                  >
                    Crear prueba de preparación
                  </Button>
                </div>
                {fullWorkflowBlocker() && (
                  <p role="status" className="text-sm text-muted-foreground">
                    Para crear un workflow completo: {fullWorkflowBlocker()}
                  </p>
                )}
              </>
            )}
            {runtime.missing.length > 0 && section !== 'repositories' && (
              <details className="border-t border-border pt-4 text-sm">
                <summary className="cursor-pointer text-muted-foreground">
                  Bloqueos detectados por el runner ({runtime.missing.length})
                </summary>
                <ul className="mt-3 list-disc space-y-1 pl-5 text-muted-foreground">
                  {[...new Set(runtime.missing.map(blockerLabel))].map((m) => (
                    <li key={m}>{m}</li>
                  ))}
                </ul>
              </details>
            )}
          </div>
        </div>
      )}
      {editor && tab === 'settings' && section === 'delivery' && (
        <form
          className={`${panel} space-y-4`}
          onSubmit={(e) =>
            submit(e, async () => {
              const d = await client.createFactoryWorkflow(JSON.parse(editor) as WorkflowDefinition)
              setDefinitionId(d.id)
              setEditor('')
              navigate({ tab: 'runs' })
              setNotice('Definición guardada. Ya puedes seleccionarla para una tarea.')
            })
          }
        >
          <h2 className="text-base font-semibold">Revisar definición antes de guardarla</h2>
          <fieldset disabled={busy || !writable}>
            <WorkflowEditor
              value={editor}
              onChange={setEditor}
              adapters={
                runner?.capabilities
                  .filter((c) => c.startsWith('adapter:'))
                  .map((c) => c.slice(8)) ?? []
              }
              repositories={
                workspace?.repositories.filter((r) => r.writable).map((r) => r.path) ?? []
              }
            />
          </fieldset>
          <div className="flex gap-2">
            <Button type="submit" disabled={busy || !writable}>
              Guardar definición
            </Button>
            <Button type="button" variant="ghost" disabled={busy} onClick={() => setEditor('')}>
              Descartar
            </Button>
          </div>
        </form>
      )}
      {tab === 'runs' && workspace && (
        <div className="space-y-6">
          <section className={`${panel} space-y-4`}>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h2 className="text-base font-semibold">Nueva tarea</h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  Elige una definición guardada. La misma clave de origen no crea ejecuciones
                  duplicadas.
                </p>
              </div>
              <Button size="sm" variant="secondary" disabled={busy || !writable} onClick={startNewWorkflow}>
                Nuevo workflow
              </Button>
            </div>
            <form
              className="space-y-4"
              onSubmit={(e) =>
                submit(e, async () => {
                  const r = await client.startFactoryWorkflow(definitionId, sourceKey.trim(), {
                    title: taskTitle,
                    acceptance_criteria: criteria,
                    origin_trust: 'untrusted',
                  })
                  navigate({ run: r.id, tab: 'runs' })
                })
              }
            >
              <div className="grid gap-4 sm:grid-cols-2">
                <label className="space-y-1.5 text-sm">
                  Workflow
                  <select
                    className={select}
                    value={definitionId}
                    required
                    disabled={!writable}
                    onChange={(e) => {
                      if (e.target.value === '__new__') startNewWorkflow()
                      else setDefinitionId(e.target.value)
                    }}
                  >
                    <option value="">
                      {definitions.length ? 'Seleccionar definición' : 'No hay workflows guardados'}
                    </option>
                    {definitions.map((d) => (
                      <option key={d.id} value={d.id}>
                        {d.definition.name}
                      </option>
                    ))}
                    <option value="__new__">+ Crear nuevo workflow…</option>
                  </select>
                </label>
                <label className="space-y-1.5 text-sm">
                  Clave de origen
                  <Input
                    value={sourceKey}
                    disabled={!writable}
                    required
                    maxLength={512}
                    placeholder="manual:kasymir-prueba-003"
                    onChange={(e) => setSourceKey(e.target.value)}
                  />
                </label>
                <label className="space-y-1.5 text-sm sm:col-span-2">
                  Tarea
                  <Input
                    value={taskTitle}
                    disabled={!writable}
                    required
                    placeholder="Describe el problema y el resultado esperado"
                    onChange={(e) => setTaskTitle(e.target.value)}
                  />
                </label>
                <label className="space-y-1.5 text-sm sm:col-span-2">
                  Criterios de aceptación
                  <textarea
                    className={`${select} h-24 py-2`}
                    value={criteria}
                    disabled={!writable}
                    placeholder="Cómo reproducirlo y qué debe cumplirse para considerarlo resuelto"
                    onChange={(e) => setCriteria(e.target.value)}
                  />
                </label>
              </div>
              {!preparationOnly && !runtime.ready && !locallyManagedReady && (
                <p role="note" className="text-sm text-muted-foreground">
                  Completa agentes, QA y servicios antes de ejecutar el flujo completo. La prueba de
                  preparación sí puede ejecutarse sin modelos.
                </p>
              )}
              <Button type="submit" disabled={busy || !writable || !startAllowed}>
                <Play className="mr-2 size-4" />
                Iniciar ejecución
              </Button>
            </form>
          </section>
          <section className={`${panel} space-y-4`}>
            <h2 className="text-base font-semibold">Ejecuciones del workspace</h2>
            {!runs.length ? (
              <p className="text-sm text-muted-foreground">
                Todavía no hay ejecuciones. Empieza con una prueba de preparación para comprobar el
                runner sin consumir modelos.
              </p>
            ) : (
              <ul className="divide-y divide-border">
                {runs.map((r) => (
                  <li key={r.id}>
                    <Link
                      to={runLink(r.id)}
                      className="flex flex-wrap items-center justify-between gap-3 rounded-lg px-2 py-4 hover:bg-muted/50 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
                    >
                      <span className="min-w-0 break-all text-sm font-medium">{r.source_key}</span>
                      <span className="flex items-center gap-3">
                        <Badge>{r.status}</Badge>
                        <span className="flex items-center gap-1 text-sm underline underline-offset-4">
                          Abrir ejecución
                          <ArrowRight className="size-4" />
                        </span>
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      )}
      {tab === 'github' && (
        <div className="space-y-5">
          <div className="max-w-3xl">
            <h2 className="text-lg font-semibold">Reviews de GitHub</h2>
            <p className="mt-2 text-sm text-muted-foreground">
              Esta integración funciona en el backend y no depende del runner de Oracle. Solicita
              revisión a la cuenta configurada en un PR no-draft creado por otra identidad. Para PRs
              que publica el mismo bot, usa la revisión interna del workflow; GitHub no admite
              autoaprobación.
            </p>
            <Link
              className="mt-3 inline-block text-sm underline underline-offset-4"
              to="/factory/runs"
            >
              Ver ejecuciones del reviewer de GitHub
            </Link>
          </div>
          <GithubReviewHooks />
        </div>
      )}
      {tab === 'sources' && workspace && (
        <section className={`${panel} space-y-5`}>
          <div>
            <h2 className="text-base font-semibold">Entradas automáticas</h2>
            <p className="mt-1 max-w-2xl text-sm text-muted-foreground">
              GitHub Issues, Linear, Slack y Sentry envían tareas al mismo workflow. Se consultan
              cada cinco minutos mientras el runner está activo y respetan políticas y aprobaciones.
            </p>
          </div>
          <ul className="divide-y divide-border">
            {catalog.data?.intakes
              ?.filter((i) => definitions.some((d) => d.id === i.definition_id))
              .map((i) => (
                <li
                  key={i.id}
                  className="flex flex-wrap items-center justify-between gap-3 py-3 text-sm"
                >
                  <span>
                    {i.kind} · {i.enabled ? 'Activa' : 'Desactivada'} ·{' '}
                    {i.last_error ?? 'Sin errores de consulta'}
                  </span>
                  <div className="flex gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={busy || !writable}
                      onClick={() => void perform(() => client.updateWorkflowIntake(i.id, !i.enabled))}
                    >
                      {i.enabled ? 'Pausar' : 'Activar'}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy || !writable}
                      onClick={() => {
                        if (window.confirm('¿Quitar esta entrada?'))
                          void perform(() => client.deleteWorkflowIntake(i.id))
                      }}
                    >
                      Quitar entrada
                    </Button>
                  </div>
                </li>
              ))}
          </ul>
          <div className="space-y-3">
            {can('autonomous_agent:manage_connectors') &&
              can('autonomous_agent:read') &&
              writable && (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={intakeKind === 'github' && !githubRepository}
                  onClick={() => setConnectorForm('intake')}
                >
                  Nuevo conector de entrada
                </Button>
              )}
            {connectorForm === 'intake' && (
              <WorkflowConnectorForm
                key={intakeKind + githubRepository}
                kind={intakeKind}
                repositories={intakeKind === 'github' ? [githubRepository] : []}
                baseUrl={sentryUrl}
                onCancel={() => setConnectorForm(null)}
                onCreated={async (id) => {
                  await connectors.refetch()
                  setIntakeConnector(id)
                  setConnectorForm(null)
                  setNotice('Conector cifrado creado. Ahora puedes conectar la entrada.')
                }}
              />
            )}
          </div>
          <form
            className="space-y-4"
            onSubmit={(e) =>
              submit(e, async () => {
                const config =
                  intakeKind === 'github'
                    ? {
                        repository: githubRepository,
                        ...(githubIncludeAssigned ? { include_assigned: true } : {}),
                        labels: githubLabels
                          .split(',')
                          .map((s) => s.trim())
                          .filter(Boolean),
                      }
                    : intakeKind === 'linear'
                      ? { team_id: team || undefined, project_id: project || undefined }
                      : intakeKind === 'slack'
                        ? {
                            channel_id: channel,
                            allowed_reactors: reactors
                              .split(',')
                              .map((s) => s.trim())
                              .filter(Boolean),
                          }
                        : {
                            base_url: sentryUrl,
                            org_slug: sentryOrg,
                            project_slug: sentryProject,
                            query: 'is:unresolved',
                          }
                await client.createWorkflowIntake({
                  definition_id: definitionId,
                  kind: intakeKind,
                  connector_id: intakeConnector,
                  config,
                  enabled: false,
                })
                setNotice('Entrada creada pausada. Actívala solo después de validar el workflow y sus permisos.')
              })
            }
          >
            <div className="grid gap-4 sm:grid-cols-2">
              <label className="space-y-1.5 text-sm">
                Workflow destino
                <select
                  className={select}
                  value={definitionId}
                  required
                  disabled={!writable}
                  onChange={(e) => setDefinitionId(e.target.value)}
                >
                  <option value="">Seleccionar definición</option>
                  {definitions.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.definition.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="space-y-1.5 text-sm">
                Aplicación
                <select
                  className={select}
                  value={intakeKind}
                  disabled={!writable}
                  onChange={(e) => {
                    setIntakeKind(e.target.value)
                    setIntakeConnector('')
                    setConnectorForm(null)
                  }}
                >
                  <option value="github">GitHub Issues</option>
                  <option value="linear">Linear</option>
                  <option value="slack">Slack</option>
                  <option value="sentry">Sentry</option>
                </select>
              </label>
              <label className="space-y-1.5 text-sm sm:col-span-2">
                Conector dedicado
                <select
                  className={select}
                  required
                  disabled={!writable}
                  value={intakeConnector}
                  onChange={(e) => setIntakeConnector(e.target.value)}
                >
                  <option value="">Seleccionar conector cifrado</option>
                  {intakeConnectors.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </select>
              </label>
              {intakeKind === 'github' && (
                <>
                  <label className="space-y-1.5 text-sm">
                    Repositorio de GitHub
                    <select
                      className={select}
                      required
                      value={githubRepository}
                      disabled={!writable || busy}
                      onChange={(e) => {
                        setGithubRepository(e.target.value)
                        setIntakeConnector('')
                      }}
                    >
                      <option value="">Seleccionar repositorio autorizado</option>
                      {githubRepositories.map((r) => (
                        <option key={r} value={r}>
                          {r}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="space-y-1.5 text-sm">
                    Labels adicionales (separadas por coma)
                    <Input
                      value={githubLabels}
                      disabled={!writable || busy}
                      placeholder="bug, frontend"
                      onChange={(e) => setGithubLabels(e.target.value)}
                    />
                  </label>
                  <p className="text-sm text-muted-foreground sm:col-span-2">
                    Solo issues abiertos con label factory y todas las labels adicionales. Por
                    defecto, sin asignar para no tomar trabajo de otra persona o duplicar un
                    resolver. Los PRs se excluyen y cada issue entra una sola vez en este workspace.
                    El conector requiere github:read y el repositorio en su allowlist.
                  </p>
                </>
              )}
              {intakeKind === 'github' && (
                <label className="flex items-start gap-2 text-xs text-muted-foreground sm:col-span-2">
                  <input
                    type="checkbox"
                    checked={githubIncludeAssigned}
                    disabled={!writable || busy}
                    onChange={(e) => setGithubIncludeAssigned(e.target.checked)}
                  />
                  <span>
                    Incluir issues asignados · comprueba antes que ningún agente individual procese
                    el mismo origen. La deduplicación del workflow es por workspace.
                  </span>
                </label>
              )}
              {intakeKind === 'linear' && (
                <>
                  <label className="space-y-1.5 text-sm">
                    Team ID (opcional)
                    <Input
                      value={team}
                      disabled={!writable}
                      onChange={(e) => setTeam(e.target.value)}
                    />
                  </label>
                  <label className="space-y-1.5 text-sm">
                    Project ID (opcional)
                    <Input
                      value={project}
                      disabled={!writable}
                      onChange={(e) => setProject(e.target.value)}
                    />
                  </label>
                </>
              )}
              {intakeKind === 'slack' && (
                <>
                  <label className="space-y-1.5 text-sm">
                    Canal de Slack
                    <Input
                      value={channel}
                      required
                      disabled={!writable}
                      onChange={(e) => setChannel(e.target.value)}
                    />
                  </label>
                  <label className="space-y-1.5 text-sm">
                    Usuarios autorizados a reaccionar (IDs separados por coma)
                    <Input
                      value={reactors}
                      required
                      disabled={!writable}
                      onChange={(e) => setReactors(e.target.value)}
                    />
                  </label>
                </>
              )}
              {intakeKind === 'sentry' && (
                <>
                  <label className="space-y-1.5 text-sm">
                    URL de Sentry
                    <Input
                      type="url"
                      value={sentryUrl}
                      required
                      disabled={!writable}
                      onChange={(e) => setSentryUrl(e.target.value)}
                    />
                  </label>
                  <label className="space-y-1.5 text-sm">
                    Organización
                    <Input
                      value={sentryOrg}
                      required
                      disabled={!writable}
                      onChange={(e) => setSentryOrg(e.target.value)}
                    />
                  </label>
                  <label className="space-y-1.5 text-sm">
                    Proyecto
                    <Input
                      value={sentryProject}
                      required
                      disabled={!writable}
                      onChange={(e) => setSentryProject(e.target.value)}
                    />
                  </label>
                </>
              )}
            </div>
            {!intakeConnectors.length && (
              <p className="text-sm text-muted-foreground">
                No hay conectores dedicados disponibles para esta fuente. Sus claves se configuran
                cifradas; no las pegues en la descripción de la tarea.
              </p>
            )}
            <div className="flex flex-wrap gap-2">
              <Button
                type="submit"
                disabled={
                  busy ||
                  !writable ||
                  !definitionId ||
                  !intakeConnector ||
                  (intakeKind === 'github' && !githubRepositories.includes(githubRepository))
                }
              >
                Conectar entrada
              </Button>
              <Button
                type="button"
                variant="secondary"
                disabled={busy || !writable}
                onClick={() => {
                  void perform(() => client.pollWorkflowIntakes())
                }}
              >
                Consultar entradas pendientes
              </Button>
              {intakeKind === 'linear' && (
                <Button
                  type="button"
                  variant="ghost"
                  disabled={busy || !writable || !definitionId || !intakeConnector}
                  onClick={() => {
                    void perform(async () => {
                      const r = await client.startNextLinearWorkflow(
                        definitionId,
                        intakeConnector,
                        team,
                        project,
                      )
                      if (r.run) navigate({ run: r.run.id, tab: 'runs' })
                      else setNotice('No hay issues asignados elegibles sin tomar.')
                    })
                  }}
                >
                  Tomar siguiente issue asignado
                </Button>
              )}
            </div>
          </form>
        </section>
      )}
      {tab === 'connections' && (
        <div className="grid items-start gap-6 lg:grid-cols-2">
          <section className={`${panel} space-y-5`}>
            <h2 className="text-base font-semibold">Runners</h2>
            <p className="text-sm text-muted-foreground">
              Procesos instalados en las máquinas que contienen tus clones. Su credencial no permite
              administrar workflows ni políticas.
            </p>
            <ul className="divide-y divide-border">
              {catalog.data?.runners.map((r) => (
                <li key={r.id} className="space-y-2 py-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <h3 className="text-sm font-medium">{r.name}</h3>
                    <Badge>
                      {r.revoked
                        ? 'Revocado'
                        : connected(r.last_seen)
                          ? 'Conectado'
                          : 'Sin conexión'}
                    </Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Última conexión: {r.last_seen ?? 'Nunca'}
                  </p>
                  {writable && (
                    <details className="text-sm">
                      <summary className="cursor-pointer text-muted-foreground">
                        Administrar credencial
                      </summary>
                      <div className="mt-3 flex gap-2">
                        <Button
                          variant="secondary"
                          size="sm"
                          disabled={busy}
                          onClick={() => {
                            if (
                              window.confirm(
                                '¿Rotar la credencial y detener la autoridad de los trabajos activos?',
                              )
                            )
                              void perform(async () => {
                                setToken((await client.rotateFactoryRunner(r.id)).token)
                                setRunnerId(r.id)
                              })
                          }}
                        >
                          Rotar credencial
                        </Button>
                        {!r.revoked && (
                          <Button
                            variant="ghost"
                            size="sm"
                            disabled={busy}
                            onClick={() => {
                              if (window.confirm(`¿Revocar ${r.name}?`))
                                void perform(() => client.revokeFactoryRunner(r.id))
                            }}
                          >
                            Revocar
                          </Button>
                        )}
                      </div>
                    </details>
                  )}
                </li>
              ))}
            </ul>
            {writable && (
              <form
                className="space-y-3 border-t border-border pt-4"
                onSubmit={(e) =>
                  submit(e, async () => {
                    const r = await client.registerFactoryRunner(runnerName)
                    setToken(r.token)
                    setRunnerId(r.id)
                    setRunnerName('')
                  })
                }
              >
                <label className="block space-y-1.5 text-sm">
                  Nombre del runner
                  <Input
                    value={runnerName}
                    required
                    maxLength={120}
                    onChange={(e) => setRunnerName(e.target.value)}
                    placeholder="Servidor de desarrollo"
                  />
                </label>
                <Button type="submit" disabled={busy}>
                  Registrar runner
                </Button>
              </form>
            )}
            {token && (
              <div role="status" className="space-y-3 rounded-lg bg-muted p-4">
                <p className="text-sm">
                  Guarda esta credencial en un archivo privado 0600. Se muestra una sola vez; no la
                  pegues en código ni issues.
                </p>
                <pre className="overflow-auto whitespace-pre-wrap break-all text-xs">{token}</pre>
                <Button variant="secondary" size="sm" onClick={() => setToken(null)}>
                  Ya la guardé · ocultar
                </Button>
              </div>
            )}
          </section>
          <section className={`${panel} space-y-4`}>
            <h2 className="text-base font-semibold">Registrar una carpeta</h2>
            <p className="text-sm text-muted-foreground">
              La ruta pertenece a la máquina del runner, no a tu navegador. Usa la carpeta padre que
              contiene los repositorios.
            </p>
            <form
              className="space-y-4"
              onSubmit={(e) =>
                submit(e, async () => {
                  const w = await client.createFactoryWorkspace({
                    runner_id: runnerId,
                    name: workspaceName,
                    root_path: rootPath,
                  })
                  setWorkspaceName('')
                  setRootPath('')
                  navigate({ workspace: w.id, tab: 'settings', section: 'repositories' })
                })
              }
            >
              <label className="block space-y-1.5 text-sm">
                Runner
                <select
                  className={select}
                  value={runnerId}
                  required
                  disabled={!writable}
                  onChange={(e) => setRunnerId(e.target.value)}
                >
                  <option value="">Seleccionar runner</option>
                  {catalog.data?.runners
                    .filter((r) => !r.revoked)
                    .map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.name}
                      </option>
                    ))}
                </select>
              </label>
              <label className="block space-y-1.5 text-sm">
                Nombre del workspace
                <Input
                  value={workspaceName}
                  required
                  maxLength={120}
                  disabled={!writable}
                  onChange={(e) => setWorkspaceName(e.target.value)}
                  placeholder="Kasymir"
                />
              </label>
              <label className="block space-y-1.5 text-sm">
                Carpeta en el runner
                <Input
                  value={rootPath}
                  required
                  pattern="/.*"
                  disabled={!writable}
                  onChange={(e) => setRootPath(e.target.value)}
                  placeholder="/home/ubuntu/nexusmind-workspaces/kasymir"
                />
              </label>
              <Button type="submit" disabled={busy || !writable}>
                Registrar carpeta
              </Button>
            </form>
          </section>
        </div>
      )}
      {!writable && (
        <p className="text-sm text-muted-foreground">
          Vista de solo lectura. Necesitas factory_policy:write para configurar o iniciar workflows.
        </p>
      )}
    </div>
  )
}
