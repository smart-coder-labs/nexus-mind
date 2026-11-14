import type { ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'

interface MetricSummaryProps {
  label: string
  value: ReactNode
  icon?: LucideIcon
  accent?: string
  children?: ReactNode
}

/** A static count with its supporting information always visible.
 *  Signal stat card: glass-1 shell, golden kpi number, data-hue icon chip and the
 *  accent edge on hover. The accent (a --d1..--d4 series colour) tints the icon
 *  only; the card surface stays neutral. */
export function MetricSummary({ label, value, icon: Icon, accent, children }: MetricSummaryProps) {
  const accentStyle = accent ? {
    '--metric-accent': accent,
    '--metric-edge': 'color-mix(in oklab, var(--metric-accent) 26%, var(--border))',
    '--metric-icon-bg': 'color-mix(in oklab, var(--metric-accent) 14%, transparent)',
  } as React.CSSProperties : undefined

  return (
    <div
      className="glass-1 sig-card sig-reveal group flex h-full min-h-[118px] min-w-0 max-w-full flex-col rounded-2xl p-phi-3 sm:p-phi-4"
      data-accent={accent ? 'true' : undefined}
      style={accentStyle}
    >
      <div className="flex min-w-0 items-center justify-between gap-phi-2">
        <span className="min-w-0 truncate text-[13px] font-medium leading-5 text-muted-foreground" title={label}>{label}</span>
        {Icon && <span className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-transparent bg-muted text-muted-foreground transition-colors data-[accent=true]:border-[var(--metric-edge)] data-[accent=true]:bg-[var(--metric-icon-bg)] data-[accent=true]:text-[var(--metric-accent)]" data-accent={accent ? 'true' : undefined}><Icon aria-hidden="true" className="size-4" /></span>}
      </div>
      <div className="mt-phi-2 min-w-0 text-title font-semibold leading-none tracking-[-0.03em] text-foreground tabular-nums [overflow-wrap:anywhere] sm:text-kpi">{value}</div>
      {children && (
        <div className="mt-auto flex flex-wrap items-center justify-between gap-x-phi-3 gap-y-phi-1 pt-phi-2 text-micro text-muted-foreground [overflow-wrap:anywhere]">
          {children}
        </div>
      )}
    </div>
  )
}
