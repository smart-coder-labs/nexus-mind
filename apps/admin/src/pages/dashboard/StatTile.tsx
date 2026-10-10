import { lazy, Suspense, type ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'
import { MetricSummary } from '@/components/ui/MetricSummary'
const MetricSeries = lazy(() => import('./MetricSeries'))

export interface StatTileProps {
  label: string
  value: string
  sub?: ReactNode
  icon: LucideIcon
  accent?: string
  /** Actual recent samples, oldest first. No interpolated or invented points. */
  sparkline?: number[]
}

export function StatTile({ label, value, sub, icon, accent, sparkline }: StatTileProps) {
  const samples = sparkline && sparkline.length > 1 ? sparkline.slice(-8) : null
  return (
    <div role="listitem" data-metric-label={label} data-metric-value={value} data-metric-accent={accent}>
      <MetricSummary label={label} value={value} icon={icon} accent={accent}>
        {(sub || samples) ? <>
          {sub && <div>{sub}</div>}
          {samples && <Suspense fallback={<div className="h-8 w-24 animate-pulse rounded-md bg-muted" aria-label="Loading recent samples" />}><MetricSeries label={label} samples={samples} /></Suspense>}
        </> : undefined}
      </MetricSummary>
    </div>
  )
}
