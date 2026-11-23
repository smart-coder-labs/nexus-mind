import { StyledSelect } from '@/components/ui/Select/StyledSelect'
import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Activity, Copy, ExternalLink, Pause, Play, Plus, Terminal, Trash2, X } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { Input } from '../components/ui/Input'
import { Modal, ModalContent, ModalDescription, ModalFooter, ModalHeader, ModalTitle } from '../components/ui/Modal'
import { cn } from '../lib/utils'
import type { AutonomousAgentConnector, AutonomousAgentDefinition, MonitorAuditExecutor, MonitorGroup, MonitorGroupStatus, MonitorProvider, MonitorSource, MonitorSourceInput, MonitorTestResult } from '../types'
import { TaskExecutorSelect } from './factory/agents/TaskExecutorSelect'
import { CardListSkeleton, Field, FieldGroup, InlineAlert, LINK_CLASS, PageHeader, PANEL_CLASS, PermissionDenied, SectionHeading, SELECT_CLASS } from './factory/govern/ui'

const GCP_PROJECT = /^[a-z][a-z0-9-]{4,28}[a-z0-9]$/

/** The public URL Pub/Sub pushes to for one source. */
export function pushEndpoint(sourceId: string, apiBase = import.meta.env.VITE_API_URL || window.location.origin): string {
  return `${apiBase.replace(/\/+$/, '')}/v1/monitor/gcp/${sourceId}`
}

/** The public URL a Supabase or Vercel log drain posts to for one source. */
export function drainEndpoint(sourceId: string, apiBase = import.meta.env.VITE_API_URL || window.location.origin): string {
  return `${apiBase.replace(/\/+$/, '')}/v1/monitor/drain/${sourceId}`
}

const PROVIDER_LABEL: Record<MonitorProvider, string> = { gcp: 'GCP', supabase: 'Supabase', vercel: 'Vercel' }
const PROVIDER_REF_LABEL: Record<Exclude<MonitorProvider, 'gcp'>, string> = { supabase: 'Supabase project ref', vercel: 'Vercel team slug' }

/**
 * Where to add the drain in Supabase or Vercel, as numbered steps. The token
 * is only known right after creating the source.
 */
