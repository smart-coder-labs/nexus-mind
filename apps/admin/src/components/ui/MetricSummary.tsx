import type { ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'

interface MetricSummaryProps {
  label: string
  value: ReactNode
  icon?: LucideIcon
  accent?: string
  children?: ReactNode
}

/** A static count with its supporting information always visible. */
export function MetricSummary({ label, value, icon: Icon, accent, children }: MetricSummaryProps) {
  const accentStyle = accent ? {
    '--metric-accent': accent,
    '--metric-tint': 'color-mix(in srgb, var(--metric-accent) 10%, var(--card))',
    '--metric-edge': 'color-mix(in srgb, var(--metric-accent) 26%, var(--border))',
    '--metric-icon-bg': 'color-mix(in srgb, var(--metric-accent) 15%, var(--card))',
  } as React.CSSProperties : undefined

  return (
    <div
      className="group flex h-full min-h-[118px] min-w-0 max-w-full flex-col rounded-xl border bg-card px-4 py-3.5 shadow-sm transition-[border-color,box-shadow,transform] duration-200 hover:-translate-y-0.5 hover:shadow-md data-[accent=true]:border-[var(--metric-edge)] data-[accent=true]:bg-[var(--metric-tint)]"
      data-accent={accent ? 'true' : undefined}
      style={accentStyle}
    >
      <div className="flex min-w-0 items-center justify-between gap-3">
        <span className="min-w-0 truncate text-xs font-medium leading-5 text-muted-foreground" title={label}>{label}</span>
        {Icon && <span className="flex size-9 shrink-0 items-center justify-center rounded-lg border border-transparent bg-muted text-muted-foreground transition-colors data-[accent=true]:border-[var(--metric-edge)] data-[accent=true]:bg-[var(--metric-icon-bg)] data-[accent=true]:text-[var(--metric-accent)]" data-accent={accent ? 'true' : undefined}><Icon aria-hidden="true" className="size-[18px]" /></span>}
      </div>
      <div className="mt-2 min-w-0 text-2xl font-semibold leading-none tracking-[-0.04em] text-foreground tabular-nums [overflow-wrap:anywhere]">{value}</div>
      {children && (
        <div className="mt-auto flex flex-wrap items-center justify-between gap-x-3 gap-y-1 pt-2 text-xs leading-relaxed text-muted-foreground [overflow-wrap:anywhere]">
          {children}
        </div>
      )}
    </div>
  )
}
