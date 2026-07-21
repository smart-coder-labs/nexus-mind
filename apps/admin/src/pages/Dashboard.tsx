import { useMemo, useState, useEffect, useRef } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { Skeleton } from '@/components/ui/Skeleton/Skeleton'
import { EmptyState } from '@/components/ui/EmptyState/EmptyState'
import { Badge } from '@/components/ui/Badge/Badge'
import { KpiMarquee } from '@/components/ui/KpiMarquee'
import { Switch } from '@/components/ui/Switch/Switch'
import { cn } from '@/lib/utils'
import type { LucideIcon } from 'lucide-react'
import {
  Brain, Clock, Users, FolderOpen, Code2, UserPlus, FolderPlus, Download, FileText, Zap,
  LayoutGrid, BookMarked, ChevronRight, Search,
} from 'lucide-react'
import type { DailyCount, AgentActivity, HeatmapDay, ContributorStat, Convention, DashboardAvailability } from '../types'
import { StatTile } from './dashboard/StatTile'
import { QuickActionsRow, type QuickAction } from './dashboard/QuickActionsRow'
import { GettingStartedPopover } from './dashboard/GettingStartedPopover'
import { MemoryHealthCard } from './dashboard/MemoryHealthCard'
import { MemoryTypesCard } from './dashboard/MemoryTypesCard'
import { TopProjectsCard } from './dashboard/TopProjectsCard'
import { accentFor } from './dashboard/colors'

type CardKey =
  | 'onboarding' | 'quick-actions' | 'recent-activity' | 'memory-trends' | 'memory-types'
  | 'agent-activity' | 'memory-health' | 'top-projects' | 'contributors' | 'usage' | 'heatmap' | 'conventions'
const ALL_CARDS: CardKey[] = [
  'onboarding', 'quick-actions', 'recent-activity', 'memory-trends', 'memory-types',
  'agent-activity', 'memory-health', 'top-projects', 'contributors', 'usage', 'heatmap', 'conventions',
]
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
const GS_MINIMIZED_KEY = 'nexusmind-dashboard-gs-minimized'

// Keyboard focus indicator (design direction §6): 2px --color-focus-ring outline
// with a 2px offset. Uses outline (not ring) so it isn't clipped by overflow-hidden
// ancestors. Both aliases are identical now; kept for call-site readability.
const FOCUS_CANVAS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'
const FOCUS_TILE = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to avoid pulling the SDD page module graph into the Dashboard page.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

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

function UnavailableState() {
  return (
    <p role="status" className="text-[13px] text-text-tertiary text-center py-4">
      Unavailable for scoped administrators.
    </p>
  )
}

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

// --- Timeline helpers ---
function timelineActionVariant(action: string): 'primary' | 'success' | 'error' | 'warning' | 'default' {
  const a = action.split('.').pop() ?? action
  if (a === 'store' || a === 'create' || a === 'invite') return 'success'
  if (a === 'search' || a === 'query') return 'primary'
  if (a === 'delete' || a === 'revoke' || a === 'remove') return 'error'
  if (a === 'update' || a === 'edit') return 'warning'
  return 'default'
}

function timelineDotClass(action: string): string {
  const variant = timelineActionVariant(action)
  const map: Record<string, string> = {
    success: 'bg-status-success',
    primary: 'bg-accent-blue',
    error:   'bg-status-error',
    warning: 'bg-status-warning',
    default: 'bg-white/[0.20]',
  }
  return map[variant]
}

function formatAbsTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function dayLabel(iso: string): string {
  const d = new Date(iso)
  const today = new Date()
  const yesterday = new Date(today)
  yesterday.setDate(today.getDate() - 1)
  if (d.toDateString() === today.toDateString()) return 'Today'
  if (d.toDateString() === yesterday.toDateString()) return 'Yesterday'
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

export default function Dashboard() {
  const { session } = useAuth()
  const navigate = useNavigate()

  const client = useMemo(
    () => createClient(),
    [session],
  )

  const isAdmin = session?.user.role === 'admin'
  const isSuperUser = session?.user.role === 'super_user'
  const hasAdminAccess = isAdmin || isSuperUser

  const [period, setPeriod] = useState<7 | 30 | 90>(30)

  const [hiddenCards, setHiddenCards] = useState<CardKey[]>(() => {
    try { return JSON.parse(localStorage.getItem(CARDS_STORAGE_KEY) ?? '[]') }
    catch { return [] }
  })
  const [showCustomize, setShowCustomize] = useState(false)
  const customizeRef = useRef<HTMLDivElement>(null)
  // Expanded activity rows (rich search events drill down into a result tree)
  const [expandedActivity, setExpandedActivity] = useState<Set<string>>(new Set())
  const toggleActivity = (id: string) =>
    setExpandedActivity(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

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

  const { data: dashboardData, isLoading: dashboardLoading, isError: statsError } = useQuery({
    queryKey: ['dashboard', period],
    queryFn: () => client.getDashboard(period),
    refetchInterval: 30_000,
    enabled: hasAdminAccess,
  })
  const stats = dashboardData?.stats
  const activity = dashboardData?.activity
  const users = dashboardData?.users
  const trends = dashboardData?.trends
  const usageStats = dashboardData?.usage
  const agentActivity = dashboardData?.agent_activity
  const heatmapData = dashboardData?.heatmap
  const contributors = dashboardData?.contributors
  const healthData = dashboardData?.health
  const onboarding = dashboardData?.onboarding
  const conventions: Convention[] | null = dashboardData?.conventions ?? null
  const statsLoading = dashboardLoading
  const trendsLoading = dashboardLoading
  const usageLoading = dashboardLoading
  const activityLoading = dashboardLoading
  const agentActivityLoading = dashboardLoading
  const contributorsLoading = dashboardLoading
  const availability: DashboardAvailability | undefined = dashboardData?.availability
  const isAvailable = (widget: keyof DashboardAvailability) => availability?.[widget] ?? false

  const conventionStats = useMemo(() => {
    if (!conventions) return []
    const counts = new Map<string, number>()
    conventions.forEach(c => counts.set(c.category ?? 'uncategorized', (counts.get(c.category ?? 'uncategorized') ?? 0) + 1))
    return [...counts.entries()].map(([category, count]) => ({ category, count })).sort((a, b) => b.count - a.count)
  }, [conventions])

  const [dismissed, setDismissed] = useState(
    () => localStorage.getItem(ONBOARDING_DISMISSED_KEY) === 'true'
  )
  const [allDoneVisible, setAllDoneVisible] = useState(false)
  const [gsMinimized, setGsMinimized] = useState(
    () => localStorage.getItem(GS_MINIMIZED_KEY) === 'true'
  )

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

  const showOnboarding = hasAdminAccess && onboarding && !dismissed && (hasIncomplete || allDoneVisible)

  const userMap = useMemo(() => {
    const map = new Map<string, string>()
    users?.forEach(u => map.set(u.id, u.name))
    return map
  }, [users])

  // Stat tiles (design delta 1). Every value comes straight from real queries
  // already fetched above. Sparklines are only attached to "Total Memories"
  // because MemoryTrends.daily_counts is the only per-day series the backend
  // exposes — the other metrics (sessions/active users/searches/top tool/
  // conventions) have no daily history endpoint, so their tiles render
  // without a sparkline rather than fabricating one.
  const statTiles = useMemo(() => {
    if (!stats) return []
    const dailySpark = trends?.daily_counts?.length ? trends.daily_counts.map((d: DailyCount) => d.count) : undefined
    const tiles: { id: string; label: string; value: string; sub?: string; icon: LucideIcon; sparkline?: number[] }[] = [
      {
        id: 'total-memories',
        label: 'Total Memories',
        value: stats.total_memories.toLocaleString(),
        sub: trends ? `${trends.this_week.toLocaleString()} new this week` : undefined,
        icon: Brain,
        sparkline: dailySpark,
      },
      {
        id: 'sessions',
        label: 'Sessions',
        value: usageStats?.sessions.toLocaleString() ?? '—',
        sub: isAvailable('usage') ? 'All time total' : 'Unavailable for scoped administrators',
        icon: Clock,
      },
      {
        id: 'active-users',
        label: 'Active Users (24h)',
        value: stats.active_users_24h.toLocaleString(),
        sub: isAvailable('users') && users ? `of ${users.length.toLocaleString()} users` : 'Unavailable for scoped administrators',
        icon: Users,
      },
      {
        id: 'searches-today',
        label: 'Searches Today',
        value: stats.searches_today.toLocaleString(),
        sub: 'Since midnight',
        icon: Search,
      },
      {
        id: 'top-tool',
        label: 'Top Tool',
        value: stats.top_tools[0]?.tool ?? '—',
        sub: stats.top_tools[0] ? `${stats.top_tools[0].count.toLocaleString()} uses recorded` : undefined,
        icon: Code2,
      },
      {
        id: 'conventions',
        label: 'Conventions',
        value: conventions?.length.toLocaleString() ?? '—',
        sub: isAvailable('conventions')
          ? (conventionStats.length ? `${conventionStats.length} categories` : undefined)
          : 'Unavailable for scoped administrators',
        icon: BookMarked,
      },
    ]
    return tiles
  }, [stats, trends, usageStats, users, conventions, conventionStats, availability])

  const quickActions: QuickAction[] = [
    { label: 'Invite user', href: '/users', icon: UserPlus },
    { label: 'New collection', href: '/memories?tab=collections', icon: FolderPlus },
    ...(isSuperUser ? [{ label: 'Export config', icon: Download, onAction: () => client.exportOrgConfig().then(b => downloadBlob(b, 'nexusmind-config.json')) }] : []),
    ...(isSuperUser ? [{ label: 'View audit log', href: '/audit', icon: FileText }] : []),
    { label: 'Manage webhooks', href: '/settings', icon: Zap },
  ]

  return (
    <div className="p-6 space-y-6 max-w-7xl mx-auto">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">Dashboard</h1>
          <p className="text-[13px] text-text-secondary mt-1">
            {session?.org.name} — organization overview
          </p>
        </div>
        {hasAdminAccess && (
          <div className="flex flex-col items-end gap-2.5">
            <div className="flex items-center gap-2">
              <div className="flex items-center gap-1 bg-white/[0.04] rounded-full p-0.5">
                {([7, 30, 90] as const).map(d => (
                  <button
                    key={d}
                    onClick={() => setPeriod(d)}
                    className={cn(
                      'text-[11px] px-2 py-0.5 rounded-full transition-colors',
                      FOCUS_CANVAS,
                      period === d
                        ? 'bg-accent-blue/10 text-accent-blue'
                        : 'text-text-tertiary hover:text-text-secondary'
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
                  className={cn('border border-border-primary rounded-full px-2.5 py-1 text-[13px] text-text-secondary hover:text-text-primary flex items-center gap-1.5 transition-colors', FOCUS_CANVAS)}
                >
                  <LayoutGrid className="w-3 h-3" /> Customize
                </button>
                {showCustomize && (
                  <div className="absolute right-0 top-full mt-2 border border-white/[0.10] bg-[#111319]/[0.95] backdrop-blur-[14px] shadow-[0_10px_34px_rgba(0,0,0,0.6)] rounded-[12px] p-[5px] min-w-[200px] z-20">
                    {ALL_CARDS.map(key => (
                      <label key={key} className="flex items-center justify-between gap-[10px] px-[11px] py-[9px] rounded-[8px] hover:bg-white/[0.06] cursor-pointer">
                        <span className="text-[12.5px] text-text-secondary capitalize">{key.replace(/-/g, ' ')}</span>
                        <Switch
                          size="sm"
                          checked={isVisible(key)}
                          onCheckedChange={() => toggleCard(key)}
                          aria-label={`${isVisible(key) ? 'Hide' : 'Show'} ${key} card`}
                        />
                      </label>
                    ))}
                  </div>
                )}
              </div>
            </div>
            {isVisible('quick-actions') && <QuickActionsRow actions={quickActions} />}
          </div>
        )}
      </div>

      {/* Stat tiles + floating Getting Started popover (design delta 1 & 2) */}
      {hasAdminAccess && (
        <section aria-label="Organization statistics" className="relative">
          {statsError ? (
            <div className="rounded-[18px] border border-status-error/30 bg-status-error/10 p-4 text-[13px] text-status-error">
              Failed to load statistics. Check your connection and try again.
            </div>
          ) : statsLoading || trendsLoading || usageLoading ? (
            <div className="grid grid-cols-2 sm:grid-cols-3 xl:grid-cols-6 gap-4">
              {Array.from({ length: 6 }).map((_, i) => (
                <Skeleton key={i} className="h-[128px] rounded-[18px]" />
              ))}
            </div>
          ) : (
            <KpiMarquee role="list" aria-label="Key statistics">
              {statTiles.map((tile, i) => (
                <div key={tile.id} className="w-[232px] flex-none">
                  <StatTile
                    label={tile.label}
                    value={tile.value}
                    sub={tile.sub}
                    icon={tile.icon}
                    accent={accentFor(i)}
                    sparkline={tile.sparkline}
                  />
                </div>
              ))}
            </KpiMarquee>
          )}

          {isVisible('onboarding') && showOnboarding && onboarding && (
            <GettingStartedPopover
              items={onboarding.items}
              doneCount={doneCount}
              totalCount={totalCount}
              allDone={allDone}
              minimized={gsMinimized}
              onMinimize={() => { setGsMinimized(true); localStorage.setItem(GS_MINIMIZED_KEY, 'true') }}
              onExpand={() => { setGsMinimized(false); localStorage.setItem(GS_MINIMIZED_KEY, 'false') }}
              onNavigate={(href) => navigate(href)}
            />
          )}
          {isVisible('onboarding') && !dashboardLoading && !isAvailable('onboarding') && (
            <div className={`mt-4 rounded-[18px] p-5 ${GLASS_PANEL}`}>
              <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Getting Started</h2>
              <UnavailableState />
            </div>
          )}
        </section>
      )}

      {/* Two-column layout (design delta 9) */}
      {hasAdminAccess && (
        <div className="grid grid-cols-1 lg:grid-cols-[1.55fr_1fr] gap-5 items-start">

          {/* LEFT column */}
          <div className="flex flex-col gap-5 min-w-0">

            {/* Recent Activity (design delta 8) */}
            {isVisible('recent-activity') && (
              <section aria-label="Recent activity" className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
                <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-4">
                  Recent Activity
                </h2>
                {activityLoading ? (
                  <div className="space-y-4">
                    {Array.from({ length: 5 }).map((_, i) => (
                      <div key={i} className="flex gap-3">
                        <Skeleton className="w-[15px] h-[15px] rounded-full mt-0.5 shrink-0" />
                        <Skeleton className="h-9 flex-1 rounded-[8px]" />
                      </div>
                    ))}
                  </div>
                ) : !activity || activity.length === 0 ? (
                  <div className="py-8">
                    <EmptyState title="No activity yet" description="Actions performed by your team will appear here." />
                  </div>
                ) : (() => {
                  // Group entries by calendar day
                  const groups: { label: string; entries: typeof activity }[] = []
                  const seen = new Map<string, typeof activity>()
                  for (const entry of activity) {
                    const label = dayLabel(entry.timestamp)
                    if (!seen.has(label)) seen.set(label, [])
                    seen.get(label)!.push(entry)
                  }
                  for (const [label, entries] of seen) {
                    groups.push({ label, entries })
                  }
                  return (
                    <div className="space-y-5">
                      {groups.map(({ label, entries }) => (
                        <div key={label}>
                          <p className="text-[11px] font-semibold text-text-tertiary uppercase tracking-wider mb-3 pl-5">
                            {label}
                          </p>
                          <div className="relative">
                            {/* Vertical connector line */}
                            <div
                              className="absolute left-[7px] top-2 bottom-2 w-px bg-border-primary"
                              aria-hidden="true"
                            />
                            <ul className="space-y-2.5">
                              {/* Rich search events (with a query + returned results) render as
                                  an expandable tree drilling into results grouped by project.
                                  Plain events collapse consecutive identical ones into one "×N"
                                  row so bursts don't read as a wall of identical lines. */}
                              {(() => {
                                type E = typeof entries[number]
                                type Item =
                                  | { kind: 'run'; entry: E; count: number; lastTimestamp: string }
                                  | { kind: 'detail'; entry: E }
                                const meta = (e: E) => (e.metadata ?? {}) as Record<string, unknown>
                                const isRich = (e: E) => {
                                  const a = e.action.toLowerCase()
                                  if (a.includes('search')) return Array.isArray(meta(e).results) || typeof meta(e).query === 'string'
                                  if (a.includes('store')) return typeof meta(e).preview === 'string' || typeof meta(e).title === 'string'
                                  return false
                                }
                                const items: Item[] = []
                                for (const entry of entries) {
                                  if (isRich(entry)) {
                                    items.push({ kind: 'detail', entry })
                                    continue
                                  }
                                  const last = items[items.length - 1]
                                  if (
                                    last && last.kind === 'run' &&
                                    last.entry.user_id === entry.user_id &&
                                    last.entry.action === entry.action &&
                                    last.entry.resource_type === entry.resource_type &&
                                    typeof (entry.metadata as Record<string, unknown>)?.description !== 'string'
                                  ) {
                                    last.count += 1
                                    last.lastTimestamp = entry.timestamp
                                  } else {
                                    items.push({ kind: 'run', entry, count: 1, lastTimestamp: entry.timestamp })
                                  }
                                }
                                return items.map((item) => {
                                  const entry = item.entry
                                  const displayName =
                                    userMap.get(entry.user_id) ??
                                    (entry.metadata?.user_email as string | undefined) ??
                                    'System'
                                  const variant = timelineActionVariant(entry.action)
                                  const actionLabel = entry.action.split('.').pop() ?? entry.action
                                  const dot = (
                                    <div
                                      className={cn(
                                        'w-[15px] h-[15px] rounded-full shrink-0 mt-0.5 ring-2 ring-background-tertiary relative z-10',
                                        timelineDotClass(entry.action)
                                      )}
                                      aria-hidden="true"
                                    />
                                  )

                                  if (item.kind === 'detail') {
                                    const md = (entry.metadata ?? {}) as Record<string, unknown>
                                    const isSearch = entry.action.toLowerCase().includes('search')
                                    const query = typeof md.query === 'string' ? md.query : null
                                    const resultCount = typeof md.result_count === 'number' ? md.result_count : null
                                    const results = (Array.isArray(md.results) ? md.results : []) as {
                                      id?: string; title?: string; project?: string; type?: string
                                    }[]
                                    const byProject = new Map<string, typeof results>()
                                    for (const r of results) {
                                      const p = r.project || 'unknown'
                                      if (!byProject.has(p)) byProject.set(p, [])
                                      byProject.get(p)!.push(r)
                                    }
                                    const project = typeof md.project === 'string' ? md.project : null
                                    const title = typeof md.title === 'string' ? md.title : null
                                    const memType = typeof md.type === 'string' ? md.type : null
                                    const tags = Array.isArray(md.tags) ? (md.tags as string[]) : []
                                    const preview = typeof md.preview === 'string' ? md.preview : null
                                    const open = expandedActivity.has(entry.id)
                                    return (
                                      <li key={entry.id} className="flex items-start gap-3">
                                        {dot}
                                        <div className="flex-1 min-w-0 max-w-2xl">
                                          <button
                                            type="button"
                                            onClick={() => toggleActivity(entry.id)}
                                            className={cn('w-full flex items-baseline justify-between gap-3 text-left rounded-[8px]', FOCUS_TILE)}
                                          >
                                            <span className="text-[13px] text-text-primary leading-snug flex items-center flex-wrap gap-1 min-w-0">
                                              <ChevronRight className={cn('w-3 h-3 text-text-quaternary transition-transform shrink-0', open && 'rotate-90')} />
                                              {displayName !== 'System' && <span className="font-semibold">{displayName}</span>}
                                              <Badge variant={variant} size="sm">{actionLabel}</Badge>
                                              <span className="text-text-secondary">{entry.resource_type}</span>
                                              {isSearch && query && <span className="text-text-primary truncate">“{query}”</span>}
                                              {isSearch && resultCount != null && (
                                                <span className="text-[12px] text-text-tertiary tabular-nums">· {resultCount} result{resultCount === 1 ? '' : 's'}</span>
                                              )}
                                              {!isSearch && project && (
                                                <span className="text-text-quaternary">in <span className="text-text-secondary">{project}</span></span>
                                              )}
                                              {!isSearch && title && <span className="text-text-primary truncate">— {title}</span>}
                                            </span>
                                            <time dateTime={entry.timestamp} className="shrink-0 text-[12px] text-text-tertiary tabular-nums" title={formatAbsTime(entry.timestamp)}>
                                              {relativeTime(entry.timestamp)}
                                            </time>
                                          </button>
                                          {open && (
                                            <div className="mt-2 ml-1.5 pl-3 border-l border-border-primary space-y-1.5 text-[11px]">
                                              {isSearch ? (
                                                <>
                                                  {query && <p className="text-text-quaternary">query: <span className="text-text-secondary">“{query}”</span></p>}
                                                  {results.length > 0 ? (
                                                    <div className="space-y-1.5">
                                                      <p className="text-text-quaternary">returned:</p>
                                                      {[...byProject.entries()].map(([proj, rs]) => (
                                                        <div key={proj} className="ml-1">
                                                          <p className="text-text-secondary flex items-center gap-1">
                                                            <FolderOpen className="w-3 h-3 shrink-0" />
                                                            <span className="truncate">{proj}</span>
                                                            <span className="text-text-quaternary">({rs.length})</span>
                                                          </p>
                                                          <ul className="ml-4 mt-0.5 space-y-0.5">
                                                            {rs.map((r, i) => (
                                                              <li key={r.id ?? i} className="text-text-tertiary truncate">• {r.title || r.id}</li>
                                                            ))}
                                                          </ul>
                                                        </div>
                                                      ))}
                                                    </div>
                                                  ) : (
                                                    <p className="text-text-tertiary">no results captured for this search</p>
                                                  )}
                                                </>
                                              ) : (
                                                <>
                                                  {project && (
                                                    <p className="text-text-quaternary flex items-center gap-1">
                                                      <FolderOpen className="w-3 h-3 shrink-0" />
                                                      <span className="text-text-secondary truncate">{project}</span>
                                                    </p>
                                                  )}
                                                  {title && <p className="text-text-quaternary">title: <span className="text-text-secondary">{title}</span></p>}
                                                  {memType && <p className="text-text-quaternary">type: <span className="text-text-secondary">{memType}</span></p>}
                                                  {tags.length > 0 && (
                                                    <div className="flex flex-wrap gap-1 items-center">
                                                      <span className="text-text-quaternary">tags:</span>
                                                      {tags.map(t => (
                                                        <span key={t} className="px-1.5 py-0.5 rounded-full bg-white/[0.06] text-text-secondary">{t}</span>
                                                      ))}
                                                    </div>
                                                  )}
                                                  {preview && (
                                                    <p className="text-text-tertiary line-clamp-3 whitespace-pre-wrap">{preview}</p>
                                                  )}
                                                </>
                                              )}
                                            </div>
                                          )}
                                        </div>
                                      </li>
                                    )
                                  }

                                  const { count, lastTimestamp } = item
                                  return (
                                    <li key={entry.id} className="flex items-start gap-3 group">
                                      {dot}
                                      <div className="flex-1 min-w-0 flex items-baseline justify-between gap-3 max-w-2xl">
                                        <p className="text-[13px] text-text-primary leading-snug flex items-center flex-wrap gap-1 min-w-0">
                                          {displayName !== 'System' && <span className="font-semibold">{displayName}</span>}
                                          <Badge variant={variant} size="sm">{actionLabel}</Badge>
                                          {entry.resource_type && <span className="text-text-secondary">{entry.resource_type}</span>}
                                          {count > 1 && (
                                            <span className="text-[12px] font-semibold text-text-tertiary tabular-nums">×{count}</span>
                                          )}
                                          {typeof entry.metadata?.description === 'string' && (
                                            <span className="text-text-tertiary truncate">— {entry.metadata.description}</span>
                                          )}
                                        </p>
                                        <time dateTime={entry.timestamp} className="shrink-0 text-[12px] text-text-tertiary tabular-nums" title={formatAbsTime(entry.timestamp)}>
                                          {count > 1
                                            ? `${relativeTime(lastTimestamp)} – ${relativeTime(entry.timestamp)}`
                                            : relativeTime(entry.timestamp)}
                                        </time>
                                      </div>
                                    </li>
                                  )
                                })
                              })()}
                            </ul>
                          </div>
                        </div>
                      ))}
                    </div>
                  )
                })()}
              </section>
            )}

            {/* Memory Trends sparkline */}
            {isVisible('memory-trends') && (
              <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
                <div className="flex items-center justify-between mb-4">
                  <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Memory Trends</h3>
                  <span className="text-[12px] text-text-tertiary">Last {period} days</span>
                </div>
                {!trends || !trends.daily_counts || trends.daily_counts.length === 0 ? (
                  <p className="text-[13px] text-text-tertiary text-center py-4">No data yet</p>
                ) : (() => {
                  const ptsN = trends.daily_counts.slice(-period)
                  const max = Math.max(...ptsN.map((t: DailyCount) => t.count), 1)
                  const w = 600, h = 130, pad = 4
                  const pts = ptsN.map((t: DailyCount, i: number, arr: DailyCount[]) => {
                    const x = pad + (arr.length > 1 ? (i / (arr.length - 1)) : 0.5) * (w - pad * 2)
                    const y = h - pad - ((t.count / max) * (h - pad * 2))
                    return `${x},${y}`
                  }).join(' ')
                  const firstX = pad
                  const lastX = w - pad
                  return (
                    <svg viewBox={`0 0 ${w} ${h}`} className="w-full h-[130px]">
                      <polyline points={`${firstX},${h - pad} ${pts} ${lastX},${h - pad}`} fill="var(--color-accent-blue)" fillOpacity="0.1" stroke="none" />
                      <polyline points={pts} fill="none" stroke="var(--color-accent-blue)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  )
                })()}
                <div className="flex items-center justify-between mt-2.5">
                  <span className="text-[12.5px] text-text-tertiary">
                    <strong className="text-text-secondary font-semibold">{trends?.this_week?.toLocaleString() ?? 0}</strong> this week
                  </span>
                  <span className="text-[12.5px] text-text-tertiary">
                    <strong className="text-text-secondary font-semibold">
                      {(trends?.daily_counts?.slice(-period).reduce((s: number, t: DailyCount) => s + t.count, 0) ?? 0).toLocaleString()}
                    </strong> last {period}d
                  </span>
                </div>
              </div>
            )}

            {/* Memory Types (design delta 5) */}
            {isVisible('memory-types') && (
              <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
                <div className="flex items-baseline justify-between mb-3">
                  <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Memory Types</h3>
                  <span className="text-[12px] text-text-tertiary">{trends?.total?.toLocaleString() ?? 0} total</span>
                </div>
                {trendsLoading ? (
                  <div className="grid grid-cols-2 gap-2.5">
                    {Array.from({ length: 4 }).map((_, i) => <Skeleton key={i} className="h-[74px] rounded-[11px]" />)}
                  </div>
                ) : (
                  <MemoryTypesCard types={trends?.by_type ?? []} total={trends?.total ?? 0} />
                )}
              </div>
            )}
          </div>

          {/* RIGHT column */}
          <div className="flex flex-col gap-5 min-w-0">

            {/* Agent Activity */}
            {isVisible('agent-activity') && (
              <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
                <div className="flex items-center justify-between mb-3.5">
                  <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Agent Activity</h3>
                  <span className="text-[12px] text-text-tertiary">{period} days</span>
                </div>
                {!isAvailable('agent_activity') && !agentActivityLoading ? <UnavailableState /> : agentActivityLoading ? (
                  <div className="space-y-2">
                    {Array.from({ length: 3 }).map((_, i) => <Skeleton key={i} className="h-[52px] rounded-[12px]" />)}
                  </div>
                ) : !agentActivity || agentActivity.length === 0 ? (
                  <div className="text-[13px] text-text-tertiary text-center py-4">No agent activity yet</div>
                ) : (() => {
                  const maxMemoriesLast7d = Math.max(...(agentActivity as AgentActivity[]).map(a => a.memories_last_7d), 1)
                  return (
                    <div className="flex flex-col gap-1.5">
                      {(agentActivity as AgentActivity[]).map(agent => (
                        <div key={agent.tool} className="flex items-center gap-3 rounded-[12px] border border-border-secondary bg-white/[0.02] px-3 py-2.5 hover:border-white/[0.14] transition-colors">
                          <div className="relative w-8 h-8 rounded-[10px] bg-accent-blue/[0.14] flex items-center justify-center shrink-0">
                            <Code2 className="w-[15px] h-[15px] text-accent-blue" />
                            <span
                              className={cn(
                                'absolute -right-0.5 -bottom-0.5 w-[9px] h-[9px] rounded-full border-2 border-background-tertiary',
                                agent.memories_last_24h > 0 ? 'bg-status-success' : 'bg-white/20'
                              )}
                            />
                          </div>
                          <div className="flex flex-col gap-0.5 flex-1 min-w-0">
                            <span className="text-[13px] font-semibold text-text-primary truncate">{agent.tool}</span>
                            <span className="text-[11px] text-text-tertiary">Last seen {relativeTime(agent.last_seen)}</span>
                          </div>
                          {/* No per-day-per-agent breakdown endpoint exists — the mockup's
                              7-day bar strip per agent isn't backed by real data, so this
                              uses the existing relative-to-max weekly bar instead. */}
                          <div className="w-16 h-1 bg-white/[0.06] rounded-full overflow-hidden shrink-0">
                            <div
                              className="h-full bg-accent-blue/70 rounded-full transition-all duration-500"
                              style={{ width: `${(agent.memories_last_7d / maxMemoriesLast7d) * 100}%` }}
                            />
                          </div>
                          <span className="w-8 text-right text-[13px] font-semibold text-text-primary tabular-nums shrink-0">
                            {agent.memories_last_7d}
                          </span>
                        </div>
                      ))}
                    </div>
                  )
                })()}
              </div>
            )}

            {/* Memory Health (design delta 4) */}
            {isVisible('memory-health') && (
              <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
                <div className="flex items-center justify-between mb-3.5">
                  <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Memory Health</h3>
                  <span className="text-[12px] text-text-tertiary">Last 30 days</span>
                </div>
                {!isAvailable('health') && !dashboardLoading ? <UnavailableState /> : (
                  <MemoryHealthCard
                    total={healthData?.total_memories}
                    duplicates={healthData?.duplicate_count}
                    stale={healthData?.stale_count}
                    untagged={healthData?.untagged_count}
                  />
                )}
              </div>
            )}

            {/* Top Projects (design delta 6) */}
            {isVisible('top-projects') && (
              <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
                <div className="flex items-baseline justify-between mb-3">
                  <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Top Projects</h3>
                  <span className="text-[12px] text-text-tertiary">{trends?.total?.toLocaleString() ?? 0} memories</span>
                </div>
                {trendsLoading ? (
                  <Skeleton className="h-[160px] rounded-[11px]" />
                ) : (
                  <TopProjectsCard projects={trends?.by_project ?? []} />
                )}
              </div>
            )}

            {/* Top Contributors (design delta 7) */}
            {isVisible('contributors') && (
              <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
                <div className="flex items-center justify-between mb-3.5">
                  <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Top Contributors</h3>
                  <span className="text-[12px] text-text-tertiary">Last {period} days</span>
                </div>
                {!isAvailable('contributors') && !contributorsLoading ? <UnavailableState /> : contributorsLoading ? (
                  <div className="space-y-3">
                    {Array.from({ length: 3 }).map((_, i) => (
                      <div key={i} className="animate-pulse bg-white/[0.04] rounded-[8px] h-5" />
                    ))}
                  </div>
                ) : contributors && contributors.length > 0 ? (
                  <div className="space-y-3">
                    {(contributors as ContributorStat[]).map((c, i) => {
                      const max = contributors[0].memory_count || 1
                      const displayName = c.user_name || c.user_email || c.user_id
                      return (
                        <div key={c.user_id} className="flex items-center gap-3">
                          <span className="text-[12px] text-text-tertiary w-4 text-right shrink-0">{i + 1}</span>
                          <span className="text-[13.5px] text-text-primary truncate flex-1 min-w-0">{displayName}</span>
                          <div className="w-24 h-1 bg-white/[0.06] rounded-full overflow-hidden shrink-0">
                            <div
                              className="h-full bg-accent-blue rounded-full"
                              style={{ width: `${(c.memory_count / max) * 100}%` }}
                            />
                          </div>
                          <span className="text-[12px] text-text-tertiary w-8 text-right shrink-0 tabular-nums">{c.memory_count}</span>
                        </div>
                      )
                    })}
                  </div>
                ) : (
                  <p className="text-[13px] text-text-tertiary">No activity in the last {period} days.</p>
                )}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Extra widgets not part of the mockup's primary layout — still real,
          still togglable via Customize, kept below the main two-column grid. */}
      {hasAdminAccess && (isVisible('usage') || isVisible('heatmap') || isVisible('conventions')) && (
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {isVisible('usage') && (
            <div className={`rounded-[18px] p-5 space-y-3 ${GLASS_PANEL}`}>
              <p className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Usage</p>
              {usageLoading ? (
                Array.from({ length: 5 }).map((_, i) => (
                  <div key={i} className="flex items-center justify-between animate-pulse">
                    <div className="h-3 w-24 rounded-[8px] bg-white/[0.04]" />
                    <div className="h-3 w-10 rounded-[8px] bg-white/[0.04]" />
                  </div>
                ))
              ) : !isAvailable('usage') ? <UnavailableState /> : usageStats ? (
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
                      <span className="text-[13px] text-text-secondary flex-1">{label}</span>
                      <span className="text-[13px] font-semibold text-text-primary tabular-nums">
                        {value.toLocaleString()}
                      </span>
                    </div>
                  ))}
                </>
              ) : (
                <div className="text-[13px] text-text-tertiary text-center py-4">No data yet</div>
              )}
            </div>
          )}

          {isVisible('heatmap') && (
            <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Memory Activity</h3>
                <span className="text-[12px] text-text-tertiary">Last {period} days</span>
              </div>
              {!isAvailable('heatmap') && !dashboardLoading ? <UnavailableState /> : heatmapData ? (
                <MemoryHeatmap data={heatmapData} />
              ) : (
                <div className="h-[78px] bg-white/[0.04] animate-pulse rounded-[8px]" />
              )}
              {isAvailable('heatmap') && <div className="flex items-center gap-1 mt-3">
                <span className="text-[12px] text-text-tertiary">Less</span>
                {(['bg-white/[0.04]', 'bg-accent-blue/20', 'bg-accent-blue/40', 'bg-accent-blue/60', 'bg-accent-blue'] as const).map((c, i) => (
                  <div key={i} className={`w-[10px] h-[10px] rounded-[2px] ${c}`} />
                ))}
                <span className="text-[12px] text-text-tertiary">More</span>
              </div>}
            </div>
          )}

          {isVisible('conventions') && (
            <div className={`rounded-[18px] p-5 ${GLASS_PANEL}`}>
              <div className="flex items-center justify-between mb-4">
                <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Conventions</h3>
                <a href="/conventions" className={cn('text-[12px] text-accent-blue hover:text-accent-blue/80 transition-colors rounded-[8px]', FOCUS_TILE)}>
                  View all →
                </a>
              </div>
              {!isAvailable('conventions') && !dashboardLoading ? <UnavailableState /> : conventionStats.map(cat => (
                <div key={cat.category} className="flex items-center justify-between py-1.5 border-b border-border-secondary/20 last:border-0">
                  <span className="text-[13px] text-text-secondary capitalize">{cat.category}</span>
                  <span className="text-[12px] text-text-tertiary">{cat.count}</span>
                </div>
              ))}
              {isAvailable('conventions') && conventionStats.length === 0 && (
                <p className="text-[13px] text-text-tertiary">No conventions yet. <a href="/conventions" className={cn('text-accent-blue rounded-[8px]', FOCUS_TILE)}>Add one →</a></p>
              )}
            </div>
          )}
        </div>
      )}

      {!hasAdminAccess && (
        <div className="space-y-4 max-w-2xl">
          <div className={`rounded-[18px] p-6 ${GLASS_PANEL}`}>
            <p className="text-[15px] font-semibold text-text-primary mb-1">
              Welcome, {session?.user.name}
            </p>
            <p className="text-[13px] text-text-tertiary">
              {session?.org.name} · <span className="capitalize">{session?.user.role}</span>
            </p>
          </div>
          <div className="grid grid-cols-2 gap-3">
            {[
              { label: 'Memories', href: '/memories', description: 'Browse and search your team memories' },
              { label: 'Search', href: '/search', description: 'Semantic search across all memories' },
              { label: 'Projects', href: '/projects', description: 'View your assigned projects' },
              { label: 'Sessions', href: '/sessions', description: 'Browse agent sessions' },
            ].map(item => (
              <Link
                key={item.label}
                to={item.href}
                className="border border-border-primary rounded-[18px] p-4 hover:bg-white/[0.04] transition-colors group"
              >
                <p className="text-[13px] font-semibold text-text-primary group-hover:text-accent-blue transition-colors">{item.label}</p>
                <p className="text-[11px] text-text-quaternary mt-0.5">{item.description}</p>
              </Link>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
