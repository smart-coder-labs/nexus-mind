import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { BellRing, GitPullRequest, Hash, Plus, Radio, Siren, Trash2, X } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { Input } from '../components/ui/Input'
import { Modal, ModalContent, ModalDescription, ModalFooter, ModalHeader, ModalTitle } from '../components/ui/Modal'
import { cn } from '../lib/utils'
import type {
  AutonomousAgentConnector,
  FactoryIntakeItem,
  FactoryIntakeKind,
  FactoryIntakeSource,
  FactoryIntakeSourceInput,
  FactoryPrivacyClass,
  FactoryWatchdog,
} from '../types'
import { CardListSkeleton, Field, FieldGroup, InlineAlert, LINK_CLASS, PageHeader, PANEL_CLASS, PermissionDenied, RadioCard, SectionHeading, SELECT_CLASS, when } from './factory/govern/ui'
import { PRIVACY_LABEL, startReason, taskClassLabel } from './factory/govern/words'

const PRIVACY: FactoryPrivacyClass[] = ['internal', 'confidential', 'restricted', 'public']
const NEW_TOKEN = '__new__'

const KIND_LABEL: Record<FactoryIntakeKind, string> = { slack: 'Slack channel', sentry: 'Sentry project' }

function KindIcon({ kind, className }: { kind: FactoryIntakeKind | 'github'; className?: string }) {
  const Icon = kind === 'slack' ? Hash : kind === 'sentry' ? Siren : GitPullRequest
  return <Icon className={className ?? 'h-4 w-4'} aria-hidden="true" />
}

function message(error: unknown): string {
  return (error as { message?: string })?.message ?? 'unknown error'
}

