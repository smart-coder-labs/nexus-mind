import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { createClient } from '../api/client'
import { useAuth, isPrivileged } from '../auth/AuthContext'
import {
  BarChart3, Coins, Clock, Activity, DatabaseZap, Loader2,
} from 'lucide-react'
import { cn } from '../lib/utils'
import {
  Select, SelectTrigger, SelectValue, SelectContent, SelectItem,
} from '../components/ui/Select/Select'
import { StatTile } from './dashboard/StatTile'
import { accentFor } from './dashboard/colors'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { EmptyState } from '../components/ui/EmptyState/EmptyState'
import type { UsageLevel, UsageSummaryRow } from '../types'

// Same glass recipe used across the admin pages (see Clients.tsx / StatTile).
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// Radix Select forbids an empty-string item value, so the "All …" options use a
// sentinel that is translated to `undefined` (omit the filter) before the call.
const ALL = '__all__'

const LEVELS: { value: UsageLevel; label: string }[] = [
  { value: 'task', label: 'Task' },
  { value: 'project', label: 'Project' },
  { value: 'client', label: 'Client' },
  { value: 'org', label: 'Org' },
]

/**
 * Formats a millisecond duration into a compact human string:
 *   820        → "820ms"
 *   4200       → "4.2s"
 *   63000      → "1m 3s"
 *   3_780_000  → "1h 3m"
 * Zero / negative collapses to "0ms".
 */
