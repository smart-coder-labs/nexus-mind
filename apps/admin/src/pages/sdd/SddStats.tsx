import { ListChecks, CheckCircle2, FileText, LayoutGrid, ChevronRight } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { PHASE_STEPS, GLASS_PANEL } from '../Sdd'
import type { SddChange } from '../../types'
import { StatTile } from '../dashboard/StatTile'
import { accentFor } from '../dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'

/**
 * Compact metric disclosures and pipeline summary for the SDD page. Every number is derived from the already-fetched changes
 * list passed in by Sdd.tsx (the same list backing "N changes" in the page
 * header) — no separate endpoint, no fabricated figures.
 *
 * A change's "current phase" is the FURTHEST pipeline step whose artifact
 * exists on it — same ground-truth-by-inventory rule `PhasePipeline` uses,
 * not the advisory `phase` column (see the comment on `PHASE_STEPS` in
 * Sdd.tsx). "In tasks" / "in verify" count changes currently sitting at
 * that step, not ones that have merely passed through it on their way
 * further along the pipeline.
 *
 * `TASKS LINKED` from the mockup is intentionally omitted: `SddChange.task_links`
 * is "hydrated on detail reads only" (see types.ts) — on the list read that backs
 * this page it is always `[]`, so a tile built from it would show a fabricated
 * zero rather than a real count. It can return once the list endpoint carries a
 * real linked-task count.
 */

function currentPhaseStep(change: SddChange): string | null {
  const kinds = new Set(change.artifacts.map(a => a.kind))
  let current: string | null = null
  for (const { step, kind } of PHASE_STEPS) {
    if (kinds.has(kind)) current = step
  }
  return current
}

interface StatTileData {
  key: string
  label: string
  value: number
  sub?: string
  icon: LucideIcon
}

interface SddStatsProps {
  changes: SddChange[]
}

export default function SddStats({ changes }: SddStatsProps) {
  const total = changes.length
  const pct = (n: number) => (total > 0 ? Math.round((n / total) * 100) : 0)

  const inTasks = changes.filter(c => currentPhaseStep(c) === 'tasks').length
  const inVerify = changes.filter(c => currentPhaseStep(c) === 'verify').length
  const specsWritten = changes.filter(c => c.artifacts.some(a => a.kind === 'spec')).length

  const tiles: StatTileData[] = [
    {
      key: 'in-tasks',
      label: 'in tasks',
      value: inTasks,
      sub: total > 0 ? `${pct(inTasks)}% of total` : undefined,
      icon: ListChecks,
    },
    {
      key: 'in-verify',
      label: 'in verify',
      value: inVerify,
      sub: total > 0 ? `${pct(inVerify)}% of total` : undefined,
      icon: CheckCircle2,
    },
    {
      key: 'specs-written',
      label: 'specs written',
      value: specsWritten,
      sub: total > 0 ? `${pct(specsWritten)}% of total` : undefined,
      icon: FileText,
    },
    {
      key: 'total-changes',
      label: 'total changes',
      value: total,
      icon: LayoutGrid,
    },
  ]

  return (
    <div className="space-y-3 mb-4">
      <KpiMarquee compact role="list" aria-label="SDD stats">
        {tiles.map((tile, index) => <StatTile key={tile.key} label={tile.label} value={tile.value.toLocaleString()} sub={tile.sub} icon={tile.icon} accent={accentFor(index)} />)}
      </KpiMarquee>

      {/* Pipeline summary — how many changes have an artifact for each of the
          six steps, same artifact-inventory rule as the per-row PhasePipeline. */}
      <div className={`flex items-center gap-3 flex-wrap rounded-xl px-4 py-3 ${GLASS_PANEL}`}>
        <span className="text-xs font-medium text-muted-foreground shrink-0">
          Pipeline
        </span>
        <div className="flex items-center gap-1.5 flex-wrap flex-1" data-testid="sdd-pipeline-summary">
          {PHASE_STEPS.map(({ step, kind }, i) => {
            const count = changes.filter(c => c.artifacts.some(a => a.kind === kind)).length
            return (
              <div key={step} className="flex items-center gap-1.5">
                <span className="inline-flex items-center gap-1.5 min-h-6 px-2 rounded-md border border-border-primary bg-surface-secondary">
                  <span className="text-[11px] font-semibold text-text-secondary">{step}</span>
                  <span className="text-[10.5px] font-bold text-text-secondary tabular-nums">{count}</span>
                </span>
                {i < PHASE_STEPS.length - 1 && (
                  <ChevronRight aria-hidden="true" className="w-3 h-3 text-text-quaternary shrink-0" />
                )}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
