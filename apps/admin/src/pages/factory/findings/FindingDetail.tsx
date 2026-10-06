import { useState, type ReactNode } from 'react'
import { Archive, ArchiveRestore, CheckCircle2, ExternalLink, FilePlus2, RefreshCw, Send, Wand2 } from 'lucide-react'
import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import { ConfirmModal } from '../../../components/ConfirmModal'
import type { AutonomousAgentDelivery, AutonomousAgentFinding } from '../../../types'
import {
  asArr, asDict, asStr, channelName, deliveryStatusMeta, evidenceKindLabel, findingStatusMeta, humanize, isPostEvidence, relativeTime,
  severityLabel, severityVariant, templateName, type Dict, type Tone,
} from '../shared/format'
import { FieldLabel, RawJson, TEXT_LINK } from '../shared/ui'
import { FINDING_TYPE_LABEL, findingKind, findingType, linkedIssueOf } from './model'

const DOT: Record<Tone, string> = { ok: 'bg-status-success', warn: 'bg-status-warning', bad: 'bg-status-error', info: 'bg-status-info', neutral: 'bg-text-tertiary' }

export type FindingDetailProps = {
  finding: AutonomousAgentFinding
  deliveries: AutonomousAgentDelivery[]
  agentName?: string
  templateKey?: string
  connectedDestinations: Set<string>
  can: (permission: string) => boolean
  onResolveWithAgent: (target: { findingId: string; title: string; finding: Dict; issue?: { repository: string; number: number } }) => void
  onCreateIssue: (findingId: string) => void
  creatingIssue: boolean
  onMarkResolved: (findingId: string) => void
  onArchive: (findingId: string) => void
  onRestore: (findingId: string) => void
  onPublish: (findingId: string, destination: 'personal' | 'organization') => void
  publishing: boolean
  onRetryDelivery: (deliveryId: string) => void
  retrying?: string
}

function Block({ title, children }: { title: string; children: ReactNode }) {
  return <div className="space-y-1.5"><FieldLabel>{title}</FieldLabel>{children}</div>
}

