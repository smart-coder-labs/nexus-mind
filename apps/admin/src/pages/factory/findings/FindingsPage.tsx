import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useNavigate } from 'react-router-dom'
import { Archive, Bug, Search, SlidersHorizontal } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { EmptyState } from '../../../components/ui/EmptyState'
import { Input } from '../../../components/ui/Input'
import { Skeleton } from '../../../components/ui/Skeleton'
import { ConfirmModal } from '../../../components/ConfirmModal'
import type { AutonomousAgentFinding } from '../../../types'
import { SEVERITY_ORDER, asDict, evidenceKindLabel, findingStatusMeta, isPostEvidence, relativeTime, severityDot, severityLabel, templateName, type Dict } from '../shared/format'
import { InlineError, PageHeader, errorMessage, useFactory } from '../shared/ui'
import { FindingDetail } from './FindingDetail'
import { FINDING_TYPE_LABEL, FINDING_TYPE_PLURAL, findingKind, findingType, type FindingType } from './model'
import { ResolveWithAgentDialog } from './ResolveWithAgentDialog'

type Filters = { type: string; severity: string; status: string; agent: string; template: string; kind: string }
const NO_FILTERS: Filters = { type: 'all', severity: 'all', status: 'all', agent: 'all', template: 'all', kind: 'all' }

type Option = { value: string; label: string }

function RailGroup({ title, options, value, count, onChange }: { title: string; options: Option[]; value: string; count: (value: string) => number; onChange: (value: string) => void }) {
  const shown = options.filter(o => o.value === 'all' || o.value === value || count(o.value) > 0)
  if (shown.length <= 1) return null
  return (
    <div role="group" aria-label={title} className="space-y-1">
      <p className="px-2 text-[12px] font-medium text-text-tertiary">{title}</p>
      {shown.map(o => {
        const active = value === o.value
        return (
          <button
            key={o.value}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(o.value)}
            className={`flex h-8 w-full items-center justify-between gap-2 rounded-[8px] px-2 text-left text-[13px] transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring ${active ? 'bg-white/[0.08] text-text-primary' : 'text-text-secondary hover:bg-white/[0.04] hover:text-text-primary'}`}
          >
            <span className="truncate">{o.label}</span>
            <span className="text-[12px] text-text-tertiary tabular-nums">{count(o.value)}</span>
          </button>
        )
      })}
    </div>
  )
}