export function drainSteps(provider: Exclude<MonitorProvider, 'gcp'>, endpoint: string, token: string | undefined): SetupStep[] {
  const header = `Authorization: Bearer ${token ?? 'TOKEN_SHOWN_WHEN_CONNECTED'}`
  const where = provider === 'supabase'
    ? {
        title: 'Add a log drain in Supabase',
        detail: 'Needs the Pro plan or above. Project Settings › Log Drains › Add destination › Custom Endpoint. Paste the URL, choose HTTP/1.1, and add the header below. Gzip on or off both work.',
      }
    : {
        title: 'Add a log drain in Vercel',
        detail: 'Needs the Pro plan or above. Team Settings › Drains › Add Drain › Logs. Pick the projects, the sources (Functions, Edge, Builds…) and the environments, then Custom Endpoint with format JSON. Paste the URL and add the header below as a custom header.',
      }
  return [
    { ...where, command: endpoint },
    {
      title: 'Add the authorization header',
      detail: token
        ? 'NexusMind only accepts deliveries with this header. Copy it now: the token is shown only once.'
        : 'The token was shown only when the source was connected. If you lost it, delete the source and connect it again.',
      command: header,
    },
    {
      title: 'Send a test',
      detail: provider === 'supabase'
        ? 'Only errors become alerts: Postgres ERROR/FATAL, 5xx responses and error-level logs. Trigger one (for example a failing query) and it appears under Errors within a minute.'
        : 'Use Test in the drain settings: it must answer 200. Only errors become alerts: error/fatal logs, 5xx responses and crashed functions.',
      command: provider === 'supabase' ? 'select * from nexusmind_missing_table;' : endpoint,
    },
  ]
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

const EXECUTOR_LABEL: Record<MonitorAuditExecutor, string> = { claude: 'Claude', codex: 'Codex', pi: 'Pi', nexus: 'Nexus' }

const SERVER_SLACK = 'server_slack_mcp'
const SERVER_GITHUB = 'server_gh_cli'
/** Select value: paste a token, stored as an org connector when saving. */
const NEW_GITHUB_TOKEN = '__new_github_token'
const SLACK_CHANNEL_ID = /^[CGD][A-Z0-9]{8,19}$/

/** The service account to suggest for a GCP project. */
export function suggestedAccount(gcpProject: string): string {
  return GCP_PROJECT.test(gcpProject) ? `nexusmind-push@${gcpProject}.iam.gserviceaccount.com` : ''
}

interface Draft {
  id: string | null
  provider: MonitorProvider
  providerRef: string
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
  /** Empty = no error audit. */
  auditExecutor: MonitorAuditExecutor | ''
  auditModel: string
  /** `new` or an existing `error_audit` agent id. */
  auditAgent: string
  auditLimit: string
  autofix: boolean
  /** '' = none, `new` or an existing `github_issue_resolver` id. */
  fixAgent: string
  fixLimit: string
  /** This project's own (managed) agents, left out of the "existing" lists. */
  ownAgents: string[]
  /** Only with `github = NEW_GITHUB_TOKEN`; sent to the connector, never to the source. */
  newToken: string
}

const EMPTY: Draft = { id: null, provider: 'gcp', providerRef: '', name: '', project: '', gcpProject: '', serviceAccount: '', slack: '', slackChannel: '', github: '', repo: '', throttle: '30', enabled: true, auditExecutor: '', auditModel: '', auditAgent: 'new', auditLimit: '10', autofix: false, fixAgent: '', fixLimit: '10', ownAgents: [], newToken: '' }

function draftFrom(source: MonitorSource): Draft {
  return {
    id: source.id,
    provider: source.provider,
    providerRef: source.provider_ref ?? '',
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
    auditExecutor: source.audit_executor ?? '',
    auditModel: source.audit_model ?? '',
    auditAgent: source.audit_agent_id && !source.audit_agent_managed ? source.audit_agent_id : 'new',
    auditLimit: String(source.audit_daily_limit),
    autofix: source.autofix,
    fixAgent: !source.autofix_agent_id ? '' : source.autofix_agent_managed ? 'new' : source.autofix_agent_id,
    fixLimit: String(source.autofix_daily_limit),
    ownAgents: [source.audit_agent_managed && source.audit_agent_id, source.autofix_agent_managed && source.autofix_agent_id].filter((id): id is string => !!id),
    newToken: '',
  }
}

function sourceInput(draft: Draft): MonitorSourceInput {
  const gcp = draft.provider === 'gcp'
  return {
    provider: draft.provider,
    provider_ref: !gcp && draft.providerRef.trim() ? draft.providerRef.trim() : null,
    name: draft.name.trim(),
    project: draft.project.trim(),
    service_account: gcp ? draft.serviceAccount.trim() : '',
    slack_connector_id: draft.slack || null,
    slack_channel: draft.slack === SERVER_SLACK ? draft.slackChannel.trim() : null,
    github_connector_id: draft.github || null,
    github_repo: draft.github ? draft.repo.trim() : null,
    throttle_minutes: Number(draft.throttle) || 30,
    enabled: draft.enabled,
    audit_executor: draft.github && draft.auditExecutor ? draft.auditExecutor : null,
    audit_model: draft.github && draft.auditExecutor && draft.auditAgent === 'new' && draft.auditModel.trim() ? draft.auditModel.trim() : null,
    audit_agent: draft.github && draft.auditExecutor ? draft.auditAgent : null,
    autofix_agent: draft.github && draft.fixAgent ? draft.fixAgent : null,
    autofix: !!draft.github && !!draft.fixAgent && draft.autofix,
    audit_daily_limit: Number(draft.auditLimit) || 10,
    autofix_daily_limit: Number(draft.fixLimit) || 10,
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
 * through a log sink and Pub/Sub push, and Supabase/Vercel projects through a
 * log drain with a bearer token. A new error alerts Slack and opens a
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
  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents(), enabled: allowed })
  const enabledAgents = (agents.data ?? []).filter(a => a.status === 'enabled')
  const agentName = (id: string | null) => agents.data?.find(a => a.id === id)?.name ?? 'Chosen agent'
  const usable = (connectors.data ?? []).filter(c => c.health !== 'revoked')
  const slackConnectors = usable.filter(c => c.kind === 'slack')
  const githubConnectors = usable.filter(isGithub)

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['monitor-sources'] })
  const save = useMutation({
    mutationFn: async (current: Draft) => {
      if (current.github === NEW_GITHUB_TOKEN) {
        const connector = await client.putAutonomousAgentConnector({
          kind: 'target_secret',
          // A unique name: the connector upsert by name must never overwrite another token.
          name: `monitor: ${current.name.trim()} · ${Date.now().toString(36)}`,
          secret: current.newToken.trim(),
          metadata: { purpose: 'github_token' },
          scopes: ['target:use'],
        })
        current = { ...current, github: connector.id, newToken: '' }
        // A retry after a failed save reuses this connector instead of making another.
        setDraft(latest => latest && { ...latest, github: connector.id, newToken: '' })
        queryClient.invalidateQueries({ queryKey: ['autonomous-connectors'] })
      }
      return current.id ? client.updateMonitorSource(current.id, sourceInput(current)) : client.createMonitorSource(sourceInput(current))
    },
    onSuccess: (source, current) => {
      setDraft(null)
      refresh()
      // A new source is useless until GCP or the drain is set up: show the steps right away
      // (a drain's token comes only in this response).
      if (!current.id) setSetupFor({ source, gcpProject: current.gcpProject.trim() })
    },
  })
  const remove = useMutation({ mutationFn: (id: string) => client.deleteMonitorSource(id), onSuccess: refresh })
  const toggle = useMutation({
    mutationFn: (source: MonitorSource) => client.setMonitorSourceEnabled(source.id, !source.enabled),
    onSuccess: refresh,
  })
  const test = useMutation({
    mutationFn: (id: string) => client.testMonitorSource(id),
    onSuccess: (result, id) => setTests(current => ({ ...current, [id]: result })),
  })
  const fix = useMutation({
    mutationFn: (id: string) => client.fixMonitorGroup(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['monitor-groups'] }),
  })
  const setStatus = useMutation({
    mutationFn: ({ id, status }: { id: string; status: MonitorGroupStatus }) => client.setMonitorGroupStatus(id, status),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['monitor-groups'] }),
  })

  if (!allowed) {
    return <PermissionDenied title="Monitoring" permission="settings:write" what="connect GCP, Supabase and Vercel projects and see their errors" />
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
        subtitle="Errors from your GCP, Supabase and Vercel services, grouped. A new error alerts Slack and opens a GitHub issue."
        action={<Button size="sm" leftIcon={<Plus className="h-4 w-4" />} onClick={() => setDraft(EMPTY)}>Connect project</Button>}
      />

      {sources.isLoading && <CardListSkeleton count={2} height="h-36" />}
      {sources.isError && <InlineAlert onRetry={() => sources.refetch()}>Could not load the monitored projects.</InlineAlert>}
      {sources.data && sources.data.length === 0 && (
        <EmptyState
          icon={<Activity />}
          title="No project connected"
          description="Connect a GCP project with a few Cloud Shell commands, or a Supabase or Vercel project with a log drain, and its errors reach you here."
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
                      <p className="break-all text-[12px] text-text-tertiary">
                        {source.provider === 'gcp' ? source.service_account : `${PROVIDER_LABEL[source.provider]} log drain${source.provider_ref ? ` · ${source.provider_ref}` : ''}`}
                      </p>
                    </div>
                    <Badge role="none" size="sm" variant="default">{PROVIDER_LABEL[source.provider]}</Badge>
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
                    <dt className="text-text-tertiary">Error audit</dt>
                    <dd className="text-text-secondary">{source.audit_executor ? `${source.audit_agent_managed ? EXECUTOR_LABEL[source.audit_executor] : agentName(source.audit_agent_id)}${source.audit_model ? ` · ${source.audit_model}` : ''} · up to ${source.audit_daily_limit} a day` : 'Off'}</dd>
                    <dt className="text-text-tertiary">Auto-fix</dt>
                    <dd className="text-text-secondary">
                      {!source.autofix_agent_id ? 'Off'
                        : `${source.autofix_agent_managed ? 'Managed agent' : agentName(source.autofix_agent_id)} · ${source.autofix ? `every new issue, up to ${source.autofix_daily_limit} a day` : 'from the Resolve issue button'}`}
                    </dd>
                    <dt className="text-text-tertiary">Repeat alerts</dt>
                    <dd className="text-text-secondary">At most every {source.throttle_minutes} min per error</dd>
                    <dt className="text-text-tertiary">Project</dt>
                    <dd className="break-words text-text-secondary">{source.project}</dd>
                  </dl>
                  {!source.enabled && (
                    <p role="status" className="text-[12px] text-text-secondary">Paused: errors from this project are dropped, with no alerts or issues, until you resume.</p>
                  )}
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
                      variant="secondary"
                      aria-label={`${source.enabled ? 'Pause' : 'Resume'} ${source.name}`}
                      leftIcon={source.enabled ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                      disabled={toggle.isPending}
                      onClick={() => toggle.mutate(source)}
                    >
                      {source.enabled ? 'Pause' : 'Resume'}
                    </Button>
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
          {(remove.isError || test.isError || toggle.isError) && <InlineAlert>That did not work: {message(remove.error ?? test.error ?? toggle.error)}.</InlineAlert>}
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
          <div className="overflow-hidden rounded-xl border border-border-primary bg-muted font-mono text-[12px] leading-relaxed">
            <div className="flex items-center gap-2 border-b border-border-secondary px-4 py-2 text-text-tertiary">
              <Terminal className="h-3.5 w-3.5" aria-hidden="true" />
              <span>monitoring — {sourceFilter ? sourceName(sourceFilter) : 'all projects'} — {groups.data.length} errors</span>
            </div>
            <ul aria-label="Errors" className="m-0 list-none p-0">
              {groups.data.map(group => (
                <ErrorLine
                  key={group.id}
                  group={group}
                  source={sources.data?.find(s => s.id === group.source_id)}
                  sourceName={sourceName(group.source_id)}
                  busy={setStatus.isPending}
                  onStatus={status => setStatus.mutate({ id: group.id, status })}
                  fixing={fix.isPending && fix.variables === group.id}
                  onFix={() => fix.mutate(group.id)}
                />
              ))}
            </ul>
          </div>
        )}
        {setStatus.isError && <InlineAlert>The status was not saved: {message(setStatus.error)}.</InlineAlert>}
        {fix.isError && <InlineAlert>The fix did not start: {message(fix.error)}.</InlineAlert>}
      </section>

      <SourceEditor
        draft={draft}
        setDraft={setDraft}
        slackConnectors={slackConnectors}
        githubConnectors={githubConnectors}
        serverSlack={options.data?.server_slack_mcp ?? false}
        serverGithub={options.data?.server_github ?? false}
        auditAgents={enabledAgents.filter(a => a.template_key === 'error_audit')}
        resolverAgents={enabledAgents.filter(a => a.template_key === 'github_issue_resolver')}
        saving={save.isPending}
        error={save.isError ? message(save.error) : null}
        onSave={() => draft && save.mutate(draft)}
        onClose={closeEditor}
      />
      <SetupModal setup={setupFor} onClose={() => setSetupFor(null)} />
    </div>
  )
}

