import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Bot, KeyRound } from 'lucide-react'
import type { NexusMindClient } from '../../api/client'
import { Badge } from '../../components/ui/Badge'
import { Button } from '../../components/ui/Button'
import { Skeleton } from '../../components/ui/Skeleton'
import { InlineAlert, when } from './govern/ui'
import { readable } from './govern/words'

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
    <section aria-labelledby="sandbox-bot-title" className="space-y-4 rounded-xl border border-border-primary bg-foreground/[0.02] p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1 basis-64">
          <h2 id="sandbox-bot-title" className="flex items-center gap-2 text-[15px] font-semibold tracking-[-0.2px] text-text-primary">
            <Bot className="h-4 w-4 text-text-secondary" aria-hidden="true" />Sandbox bot
          </h2>
          <p className="mt-1 max-w-2xl text-sm leading-normal text-text-secondary">
            Sandboxed agents call NexusMind as this bot. Its permissions come from the <code className="font-mono text-[12px]">factory-bot</code> role
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

      {bot.isLoading && (
        <div className="space-y-2" aria-hidden="true">
          <Skeleton className="h-4 w-48" />
          <Skeleton className="h-4 w-64" />
        </div>
      )}
      {bot.isError && (
        <InlineAlert onRetry={() => bot.refetch()}>Could not load the sandbox bot. Reload it before issuing a key.</InlineAlert>
      )}
      {bot.isSuccess && !current && (
        <p className="text-sm text-text-secondary">No sandbox bot yet — sandboxed agents run without NexusMind context until a key is generated.</p>
      )}
      {current && (
        <dl className="grid gap-x-4 gap-y-1.5 text-sm sm:grid-cols-[8rem_1fr]">
          <dt className="text-text-tertiary">Status</dt>
          <dd>
            <Badge role="none" size="sm" variant={current.status === 'active' ? 'success' : 'default'}>{readable(current.status)}</Badge>
          </dd>
          <dt className="text-text-tertiary">Key issued</dt>
          <dd className="text-text-primary">{current.key_created_at ? when(current.key_created_at) : 'No active key'}</dd>
          <dt className="text-text-tertiary">Permissions</dt>
          <dd className="min-w-0 break-words font-mono text-[12px] text-text-secondary">{current.role_permissions.join(', ') || 'None'}</dd>
        </dl>
      )}

      {issuedKey && (
        <div role="status" className="space-y-2 rounded-md border border-status-warning/20 bg-status-warning/[0.08] p-3.5">
          <p className="text-sm text-text-primary">
            Copy this key into the proxy secret (<code className="font-mono text-[12px]">FACTORY_NEXUSMIND_KEYS</code>) now. It will not be shown again.
          </p>
          <code className="block break-all rounded-md bg-background-primary/60 p-2.5 font-mono text-[12px] text-text-primary">{issuedKey}</code>
          <Button size="sm" variant="secondary" onClick={() => setIssuedKey(null)}>I stored it</Button>
        </div>
      )}
      {error && <InlineAlert>{error}</InlineAlert>}
    </section>
  )
}
