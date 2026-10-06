import { useMemo } from 'react'
import { createClient } from '../../api/client'
import { useAuth } from '../../auth/AuthContext'
import { Badge } from '../../components/ui/Badge'
import { ShadowRouterPanel } from './ShadowRouterPanel'
import { PageHeader, PermissionDenied } from './govern/ui'

/** The decision model in shadow: how close each task class is to automatic routing. */
export default function DecisionModel() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  const client = useMemo(() => createClient(), [session])

  if (!canRead) {
    return <PermissionDenied title="Decision model" permission="factory_policy:read" what="see the decision model" />
  }

  return (
    <div className="p-6 md:p-8 space-y-6 max-w-7xl mx-auto">
      <PageHeader
        title="Decision model"
        aside={<Badge role="note" variant="default">Shadow — observes, never acts</Badge>}
        subtitle="Judges every reviewed pull request and records what it would have done, without changing what the factory does."
      />
      <ShadowRouterPanel client={client} canWrite={canWrite} />
    </div>
  )
}
