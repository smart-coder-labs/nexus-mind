import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { ActivityItem } from '../components/ActivityItem'

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="py-6 px-1">
      <p className="text-3xl font-semibold tracking-tight text-white tabular-nums">{value}</p>
      <p className="text-[11px] text-white/30 uppercase tracking-wide mt-2">{label}</p>
    </div>
  )
}

function StatSkeleton() {
  return (
    <div className="py-6 px-1 space-y-2">
      <div className="h-8 w-20 rounded bg-white/5 animate-pulse" />
      <div className="h-3 w-28 rounded bg-white/4 animate-pulse" />
    </div>
  )
}

export default function Dashboard() {
  const { session } = useAuth()

  const client = useMemo(
    () => createClient(session!.apiKey),
    [session],
  )

  const { data: stats, isLoading: statsLoading, isError: statsError } = useQuery({
    queryKey: ['stats'],
    queryFn: () => client.getStats(),
    refetchInterval: 30_000,
  })

  const { data: activity, isLoading: activityLoading } = useQuery({
    queryKey: ['audit', 'recent'],
    queryFn: () => client.getAuditLog({ limit: 20 }),
    refetchInterval: 30_000,
  })

  const { data: users } = useQuery({
    queryKey: ['users'],
    queryFn: () => client.listUsers(),
    staleTime: 60_000,
  })

  const userMap = useMemo(() => {
    const map = new Map<string, string>()
    users?.forEach(u => map.set(u.id, u.name))
    return map
  }, [users])

  const metrics = useMemo(() => {
    if (!stats) return []
    return [
      { label: 'Total Memories',    value: stats.total_memories.toLocaleString() },
      { label: 'Active Users (24h)', value: stats.active_users_24h.toLocaleString() },
      { label: 'Searches Today',    value: stats.searches_today.toLocaleString() },
      { label: 'Top Tool',          value: stats.top_tools[0]?.tool ?? '—' },
    ]
  }, [stats])

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-12">
      {/* Header */}
      <div>
        <h1 className="text-lg font-semibold text-white">{session?.org.name}</h1>
        <p className="text-[12px] text-white/30 mt-0.5">Organization overview</p>
      </div>

      {/* Stats */}
      <section>
        {statsError ? (
          <p className="text-[12px] text-red-400/70">Failed to load statistics.</p>
        ) : (
          <div className="grid grid-cols-2 xl:grid-cols-4 divide-x divide-white/5">
            {statsLoading
              ? Array.from({ length: 4 }).map((_, i) => <StatSkeleton key={i} />)
              : metrics.map(m => <StatCard key={m.label} label={m.label} value={m.value} />)
            }
          </div>
        )}
      </section>

      {/* Divider */}
      <div className="border-t border-white/5" />

      {/* Activity */}
      <section>
        <p className="text-[11px] text-white/30 uppercase tracking-wide mb-6">Recent Activity</p>

        {activityLoading ? (
          <div className="space-y-4">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="h-5 rounded bg-white/4 animate-pulse" />
            ))}
          </div>
        ) : !activity || activity.length === 0 ? (
          <p className="text-sm text-white/20">No activity yet.</p>
        ) : (
          <div className="divide-y divide-white/5">
            {activity.map(entry => (
              <ActivityItem
                key={entry.id}
                entry={entry}
                userName={userMap.get(entry.user_id)}
              />
            ))}
          </div>
        )}
      </section>
    </div>
  )
}
