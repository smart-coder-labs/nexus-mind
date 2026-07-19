import { TrendingUp, Pin, Copy, Hash, Database } from 'lucide-react'
import { cn } from '@/lib/utils'
import { KpiMarquee } from '@/components/ui/KpiMarquee'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Memories page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// The app's design tokens (src/index.css) define exactly five accent hues
// (accent-blue, accent-purple, status-success, status-warning, status-error)
// — there is no dedicated teal/orange token. Per this task's hard constraint
// ("do not touch global CSS"), this tile row reuses those five existing CSS
// custom properties in a fixed order instead of inventing new colors, mirroring
// the same documented tradeoff already made in src/pages/dashboard/colors.ts.
const TILE_ACCENTS = {
  week:       'var(--color-status-success)', // closest existing hue to the mock's teal
  pinned:     'var(--color-status-warning)',
  duplicates: 'var(--color-status-error)',
  untagged:   'var(--color-accent-purple)',  // closest existing hue to the mock's orange
  total:      'var(--color-accent-blue)',
} as const

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

function fmt(n: number | undefined): string {
  return n != null ? n.toLocaleString() : '—'
}

function Glow({ color }: { color: string }) {
  return (
    <div
      aria-hidden="true"
      className="absolute -top-10 -right-8 w-[110px] h-[110px] rounded-full pointer-events-none"
      style={{ background: color, opacity: 0.16, filter: 'blur(30px)' }}
    />
  )
}

/** Bar sparkline — only rendered where we have genuine time-series data
 *  (the WEEK tile, from `daily_counts`). The other tiles are point-in-time
 *  aggregates with no backing history endpoint, so per the task's "real
 *  data only, do not fabricate" rule they render without a sparkline
 *  instead of a fake/static one. */
function Sparkline({ values, color }: { values: number[]; color: string }) {
  const max = Math.max(1, ...values)
  return (
    <div className="flex items-end gap-[3px] h-[18px] shrink-0">
      {values.map((v, i) => (
        <div
          key={i}
          className="w-[3.5px] rounded-[1.5px]"
          style={{ height: `${Math.max(8, (v / max) * 100)}%`, background: color, opacity: 0.85 }}
        />
      ))}
    </div>
  )
}

function Tile({
  label,
  icon,
  value,
  caption,
  captionClassName,
  accent,
  sparkline,
}: {
  label: string
  icon: React.ReactNode
  value: string
  caption?: string
  captionClassName?: string
  accent: string
  sparkline?: number[]
}) {
  return (
    <div className={`relative flex flex-col gap-2 rounded-[13px] overflow-hidden px-4 py-3.5 ${GLASS_PANEL}`}>
      <Glow color={accent} />
      <div className="flex items-center justify-between gap-2 relative">
        <span className="text-[11px] font-bold tracking-wide uppercase text-text-quaternary">{label}</span>
        <span style={{ color: accent }} className="shrink-0">{icon}</span>
      </div>
      <div className="flex items-end justify-between gap-2 relative">
        <div className="flex flex-col gap-0.5 min-w-0">
          <span className="text-lg font-extrabold tracking-tight text-text-primary leading-none tabular-nums">
            {value}
          </span>
          {caption && (
            <span className={cn('text-[11.5px]', captionClassName ?? 'text-text-quaternary')}>{caption}</span>
          )}
        </div>
        {sparkline && sparkline.length > 0 && <Sparkline values={sparkline} color={accent} />}
      </div>
    </div>
  )
}

/**
 * Five gradient glass stat tiles between the Memories header and the tabs
 * (design delta 1 — see task spec). Every number is sourced from an
 * endpoint the app already calls; nothing here is fabricated. See
 * Memories.tsx for exactly which queries feed each prop.
 */
export function MemoryStatTiles(props: MemoryStatTilesProps) {
  const {
    weekCount, weekAvgPerDay, weekSparkline,
    pinnedCount, pinnedPctOfTotal,
    duplicateCount, duplicateGroupCount,
    untaggedCount, untaggedPctOfTotal,
    totalCount, totalThisWeek,
  } = props

  return (
    <KpiMarquee>
      <div key="this-week" className="w-[232px] flex-none">
        <Tile
          label="This week"
          icon={<TrendingUp className="w-3.5 h-3.5" />}
          value={fmt(weekCount)}
          caption={weekAvgPerDay != null ? `${weekAvgPerDay} avg/day` : undefined}
          accent={TILE_ACCENTS.week}
          sparkline={weekSparkline}
        />
      </div>
      <div key="pinned" className="w-[232px] flex-none">
        <Tile
          label="Pinned"
          icon={<Pin className="w-3.5 h-3.5" />}
          value={fmt(pinnedCount)}
          // No pinned_at timestamp exists server-side, so a "N this week" delta
          // (like the mock shows) can't be computed without fabricating a number.
          // "% of total" is the closest stat we can compute honestly from real data.
          caption={pinnedPctOfTotal != null ? `${pinnedPctOfTotal}% of total` : undefined}
          accent={TILE_ACCENTS.pinned}
        />
      </div>
      <div key="duplicates" className="w-[232px] flex-none">
        <Tile
          label="Duplicates"
          icon={<Copy className="w-3.5 h-3.5" />}
          value={fmt(duplicateCount)}
          caption={duplicateGroupCount != null ? `${duplicateGroupCount} group${duplicateGroupCount === 1 ? '' : 's'}` : undefined}
          accent={TILE_ACCENTS.duplicates}
        />
      </div>
      <div key="untagged" className="w-[232px] flex-none">
        <Tile
          label="Untagged"
          icon={<Hash className="w-3.5 h-3.5" />}
          value={fmt(untaggedCount)}
          caption={untaggedPctOfTotal != null ? `${untaggedPctOfTotal}% of total` : undefined}
          accent={TILE_ACCENTS.untagged}
        />
      </div>
      <div key="total" className="w-[232px] flex-none">
        <Tile
          label="Total"
          icon={<Database className="w-3.5 h-3.5" />}
          value={fmt(totalCount)}
          caption={totalThisWeek != null ? `+${totalThisWeek} this week` : undefined}
          captionClassName="text-status-success"
          accent={TILE_ACCENTS.total}
        />
      </div>
    </KpiMarquee>
  )
}