function LeadBlock({ lead }: { lead: Dict }) {
  const execs = (asArr(lead.executives) ?? []).map(asDict).filter(Boolean) as Dict[]
  const sources = (asArr(lead.source_urls) ?? []).map(asStr).filter(Boolean) as string[]
  const link = (label: string, url?: string, text?: string) => url ? <a key={label} href={url} target="_blank" rel="noreferrer" className={TEXT_LINK}>{text ?? label}</a> : null
  const socialChips = (arr: unknown, prefix: string) => (asArr(arr) ?? []).map(asDict).filter(Boolean).map((s, i) => {
    const url = asStr((s as Dict).url); const platform = asStr((s as Dict).platform)
    return url ? <a key={`${prefix}${i}`} href={url} target="_blank" rel="noreferrer" className={TEXT_LINK}>{platform ? humanize(platform) : 'Link'}</a> : null
  }).filter(Boolean)
  const contacts = [
    link('Website', asStr(lead.website)),
    asStr(lead.contact_email) ? <a key="email" href={`mailto:${asStr(lead.contact_email)}`} className={TEXT_LINK}>{asStr(lead.contact_email)}</a> : null,
    asStr(lead.contact_phone) ? <span key="phone" className="text-text-secondary">{asStr(lead.contact_phone)}</span> : null,
    link('Contact page', asStr(lead.contact_page)),
    link('LinkedIn', asStr(lead.company_linkedin)),
    ...socialChips(lead.social_links, 'co-social-'),
  ].filter(Boolean)
  return (
    <section aria-label="Lead" className="space-y-3 rounded-[18px] border border-border-primary p-4 text-[13px]">
      {(asStr(lead.industry) || asStr(lead.headquarters)) && (
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
          {asStr(lead.industry) && <><dt className="text-text-tertiary">Industry</dt><dd className="text-text-secondary">{asStr(lead.industry)}</dd></>}
          {asStr(lead.headquarters) && <><dt className="text-text-tertiary">Headquarters</dt><dd className="text-text-secondary">{asStr(lead.headquarters)}</dd></>}
        </dl>
      )}
      {contacts.length > 0 && <div className="flex flex-wrap gap-x-4 gap-y-1">{contacts}</div>}
      {execs.length > 0 && (
        <Block title="Decision-makers">
          <ul className="list-none space-y-1 p-0">
            {execs.map((e, i) => (
              <li key={i} className="flex flex-wrap items-baseline gap-x-3">
                <span className="font-medium text-text-primary">{asStr(e.name) ?? 'Unnamed'}</span>
                {asStr(e.title) && <span className="text-text-tertiary">{asStr(e.title)}</span>}
                {asStr(e.linkedin) && <a href={asStr(e.linkedin)} target="_blank" rel="noreferrer" className={TEXT_LINK}>LinkedIn</a>}
                {asStr(e.public_email) && <a href={`mailto:${asStr(e.public_email)}`} className={TEXT_LINK}>{asStr(e.public_email)}</a>}
                {asStr(e.direct_phone) && <span className="text-text-secondary">{asStr(e.direct_phone)}</span>}
                {socialChips(e.social_links, `ex${i}-social-`)}
              </li>
            ))}
          </ul>
        </Block>
      )}
      {(asStr(lead.email_subject) || asStr(lead.email_body)) && (
        <details>
          <summary className="cursor-pointer rounded-[8px] text-[12px] font-medium text-text-tertiary hover:text-text-secondary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">Drafted email</summary>
          {asStr(lead.email_subject) && <p className="mt-1.5 text-text-secondary"><span className="text-text-tertiary">Subject:</span> {asStr(lead.email_subject)}</p>}
          {asStr(lead.email_body) && <pre className="mt-1 whitespace-pre-wrap font-sans text-text-secondary">{asStr(lead.email_body)}</pre>}
        </details>
      )}
      {sources.length > 0 && (
        <Block title="Sources">
          <div className="flex flex-wrap gap-x-3 gap-y-1">{sources.slice(0, 6).map((u, i) => <a key={i} href={u} target="_blank" rel="noreferrer" className="max-w-[220px] truncate text-[12px] text-text-tertiary hover:text-text-secondary">{u.replace(/^https?:\/\//, '')}</a>)}</div>
        </Block>
      )}
    </section>
  )
}

/** One finding: pills, title, summary, one primary action, evidence, deliveries, raw last. */
export function FindingDetail(props: FindingDetailProps) {
  const { finding, deliveries, agentName, templateKey, connectedDestinations: connected, can } = props
  const [confirmPublish, setConfirmPublish] = useState<'personal' | 'organization' | null>(null)
  const ev = (finding.evidence ?? {}) as Dict
  const type = findingType(finding)
  const kind = findingKind(finding)
  const archived = finding.status === 'ignored'
  const isPost = isPostEvidence(ev)
  const lead = asDict(ev.lead)
  const statusMeta = findingStatusMeta(finding.status)
  const { delivery: issueDelivery, issue } = linkedIssueOf(deliveries)
  // Whether this post was already published to LinkedIn (a delivered 'linkedin'
  // delivery). Drives the "Published" state and blocks re-publish.
  const linkedinDelivery = deliveries.find(d => d.channel === 'linkedin' && d.status === 'delivered')

  // Build the screenshot src from the stable re-signing endpoint using the run id
  // + the stored filename, so old evidence (whose baked-in presigned URL has since
  // expired) still renders. Fall back to the stored URL.
  const shotName = asStr(ev.screenshot)
  const shot = finding.run_id && shotName
    ? `${import.meta.env.VITE_API_URL ?? ''}/evidence/${encodeURIComponent(finding.run_id)}/${encodeURIComponent(shotName)}`
    : (asStr(ev.screenshot_url) ?? shotName)
  const locDict = asDict(ev.location)
  const locStr = asStr(ev.location)
  const steps = asArr(ev.steps)
  const repro = asStr(ev.repro) ?? asStr(ev.excerpt) ?? asStr(ev.code)
  const images = ((asArr(asDict(ev.post)?.images) ?? []).map(asDict).filter(Boolean) as Dict[]).filter(image => asStr(image.url))

  const postDest = asStr(asDict(ev.post)?.destination)
  const publishDest: 'personal' | 'organization' | null = (postDest === 'organization' || postDest === 'personal') && connected.has(postDest)
    ? postDest : connected.has('personal') ? 'personal' : connected.has('organization') ? 'organization' : null

  const canRun = can('autonomous_agent:run')
  const canUpdate = can('autonomous_agent:update')
  const resolvable = !isPost && !lead

  // One primary action per finding.
  let primary: ReactNode = null
  if (!archived && canRun && resolvable) {
    primary = <Button size="sm" variant="primary" leftIcon={<Wand2 className="h-3.5 w-3.5" />} onClick={() => props.onResolveWithAgent({ findingId: finding.id, title: finding.title, finding: { title: finding.title, summary: finding.summary, severity: finding.severity, evidence: ev }, issue })}>Resolve with agent</Button>
  } else if (!archived && canRun && isPost && !linkedinDelivery && publishDest) {
    primary = <Button size="sm" variant="primary" leftIcon={<Send className="h-3.5 w-3.5" />} loading={props.publishing} onClick={() => setConfirmPublish(publishDest)}>Publish to LinkedIn</Button>
  }
  const secondary: ReactNode[] = []
  if (!archived && canRun && resolvable && !issueDelivery) secondary.push(<Button key="issue" size="sm" variant="secondary" leftIcon={<FilePlus2 className="h-3.5 w-3.5" />} loading={props.creatingIssue} onClick={() => props.onCreateIssue(finding.id)}>Create issue</Button>)
  if (canUpdate && finding.status === 'open') secondary.push(<Button key="resolve" size="sm" variant="secondary" leftIcon={<CheckCircle2 className="h-3.5 w-3.5" />} onClick={() => props.onMarkResolved(finding.id)}>Mark resolved</Button>)
  if (canUpdate && !archived) secondary.push(<Button key="archive" size="sm" variant="ghost" leftIcon={<Archive className="h-3.5 w-3.5" />} onClick={() => props.onArchive(finding.id)}>Archive</Button>)
  if (canUpdate && archived) secondary.push(<Button key="restore" size="sm" variant="secondary" leftIcon={<ArchiveRestore className="h-3.5 w-3.5" />} onClick={() => props.onRestore(finding.id)}>Restore</Button>)

  return (
    <article aria-labelledby="finding-title" className="space-y-5">
      <div className="space-y-3">
        <div className="flex flex-wrap items-center gap-1.5">
          <Badge size="sm" variant={severityVariant(finding.severity)} dot>{severityLabel(finding.severity)}</Badge>
          <Badge size="sm" variant="default">{FINDING_TYPE_LABEL[type]}{kind && type === 'bug' ? ` · ${evidenceKindLabel(kind)}` : ''}</Badge>
          <Badge size="sm" variant={statusMeta.variant}>{statusMeta.label}</Badge>
          {isPost && linkedinDelivery && (
            <Badge size="sm" variant="success">
              Published
              {linkedinDelivery.external_url && <a href={linkedinDelivery.external_url} target="_blank" rel="noreferrer" aria-label="Open the published post" className="inline-flex rounded-full focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring"><ExternalLink className="h-3 w-3" aria-hidden /></a>}
            </Badge>
          )}
        </div>
        <h2 id="finding-title" className="text-[15px] font-semibold leading-snug tracking-[-0.2px] text-text-primary">{finding.title}</h2>
        <p className="text-[13px] leading-relaxed text-text-secondary">{finding.summary}</p>
        <dl className="flex flex-wrap gap-x-5 gap-y-1 text-[12px]">
          {agentName && <div className="flex gap-1.5"><dt className="text-text-tertiary">Agent</dt><dd className="text-text-secondary">{agentName}{templateKey ? ` (${templateName(templateKey)})` : ''}</dd></div>}
          <div className="flex gap-1.5"><dt className="text-text-tertiary">First seen</dt><dd className="text-text-secondary">{relativeTime(finding.created_at)}</dd></div>
          <div className="flex gap-1.5"><dt className="text-text-tertiary">Seen</dt><dd className="text-text-secondary tabular-nums">{finding.occurrence_count} time{finding.occurrence_count === 1 ? '' : 's'}</dd></div>
        </dl>
        {(primary || secondary.length > 0) && <div className="flex flex-wrap items-center gap-2 pt-1">{primary}{secondary}</div>}
        {isPost && !linkedinDelivery && !archived && canRun && !publishDest && <p className="text-[12px] text-text-tertiary">Connect a LinkedIn account above to publish this post.</p>}
      </div>

      {images.length > 0 && (
        // Imagery generated for this post under the agent's design system, shown
        // before publishing so the image is judged together with the copy.
        <Block title="Post images">
          <div className="flex flex-wrap gap-2">
            {images.map((image, index) => {
              const url = asStr(image.url)!
              const alt = asStr(image.alt_text) ?? `Generated image ${index + 1} for this post`
              return (
                <a key={url} href={url} target="_blank" rel="noreferrer" title={alt} className="rounded-[8px] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
                  <img src={url} alt={alt} className="h-28 w-28 rounded-[8px] border border-border-primary object-cover" loading="lazy" />
                </a>
              )
            })}
          </div>
        </Block>
      )}

      {lead && <LeadBlock lead={lead} />}

      {(shot || locDict || locStr || (steps && steps.length) || repro) && (
        <section aria-label="Evidence" className="space-y-4">
          {shot && (
            <a href={shot} target="_blank" rel="noreferrer" className="block max-w-md rounded-[11px] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
              <img src={shot} alt={`Screenshot for ${finding.title}`} className="max-h-56 w-full rounded-[11px] border border-border-primary object-cover object-top" />
            </a>
          )}
          {(locDict || locStr) && (
            <Block title="Where">
              {locDict
                ? <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13px]">{Object.entries(locDict).map(([k, v]) => <div key={k} className="contents"><dt className="text-text-tertiary">{humanize(k)}</dt><dd className="break-all font-mono text-[12px] text-text-secondary">{typeof v === 'string' || typeof v === 'number' ? String(v) : JSON.stringify(v)}</dd></div>)}</dl>
                : <p className="break-all font-mono text-[12px] text-text-secondary">{locStr}</p>}
            </Block>
          )}
          {steps && steps.length > 0 && (
            <Block title="Steps to reproduce">
              <ol className="grid list-none gap-1.5 p-0">
                {steps.map((s, i) => (
                  <li key={i} className="flex gap-2 text-[13px] text-text-secondary">
                    <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full border border-border-primary bg-white/[0.05] text-[11px] text-text-tertiary tabular-nums">{i + 1}</span>
                    {typeof s === 'string' ? s : JSON.stringify(s)}
                  </li>
                ))}
              </ol>
            </Block>
          )}
          {repro && (
            <Block title="Evidence excerpt">
              <pre className="overflow-auto rounded-[11px] bg-black/30 p-3 font-mono text-[12px] text-text-secondary">{repro}</pre>
            </Block>
          )}
        </section>
      )}

      <Block title="Deliveries">
        {deliveries.length === 0 ? <p className="text-[13px] text-text-tertiary">Not delivered anywhere yet.</p> : (
          <ul className="list-none space-y-1.5 p-0">
            {deliveries.map(item => {
              const meta = deliveryStatusMeta(item.status)
              const retryable = canRun && ['slack', 'github_issue'].includes(item.channel) && ['failed', 'dead_letter'].includes(item.status)
              return (
                <li key={item.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-[11px] border border-border-primary bg-white/[0.02] px-3 py-2 text-[13px]">
                  <span className={`h-2 w-2 shrink-0 rounded-full ${DOT[meta.tone]}`} aria-hidden />
                  <span className="text-text-primary">{channelName(item.channel)}</span>
                  <span className="text-text-secondary">{meta.label}</span>
                  {item.last_error_code && <span className="text-[12px] text-status-error">{humanize(item.last_error_code)}</span>}
                  <span className="ml-auto flex items-center gap-2">
                    {item.external_url && <a href={item.external_url} target="_blank" rel="noreferrer" className={`inline-flex items-center gap-1 ${TEXT_LINK}`}>Open<ExternalLink className="h-3 w-3" aria-hidden /></a>}
                    {retryable && <Button size="sm" variant="secondary" leftIcon={<RefreshCw className="h-3 w-3" />} loading={props.retrying === item.id} onClick={() => props.onRetryDelivery(item.id)}>Retry</Button>}
                  </span>
                </li>
              )
            })}
          </ul>
        )}
      </Block>

      <RawJson label="Raw evidence (JSON)" value={finding.evidence} />

      <ConfirmModal
        open={confirmPublish != null}
        title="Publish this post to LinkedIn?"
        description={`It goes out now on the ${confirmPublish === 'organization' ? 'company page' : 'personal profile'} that is connected.`}
        confirmLabel="Publish"
        loading={props.publishing}
        onClose={() => setConfirmPublish(null)}
        onConfirm={() => { if (confirmPublish) props.onPublish(finding.id, confirmPublish); setConfirmPublish(null) }}
      />
    </article>
  )
}
