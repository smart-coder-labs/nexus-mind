import { useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Pause, Play } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { ConfirmModal } from '../../../components/ConfirmModal'
import { errorMessage, useFactory } from './ui'

/** Org-wide factory settings (the "pause all agents" switch and retention). */
export function useFactorySettings() {
  const { can, client, invalidate } = useFactory()
  const settings = useQuery({
    queryKey: ['autonomous-settings'],
    queryFn: () => client.getAutonomousAgentSettings(),
    enabled: can('autonomous_agent:read'),
  })
  const toggle = useMutation({
    mutationFn: (enabled: boolean) => client.patchAutonomousAgentSettings({ enabled }),
    onSuccess: () => invalidate('autonomous-settings', 'autonomous-runs'),
  })
  return { settings, toggle }
}

/** Runtime health of the Claude Code runtime the agents run on. */
export function useRuntimeHealth() {
  const { can, client, invalidate } = useFactory()
  const runtime = useQuery({
    queryKey: ['autonomous-runtime'],
    queryFn: () => client.getAutonomousRuntimeHealth(),
    enabled: can('autonomous_agent:read'),
  })
  const check = useMutation({
    mutationFn: () => client.checkAutonomousRuntimeHealth(),
    onSuccess: () => invalidate('autonomous-runtime'),
  })
  return { runtime, check }
}

/** Local Pi executor OAuth readiness; checking it does not invoke a model. */
export function usePiRuntimeHealth() {
  const { can, client } = useFactory()
  return useQuery({
    queryKey: ['autonomous-pi-runtime'],
    queryFn: () => client.getAutonomousPiRuntimeHealth(),
    enabled: can('autonomous_agent:read'),
    refetchInterval: 60_000,
  })
}

/**
 * "Pause all agents" / "Resume agents". Pausing stops every agent in the org, so
 * it is destructive-styled and always confirmed; resuming is a plain action.
 */
export function PauseAllControl({
  enabled,
  pending,
  onToggle,
  error,
}: {
  enabled: boolean
  pending: boolean
  onToggle: (enabled: boolean) => void
  error?: unknown
}) {
  const [confirming, setConfirming] = useState(false)
  return (
    <>
      {enabled ? (
        <Button
          size="sm"
          variant="destructive"
          leftIcon={<Pause className="h-3.5 w-3.5" />}
          loading={pending}
          onClick={() => setConfirming(true)}
        >
          Pausar Factory
        </Button>
      ) : (
        <Button
          size="sm"
          variant="secondary"
          leftIcon={<Play className="h-3.5 w-3.5" />}
          loading={pending}
          onClick={() => onToggle(true)}
        >
          Reactivar Factory
        </Button>
      )}
      {error != null && (
        <p role="alert" className="text-[12px] text-status-error">
          {errorMessage(error, 'No se pudo cambiar la pausa de Factory.')} Reintenta.
        </p>
      )}
      <ConfirmModal
        open={confirming}
        danger
        title="¿Pausar Factory?"
        description="Se impiden nuevos inicios en ambos motores, se cancelan ejecuciones activas y se revocan sus leases. La cola, las decisiones pendientes y la evidencia se conservan. No deshace efectos externos ya completados ni relanza ejecuciones canceladas al reactivar."
        confirmLabel="Pausar Factory"
        loading={pending}
        onClose={() => setConfirming(false)}
        onConfirm={() => {
          onToggle(false)
          setConfirming(false)
        }}
      />
    </>
  )
}