function formatDuration(ms: number): string {
  if (!ms || ms <= 0) return '0ms'
  if (ms < 1000) return `${Math.round(ms)}ms`

  const totalSec = ms / 1000
  if (totalSec < 60) {
    // One decimal place, trimming a trailing ".0" (e.g. "4.2s", "12s").
    const s = Math.round(totalSec * 10) / 10
    return `${Number.isInteger(s) ? s : s.toFixed(1)}s`
  }

  const totalMin = Math.floor(totalSec / 60)
  const hours = Math.floor(totalMin / 60)
  const mins = totalMin % 60
  if (hours > 0) return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`

  const secs = Math.floor(totalSec % 60)
  return secs > 0 ? `${totalMin}m ${secs}s` : `${totalMin}m`
}

export default function Usage() {
  const { session } = useAuth()
  const isAdmin = isPrivileged(session?.user.role)
  const isSuperUser = session?.user.role === 'super_user'
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [level, setLevel] = useState<UsageLevel>('project')
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const [clientId, setClientId] = useState<string>(ALL)
  const [projectId, setProjectId] = useState<string>(ALL)
  const [backfillMsg, setBackfillMsg] = useState<string | null>(null)

  const clientFilter = clientId === ALL ? undefined : clientId
  const projectFilter = projectId === ALL ? undefined : projectId

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

  const usageQueryKey = ['usage-summary', level, from, to, clientFilter, projectFilter] as const
  const { data, isLoading, isError, error } = useQuery({
    queryKey: usageQueryKey,
    queryFn: () =>
      client.getUsageSummary({
        level,
        from: from || undefined,
        to: to || undefined,
        client_id: clientFilter,
        project_id: projectFilter,
      }),
    enabled: isAdmin,
  })

  const backfillMut = useMutation({
    mutationFn: () => client.runUsageBackfill(),
    onSuccess: (res) => {
      setBackfillMsg(`${res.inserted.toLocaleString()} events backfilled`)
      qc.invalidateQueries({ queryKey: ['usage-summary'] })
    },
    onError: (err: unknown) =>
      setBackfillMsg((err as Error)?.message ?? 'Backfill failed'),
  })

  const totals = data?.totals

  // Rows sorted by total tokens descending (design: rollup ranked by usage).
  const rows = useMemo(() => {
    const list = data?.rows ?? []
    return [...list].sort((a, b) => b.tokens_total - a.tokens_total)
  }, [data])

  const statTiles = [
    {
      label: 'Total Tokens',
      value: (totals?.tokens_total ?? 0).toLocaleString(),
      sub: totals
        ? `${totals.tokens_in.toLocaleString()} in · ${totals.tokens_out.toLocaleString()} out`
        : undefined,
      icon: Coins,
    },
    {
      label: 'Total Time',
      value: formatDuration(totals?.duration_ms ?? 0),
      sub: 'execution time',
      icon: Clock,
    },
    {
      label: 'Events',
      value: (totals?.event_count ?? 0).toLocaleString(),
      sub: `at ${level} level`,
      icon: Activity,
    },
  ]

  // When the client filter changes, drop a now-inconsistent project selection.
  const onClientChange = (value: string) => {
    setClientId(value)
    setProjectId(ALL)
  }

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3.5">
          <div className="w-11 h-11 rounded-[13px] bg-accent-blue/12 flex items-center justify-center shrink-0">
            <BarChart3 className="w-5 h-5 text-accent-blue" />
          </div>
          <div>
            <h1 className="text-base font-semibold text-text-primary">Usage</h1>
            <p className="text-xs text-text-quaternary mt-0.5">
              Token consumption and execution time, rolled up by task, project, client or org.
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
          {/* Filter header */}
          <div className={`rounded-[18px] p-4 ${GLASS_PANEL}`}>
            <div className="flex flex-wrap items-end gap-4">
              <div className="flex flex-col gap-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">Level</label>
                <Select value={level} onValueChange={v => setLevel(v as UsageLevel)}>
                  <SelectTrigger className="w-36 h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {LEVELS.map(l => (
                      <SelectItem key={l.value} value={l.value}>{l.label}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex flex-col gap-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">From</label>
                <input
                  type="date"
                  value={from}
                  onChange={e => setFrom(e.target.value)}
                  aria-label="From date"
                  className="h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 text-text-primary focus:outline-none focus:border-accent-blue/60"
                />
              </div>

              <div className="flex flex-col gap-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">To</label>
                <input
                  type="date"
                  value={to}
                  onChange={e => setTo(e.target.value)}
                  aria-label="To date"
                  className="h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 text-text-primary focus:outline-none focus:border-accent-blue/60"
                />
              </div>

              <div className="flex flex-col gap-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">Client</label>
                <Select value={clientId} onValueChange={onClientChange}>
                  <SelectTrigger className="w-44 h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={ALL}>All clients</SelectItem>
                    {(clients ?? []).map(c => (
                      <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex flex-col gap-1">
                <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px] uppercase">Project</label>
                <Select value={projectId} onValueChange={setProjectId}>
                  <SelectTrigger className="w-44 h-9 text-xs bg-transparent border border-border-primary rounded-[11px] px-3 focus:outline-none focus:border-accent-blue/60">
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
          </div>

          {/* KPI row */}
          <KpiMarquee role="list" aria-label="Usage statistics">
            {statTiles.map((tile, i) => (
              <div key={tile.label} className="w-[232px] flex-none">
                <StatTile
                  label={tile.label}
                  value={isLoading ? '—' : tile.value}
                  sub={tile.sub}
                  icon={tile.icon}
                  accent={accentFor(i)}
                />
              </div>
            ))}
          </KpiMarquee>

          {/* Rollup table */}
          <div className={`rounded-[18px] overflow-hidden ${GLASS_PANEL}`}>
            <div className="px-5 py-4 border-b border-border-secondary flex items-center justify-between gap-3">
              <span className="text-sm font-semibold text-text-primary">
                Usage by {level}
              </span>
              {!isLoading && !isError && (
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
                    <div className="h-3.5 w-16 rounded-[5px] bg-white/[0.04] animate-pulse" />
                    <div className="h-3.5 w-16 rounded-[5px] bg-white/[0.04] animate-pulse" />
                    <div className="h-3.5 w-16 rounded-[5px] bg-white/[0.04] animate-pulse" />
                    <div className="h-3.5 w-16 rounded-[5px] bg-white/[0.04] animate-pulse" />
                    <div className="h-3.5 w-12 rounded-[5px] bg-white/[0.04] animate-pulse" />
                  </div>
                ))}
              </div>
            ) : isError ? (
              <div className="flex flex-col items-center gap-2 py-12 text-center">
                <BarChart3 className="w-6 h-6 text-status-error/60" />
                <p className="text-xs font-semibold text-text-secondary">Couldn't load usage</p>
                <p className="text-xs text-text-quaternary max-w-xs">{(error as Error)?.message ?? 'Unknown error'}</p>
              </div>
            ) : rows.length === 0 ? (
              <div className="flex flex-col items-center gap-2 py-12 text-center">
                <BarChart3 className="w-6 h-6 text-text-quaternary/50" />
                <p className="text-xs font-semibold text-text-secondary">No usage recorded</p>
                <p className="text-xs text-text-quaternary max-w-xs">
                  No usage events match these filters. Agents report usage as they run
                  {isSuperUser ? ', or run a backfill to seed time from existing sessions.' : '.'}
                </p>
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="text-text-tertiary border-b border-border-secondary">
                      <th className="text-left font-semibold px-5 py-2.5">{level === 'org' ? 'Organization' : level.charAt(0).toUpperCase() + level.slice(1)}</th>
                      <th className="text-right font-semibold px-4 py-2.5">Tokens in</th>
                      <th className="text-right font-semibold px-4 py-2.5">Tokens out</th>
                      <th className="text-right font-semibold px-4 py-2.5">Total tokens</th>
                      <th className="text-right font-semibold px-4 py-2.5">Duration</th>
                      <th className="text-right font-semibold px-5 py-2.5">Events</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border-secondary">
                    {rows.map((row: UsageSummaryRow, i) => (
                      <tr key={row.key_id ?? `${row.key_name}-${i}`} className="hover:bg-white/[0.03] transition-colors">
                        <td className="px-5 py-3 text-text-primary font-medium truncate max-w-xs">{row.key_name}</td>
                        <td className="px-4 py-3 text-right text-text-secondary tabular-nums">{row.tokens_in.toLocaleString()}</td>
                        <td className="px-4 py-3 text-right text-text-secondary tabular-nums">{row.tokens_out.toLocaleString()}</td>
                        <td className="px-4 py-3 text-right text-text-primary font-semibold tabular-nums">{row.tokens_total.toLocaleString()}</td>
                        <td className="px-4 py-3 text-right text-text-secondary tabular-nums">{formatDuration(row.duration_ms)}</td>
                        <td className="px-5 py-3 text-right text-text-secondary tabular-nums">{row.event_count.toLocaleString()}</td>
                      </tr>
                    ))}
                  </tbody>
                  {totals && (
                    <tfoot>
                      <tr className={cn('border-t border-border-secondary text-text-primary font-semibold', 'bg-white/[0.02]')}>
                        <td className="px-5 py-3">Total</td>
                        <td className="px-4 py-3 text-right tabular-nums">{totals.tokens_in.toLocaleString()}</td>
                        <td className="px-4 py-3 text-right tabular-nums">{totals.tokens_out.toLocaleString()}</td>
                        <td className="px-4 py-3 text-right tabular-nums">{totals.tokens_total.toLocaleString()}</td>
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
    </div>
  )
}
