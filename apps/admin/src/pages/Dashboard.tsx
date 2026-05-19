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
        <h1 className="text-2xl font-bold text-text-primary">Dashboard</h1>
        <p className="text-text-secondary mt-1 text-sm">
          {session?.org.name} — organization overview
        </p>
      </div>

      {/* Stat cards */}
      <section aria-label="Organization statistics">
        {statsError ? (
          <div className="rounded-2xl border border-status-error/30 bg-status-error/10 p-4 text-sm text-status-error">
            Failed to load statistics. Check your connection and try again.
          </div>
        ) : statsLoading ? (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <Skeleton key={i} className="h-32 rounded-2xl" />
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

      {/* Activity timeline */}
      <section aria-label="Recent activity">
        <h2 className="text-lg font-semibold text-text-primary mb-4">
          Recent Activity
        </h2>
        <div className="bg-surface-primary border border-border-primary rounded-2xl px-6 divide-y divide-border-secondary">
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
              <ActivityItem key={entry.id} entry={entry} />
            ))
          )}
        </div>
      </section>
    </div>
  )
}
