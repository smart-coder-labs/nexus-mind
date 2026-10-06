import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Pencil, Plus } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/ui/Button'
import { Skeleton } from '../components/ui/Skeleton'
import { cn } from '../lib/utils'
import type { FactoryAction, FactoryActionPolicy, FactoryPolicy } from '../types'
import { PolicyEditor, toDraft, toPolicy, type PolicyDraft } from './factory/govern/PolicyEditor'
import { SafetyFloors } from './factory/govern/SafetyFloors'
import { InlineAlert, PageHeader, PermissionDenied } from './factory/govern/ui'
import { ACTION_DESCRIPTION, ACTIONS, RUNGS, modeMeta, rungOf, taskClassLabel, type Rung } from './factory/govern/words'

function saveErrorMessage(error: unknown): string {
  const failure = error as { status?: number; code?: string; message?: string }
  if (failure.status === 409) return 'Someone changed this policy since you opened it. Reload the page and apply your change again.'
  if (failure.code === 'invalid_policy') return failure.message ?? 'The policy is not valid.'
  return failure.message ?? 'The policy could not be saved.'
}

function scopeLabel(policy: FactoryPolicy): string {
  const parts = [policy.scope.project, policy.scope.task_class ? taskClassLabel(policy.scope.task_class) : undefined].filter(Boolean)
  return parts.join(', ')
}

const isDefaultScope = (policy: FactoryPolicy) => !policy.scope.project && !policy.scope.task_class

function conditionsLabel(policy: FactoryPolicy): string {
  if (policy.allow.length + policy.stop.length === 0) return 'No conditions'
  return `Conditions: ${policy.allow.length} allow, ${policy.stop.length} stop`
}

/**
 * Autonomy policies (factory PLAN §4): one row per action, the default policy
 * placed on the Never / Person / Gated / Model ladder, scoped overrides as chips.
 * An action with no policy waits for a person.
 */
export default function FactoryPolicies() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()

  const [draft, setDraft] = useState<PolicyDraft | null>(null)
  const [editingPolicy, setEditingPolicy] = useState<FactoryPolicy | null>(null)
  const [saveError, setSaveError] = useState('')
  const [deleteError, setDeleteError] = useState('')

  const policies = useQuery({
    queryKey: ['factory-policies'],
    queryFn: () => client.listFactoryPolicies(),
    enabled: canRead,
  })

  const close = () => {
    setDraft(null)
    setEditingPolicy(null)
  }

  const save = useMutation({
    mutationFn: (policy: FactoryActionPolicy) => client.putFactoryPolicy(policy),
    onSuccess: () => {
      close()
      queryClient.invalidateQueries({ queryKey: ['factory-policies'] })
    },
    onError: error => setSaveError(saveErrorMessage(error)),
  })

  const remove = useMutation({
    mutationFn: (policy: FactoryPolicy) => client.deleteFactoryPolicy(policy.id),
    onMutate: () => setDeleteError(''),
    onSuccess: () => {
      close()
      queryClient.invalidateQueries({ queryKey: ['factory-policies'] })
    },
    // A silent failure would leave the admin believing the action now waits for a
    // person while the policy is in fact still active.
    onError: (error, policy) => {
      const message = (error as { message?: string }).message ?? 'unknown error'
      setDeleteError(`Could not delete the ${policy.action} policy: ${message}. It is still active.`)
    },
  })

  const open = (policy?: FactoryPolicy, action?: FactoryAction) => {
    setSaveError('')
    setDeleteError('')
    setEditingPolicy(policy ?? null)
    setDraft(toDraft(policy, action))
  }

  if (!canRead) {
    return <PermissionDenied title="Autonomy policies" permission="factory_policy:read" what="see what agents are allowed to do" />
  }

  const rows = policies.data ?? []

  return (
    <div className="p-6 md:p-8 space-y-6 max-w-7xl mx-auto">
      <PageHeader
        title="Autonomy policies"
        subtitle="How far an agent may go on its own, decided per action. Allowing a fix never allows a merge. An action with no policy waits for a person."
        action={canWrite && <Button size="sm" leftIcon={<Plus className="h-4 w-4" />} onClick={() => open()}>New policy</Button>}
      />

      <SafetyFloors />

      {policies.isError && (
        <InlineAlert onRetry={() => policies.refetch()}>The policies could not be loaded, so this page cannot show what agents may do.</InlineAlert>
      )}

      {policies.isLoading && (
        <div className="space-y-2" aria-hidden="true">
          {ACTIONS.map(action => <Skeleton key={action} className="h-[72px] w-full rounded-[18px]" />)}
        </div>
      )}

      {policies.isSuccess && (
        <section aria-labelledby="matrix-title" className="space-y-3">
          <div className="flex flex-wrap items-end justify-between gap-x-4 gap-y-1">
            <h2 id="matrix-title" className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">Actions</h2>
            <p className="text-[12px] text-text-tertiary">
              {rows.length === 0
                ? 'No policies yet, so every action waits for a person.'
                : 'The highlighted step applies to all work unless an override matches.'}
            </p>
          </div>
          <ul aria-label="Policy per action" className="m-0 list-none divide-y divide-border-secondary rounded-[18px] border border-border-primary p-0">
            {ACTIONS.map(action => (
              <ActionRow
                key={action}
                action={action}
                policies={rows.filter(policy => policy.action === action)}
                canWrite={canWrite}
                onEdit={policy => open(policy)}
                onAdd={() => open(undefined, action)}
              />
            ))}
          </ul>
        </section>
      )}

      <PolicyEditor
        draft={draft}
        editing={editingPolicy !== null}
        onChange={setDraft}
        onClose={close}
        saving={save.isPending}
        saveError={saveError}
        onSave={() => {
          if (!draft) return
          setSaveError('')
          save.mutate(toPolicy(draft))
        }}
        deleting={remove.isPending}
        deleteError={deleteError}
        onDelete={
          editingPolicy
            ? () => {
                if (window.confirm(`Delete the ${editingPolicy.action} policy? That action will wait for a person again.`)) remove.mutate(editingPolicy)
              }
            : undefined
        }
      />
    </div>
  )
}

