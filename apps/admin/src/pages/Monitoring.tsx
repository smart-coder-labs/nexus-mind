import { StyledSelect } from '@/components/ui/Select/StyledSelect'
import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Activity, Copy, ExternalLink, Plus, Terminal, Trash2, X } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { Input } from '../components/ui/Input'
import { Modal, ModalContent, ModalDescription, ModalFooter, ModalHeader, ModalTitle } from '../components/ui/Modal'
import { cn } from '../lib/utils'
import type { AutonomousAgentConnector, MonitorGroup, MonitorGroupStatus, MonitorSource, MonitorSourceInput, MonitorTestResult } from '../types'
import { CardListSkeleton, Field, FieldGroup, InlineAlert, LINK_CLASS, PageHeader, PANEL_CLASS, PermissionDenied, SectionHeading, SELECT_CLASS, when } from './factory/govern/ui'

const GCP_PROJECT = /^[a-z][a-z0-9-]{4,28}[a-z0-9]$/

/** The public URL Pub/Sub pushes to for one source. */
export function pushEndpoint(sourceId: string, apiBase = import.meta.env.VITE_API_URL || window.location.origin): string {
  return `${apiBase.replace(/\/+$/, '')}/v1/monitor/gcp/${sourceId}`
}

export interface SetupStep {
  title: string
  detail: string
  command: string
  optional?: boolean
}

/**
 * The one-time `gcloud` setup that routes a GCP project's errors to a source,
 * as numbered steps. Every value is either validated here or by the server
 * (service account, uuid audience), so the commands are safe to paste.
 */
export function setupSteps(source: Pick<MonitorSource, 'id' | 'service_account' | 'audience'>, gcpProject: string, endpoint: string): SetupStep[] {
  const project = GCP_PROJECT.test(gcpProject) ? gcpProject : 'PROJECT_ID'
  const account = source.service_account
  const accountName = account.split('@')[0]
  const ownAccount = account.endsWith(`@${project}.iam.gserviceaccount.com`)
  const steps: SetupStep[] = [
    {
      title: 'Sign in and pick the project',
      detail: 'Use Cloud Shell, or a terminal where you are signed in as an owner or editor of the project.',
      command: ['gcloud auth list', `gcloud projects describe ${project} --format='value(projectNumber)'`].join('\n'),
    },
  ]
  if (ownAccount) {
    steps.push({
      title: 'Create the push service account',
      detail: 'Pub/Sub signs every push as this account and NexusMind only accepts pushes signed by it. It needs no roles and no key. Skip if it already exists.',
      command: `gcloud iam service-accounts create ${accountName} --project=${project} --display-name="NexusMind push"`,
    })
  }
  steps.push(
    {
      title: 'Create the topic',
      detail: 'Errors from the project are collected here.',
      command: `gcloud pubsub topics create nexusmind-errors --project=${project}`,
    },
    {
      title: 'Send ERROR logs to the topic',
      detail: 'A log sink copies every log with severity ERROR or worse, without audit logs. Logs keep going to Cloud Logging as before.',
      command: [
        'gcloud logging sinks create nexusmind-errors \\',
        `  pubsub.googleapis.com/projects/${project}/topics/nexusmind-errors \\`,
        `  --project=${project} \\`,
        `  --log-filter='severity>=ERROR AND NOT logName:"cloudaudit.googleapis.com"'`,
      ].join('\n'),
    },
    {
      title: 'Let the sink publish to the topic',
      detail: 'The sink writes as its own identity, which needs the publisher role on the topic.',
      command: [
        `gcloud pubsub topics add-iam-policy-binding nexusmind-errors --project=${project} \\`,
        `  --member="$(gcloud logging sinks describe nexusmind-errors --project=${project} --format='value(writerIdentity)')" \\`,
        '  --role=roles/pubsub.publisher',
      ].join('\n'),
    },
    {
      title: 'Let Pub/Sub sign as the service account',
      detail: 'Only needed for projects created before April 2021. If you are unsure, run it: it is harmless.',
      optional: true,
      command: [
        `PROJECT_NUMBER=$(gcloud projects describe ${project} --format='value(projectNumber)')`,
        `gcloud iam service-accounts add-iam-policy-binding ${account} \\`,
        `  --project=${project} \\`,
        '  --member="serviceAccount:service-${PROJECT_NUMBER}@gcp-sa-pubsub.iam.gserviceaccount.com" \\',
        '  --role=roles/iam.serviceAccountTokenCreator',
      ].join('\n'),
    },
    {
      title: 'Push errors to NexusMind',
      detail: 'The subscription posts each error to NexusMind, signed as the service account, with this source as the audience.',
      command: [
        `gcloud pubsub subscriptions create nexusmind-push --project=${project} \\`,
        '  --topic=nexusmind-errors \\',
        `  --push-endpoint=${endpoint} \\`,
        `  --push-auth-service-account=${account} \\`,
        `  --push-auth-token-audience=${source.audience}`,
      ].join('\n'),
    },
    {
      title: 'Send a test error',
      detail: 'Within about a minute it should appear under Errors, in Slack and as a GitHub issue. Press Test on the card first to check Slack and GitHub.',
      command: `gcloud logging write nexusmind-test '{"message":"NexusMind test error"}' --payload-type=json --severity=ERROR --project=${project}`,
    },
    {
      title: 'Also send Cloud Monitoring alerts',
      detail: 'Restarts, error rate and other alert policies can publish to the same topic. Add this channel to your alert policies afterwards.',
      optional: true,
      command: [
        `gcloud beta monitoring channels create --project=${project} --type=pubsub --display-name="NexusMind" \\`,
        `  --channel-labels=topic=projects/${project}/topics/nexusmind-errors`,
      ].join('\n'),
    },
  )
  return steps
}

