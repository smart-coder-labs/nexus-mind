import { useState, useEffect } from 'react'
import { Link } from 'react-router-dom'
import { Building2, Users, Brain, ArrowRight, AlertCircle, RefreshCw } from 'lucide-react'
import { listOrgs } from '../api/client'
import type { Org } from '../types'
import { cn } from '@/lib/utils'

function StatCard({
  label,
  value,
  icon: Icon,
  loading,
}: {
  label: string
  value: number | string
  icon: React.ComponentType<{ className?: string }>
  loading: boolean
}) {
  return (
    <div className="bg-surface-primary border border-border-primary rounded-xl p-5">
      <div className="flex items-center gap-3 mb-3">
        <div className="w-8 h-8 rounded-lg bg-accent-blue-tint flex items-center justify-center">
          <Icon className="w-4 h-4 text-accent-blue" />
        </div>
        <span className="text-xs font-medium text-text-tertiary uppercase tracking-wider">{label}</span>
      </div>
      {loading ? (
        <div className="h-8 w-16 bg-surface-secondary animate-pulse rounded-md" />
      ) : (
        <p className="text-3xl font-semibold text-text-primary tracking-tight">{value}</p>
      )}
    </div>
  )
}

export default function Dashboard() {
  const [orgs, setOrgs] = useState<Org[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')

  const fetchOrgs = () => {
    setLoading(true)
    setError('')
    listOrgs()
      .then(setOrgs)
      .catch(err => setError(err.message ?? 'Failed to load organizations'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { fetchOrgs() }, [])

  const recent = [...orgs]
    .sort((a, b) => b.created_at.localeCompare(a.created_at))
    .slice(0, 5)

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-8 animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-text-primary">Overview</h1>
          <p className="text-sm text-text-secondary mt-0.5">System-wide metrics across all organizations</p>
        </div>
        <button
          id="dashboard-refresh"
          onClick={fetchOrgs}
          disabled={loading}
          className={cn(
            'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs text-text-secondary hover:text-text-primary hover:bg-surface-secondary transition-colors',
            loading && 'opacity-50 cursor-not-allowed',
          )}
        >
          <RefreshCw className={cn('w-3 h-3', loading && 'animate-spin')} />
          Refresh
        </button>
      </div>

      {error && (
        <div className="flex items-center gap-2 px-4 py-3 bg-status-error/10 border border-status-error/20 rounded-lg text-sm text-status-error">
          <AlertCircle className="w-4 h-4 flex-shrink-0" />
          {error}
        </div>
      )}

      {/* KPI row */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
        <StatCard
          label="Organizations"
          value={orgs.length}
          icon={Building2}
          loading={loading}
        />
        <StatCard
          label="Total Users"
          value="—"
          icon={Users}
          loading={loading}
        />
        <StatCard
          label="Total Memories"
          value="—"
          icon={Brain}
          loading={loading}
        />
      </div>

      {/* Recent orgs */}
      <div className="bg-surface-primary border border-border-primary rounded-xl overflow-hidden">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border-secondary">
          <h2 className="text-sm font-medium text-text-primary">Recent Organizations</h2>
          <Link
            to="/orgs"
            className="flex items-center gap-1 text-xs text-accent-blue hover:text-accent-blue-hover transition-colors"
          >
            View all <ArrowRight className="w-3 h-3" />
          </Link>
        </div>

        {loading ? (
          <div className="divide-y divide-border-secondary">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="px-5 py-3.5 flex items-center gap-3">
                <div className="w-32 h-4 bg-surface-secondary animate-pulse rounded" />
                <div className="w-20 h-3 bg-surface-secondary animate-pulse rounded ml-auto" />
              </div>
            ))}
          </div>
        ) : recent.length === 0 ? (
          <div className="px-5 py-10 text-center text-sm text-text-tertiary">
            No organizations yet.{' '}
            <Link to="/orgs" className="text-accent-blue hover:underline">
              Create the first one
            </Link>
          </div>
        ) : (
          <div className="divide-y divide-border-secondary">
            {recent.map(org => (
              <Link
                key={org.id}
                to={`/orgs/${org.id}`}
                className="flex items-center justify-between px-5 py-3.5 hover:bg-surface-secondary/40 transition-colors group"
              >
                <div className="flex items-center gap-3">
                  <div className="w-7 h-7 rounded-md bg-accent-blue-tint flex items-center justify-center flex-shrink-0">
                    <Building2 className="w-3.5 h-3.5 text-accent-blue" />
                  </div>
                  <div>
                    <p className="text-sm font-medium text-text-primary">{org.name}</p>
                    <p className="text-xs text-text-tertiary">{org.slug}</p>
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-xs text-text-tertiary">
                    {new Date(org.created_at).toLocaleDateString()}
                  </span>
                  <ArrowRight className="w-3.5 h-3.5 text-text-quaternary group-hover:text-text-secondary transition-colors" />
                </div>
              </Link>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
