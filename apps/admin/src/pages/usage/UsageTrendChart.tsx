import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from 'recharts'
import { ChartContainer, ChartLegend, ChartLegendContent, ChartTooltip, ChartTooltipContent } from '@/components/ui/shadcn/chart'
import type { UsageBucket, UsageBucketSize } from '../../types'
import { bucketLabel, compactNumber, formatDuration } from './format'

export type TrendMetric = 'tokens' | 'duration' | 'events'
export interface UsageTrendChartProps {
  /** Gap-filled buckets, oldest first. Zero-value buckets are meaningful. */
  buckets: UsageBucket[]
  size: UsageBucketSize
  metric: TrendMetric
}
const LABEL = { tokens: 'Tokens', duration: 'Execution time', events: 'Events' }

export function UsageTrendChart({ buckets, size, metric }: UsageTrendChartProps) {
  const stacked = metric === 'tokens'
  const key = metric === 'duration' ? 'duration_ms' : 'event_count'
  const valueOf = (b: UsageBucket) => stacked ? b.tokens_total : b[key]
  const formatValue = (v: number) => metric === 'duration' ? formatDuration(v) : v.toLocaleString()
  const summary = `${LABEL[metric]} by ${size}, ${buckets.length} buckets, ${formatValue(buckets.reduce((sum, b) => sum + valueOf(b), 0))} total.`
  const config = {
    tokens_in: { label: 'Tokens in', color: 'var(--chart-2)' },
    tokens_out: { label: 'Tokens out', color: 'var(--chart-1)' },
    duration_ms: { label: 'Execution time', color: 'var(--chart-1)' },
    event_count: { label: 'Events', color: 'var(--chart-1)' },
  }
  return (
    <>
      <ChartContainer role="figure" config={config} className="h-[268px] w-full aspect-auto" aria-label={summary}>
        <BarChart accessibilityLayer data={buckets} margin={{ left: 0, right: 4, top: 8, bottom: 0 }}>
          <CartesianGrid vertical={false} />
          <XAxis dataKey="bucket_ts" tickFormatter={v => bucketLabel(v, size)} axisLine={false} tickLine={false} tickMargin={10} minTickGap={40} />
          <YAxis width={56} axisLine={false} tickLine={false} allowDecimals={false} tickFormatter={v => metric === 'duration' ? formatDuration(v) : compactNumber(v)} />
          <ChartTooltip content={<ChartTooltipContent labelFormatter={(_, payload) => bucketLabel(payload[0]?.payload.bucket_ts ?? '', size)} formatter={(value, name) => <div className="flex w-full items-center justify-between gap-4"><span className="text-muted-foreground">{config[name as keyof typeof config]?.label ?? name}</span><span className="font-medium tabular-nums text-foreground">{formatValue(Number(value))}</span></div>} />} />
          {stacked && <ChartLegend content={<ChartLegendContent />} />}
          {stacked && <Bar dataKey="tokens_in" stackId="tokens" fill="var(--color-tokens_in)" maxBarSize={24} isAnimationActive={false} />}
          {stacked && <Bar dataKey="tokens_out" stackId="tokens" fill="var(--color-tokens_out)" radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />}
          {!stacked && <Bar dataKey={key} fill={`var(--color-${key})`} radius={[4, 4, 0, 0]} maxBarSize={24} isAnimationActive={false} />}
        </BarChart>
      </ChartContainer>
      <div className="sr-only"><table><caption>{summary}</caption><thead><tr><th scope="col">Period</th><th scope="col">{LABEL[metric]}</th>{stacked && <><th scope="col">Tokens in</th><th scope="col">Tokens out</th></>}</tr></thead><tbody>{buckets.map(b => <tr key={b.bucket_ts}><th scope="row">{bucketLabel(b.bucket_ts, size)}</th><td>{formatValue(valueOf(b))}</td>{stacked && <><td>{b.tokens_in.toLocaleString()}</td><td>{b.tokens_out.toLocaleString()}</td></>}</tr>)}</tbody></table></div>
    </>
  )
}
