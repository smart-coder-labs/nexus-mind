import { useMemo, useState, useEffect, useRef, lazy, Suspense } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { Skeleton } from '@/components/ui/Skeleton/Skeleton'
import { EmptyState } from '@/components/ui/EmptyState/EmptyState'
import { Badge } from '@/components/ui/Badge/Badge'
import { cn } from '@/lib/utils'
import { Sparkles, X, Check, CheckCircle, CheckCircle2, Circle, Brain, Clock, Users, FolderOpen, Code2, UserPlus, FolderPlus, Download, FileText, Zap, LayoutGrid, User, Key, BookMarked, Webhook, Activity, Copy, Tag, ChevronRight, Share2, List } from 'lucide-react'
import type { NameCount, DailyCount, AgentActivity, HeatmapDay, ContributorStat } from '../types'
import { DISABLED_NAV_HREFS } from '../config/disabled-sections'
import { usePersistedGraphState } from '../hooks/usePersistedGraphState'

const OrgMemoryGraph = lazy(() => import('../components/OrgMemoryGraph'))

type CardKey = 'onboarding' | 'trends' | 'heatmap' | 'contributors' | 'agent-activity' | 'usage' | 'quick-actions' | 'conventions' | 'recent-activity' | 'getting-started' | 'memory-trends' | 'memory-health'
const ALL_CARDS: CardKey[] = ['onboarding', 'trends', 'heatmap', 'contributors', 'conventions', 'getting-started', 'recent-activity', 'memory-trends', 'memory-health', 'agent-activity', 'usage', 'quick-actions']
const CARDS_STORAGE_KEY = 'nexusmind-dashboard-cards'
const DASHBOARD_VIEW_KEY = 'nexusmind-dashboard-view'

