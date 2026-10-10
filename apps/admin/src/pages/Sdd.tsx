import { type ReactNode, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Activity, Archive, BookMarked, ClipboardCheck, FileStack, FlaskConical, Lightbulb, ListChecks, Palette, Rocket, Search } from 'lucide-react'
import { Navigate, useSearchParams } from 'react-router-dom'
import { createClient } from '../api/client'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import { Modal } from '../components/ui/Modal/Modal'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import { Badge } from '../components/ui/Badge/Badge'
import { EmptyState } from '../components/ui/EmptyState/EmptyState'
import ChangeDetail from './sdd/ChangeDetail'
import SpecDetail from './sdd/SpecDetail'
import SddStats from './sdd/SddStats'
import type {
  SddArtifact, SddArtifactKind, SddChange, SddPhase, SddSpec, SddStatus,
} from '../types'

const client = createClient()

const TAB_FOCUS =
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

/** True glassmorphic panel — a translucent near-black tint with a blurred
 *  backdrop, matching the target mockup (and the same recipe Layout.tsx's
 *  sidebar and OrgMemoryGraph's panels already use: `#0d0f14` alpha-blended
 *  + backdrop-blur). Deliberately NOT the flat opaque `bg-surface-secondary` surface
 *  token used elsewhere in the admin — that reads as a plain gray card, not
 *  glass. */
export const GLASS_PANEL =
  'border border-border-primary bg-surface-primary '

export const SDD_PHASE_OPTIONS: SddPhase[] = [
  'explore', 'propose', 'spec', 'design', 'tasks', 'apply', 'verify', 'archive',
]

export const SDD_STATUS_OPTIONS: SddStatus[] = ['active', 'archived', 'abandoned']

export const SDD_STATUS_BADGE_VARIANT: Record<SddStatus, 'default' | 'success' | 'warning'> = {
  active: 'success',
  archived: 'default',
  abandoned: 'warning',
}

const PHASE_PRESENTATION: Record<SddPhase, { description: string; color: string; icon: ReactNode }> = {
  explore: { description: 'Gather context, constraints, and the problem to solve.', color: 'var(--data-blue)', icon: <Search /> },
  propose: { description: 'Compare approaches and agree on a direction.', color: 'var(--data-amber)', icon: <Lightbulb /> },
  spec: { description: 'Define requirements and observable acceptance criteria.', color: 'var(--data-blue)', icon: <FileStack /> },
  design: { description: 'Plan the architecture, interfaces, and experience.', color: 'var(--chart-5)', icon: <Palette /> },
  tasks: { description: 'Break the approved change into actionable work.', color: 'var(--data-teal)', icon: <ListChecks /> },
  apply: { description: 'Implement the planned change in the codebase.', color: 'var(--color-status-info)', icon: <Rocket /> },
  verify: { description: 'Run checks and confirm the change meets its goals.', color: 'var(--color-status-success)', icon: <ClipboardCheck /> },
  archive: { description: 'Record the outcome and preserve completed work.', color: 'var(--muted-foreground)', icon: <Archive /> },
}

const STATUS_PRESENTATION: Record<SddStatus, { description: string; color: string; icon: ReactNode }> = {
  active: { description: 'In progress and still receiving updates.', color: 'var(--color-status-success)', icon: <Activity /> },
  archived: { description: 'Completed and kept for reference.', color: 'var(--muted-foreground)', icon: <Archive /> },
  abandoned: { description: 'Stopped before completion.', color: 'var(--color-status-warning)', icon: <FlaskConical /> },
}

export function SddPhaseOption({ phase }: { phase: SddPhase }) {
  const option = PHASE_PRESENTATION[phase]
  return <SelectItem value={phase} icon={option.icon} indicatorColor={option.color} description={option.description}>{phase}</SelectItem>
}

export function SddStatusOption({ status }: { status: SddStatus }) {
  const option = STATUS_PRESENTATION[status]
  return <SelectItem value={status} icon={option.icon} indicatorColor={option.color} description={option.description}>{status}</SelectItem>
}

/**
 * The pipeline the admin shows. Six user-facing steps, each backed by the
 * artifact kind whose existence proves the step was actually done.
 *
 * The change's own `phase` column is **advisory** — an agent may forget to bump
 * it, and a change sitting at `phase: "spec"` can already have a design and a
 * tasks document on disk. The artifact inventory is the ground truth, so that is
 * what we render. (`exploration`, `archive-report` and `state` have no step;
 * they are not part of the six-step display.)
 */
