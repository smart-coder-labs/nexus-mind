import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import {
  BarChart3, Coins, Clock, Activity, DatabaseZap, Loader2, Gauge, ArrowUpRight,
  ArrowDownRight, Minus, Cpu,
} from 'lucide-react'
import { cn } from '../lib/utils'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import { SegmentedControl } from '../components/ui/SegmentedControl'
import { DateRangePicker } from '../components/ui/DateRangePicker'
import { StatTile } from './dashboard/StatTile'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { EmptyState } from '../components/ui/EmptyState/EmptyState'
import { UsageTrendChart, type TrendMetric } from './usage/UsageTrendChart'
import { RankedBars } from './usage/RankedBars'
import { CHART_BRIGHT, CHART_DIM, CHART_PRIMARY } from './usage/chartColors'
import {
  addDaysIso, daysBetween, fillBuckets, formatDuration, todayIso,
} from './usage/format'
import type { UsageBucketSize, UsageLevel, UsageSummaryRow } from '../types'

// Same glass recipe used across the admin pages (see Clients.tsx / StatTile).
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// Radix Select forbids an empty-string item value, so the "All …" options use a
// sentinel that is translated to `undefined` (omit the filter) before the call.
const ALL = '__all__'

// `model` is intentionally absent: it gets its own dedicated card, so offering
// it here too would render the same numbers twice side by side.
const LEVELS: { value: UsageLevel; label: string }[] = [
  { value: 'project', label: 'Project' },
  { value: 'client', label: 'Client' },
  { value: 'task', label: 'Task' },
  { value: 'user', label: 'User' },
  { value: 'org', label: 'Org' },
]

type RangePreset = '7d' | '30d' | '90d' | 'all' | 'custom'

const RANGE_OPTIONS: { value: RangePreset; label: string }[] = [
  { value: '7d', label: '7d' },
  { value: '30d', label: '30d' },
  { value: '90d', label: '90d' },
  { value: 'all', label: 'All' },
]

const PRESET_DAYS: Record<'7d' | '30d' | '90d', number> = { '7d': 7, '30d': 30, '90d': 90 }

/** Resolves a preset into the `[from, to]` the API filters on. */
function presetRange(preset: Exclude<RangePreset, 'custom'>): { from: string; to: string } {
  if (preset === 'all') return { from: '', to: '' }
  const to = todayIso()
  return { from: addDaysIso(to, -(PRESET_DAYS[preset] - 1)), to }
}

/**
 * The window of the same length immediately before `[from, to]`, used for the
 * "vs previous" deltas. Returns null for an unbounded range — there is no
 * previous period to compare an all-time total against, and inventing one would
 * put a number on the tile that means nothing.
 */
function previousRange(from: string, to: string): { from: string; to: string } | null {
  if (!from || !to) return null
  const span = daysBetween(from, to) + 1
  if (!Number.isFinite(span) || span <= 0) return null
  const prevTo = addDaysIso(from, -1)
  return { from: addDaysIso(prevTo, -(span - 1)), to: prevTo }
}

/** Signed percent change, or null when the baseline is zero (undefined growth). */
function pctDelta(current: number, previous: number): number | null {
  if (!previous) return null
  return ((current - previous) / previous) * 100
}

function DeltaChip({ delta }: { delta: number | null }) {
  if (delta === null) {
    return (
      <span className="inline-flex items-center gap-1 text-text-quaternary">
        <Minus className="w-3 h-3" />
        no baseline
      </span>
    )
  }
  const rounded = Math.round(delta)
  if (rounded === 0) {
    return (
      <span className="inline-flex items-center gap-1 text-text-quaternary">
        <Minus className="w-3 h-3" />
        flat vs prev
      </span>
    )
  }
  const up = rounded > 0
  const Icon = up ? ArrowUpRight : ArrowDownRight
  // Neutral ink on purpose: more usage is neither good nor bad here, so the
  // status hues would assert a judgement the data does not support.
  return (
    <span className="inline-flex items-center gap-1 text-text-tertiary">
      <Icon className="w-3 h-3" />
      {up ? '+' : ''}{rounded}% vs prev
    </span>
  )
}

function PanelCard({
  title, action, children, className,
}: {
  title: string
  action?: React.ReactNode
  children: React.ReactNode
  className?: string
}) {
  return (
    <section className={cn('rounded-[18px]', GLASS_PANEL, className)}>
      <header className="px-5 py-4 border-b border-border-secondary flex items-center justify-between gap-3 flex-wrap">
        <h2 className="text-sm font-semibold text-text-primary">{title}</h2>
        {action}
      </header>
      <div className="p-5">{children}</div>
    </section>
  )
}

