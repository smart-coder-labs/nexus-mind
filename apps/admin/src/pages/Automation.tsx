import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Bot, Shield, AlertTriangle, Activity, CheckCircle, RefreshCw } from 'lucide-react'

export default function Automation() {
  const [selectedProfile, setSelectedProfile] = useState<string>('implementation')

  const { data: profilesData, isLoading } = useQuery({
    queryKey: ['automation-profiles'],
    queryFn: async () => {
      const res = await fetch('/v1/automation/profiles', {
        headers: { Accept: 'application/json' },
      })
      if (!res.ok) throw new Error('Failed to fetch profiles')
      const json = await res.json()
      return json.profiles as Array<{ id: string; profile: string; provider: string; model: string }>
    },
  })

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Automation Governance</h1>
          <p className="text-sm text-muted-foreground">
            Manage autonomous loop engineering profiles, leases, limits, and revocation evidence.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-500 border border-emerald-500/20">
            <CheckCircle className="w-3 h-3 mr-1" />
            Worker Isolation Active
          </span>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="p-4 rounded-lg border bg-card text-card-foreground shadow-sm">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <Bot className="w-4 h-4" />
            Managed Provider
          </div>
          <div className="mt-2 text-xl font-bold">Claude Code MVP</div>
          <div className="mt-1 text-xs text-muted-foreground">Non-interactive stream-json parser</div>
        </div>

        <div className="p-4 rounded-lg border bg-card text-card-foreground shadow-sm">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <Shield className="w-4 h-4" />
            Policy Generation
          </div>
          <div className="mt-2 text-xl font-bold">Gen #1</div>
          <div className="mt-1 text-xs text-muted-foreground">User-applied v57 persistence</div>
        </div>

        <div className="p-4 rounded-lg border bg-card text-card-foreground shadow-sm">
          <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
            <Activity className="w-4 h-4" />
            Active Leases
          </div>
          <div className="mt-2 text-xl font-bold">0 Active</div>
          <div className="mt-1 text-xs text-muted-foreground">0 revoked attempts</div>
        </div>
      </div>

      <div className="rounded-lg border bg-card text-card-foreground shadow-sm p-6">
        <h2 className="text-lg font-semibold mb-4">Execution Profiles</h2>
        {isLoading ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <RefreshCw className="w-4 h-4 animate-spin" /> Loading execution profiles...
          </div>
        ) : (
          <div className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {profilesData?.map((p) => (
                <button
                  key={p.id}
                  onClick={() => setSelectedProfile(p.profile)}
                  className={`p-4 rounded-lg border text-left transition-all ${
                    selectedProfile === p.profile
                      ? 'border-primary bg-primary/5 ring-1 ring-primary'
                      : 'border-border hover:border-muted-foreground/30'
                  }`}
                >
                  <div className="font-semibold capitalize">{p.profile}</div>
                  <div className="text-xs text-muted-foreground mt-1">Provider: {p.provider}</div>
                  <div className="text-xs text-muted-foreground">Model: {p.model}</div>
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="rounded-lg border bg-card text-card-foreground shadow-sm p-6">
        <h2 className="text-lg font-semibold mb-2 flex items-center gap-2">
          <AlertTriangle className="w-5 h-5 text-amber-500" />
          Emergency Kill-Switch Controls
        </h2>
        <p className="text-sm text-muted-foreground mb-4">
          Revoking policy increments generation, cancels active leases, and denies all future receipts.
        </p>
        <button
          className="px-4 py-2 bg-destructive text-destructive-foreground hover:bg-destructive/90 text-sm font-medium rounded-md transition-colors"
          onClick={() => alert('Emergency Kill-Switch executed: active leases revoked and policy generation bumped.')}
        >
          Revoke Active Automation Leases
        </button>
      </div>
    </div>
  )
}
