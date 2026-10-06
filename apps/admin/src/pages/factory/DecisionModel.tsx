import { useMemo } from 'react'
import { createClient } from '../../api/client'
import { useAuth } from '../../auth/AuthContext'
import { EmptyState } from '../../components/ui/EmptyState'
import { ShadowRouterPanel } from './ShadowRouterPanel'

/** The decision model in shadow: how close each task class is to automatic routing. */
export default function DecisionModel() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  const client = useMemo(() => createClient(), [session])

  if (!canRead) {
    return (
      <div className="p-6 md:p-8 max-w-7xl mx-auto">
        <EmptyState title="Decision model" description="You need the factory_policy:read permission to see the decision model. Ask an organization owner to grant it." />
      </div>
    )
  }

  return (
    <div className="p-6 md:p-8 space-y-6 max-w-7xl mx-auto">
      <header>
        <h1 className="text-[22px] font-semibold tracking-[-0.3px] text-text-primary">Decision model</h1>
        <p className="mt-1 text-[13px] text-text-secondary">Judges every reviewed pull request without changing what the factory does.</p>
      </header>
      <ShadowRouterPanel client={client} canWrite={canWrite} />
    </div>
  )
}
