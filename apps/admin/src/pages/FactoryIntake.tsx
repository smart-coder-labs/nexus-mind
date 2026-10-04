import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Radio } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { Input } from '../components/ui/Input'
import type {
  AutonomousAgentConnector,
  FactoryIntakeKind,
  FactoryIntakeSource,
  FactoryIntakeSourceInput,
  FactoryPrivacyClass,
  FactoryWatchdog,
} from '../types'

const SELECT_CLASS = 'mt-1 block w-full rounded-lg border border-border-primary bg-transparent px-3 py-2 text-sm text-text-primary focus:border-accent-blue focus:outline-none'
const PRIVACY: FactoryPrivacyClass[] = ['internal', 'confidential', 'restricted', 'public']
const NEW_TOKEN = '__new__'

function when(value: string | null): string {
  return value ? value.replace('T', ' ').slice(0, 16) : '—'
}

function message(error: unknown): string {
  return (error as { message?: string })?.message ?? 'unknown error'
}

interface Draft {
  id: string | null
  kind: FactoryIntakeKind
  name: string
  project: string
  resolver: string
  baseRef: string
  privacy: FactoryPrivacyClass
  connector: string
  newToken: string
  enabled: boolean
  channelId: string
  reactors: string
  baseUrl: string
  orgSlug: string
  projectSlug: string
  query: string
}

const EMPTY: Draft = {
  id: null,
  kind: 'slack',
  name: '',
  project: '',
  resolver: '',
  baseRef: 'main',
  privacy: 'internal',
  connector: NEW_TOKEN,
  newToken: '',
  enabled: true,
  channelId: '',
  reactors: '',
  baseUrl: 'https://sentry.io',
  orgSlug: '',
  projectSlug: '',
  query: 'is:unresolved level:[error,fatal]',
}

function draftFrom(source: FactoryIntakeSource): Draft {
  const text = (key: string) => (typeof source.config[key] === 'string' ? (source.config[key] as string) : '')
  const reactors = Array.isArray(source.config.allowed_reactors) ? (source.config.allowed_reactors as string[]) : []
  return {
    ...EMPTY,
    id: source.id,
    kind: source.kind,
    name: source.name,
    project: source.project,
    resolver: source.resolver_definition_id,
    baseRef: source.base_ref,
    privacy: source.privacy_class,
    connector: source.connector_id,
    enabled: source.enabled,
    channelId: text('channel_id'),
    reactors: reactors.join(', '),
    baseUrl: text('base_url') || EMPTY.baseUrl,
    orgSlug: text('org_slug'),
    projectSlug: text('project_slug'),
    query: text('query'),
  }
}

function sourceInput(draft: Draft, connectorId: string): FactoryIntakeSourceInput {
  const config =
    draft.kind === 'slack'
      ? {
          channel_id: draft.channelId.trim(),
          allowed_reactors: draft.reactors.split(/[\s,]+/).map(user => user.trim()).filter(Boolean),
        }
      : {
          base_url: draft.baseUrl.trim(),
          org_slug: draft.orgSlug.trim(),
          project_slug: draft.projectSlug.trim(),
          query: draft.query.trim(),
        }
  return {
    kind: draft.kind,
    name: draft.name.trim(),
    project: draft.project.trim(),
    resolver_definition_id: draft.resolver,
    base_ref: draft.baseRef.trim() || 'main',
    privacy_class: draft.privacy,
    connector_id: connectorId,
    config,
    enabled: draft.enabled,
  }
}

/**
 * Factory intake (F3): Slack channels and Sentry projects that feed the
 * factory, what came in and whether it started, and who hears about what waits
 * on a person. Slack and Sentry text is untrusted: the decision model may start
 * a run only within fixed limits, and a PR from such a run never merges
 * without a person.
 */
