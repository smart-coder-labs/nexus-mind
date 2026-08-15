import { useMemo } from 'react'
import type { UsageSummaryRow } from '../../types'
import { CHART_PRIMARY } from './chartColors'
import { formatDuration } from './format'

export type RankMetric = 'tokens' | 'duration' | 'events'

export interface RankedBarsProps {
  rows: UsageSummaryRow[]
  metric: RankMetric
  /** Rows shown before the tail is folded into a single "Other" row. */
  limit?: number
  /** Copy for the zero-rows case. */
  emptyLabel: string
}

function valueOf(row: UsageSummaryRow, metric: RankMetric): number {
  if (metric === 'duration') return row.duration_ms
  if (metric === 'events') return row.event_count
  return row.tokens_total
}

function formatMetric(metric: RankMetric, v: number): string {
  return metric === 'duration' ? formatDuration(v) : v.toLocaleString()
}

/**
 * Ranked horizontal bars — the "compare magnitude" form.
 *
 * One hue for every bar, deliberately: length already encodes magnitude, so
 * stepping the colour by rank would double-encode the same variable and imply a
 * category difference that isn't there. Rank is read from the order.
 *
 * Bars are scaled against the largest row rather than the total, so the leader
 * always fills the track and the tail stays comparable.
 */
export function RankedBars({ rows, metric, limit = 8, emptyLabel }: RankedBarsProps) {
  const { items, total } = useMemo(() => {
    const sorted = [...rows].sort((a, b) => valueOf(b, metric) - valueOf(a, metric))
    const sum = sorted.reduce((acc, r) => acc + valueOf(r, metric), 0)

    const head = sorted.slice(0, limit).map(r => ({
      key: r.key_id ?? r.key_name,
      name: r.key_name,
      value: valueOf(r, metric),
    }))
    const tail = sorted.slice(limit)
    if (tail.length > 0) {
      head.push({
        key: '__other__',
        name: `Other (${tail.length})`,
        value: tail.reduce((acc, r) => acc + valueOf(r, metric), 0),
      })
    }
    return { items: head, total: sum }
  }, [rows, metric, limit])

  if (items.length === 0) {
    return <p className="text-[12.5px] text-text-tertiary text-center py-8">{emptyLabel}</p>
  }

  const max = Math.max(...items.map(i => i.value), 1)

  return (
    <ul className="flex flex-col gap-3">
      {items.map(item => {
        const pct = total > 0 ? (item.value / total) * 100 : 0
        return (
          <li key={item.key} className="flex flex-col gap-1.5">
            <div className="flex items-baseline gap-2.5">
              <span className="text-[12.5px] text-text-primary truncate min-w-0 flex-1">
                {item.name}
              </span>
              <span className="text-[11px] text-text-quaternary shrink-0 tabular-nums">
                {pct < 1 && pct > 0 ? '<1' : Math.round(pct)}%
              </span>
              <span className="text-[12.5px] font-semibold text-text-secondary shrink-0 tabular-nums">
                {formatMetric(metric, item.value)}
              </span>
            </div>
            {/* Track + fill. 4px rounded data-end, square at the baseline. */}
            <div className="h-[6px] rounded-[3px] bg-white/[0.045] overflow-hidden">
              <div
                className="h-full rounded-r-[3px]"
                style={{
                  width: `${Math.max((item.value / max) * 100, item.value > 0 ? 1.5 : 0)}%`,
                  backgroundColor: CHART_PRIMARY,
                  opacity: item.key === '__other__' ? 0.4 : 1,
                }}
              />
            </div>
          </li>
        )
      })}
    </ul>
  )
}
