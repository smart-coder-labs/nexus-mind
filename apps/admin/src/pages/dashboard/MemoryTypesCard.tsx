import type { NameCount } from '../../types'
import { accentFor } from './colors'

/**
 * Design delta 5: grid of small cards (dot + type name, big count, small %,
 * thin colored underline). Data source: MemoryTrends.by_type — the same
 * real per-type counts the previous "Memory Types" bar list used.
 */
export function MemoryTypesCard({ types, total }: { types: NameCount[]; total: number }) {
  if (types.length === 0) {
    return <p className="text-[13px] text-text-tertiary text-center py-4">No data yet</p>
  }
  return (
    <div className="grid grid-cols-2 gap-2.5">
      {types.map((t, i) => {
        const color = accentFor(i)
        const pct = total > 0 ? Math.round((t.count / total) * 100) : 0
        return (
          <div
            key={t.name || i}
            className="relative flex flex-col gap-1 rounded-[11px] border border-border-secondary bg-white/[0.02] px-3.5 pt-3 pb-3.5 overflow-hidden hover:border-white/[0.14] transition-colors"
          >
            <div className="flex items-center gap-1.5">
              <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: color }} />
              <span className="text-[11.5px] text-text-secondary truncate">{t.name || 'unset'}</span>
            </div>
            <div className="flex items-baseline gap-1.5">
              <span className="text-[21px] font-semibold tracking-[-0.01em] text-text-primary tabular-nums">
                {t.count.toLocaleString()}
              </span>
              <span className="text-[11px] text-text-tertiary">{pct}%</span>
            </div>
            <div className="absolute left-0 right-0 bottom-0 h-[3px] bg-white/[0.04]">
              <div className="h-full opacity-80" style={{ width: `${pct}%`, backgroundColor: color }} />
            </div>
          </div>
        )
      })}
    </div>
  )
}
