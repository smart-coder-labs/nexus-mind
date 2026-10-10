import { Bar, BarChart } from 'recharts'
import { ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/shadcn/chart'

export default function MetricSeries({ label, samples }: { label: string; samples: number[] }) {
  return <ChartContainer role="figure" config={{ count: { label, color: 'var(--chart-1)' } }} className="h-8 w-24 shrink-0 aspect-auto" aria-label={`${label}, recent samples: ${samples.join(', ')}`}>
    <BarChart accessibilityLayer data={samples.map((count, index) => ({ count, name: `Sample ${index + 1}` }))}>
      <ChartTooltip content={<ChartTooltipContent hideLabel />} />
      <Bar dataKey="count" fill="var(--color-count)" radius={[3, 3, 0, 0]} maxBarSize={16} isAnimationActive={false} />
    </BarChart>
  </ChartContainer>
}