/** `22:42:57` today, `Nov 19 22:42` before. */
function clock(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  const today = new Date().toDateString() === date.toDateString()
  return today
    ? date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
    : date.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })
}

const SEVERITY_CLASS: Record<string, string> = {
  CRITICAL: 'text-status-error',
  ALERT: 'text-status-error',
  EMERGENCY: 'text-status-error',
  ERROR: 'text-status-error',
}

/** One error as a terminal line; opening it shows every detail. */
function ErrorLine({ group, source, sourceName, busy, onStatus, fixing, onFix }: {
  group: MonitorGroup
  source: MonitorSource | undefined
  sourceName: string
  busy: boolean
  onStatus: (status: MonitorGroupStatus) => void
  fixing: boolean
  onFix: () => void
}) {
  const issueRepo = group.issue_repo ?? source?.github_repo
  const issueUrl = issueRepo && group.issue_number ? `https://github.com/${issueRepo}/issues/${group.issue_number}` : null
  let sample: { message?: string; stack?: string | null } = {}
  try { sample = JSON.parse(group.sample) } catch { sample = {} }
  const status = group.status === 'open' ? 'open' : group.status === 'resolved' ? 'resolved' : 'muted'
  const statusClass = group.status === 'open' ? 'text-status-error' : group.status === 'resolved' ? 'text-status-success' : 'text-text-tertiary'
  const audit = group.audit_status === 'pending' ? 'auditing…'
    : group.audit_status === 'done' ? (group.audit_summary ?? 'done')
    : group.audit_status === 'failed' ? 'did not reach a conclusion' : null
  return (
    <li className="border-b border-border-secondary last:border-0">
      <details className="group/line">
        <summary className="flex cursor-pointer list-none flex-wrap items-baseline gap-x-3 gap-y-0.5 px-4 py-1.5 hover:bg-foreground/[0.04] focus-visible:bg-foreground/[0.06] focus-visible:outline-none [&::-webkit-details-marker]:hidden">
          <span aria-hidden="true" className="text-text-tertiary transition-transform group-open/line:rotate-90">›</span>
          <span className="tabular-nums text-text-tertiary">{clock(group.last_seen)}</span>
          <span className={cn('font-semibold', SEVERITY_CLASS[group.severity] ?? 'text-status-warning')}>{group.severity}</span>
          <span className="text-text-secondary">{group.service}</span>
          <span className="min-w-0 flex-1 break-words text-text-primary">{group.title}</span>
          <span className="tabular-nums text-text-secondary">×{group.count}</span>
          {group.audit_status === 'pending' && <span className="text-status-warning">audit…</span>}
          {group.fix_status === 'pending' && <span className="text-status-warning">fixing…</span>}
          {group.fix_status === 'pr_open' && <span className="text-status-success">PR</span>}
          <span className={statusClass}>[{status}]</span>
        </summary>
        <dl className="m-0 grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-3 gap-y-1 border-t border-border-secondary bg-background-primary/40 px-4 py-3 pl-9">
          <dt className="text-text-tertiary">source</dt>
          <dd className="m-0 text-text-secondary">{sourceName}</dd>
          <dt className="text-text-tertiary">seen</dt>
          <dd className="m-0 text-text-secondary">{group.count}× · first {group.first_seen.replace('T', ' ').slice(0, 19)} · last {group.last_seen.replace('T', ' ').slice(0, 19)}</dd>
          <dt className="text-text-tertiary">message</dt>
          <dd className="m-0"><pre className="m-0 whitespace-pre-wrap break-words text-text-primary">{sample.message || group.title}</pre></dd>
          {sample.stack && (
            <>
              <dt className="text-text-tertiary">stack</dt>
              <dd className="m-0"><pre className="m-0 max-h-72 overflow-auto whitespace-pre text-text-secondary">{sample.stack}</pre></dd>
            </>
          )}
          {audit && (
            <>
              <dt className="text-text-tertiary">audit</dt>
              <dd className="m-0 break-words text-text-secondary">
                {audit}
                {group.audit_run_id && <> · <a className={LINK_CLASS} href={`/factory/runs?agent_run=${encodeURIComponent(group.audit_run_id)}`}>audit run</a></>}
              </dd>
            </>
          )}
          <dt className="text-text-tertiary">links</dt>
          <dd className="m-0 flex flex-wrap gap-x-4">
            {issueUrl ? (
              <a className={LINK_CLASS} href={issueUrl} target="_blank" rel="noreferrer">
                {group.issue_repo ? `${group.issue_repo.split('/')[1]} #${group.issue_number}` : `Issue #${group.issue_number}`}
              </a>
            ) : <span className="text-text-tertiary">no issue yet</span>}
            {group.fix_pr_url && <a className={LINK_CLASS} href={group.fix_pr_url} target="_blank" rel="noreferrer">Fix PR</a>}
            {group.fix_status === 'failed' && group.fix_run_id && <a className={LINK_CLASS} href={`/factory/runs?agent_run=${encodeURIComponent(group.fix_run_id)}`}>fix run (no PR)</a>}
            {group.log_url && <a className={LINK_CLASS} href={group.log_url} target="_blank" rel="noreferrer">Logs <ExternalLink className="inline h-3 w-3" aria-hidden="true" /></a>}
          </dd>
          <dt className="text-text-tertiary">status</dt>
          <dd className="m-0 flex flex-wrap items-center gap-2">
            <span className={statusClass}>{status}</span>
            {source?.autofix_agent_id && group.fix_status !== 'pending' && (
              <Button size="sm" variant="secondary" disabled={fixing} aria-label={`Open a fix PR for ${group.title}`} onClick={onFix}>
                {fixing ? 'Starting…' : group.issue_number ? 'Resolve issue' : 'Open issue and resolve'}
              </Button>
            )}
            {group.status === 'open' ? (
              <>
                <Button size="sm" variant="ghost" disabled={busy} aria-label={`Resolve ${group.title}`} onClick={() => onStatus('resolved')}>Resolve</Button>
                <Button size="sm" variant="ghost" disabled={busy} aria-label={`Mute ${group.title}`} onClick={() => onStatus('muted')}>Mute</Button>
              </>
            ) : (
              <Button size="sm" variant="ghost" disabled={busy} aria-label={`Reopen ${group.title}`} onClick={() => onStatus('open')}>Reopen</Button>
            )}
          </dd>
        </dl>
      </details>
    </li>
  )
}