/** Known poll failures → what to do about them. */
function pollHint(error: string): string | null {
  if (error.includes('not_in_channel')) return 'Invite the Slack bot to the channel.'
  if (error.includes('invalid_auth') || error.includes('token_revoked')) return 'The token is no longer valid. Edit the source and paste a new one.'
  if (error.includes('missing_scope')) return 'The Slack token needs the channels:history scope.'
  return null
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
    return <PermissionDenied title="Intake" permission="factory_policy:read" what="see where the factory takes work from" />
  }

  const closeEditor = () => { setDraft(null); save.reset() }
  const startNew = () => setDraft({ ...EMPTY, connector: secretsFor(EMPTY)[0]?.id ?? NEW_TOKEN, resolver: resolvers[0]?.id ?? '' })
  const itemCount = (sourceId: string) => (items.data ?? []).filter(item => item.source_id === sourceId).length
  const sourceName = (item: FactoryIntakeItem) => sources.data?.find(source => source.id === item.source_id)?.name

  return (
    <div className="p-6 md:p-8 max-w-7xl mx-auto space-y-8">
      <PageHeader
        title="Intake"
        subtitle="Where the factory takes work from. Every new item becomes a factory task."
        action={canWrite && <Button size="sm" leftIcon={<Plus className="h-4 w-4" />} onClick={startNew}>Add source</Button>}
      />
      <p className="-mt-4 max-w-3xl text-[12px] leading-normal text-text-tertiary">
        The decision model may start only docs, tests, UI or bug fixes on its own, at most 5 a day, in private repositories.
        The pull request it opens always waits for a person before merging.
      </p>

      {sources.isLoading && <CardListSkeleton count={2} height="h-36" />}
      {sources.isError && <InlineAlert onRetry={() => sources.refetch()}>Could not load the intake sources.</InlineAlert>}
      {sources.data && sources.data.length === 0 && (
        <EmptyState
          icon={<Radio />}
          title="No intake sources yet"
          description="GitHub issues already reach the factory through the issue resolver. Add a Slack channel or Sentry project to bring in more work."
        />
      )}
      {sources.data && sources.data.length > 0 && (
        <section aria-labelledby="sources-title" className="space-y-3">
          <SectionHeading id="sources-title" title="Sources" />
          <ul className="m-0 grid list-none gap-4 p-0 lg:grid-cols-2">
            {sources.data.map(source => (
              <li key={source.id} className={cn(PANEL_CLASS, 'flex flex-col gap-4')}>
                <div className="flex items-start gap-3">
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-border-primary bg-white/[0.04] text-text-secondary">
                    <KindIcon kind={source.kind} />
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{source.name}</p>
                    <p className="text-[12px] text-text-tertiary">{KIND_LABEL[source.kind]}</p>
                  </div>
                  {!source.enabled ? (
                    <Badge role="none" size="sm" variant="default">Paused</Badge>
                  ) : source.last_error ? (
                    <Badge role="none" size="sm" variant="error" dot>Failing</Badge>
                  ) : (
                    <Badge role="none" size="sm" variant="success" dot>Connected</Badge>
                  )}
                </div>
                <dl className="grid grid-cols-[8rem_1fr] gap-x-3 gap-y-1.5 text-[13px]">
                  <dt className="text-text-tertiary">Repository</dt>
                  <dd className="min-w-0 truncate">
                    {source.repository ? <span className="font-mono text-text-primary">{source.repository}</span> : <span className="text-text-secondary">No repository</span>}
                  </dd>
                  <dt className="text-text-tertiary">Tasks go to</dt>
                  <dd className="min-w-0 truncate text-text-secondary">{source.project}</dd>
                  <dt className="text-text-tertiary">Data privacy</dt>
                  <dd className="text-text-secondary">{PRIVACY_LABEL[source.privacy_class]}</dd>
                  <dt className="text-text-tertiary">Last fetch</dt>
                  <dd className="text-text-secondary">{source.enabled ? when(source.last_polled_at) : 'Paused'}</dd>
                  <dt className="text-text-tertiary">Brought in</dt>
                  <dd className="tabular-nums text-text-secondary">{itemCount(source.id)} recent items</dd>
                </dl>
                {source.last_error && (
                  <div role="status" className="rounded-[11px] border border-status-error/20 bg-status-error/[0.08] px-3.5 py-2.5 text-[13px]">
                    <p className="break-words text-text-primary">Last poll failed: {source.last_error}</p>
                    {pollHint(source.last_error) && <p className="mt-0.5 text-[12px] text-text-secondary">{pollHint(source.last_error)}</p>}
                  </div>
                )}
                {canWrite && (
                  <div className="mt-auto flex flex-wrap items-center gap-2 border-t border-border-secondary pt-4">
                    <Button size="sm" variant="secondary" aria-label={`Edit ${source.name}`} onClick={() => setDraft(draftFrom(source))}>Edit</Button>
                    <Button size="sm" variant="secondary" aria-label={`${source.enabled ? 'Pause' : 'Resume'} ${source.name}`} disabled={toggle.isPending} onClick={() => toggle.mutate(source)}>
                      {source.enabled ? 'Pause' : 'Resume'}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="ml-auto"
                      aria-label={`Delete ${source.name}`}
                      leftIcon={<Trash2 className="h-4 w-4" />}
                      disabled={remove.isPending}
                      onClick={() => { if (window.confirm(`Delete the intake source ${source.name}? Tasks already created stay.`)) remove.mutate(source.id) }}
                    />
                  </div>
                )}
              </li>
            ))}
            <li className="flex items-start gap-3 rounded-[18px] border border-dashed border-border-primary p-5">
              <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[11px] border border-border-primary text-text-tertiary">
                <KindIcon kind="github" />
              </span>
              <div className="min-w-0">
                <p className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">GitHub issues</p>
                <p className="mt-0.5 text-[13px] text-text-secondary">Reach the factory through the issue resolver agent. No source needed.</p>
              </div>
            </li>
          </ul>
          {(toggle.isError || remove.isError) && <InlineAlert>The change was not saved: {message(toggle.error ?? remove.error)}.</InlineAlert>}
        </section>
      )}

      {items.isError && <InlineAlert onRetry={() => items.refetch()}>Could not load what came in.</InlineAlert>}
      {items.data && items.data.length > 0 && (
        <section aria-labelledby="items-title" className="space-y-3">
          <SectionHeading id="items-title" title="What came in" description="The latest 50 items and whether each started on its own." />
          <div className="overflow-x-auto rounded-[18px] border border-border-primary">
            <table className="w-full min-w-[44rem] text-[13px]" aria-label="Intake items">
              <thead>
                <tr className="border-b border-border-secondary text-left text-[12px] text-text-tertiary">
                  <th scope="col" className="px-4 py-2.5 font-medium">When</th>
                  <th scope="col" className="px-4 py-2.5 font-medium">From</th>
                  <th scope="col" className="px-4 py-2.5 font-medium">Task class</th>
                  <th scope="col" className="px-4 py-2.5 font-medium">Decision</th>
                </tr>
              </thead>
              <tbody>
                {items.data.map(item => (
                  <tr key={item.task_id} className="border-b border-border-secondary align-top last:border-0 hover:bg-white/[0.03]">
                    <td className="whitespace-nowrap px-4 py-2.5 text-text-tertiary">{when(item.created_at)}</td>
                    <td className="px-4 py-2.5">
                      {sourceName(item) && <span className="block text-text-primary">{sourceName(item)}</span>}
                      <span className="font-mono text-[12px] text-text-tertiary">{item.source_ref}</span>
                    </td>
                    <td className="px-4 py-2.5 text-text-secondary">{taskClassLabel(item.task_class)}</td>
                    <td className="px-4 py-2.5">
                      {item.start_decision === 'started' ? (
                        <a className={LINK_CLASS} href={`https://github.com/${item.repository}/issues/${item.issue_number}`} target="_blank" rel="noreferrer">
                          Started as issue #{item.issue_number}
                        </a>
                      ) : (
                        <div className="space-y-0.5">
                          <span className="block text-text-primary">Waiting for a person</span>
                          <span className="block text-[12px] text-text-secondary">{startReason(item.start_reason)}</span>
                          <code className="block font-mono text-[11px] text-text-tertiary">{item.start_reason}</code>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}

      <WatchdogCard watchdog={watchdog.data ?? null} loading={watchdog.isLoading} failed={watchdog.isError} onRetry={() => watchdog.refetch()} hooks={hooks} canWrite={canWrite} />

      <SourceEditor
        draft={draft}
        setDraft={setDraft}
        onClose={closeEditor}
        resolvers={resolvers}
        secrets={draft ? secretsFor(draft) : []}
        canAddToken={canAddToken}
        saving={save.isPending}
        saveError={save.isError ? `The source was not saved: ${message(save.error)}.` : ''}
        onSave={current => save.mutate(current)}
      />
    </div>
  )
}

function SourceEditor({
  draft,
  setDraft,
  onClose,
  resolvers,
  secrets,
  canAddToken,
  saving,
  saveError,
  onSave,
}: {
  draft: Draft | null
  setDraft: (update: (current: Draft | null) => Draft | null) => void
  onClose: () => void
  resolvers: { id: string; name: string }[]
  secrets: AutonomousAgentConnector[]
  canAddToken: boolean
  saving: boolean
  saveError: string
  onSave: (draft: Draft) => void
}) {
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
    <Modal open={draft !== null} onOpenChange={value => { if (!value) onClose() }} size="xl" position="right">
      {draft && (
        <div className="flex min-h-full flex-col">
          <ModalHeader className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <ModalTitle>{draft.id ? `Edit ${draft.name}` : 'New intake source'}</ModalTitle>
              <ModalDescription>Slack and Sentry text is untrusted: what it starts never merges without a person.</ModalDescription>
            </div>
            <Button size="sm" variant="ghost" aria-label="Close" leftIcon={<X className="h-4 w-4" />} onClick={onClose} />
          </ModalHeader>
          <ModalContent className="flex-1 space-y-6">
            <FieldGroup title="Source">
              <div role="radiogroup" aria-label="Kind of source" className="grid gap-2 sm:grid-cols-2">
                {(['slack', 'sentry'] as FactoryIntakeKind[]).map(kind => (
                  <RadioCard
                    key={kind}
                    name="intake-kind"
                    value={kind}
                    checked={draft.kind === kind}
                    disabled={!!draft.id}
                    onChange={value => set('kind', value as FactoryIntakeKind)}
                    icon={<KindIcon kind={kind} className="h-4 w-4 text-text-secondary" />}
                    title={KIND_LABEL[kind]}
                    description={kind === 'slack' ? 'Messages someone marks for the factory.' : 'Unresolved errors that match a search.'}
                  />
                ))}
              </div>
              <Field id="intake-name" label="Name">
                <Input id="intake-name" value={draft.name} placeholder="Bugs channel" onChange={event => set('name', event.target.value)} />
              </Field>
              {draft.kind === 'slack' ? (
                <Field id="intake-channel" label="Channel ID" hint="The bot must be a member of the channel.">
                  <Input id="intake-channel" value={draft.channelId} placeholder="C0123ABCD" onChange={event => set('channelId', event.target.value)} />
                </Field>
              ) : (
                <>
                  <Field id="intake-sentry-url" label="Sentry URL">
                    <Input id="intake-sentry-url" value={draft.baseUrl} onChange={event => set('baseUrl', event.target.value)} />
                  </Field>
                  <div className="grid gap-4 sm:grid-cols-2">
                    <Field id="intake-sentry-org" label="Organization slug">
                      <Input id="intake-sentry-org" value={draft.orgSlug} onChange={event => set('orgSlug', event.target.value)} />
                    </Field>
                    <Field id="intake-sentry-project" label="Project slug">
                      <Input id="intake-sentry-project" value={draft.projectSlug} onChange={event => set('projectSlug', event.target.value)} />
                    </Field>
                  </div>
                </>
              )}
            </FieldGroup>

            <FieldGroup title="Where work lands">
              <Field id="intake-resolver" label="Issue resolver (sets the repository)">
                <select id="intake-resolver" className={SELECT_CLASS} value={draft.resolver} onChange={event => set('resolver', event.target.value)}>
                  <option value="">Choose an agent</option>
                  {resolvers.map(agent => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
                  {draft.resolver && !resolvers.some(agent => agent.id === draft.resolver) && (
                    <option value={draft.resolver}>Unavailable agent (choose another)</option>
                  )}
                </select>
              </Field>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field id="intake-project" label="NexusMind project for the tasks">
                  <Input id="intake-project" value={draft.project} placeholder="app" onChange={event => set('project', event.target.value)} />
                </Field>
                <Field id="intake-base" label="Base branch">
                  <Input id="intake-base" value={draft.baseRef} onChange={event => set('baseRef', event.target.value)} />
                </Field>
              </div>
              <Field id="intake-privacy" label="Data privacy" hint="Restricted sources never start work on their own.">
                <select id="intake-privacy" className={SELECT_CLASS} value={draft.privacy} onChange={event => set('privacy', event.target.value as FactoryPrivacyClass)}>
                  {PRIVACY.map(value => <option key={value} value={value}>{PRIVACY_LABEL[value]}{value === 'restricted' ? ' (never starts on its own)' : ''}</option>)}
                </select>
              </Field>
            </FieldGroup>

            <FieldGroup
              title="Credentials"
              description={
                canAddToken
                  ? 'Tokens are stored encrypted and are never shown again. A Sentry token only works for the Sentry URL it was stored for.'
                  : 'Adding a token needs the permission to manage connectors; you can choose a token stored for this kind of source.'
              }
            >
              <Field id="intake-token" label="Token">
                <select id="intake-token" className={SELECT_CLASS} value={draft.connector} onChange={event => set('connector', event.target.value)}>
                  {canAddToken && <option value={NEW_TOKEN}>Paste a new token</option>}
                  {!canAddToken && draft.connector === NEW_TOKEN && <option value={NEW_TOKEN}>Choose a stored token</option>}
                  {secrets.map(connector => <option key={connector.id} value={connector.id}>{connector.name}</option>)}
                  {draft.connector !== NEW_TOKEN && !secrets.some(connector => connector.id === draft.connector) && (
                    <option value={draft.connector}>Unavailable token (paste or choose another)</option>
                  )}
                </select>
              </Field>
              {draft.connector === NEW_TOKEN && canAddToken && (
                <Field id="intake-new-token" label={draft.kind === 'slack' ? 'Slack bot token (xoxb-…)' : 'Sentry auth token'}>
                  <Input id="intake-new-token" type="password" autoComplete="off" value={draft.newToken} onChange={event => set('newToken', event.target.value)} />
                </Field>
              )}
            </FieldGroup>

            <FieldGroup title="Filter">
              {draft.kind === 'slack' ? (
                <Field
                  id="intake-reactors"
                  label="Who can send a message to the factory (Slack user IDs)"
                  hint={<>A top-level message counts only when one of these people reacts to it with <code className="font-mono">:factory:</code>. The bot token needs <code className="font-mono">channels:history</code>.</>}
                >
                  <Input id="intake-reactors" value={draft.reactors} placeholder="U0123ABCD, U0456EFGH" onChange={event => set('reactors', event.target.value)} />
                </Field>
              ) : (
                <Field
                  id="intake-sentry-query"
                  label="Issues to take (Sentry search)"
                  hint="Only unresolved error or fatal issues that match become tasks. Narrow it, for example with a tag, so a noisy project does not flood the factory."
                >
                  <Input id="intake-sentry-query" className="font-mono" value={draft.query} onChange={event => set('query', event.target.value)} />
                </Field>
              )}
            </FieldGroup>

            {saveError && <InlineAlert>{saveError}</InlineAlert>}
          </ModalContent>
          <ModalFooter className="border-t border-border-primary pt-4">
            <Button size="sm" variant="secondary" onClick={onClose}>Cancel</Button>
            <Button size="sm" disabled={incomplete || saving} onClick={() => onSave(draft)}>{saving ? 'Saving…' : 'Save source'}</Button>
          </ModalFooter>
        </div>
      )}
    </Modal>
  )
}

function WatchdogCard({
  watchdog,
  loading,
  failed,
  onRetry,
  hooks,
  canWrite,
}: {
  watchdog: FactoryWatchdog | null
  loading: boolean
  failed: boolean
  onRetry: () => void
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
    <section aria-labelledby="watchdog-title" className="space-y-3 border-t border-border-primary pt-8">
      <div className="flex flex-wrap items-start gap-3">
        <BellRing className="mt-0.5 h-5 w-5 shrink-0 text-text-secondary" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 id="watchdog-title" className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Slack notifications</h2>
            {!loading && !failed && (
              <Badge role="none" size="sm" variant={enabled ? 'success' : 'default'}>{enabled ? 'On' : 'Off'}</Badge>
            )}
          </div>
          <p className="mt-1 max-w-3xl text-[13px] text-text-secondary">
            Posts to Slack when a merge is held for a person, a run stops short or a new factory task arrives (at most one
            message every 15 minutes), plus a daily summary.
            {enabled && <> Last message {when(watchdog?.last_sent_at ?? null)}.</>}
          </p>
        </div>
      </div>
      {loading && <CardListSkeleton count={1} height="h-20" />}
      {failed && <InlineAlert onRetry={onRetry}>Could not load the notification settings.</InlineAlert>}
      {canWrite && (
        <div className={cn(PANEL_CLASS, 'grid items-end gap-4 sm:grid-cols-[minmax(0,1fr)_12rem_auto]')}>
          <Field id="watchdog-connector" label="Slack webhook" hint={hooks.length === 0 ? 'Add a Slack webhook connector in Autonomous agents first.' : undefined}>
            <select id="watchdog-connector" className={SELECT_CLASS} value={selected} onChange={event => setConnector(event.target.value)}>
              <option value="">Choose a Slack connector</option>
              {hooks.map(hook => <option key={hook.id} value={hook.id}>{hook.name}</option>)}
            </select>
          </Field>
          <Field id="watchdog-hour" label="Daily summary hour (UTC)">
            <Input id="watchdog-hour" type="number" min={0} max={23} value={dailyHour} onChange={event => setHour(Math.min(23, Math.max(0, Number(event.target.value) || 0)))} />
          </Field>
          <div className="flex gap-2 sm:pb-0">
            {enabled && <Button size="sm" variant="secondary" disabled={save.isPending} onClick={() => save.mutate(false)}>Turn off</Button>}
            <Button size="sm" disabled={!selected || save.isPending} onClick={() => save.mutate(true)}>{enabled ? 'Save' : 'Turn on'}</Button>
          </div>
        </div>
      )}
      {save.isError && <InlineAlert>The settings were not saved: {message(save.error)}.</InlineAlert>}
    </section>
  )
}