export default function FactoryIntake() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  // Storing a new token creates a connector, which needs its own permission.
  const canAddToken = permissions.includes('autonomous_agent:manage_connectors')
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState<Draft | null>(null)

  const sources = useQuery({ queryKey: ['factory-intake-sources'], queryFn: () => client.listFactoryIntakeSources(), enabled: canRead })
  const items = useQuery({ queryKey: ['factory-intake-items'], queryFn: () => client.listFactoryIntakeItems(50), enabled: canRead })
  const watchdog = useQuery({ queryKey: ['factory-watchdog'], queryFn: () => client.getFactoryWatchdog(), enabled: canRead })
  const connectors = useQuery({ queryKey: ['autonomous-connectors'], queryFn: () => client.listAutonomousAgentConnectors(), enabled: canWrite })
  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents(), enabled: canWrite })

  const resolvers = (agents.data ?? []).filter(agent => agent.template_key === 'github_issue_resolver' && agent.status !== 'archived')
  // Only tokens stored for this kind of intake (and, for Sentry, this host) are accepted.
  const sentryHost = (url: string) => { try { return new URL(url).hostname.toLowerCase() } catch { return '' } }
  const secretsFor = (current: Draft) =>
    (connectors.data ?? []).filter(connector =>
      connector.kind === 'target_secret' &&
      connector.health !== 'revoked' &&
      connector.metadata.purpose === 'factory_intake' &&
      connector.metadata.source_kind === current.kind &&
      (current.kind !== 'sentry' || connector.metadata.sentry_host === sentryHost(current.baseUrl)))
  const hooks = (connectors.data ?? []).filter(connector => connector.kind === 'slack' && connector.health !== 'revoked')

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['factory-intake-sources'] })
  const save = useMutation({
    mutationFn: async (current: Draft) => {
      let connectorId = current.connector
      if (connectorId === NEW_TOKEN) {
        const metadata: Record<string, unknown> = { purpose: 'factory_intake', source_kind: current.kind }
        if (current.kind === 'sentry') metadata.sentry_host = sentryHost(current.baseUrl)
        const connector = await client.putAutonomousAgentConnector({
          kind: 'target_secret',
          // A unique name: the connector upsert by name must never overwrite another token.
          name: `intake: ${current.name.trim()} · ${Date.now().toString(36)}`,
          secret: current.newToken.trim(),
          metadata,
          scopes: ['target:use'],
        })
        connectorId = connector.id
        // A retry after a failed save reuses this connector instead of making another.
        setDraft(latest => (latest ? { ...latest, connector: connector.id, newToken: '' } : latest))
        queryClient.invalidateQueries({ queryKey: ['autonomous-connectors'] })
      }
      const input = sourceInput(current, connectorId)
      return current.id ? client.updateFactoryIntakeSource(current.id, input) : client.createFactoryIntakeSource(input)
    },
    onSuccess: () => {
      setDraft(null)
      refresh()
    },
  })
  const toggle = useMutation({
    mutationFn: (source: FactoryIntakeSource) => client.setFactoryIntakeSourceEnabled(source.id, !source.enabled),
    onSuccess: refresh,
  })
  const remove = useMutation({ mutationFn: (id: string) => client.deleteFactoryIntakeSource(id), onSuccess: refresh })

  if (!canRead) {
    return (
      <div className="p-6 md:p-8 max-w-7xl mx-auto">
        <EmptyState
          title="Factory intake"
          description="You need the factory_policy:read permission to see where the factory takes work from. Ask an organization owner to grant it."
        />
      </div>
    )
  }

  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setDraft(current => (current ? { ...current, [key]: value } : current))
  const needsToken = draft?.connector === NEW_TOKEN && (!canAddToken || !draft.newToken.trim())
  const incomplete =
    !draft ||
    !draft.name.trim() ||
    !draft.project.trim() ||
    !draft.resolver ||
    needsToken ||
    (draft.kind === 'slack' ? !draft.channelId.trim() || !draft.reactors.trim() : !draft.orgSlug.trim() || !draft.projectSlug.trim() || !draft.query.trim())

  return (
    <div className="p-6 md:p-8 max-w-7xl mx-auto space-y-6">
      <header className="flex flex-wrap items-start gap-3">
        <div className="flex-1 min-w-[16rem]">
          <h1 className="text-xl font-semibold text-text-primary flex items-center gap-2">
            <Radio className="h-5 w-5" />Factory intake
          </h1>
          <p className="mt-1 max-w-2xl text-sm text-text-tertiary">
            Slack channels and Sentry projects the factory takes work from. Every new item becomes a factory task. The
            decision model may start it on its own only for docs, tests, UI or bug fixes, at most 5 a day, in private
            repositories, and the pull request it opens always waits for a person before merging.
          </p>
        </div>
        {canWrite && !draft && (
          <Button size="sm" onClick={() => setDraft({ ...EMPTY, connector: secretsFor(EMPTY)[0]?.id ?? NEW_TOKEN, resolver: resolvers[0]?.id ?? '' })}>
            Add source
          </Button>
        )}
      </header>

      {draft && (
        <section aria-labelledby="source-form-title" className="rounded-xl border border-border-primary p-4 space-y-4">
          <h2 id="source-form-title" className="text-sm font-semibold text-text-primary">{draft.id ? `Edit ${draft.name}` : 'New intake source'}</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="block text-sm text-text-secondary" htmlFor="intake-kind">
              Source
              <select id="intake-kind" className={SELECT_CLASS} value={draft.kind} disabled={!!draft.id} onChange={event => set('kind', event.target.value as FactoryIntakeKind)}>
                <option value="slack">Slack channel</option>
                <option value="sentry">Sentry project</option>
              </select>
            </label>
            <label className="block text-sm text-text-secondary" htmlFor="intake-name">
              Name
              <Input id="intake-name" inputSize="sm" value={draft.name} placeholder="Bugs channel" onChange={event => set('name', event.target.value)} />
            </label>
            <label className="block text-sm text-text-secondary" htmlFor="intake-resolver">
              Issue resolver (sets the repository)
              <select id="intake-resolver" className={SELECT_CLASS} value={draft.resolver} onChange={event => set('resolver', event.target.value)}>
                <option value="">Choose an agent</option>
                {resolvers.map(agent => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
                {draft.resolver && !resolvers.some(agent => agent.id === draft.resolver) && (
                  <option value={draft.resolver}>Unavailable agent (choose another)</option>
                )}
              </select>
            </label>
            <label className="block text-sm text-text-secondary" htmlFor="intake-project">
              NexusMind project for the tasks
              <Input id="intake-project" inputSize="sm" value={draft.project} placeholder="app" onChange={event => set('project', event.target.value)} />
            </label>
            <label className="block text-sm text-text-secondary" htmlFor="intake-base">
              Base branch
              <Input id="intake-base" inputSize="sm" value={draft.baseRef} onChange={event => set('baseRef', event.target.value)} />
            </label>
            <label className="block text-sm text-text-secondary" htmlFor="intake-privacy">
              Data privacy
              <select id="intake-privacy" className={SELECT_CLASS} value={draft.privacy} onChange={event => set('privacy', event.target.value as FactoryPrivacyClass)}>
                {PRIVACY.map(value => <option key={value} value={value}>{value}{value === 'restricted' ? ' (never starts on its own)' : ''}</option>)}
              </select>
            </label>
          </div>

          {draft.kind === 'slack' ? (
            <div className="grid gap-4 sm:grid-cols-2">
              <label className="block text-sm text-text-secondary" htmlFor="intake-channel">
                Channel ID
                <Input id="intake-channel" inputSize="sm" value={draft.channelId} placeholder="C0123ABCD" onChange={event => set('channelId', event.target.value)} />
              </label>
              <label className="block text-sm text-text-secondary" htmlFor="intake-reactors">
                Who can send a message to the factory (Slack user IDs)
                <Input id="intake-reactors" inputSize="sm" value={draft.reactors} placeholder="U0123ABCD, U0456EFGH" onChange={event => set('reactors', event.target.value)} />
              </label>
              <p className="sm:col-span-2 text-xs text-text-tertiary">
                A top-level message counts only when one of these people reacts to it with <code>:factory:</code>. The bot
                token needs <code>channels:history</code> and must be in the channel.
              </p>
            </div>
          ) : (
            <div className="grid gap-4 sm:grid-cols-2">
              <label className="block text-sm text-text-secondary" htmlFor="intake-sentry-url">
                Sentry URL
                <Input id="intake-sentry-url" inputSize="sm" value={draft.baseUrl} onChange={event => set('baseUrl', event.target.value)} />
              </label>
              <label className="block text-sm text-text-secondary" htmlFor="intake-sentry-query">
                Issues to take (Sentry search)
                <Input id="intake-sentry-query" inputSize="sm" value={draft.query} onChange={event => set('query', event.target.value)} />
              </label>
              <label className="block text-sm text-text-secondary" htmlFor="intake-sentry-org">
                Organization slug
                <Input id="intake-sentry-org" inputSize="sm" value={draft.orgSlug} onChange={event => set('orgSlug', event.target.value)} />
              </label>
              <label className="block text-sm text-text-secondary" htmlFor="intake-sentry-project">
                Project slug
                <Input id="intake-sentry-project" inputSize="sm" value={draft.projectSlug} onChange={event => set('projectSlug', event.target.value)} />
              </label>
              <p className="sm:col-span-2 text-xs text-text-tertiary">
                Only unresolved error or fatal issues that match the search become tasks. Narrow it (for example with a tag) so
                a noisy project does not flood the factory.
              </p>
            </div>
          )}

          <div className="grid gap-4 sm:grid-cols-2">
            <label className="block text-sm text-text-secondary" htmlFor="intake-token">
              Token
              <select id="intake-token" className={SELECT_CLASS} value={draft.connector} onChange={event => set('connector', event.target.value)}>
                {canAddToken && <option value={NEW_TOKEN}>Paste a new token</option>}
                {!canAddToken && draft.connector === NEW_TOKEN && <option value={NEW_TOKEN}>Choose a stored token</option>}
                {secretsFor(draft).map((connector: AutonomousAgentConnector) => <option key={connector.id} value={connector.id}>{connector.name}</option>)}
                {draft.connector !== NEW_TOKEN && !secretsFor(draft).some(connector => connector.id === draft.connector) && (
                  <option value={draft.connector}>Unavailable token (paste or choose another)</option>
                )}
              </select>
            </label>
            {draft.connector === NEW_TOKEN && canAddToken && (
              <label className="block text-sm text-text-secondary" htmlFor="intake-new-token">
                {draft.kind === 'slack' ? 'Slack bot token (xoxb-…)' : 'Sentry auth token'}
                <Input id="intake-new-token" inputSize="sm" type="password" autoComplete="off" value={draft.newToken} onChange={event => set('newToken', event.target.value)} />
              </label>
            )}
          </div>
          <p className="text-xs text-text-tertiary">
            {canAddToken
              ? 'Tokens are stored encrypted and are never shown again. A Sentry token only works for the Sentry URL it was stored for.'
              : 'Adding a token needs the permission to manage connectors; you can choose a token stored for this kind of source.'}
          </p>

          {save.isError && <p role="alert" className="text-xs text-text-primary">The source was not saved: {message(save.error)}.</p>}
          <div className="flex gap-2">
            <Button size="sm" disabled={incomplete || save.isPending} onClick={() => save.mutate(draft)}>{save.isPending ? 'Saving…' : 'Save source'}</Button>
            <Button size="sm" variant="ghost" onClick={() => { setDraft(null); save.reset() }}>Cancel</Button>
          </div>
        </section>
      )}

      {sources.isLoading && <p className="text-sm text-text-tertiary">Loading…</p>}
      {sources.isError && <p role="alert" className="text-sm text-text-primary">Could not load the intake sources.</p>}
      {sources.data && sources.data.length === 0 && !draft && (
        <p className="text-sm text-text-tertiary">No intake sources yet. GitHub issues already reach the factory through the issue resolver.</p>
      )}
      {sources.data && sources.data.length > 0 && (
        <section aria-labelledby="sources-title" className="rounded-xl border border-border-primary p-4 space-y-2">
          <h2 id="sources-title" className="text-sm font-semibold text-text-primary">Sources</h2>
          <ul className="m-0 p-0 list-none divide-y divide-border-primary">
            {sources.data.map(source => (
              <li key={source.id} className="py-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                <span className="font-medium text-text-primary">{source.name}</span>
                <span className="rounded border border-border-primary px-1.5 py-0.5">{source.kind}</span>
                <span className="font-mono text-text-secondary">{source.repository ?? 'no repository'}</span>
                <span className="text-text-tertiary">{source.enabled ? `polled ${when(source.last_polled_at)}` : 'paused'}</span>
                {source.last_error && <span role="status" className="text-text-primary">Last poll failed: {source.last_error}</span>}
                {canWrite && (
                  <span className="ml-auto flex gap-1.5">
                    <Button size="sm" variant="ghost" aria-label={`Edit ${source.name}`} onClick={() => setDraft(draftFrom(source))}>Edit</Button>
                    <Button size="sm" variant="secondary" aria-label={`${source.enabled ? 'Pause' : 'Resume'} ${source.name}`} disabled={toggle.isPending} onClick={() => toggle.mutate(source)}>
                      {source.enabled ? 'Pause' : 'Resume'}
                    </Button>
                    <Button size="sm" variant="ghost" aria-label={`Delete ${source.name}`} disabled={remove.isPending} onClick={() => { if (window.confirm(`Delete the intake source ${source.name}? Tasks already created stay.`)) remove.mutate(source.id) }}>
                      Delete
                    </Button>
                  </span>
                )}
              </li>
            ))}
          </ul>
          {(toggle.isError || remove.isError) && <p role="alert" className="text-xs text-text-primary">The change was not saved: {message(toggle.error ?? remove.error)}.</p>}
        </section>
      )}

      {items.data && items.data.length > 0 && (
        <section aria-labelledby="items-title" className="rounded-xl border border-border-primary p-4 space-y-2">
          <h2 id="items-title" className="text-sm font-semibold text-text-primary">What came in</h2>
          <table className="w-full text-xs" aria-label="Intake items">
            <thead>
              <tr className="text-left text-text-tertiary">
                <th className="py-1 font-medium">When</th>
                <th className="py-1 font-medium">From</th>
                <th className="py-1 font-medium">Class</th>
                <th className="py-1 font-medium">Decision</th>
              </tr>
            </thead>
            <tbody>
              {items.data.map(item => (
                <tr key={item.task_id} className="border-t border-border-primary">
                  <td className="py-1.5 text-text-tertiary">{when(item.created_at)}</td>
                  <td className="py-1.5 font-mono">{item.source_ref}</td>
                  <td className="py-1.5">{item.task_class}</td>
                  <td className="py-1.5">
                    {item.start_decision === 'started' ? (
                      <a className="underline" href={`https://github.com/${item.repository}/issues/${item.issue_number}`} target="_blank" rel="noreferrer">
                        Started · issue #{item.issue_number}
                      </a>
                    ) : (
                      <span className="text-text-secondary">Waiting for a person · {item.start_reason}</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}

      <WatchdogCard watchdog={watchdog.data ?? null} loading={watchdog.isLoading} failed={watchdog.isError} hooks={hooks} canWrite={canWrite} />
    </div>
  )
}

function WatchdogCard({
  watchdog,
  loading,
  failed,
  hooks,
  canWrite,
}: {
  watchdog: FactoryWatchdog | null
  loading: boolean
  failed: boolean
  hooks: AutonomousAgentConnector[]
  canWrite: boolean
}) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const [connector, setConnector] = useState<string | null>(null)
  const [hour, setHour] = useState<number | null>(null)
  const selected = connector ?? watchdog?.slack_connector_id ?? ''
  const dailyHour = hour ?? watchdog?.daily_hour_utc ?? 13
  const save = useMutation({
    mutationFn: (enabled: boolean) => client.putFactoryWatchdog({ slack_connector_id: selected || null, enabled, daily_hour_utc: dailyHour }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['factory-watchdog'] }),
  })
  const enabled = !!watchdog?.enabled && !!watchdog.slack_connector_id

  return (
    <section aria-labelledby="watchdog-title" className="rounded-xl border border-border-primary p-4 space-y-3">
      <h2 id="watchdog-title" className="text-sm font-semibold text-text-primary">Slack notifications</h2>
      <p className="text-xs text-text-secondary">
        Posts to Slack when a merge is held for a person, a run stops short or a new factory task arrives (at most one
        message every 15 minutes), plus a daily summary.
      </p>
      {loading && <p className="text-xs text-text-tertiary">Loading…</p>}
      {failed && <p role="alert" className="text-xs text-text-primary">Could not load the notification settings.</p>}
      <p className="text-xs text-text-primary">{enabled ? `On · last message ${when(watchdog?.last_sent_at ?? null)}` : 'Off'}</p>
      {canWrite && (
        <div className="grid gap-4 sm:grid-cols-3 items-end">
          <label className="block text-sm text-text-secondary" htmlFor="watchdog-connector">
            Slack webhook
            <select id="watchdog-connector" className={SELECT_CLASS} value={selected} onChange={event => setConnector(event.target.value)}>
              <option value="">Choose a Slack connector</option>
              {hooks.map(hook => <option key={hook.id} value={hook.id}>{hook.name}</option>)}
            </select>
          </label>
          <label className="block text-sm text-text-secondary" htmlFor="watchdog-hour">
            Daily summary hour (UTC)
            <Input id="watchdog-hour" inputSize="sm" type="number" min={0} max={23} value={dailyHour} onChange={event => setHour(Math.min(23, Math.max(0, Number(event.target.value) || 0)))} />
          </label>
          <span className="flex gap-2">
            <Button size="sm" disabled={!selected || save.isPending} onClick={() => save.mutate(true)}>{enabled ? 'Save' : 'Turn on'}</Button>
            {enabled && <Button size="sm" variant="ghost" disabled={save.isPending} onClick={() => save.mutate(false)}>Turn off</Button>}
          </span>
        </div>
      )}
      {canWrite && hooks.length === 0 && <p className="text-xs text-text-tertiary">Add a Slack webhook connector in Autonomous agents first.</p>}
      {save.isError && <p role="alert" className="text-xs text-text-primary">The settings were not saved: {message(save.error)}.</p>}
    </section>
  )
}