function SourceEditor({ draft, setDraft, slackConnectors, githubConnectors, serverSlack, serverGithub, auditAgents, resolverAgents, saving, error, onSave, onClose }: {
  draft: Draft | null
  setDraft: (draft: Draft | null) => void
  slackConnectors: AutonomousAgentConnector[]
  githubConnectors: AutonomousAgentConnector[]
  serverSlack: boolean
  serverGithub: boolean
  auditAgents: AutonomousAgentDefinition[]
  resolverAgents: AutonomousAgentDefinition[]
  saving: boolean
  error: string | null
  onSave: () => void
  onClose: () => void
}) {
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => draft && setDraft({ ...draft, [key]: value })
  const suggestion = draft ? suggestedAccount(draft.gcpProject.trim()) : ''
  const channelOk = !draft || draft.slack !== SERVER_SLACK || SLACK_CHANNEL_ID.test(draft.slackChannel.trim())
  const gcp = draft?.provider === 'gcp'
  const complete = !!draft && !!draft.name.trim() && !!draft.project.trim() && (!gcp || !!draft.serviceAccount.trim())
    && (!draft.github || !!draft.repo.trim()) && channelOk
    && (draft.github !== NEW_GITHUB_TOKEN || !!draft.newToken.trim())
  return (
    <Modal open={draft !== null} onOpenChange={value => { if (!value) onClose() }} size="xl" position="right">
      {draft && (
        <div className="flex min-h-full flex-col">
          <ModalHeader className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <ModalTitle>{draft.id ? `Edit ${draft.name}` : 'Connect a project'}</ModalTitle>
              <ModalDescription>Log text is untrusted: it is redacted and shown as data in Slack and GitHub.</ModalDescription>
            </div>
            <Button size="sm" variant="ghost" aria-label="Close" leftIcon={<X className="h-4 w-4" />} onClick={onClose} />
          </ModalHeader>
          <ModalContent className="flex-1 space-y-6">
            {!draft.id && (
              <Field id="monitor-provider" label="Errors come from" hint="Fixed once connected.">
                <StyledSelect id="monitor-provider" className={SELECT_CLASS} value={draft.provider} onChange={e => set('provider', e.target.value as MonitorProvider)}>
                  <option value="gcp">GCP (Cloud Run, GKE, Cloud SQL…)</option>
                  <option value="supabase">Supabase (log drain)</option>
                  <option value="vercel">Vercel (log drain)</option>
                </StyledSelect>
              </Field>
            )}
            {!draft.id && !gcp && (
              <section aria-labelledby="monitor-how-title" className="rounded-lg border border-border-primary bg-foreground/[0.03] p-4">
                <h3 id="monitor-how-title" className="text-sm font-semibold text-text-primary">How it works</h3>
                <ol className="mt-2 list-decimal space-y-1.5 pl-5 text-[13px] leading-normal text-text-secondary">
                  <li>Choose where alerts go: Slack and, optionally, a repository for GitHub issues.</li>
                  <li>Press <strong>Connect</strong>. You get the drain URL and a token, shown only once.</li>
                  <li>Add a log drain in {PROVIDER_LABEL[draft.provider]} with that URL and the token as an <code className="font-mono">Authorization</code> header. Needs the Pro plan.</li>
                  <li>Only errors become alerts; the rest of the logs are dropped.</li>
                </ol>
              </section>
            )}
            {!draft.id && gcp && (
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
              {draft.provider !== 'gcp' && (
                <Field id="monitor-provider-ref" label={`${PROVIDER_REF_LABEL[draft.provider]} (optional)`} hint="Only used to link each error to its logs.">
                  <Input id="monitor-provider-ref" className="font-mono" value={draft.providerRef} placeholder={draft.provider === 'supabase' ? 'abcdefghijklmnopqrst' : 'my-team'} onChange={e => set('providerRef', e.target.value.trim())} />
                </Field>
              )}
              {gcp && <>
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
              </>}
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
              <Field id="monitor-github" label="GitHub issues" hint={serverGithub ? 'The server\'s gh account, a GitHub App or token connector of this organization, or a new token.' : 'A GitHub App or token connector of this organization, or a new token.'}>
                <StyledSelect id="monitor-github" className={SELECT_CLASS} value={draft.github} onChange={e => set('github', e.target.value)}>
                  <option value="">No GitHub issues</option>
                  {serverGithub && <option value={SERVER_GITHUB}>Server GitHub (authenticated gh)</option>}
                  {githubConnectors.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                  <option value={NEW_GITHUB_TOKEN}>New GitHub token…</option>
                </StyledSelect>
              </Field>
              {draft.github === NEW_GITHUB_TOKEN && (
                <Field id="monitor-github-token" label="GitHub token" hint="Fine-grained token with Issues: read and write (and Contents: read for audits) on the repositories. Stored encrypted as a connector and never shown again.">
                  <Input id="monitor-github-token" type="password" autoComplete="off" className="font-mono" value={draft.newToken} placeholder="github_pat_…" onChange={e => set('newToken', e.target.value)} />
                </Field>
              )}
              {draft.github && (
                <Field id="monitor-repo" label={draft.auditExecutor ? 'Default repository for issues' : 'Repository for issues'} hint={draft.auditExecutor ? 'Used when the audit cannot name the repository that causes the error. Its owner is where the audit may look.' : undefined}>
                  <Input id="monitor-repo" className="font-mono" value={draft.repo} placeholder="owner/repo" onChange={e => set('repo', e.target.value)} />
                </Field>
              )}
            </FieldGroup>
            <FieldGroup
              title="Error audit"
              description="Before opening the issue, an agent reads the code to find why the error happens and which repository causes it, then the issue opens there. Only for new and regressed errors, up to the daily limit; past it, issues open without one."
            >
              {!draft.github ? (
                <p className="text-[13px] text-text-secondary">Turn on GitHub issues first: the audit decides which repository gets them.</p>
              ) : (
                <>
                  <label className="flex items-center gap-2 text-sm text-text-secondary">
                    <input
                      type="checkbox"
                      checked={!!draft.auditExecutor}
                      onChange={e => setDraft({ ...draft, auditExecutor: e.target.checked ? 'claude' : '', auditModel: e.target.checked ? draft.auditModel : '' })}
                    />
                    Audit new errors before opening the issue
                  </label>
                  {draft.auditExecutor && (
                    <>
                      <Field id="monitor-audit-agent" label="Audit agent" hint="A new agent is created and kept in sync from here. An existing one is used as it is.">
                        <StyledSelect id="monitor-audit-agent" className={SELECT_CLASS} value={draft.auditAgent} onChange={e => set('auditAgent', e.target.value)}>
                          <option value="new">New agent</option>
                          {auditAgents.filter(a => !draft.ownAgents.includes(a.id)).map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
                        </StyledSelect>
                      </Field>
                      <Field id="monitor-audit-limit" label="Automatic audits per day">
                        <Input id="monitor-audit-limit" type="number" min={1} max={500} value={draft.auditLimit} onChange={e => set('auditLimit', e.target.value)} />
                      </Field>
                    </>
                  )}
                  {draft.auditExecutor && draft.auditAgent === 'new' && (
                    <>
                      <Field id="monitor-audit-executor" label="Executor">
                        <TaskExecutorSelect value={draft.auditExecutor} template="error_audit" onChange={value => set('auditExecutor', value)} />
                      </Field>
                      <Field id="monitor-audit-model" label="Model (optional)" hint="Leave empty for the executor's default model.">
                        <Input id="monitor-audit-model" className="font-mono" value={draft.auditModel} placeholder={draft.auditExecutor === 'claude' ? 'sonnet' : ''} onChange={e => set('auditModel', e.target.value)} />
                      </Field>
                    </>
                  )}
                </>
              )}
            </FieldGroup>
            <FieldGroup
              title="Resolve issues"
              description="An agent writes a fix and opens a draft PR that closes the issue. The PR is posted in the Slack thread of the alert for someone to review; nothing is merged automatically. Use it from the Resolve issue button on each error (no limit), or on every new issue (up to the daily limit)."
            >
              {!draft.github ? (
                <p className="text-[13px] text-text-secondary">Turn on GitHub issues first: the agent resolves those issues.</p>
              ) : (
                <>
                  <Field
                    id="monitor-fix-agent"
                    label="Agent that resolves issues"
                    hint="Any GitHub Issue Resolver of this organization works on its own repositories. A new agent follows the issue to any repository of the owner, and needs the server's gh account."
                  >
                    <StyledSelect id="monitor-fix-agent" className={SELECT_CLASS} value={draft.fixAgent} onChange={e => setDraft({ ...draft, fixAgent: e.target.value, autofix: e.target.value ? draft.autofix : false })}>
                      <option value="">None</option>
                      {draft.github === SERVER_GITHUB && <option value="new">New agent (uses the audit&apos;s executor, or Claude)</option>}
                      {resolverAgents.filter(a => !draft.ownAgents.includes(a.id)).map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
                    </StyledSelect>
                  </Field>
                  {draft.fixAgent && (
                    <>
                      <label className="flex items-center gap-2 text-sm text-text-secondary">
                        <input type="checkbox" checked={draft.autofix} onChange={e => set('autofix', e.target.checked)} />
                        Resolve every new issue automatically
                      </label>
                      {draft.autofix && (
                        <Field id="monitor-fix-limit" label="Automatic fixes per day" hint="Fixes from the Resolve issue button don't count and have no limit.">
                          <Input id="monitor-fix-limit" type="number" min={1} max={500} value={draft.fixLimit} onChange={e => set('fixLimit', e.target.value)} />
                        </Field>
                      )}
                    </>
                  )}
                </>
              )}
            </FieldGroup>
            <FieldGroup title="Repeats">
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
  const drain = source && source.provider !== 'gcp' ? source.provider : null
  const endpoint = source ? (drain ? drainEndpoint(source.id) : pushEndpoint(source.id)) : ''
  const steps = !source ? [] : drain ? drainSteps(drain, endpoint, source.ingest_token) : setupSteps(source, gcpProject, endpoint)
  const close = () => { setEdited(null); onClose() }
  return (
    <Modal open={source !== null} onOpenChange={value => { if (!value) close() }} size="xl" position="right">
      {source && (
        <div className="flex min-h-full flex-col">
          <ModalHeader className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <ModalTitle>Set up {source.name} in {PROVIDER_LABEL[source.provider]}</ModalTitle>
              <ModalDescription>
                {drain ? 'Add one log drain. Every function, database and request log of the project is covered, with no code changes.' : 'Run these once, in order, in Cloud Shell. Every service in the project is covered, with no code changes.'}
              </ModalDescription>
            </div>
            <Button size="sm" variant="ghost" aria-label="Close" leftIcon={<X className="h-4 w-4" />} onClick={close} />
          </ModalHeader>
          <ModalContent className="flex-1 space-y-4">
            {!drain && (
              <Field id="monitor-gcp-project" label="GCP project ID">
                <Input id="monitor-gcp-project" className="font-mono" value={edited ?? setup?.gcpProject ?? ''} placeholder="my-project-123" onChange={e => setEdited(e.target.value)} />
              </Field>
            )}
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
            {!drain && (
              <p className="text-[12px] text-text-tertiary">
                A command that says <em>already exists</em> is fine: move on to the next one.
              </p>
            )}
          </ModalContent>
          <ModalFooter>
            {!drain && <CopyButton text={source ? setupCommands(source, gcpProject, endpoint) : ''} label="Copy all steps" />}
            <Button onClick={close}>Done</Button>
          </ModalFooter>
        </div>
      )}
    </Modal>
  )
}