function downloadBlob(blob: Blob, filename = 'download.json') {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

const ONBOARDING_DISMISSED_KEY = 'onboardingDismissed'

// Keyboard focus indicator (design direction §6): 2px --color-focus-ring outline
// with a 2px offset. Uses outline (not ring) so it isn't clipped by overflow-hidden
// ancestors. Both aliases are identical now; kept for call-site readability.
const FOCUS_CANVAS = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'
const FOCUS_TILE = 'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

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
  'var(--color-accent-purple)',
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

function activityIcon(action: string) {
  if (action.startsWith('memory.')) return Brain
  if (action.startsWith('user.')) return User
  if (action.startsWith('api_key.')) return Key
  if (action.startsWith('convention.')) return BookMarked
  if (action.startsWith('webhook.')) return Webhook
  return Activity
}

// --- Stat pill badge accent mapping ---
// Design direction §5: stat tiles use one neutral treatment only.
// Color on a metric encodes state (e.g. status-error for an error count), never identity.
const BADGE_ACCENT: Record<string, string> = {}
const STAT_TILE_BASE = 'bg-white/[0.04] border-border-primary'

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

  const client = useMemo(
    () => createClient(),
    [session],
  )

  const isAdmin = session?.user.role === 'admin'

  const [period, setPeriod] = useState<7 | 30 | 90>(30)

  // Persist the list/graph view toggle across reloads via the shared hook.
  // A `validate` predicate ensures legacy non-JSON values (from before the
  // hook was introduced) fall back to the default 'list' view.
  const [dashboardView, setDashboardView] = usePersistedGraphState<'list' | 'graph'>(
    DASHBOARD_VIEW_KEY,
    'list',
    {
      validate: v => v === 'list' || v === 'graph',
    },
  )

  const handleDashboardViewChange = (view: 'list' | 'graph') => {
    setDashboardView(view)
  }

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

  const { data: stats, isLoading: statsLoading, isError: statsError } = useQuery({
    queryKey: ['stats'],
    queryFn: () => client.getStats(),
    refetchInterval: 30_000,
    enabled: isAdmin,
  })

  const [activityLimit, setActivityLimit] = useState(20)
  const { data: activity, isLoading: activityLoading, isFetching: activityFetching } = useQuery({
    queryKey: ['audit', 'recent', activityLimit],
    queryFn: () => client.getAuditLog({ limit: activityLimit }),
    refetchInterval: 30_000,
    enabled: isAdmin,
    placeholderData: (prev) => prev,
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

  const { data: recentActivity } = useQuery({
    queryKey: ['recent-activity'],
    queryFn: () => client.getAuditLog({ limit: 10 }),
    refetchInterval: 30_000,
    enabled: isAdmin,
  })

  const { data: conventions } = useQuery({
    queryKey: ['conventions'],
    queryFn: () => client.listConventions(),
    staleTime: 60_000,
    enabled: isAdmin,
  })

  const { data: projects } = useQuery({
    queryKey: ['projects-check'],
    queryFn: () => client.listProjects(),
    staleTime: 5 * 60_000,
    enabled: isAdmin,
  })

  const { data: apiKeys } = useQuery({
    queryKey: ['api-keys-check'],
    queryFn: () => client.listOrgKeys(),
    staleTime: 5 * 60_000,
    // Only feeds the "Create an API key" checklist item — skip while that section is disabled.
    enabled: isAdmin && !DISABLED_NAV_HREFS.has('/api-keys'),
  })

  const { data: healthData } = useQuery({
    queryKey: ['memory-health'],
    queryFn: () => client.getMemoryHealth(),
    staleTime: 5 * 60_000,
    enabled: isAdmin,
    retry: false,
  })

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
    const sessionMetric = usageStats ? [
      {
        id: 'total-sessions',
        label: 'Sessions',
        value: usageStats.sessions.toLocaleString(),
      },
    ] : []
    return [...base, ...sessionMetric, ...extra]
  }, [stats, trends, usageStats])

  return (
    <div className="p-6 space-y-6 max-w-7xl mx-auto">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-[22px] font-semibold tracking-[-0.3px] leading-[1.2] text-text-primary">Dashboard</h1>
          <p className="text-[13px] text-text-secondary mt-1">
            {session?.org.name} — organization overview
          </p>
        </div>
        {isAdmin && (
          <div className="flex items-center gap-2">
            {/* Overview / Graph toggle */}
            <div className="flex items-center gap-0.5 bg-white/[0.04] rounded-full p-0.5">
              <button
                type="button"
                onClick={() => handleDashboardViewChange('list')}
                className={cn(
                  'flex items-center gap-1 px-2.5 py-1 rounded-full text-[11px] transition-colors',
                  FOCUS_CANVAS,
                  dashboardView === 'list'
                    ? 'bg-white/[0.08] text-text-primary'
                    : 'text-text-quaternary hover:text-text-secondary'
                )}
                aria-label="Overview"
              >
                <List className="w-3 h-3" /> Overview
              </button>
              <button
                type="button"
                onClick={() => handleDashboardViewChange('graph')}
                className={cn(
                  'flex items-center gap-1 px-2.5 py-1 rounded-full text-[11px] transition-colors',
                  FOCUS_CANVAS,
                  dashboardView === 'graph'
                    ? 'bg-white/[0.08] text-text-primary'
                    : 'text-text-quaternary hover:text-text-secondary'
                )}
                aria-label="Graph"
              >
                <Share2 className="w-3 h-3" /> Graph
              </button>
            </div>

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
                <div className="absolute right-0 top-full mt-2 bg-background-tertiary border border-border-primary rounded-[18px] py-2 min-w-[200px] z-20">
                  {ALL_CARDS.map(key => (
                    <label key={key} className="flex items-center justify-between px-3 py-2 hover:bg-white/[0.04] cursor-pointer">
                      <span className="text-[13px] text-text-secondary capitalize">{key.replace(/-/g, ' ')}</span>
                      <button
                        onClick={() => toggleCard(key)}
                        className={cn(
                          'w-8 h-4 rounded-full transition-colors relative shrink-0',
                          FOCUS_TILE,
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

      {/* Graph view — org-wide memory graph (all projects merged via real API) */}
      {dashboardView === 'graph' && isAdmin && (
        <Suspense fallback={
          <div className="border border-border-primary rounded-[18px] flex items-center justify-center py-20">
            <div className="w-5 h-5 animate-spin rounded-full border-2 border-text-quaternary border-t-transparent" />
          </div>
        }>
          <OrgMemoryGraph storageKey="dashboard" height={500} />
        </Suspense>
      )}

      {/* Onboarding checklist */}
      {isVisible('onboarding') && showOnboarding && (
        <section aria-label="Getting started checklist">
          <div className="border border-accent-blue/20 rounded-[18px] p-5 bg-accent-blue/[0.04] space-y-4">
            {/* Header */}
            <div className="flex items-center justify-between">
              <div className="flex items-center">
                <Sparkles className="w-4 h-4 text-accent-blue" />
                <span className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary ml-2">Getting started</span>
              </div>
              <button
                onClick={handleDismiss}
                aria-label="Dismiss"
                className={cn('text-text-tertiary hover:text-text-secondary transition-colors rounded-full', FOCUS_CANVAS)}
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {allDone ? (
              <div className="text-center py-2 space-y-1">
                <CheckCircle className="w-6 h-6 text-status-success mx-auto" />
                <p className="text-[13px] font-semibold text-text-primary">You're all set!</p>
              </div>
            ) : (
              <>
                {/* Progress bar */}
                <div className="w-full">
                  <div className="h-1 bg-background-tertiary rounded-full w-full">
                    <div
                      className="h-1 bg-accent-blue rounded-full transition-all duration-500"
                      style={{ width: `${totalCount > 0 ? (doneCount / totalCount) * 100 : 0}%` }}
                    />
                  </div>
                  <span className="text-[12px] text-text-tertiary">{doneCount} of {totalCount} complete</span>
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
                        <p className={`text-[13px] leading-tight ${item.done ? 'line-through text-text-quaternary opacity-50' : 'text-accent-blue'}`}>
                          {item.label}
                        </p>
                        <p className="text-[12px] text-text-tertiary mt-0.5">
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

      {/* Overview content — hidden in graph mode */}
      {dashboardView === 'list' && <>

      {/* Stat cards */}
      {isAdmin && (
        <section aria-label="Organization statistics">
          {statsError ? (
            <div className="rounded-[18px] border border-status-error/30 bg-status-error/10 p-4 text-[13px] text-status-error">
              Failed to load statistics. Check your connection and try again.
            </div>
          ) : statsLoading || trendsLoading || usageLoading ? (
            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4">
              {Array.from({ length: 7 }).map((_, i) => (
                <Skeleton key={i} className="h-[92px] rounded-[18px]" />
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4" role="list" aria-label="Key statistics">
              {metrics.map((metric) => (
                <div
                  key={metric.id}
                  role="listitem"
                  className={cn(
                    'flex flex-col gap-1 rounded-[18px] border p-5 transition-colors',
                    BADGE_ACCENT[metric.id ?? ''] ?? STAT_TILE_BASE
                  )}
                >
                  <span className="text-[28px] font-semibold leading-none text-text-primary tabular-nums truncate">{metric.value}</span>
                  <span className="text-[12px] text-text-tertiary">{metric.label}</span>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      {/* Activity timeline */}
      {isAdmin && (
        <section aria-label="Recent activity">
          <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-4">
            Recent Activity
          </h2>
          <div className="bg-background-tertiary border border-white/[0.06] rounded-[18px] p-5">
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
                  {activity.length >= activityLimit && (
                    <div className="pt-1 pl-5">
                      <button
                        type="button"
                        onClick={() => setActivityLimit(l => l + 20)}
                        disabled={activityFetching}
                        className={cn('text-[13px] text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50 rounded-[8px] px-1', FOCUS_TILE)}
                      >
                        {activityFetching ? 'Loading…' : 'Show more'}
                      </button>
                    </div>
                  )}
                </div>
              )
            })()}
          </div>
        </section>
      )}

      {/* Usage stats + Agent Activity */}
      {isAdmin && (isVisible('usage') || isVisible('agent-activity')) && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {/* Usage */}
          {isVisible('usage') && <div className="border border-border-primary rounded-[18px] p-5 space-y-3">
            <p className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Usage</p>
            {usageLoading ? (
              Array.from({ length: 5 }).map((_, i) => (
                <div key={i} className="flex items-center justify-between animate-pulse">
                  <div className="h-3 w-24 rounded-[8px] bg-background-tertiary" />
                  <div className="h-3 w-10 rounded-[8px] bg-background-tertiary" />
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
          </div>}

          {/* Agent Activity */}
          {isVisible('agent-activity') && <div className="rounded-[18px] bg-background-tertiary border border-border-primary p-5 space-y-4">
            <p className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Agent Activity</p>
            {agentActivityLoading ? (
              Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="space-y-1.5 animate-pulse">
                  <div className="h-3 w-32 rounded bg-background-secondary" />
                  <div className="h-1 w-full rounded-full bg-background-secondary" />
                  <div className="h-2 w-20 rounded bg-background-secondary" />
                </div>
              ))
            ) : !agentActivity || agentActivity.length === 0 ? (
              <div className="text-[13px] text-text-tertiary text-center py-4">No agent activity yet</div>
            ) : (() => {
              const maxMemoriesLast7d = Math.max(...(agentActivity as AgentActivity[]).map(a => a.memories_last_7d), 1)
              return (agentActivity as AgentActivity[]).map(agent => (
                <div key={agent.tool} className="space-y-1.5">
                  <div className="flex items-center justify-between">
                    <span className="text-[13px] font-semibold text-text-primary truncate">{agent.tool}</span>
                    <div className="flex items-center gap-2 shrink-0">
                      {agent.memories_last_24h > 0 && (
                        <span className="w-1.5 h-1.5 rounded-full bg-status-success" />
                      )}
                      <span className="text-[12px] text-text-tertiary">{agent.memories_last_7d} this week</span>
                    </div>
                  </div>
                  <div className="h-1 bg-background-secondary rounded-full">
                    <div
                      className="h-1 bg-accent-blue/60 rounded-full transition-all duration-500"
                      style={{ width: `${(agent.memories_last_7d / maxMemoriesLast7d) * 100}%` }}
                    />
                  </div>
                  <p className="text-[12px] text-text-tertiary">Last seen {relativeTime(agent.last_seen)}</p>
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
              <div className="h-4 bg-background-tertiary rounded w-1/3 mb-4" />
              <div className="h-12 bg-background-tertiary rounded" />
            </div>
          ) : trends ? (
            <div className="border border-border-primary rounded-[18px] p-5">
              <p className="text-[12px] text-text-tertiary mb-3">Last {period} Days</p>
              {trends.daily_counts.length === 0 ? (
                <div className="text-[13px] text-text-tertiary text-center py-4">No data yet</div>
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
                      <span className="text-[12px] text-text-tertiary">{first}</span>
                      <span className="text-[12px] text-text-tertiary">{mid}</span>
                      <span className="text-[12px] text-text-tertiary">{last}</span>
                    </div>
                  </>
                )
              })()}
            </div>
          ) : null}

          {/* Top Projects */}
          {trendsLoading ? (
            <div className="border border-border-primary rounded-[18px] p-5 animate-pulse">
              <div className="h-4 bg-background-tertiary rounded w-1/3 mb-4" />
              <div className="h-12 bg-background-tertiary rounded" />
            </div>
          ) : trends ? (
            <div className="border border-border-primary rounded-[18px] p-5 space-y-3">
              <p className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-3">Top Projects</p>
              {trends.by_project.length === 0 ? (
                <div className="text-[13px] text-text-tertiary text-center py-4">No data yet</div>
              ) : (() => {
                const maxCount = Math.max(...trends.by_project.map((p: NameCount) => p.count), 1)
                return (
                  <>
                    {trends.by_project.map((p: NameCount) => (
                      <div key={p.name} className="space-y-1">
                        <div className="flex items-center justify-between">
                          <span className="text-[13px] text-text-secondary truncate max-w-[60%]">{p.name}</span>
                          <span className="text-[13px] text-text-tertiary">{p.count.toLocaleString()}</span>
                        </div>
                        <div className="h-1 bg-background-tertiary rounded-full">
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
              <div className="h-4 bg-background-tertiary rounded w-1/3 mb-4" />
              <div className="h-12 bg-background-tertiary rounded" />
            </div>
          ) : trends ? (
            <div className="border border-border-primary rounded-[18px] p-5 space-y-3">
              <p className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-3">Memory Types</p>
              {trends.by_type.length === 0 ? (
                <div className="text-[13px] text-text-tertiary text-center py-4">No data yet</div>
              ) : (() => {
                const maxTypeCount = Math.max(...trends.by_type.map((t: NameCount) => t.count), 1)
                return (
                  <>
                    {trends.by_type.map((t: NameCount, i: number) => (
                      <div key={t.name} className="space-y-1">
                        <div className="flex justify-between items-center">
                          <span className="text-[13px] text-text-secondary truncate max-w-[60%]">{t.name || 'unset'}</span>
                          <span className="text-[13px] font-semibold text-text-primary">{t.count}</span>
                        </div>
                        <div className="h-1 bg-background-secondary rounded-full">
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
        <div className="bg-background-tertiary rounded-[18px] p-5 border border-border-primary">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Memory Activity</h3>
            <span className="text-[12px] text-text-tertiary">Last {period} days</span>
          </div>
          {heatmapData ? (
            <MemoryHeatmap data={heatmapData} />
          ) : (
            <div className="h-[78px] bg-white/[0.04] animate-pulse rounded-[8px]" />
          )}
          <div className="flex items-center gap-1 mt-3">
            <span className="text-[12px] text-text-tertiary">Less</span>
            {(['bg-white/[0.04]', 'bg-accent-blue/20', 'bg-accent-blue/40', 'bg-accent-blue/60', 'bg-accent-blue'] as const).map((c, i) => (
              <div key={i} className={`w-[10px] h-[10px] rounded-[2px] ${c}`} />
            ))}
            <span className="text-[12px] text-text-tertiary">More</span>
          </div>
        </div>
      )}

      {/* Top Contributors */}
      {isAdmin && isVisible('contributors') && (
        <div className="bg-background-tertiary rounded-[18px] p-5 border border-border-primary">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Top Contributors</h3>
            <span className="text-[12px] text-text-tertiary">Last {period} days</span>
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
                const displayName = c.user_name || c.user_email || c.user_id
                return (
                  <div key={c.user_id} className="flex items-center gap-3">
                    <span className="text-[12px] text-text-tertiary w-3 text-right">{i + 1}</span>
                    <span className="text-[13px] text-text-secondary truncate flex-1 font-mono">{displayName}</span>
                    <div className="w-24 h-1 bg-white/[0.06] rounded-full overflow-hidden">
                      <div
                        className="h-full bg-accent-blue rounded-full"
                        style={{ width: `${(c.memory_count / max) * 100}%` }}
                      />
                    </div>
                    <span className="text-[12px] text-text-tertiary w-6 text-right">{c.memory_count}</span>
                  </div>
                )
              })}
            </div>
          ) : (
            <p className="text-[13px] text-text-tertiary">No activity in the last 30 days.</p>
          )}
        </div>
      )}

      {/* Conventions */}
      {isAdmin && isVisible('conventions') && (
        <div className="bg-background-tertiary rounded-[18px] border border-border-primary p-5">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Conventions</h3>
            <a href="/conventions" className={cn('text-[12px] text-accent-blue hover:text-accent-blue/80 transition-colors rounded-[8px]', FOCUS_TILE)}>
              View all →
            </a>
          </div>
          {conventionStats.map(cat => (
            <div key={cat.category} className="flex items-center justify-between py-1.5 border-b border-border-secondary/20 last:border-0">
              <span className="text-[13px] text-text-secondary capitalize">{cat.category}</span>
              <span className="text-[12px] text-text-tertiary">{cat.count}</span>
            </div>
          ))}
          {conventionStats.length === 0 && (
            <p className="text-[13px] text-text-tertiary">No conventions yet. <a href="/conventions" className={cn('text-accent-blue rounded-[8px]', FOCUS_TILE)}>Add one →</a></p>
          )}
        </div>
      )}

      {/* Getting Started */}
      {isAdmin && isVisible('getting-started') && (() => {
        const allChecklistItems = [
          {
            label: 'Create your first memory',
            done: (stats?.total_memories ?? 0) > 0,
            href: '/memories',
          },
          {
            label: 'Invite a team member',
            done: (users?.length ?? 0) > 1,
            href: '/users',
          },
          {
            label: 'Create a project',
            done: Array.isArray(projects) ? projects.length > 0 : false,
            href: '/projects',
          },
          {
            label: 'Set team conventions',
            done: Array.isArray(conventions) ? conventions.length > 0 : false,
            href: '/conventions',
          },
          {
            label: 'Create an API key',
            done: Array.isArray(apiKeys) ? apiKeys.length > 0 : false,
            href: '/api-keys',
          },
        ]
        // Hide items whose destination section is currently disabled — reversible:
        // remove the href from DISABLED_NAV_HREFS in src/config/disabled-sections.ts to restore.
        const checklistItems = allChecklistItems.filter(i => !DISABLED_NAV_HREFS.has(i.href))
        const completedCount = checklistItems.filter(i => i.done).length
        const totalCount = checklistItems.length
        return (
          <div className="bg-background-tertiary rounded-[18px] border border-border-primary p-5">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Getting Started</h3>
              <span className="text-[12px] text-text-tertiary">{completedCount}/{totalCount} completed</span>
            </div>
            {completedCount === totalCount ? (
              <div className="flex flex-col items-center py-4 gap-2">
                <CheckCircle2 className="w-8 h-8 text-status-success" />
                <p className="text-[13px] text-text-secondary">All set up! Your team is ready.</p>
              </div>
            ) : (
              checklistItems.map(item => (
                <div key={item.label} className={`flex items-center gap-2.5 py-2 border-b border-border-secondary/20 last:border-0 ${item.done ? 'opacity-50' : ''}`}>
                  {item.done
                    ? <CheckCircle2 className="w-4 h-4 text-status-success shrink-0" />
                    : <Circle className="w-4 h-4 text-text-quaternary shrink-0" />
                  }
                  {item.done
                    ? <span className="text-[13px] text-text-quaternary line-through">{item.label}</span>
                    : <Link to={item.href} className={cn('text-[13px] text-accent-blue hover:text-accent-blue/80 transition-colors rounded-[8px]', FOCUS_TILE)}>{item.label}</Link>
                  }
                </div>
              ))
            )}
          </div>
        )
      })()}

      {/* Recent Activity feed */}
      {isAdmin && isVisible('recent-activity') && (
        <div className="bg-background-tertiary rounded-[18px] border border-border-primary p-5">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Recent Activity</h3>
            <span className="text-[12px] text-text-tertiary">Live · 30s</span>
          </div>
          {recentActivity && recentActivity.length > 0 ? (
            recentActivity.map(entry => {
              const Icon = activityIcon(entry.action)
              return (
                <div key={entry.id} className="flex items-start gap-3 py-2 border-b border-border-secondary/20 last:border-0">
                  <div className="w-6 h-6 rounded-full bg-white/[0.06] flex items-center justify-center flex-shrink-0 mt-0.5">
                    <Icon className="w-4 h-4 text-text-quaternary" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <p className="text-[13px] text-text-secondary line-clamp-1">
                      {(entry.metadata?.description as string) || entry.action}
                    </p>
                    <p className="text-[12px] text-text-tertiary mt-0.5">
                      {relativeTime(entry.timestamp)} · {(entry.metadata?.user_email as string) || userMap.get(entry.user_id) || 'System'}
                    </p>
                  </div>
                </div>
              )
            })
          ) : (
            <p className="text-[13px] text-text-tertiary text-center py-4">No recent activity</p>
          )}
        </div>
      )}

      {/* Memory Trends sparkline */}
      {isAdmin && isVisible('memory-trends') && (
        <div className="bg-background-tertiary rounded-[18px] border border-border-primary p-5">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Memory Trends</h3>
            <span className="text-[12px] text-text-tertiary">Last {period} days</span>
          </div>
          {!trends || !trends.daily_counts || trends.daily_counts.length === 0 ? (
            <p className="text-[13px] text-text-tertiary text-center py-4">No data yet</p>
          ) : (() => {
            const ptsN = trends.daily_counts.slice(-period)
            const max = Math.max(...ptsN.map((t: DailyCount) => t.count), 1)
            const w = 300, h = 60, pad = 4
            const pts = ptsN.map((t: DailyCount, i: number, arr: DailyCount[]) => {
              const x = pad + (arr.length > 1 ? (i / (arr.length - 1)) : 0.5) * (w - pad * 2)
              const y = h - pad - ((t.count / max) * (h - pad * 2))
              return `${x},${y}`
            }).join(' ')
            const firstX = pad
            const lastX = w - pad
            return (
              <svg viewBox={`0 0 ${w} ${h}`} className="w-full h-14">
                <polyline points={`${firstX},${h - pad} ${pts} ${lastX},${h - pad}`} fill="var(--color-accent-blue)" fillOpacity="0.08" stroke="none" />
                <polyline points={pts} fill="none" stroke="var(--color-accent-blue)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            )
          })()}
          <div className="flex items-center justify-between mt-2">
            <span className="text-[12px] text-text-tertiary">
              {trends?.daily_counts?.slice(-7).reduce((s: number, t: DailyCount) => s + t.count, 0) ?? 0} this week
            </span>
            <span className="text-[12px] text-text-tertiary">
              {trends?.daily_counts?.slice(-period).reduce((s: number, t: DailyCount) => s + t.count, 0) ?? 0} last {period}d
            </span>
          </div>
        </div>
      )}

      {/* Memory Health */}
      {isAdmin && isVisible('memory-health') && (
        <div className="rounded-[18px] border border-border-primary bg-background-tertiary p-5">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Memory Health</h3>
            <span className="text-[12px] text-text-tertiary">Last 30 days</span>
          </div>
          <div className="grid grid-cols-2 gap-3">
            {[
              { label: 'Total', value: healthData?.total_memories ?? '—', icon: Brain, ok: true },
              { label: 'Duplicates', value: healthData?.duplicate_count ?? '—', icon: Copy, ok: (healthData?.duplicate_count ?? 0) === 0 },
              { label: 'Stale (>30d)', value: healthData?.stale_count ?? '—', icon: Clock, ok: (healthData?.stale_count ?? 0) < 10 },
              { label: 'Untagged', value: healthData?.untagged_count ?? '—', icon: Tag, ok: (healthData?.untagged_count ?? 0) < 5 },
            ].map(({ label, value, icon: Icon, ok }) => (
              <div key={label} className="rounded-[11px] bg-white/[0.04] p-3">
                <div className="flex items-center gap-1.5 mb-1">
                  <Icon className="w-3 h-3 text-text-quaternary" />
                  <span className="text-[12px] text-text-tertiary">{label}</span>
                </div>
                <span className={`text-[28px] font-semibold leading-none tabular-nums ${ok ? 'text-text-primary' : 'text-status-warning'}`}>
                  {value}
                </span>
              </div>
            ))}
          </div>
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
          <div className="bg-background-tertiary rounded-[18px] p-5 border border-border-primary">
            <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-4">Quick Actions</h3>
            <div className="grid grid-cols-2 gap-2">
              {QUICK_ACTIONS.map(action => (
                'href' in action ? (
                  <Link
                    key={action.label}
                    to={action.href}
                    className={cn('flex items-center gap-2 px-3 py-2 rounded-[8px] bg-white/[0.03] hover:bg-white/[0.06] text-[13px] font-semibold text-text-primary transition-colors border border-border-secondary/30', FOCUS_TILE)}
                  >
                    <action.icon className="w-4 h-4" />
                    {action.label}
                  </Link>
                ) : (
                  <button
                    key={action.label}
                    onClick={action.action}
                    className={cn('flex items-center gap-2 px-3 py-2 rounded-[8px] bg-white/[0.03] hover:bg-white/[0.06] text-[13px] font-semibold text-text-primary transition-colors border border-border-secondary/30', FOCUS_TILE)}
                  >
                    <action.icon className="w-4 h-4" />
                    {action.label}
                  </button>
                )
              ))}
            </div>
          </div>
        )
      })()}

      </>}

      {!isAdmin && (
        <div className="border border-white/[0.08] bg-background-tertiary rounded-[18px] p-6 max-w-xl">
          <p className="text-[13px] text-text-secondary leading-relaxed">
            Welcome to <strong>{session?.org.name}</strong> on NexusMind.
          </p>
          <p className="text-[13px] text-text-tertiary mt-2">
            Use the navigation sidebar to browse, search, and manage your team's shared AI memories.
          </p>
        </div>
      )}
    </div>
  )
}
