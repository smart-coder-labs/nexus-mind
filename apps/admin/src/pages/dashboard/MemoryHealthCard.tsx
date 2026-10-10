import { CountChart } from '@/components/ui/CountChart'

export interface MemoryHealthCardProps {
  total: number | undefined
  duplicates: number | undefined
  stale: number | undefined
  untagged: number | undefined
}

export function MemoryHealthCard({ total, duplicates, stale, untagged }: MemoryHealthCardProps) {
  if (total == null) return <p className="py-6 text-sm text-muted-foreground">Health data unavailable</p>
  const categories = [
    { name: 'Duplicates', count: duplicates },
    { name: 'Stale (>30d)', count: stale },
    { name: 'Untagged', count: untagged },
  ]
  return (
    <div className="space-y-3">
      <p className="flex items-center justify-between gap-3 text-sm text-muted-foreground">Total memories <span className="rounded-md bg-muted px-2 py-0.5 text-xs font-semibold text-foreground tabular-nums">{total.toLocaleString()}</span></p>
      <CountChart data={categories.filter((c): c is { name: string; count: number } => c.count != null)} label="Memory health indicators" color="var(--chart-2)" />
      <p className="text-xs leading-relaxed text-muted-foreground">Categories may overlap. Counts are shown separately.</p>
      {categories.some(c => c.count == null) && <p role="status" className="text-xs text-muted-foreground">Some health indicators are unavailable.</p>}
    </div>
  )
}
