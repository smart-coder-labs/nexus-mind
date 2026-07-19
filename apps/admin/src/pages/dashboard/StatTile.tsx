import type { LucideIcon } from 'lucide-react'
import { cn } from '@/lib/utils'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx (translucent near-black
// tint + backdrop blur) — inlined rather than imported to avoid pulling the
// entire SDD page module graph into every page that renders a stat tile.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

export interface StatTileProps {
  label: string
  value: string
  sub?: string
  icon: LucideIcon
  accent: string
  /** Recent daily counts, most recent last. Omitted entirely when the
   *  metric has no real per-day series (never fabricated). */
  sparkline?: number[]
}

/**
 * Rebuilt stat tile (design delta 1): tinted glow background, uppercase
 * label, icon in a tinted rounded square top-right, big number, sub-caption,
 * and an optional mini bar-sparkline bottom-right — sparkline is only
 * rendered when real per-day data was passed in.
 */
export function StatTile({ label, value, sub, icon: Icon, accent, sparkline }: StatTileProps) {
  const bars = sparkline && sparkline.length > 1 ? sparkline.slice(-8) : null
  const maxBar = bars ? Math.max(...bars, 1) : 1

  return (
    <div
      role="listitem"
      className={`relative flex flex-col gap-2.5 rounded-[18px] p-5 overflow-hidden transition-colors hover:border-white/[0.16] ${GLASS_PANEL}`}
    >
      {/* Decorative glow blob, tinted by the metric's accent color */}
      <div
        aria-hidden="true"
        className="absolute -top-11 -right-9 w-32 h-32 rounded-full pointer-events-none"
        style={{ background: accent, opacity: 0.16, filter: 'blur(34px)' }}
      />

      <div className="flex items-center justify-between gap-2 relative">
        <span className="text-[11px] font-semibold tracking-[0.06em] uppercase text-text-tertiary truncate">
          {label}
        </span>
        <div
          className="w-[30px] h-[30px] rounded-[9px] flex items-center justify-center shrink-0"
          style={{ backgroundColor: `color-mix(in srgb, ${accent} 16%, transparent)` }}
        >
          <Icon className="w-[15px] h-[15px]" style={{ color: accent }} />
        </div>
      </div>

      <span className="text-lg font-semibold leading-none text-text-primary tabular-nums truncate relative">
        {value}
      </span>

      <div className="flex items-end justify-between gap-2 relative">
        {sub && <span className="text-[12px] text-text-tertiary truncate">{sub}</span>}
        {bars && (
          <div className="flex items-end gap-[2px] h-[18px] shrink-0" aria-hidden="true">
            {bars.map((v, i) => (
              <div
                key={i}
                className={cn('w-1 rounded-[1.5px]', i === bars.length - 1 ? 'opacity-90' : 'opacity-50')}
                style={{ height: `${Math.max((v / maxBar) * 100, 10)}%`, backgroundColor: accent }}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