/** /factory/findings — triage: filter rail, list, detail with one primary action. */
export default function FindingsPage() {
  const { can, client, invalidate } = useFactory()
  const navigate = useNavigate()
  const [filters, setFilters] = useState<Filters>(NO_FILTERS)
  const [search, setSearch] = useState('')
  const [filtersOpen, setFiltersOpen] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [actionError, setActionError] = useState('')
  const [confirmArchiveAll, setConfirmArchiveAll] = useState(false)
  // "Resolve with agent": hand a single finding (and its linked GitHub issue, if
  // one was filed) to a chosen issue-resolver so it fixes ONLY that one thing.
  const [resolveWith, setResolveWith] = useState<{ findingId: string; title: string; finding: Dict; issue?: { repository: string; number: number } } | null>(null)
  const detailRef = useRef<HTMLDivElement>(null)

  const agents = useQuery({ queryKey: ['autonomous-agents'], queryFn: () => client.listAutonomousAgents() })
  const findings = useQuery({ queryKey: ['autonomous-findings'], queryFn: () => client.listAutonomousAgentFindings() })
  const deliveries = useQuery({ queryKey: ['autonomous-deliveries'], queryFn: () => client.listAutonomousAgentDeliveries() })
  const linkedinConnections = useQuery({ queryKey: ['linkedin-connections'], queryFn: () => client.listLinkedinConnections() })

  const onError = (fallback: string) => (value: unknown) => setActionError(errorMessage(value, fallback))
  const retryDelivery = useMutation({ mutationFn: (id: string) => client.retryAutonomousAgentDelivery(id), onSuccess: () => invalidate('autonomous-deliveries'), onError: onError('Could not retry the delivery.') })
  const resolveFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'resolved'), onSuccess: () => invalidate('autonomous-findings'), onError: onError('Could not mark the finding resolved.') })
  const archiveFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'ignored'), onSuccess: () => invalidate('autonomous-findings'), onError: onError('Could not archive the finding.') })
  const restoreFinding = useMutation({ mutationFn: (id: string) => client.patchAutonomousAgentFinding(id, 'open'), onSuccess: () => invalidate('autonomous-findings'), onError: onError('Could not restore the finding.') })
  // Approve & publish a generated post draft to LinkedIn.
  const publishPost = useMutation({
    mutationFn: (vars: { id: string; destination: 'personal' | 'organization' }) => client.publishFindingLinkedin(vars.id, { destination: vars.destination }),
    onSuccess: result => { invalidate('autonomous-findings', 'autonomous-deliveries'); if (result.url) window.open(result.url, '_blank') },
    onError: (err: unknown) => setActionError(`Could not publish to LinkedIn: ${errorMessage(err, 'unknown error')}. Check the connection and try again.`),
  })
  const connectLinkedin = useMutation({
    mutationFn: (destination: 'personal' | 'organization') => client.linkedinAuthorize(destination),
    onSuccess: result => { if (result.url) window.open(result.url, '_blank', 'width=600,height=760') },
    onError: (err: unknown) => setActionError(`LinkedIn is not configured on this server: ${errorMessage(err, 'unknown error')}.`),
  })
  const archiveAllFindings = useMutation({ mutationFn: () => client.archiveAllAutonomousAgentFindings(), onSuccess: () => invalidate('autonomous-findings'), onError: onError('Could not archive the findings.') })
  // "Create issue" for a finding the agent did not file. Retries with an explicit
  // repository when the agent has none configured (the prompt is required input).
  const createFindingIssue = useMutation({
    mutationFn: (vars: { findingId: string; repository?: string }) => client.createFindingIssue(vars.findingId, vars.repository),
    onSuccess: () => invalidate('autonomous-findings', 'autonomous-deliveries'),
    onError: (err: unknown, vars) => {
      const code = (err as { code?: string })?.code
      if (code === 'repository_required' && !vars.repository) {
        const repository = window.prompt('Repository for this issue (owner/repo):')?.trim()
        if (repository) createFindingIssue.mutate({ findingId: vars.findingId, repository })
        return
      }
      setActionError(`Could not create the issue: ${errorMessage(err, 'unknown error')}.`)
    },
  })
  const resolveWithAgent = useMutation({
    mutationFn: (vars: { agentId: string; findingId: string; finding: Dict; issue?: { repository: string; number: number } }) =>
      client.runAutonomousAgent(vars.agentId, { finding: vars.finding, finding_id: vars.findingId, ...(vars.issue ? { issue: vars.issue } : {}) }),
    onSuccess: run => { invalidate('autonomous-runs'); setResolveWith(null); navigate('/factory/runs', { state: { runId: run?.id } }) },
  })

  const allAgents = agents.data ?? []
  const agentsById = new Map(allAgents.map(a => [a.id, a]))
  const allFindings = findings.data ?? []
  const templateOf = (f: AutonomousAgentFinding) => agentsById.get(f.definition_id)?.template_key ?? ''
  const q = search.trim().toLowerCase()

  const matches = (f: AutonomousAgentFinding, applied: Filters) => {
    if (applied.status === 'all' ? f.status === 'ignored' : applied.status === 'archived' ? f.status !== 'ignored' : f.status !== applied.status) return false
    if (applied.type !== 'all' && findingType(f) !== applied.type) return false
    if (applied.severity !== 'all' && f.severity !== applied.severity) return false
    if (applied.agent !== 'all' && f.definition_id !== applied.agent) return false
    if (applied.template !== 'all' && templateOf(f) !== applied.template) return false
    if (applied.kind !== 'all' && findingKind(f) !== applied.kind) return false
    if (q && !(f.title?.toLowerCase().includes(q) || f.summary?.toLowerCase().includes(q))) return false
    return true
  }
  // Faceted counts: how many findings each option would show, given the other filters.
  const countFor = (group: keyof Filters) => (value: string) => allFindings.filter(f => matches(f, { ...filters, [group]: value })).length
  const setFilter = (group: keyof Filters) => (value: string) => setFilters(prev => ({ ...prev, [group]: value }))
  const visible = allFindings.filter(f => matches(f, filters))
  const activeCount = allFindings.filter(f => f.status !== 'ignored').length
  const anyFilter = q !== '' || Object.entries(filters).some(([, v]) => v !== 'all')

  const presentTemplates = [...new Set(allFindings.map(templateOf).filter(Boolean))].sort()
  const presentKinds = [...new Set(allFindings.map(findingKind).filter(Boolean))].sort()
  const presentAgents = [...new Map(allFindings.map(f => [f.definition_id, agentsById.get(f.definition_id)?.name ?? 'Deleted agent'] as const)).entries()]

  const selected = visible.find(f => f.id === selectedId) ?? visible[0]
  useEffect(() => { if (selected && selected.id !== selectedId) setSelectedId(selected.id) }, [selected, selectedId])
  const select = (id: string) => {
    setSelectedId(id)
    // Small screens stack the detail under the list: bring it into view.
    if (typeof window !== 'undefined' && window.innerWidth < 1024) requestAnimationFrame(() => detailRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' }))
  }

  const connected = new Set((linkedinConnections.data ?? []).map(c => c.destination))
  const hasPosts = allFindings.some(f => isPostEvidence(asDict(f.evidence) ?? {}))
  const deliveriesFor = (id: string) => deliveries.data?.filter(item => item.finding_id === id) ?? []

  const rail = (
    <div className="space-y-5">
      <RailGroup title="Type" value={filters.type} onChange={setFilter('type')} count={countFor('type')}
        options={[{ value: 'all', label: 'All types' }, ...(['bug', 'post', 'feedback', 'lead'] as FindingType[]).map(t => ({ value: t, label: FINDING_TYPE_PLURAL[t] }))]} />
      <RailGroup title="Severity" value={filters.severity} onChange={setFilter('severity')} count={countFor('severity')}
        options={[{ value: 'all', label: 'Any severity' }, ...[...SEVERITY_ORDER, ...new Set(allFindings.map(f => f.severity).filter(s => !SEVERITY_ORDER.includes(s)))].map(s => ({ value: s, label: severityLabel(s) }))]} />
      <RailGroup title="Status" value={filters.status} onChange={setFilter('status')} count={countFor('status')}
        options={[{ value: 'all', label: 'Open and resolved' }, { value: 'open', label: 'Open' }, { value: 'resolved', label: 'Resolved' }, { value: 'archived', label: 'Archived' }]} />
      <RailGroup title="Agent" value={filters.agent} onChange={setFilter('agent')} count={countFor('agent')}
        options={[{ value: 'all', label: 'All agents' }, ...presentAgents.map(([id, name]) => ({ value: id, label: name }))]} />
      {presentTemplates.length > 1 && <RailGroup title="Template" value={filters.template} onChange={setFilter('template')} count={countFor('template')}
        options={[{ value: 'all', label: 'All templates' }, ...presentTemplates.map(t => ({ value: t, label: templateName(t) }))]} />}
      {presentKinds.length > 1 && <RailGroup title="Category" value={filters.kind} onChange={setFilter('kind')} count={countFor('kind')}
        options={[{ value: 'all', label: 'All categories' }, ...presentKinds.map(k => ({ value: k, label: evidenceKindLabel(k) }))]} />}
      {anyFilter && <Button size="sm" variant="ghost" onClick={() => { setFilters(NO_FILTERS); setSearch('') }}>Clear filters</Button>}
    </div>
  )

  return (
    <div className="mx-auto max-w-[1440px] space-y-6 p-6 md:p-8">
      <PageHeader
        title="Findings"
        subtitle="Bugs, posts, feedback and leads the agents found. Decide what happens to each one."
        actions={<>
          <div className="w-full sm:w-60"><Input inputSize="sm" type="search" aria-label="Search findings" placeholder="Search findings" leftIcon={<Search className="h-3.5 w-3.5" />} value={search} onChange={e => setSearch(e.target.value)} /></div>
          {can('autonomous_agent:update') && activeCount > 0 && <Button size="sm" variant="secondary" leftIcon={<Archive className="h-3.5 w-3.5" />} loading={archiveAllFindings.isPending} onClick={() => setConfirmArchiveAll(true)}>Archive all</Button>}
        </>}
      />

      {actionError && <InlineError message={actionError} onDismiss={() => setActionError('')} />}

      {hasPosts && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-[18px] border border-border-primary bg-white/[0.02] px-5 py-4">
          <div className="text-[13px] text-text-secondary">
            <span className="font-medium text-text-primary">LinkedIn</span>{' '}
            {connected.size
              ? <>connected for {[...connected].map(d => (d === 'organization' ? 'the company page' : 'your personal profile')).join(' and ')}.</>
              : 'is not connected. Connect an account to publish approved posts.'}
          </div>
          {can('autonomous_agent:update') && (
            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" variant="secondary" loading={connectLinkedin.isPending && connectLinkedin.variables === 'personal'} onClick={() => connectLinkedin.mutate('personal')}>{connected.has('personal') ? 'Reconnect personal' : 'Connect personal'}</Button>
              <Button size="sm" variant="secondary" loading={connectLinkedin.isPending && connectLinkedin.variables === 'organization'} onClick={() => connectLinkedin.mutate('organization')}>{connected.has('organization') ? 'Reconnect company' : 'Connect company'}</Button>
              <Button size="sm" variant="ghost" loading={linkedinConnections.isFetching} onClick={() => void linkedinConnections.refetch()}>Refresh</Button>
            </div>
          )}
        </div>
      )}

      {findings.isError && <InlineError message="Could not load findings." onRetry={() => void findings.refetch()} />}
      {deliveries.isError && <InlineError message="Could not load deliveries, so delivery status is missing." onRetry={() => void deliveries.refetch()} />}

      {findings.isLoading && (
        <div className="grid gap-6 lg:grid-cols-[176px_minmax(0,1fr)]" aria-busy="true" aria-label="Loading findings">
          <div className="hidden space-y-2 lg:block">{[0, 1, 2, 3].map(i => <Skeleton key={i} className="h-6 w-full" />)}</div>
          <div className="space-y-3">{[0, 1, 2].map(i => <Skeleton key={i} className="h-16 w-full" />)}</div>
        </div>
      )}

      {findings.isSuccess && allFindings.length === 0 && (
        <EmptyState icon={<Bug className="h-6 w-6" />} title="No findings yet" description="QA, security and review agents file what they find here, with the screenshot, the reproduction steps and where it was delivered." action={<Button size="sm" variant="primary" onClick={() => navigate('/factory')}>Go to agents</Button>} />
      )}

      {findings.isSuccess && allFindings.length > 0 && (
        <div className="grid gap-6 lg:grid-cols-[176px_minmax(0,1fr)]">
          <aside aria-label="Filter findings">
            <Button size="sm" variant="secondary" className="lg:hidden" leftIcon={<SlidersHorizontal className="h-3.5 w-3.5" />} aria-expanded={filtersOpen} onClick={() => setFiltersOpen(v => !v)}>
              {filtersOpen ? 'Hide filters' : 'Filters'}
            </Button>
            <div className={`${filtersOpen ? 'mt-4 block' : 'hidden'} lg:mt-0 lg:block`}>{rail}</div>
          </aside>

          <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(240px,320px)_minmax(0,1fr)]">
            <section aria-label="Finding list" className="min-w-0 overflow-hidden rounded-[18px] border border-border-primary lg:max-h-[calc(100vh-14rem)] lg:overflow-y-auto">
              <p className="border-b border-border-primary px-4 py-2.5 text-[12px] text-text-tertiary"><span className="tabular-nums">{visible.length}</span> of <span className="tabular-nums">{allFindings.length}</span></p>
              {visible.length === 0 ? (
                <div className="p-4">
                  <EmptyState title="Nothing matches" description="No finding fits these filters." action={<Button size="sm" variant="secondary" onClick={() => { setFilters(NO_FILTERS); setSearch('') }}>Clear filters</Button>} />
                </div>
              ) : (
                <ul className="list-none divide-y divide-border-secondary p-0">
                  {visible.map(f => {
                    const active = selected?.id === f.id
                    const status = findingStatusMeta(f.status)
                    return (
                      <li key={f.id}>
                        <button
                          type="button"
                          aria-current={active ? 'true' : undefined}
                          onClick={() => select(f.id)}
                          className={`flex w-full flex-col gap-1 px-4 py-3 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus-ring ${active ? 'bg-white/[0.06] shadow-[inset_3px_0_0_var(--color-accent-blue)]' : 'hover:bg-white/[0.03]'}`}
                        >
                          <span className="line-clamp-2 text-[13px] font-medium text-text-primary">{f.title}</span>
                          <span className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[12px] text-text-tertiary">
                            <span className="inline-flex items-center gap-1.5"><span className={`h-2 w-2 rounded-full ${severityDot(f.severity)}`} aria-hidden />{severityLabel(f.severity)}</span>
                            <span>{FINDING_TYPE_LABEL[findingType(f)]}</span>
                            {f.status !== 'open' && <span>{status.label}</span>}
                            <span>{relativeTime(f.created_at)}</span>
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              )}
            </section>

            <section ref={detailRef} aria-label="Finding detail" className="min-w-0 scroll-mt-4 rounded-[18px] border border-border-primary p-5 md:p-6 lg:max-h-[calc(100vh-14rem)] lg:overflow-y-auto">
              {selected ? (
                <FindingDetail
                  key={selected.id}
                  finding={selected}
                  deliveries={deliveriesFor(selected.id)}
                  agentName={agentsById.get(selected.definition_id)?.name}
                  templateKey={agentsById.get(selected.definition_id)?.template_key}
                  connectedDestinations={connected}
                  can={can}
                  onResolveWithAgent={target => { resolveWithAgent.reset(); setResolveWith(target) }}
                  onCreateIssue={id => createFindingIssue.mutate({ findingId: id })}
                  creatingIssue={createFindingIssue.isPending}
                  onMarkResolved={id => resolveFinding.mutate(id)}
                  onArchive={id => archiveFinding.mutate(id)}
                  onRestore={id => restoreFinding.mutate(id)}
                  onPublish={(id, destination) => publishPost.mutate({ id, destination })}
                  publishing={publishPost.isPending}
                  onRetryDelivery={id => retryDelivery.mutate(id)}
                  retrying={retryDelivery.isPending ? retryDelivery.variables : undefined}
                />
              ) : <EmptyState title="Select a finding" description="Its evidence, deliveries and next step appear here." />}
            </section>
          </div>
        </div>
      )}

      {resolveWith && (
        <ResolveWithAgentDialog
          target={resolveWith}
          resolvers={allAgents.filter(a => a.template_key === 'github_issue_resolver' && a.status === 'enabled')}
          pending={resolveWithAgent.isPending}
          error={resolveWithAgent.error ? `Could not start the resolver: ${errorMessage(resolveWithAgent.error, 'unknown error')}.` : undefined}
          onClose={() => setResolveWith(null)}
          onRun={agentId => resolveWithAgent.mutate({ agentId, findingId: resolveWith.findingId, finding: resolveWith.finding, issue: resolveWith.issue })}
        />
      )}

      <ConfirmModal
        open={confirmArchiveAll}
        title={`Archive all ${activeCount} finding${activeCount === 1 ? '' : 's'}?`}
        description="They leave the list but are kept. Pick Archived under Status to see or restore them."
        confirmLabel="Archive all"
        loading={archiveAllFindings.isPending}
        onClose={() => setConfirmArchiveAll(false)}
        onConfirm={() => { archiveAllFindings.mutate(); setConfirmArchiveAll(false) }}
      />
    </div>
  )
}