export const PHASE_STEPS: { step: string; kind: SddArtifactKind }[] = [
  { step: 'propose', kind: 'proposal' },
  { step: 'spec',    kind: 'spec' },
  { step: 'design',  kind: 'design' },
  { step: 'tasks',   kind: 'tasks' },
  { step: 'apply',   kind: 'apply-progress' },
  { step: 'verify',  kind: 'verify-report' },
]

export function PhasePipeline({ artifacts }: { artifacts: SddArtifact[] }) {
  const kinds = new Set((artifacts ?? []).map(a => a.kind))

  return (
    <div data-testid="phase-pipeline" className="flex items-center gap-1 flex-wrap">
      {PHASE_STEPS.map(({ step, kind }, i) => {
        const present = kinds.has(kind)
        return (
          <span
            key={step}
            data-testid={`phase-step-${step}`}
            data-present={present ? 'true' : 'false'}
            className="flex items-center gap-0.5"
          >
            {i > 0 && <span aria-hidden="true" className="text-text-quaternary text-[9px] mx-0.5">→</span>}
            <span
              className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-semibold ${
                present
                  ? 'border-accent-blue/30 bg-action-primary/10 text-accent-blue'
                  : 'border-border-primary text-text-quaternary/60'
              }`}
            >
              {step}
            </span>
          </span>
        )
      })}
    </div>
  )
}

/**
 * `openspec/` has two trees and the page shows both:
 *
 *   * **Changes** — `openspec/changes/{name}/`, the work in flight.
 *   * **Specs**   — `openspec/specs/{capability}/spec.md`, the LIVING SPECIFICATION.
 *     The source of truth. `sdd-archive` merges a change's delta specs into it when
 *     the change closes, which is why each row names the change that last did so.
 *
 * They are separate views rather than one list because they are separate entities: a
 * spec is not an artifact of a change, and it outlives the changes that amend it.
 */
type SddTab = 'changes' | 'specs'

export default function Sdd() {
  const { session } = useAuth()
  const isAdmin = isPrivileged(session?.user.role)
  const permissions = session?.user.permissions ?? []
  const canRead = isAdmin || permissions.includes('sdd:read')

  const [searchParams] = useSearchParams()
  const deepLinkedName = searchParams.get('change')
  const deepLinkedSpecId = searchParams.get('spec')
  const deepLinkedTab = searchParams.get('tab')

  const [activeTab, setActiveTab] = useState<SddTab | null>(null)
  // A `?spec=` or `?tab=specs` deep link opens the Specs view on first paint, and
  // loses the moment the user picks a tab themselves.
  const tab: SddTab =
    activeTab ?? (deepLinkedTab === 'specs' || deepLinkedSpecId ? 'specs' : 'changes')

  const [projectFilter, setProjectFilter] = useState<string>('')
  const [phaseFilter, setPhaseFilter] = useState<string>('')
  const [statusFilter, setStatusFilter] = useState<string>('')

  // `null` means "the user has not clicked anything yet", which is what lets the
  // `?change=` deep link win on first paint and lose the moment the user clicks
  // a different row (or closes the drawer).
  const [openChangeId, setOpenChangeId] = useState<string | null>(null)
  const [openSpecId, setOpenSpecId] = useState<string | null>(null)
  const [dismissedDeepLink, setDismissedDeepLink] = useState(false)

  const filters = useMemo(
    () => ({
      project: projectFilter || undefined,
      phase: phaseFilter ? (phaseFilter as SddPhase) : undefined,
      status: statusFilter ? (statusFilter as SddStatus) : undefined,
    }),
    [projectFilter, phaseFilter, statusFilter],
  )

  const { data: changes = [], isLoading } = useQuery({
    queryKey: ['sdd-changes', filters],
    queryFn: () => client.listSddChanges(filters),
    enabled: canRead,
  })

  // Metadata only — the list read never carries a contract's text. Gated on sdd:read
  // like every other query here: an ungated 403 trips the client's global handler and
  // redirects the whole app to /401.
  const { data: specs = [], isLoading: specsLoading } = useQuery({
    queryKey: ['sdd-specs', projectFilter || undefined],
    queryFn: () => client.listSddSpecs({ project: projectFilter || undefined }),
    enabled: canRead,
  })

  const { data: projects = [] } = useQuery({
    queryKey: ['projects'],
    queryFn: () => client.listProjects(),
    enabled: canRead,
  })

  // A deep-linked name that matches no change is inert — no selection, no fetch,
  // no error. Renames leave dangling links behind and they must not break a page.
  const deepLinkedId =
    !dismissedDeepLink && deepLinkedName
      ? changes.find(c => c.name === deepLinkedName)?.id ?? null
      : null
  const selectedId = openChangeId ?? deepLinkedId
  const selectedChange: SddChange | undefined = changes.find(c => c.id === selectedId)

  const selectedSpecId =
    openSpecId ?? (!dismissedDeepLink && deepLinkedSpecId ? deepLinkedSpecId : null)
  const selectedSpec: SddSpec | undefined = specs.find(s => s.id === selectedSpecId)

  const closeDetail = () => {
    setOpenChangeId(null)
    setDismissedDeepLink(true)
  }

  const closeSpecDetail = () => {
    setOpenSpecId(null)
    setDismissedDeepLink(true)
  }

  const selectTab = (next: SddTab) => {
    setActiveTab(next)
    setDismissedDeepLink(true)
    setOpenChangeId(null)
    setOpenSpecId(null)
  }

  if (!canRead) return <Navigate to="/401" replace />

  return (
    <div className="p-6 max-w-6xl">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <div
            aria-hidden="true"
            className="w-11 h-11 rounded-xl bg-accent-purple/10 flex items-center justify-center shrink-0"
          >
            <FileStack className="w-5 h-5 text-accent-purple" />
          </div>
          <div>
            <h1 className="text-[22px] leading-tight font-semibold text-text-primary">SDD</h1>
            <p className="text-xs text-text-quaternary mt-0.5">
              {tab === 'changes'
                ? `${changes.length} changes`
                : `${specs.length} specifications`}
              {' — spec-driven development'}
            </p>
          </div>
        </div>
      </div>

      {/* Stat tiles + pipeline summary — derived from the already-fetched,
          filter-scoped changes list (same data backing "N changes" above). No
          separate endpoint, no fabricated numbers. Change-lifecycle stats only,
          so they sit above the Changes view and not the Specs one. */}
      {tab === 'changes' && <SddStats changes={changes} />}

      {/* The two trees */}
      <div role="tablist" aria-label="SDD" className="flex items-center gap-1 border-b border-border-secondary mb-4">
        {([
          { id: 'changes' as const, label: 'Changes' },
          { id: 'specs' as const,   label: 'Specs' },
        ]).map(({ id, label }) => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            onClick={() => selectTab(id)}
            className={`px-3 py-1.5 text-xs transition-colors ${TAB_FOCUS} ${
              tab === id
                ? 'text-text-primary font-semibold border-b-2 border-accent-blue'
                : 'text-text-quaternary hover:text-text-secondary'
            }`}
          >
            {label}
          </button>
        ))}
      </div>

      {/* Filters. Phase and status describe a CHANGE's lifecycle — a living
          specification has neither, so they are not offered on the specs view. */}
      <div className="flex flex-wrap items-center gap-2 mb-4">
        <Select value={projectFilter} onValueChange={setProjectFilter}>
          <SelectTrigger className="w-full min-w-0 sm:w-48" aria-label="Project">
            <SelectValue placeholder="All projects" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="" icon={<FileStack />} description="Show changes from every project">All projects</SelectItem>
            {projects.map(p => (
              <SelectItem key={p.id} value={p.name} icon={<FileStack />} description={p.description || `Changes and specifications for ${p.name}`}>{p.name}</SelectItem>
            ))}
          </SelectContent>
        </Select>

        {tab === 'changes' && (
          <>
            <Select value={phaseFilter} onValueChange={setPhaseFilter}>
              <SelectTrigger className="w-full min-w-0 sm:w-40" aria-label="Phase">
                <SelectValue placeholder="All phases" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="" icon={<ListChecks />} description="Show changes at every stage">All phases</SelectItem>
                {SDD_PHASE_OPTIONS.map(phase => <SddPhaseOption key={phase} phase={phase} />)}
              </SelectContent>
            </Select>

            <Select value={statusFilter} onValueChange={setStatusFilter}>
              <SelectTrigger className="w-full min-w-0 sm:w-40" aria-label="Status">
                <SelectValue placeholder="All statuses" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="" icon={<Activity />} description="Show active and completed changes">All statuses</SelectItem>
                {SDD_STATUS_OPTIONS.map(status => <SddStatusOption key={status} status={status} />)}
              </SelectContent>
            </Select>
          </>
        )}
      </div>

      {/* Changes */}
      {tab === 'changes' && (
        isLoading ? (
          <div data-testid="sdd-skeleton" className="space-y-2">
            {[...Array(4)].map((_, i) => (
              <div key={i} className={`rounded-xl h-14 animate-pulse ${GLASS_PANEL}`} />
            ))}
          </div>
        ) : changes.length === 0 ? (
          <EmptyState
            icon={<FileStack />}
            title="No changes found"
            description="No SDD changes match the current filters. Changes are written by the harness and by git — the admin reads them."
          />
        ) : (
          <div className={`overflow-x-auto rounded-xl ${GLASS_PANEL}`}>
            <table className="admin-data-table w-full min-w-[760px] border-collapse text-left">
              <thead className="border-b border-border-primary">
                <tr>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Change</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Project</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Status</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Pipeline</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Updated</th>
                </tr>
              </thead>
              <tbody>
                {changes.map(change => (
                  <tr
                    key={change.id}
                    aria-selected={change.id === selectedId}
                    onClick={() => { setDismissedDeepLink(true); setOpenChangeId(change.id) }}
                    className={`border-b border-border-secondary last:border-b-0 cursor-pointer transition-colors ${
                      change.id === selectedId ? 'bg-action-primary/10' : 'hover:bg-action-primary/[0.05]'
                    }`}
                  >
                    <td className="px-4 py-3">
                      <p className="text-xs text-text-primary font-semibold">{change.name}</p>
                      {change.title && (
                        <p className="text-xs text-text-quaternary mt-0.5">{change.title}</p>
                      )}
                    </td>
                    <td className="px-4 py-3 text-xs text-text-secondary">{change.project}</td>
                    <td className="px-4 py-3">
                      <Badge variant={SDD_STATUS_BADGE_VARIANT[change.status] ?? 'default'} size="sm">
                        {change.status}
                      </Badge>
                    </td>
                    <td className="px-4 py-3">
                      <PhasePipeline artifacts={change.artifacts} />
                    </td>
                    <td className="px-4 py-3 text-xs text-text-secondary">
                      {new Date(change.updated_at).toLocaleDateString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {/* Specs — one row per capability. The contract, not the drafts. */}
      {tab === 'specs' && (
        specsLoading ? (
          <div data-testid="sdd-specs-skeleton" className="space-y-2">
            {[...Array(3)].map((_, i) => (
              <div key={i} className={`rounded-xl h-14 animate-pulse ${GLASS_PANEL}`} />
            ))}
          </div>
        ) : specs.length === 0 ? (
          <EmptyState
            icon={<BookMarked />}
            title="No specifications found"
            description="No living specifications for this project yet. They live at openspec/specs/{capability}/spec.md and are written by the harness and by git — the admin reads them."
          />
        ) : (
          <div className={`overflow-x-auto rounded-xl ${GLASS_PANEL}`}>
            <table className="admin-data-table w-full min-w-[760px] border-collapse text-left">
              <thead className="border-b border-border-primary">
                <tr>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Capability</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Project</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Revision</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Last Merged From</th>
                  <th className="px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide">Updated</th>
                </tr>
              </thead>
              <tbody>
                {specs.map(spec => (
                  <tr
                    key={spec.id}
                    aria-selected={spec.id === selectedSpecId}
                    onClick={() => { setDismissedDeepLink(true); setOpenSpecId(spec.id) }}
                    className={`border-b border-border-secondary last:border-b-0 cursor-pointer transition-colors ${
                      spec.id === selectedSpecId ? 'bg-action-primary/10' : 'hover:bg-action-primary/[0.05]'
                    }`}
                  >
                    <td className="px-4 py-3">
                      <p className="text-xs text-text-primary font-semibold">{spec.capability}</p>
                      {spec.title && (
                        <p className="text-xs text-text-quaternary mt-0.5">{spec.title}</p>
                      )}
                    </td>
                    <td className="px-4 py-3 text-xs text-text-secondary">{spec.project}</td>
                    <td className="px-4 py-3">
                      <Badge variant="primary" size="sm">rev {spec.latest_revision}</Badge>
                    </td>
                    <td className="px-4 py-3 text-xs text-text-secondary">
                      {spec.last_merged_from_change_name ?? (
                        <span className="text-text-quaternary">—</span>
                      )}
                    </td>
                    <td className="px-4 py-3 text-xs text-text-secondary">
                      {new Date(spec.updated_at).toLocaleDateString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {/* Change detail — a right-side drawer over the list */}
      <Modal
        open={!!selectedChange}
        onOpenChange={(open) => { if (!open) closeDetail() }}
        position="right"
        size="lg"
      >
        {selectedChange && (
          <ChangeDetail changeId={selectedChange.id} onClose={closeDetail} />
        )}
      </Modal>

      {/* Spec detail — the same drawer, over the specs list */}
      <Modal
        open={!!selectedSpec}
        onOpenChange={(open) => { if (!open) closeSpecDetail() }}
        position="right"
        size="lg"
      >
        {selectedSpec && (
          <SpecDetail specId={selectedSpec.id} onClose={closeSpecDetail} />
        )}
      </Modal>
    </div>
  )
}
