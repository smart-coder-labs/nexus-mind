import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { StatisticDisplay } from '@/components/ui/StatisticDisplay/StatisticDisplay'
import { Skeleton } from '@/components/ui/Skeleton/Skeleton'
import { EmptyState } from '@/components/ui/EmptyState/EmptyState'
import { ActivityItem } from '../components/ActivityItem'

export default function Dashboard() {
  const { session } = useAuth()

  const client = useMemo(
    () => createClient(),
    [session],
  )

  const isAdmin = session?.user.role === 'admin'

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

  const userMap = useMemo(() => {
    const map = new Map<string, string>()
    users?.forEach(u => map.set(u.id, u.name))
    return map
  }, [users])

  const metrics = useMemo(() => {
    if (!stats) return []
    return [
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
    ]
  }, [stats])

  return (
    <div className="p-6 space-y-8 max-w-7xl mx-auto">
      <div>
        <h1 className="text-[34px] font-semibold text-text-primary tracking-[-0.28px]">Dashboard</h1>
        <p className="text-text-secondary mt-1 text-sm">
          {session?.org.name} — organization overview
        </p>
      </div>

      {/* Stat cards */}
      {isAdmin && (
        <section aria-label="Organization statistics">
          {statsError ? (
            <div className="rounded-[18px] border border-status-error/30 bg-status-error/10 p-4 text-sm text-status-error">
              Failed to load statistics. Check your connection and try again.
            </div>
          ) : statsLoading ? (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
              {Array.from({ length: 4 }).map((_, i) => (
                <Skeleton key={i} className="h-32 rounded-[18px]" />
              ))}
            </div>
          ) : (
            <StatisticDisplay
              metrics={metrics}
              columns={4}
              variant="card"
              size="md"
            />
          )}
        </section>
      )}

      {/* Activity timeline */}
      {isAdmin && (
        <section aria-label="Recent activity">
          <h2 className="text-[21px] font-semibold text-text-primary mb-4 tracking-[0.231px]">
            Recent Activity
          </h2>
          <div className="bg-[#272729] border border-white/[0.06] rounded-[18px] px-6 divide-y divide-border-secondary">
            {activityLoading ? (
              Array.from({ length: 5 }).map((_, i) => (
                <div key={i} className="py-3">
                  <Skeleton className="h-6 w-full rounded-md" />
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
