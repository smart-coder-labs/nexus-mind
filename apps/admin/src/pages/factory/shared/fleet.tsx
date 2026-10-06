import { useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Pause, Play } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { ConfirmModal } from '../../../components/ConfirmModal'
import { errorMessage, useFactory } from './ui'

/** Org-wide factory settings (the "pause all agents" switch and retention). */
export function useFactorySettings() {
  const { can, client, invalidate } = useFactory()
  const settings = useQuery({ queryKey: ['autonomous-settings'], queryFn: () => client.getAutonomousAgentSettings(), enabled: can('autonomous_agent:read') })
  const toggle = useMutation({ mutationFn: (enabled: boolean) => client.patchAutonomousAgentSettings({ enabled }), onSuccess: () => invalidate('autonomous-settings', 'autonomous-runs') })
  return { settings, toggle }
}

/** Runtime health of the Claude Code runtime the agents run on. */
export function useRuntimeHealth() {
  const { can, client, invalidate } = useFactory()
  const runtime = useQuery({ queryKey: ['autonomous-runtime'], queryFn: () => client.getAutonomousRuntimeHealth(), enabled: can('autonomous_agent:read') })
  const check = useMutation({ mutationFn: () => client.checkAutonomousRuntimeHealth(), onSuccess: () => invalidate('autonomous-runtime') })
  return { runtime, check }
}

/**
 * "Pause all agents" / "Resume agents". Pausing stops every agent in the org, so
 * it is destructive-styled and always confirmed; resuming is a plain action.
 */
export function PauseAllControl({ enabled, pending, onToggle, error }: { enabled: boolean; pending: boolean; onToggle: (enabled: boolean) => void; error?: unknown }) {
  const [confirming, setConfirming] = useState(false)
  return (
    <>
      {enabled
        ? <Button size="sm" variant="destructive" leftIcon={<Pause className="h-3.5 w-3.5" />} loading={pending} onClick={() => setConfirming(true)}>Pause all agents</Button>
        : <Button size="sm" variant="secondary" leftIcon={<Play className="h-3.5 w-3.5" />} loading={pending} onClick={() => onToggle(true)}>Resume agents</Button>}
      {error != null && <p role="alert" className="text-[12px] text-status-error">{errorMessage(error, 'Could not change the fleet state.')} Try again.</p>}
      <ConfirmModal
        open={confirming}
        danger
        title="Pause all agents?"
        description="Runs in progress are cancelled now, and no agent in this organization starts a new run until you resume. Schedules and queued runs stay saved."
        confirmLabel="Pause all agents"
        loading={pending}
        onClose={() => setConfirming(false)}
        onConfirm={() => { onToggle(false); setConfirming(false) }}
      />
    </>
  )
}
