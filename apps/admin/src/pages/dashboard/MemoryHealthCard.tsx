import { cn } from '@/lib/utils'
import { DASHBOARD_ACCENTS } from './colors'

export interface MemoryHealthCardProps {
  total: number | undefined
  duplicates: number | undefined
  stale: number | undefined
  untagged: number | undefined
}

const RADIUS = 46
const CIRCUMFERENCE = 2 * Math.PI * RADIUS

/**
 * Design delta 4: donut ring + legend. The "healthy %" is derived from the
 * real health counters already returned by GET /v1/admin/memories/health —
 * healthy = total minus duplicates/stale/untagged, clamped to [0, 100].
 * This is a computed ratio of real numbers, not a fabricated figure.
 */
export function MemoryHealthCard({ total, duplicates, stale, untagged }: MemoryHealthCardProps) {
  const hasData = total != null
  const t = total ?? 0
  const d = duplicates ?? 0
  const s = stale ?? 0
  const u = untagged ?? 0
  const healthyPct = hasData && t > 0
    ? Math.max(0, Math.min(100, Math.round((1 - (d + s + u) / t) * 100)))
    : 0
  const dash = (healthyPct / 100) * CIRCUMFERENCE

  const rows = [
    { label: 'Total memories', value: total, color: DASHBOARD_ACCENTS[0] },
    { label: 'Duplicates', value: duplicates, color: DASHBOARD_ACCENTS[2] },
    { label: 'Stale (>30d)', value: stale, color: DASHBOARD_ACCENTS[4] },
    { label: 'Untagged', value: untagged, color: DASHBOARD_ACCENTS[3] },
  ]

  return (
    <div className="flex items-center gap-5">
      <div className="relative w-[110px] h-[110px] shrink-0">
        <svg width="110" height="110" viewBox="0 0 110 110">
          <circle cx="55" cy="55" r={RADIUS} fill="none" stroke="rgba(255,255,255,0.06)" strokeWidth="9" />
          {hasData && (
            <circle
              cx="55"
              cy="55"
              r={RADIUS}
              fill="none"
              stroke="var(--color-status-success)"
              strokeWidth="9"
              strokeLinecap="round"
              strokeDasharray={`${dash} ${CIRCUMFERENCE}`}
              transform="rotate(-90 55 55)"
              className="transition-[stroke-dasharray] duration-500"
            />
          )}
        </svg>
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-0.5">
          <span className="text-[24px] font-semibold tracking-[-0.02em] text-text-primary tabular-nums">
            {hasData ? `${healthyPct}%` : '—'}
          </span>
          <span className="text-[10.5px] text-text-tertiary">healthy</span>
        </div>
      </div>
      <div className="flex flex-col gap-2.5 flex-1 min-w-0">
        {rows.map(row => (
          <div key={row.label} className="flex items-center gap-2.5">
            <span className={cn('w-2 h-2 rounded-full shrink-0')} style={{ backgroundColor: row.color }} />
            <span className="text-[12.5px] text-text-secondary flex-1 min-w-0 truncate">{row.label}</span>
            <span className="text-[13px] font-semibold text-text-primary tabular-nums">
              {row.value != null ? row.value.toLocaleString() : '—'}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