const CHIP_CLASS =
  'inline-flex h-6 items-center gap-1.5 rounded-full border border-white/[0.09] bg-white/[0.06] px-2.5 text-[11px] font-medium text-text-secondary'

function ActionRow({
  action,
  policies,
  canWrite,
  onEdit,
  onAdd,
}: {
  action: FactoryAction
  policies: FactoryPolicy[]
  canWrite: boolean
  onEdit: (policy: FactoryPolicy) => void
  onAdd: () => void
}) {
  const base = policies.find(isDefaultScope)
  const overrides = policies.filter(policy => !isDefaultScope(policy))
  const meta = base ? modeMeta(base.mode) : null
  const labelId = `policy-row-${action}`
  const gatedDetail = base?.mode === 'after_fix' ? 'after fix' : base?.mode === 'after_merge' ? 'after merge' : undefined

  return (
    <li aria-labelledby={labelId} className="grid gap-3 px-5 py-4 lg:grid-cols-[minmax(0,13rem)_minmax(0,1fr)_auto] lg:items-center lg:gap-6">
      <div className="min-w-0">
        <p id={labelId} className="font-mono text-[13px] font-medium text-text-primary">{action}</p>
        <p className="mt-0.5 text-[12px] leading-normal text-text-tertiary">{ACTION_DESCRIPTION[action]}</p>
      </div>

      <div className="min-w-0 space-y-2">
        <Ladder current={base ? rungOf(base.mode) : 'person'} implicit={!base} gatedDetail={gatedDetail} />
        <p className="text-[13px] leading-normal text-text-secondary">
          {base && meta ? (
            <>
              <span className="text-text-primary">{meta.short}.</span> {meta.description}{' '}
              <span className="whitespace-nowrap text-text-tertiary">{conditionsLabel(base)}</span>
            </>
          ) : (
            <>No policy{overrides.length ? ' for other work' : ''} — waits for a person.</>
          )}
        </p>
        {overrides.length > 0 && (
          <ul aria-label={`Overrides for ${action}`} className="m-0 flex list-none flex-wrap gap-1.5 p-0">
            {overrides.map(policy => {
              const label = `${scopeLabel(policy)}: ${modeMeta(policy.mode).short}`
              return (
                <li key={policy.id}>
                  {canWrite ? (
                    <button
                      type="button"
                      aria-label={`Edit ${action} policy for ${scopeLabel(policy)}`}
                      onClick={() => onEdit(policy)}
                      className={cn(
                        CHIP_CLASS,
                        'transition-apple hover:bg-white/[0.1] hover:text-text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring',
                      )}
                    >
                      {label}
                      <Pencil className="h-3 w-3" aria-hidden="true" />
                    </button>
                  ) : (
                    <span className={CHIP_CLASS}>{label}</span>
                  )}
                </li>
              )
            })}
          </ul>
        )}
      </div>

      {canWrite && (
        <div className="flex lg:justify-end">
          {base ? (
            <Button size="sm" variant="secondary" aria-label={`Edit ${action} policy`} leftIcon={<Pencil className="h-4 w-4" />} onClick={() => onEdit(base)}>
              Edit
            </Button>
          ) : (
            <Button size="sm" variant="secondary" aria-label={`Add a policy for ${action}`} leftIcon={<Plus className="h-4 w-4" />} onClick={onAdd}>
              Add policy
            </Button>
          )}
        </div>
      )}
    </li>
  )
}

/** Never / Person / Gated / Model, with the step that applies highlighted. */
function Ladder({ current, implicit, gatedDetail }: { current: Rung; implicit: boolean; gatedDetail?: string }) {
  return (
    <ol aria-label="Who decides" className="m-0 grid max-w-xl list-none grid-cols-4 gap-1 p-0">
      {RUNGS.map(({ rung, label }) => {
        const active = rung === current
        return (
          <li
            key={rung}
            aria-current={active ? 'step' : undefined}
            className={cn(
              'flex h-8 min-w-0 items-center justify-center rounded-[8px] border px-1.5 text-[12px]',
              active && !implicit && 'border-accent-blue/60 bg-accent-blue-tint font-medium text-text-primary',
              active && implicit && 'border-dashed border-white/[0.25] font-medium text-text-secondary',
              !active && 'border-border-secondary text-text-tertiary',
            )}
          >
            <span className="truncate">
              {label}
              {active && gatedDetail && <span className="hidden font-normal text-text-secondary sm:inline"> · {gatedDetail}</span>}
            </span>
          </li>
        )
      })}
    </ol>
  )
}