export default function Usage() {
  const { session } = useAuth()
  const isAdmin = isPrivileged(session?.user.role)
  const isSuperUser = session?.user.role === 'super_user'
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [level, setLevel] = useState<UsageLevel>('project')
  const [preset, setPreset] = useState<RangePreset>('30d')
  const [custom, setCustom] = useState({ from: '', to: '' })
  const [bucket, setBucket] = useState<UsageBucketSize>('day')
  const [metric, setMetric] = useState<TrendMetric>('tokens')
  const [clientId, setClientId] = useState<string>(ALL)
  const [projectId, setProjectId] = useState<string>(ALL)
  const [backfillMsg, setBackfillMsg] = useState<string | null>(null)

  const { from, to } = preset === 'custom' ? custom : presetRange(preset)
  const clientFilter = clientId === ALL ? undefined : clientId
  const projectFilter = projectId === ALL ? undefined : projectId

  const scope = {
    from: from || undefined,
    to: to || undefined,
    client_id: clientFilter,
    project_id: projectFilter,
  }
  const scopeKey = [from, to, clientFilter ?? '', projectFilter ?? ''] as const

  // Filter Select data sources.
  const { data: clients } = useQuery({
    queryKey: ['clients', false],
    queryFn: () => client.listClients(),
    enabled: isAdmin,
  })

  const { data: projects } = useQuery({
    queryKey: ['projects', clientFilter ?? 'all'],
    queryFn: () => client.listProjects({ client_id: clientFilter }),
    enabled: isAdmin,
  })

  const { data, isLoading, isError, error } = useQuery({
    queryKey: ['usage-summary', level, ...scopeKey],
    queryFn: () => client.getUsageSummary({ level, ...scope }),
    enabled: isAdmin,
  })

  const { data: series, isLoading: seriesLoading } = useQuery({
    queryKey: ['usage-timeseries', bucket, ...scopeKey],
    queryFn: () => client.getUsageTimeseries({ bucket, ...scope }),
    enabled: isAdmin,
  })

  const { data: byModel } = useQuery({
    queryKey: ['usage-summary', 'model', ...scopeKey],
    queryFn: () => client.getUsageSummary({ level: 'model', ...scope }),
    enabled: isAdmin,
  })

  const prev = previousRange(from, to)
  const { data: prevData } = useQuery({
    queryKey: ['usage-summary', 'org', prev?.from ?? '', prev?.to ?? '', clientFilter ?? '', projectFilter ?? ''],
    queryFn: () =>
      client.getUsageSummary({
        level: 'org',
        from: prev!.from,
        to: prev!.to,
        client_id: clientFilter,
        project_id: projectFilter,
      }),
    enabled: isAdmin && prev !== null,
  })

  const backfillMut = useMutation({
    mutationFn: () => client.runUsageBackfill(),
    onSuccess: (res) => {
      setBackfillMsg(`${res.inserted.toLocaleString()} events backfilled`)
      qc.invalidateQueries({ queryKey: ['usage-summary'] })
      qc.invalidateQueries({ queryKey: ['usage-timeseries'] })
    },
    onError: (err: unknown) =>
      setBackfillMsg((err as Error)?.message ?? 'Backfill failed'),
  })

  const totals = data?.totals
  const prevTotals = prevData?.totals

  // Rows sorted by total tokens descending (design: rollup ranked by usage).
  const rows = useMemo(() => {
    const list = data?.rows ?? []
    return [...list].sort((a, b) => b.tokens_total - a.tokens_total)
  }, [data])

  // Gap-filled so idle days occupy real width instead of being compressed away.
  const buckets = useMemo(
    () => fillBuckets(series?.buckets ?? [], bucket, from, to),
    [series, bucket, from, to],
  )

  const hasData = (totals?.event_count ?? 0) > 0

  const statTiles = totals
    ? [
        {
          label: 'Total tokens',
          value: totals.tokens_total.toLocaleString(),
          sub: <DeltaChip delta={pctDelta(totals.tokens_total, prevTotals?.tokens_total ?? 0)} />,
          icon: Coins,
          accent: CHART_PRIMARY,
          sparkline: buckets.map(b => b.tokens_total),
        },
        {
          label: 'Tokens out',
          value: totals.tokens_out.toLocaleString(),
          sub: (
            <span className="text-text-tertiary">
              {totals.tokens_total > 0
                ? `${Math.round((totals.tokens_out / totals.tokens_total) * 100)}% of total`
                : 'no tokens yet'}
            </span>
          ),
          icon: ArrowUpRight,
          accent: CHART_BRIGHT,
          sparkline: buckets.map(b => b.tokens_out),
        },
        {
          label: 'Execution time',
          value: formatDuration(totals.duration_ms),
          sub: <DeltaChip delta={pctDelta(totals.duration_ms, prevTotals?.duration_ms ?? 0)} />,
          icon: Clock,
          accent: CHART_DIM,
          sparkline: buckets.map(b => b.duration_ms),
        },
        {
          label: 'Events',
          value: totals.event_count.toLocaleString(),
          sub: <DeltaChip delta={pctDelta(totals.event_count, prevTotals?.event_count ?? 0)} />,
          icon: Activity,
          accent: CHART_PRIMARY,
          sparkline: buckets.map(b => b.event_count),
        },
        {
          label: 'Avg per event',
          value:
            totals.event_count > 0
              ? Math.round(totals.tokens_total / totals.event_count).toLocaleString()
              : '0',
          sub: (
            <span className="text-text-tertiary">
              tokens ·{' '}
              {totals.event_count > 0
                ? formatDuration(totals.duration_ms / totals.event_count)
                : '0ms'}
            </span>
          ),
          icon: Gauge,
          accent: CHART_BRIGHT,
          // A ratio has no meaningful per-day series to sparkline; the tile
          // deliberately shows none rather than fabricating one.
          sparkline: undefined as number[] | undefined,
        },
      ]
    : []

  // When the client filter changes, drop a now-inconsistent project selection.
  const onClientChange = (value: string) => {
    setClientId(value)
    setProjectId(ALL)
  }

  const onCustomRange = (next: { from: string; to: string }) => {
    setCustom(next)
    setPreset('custom')
  }

  const levelLabel = LEVELS.find(l => l.value === level)?.label ?? level

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-6">
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 rounded-[13px] bg-accent-blue/12 flex items-center justify-center shrink-0">
            <BarChart3 className="w-5 h-5 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-base font-semibold text-text-primary">Usage</h1>
            <p className="text-xs text-text-quaternary mt-0.5">
              Token consumption and execution time over time, by project, client, task, user or model.
            </p>
          </div>
        </div>

        {isSuperUser && (
          <div className="flex items-center gap-3">
            {backfillMsg && (
              <span className="text-xs text-text-tertiary">{backfillMsg}</span>
            )}
            <button
              onClick={() => { setBackfillMsg(null); backfillMut.mutate() }}
              disabled={backfillMut.isPending}
              className="flex items-center gap-1.5 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white px-3.5 py-1.5 text-xs font-semibold transition-colors disabled:opacity-50"
            >
              {backfillMut.isPending
                ? <Loader2 className="w-3.5 h-3.5 animate-spin" />
                : <DatabaseZap className="w-3.5 h-3.5" />}
              {backfillMut.isPending ? 'Backfilling…' : 'Run backfill'}
            </button>
          </div>
        )}
      </div>

      {!isAdmin ? (
        <EmptyState
          title="Usage metrics are unavailable"
          description="Only administrators and super users can view organization usage metrics."
          icon={<BarChart3 />}
        />
      ) : (
        <>
          {/* Filter bar — one row above the charts. */}
          <div className={`rounded-[18px] p-3.5 ${GLASS_PANEL}`}>
            <div className="flex flex-wrap items-center gap-3">
              <SegmentedControl
                size="sm"
                options={RANGE_OPTIONS}
                value={preset}
                onChange={(v) => setPreset(v as RangePreset)}
              />
              <DateRangePicker
                from={preset === 'custom' ? custom.from : ''}
                to={preset === 'custom' ? custom.to : ''}
                onChange={onCustomRange}
                placeholder="Custom"
              />

              <span className="w-px h-6 bg-border-secondary mx-1" aria-hidden="true" />

              <Select value={clientId} onValueChange={onClientChange}>
                <SelectTrigger
                  aria-label="Filter by client"
                  className="w-44 h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL}>All clients</SelectItem>
                  {(clients ?? []).map(c => (
                    <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Select value={projectId} onValueChange={setProjectId}>
                <SelectTrigger
                  aria-label="Filter by project"
                  className="w-44 h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={ALL}>All projects</SelectItem>
                  {(projects ?? []).map(p => (
                    <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          {isError ? (
            <div className={`rounded-[18px] flex flex-col items-center gap-2 py-14 text-center ${GLASS_PANEL}`}>
              <BarChart3 className="w-6 h-6 text-status-error/60" />
              <p className="text-xs font-semibold text-text-secondary">Couldn't load usage</p>
              <p className="text-xs text-text-quaternary max-w-xs">
                {(error as Error)?.message ?? 'Unknown error'}
              </p>
            </div>
          ) : !isLoading && !hasData ? (
            <div className={`rounded-[18px] flex flex-col items-center gap-2.5 py-16 text-center ${GLASS_PANEL}`}>
              <BarChart3 className="w-7 h-7 text-text-quaternary/50" />
              <p className="text-sm font-semibold text-text-secondary">No usage in this range</p>
              <p className="text-xs text-text-quaternary max-w-sm leading-relaxed">
                Agents report usage as they run, so this fills in on its own.
                {preset !== 'all' && ' Widen the range to “All” to check whether anything was recorded earlier.'}
                {isSuperUser && ' A backfill seeds execution time from existing sessions — it recovers no tokens, since sessions never carried them.'}
              </p>
              {preset !== 'all' && (
                <button
                  onClick={() => setPreset('all')}
                  className="mt-1 text-xs font-semibold text-accent-blue hover:underline"
                >
                  Show all time
                </button>
              )}
            </div>
          ) : (
            <>
              {/* KPI row */}
              <KpiMarquee role="list" aria-label="Usage statistics">
                {isLoading
                  ? Array.from({ length: 5 }).map((_, i) => (
                      <div key={i} className="w-[232px] flex-none">
                        <div className={`h-[122px] rounded-[18px] animate-pulse ${GLASS_PANEL}`} />
                      </div>
                    ))
                  : statTiles.map(tile => (
                      <div key={tile.label} className="w-[232px] flex-none">
                        <StatTile
                          label={tile.label}
                          value={tile.value}
                          sub={tile.sub}
                          icon={tile.icon}
                          accent={tile.accent}
                          sparkline={tile.sparkline}
                        />
                      </div>
                    ))}
              </KpiMarquee>

              {/* Lead chart */}
              <PanelCard
                title="Usage over time"
                action={
                  <div className="flex items-center gap-2 flex-wrap">
                    <SegmentedControl
                      size="sm"
                      options={[
                        { value: 'tokens', label: 'Tokens' },
                        { value: 'duration', label: 'Time' },
                        { value: 'events', label: 'Events' },
                      ]}
                      value={metric}
                      onChange={(v) => setMetric(v as TrendMetric)}
                    />
                    <SegmentedControl
                      size="sm"
                      options={[
                        { value: 'hour', label: 'Hour' },
                        { value: 'day', label: 'Day' },
                        { value: 'week', label: 'Week' },
                      ]}
                      value={bucket}
                      onChange={(v) => setBucket(v as UsageBucketSize)}
                    />
                  </div>
                }
              >
                {seriesLoading ? (
                  <div className="h-[268px] rounded-[12px] bg-white/[0.02] animate-pulse" />
                ) : buckets.length === 0 ? (
                  <p className="text-[12.5px] text-text-tertiary text-center py-20">
                    No events in this range to plot.
                  </p>
                ) : (
                  <UsageTrendChart buckets={buckets} size={bucket} metric={metric} />
                )}
              </PanelCard>

              {/* Breakdowns */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <PanelCard
                  title={`Top by ${levelLabel.toLowerCase()}`}
                  action={
                    <Select value={level} onValueChange={v => setLevel(v as UsageLevel)}>
                      <SelectTrigger
                        aria-label="Rollup level"
                        className="w-32 h-8 text-xs bg-transparent border border-border-primary rounded-[10px] px-3 focus:outline-none focus:border-accent-blue/60"
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {LEVELS.map(l => (
                          <SelectItem key={l.value} value={l.value}>{l.label}</SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  }
                >
                  {isLoading ? (
                    <div className="flex flex-col gap-3">
                      {Array.from({ length: 6 }).map((_, i) => (
                        <div key={i} className="h-[26px] rounded-[6px] bg-white/[0.03] animate-pulse" />
                      ))}
                    </div>
                  ) : (
                    <RankedBars
                      rows={rows}
                      metric={metric === 'duration' ? 'duration' : metric === 'events' ? 'events' : 'tokens'}
                      emptyLabel={`No usage attributed to a ${levelLabel.toLowerCase()} in this range.`}
                    />
                  )}
                </PanelCard>

                <PanelCard
                  title="By model"
                  action={
                    <span className="flex items-center gap-1.5 text-[11px] text-text-quaternary">
                      <Cpu className="w-3.5 h-3.5" />
                      as reported by agents
                    </span>
                  }
                >
                  <RankedBars
                    rows={byModel?.rows ?? []}
                    metric={metric === 'duration' ? 'duration' : metric === 'events' ? 'events' : 'tokens'}
                    emptyLabel="No agent reported a model with its usage."
                  />
                </PanelCard>
              </div>

              {/* Detail table — the auditable surface behind the charts. */}
              <div className={`rounded-[18px] overflow-hidden ${GLASS_PANEL}`}>
                <div className="px-5 py-4 border-b border-border-secondary flex items-center justify-between gap-3">
                  <span className="text-sm font-semibold text-text-primary">
                    Detail by {levelLabel.toLowerCase()}
                  </span>
                  {!isLoading && (
                    <span className="text-[11px] text-text-quaternary">
                      {rows.length.toLocaleString()} {rows.length === 1 ? 'row' : 'rows'}
                    </span>
                  )}
                </div>

                {isLoading ? (
                  <div className="divide-y divide-border-secondary">
                    {Array.from({ length: 5 }).map((_, i) => (
                      <div key={i} className="px-5 py-3.5 flex items-center gap-4">
                        <div className="h-3.5 rounded-[5px] bg-white/[0.04] animate-pulse flex-1" />
                        {Array.from({ length: 5 }).map((_, j) => (
                          <div key={j} className="h-3.5 w-16 rounded-[5px] bg-white/[0.04] animate-pulse" />
                        ))}
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full text-xs">
                      <thead>
                        <tr className="text-text-tertiary border-b border-border-secondary">
                          <th className="text-left font-semibold px-5 py-2.5">{levelLabel}</th>
                          <th className="text-right font-semibold px-4 py-2.5">Tokens in</th>
                          <th className="text-right font-semibold px-4 py-2.5">Tokens out</th>
                          <th className="text-right font-semibold px-4 py-2.5">Total tokens</th>
                          <th className="text-right font-semibold px-4 py-2.5">Share</th>
                          <th className="text-right font-semibold px-4 py-2.5">Duration</th>
                          <th className="text-right font-semibold px-5 py-2.5">Events</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-border-secondary">
                        {rows.map((row: UsageSummaryRow, i) => {
                          const share = totals && totals.tokens_total > 0
                            ? (row.tokens_total / totals.tokens_total) * 100
                            : 0
                          return (
                            <tr key={row.key_id ?? `${row.key_name}-${i}`} className="hover:bg-white/[0.03] transition-colors">
                              <td className="px-5 py-3 text-text-primary font-medium truncate max-w-xs">{row.key_name}</td>
                              <td className="px-4 py-3 text-right text-text-secondary tabular-nums">{row.tokens_in.toLocaleString()}</td>
                              <td className="px-4 py-3 text-right text-text-secondary tabular-nums">{row.tokens_out.toLocaleString()}</td>
                              <td className="px-4 py-3 text-right text-text-primary font-semibold tabular-nums">{row.tokens_total.toLocaleString()}</td>
                              <td className="px-4 py-3 text-right text-text-tertiary tabular-nums">
                                {share > 0 && share < 1 ? '<1' : Math.round(share)}%
                              </td>
                              <td className="px-4 py-3 text-right text-text-secondary tabular-nums">{formatDuration(row.duration_ms)}</td>
                              <td className="px-5 py-3 text-right text-text-secondary tabular-nums">{row.event_count.toLocaleString()}</td>
                            </tr>
                          )
                        })}
                      </tbody>
                      {totals && (
                        <tfoot>
                          <tr className={cn('border-t border-border-secondary text-text-primary font-semibold', 'bg-white/[0.02]')}>
                            <td className="px-5 py-3">Total</td>
                            <td className="px-4 py-3 text-right tabular-nums">{totals.tokens_in.toLocaleString()}</td>
                            <td className="px-4 py-3 text-right tabular-nums">{totals.tokens_out.toLocaleString()}</td>
                            <td className="px-4 py-3 text-right tabular-nums">{totals.tokens_total.toLocaleString()}</td>
                            <td className="px-4 py-3 text-right tabular-nums">100%</td>
                            <td className="px-4 py-3 text-right tabular-nums">{formatDuration(totals.duration_ms)}</td>
                            <td className="px-5 py-3 text-right tabular-nums">{totals.event_count.toLocaleString()}</td>
                          </tr>
                        </tfoot>
                      )}
                    </table>
                  </div>
                )}
              </div>
            </>
          )}
        </>
      )}
    </div>
  )
}
