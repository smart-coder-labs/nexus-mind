import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { FileStack } from 'lucide-react'
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
import type { SddArtifact, SddArtifactKind, SddChange, SddPhase, SddStatus } from '../types'

const client = createClient()

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

export default function Sdd() {
  const { session } = useAuth()
  const isAdmin = isPrivileged(session?.user.role)
  const permissions = session?.user.permissions ?? []
  const canRead = isAdmin || permissions.includes('sdd:read')

  const [searchParams] = useSearchParams()
  const deepLinkedName = searchParams.get('change')

  const [projectFilter, setProjectFilter] = useState<string>('')
  const [phaseFilter, setPhaseFilter] = useState<string>('')
  const [statusFilter, setStatusFilter] = useState<string>('')

  // `null` means "the user has not clicked anything yet", which is what lets the
  // `?change=` deep link win on first paint and lose the moment the user clicks
  // a different row (or closes the drawer).
  const [openChangeId, setOpenChangeId] = useState<string | null>(null)
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

  const closeDetail = () => {
    setOpenChangeId(null)
    setDismissedDeepLink(true)
  }

  if (!canRead) return <Navigate to="/401" replace />

  return (
    <div className="p-6 max-w-6xl">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-base font-semibold text-text-primary">SDD</h1>
          <p className="text-xs text-text-quaternary mt-0.5">{changes.length} changes</p>
        </div>
      </div>

      {/* Filters */}
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
      </div>

      {/* List */}
      {isLoading ? (
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
    </div>
  )
}
