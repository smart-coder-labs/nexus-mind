import { Navigate } from 'react-router-dom'
import { useAuth } from '../auth/AuthContext'
import AgentsPage from './factory/agents/AgentsPage'
import FindingsPage from './factory/findings/FindingsPage'
import RunsPage from './factory/runs/RunsPage'
import TemplatesPage from './factory/templates/TemplatesPage'
import FactorySettings from './factory/FactorySettings'

export type AutonomousAgentsSection = 'agents' | 'templates' | 'runs' | 'findings' | 'runtime'

/**
 * The factory's "operate" pages, one per route (/factory, /factory/runs,
 * /factory/findings, /factory/templates). The sidebar is the switcher, so there
 * is no in-page tab bar. `runtime` is kept for old callers and renders the
 * Factory settings page.
 */
export default function AutonomousAgents({ section = 'agents' }: { section?: AutonomousAgentsSection }) {
  const { session } = useAuth()
  if (!(session?.user.permissions ?? []).includes('autonomous_agent:read')) return <Navigate to="/401" replace />
  switch (section) {
    case 'runs': return <RunsPage />
    case 'findings': return <FindingsPage />
    case 'templates': return <TemplatesPage />
    case 'runtime': return <FactorySettings />
    default: return <AgentsPage />
  }
}
