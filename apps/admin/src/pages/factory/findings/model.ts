import type { AutonomousAgentDelivery, AutonomousAgentFinding } from '../../../types'
import { asDict, asStr, isPostEvidence } from '../shared/format'

export type FindingType = 'bug' | 'post' | 'feedback' | 'lead'
export const FINDING_TYPE_LABEL: Record<FindingType, string> = { bug: 'Bug', post: 'Post', feedback: 'Feedback', lead: 'Lead' }
export const FINDING_TYPE_PLURAL: Record<FindingType, string> = { bug: 'Bugs', post: 'Posts', feedback: 'Feedback', lead: 'Leads' }

/** A finding's type: post / feedback / lead are flagged in evidence; the rest are bugs/issues. */
export function findingType(f: AutonomousAgentFinding): FindingType {
  const ev = asDict(f.evidence) ?? {}
  if (isPostEvidence(ev)) return 'post'
  if (asStr(ev.kind) === 'feedback') return 'feedback'
  if (asDict(ev.lead)) return 'lead'
  return 'bug'
}

/** Evidence class (sast/sca/dast for security, functional/… for QA). */
export function findingKind(f: AutonomousAgentFinding): string {
  const ev = asDict(f.evidence) ?? {}
  return asStr(ev.kind) || asStr(ev.type) || ''
}

/** The GitHub issue a finding was filed as, parsed from its delivery URL. */
export function linkedIssueOf(deliveries: AutonomousAgentDelivery[]): { delivery?: AutonomousAgentDelivery; issue?: { repository: string; number: number } } {
  const delivery = deliveries.find(d => d.channel === 'github_issue' && d.external_url)
  const parsed = delivery?.external_url?.match(/github\.com\/([^/]+\/[^/]+)\/issues\/(\d+)/)
  return { delivery, issue: parsed ? { repository: parsed[1], number: Number(parsed[2]) } : undefined }
}
