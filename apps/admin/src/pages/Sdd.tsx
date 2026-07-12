import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { BookMarked, FileStack } from 'lucide-react'
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
import type {
  SddArtifact, SddArtifactKind, SddChange, SddPhase, SddSpec, SddStatus,
} from '../types'

const client = createClient()

const TAB_FOCUS =
  'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

export const SDD_PHASE_OPTIONS: SddPhase[] = [
  'explore', 'propose', 'spec', 'design', 'tasks', 'apply', 'verify', 'archive',
]

export const SDD_STATUS_OPTIONS: SddStatus[] = ['active', 'archived', 'abandoned']

export const SDD_STATUS_BADGE_VARIANT: Record<SddStatus, 'default' | 'success' | 'warning'> = {
  active: 'success',
  archived: 'default',
  abandoned: 'warning',
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
            className="flex items-center gap-1"
          >
            {i > 0 && <span aria-hidden="true" className="text-text-quaternary text-[10px]">→</span>}
            <Badge
              variant={present ? 'primary' : 'default'}
              size="sm"
              className={present ? undefined : 'opacity-40'}
            >
              {step}
            </Badge>
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
        <div>
          <h1 className="text-base font-semibold text-text-primary">SDD</h1>
          <p className="text-xs text-text-quaternary mt-0.5">
            {tab === 'changes'
              ? `${changes.length} changes`
              : `${specs.length} specifications`}
          </p>
        </div>
      </div>

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
      <div className="flex items-center gap-3 mb-4">
        <Select value={projectFilter} onValueChange={setProjectFilter}>
          <SelectTrigger className="w-48" aria-label="Project">
            <SelectValue placeholder="All projects" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="">All projects</SelectItem>
            {projects.map(p => (
              <SelectItem key={p.id} value={p.name}>{p.name}</SelectItem>
            ))}
          </SelectContent>
        </Select>

        {tab === 'changes' && (
          <>
            <Select value={phaseFilter} onValueChange={setPhaseFilter}>
              <SelectTrigger className="w-40" aria-label="Phase">
                <SelectValue placeholder="All phases" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">All phases</SelectItem>
                {SDD_PHASE_OPTIONS.map(p => (
                  <SelectItem key={p} value={p}>{p}</SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select value={statusFilter} onValueChange={setStatusFilter}>
              <SelectTrigger className="w-40" aria-label="Status">
                <SelectValue placeholder="All statuses" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">All statuses</SelectItem>
                {SDD_STATUS_OPTIONS.map(s => (
                  <SelectItem key={s} value={s}>{s}</SelectItem>
                ))}
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
              <div key={i} className="rounded-[18px] bg-[#272729] border border-border-primary h-14 animate-pulse" />
            ))}
          </div>
        ) : changes.length === 0 ? (
          <EmptyState
            icon={<FileStack />}
            title="No changes found"
            description="No SDD changes match the current filters. Changes are written by the harness and by git — the admin reads them."
          />
        ) : (
          <div className="overflow-hidden border border-border-primary rounded-[18px] bg-[#272729]">
            <table className="w-full border-collapse text-left">
              <thead className="bg-[#272729]/40 border-b border-border-secondary">
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
                      change.id === selectedId ? 'bg-background-tertiary/60' : 'hover:bg-background-tertiary/40'
                    }`}
                  >
                    <td className="px-4 py-3">
                      <p className="text-xs text-text-primary font-semibold">{change.name}</p>
                      {change.title && (
                        <p className="text-[10px] text-text-quaternary mt-0.5">{change.title}</p>
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
              <div key={i} className="rounded-[18px] bg-[#272729] border border-border-primary h-14 animate-pulse" />
            ))}
          </div>
        ) : specs.length === 0 ? (
          <EmptyState
            icon={<BookMarked />}
            title="No specifications found"
            description="No living specifications for this project yet. They live at openspec/specs/{capability}/spec.md and are written by the harness and by git — the admin reads them."
          />
        ) : (
          <div className="overflow-hidden border border-border-primary rounded-[18px] bg-[#272729]">
            <table className="w-full border-collapse text-left">
              <thead className="bg-[#272729]/40 border-b border-border-secondary">
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
                      spec.id === selectedSpecId ? 'bg-background-tertiary/60' : 'hover:bg-background-tertiary/40'
                    }`}
                  >
                    <td className="px-4 py-3">
                      <p className="text-xs text-text-primary font-semibold">{spec.capability}</p>
                      {spec.title && (
                        <p className="text-[10px] text-text-quaternary mt-0.5">{spec.title}</p>
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
