import { Bar, BarChart, CartesianGrid, LabelList, XAxis, YAxis } from 'recharts'
import { ChartContainer, ChartTooltip, ChartTooltipContent } from './shadcn/chart'

interface CountChartProps {
  data: { name: string; count: number }[]
  label: string
  color?: string
  total?: number
  valueLabel?: string
  formatValue?: (value: number) => string
  formatTick?: (value: number) => string
}

/** Shared shadcn horizontal chart; full labels and exact counts remain accessible. */
export function CountChart({ data, label, color = 'var(--chart-1)', total, valueLabel = 'Count', formatValue = v => v.toLocaleString(), formatTick = v => Intl.NumberFormat('en', { notation: 'compact' }).format(v) }: CountChartProps) {
  if (!data.length) return <p className="py-6 text-sm text-muted-foreground">No data yet</p>
  return (
    <>
      <ChartContainer role="figure" config={{ count: { label, color } }} className="w-full aspect-auto" style={{ height: Math.max(140, data.length * 36 + 32) }} aria-label={label}>
        <BarChart accessibilityLayer data={data} layout="vertical" margin={{ left: 0, right: 52, top: 4, bottom: 0 }}>
          <CartesianGrid horizontal={false} />
          <XAxis type="number" allowDecimals={false} axisLine={false} tickLine={false} tickFormatter={formatTick} />
          <YAxis dataKey="name" type="category" axisLine={false} tickLine={false} width={104} tickMargin={8} tickFormatter={v => String(v).length > 15 ? `${String(v).slice(0, 14)}…` : String(v)} />
          <ChartTooltip cursor={{ fill: 'var(--muted)' }} content={<ChartTooltipContent labelFormatter={(_, payload) => payload[0]?.payload.name} formatter={value => <span className="font-medium tabular-nums">{formatValue(Number(value))}{total != null && total > 0 ? ` · ${Math.round(Number(value) / total * 100)}%` : ''}</span>} />} />
          <Bar dataKey="count" fill="var(--color-count)" radius={[0, 4, 4, 0]} maxBarSize={14} isAnimationActive={false}>
            <LabelList dataKey="count" position="right" className="fill-foreground text-xs" formatter={formatTick} />
          </Bar>
        </BarChart>
      </ChartContainer>
      <div className="sr-only"><table><caption>{label}</caption><thead><tr><th scope="col">Category</th><th scope="col">{valueLabel}</th></tr></thead><tbody>{data.map((row, i) => <tr key={`${row.name}-${i}`}><th scope="row">{row.name}</th><td>{formatValue(row.count)}</td></tr>)}</tbody></table></div>
    </>
  )
}