/** Every step as one script, for "Copy all". */
export function setupCommands(source: Pick<MonitorSource, 'id' | 'service_account' | 'audience'>, gcpProject: string, endpoint: string): string {
  return setupSteps(source, gcpProject, endpoint)
    .map((step, index) => `# ${index + 1}. ${step.title}${step.optional ? ' (optional)' : ''}\n${step.command}`)
    .join('\n\n')
}

const SERVER_SLACK = 'server_slack_mcp'
const SERVER_GITHUB = 'server_gh_cli'
const SLACK_CHANNEL_ID = /^[CGD][A-Z0-9]{8,19}$/

/** The service account to suggest for a GCP project. */
export function suggestedAccount(gcpProject: string): string {
  return GCP_PROJECT.test(gcpProject) ? `nexusmind-push@${gcpProject}.iam.gserviceaccount.com` : ''
}

interface Draft {
  id: string | null
  name: string
  project: string
  /** Only used to fill the setup commands; not stored. */
  gcpProject: string
  serviceAccount: string
  slack: string
  slackChannel: string
  github: string
  repo: string
  throttle: string
  enabled: boolean
}

const EMPTY: Draft = { id: null, name: '', project: '', gcpProject: '', serviceAccount: '', slack: '', slackChannel: '', github: '', repo: '', throttle: '30', enabled: true }

function draftFrom(source: MonitorSource): Draft {
  return {
    id: source.id,
    name: source.name,
    project: source.project,
    gcpProject: source.service_account.match(/@([a-z][a-z0-9-]+)\.iam\.gserviceaccount\.com$/)?.[1] ?? '',
    serviceAccount: source.service_account,
    slack: source.slack_connector_id ?? '',
    slackChannel: source.slack_channel ?? '',
    github: source.github_connector_id ?? '',
    repo: source.github_repo ?? '',
    throttle: String(source.throttle_minutes),
    enabled: source.enabled,
  }
}

