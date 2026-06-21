import { useMemo, useState, useEffect, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { StatisticDisplay } from '@/components/ui/StatisticDisplay/StatisticDisplay'
import { Skeleton } from '@/components/ui/Skeleton/Skeleton'
import { EmptyState } from '@/components/ui/EmptyState/EmptyState'
import { ActivityItem } from '../components/ActivityItem'
import { cn } from '@/lib/utils'
import { Sparkles, X, Check, CheckCircle, Brain, Clock, Users, FolderOpen, Code2, UserPlus, FolderPlus, Download, FileText, Zap, LayoutGrid } from 'lucide-react'
import type { NameCount, DailyCount, AgentActivity, HeatmapDay, ContributorStat } from '../types'

type CardKey = 'onboarding' | 'trends' | 'heatmap' | 'contributors' | 'agent-activity' | 'usage' | 'quick-actions'
const ALL_CARDS: CardKey[] = ['onboarding', 'trends', 'heatmap', 'contributors', 'agent-activity', 'usage', 'quick-actions']
const CARDS_STORAGE_KEY = 'nexusmind-dashboard-cards'

function downloadBlob(blob: Blob, filename = 'download.json') {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

const ONBOARDING_DISMISSED_KEY = 'onboardingDismissed'

function MemoryHeatmap({ data }: { data: HeatmapDay[] }) {
  // Build a map from day-string → count
  const dayMap = new Map(data.map(d => [d.day, d.count]))

  // Generate 90 days from today-89 to today
  const days: { date: string; count: number }[] = []
  for (let i = 89; i >= 0; i--) {
    const d = new Date()
    d.setDate(d.getDate() - i)
    const key = d.toISOString().slice(0, 10) // YYYY-MM-DD
    days.push({ date: key, count: dayMap.get(key) ?? 0 })
  }

  // Pad start to align to Sunday (0)
  const firstDow = new Date(days[0].date).getDay()
  const padded = Array(firstDow).fill(null).concat(days)

  // Split into weeks (columns of 7)
  const weeks: (typeof days[0] | null)[][] = []
  for (let i = 0; i < padded.length; i += 7) {
    weeks.push(padded.slice(i, i + 7))
  }

  const maxCount = Math.max(...data.map(d => d.count), 1)

  function cellColor(count: number): string {
    if (count === 0) return 'bg-white/[0.04]'
    const intensity = count / maxCount
    if (intensity < 0.25) return 'bg-accent-blue/20'
    if (intensity < 0.5)  return 'bg-accent-blue/40'
    if (intensity < 0.75) return 'bg-accent-blue/60'
    return 'bg-accent-blue'
  }

  return (
    <div className="flex gap-[3px]">
      {weeks.map((week, wi) => (
        <div key={wi} className="flex flex-col gap-[3px]">
          {week.map((day, di) =>
            day === null ? (
              <div key={di} className="w-[10px] h-[10px]" />
            ) : (
              <div
                key={di}
                title={`${day.date}: ${day.count} memories`}
                className={`w-[10px] h-[10px] rounded-[2px] ${cellColor(day.count)} transition-colors`}
              />
            )
          )}
        </div>
      ))}
    </div>
  )
}

const TYPE_COLORS = [
  'var(--color-accent-blue)',
  'var(--color-status-success)',
  'var(--color-status-warning)',
  'var(--color-status-error)',
  '#bf5af2',
]

function relativeTime(isoString: string): string {
  const diff = Date.now() - new Date(isoString).getTime()
  const minutes = Math.floor(diff / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

export default function Dashboard() {
  const { session } = useAuth()

  const client = useMemo(
    () => createClient(),
    [session],
  )

  const isAdmin = session?.user.role === 'admin'

  const [period, setPeriod] = useState<7 | 30 | 90>(30)

  const [hiddenCards, setHiddenCards] = useState<CardKey[]>(() => {
    try { return JSON.parse(localStorage.getItem(CARDS_STORAGE_KEY) ?? '[]') }
    catch { return [] }
  })
  const [showCustomize, setShowCustomize] = useState(false)
  const customizeRef = useRef<HTMLDivElement>(null)

  const toggleCard = (key: CardKey) => {
    setHiddenCards(prev => {
      const next = prev.includes(key) ? prev.filter(k => k !== key) : [...prev, key]
      localStorage.setItem(CARDS_STORAGE_KEY, JSON.stringify(next))
      return next
    })
  }
  const isVisible = (key: CardKey) => !hiddenCards.includes(key)

  // Close customize dropdown on outside click
  useEffect(() => {
    if (!showCustomize) return
    const handler = (e: MouseEvent) => {
      if (customizeRef.current && !customizeRef.current.contains(e.target as Node)) {
        setShowCustomize(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [showCustomize])

  const { data: stats, isLoading: statsLoading, isError: statsError } = useQuery({
    queryKey: ['stats'],
    queryFn: () => client.getStats(),
    refetchInterval: 30_000,
    enabled: isAdmin,
  })

  const { data: activity, isLoading: activityLoading } = useQuery({
    queryKey: ['audit', 'recent'],
    queryFn: () => client.getAuditLog({ limit: 20 }),
    refetchInterval: 30_000,
    enabled: isAdmin,
  })

  const { data: users } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    staleTime: 60_000,
    enabled: isAdmin,
  })

  const { data: trends, isLoading: trendsLoading } = useQuery({
    queryKey: ['memory-trends', period],
    queryFn: () => client.getMemoryTrends(period),
    refetchInterval: 60_000,
    enabled: isAdmin,
  })

  const { data: onboarding } = useQuery({
    queryKey: ['onboarding'],
    queryFn: () => client.getOnboarding(),
    staleTime: 60_000,
    enabled: isAdmin,
  })

  const { data: usageStats, isLoading: usageLoading } = useQuery({
    queryKey: ['usage-stats'],
    queryFn: () => client.getUsageStats(),
    staleTime: 60_000,
    enabled: isAdmin,
  })

  const { data: agentActivity, isLoading: agentActivityLoading } = useQuery({
    queryKey: ['agent-activity', period],
    queryFn: () => client.getAgentActivity(period),
    refetchInterval: 60_000,
    enabled: isAdmin,
  })

  const { data: heatmapData } = useQuery({
    queryKey: ['memory-heatmap', period],
    queryFn: () => client.getMemoryHeatmap(period),
    staleTime: 60_000,
    enabled: isAdmin,
  })

  const { data: contributors, isLoading: contributorsLoading } = useQuery({
    queryKey: ['top-contributors', period],
    queryFn: () => client.getTopContributors(period),
    staleTime: 60_000,
    enabled: isAdmin,
  })

  const [dismissed, setDismissed] = useState(
    () => localStorage.getItem(ONBOARDING_DISMISSED_KEY) === 'true'
  )
  const [allDoneVisible, setAllDoneVisible] = useState(false)

  const doneCount = onboarding?.items.filter(i => i.done).length ?? 0
  const totalCount = onboarding?.items.length ?? 0
  const allDone = totalCount > 0 && doneCount === totalCount
  const hasIncomplete = totalCount > 0 && doneCount < totalCount

  // Auto-hide after 3s when all done
  useEffect(() => {
    if (allDone && !dismissed) {
      setAllDoneVisible(true)
      const t = setTimeout(() => setDismissed(true), 3000)
      return () => clearTimeout(t)
    }
  }, [allDone, dismissed])

  const showOnboarding = isAdmin && onboarding && !dismissed && (hasIncomplete || allDoneVisible)

  function handleDismiss() {
    localStorage.setItem(ONBOARDING_DISMISSED_KEY, 'true')
    setDismissed(true)
  }

  const userMap = useMemo(() => {
    const map = new Map<string, string>()
    users?.forEach(u => map.set(u.id, u.name))
    return map
  }, [users])

  const metrics = useMemo(() => {
    const base = stats ? [
      {
        id: 'total-memories',
        label: 'Total Memories',
        value: stats.total_memories.toLocaleString(),
      },
      {
        id: 'active-users',
        label: 'Active Users (24h)',
        value: stats.active_users_24h.toLocaleString(),
      },
      {
        id: 'searches-today',
        label: 'Searches Today',
        value: stats.searches_today.toLocaleString(),
      },
      {
        id: 'top-tool',
        label: 'Top Tool',
        value: stats.top_tools[0]?.tool ?? '—',
      },
    ] : []
    const extra = trends ? [
      {
        id: 'this-week',
        label: 'This Week',
        value: trends.this_week.toLocaleString(),
      },
      {
        id: 'this-month',
        label: 'This Month',
        value: trends.this_month.toLocaleString(),
      },
    ] : []
    return [...base, ...extra]
  }, [stats, trends])

  return (
    <div className="p-8 space-y-8 max-w-7xl mx-auto">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Dashboard</h1>
          <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">
            {session?.org.name} — organization overview
          </p>
        </div>
        {isAdmin && (
          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1 bg-white/[0.04] rounded-full p-0.5">
              {([7, 30, 90] as const).map(d => (
                <button
                  key={d}
                  onClick={() => setPeriod(d)}
                  className={cn(
                    'px-3 py-1 rounded-full text-xs transition-colors',
                    period === d
                      ? 'bg-[#272729] text-text-primary font-semibold shadow-sm'
                      : 'text-text-quaternary hover:text-text-secondary'
                  )}
                >
                  {d}d
                </button>
              ))}
            </div>

            {/* Customize dropdown */}
            <div ref={customizeRef} className="relative">
              <button
                onClick={() => setShowCustomize(prev => !prev)}
                className="border border-border-primary rounded-full px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors"
              >
                <LayoutGrid className="w-3 h-3" /> Customize
              </button>
              {showCustomize && (
                <div className="absolute right-0 top-full mt-2 bg-[#272729] border border-border-primary rounded-[11px] py-2 min-w-[200px] shadow-xl z-20">
                  {ALL_CARDS.map(key => (
                    <label key={key} className="flex items-center justify-between px-3 py-2 hover:bg-white/[0.04] cursor-pointer">
                      <span className="text-xs text-text-secondary capitalize">{key.replace(/-/g, ' ')}</span>
                      <button
                        onClick={() => toggleCard(key)}
                        className={cn(
                          'w-8 h-4 rounded-full transition-colors relative shrink-0',
                          isVisible(key) ? 'bg-accent-blue' : 'bg-white/[0.12]'
                        )}
                        aria-label={`${isVisible(key) ? 'Hide' : 'Show'} ${key} card`}
                      >
                        <span className={cn(
                          'absolute top-0.5 w-3 h-3 rounded-full bg-white transition-transform',
                          isVisible(key) ? 'translate-x-4' : 'translate-x-0.5'
                        )} />
                      </button>
                    </label>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {/* Onboarding checklist */}
      {isVisible('onboarding') && showOnboarding && (
        <section aria-label="Getting started checklist">
          <div className="border border-accent-blue/20 rounded-[18px] p-5 bg-accent-blue/[0.04] space-y-4">
            {/* Header */}
            <div className="flex items-center justify-between">
              <div className="flex items-center">
                <Sparkles className="w-4 h-4 text-accent-blue" />
                <span className="text-sm font-semibold text-text-primary ml-2">Getting started</span>
              </div>
              <button
                onClick={handleDismiss}
                aria-label="Dismiss"
                className="text-text-quaternary hover:text-text-tertiary transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {allDone ? (
              <div className="text-center py-2 space-y-1">
                <CheckCircle className="w-6 h-6 text-status-success mx-auto" />
                <p className="text-sm font-semibold text-text-primary">You're all set!</p>
              </div>
            ) : (
              <>
                {/* Progress bar */}
                <div className="w-full">
                  <div className="h-1 bg-[#272729] rounded-full w-full">
                    <div
                      className="h-1 bg-accent-blue rounded-full transition-all duration-500"
                      style={{ width: `${totalCount > 0 ? (doneCount / totalCount) * 100 : 0}%` }}
                    />
                  </div>
                  <span className="text-[11px] text-text-quaternary">{doneCount} of {totalCount} complete</span>
                </div>

                {/* Items */}
                <ul className="space-y-2.5">
                  {onboarding!.items.map(item => (
                    <li key={item.key} className="flex items-start gap-3">
                      {item.done ? (
                        <div className="w-5 h-5 rounded-full bg-status-success/15 border border-status-success/30 flex items-center justify-center shrink-0 mt-0.5">
                          <Check className="w-3 h-3 text-status-success" />
                        </div>
                      ) : (
                        <div className="w-5 h-5 rounded-full border border-border-primary flex items-center justify-center shrink-0 mt-0.5" />
                      )}
                      <div>
                        <p className={`text-sm leading-tight ${item.done ? 'line-through text-text-quaternary' : 'text-text-secondary'}`}>
                          {item.label}
                        </p>
                        <p className="text-xs text-text-tertiary mt-0.5">
                          {item.description}
                        </p>
                      </div>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        </section>
      )}

      {/* Stat cards */}
      {isAdmin && (
        <section aria-label="Organization statistics">
          {statsError ? (
            <div className="rounded-[18px] border border-status-error/30 bg-status-error/10 p-4 text-sm text-status-error">
              Failed to load statistics. Check your connection and try again.
            </div>
          ) : statsLoading || trendsLoading ? (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
              {Array.from({ length: 6 }).map((_, i) => (
                <Skeleton key={i} className="h-32 rounded-[18px]" />
              ))}
            </div>
          ) : (
            <StatisticDisplay
              metrics={metrics}
              columns={3}
              variant="card"
              size="md"
            />
          )}
        </section>
      )}

      {/* Activity timeline */}
      {isAdmin && (
        <section aria-label="Recent activity">
          <h2 className="text-[15px] font-semibold text-text-secondary mb-4 tracking-[-0.15px]">
            Recent Activity
          </h2>
          <div className="bg-[#272729] border border-white/[0.06] rounded-[18px] px-6 divide-y divide-border-secondary">
            {activityLoading ? (
              Array.from({ length: 5 }).map((_, i) => (
                <div key={i} className="py-3">
                  <Skeleton className="h-6 w-full rounded-[5px]" />
                </div>
              ))
            ) : !activity || activity.length === 0 ? (
              <div className="py-8">
                <EmptyState title="No activity yet" description="Actions performed by your team will appear here." />
              </div>
            ) : (
              activity.map((entry) => (
                <ActivityItem
                  key={entry.id}
                  entry={entry}
                  userName={userMap.get(entry.user_id)}
                />
              ))
            )}
          </div>
        </section>
      )}

      {/* Usage stats + Agent Activity */}
      {isAdmin && (isVisible('usage') || isVisible('agent-activity')) && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {/* Usage */}
          {isVisible('usage') && <div className="border border-border-primary rounded-[18px] p-5 space-y-3">
            <p className="text-[12px] tracking-[-0.12px] text-text-tertiary">Usage</p>
            {usageLoading ? (
              Array.from({ length: 5 }).map((_, i) => (
                <div key={i} className="flex items-center justify-between animate-pulse">
                  <div className="h-3 w-24 rounded-[8px] bg-[#272729]" />
                  <div className="h-3 w-10 rounded-[8px] bg-[#272729]" />
                </div>
              ))
            ) : usageStats ? (
              <>
                {([
                  { icon: Brain, label: 'Memories', value: usageStats.memories },
                  { icon: Clock, label: 'Sessions', value: usageStats.sessions },
                  { icon: Users, label: 'Users', value: usageStats.users },
                  { icon: FolderOpen, label: 'Projects', value: usageStats.projects },
                  { icon: Code2, label: 'Code Repos', value: usageStats.code_repos },
                ] as const).map(({ icon: Icon, label, value }) => (
                  <div key={label} className="flex items-center gap-3">
                    <Icon className="w-3.5 h-3.5 text-text-quaternary shrink-0" />
                    <span className="text-xs text-text-secondary flex-1">{label}</span>
                    <span className="text-xs font-semibold text-text-primary tabular-nums">
                      {value.toLocaleString()}
                    </span>
                  </div>
                ))}
              </>
            ) : (
              <div className="text-xs text-text-quaternary text-center py-4">No data yet</div>
            )}
          </div>}

          {/* Agent Activity */}
          {isVisible('agent-activity') && <div className="rounded-[18px] bg-[#272729] border border-border-primary p-5 space-y-4">
            <p className="text-sm font-semibold text-text-primary">Agent Activity</p>
            {agentActivityLoading ? (
              Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="space-y-1.5 animate-pulse">
                  <div className="h-3 w-32 rounded bg-[#1d1d1f]" />
                  <div className="h-1 w-full rounded-full bg-[#1d1d1f]" />
                  <div className="h-2 w-20 rounded bg-[#1d1d1f]" />
                </div>
              ))
            ) : !agentActivity || agentActivity.length === 0 ? (
              <div className="text-xs text-text-quaternary text-center py-4">No agent activity yet</div>
            ) : (() => {
              const maxMemoriesLast7d = Math.max(...(agentActivity as AgentActivity[]).map(a => a.memories_last_7d), 1)
              return (agentActivity as AgentActivity[]).map(agent => (
                <div key={agent.tool} className="space-y-1.5">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-semibold text-text-secondary truncate">{agent.tool}</span>
                    <div className="flex items-center gap-2 shrink-0">
                      {agent.memories_last_24h > 0 && (
                        <span className="w-1.5 h-1.5 rounded-full bg-status-success" />
                      )}
                      <span className="text-[10px] text-text-quaternary">{agent.memories_last_7d} this week</span>
                    </div>
                  </div>
                  <div className="h-1 bg-[#1d1d1f] rounded-full">
                    <div
                      className="h-1 bg-accent-blue/60 rounded-full transition-all duration-500"
                      style={{ width: `${(agent.memories_last_7d / maxMemoriesLast7d) * 100}%` }}
                    />
                  </div>
                  <p className="text-[10px] text-text-quaternary">Last seen {relativeTime(agent.last_seen)}</p>
                </div>
              ))
            })()}
          </div>}
        </div>
      )}

      {/* Memory analytics */}
      {isAdmin && isVisible('trends') && (
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">

          {/* Last 30 days sparkline */}
          {trendsLoading ? (
            <div className="border border-border-primary rounded-[18px] p-5 animate-pulse">
              <div className="h-4 bg-[#272729] rounded w-1/3 mb-4" />
              <div className="h-12 bg-[#272729] rounded" />
            </div>
          ) : trends ? (
            <div className="border border-border-primary rounded-[18px] p-5">
              <p className="text-[10px] text-text-quaternary mb-3">Last {period} Days</p>
              {trends.daily_counts.length === 0 ? (
                <div className="text-xs text-text-quaternary text-center py-4">No data yet</div>
              ) : (() => {
                const maxDay = Math.max(...trends.daily_counts.map((d: DailyCount) => d.count), 1)
                const first = trends.daily_counts[0]?.date ?? ''
                const mid = trends.daily_counts[Math.floor(trends.daily_counts.length / 2)]?.date ?? ''
                const last = trends.daily_counts[trends.daily_counts.length - 1]?.date ?? ''
                return (
                  <>
                    <div className="flex items-end gap-[2px] h-12">
                      {trends.daily_counts.map((d: DailyCount) => (
                        <div
                          key={d.date}
                          className="flex-1 bg-accent-blue/40 rounded-t-[2px] min-h-[2px] hover:bg-accent-blue/70 transition-colors cursor-default"
                          style={{ height: `${Math.max((d.count / maxDay) * 100, 2)}%` }}
                          title={`${d.date}: ${d.count}`}
                        />
                      ))}
                    </div>
                    <div className="flex justify-between mt-1">
                      <span className="text-[10px] text-text-quaternary">{first}</span>
                      <span className="text-[10px] text-text-quaternary">{mid}</span>
                      <span className="text-[10px] text-text-quaternary">{last}</span>
                    </div>
                  </>
                )
              })()}
            </div>
          ) : null}

          {/* Top Projects */}
          {trendsLoading ? (
            <div className="border border-border-primary rounded-[18px] p-5 animate-pulse">
              <div className="h-4 bg-[#272729] rounded w-1/3 mb-4" />
              <div className="h-12 bg-[#272729] rounded" />
            </div>
          ) : trends ? (
            <div className="border border-border-primary rounded-[18px] p-5 space-y-3">
              <p className="text-[12px] tracking-[-0.12px] text-text-tertiary mb-3">Top Projects</p>
              {trends.by_project.length === 0 ? (
                <div className="text-xs text-text-quaternary text-center py-4">No data yet</div>
              ) : (() => {
                const maxCount = Math.max(...trends.by_project.map((p: NameCount) => p.count), 1)
                return (
                  <>
                    {trends.by_project.map((p: NameCount) => (
                      <div key={p.name} className="space-y-1">
                        <div className="flex items-center justify-between">
                          <span className="text-xs text-text-secondary truncate max-w-[60%]">{p.name}</span>
                          <span className="text-xs text-text-quaternary">{p.count.toLocaleString()}</span>
                        </div>
                        <div className="h-1 bg-[#272729] rounded-full">
                          <div
                            className="h-1 bg-accent-blue/50 rounded-full transition-all"
                            style={{ width: `${(p.count / maxCount) * 100}%` }}
                          />
                        </div>
                      </div>
                    ))}
                  </>
                )
              })()}
            </div>
          ) : null}

          {/* Memory Types */}
          {trendsLoading ? (
            <div className="border border-border-primary rounded-[18px] p-5 animate-pulse">
              <div className="h-4 bg-[#272729] rounded w-1/3 mb-4" />
              <div className="h-12 bg-[#272729] rounded" />
            </div>
          ) : trends ? (
            <div className="border border-border-primary rounded-[18px] p-5 space-y-3">
              <p className="text-[12px] tracking-[-0.12px] text-text-tertiary mb-3">Memory Types</p>
              {trends.by_type.length === 0 ? (
                <div className="text-xs text-text-quaternary text-center py-4">No data yet</div>
              ) : (() => {
                const maxTypeCount = Math.max(...trends.by_type.map((t: NameCount) => t.count), 1)
                return (
                  <>
                    {trends.by_type.map((t: NameCount, i: number) => (
                      <div key={t.name} className="space-y-1">
                        <div className="flex justify-between items-center">
                          <span className="text-xs text-text-secondary truncate max-w-[60%]">{t.name || 'unset'}</span>
                          <span className="text-xs font-semibold text-text-primary">{t.count}</span>
                        </div>
                        <div className="h-1 bg-[#1d1d1f] rounded-full">
                          <div
                            className="h-1 rounded-full transition-all duration-500"
                            style={{
                              width: `${(t.count / maxTypeCount) * 100}%`,
                              backgroundColor: TYPE_COLORS[i % TYPE_COLORS.length],
                            }}
                          />
                        </div>
                      </div>
                    ))}
                  </>
                )
              })()}
            </div>
          ) : null}

        </div>
      )}

      {/* Memory Activity Heatmap */}
      {isAdmin && isVisible('heatmap') && (
        <div className="bg-[#272729] rounded-[18px] p-5 border border-border-primary">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-sm font-semibold text-text-primary">Memory Activity</h3>
            <span className="text-[10px] text-text-quaternary">Last {period} days</span>
          </div>
          {heatmapData ? (
            <MemoryHeatmap data={heatmapData} />
          ) : (
            <div className="h-[78px] bg-white/[0.04] animate-pulse rounded-[8px]" />
          )}
          <div className="flex items-center gap-1 mt-3">
            <span className="text-[10px] text-text-quaternary">Less</span>
            {(['bg-white/[0.04]', 'bg-accent-blue/20', 'bg-accent-blue/40', 'bg-accent-blue/60', 'bg-accent-blue'] as const).map((c, i) => (
              <div key={i} className={`w-[10px] h-[10px] rounded-[2px] ${c}`} />
            ))}
            <span className="text-[10px] text-text-quaternary">More</span>
          </div>
        </div>
      )}

      {/* Top Contributors */}
      {isAdmin && isVisible('contributors') && (
        <div className="bg-[#272729] rounded-[18px] p-5 border border-border-primary">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-sm font-semibold text-text-primary">Top Contributors</h3>
            <span className="text-[10px] text-text-quaternary">Last {period} days</span>
          </div>
          {contributorsLoading ? (
            <div className="space-y-3">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="animate-pulse bg-white/[0.04] rounded-[8px] h-5" />
              ))}
            </div>
          ) : contributors && contributors.length > 0 ? (
            <div className="space-y-3">
              {(contributors as ContributorStat[]).map((c, i) => {
                const max = contributors[0].memory_count
                return (
                  <div key={c.agent_id} className="flex items-center gap-3">
                    <span className="text-[10px] text-text-quaternary w-3 text-right">{i + 1}</span>
                    <span className="text-xs text-text-secondary truncate flex-1 font-mono">{c.agent_id}</span>
                    <div className="w-24 h-1 bg-white/[0.06] rounded-full overflow-hidden">
                      <div
                        className="h-full bg-accent-blue rounded-full"
                        style={{ width: `${(c.memory_count / max) * 100}%` }}
                      />
                    </div>
                    <span className="text-[10px] text-text-quaternary w-6 text-right">{c.memory_count}</span>
                  </div>
                )
              })}
            </div>
          ) : (
            <p className="text-xs text-text-quaternary">No activity in the last 30 days.</p>
          )}
        </div>
      )}

      {/* Quick Actions */}
      {isAdmin && isVisible('quick-actions') && (() => {
        const QUICK_ACTIONS = [
          { label: 'Invite user', href: '/users', icon: UserPlus },
          { label: 'New collection', href: '/memories?tab=collections', icon: FolderPlus },
          { label: 'Export config', action: () => createClient().exportOrgConfig().then(b => downloadBlob(b, 'nexusmind-config.json')), icon: Download },
          { label: 'View audit log', href: '/audit', icon: FileText },
          { label: 'Manage webhooks', href: '/settings', icon: Zap },
        ] as const
        return (
          <div className="bg-[#272729] rounded-[18px] p-5 border border-border-primary">
            <h3 className="text-sm font-semibold text-text-primary mb-4">Quick Actions</h3>
            <div className="grid grid-cols-2 gap-2">
              {QUICK_ACTIONS.map(action => (
                'href' in action ? (
                  <Link
                    key={action.label}
                    to={action.href}
                    className="flex items-center gap-2 px-3 py-2 rounded-[8px] bg-white/[0.03] hover:bg-white/[0.06] text-xs text-text-secondary hover:text-text-primary transition-colors border border-border-secondary/30"
                  >
                    <action.icon className="w-3 h-3" />
                    {action.label}
                  </Link>
                ) : (
                  <button
                    key={action.label}
                    onClick={action.action}
                    className="flex items-center gap-2 px-3 py-2 rounded-[8px] bg-white/[0.03] hover:bg-white/[0.06] text-xs text-text-secondary hover:text-text-primary transition-colors border border-border-secondary/30"
                  >
                    <action.icon className="w-3 h-3" />
                    {action.label}
                  </button>
                )
              ))}
            </div>
          </div>
        )
      })()}

      {!isAdmin && (
        <div className="border border-white/[0.08] bg-[#272729] rounded-[18px] p-6 max-w-xl">
          <p className="text-sm text-text-secondary leading-relaxed">
            Welcome to <strong>{session?.org.name}</strong> on NexusMind.
          </p>
          <p className="text-xs text-text-tertiary mt-2">
            Use the navigation sidebar to browse, search, and manage your team's shared AI memories.
          </p>
        </div>
      )}
    </div>
  )
}
