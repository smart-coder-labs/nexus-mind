import { useMemo } from 'react'
import { createClient } from '../../api/client'
import { useAuth } from '../../auth/AuthContext'
import AutonomousAgents from '../AutonomousAgents'
import { SandboxBotPanel } from './SandboxBotPanel'

/** Runtime health, the organization kill switch, retention and the sandbox bot. */
export default function FactorySettings() {
  const { session } = useAuth()
  const canWrite = (session?.user.permissions ?? []).includes('factory_policy:write')
  const client = useMemo(() => createClient(), [session])
  return (
    <div className="space-y-2">
      <AutonomousAgents section="runtime" />
      <div className="px-6 md:px-8 pb-8 max-w-7xl mx-auto">
        <SandboxBotPanel client={client} canWrite={canWrite} />
      </div>
    </div>
  )
}