function sourceInput(draft: Draft): MonitorSourceInput {
  return {
    name: draft.name.trim(),
    project: draft.project.trim(),
    service_account: draft.serviceAccount.trim(),
    slack_connector_id: draft.slack || null,
    slack_channel: draft.slack === SERVER_SLACK ? draft.slackChannel.trim() : null,
    github_connector_id: draft.github || null,
    github_repo: draft.github ? draft.repo.trim() : null,
    throttle_minutes: Number(draft.throttle) || 30,
    enabled: draft.enabled,
  }
}

function message(error: unknown): string {
  return (error as { message?: string })?.message ?? 'unknown error'
}

const isGithub = (c: AutonomousAgentConnector) => c.kind === 'github_app' || (c.kind === 'target_secret' && c.metadata.purpose === 'github_token')

const TEST_LABEL: Record<string, string> = {
  not_authorized: 'server account not authorized for this organization',
  channel_missing: 'no channel set',
  ok: 'works',
  not_configured: 'not set up',
  connector_unavailable: 'connector unavailable',
  send_failed: 'message failed',
  repository_unreadable: 'repository not readable with this connector',
}

/**
 * Runtime error monitoring: GCP projects whose ERROR logs reach NexusMind
 * through a log sink and Pub/Sub push. A new error alerts Slack and opens a
 * GitHub issue; repeats only count, and alert again at most once per window.
 */
