import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/shadcn/chart'
import type { DailyCount } from '../../types'

export function MemoryTrendChart({ data }: { data: DailyCount[] }) {
  return (
    <>
      <ChartContainer role="figure" config={{ count: { label: 'Memories', color: 'var(--chart-1)' } }} className="h-48 w-full aspect-auto" aria-label="Memories created per day">
        <BarChart accessibilityLayer data={data} margin={{ left: -20, right: 4, top: 8, bottom: 0 }}>
          <CartesianGrid vertical={false} />
          <XAxis dataKey="date" axisLine={false} tickLine={false} tickMargin={8} minTickGap={32} tickFormatter={v => String(v).slice(5)} />
          <YAxis axisLine={false} tickLine={false} allowDecimals={false} tickFormatter={v => Intl.NumberFormat('en', { notation: 'compact' }).format(v)} />
          <ChartTooltip content={<ChartTooltipContent />} />
          <Bar dataKey="count" fill="var(--color-count)" maxBarSize={20} radius={[4, 4, 0, 0]} isAnimationActive={false} />
        </BarChart>
      </ChartContainer>
      <div className="sr-only"><table><caption>Memories created per day</caption><thead><tr><th scope="col">Date</th><th scope="col">Memories</th></tr></thead><tbody>{data.map(d => <tr key={d.date}><th scope="row">{d.date}</th><td>{d.count}</td></tr>)}</tbody></table></div>
    </>
  )
}
