import type { NameCount } from '../../types'
import { accentFor } from './colors'

/**
 * Design delta 6: horizontal stacked color bar + ranked rows (rank, dot,
 * name, % chip, count). Data source: MemoryTrends.by_project — the same
 * real per-project counts the previous "Top Projects" bar list used.
 */
export function TopProjectsCard({ projects }: { projects: NameCount[] }) {
  if (projects.length === 0) {
    return <p className="text-[13px] text-text-tertiary text-center py-4">No data yet</p>
  }
  const total = projects.reduce((sum, p) => sum + p.count, 0)

  return (
    <div className="flex flex-col">
      <div className="flex h-[10px] rounded-[5px] overflow-hidden gap-[2px] mb-3.5">
        {projects.map((p, i) => (
          <div
            key={p.name}
            className="h-full opacity-90"
            style={{ width: `${total > 0 ? (p.count / total) * 100 : 0}%`, backgroundColor: accentFor(i) }}
            title={p.name}
          />
        ))}
      </div>
      <div className="flex flex-col">
        {projects.map((p, i) => {
          const pct = total > 0 ? Math.round((p.count / total) * 100) : 0
          return (
            <div
              key={p.name}
              className="flex items-center gap-2.5 py-2 border-b border-border-secondary/40 last:border-0"
            >
              <span className="w-3.5 text-[11px] text-text-quaternary shrink-0">{i + 1}</span>
              <span className="w-2 h-2 rounded-[3px] shrink-0" style={{ backgroundColor: accentFor(i) }} />
              <span className="text-[13px] text-text-primary truncate min-w-0 flex-1">{p.name}</span>
              <span className="text-[11px] font-semibold px-2 py-0.5 rounded-full bg-white/[0.05] text-text-tertiary shrink-0">
                {pct}%
              </span>
              <span className="w-9 text-right text-[12.5px] font-semibold text-text-secondary shrink-0 tabular-nums">
                {p.count.toLocaleString()}
              </span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