export default function Monitoring() {
  const { session } = useAuth()
  const allowed = (session?.user.permissions ?? []).includes('settings:write')
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState<Draft | null>(null)
  const [setupFor, setSetupFor] = useState<{ source: MonitorSource; gcpProject: string } | null>(null)
  const [tests, setTests] = useState<Record<string, MonitorTestResult>>({})
  const [sourceFilter, setSourceFilter] = useState('')

  const sources = useQuery({ queryKey: ['monitor-sources'], queryFn: () => client.listMonitorSources(), enabled: allowed })
  const groups = useQuery({
    queryKey: ['monitor-groups', sourceFilter],
    queryFn: () => client.listMonitorGroups(sourceFilter || undefined),
    refetchInterval: 30_000,
    enabled: allowed,
  })
  const options = useQuery({ queryKey: ['monitor-options'], queryFn: () => client.getMonitorOptions(), enabled: allowed })
  const connectors = useQuery({ queryKey: ['autonomous-connectors'], queryFn: () => client.listAutonomousAgentConnectors(), enabled: allowed })
  const usable = (connectors.data ?? []).filter(c => c.health !== 'revoked')
  const slackConnectors = usable.filter(c => c.kind === 'slack')
  const githubConnectors = usable.filter(isGithub)

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['monitor-sources'] })
  const save = useMutation({
    mutationFn: (current: Draft) =>
      current.id ? client.updateMonitorSource(current.id, sourceInput(current)) : client.createMonitorSource(sourceInput(current)),
    onSuccess: (source, current) => {
      setDraft(null)
      refresh()
      // A new source is useless until GCP is set up: show the commands right away.
      if (!current.id) setSetupFor({ source, gcpProject: current.gcpProject.trim() })
    },
  })
  const remove = useMutation({ mutationFn: (id: string) => client.deleteMonitorSource(id), onSuccess: refresh })
  const test = useMutation({
    mutationFn: (id: string) => client.testMonitorSource(id),
    onSuccess: (result, id) => setTests(current => ({ ...current, [id]: result })),
  })
  const setStatus = useMutation({
    mutationFn: ({ id, status }: { id: string; status: MonitorGroupStatus }) => client.setMonitorGroupStatus(id, status),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['monitor-groups'] }),
  })

  if (!allowed) {
    return <PermissionDenied title="Monitoring" permission="settings:write" what="connect GCP projects and see their errors" />
  }

  const slackLabel = (source: MonitorSource) =>
    !source.slack_connector_id ? 'Off'
      : source.slack_connector_id === SERVER_SLACK ? `Server Slack (MCP) · ${source.slack_channel ?? '—'}`
      : usable.find(c => c.id === source.slack_connector_id)?.name ?? 'Unavailable connector'
  const gcpProjectOf = (source: MonitorSource) => source.service_account.match(/@([a-z][a-z0-9-]+)\.iam\.gserviceaccount\.com$/)?.[1] ?? ''

  const sourceName = (id: string) => sources.data?.find(s => s.id === id)?.name ?? '—'
  const closeEditor = () => { setDraft(null); save.reset() }

  return (
    <div className="p-6 md:p-8 min-w-0 max-w-7xl mx-auto space-y-6">
      <PageHeader
        title="Monitoring"
        subtitle="Errors from your GCP services, grouped. A new error alerts Slack and opens a GitHub issue."
        action={<Button size="sm" leftIcon={<Plus className="h-4 w-4" />} onClick={() => setDraft(EMPTY)}>Connect GCP project</Button>}
      />

      {sources.isLoading && <CardListSkeleton count={2} height="h-36" />}
      {sources.isError && <InlineAlert onRetry={() => sources.refetch()}>Could not load the monitored projects.</InlineAlert>}
      {sources.data && sources.data.length === 0 && (
        <EmptyState
          icon={<Activity />}
          title="No GCP project connected"
          description="Connect a project, paste the setup commands into Cloud Shell, and every ERROR log from its services reaches you here."
        />
      )}
      {sources.data && sources.data.length > 0 && (
        <section aria-labelledby="sources-title" className="space-y-3">
          <SectionHeading id="sources-title" title="Connected projects" />
          <ul className="m-0 grid list-none gap-4 p-0 lg:grid-cols-2">
            {sources.data.map(source => {
              const result = tests[source.id]
              return (
                <li key={source.id} className={cn(PANEL_CLASS, 'flex flex-col gap-4')}>
                  <div className="flex items-start gap-3">
                    <div className="min-w-0 flex-1">
                      <p className="break-words text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{source.name}</p>
                      <p className="break-all text-[12px] text-text-tertiary">{source.service_account}</p>
                    </div>
                    {source.enabled ? <Badge role="none" size="sm" variant="success" dot>Active</Badge> : <Badge role="none" size="sm" variant="default">Paused</Badge>}
                  </div>
                  <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-sm">
                    <dt className="text-text-tertiary">Slack</dt>
                    <dd className="break-words text-text-secondary">{slackLabel(source)}</dd>
                    <dt className="text-text-tertiary">GitHub issues</dt>
                    <dd className="min-w-0 break-all">
                      {source.github_repo ? <span className="font-mono text-text-primary">{source.github_repo}</span> : <span className="text-text-secondary">Off</span>}
                      {source.github_connector_id === SERVER_GITHUB && <span className="text-text-tertiary"> · server gh</span>}
                    </dd>
                    <dt className="text-text-tertiary">Repeat alerts</dt>
                    <dd className="text-text-secondary">At most every {source.throttle_minutes} min per error</dd>
                    <dt className="text-text-tertiary">Project</dt>
                    <dd className="break-words text-text-secondary">{source.project}</dd>
                  </dl>
                  {result && (
                    <p role="status" className="text-[12px] text-text-secondary">
                      Test: Slack {TEST_LABEL[result.slack] ?? result.slack} · GitHub {TEST_LABEL[result.github] ?? result.github}
                    </p>
                  )}
                  <div className="mt-auto flex flex-wrap items-center gap-2 border-t border-border-secondary pt-4">
                    <Button size="sm" variant="secondary" leftIcon={<Terminal className="h-4 w-4" />} aria-label={`Setup commands for ${source.name}`} onClick={() => setSetupFor({ source, gcpProject: gcpProjectOf(source) })}>Setup</Button>
                    <Button size="sm" variant="secondary" aria-label={`Test ${source.name}`} disabled={test.isPending} onClick={() => test.mutate(source.id)}>Test</Button>
                    <Button size="sm" variant="secondary" aria-label={`Edit ${source.name}`} onClick={() => setDraft(draftFrom(source))}>Edit</Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="ml-auto"
                      aria-label={`Delete ${source.name}`}
                      leftIcon={<Trash2 className="h-4 w-4" />}
                      disabled={remove.isPending}
                      onClick={() => { if (window.confirm(`Stop monitoring ${source.name}? Its error groups are deleted; GitHub issues stay.`)) remove.mutate(source.id) }}
                    />
                  </div>
                </li>
              )
            })}
          </ul>
          {(remove.isError || test.isError) && <InlineAlert>That did not work: {message(remove.error ?? test.error)}.</InlineAlert>}
        </section>
      )}

      <section aria-labelledby="groups-title" className="space-y-3">
        <SectionHeading
          id="groups-title"
          title="Errors"
          description="One row per kind of error. Resolve one and it alerts again as a regression if it comes back."
          action={sources.data && sources.data.length > 1 && (
            <StyledSelect aria-label="Filter by project" className={SELECT_CLASS} value={sourceFilter} onChange={event => setSourceFilter(event.target.value)}>
              <option value="">All projects</option>
              {sources.data.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </StyledSelect>
          )}
        />
        {groups.isError && <InlineAlert onRetry={() => groups.refetch()}>Could not load the errors.</InlineAlert>}
        {groups.data && groups.data.length === 0 && <p className="text-sm text-text-secondary">No errors yet.</p>}
        {groups.data && groups.data.length > 0 && (
          <div className="overflow-x-auto rounded-xl border border-border-primary">
            <table className="admin-data-table w-full min-w-[48rem] text-sm" aria-label="Errors">
              <thead>
                <tr className="border-b border-border-secondary text-left text-[12px] text-text-tertiary">
                  <th scope="col" className="px-4 py-2.5 font-medium">Error</th>
                  <th scope="col" className="px-4 py-2.5 font-medium">Service</th>
                  <th scope="col" className="px-4 py-2.5 text-right font-medium">Count</th>
                  <th scope="col" className="px-4 py-2.5 font-medium">Last seen</th>
                  <th scope="col" className="px-4 py-2.5 font-medium">Links</th>
                  <th scope="col" className="px-4 py-2.5 font-medium">Status</th>
                </tr>
              </thead>
              <tbody>
                {groups.data.map(group => (
                  <GroupRow
                    key={group.id}
                    group={group}
                    source={sources.data?.find(s => s.id === group.source_id)}
                    sourceName={sourceName(group.source_id)}
                    busy={setStatus.isPending}
                    onStatus={status => setStatus.mutate({ id: group.id, status })}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
        {setStatus.isError && <InlineAlert>The status was not saved: {message(setStatus.error)}.</InlineAlert>}
      </section>

      <SourceEditor
        draft={draft}
        setDraft={setDraft}
        slackConnectors={slackConnectors}
        githubConnectors={githubConnectors}
        serverSlack={options.data?.server_slack_mcp ?? false}
        serverGithub={options.data?.server_github ?? false}
        saving={save.isPending}
        error={save.isError ? message(save.error) : null}
        onSave={() => draft && save.mutate(draft)}
        onClose={closeEditor}
      />
      <SetupModal setup={setupFor} onClose={() => setSetupFor(null)} />
    </div>
  )
}

function GroupRow({ group, source, sourceName, busy, onStatus }: {
  group: MonitorGroup
  source: MonitorSource | undefined
  sourceName: string
  busy: boolean
  onStatus: (status: MonitorGroupStatus) => void
}) {
  const issueUrl = source?.github_repo && group.issue_number ? `https://github.com/${source.github_repo}/issues/${group.issue_number}` : null
  return (
    <tr className="border-b border-border-secondary last:border-0 align-top">
      <td className="max-w-[28rem] px-4 py-3">
        <p className="break-words font-medium text-text-primary">{group.title}</p>
        <p className="text-[12px] text-text-tertiary">{sourceName} · {group.severity}</p>
      </td>
      <td className="px-4 py-3 font-mono text-[12px] text-text-secondary">{group.service}</td>
      <td className="px-4 py-3 text-right tabular-nums text-text-primary">{group.count}</td>
      <td className="px-4 py-3 text-text-secondary">{when(group.last_seen)}</td>
      <td className="px-4 py-3">
        <div className="flex flex-col gap-1">
          {issueUrl && <a className={LINK_CLASS} href={issueUrl} target="_blank" rel="noreferrer">Issue #{group.issue_number}</a>}
          {group.log_url && <a className={LINK_CLASS} href={group.log_url} target="_blank" rel="noreferrer">Logs <ExternalLink className="inline h-3 w-3" aria-hidden="true" /></a>}
        </div>
      </td>
      <td className="px-4 py-3">
        <div className="flex items-center gap-2">
          <Badge role="none" size="sm" variant={group.status === 'open' ? 'error' : group.status === 'resolved' ? 'success' : 'default'}>
            {group.status === 'open' ? 'Open' : group.status === 'resolved' ? 'Resolved' : 'Muted'}
          </Badge>
          {group.status === 'open' ? (
            <>
              <Button size="sm" variant="ghost" disabled={busy} aria-label={`Resolve ${group.title}`} onClick={() => onStatus('resolved')}>Resolve</Button>
              <Button size="sm" variant="ghost" disabled={busy} aria-label={`Mute ${group.title}`} onClick={() => onStatus('muted')}>Mute</Button>
            </>
          ) : (
            <Button size="sm" variant="ghost" disabled={busy} aria-label={`Reopen ${group.title}`} onClick={() => onStatus('open')}>Reopen</Button>
          )}
        </div>
      </td>
    </tr>
  )
}

function SourceEditor({ draft, setDraft, slackConnectors, githubConnectors, serverSlack, serverGithub, saving, error, onSave, onClose }: {
  draft: Draft | null
  setDraft: (draft: Draft | null) => void
  slackConnectors: AutonomousAgentConnector[]
  githubConnectors: AutonomousAgentConnector[]
  serverSlack: boolean
  serverGithub: boolean
  saving: boolean
  error: string | null
  onSave: () => void
  onClose: () => void
}) {
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => draft && setDraft({ ...draft, [key]: value })
  const suggestion = draft ? suggestedAccount(draft.gcpProject.trim()) : ''
  const channelOk = !draft || draft.slack !== SERVER_SLACK || SLACK_CHANNEL_ID.test(draft.slackChannel.trim())
  const complete = !!draft && !!draft.name.trim() && !!draft.project.trim() && !!draft.serviceAccount.trim()
    && (!draft.github || !!draft.repo.trim()) && channelOk
  return (
    <Modal open={draft !== null} onOpenChange={value => { if (!value) onClose() }} size="xl" position="right">
      {draft && (
        <div className="flex min-h-full flex-col">
          <ModalHeader className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <ModalTitle>{draft.id ? `Edit ${draft.name}` : 'Connect a GCP project'}</ModalTitle>
              <ModalDescription>Log text is untrusted: it is redacted and shown as data in Slack and GitHub.</ModalDescription>
            </div>
            <Button size="sm" variant="ghost" aria-label="Close" leftIcon={<X className="h-4 w-4" />} onClick={onClose} />
          </ModalHeader>
          <ModalContent className="flex-1 space-y-6">
            {!draft.id && (
              <section aria-labelledby="monitor-how-title" className="rounded-lg border border-border-primary bg-foreground/[0.03] p-4">
                <h3 id="monitor-how-title" className="text-sm font-semibold text-text-primary">How it works</h3>
                <ol className="mt-2 list-decimal space-y-1.5 pl-5 text-[13px] leading-normal text-text-secondary">
                  <li>Enter the GCP project ID. The push service account is filled in for you.</li>
                  <li>Choose where alerts go: Slack and, optionally, a repository for GitHub issues.</li>
                  <li>Press <strong>Connect</strong>. The setup steps open with every <code className="font-mono">gcloud</code> command filled in.</li>
                  <li>Run them in Cloud Shell, in order. You only do this once per project, and no service needs code changes.</li>
                  <li>Press <strong>Test</strong> on the card, then send the test error from the last step.</li>
                </ol>
              </section>
            )}
            <FieldGroup title="Source">
              <Field id="monitor-name" label="Name">
                <Input id="monitor-name" value={draft.name} placeholder="Kasymir beta" onChange={e => set('name', e.target.value)} />
              </Field>
              <Field id="monitor-project" label="NexusMind project">
                <Input id="monitor-project" value={draft.project} placeholder="kasymir" onChange={e => set('project', e.target.value)} />
              </Field>
              <Field id="monitor-gcp" label="GCP project ID" hint="Fills the service account and the setup commands. Find it in the GCP console project picker.">
                <Input
                  id="monitor-gcp"
                  className="font-mono"
                  value={draft.gcpProject}
                  placeholder="my-project-123"
                  onChange={e => {
                    const gcpProject = e.target.value
                    const previous = suggestedAccount(draft.gcpProject.trim())
                    // Follow the project while the account is still the suggestion (or empty).
                    const follow = !draft.serviceAccount || draft.serviceAccount === previous
                    setDraft({ ...draft, gcpProject, serviceAccount: follow ? suggestedAccount(gcpProject.trim()) : draft.serviceAccount })
                  }}
                />
              </Field>
              <Field
                id="monitor-sa"
                label="Push service account"
                hint="Pub/Sub signs each push as this account; only pushes signed by it are accepted. It needs no roles and no key. The setup steps create it."
              >
                <Input id="monitor-sa" value={draft.serviceAccount} placeholder={suggestion || 'nexusmind-push@my-project-123.iam.gserviceaccount.com'} onChange={e => set('serviceAccount', e.target.value)} />
              </Field>
            </FieldGroup>
            <FieldGroup title="Where alerts go">
              <Field
                id="monitor-slack"
                label="Slack"
                hint={slackConnectors.length === 0 && !serverSlack ? 'Add a Slack connector under Factory settings first.' : undefined}
              >
                <StyledSelect id="monitor-slack" className={SELECT_CLASS} value={draft.slack} onChange={e => set('slack', e.target.value)}>
                  <option value="">No Slack alerts</option>
                  {serverSlack && <option value={SERVER_SLACK}>Server Slack (Claude&apos;s Slack MCP)</option>}
                  {slackConnectors.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                </StyledSelect>
              </Field>
              {draft.slack === SERVER_SLACK && (
                <Field
                  id="monitor-slack-channel"
                  label="Slack channel ID"
                  hint="In Slack, open the channel, click its name, and copy the ID at the bottom (C…). Alerts post as the account the server's Slack MCP is signed in with."
                >
                  <Input id="monitor-slack-channel" className="font-mono" value={draft.slackChannel} placeholder="C0123ABCDE" onChange={e => set('slackChannel', e.target.value.trim().toUpperCase())} />
                </Field>
              )}
              <Field id="monitor-github" label="GitHub issues" hint={serverGithub ? 'The server\'s gh account, or a GitHub App or token connector of this organization.' : 'A GitHub App or token connector of this organization.'}>
                <StyledSelect id="monitor-github" className={SELECT_CLASS} value={draft.github} onChange={e => set('github', e.target.value)}>
                  <option value="">No GitHub issues</option>
                  {serverGithub && <option value={SERVER_GITHUB}>Server GitHub (authenticated gh)</option>}
                  {githubConnectors.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                </StyledSelect>
              </Field>
              {draft.github && (
                <Field id="monitor-repo" label="Repository for issues">
                  <Input id="monitor-repo" className="font-mono" value={draft.repo} placeholder="owner/repo" onChange={e => set('repo', e.target.value)} />
                </Field>
              )}
              <Field id="monitor-throttle" label="Alert again for the same error at most every (minutes)">
                <Input id="monitor-throttle" type="number" min={1} max={1440} value={draft.throttle} onChange={e => set('throttle', e.target.value)} />
              </Field>
              <label className="flex items-center gap-2 text-sm text-text-secondary">
                <input type="checkbox" checked={draft.enabled} onChange={e => set('enabled', e.target.checked)} />
                Active
              </label>
            </FieldGroup>
            {error && <InlineAlert>Not saved: {error}.</InlineAlert>}
          </ModalContent>
          <ModalFooter>
            <Button variant="secondary" onClick={onClose}>Cancel</Button>
            <Button disabled={!complete || saving} onClick={onSave}>{saving ? 'Saving…' : draft.id ? 'Save' : 'Connect'}</Button>
          </ModalFooter>
        </div>
      )}
    </Modal>
  )
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    } catch {
      setCopied(false)
    }
  }
  return (
    <Button size="sm" variant="secondary" aria-label={label} leftIcon={<Copy className="h-4 w-4" />} onClick={copy}>
      {copied ? 'Copied' : 'Copy'}
    </Button>
  )
}

function SetupModal({ setup, onClose }: { setup: { source: MonitorSource; gcpProject: string } | null; onClose: () => void }) {
  const [edited, setEdited] = useState<string | null>(null)
  const source = setup?.source ?? null
  const gcpProject = (edited ?? setup?.gcpProject ?? '').trim()
  const endpoint = source ? pushEndpoint(source.id) : ''
  const steps = source ? setupSteps(source, gcpProject, endpoint) : []
  const close = () => { setEdited(null); onClose() }
  return (
    <Modal open={source !== null} onOpenChange={value => { if (!value) close() }} size="xl" position="right">
      {source && (
        <div className="flex min-h-full flex-col">
          <ModalHeader className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <ModalTitle>Set up {source.name} in GCP</ModalTitle>
              <ModalDescription>Run these once, in order, in Cloud Shell. Every service in the project is covered, with no code changes.</ModalDescription>
            </div>
            <Button size="sm" variant="ghost" aria-label="Close" leftIcon={<X className="h-4 w-4" />} onClick={close} />
          </ModalHeader>
          <ModalContent className="flex-1 space-y-4">
            <Field id="monitor-gcp-project" label="GCP project ID">
              <Input id="monitor-gcp-project" className="font-mono" value={edited ?? setup?.gcpProject ?? ''} placeholder="my-project-123" onChange={e => setEdited(e.target.value)} />
            </Field>
            <ol aria-label="Setup steps" className="m-0 list-none space-y-4 p-0">
              {steps.map((step, index) => (
                <li key={step.title} className="rounded-lg border border-border-primary p-4">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="text-sm font-semibold text-text-primary">
                        {index + 1}. {step.title}
                        {step.optional && <span className="ml-2 font-normal text-text-tertiary">optional</span>}
                      </p>
                      <p className="mt-0.5 text-[12px] leading-normal text-text-secondary">{step.detail}</p>
                    </div>
                    <CopyButton text={step.command} label={`Copy step ${index + 1}`} />
                  </div>
                  <pre className="mt-3 overflow-x-auto whitespace-pre rounded-md bg-foreground/[0.04] p-3 font-mono text-[12px] leading-relaxed text-text-primary">{step.command}</pre>
                </li>
              ))}
            </ol>
            <p className="text-[12px] text-text-tertiary">
              A command that says <em>already exists</em> is fine: move on to the next one.
            </p>
          </ModalContent>
          <ModalFooter>
            <CopyButton text={source ? setupCommands(source, gcpProject, endpoint) : ''} label="Copy all steps" />
            <Button onClick={close}>Done</Button>
          </ModalFooter>
        </div>
      )}
    </Modal>
  )
}
