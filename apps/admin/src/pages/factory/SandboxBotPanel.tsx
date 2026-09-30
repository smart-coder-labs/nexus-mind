import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Bot, KeyRound } from 'lucide-react'
import type { NexusMindClient } from '../../api/client'
import { Button } from '../../components/ui/Button'

/**
 * The organization's sandbox bot: the identity sandboxed agents use to call
 * NexusMind. Its key lives only in the egress proxy's secret, so it is shown once
 * and never readable again.
 */
export function SandboxBotPanel({ client, canWrite }: { client: NexusMindClient; canWrite: boolean }) {
  const queryClient = useQueryClient()
  const [issuedKey, setIssuedKey] = useState<string | null>(null)
  const [error, setError] = useState('')

  const bot = useQuery({ queryKey: ['factory-bot'], queryFn: () => client.getFactoryBot() })
  const current = bot.data?.bot ?? null
  const hasKey = Boolean(current?.key_created_at)

  const rotate = useMutation({
    mutationFn: () => client.rotateFactoryBotKey(),
    onMutate: () => setError(''),
    onSuccess: result => {
      setIssuedKey(result.api_key)
      queryClient.setQueryData(['factory-bot'], { bot: result.bot })
    },
    onError: failure => setError((failure as { message?: string }).message ?? 'The key could not be issued.'),
  })

  const onRotate = () => {
    if (hasKey && !window.confirm('Rotate the bot key? The key in the egress proxy stops working until you replace it.')) return
    rotate.mutate()
  }

  return (
    <section aria-labelledby="sandbox-bot-title" className="rounded-xl border border-border-primary p-4 space-y-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 id="sandbox-bot-title" className="text-sm font-semibold text-text-primary flex items-center gap-2">
            <Bot className="h-4 w-4" />Sandbox bot
          </h2>
          <p className="mt-1 max-w-2xl text-xs text-text-tertiary">
            Sandboxed agents call NexusMind as this bot. Its permissions come from the <code>factory-bot</code> role
            (edit it in Roles). The key goes only into the egress proxy secret; agents never see it.
          </p>
        </div>
        {canWrite && (
          // Only once the current state is known: otherwise an active key could be
          // revoked without the confirmation that protects it.
          <Button size="sm" variant="secondary" leftIcon={<KeyRound className="h-4 w-4" />} loading={rotate.isPending} disabled={!bot.isSuccess} onClick={onRotate}>
            {hasKey ? 'Rotate bot key' : 'Generate bot key'}
          </Button>
        )}
      </div>

      {bot.isLoading && <p className="text-xs text-text-tertiary">Loading…</p>}
      {bot.isError && (
        <p role="alert" className="text-xs text-text-primary">Could not load the sandbox bot. Reload the page before issuing a key.</p>
      )}
      {bot.isSuccess && !current && (
        <p className="text-xs text-text-secondary">No sandbox bot yet — sandboxed agents run without NexusMind context until a key is generated.</p>
      )}
      {current && (
        <dl className="grid gap-1 text-xs sm:grid-cols-[8rem_1fr]">
          <dt className="text-text-tertiary">Status</dt>
          <dd className="text-text-primary">{current.status}</dd>
          <dt className="text-text-tertiary">Key issued</dt>
          <dd className="text-text-primary">{current.key_created_at ?? 'no active key'}</dd>
          <dt className="text-text-tertiary">Permissions</dt>
          <dd className="font-mono text-text-secondary">{current.role_permissions.join(', ') || 'none'}</dd>
        </dl>
      )}

      {issuedKey && (
        <div role="status" className="rounded-lg border border-border-secondary p-3 space-y-2">
          <p className="text-xs text-text-primary">
            Copy this key into the proxy secret (<code>FACTORY_NEXUSMIND_KEYS</code>) now. It will not be shown again.
          </p>
          <code className="block break-all rounded bg-black/30 p-2 font-mono text-xs text-text-primary">{issuedKey}</code>
          <Button size="sm" variant="ghost" onClick={() => setIssuedKey(null)}>I stored it</Button>
        </div>
      )}
      {error && <p role="alert" className="text-xs text-text-primary">{error}</p>}
    </section>
  )
}
