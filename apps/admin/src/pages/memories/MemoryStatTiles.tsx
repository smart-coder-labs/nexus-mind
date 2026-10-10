import { TrendingUp, Pin, Copy, Hash, Database } from 'lucide-react'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { StatTile } from '../dashboard/StatTile'
import { accentFor } from '../dashboard/colors'

export interface MemoryStatTilesProps {
  /** Memories created in the last 7 days — GET /v1/admin/stats/trends. */
  weekCount: number | undefined
  /** Average memories/day over the last 7 days, derived from weekCount. */
  weekAvgPerDay: number | undefined
  /** Daily counts for the last up-to-7 days, oldest → newest, for the bar sparkline. */
  weekSparkline: number[]
  /** Total pinned memories — see comment at the call site in Memories.tsx for
   *  why this requires a client-side scan (no backend aggregate exists). */
  pinnedCount: number | undefined
  pinnedPctOfTotal: number | undefined
  /** GET /v1/admin/memories/health — duplicate_count + duplicate group count. */
  duplicateCount: number | undefined
  duplicateGroupCount: number | undefined
  /** GET /v1/admin/memories/health — untagged_count + % of total. */
  untaggedCount: number | undefined
  untaggedPctOfTotal: number | undefined
  /** GET /v1/admin/memories/health — total_memories + this week's delta. */
  totalCount: number | undefined
  totalThisWeek: number | undefined
}

const fmt = (n: number | undefined) => n != null ? n.toLocaleString() : '—'

export function MemoryStatTiles(props: MemoryStatTilesProps) {
  const tiles = [
    { label: 'This week', icon: TrendingUp, value: props.weekCount, sub: props.weekAvgPerDay != null ? `${props.weekAvgPerDay} avg/day` : undefined, sparkline: props.weekSparkline },
    { label: 'Pinned', icon: Pin, value: props.pinnedCount, sub: props.pinnedPctOfTotal != null ? `${props.pinnedPctOfTotal}% of total` : undefined },
    { label: 'Duplicates', icon: Copy, value: props.duplicateCount, sub: props.duplicateGroupCount != null ? `${props.duplicateGroupCount} groups` : undefined },
    { label: 'Untagged', icon: Hash, value: props.untaggedCount, sub: props.untaggedPctOfTotal != null ? `${props.untaggedPctOfTotal}% of total` : undefined },
    { label: 'Total', icon: Database, value: props.totalCount, sub: props.totalThisWeek != null ? `+${props.totalThisWeek} this week` : undefined },
  ]

  return <KpiMarquee compact dockOnScroll role="list" aria-label="Memory statistics">
    {tiles.map((tile, index) => <StatTile key={tile.label} {...tile} value={fmt(tile.value)} accent={accentFor(index)} />)}
  </KpiMarquee>
}
