import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Pencil, Plus, ShieldCheck, Trash2 } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui/EmptyState'
import { Input, Textarea } from '../components/ui/Input'
import { Modal, ModalContent, ModalFooter, ModalHeader, ModalTitle } from '../components/ui/Modal'
import { SandboxBotPanel } from './factory/SandboxBotPanel'
import { ShadowRouterPanel } from './factory/ShadowRouterPanel'
import type {
  FactoryAction,
  FactoryActionPolicy,
  FactoryPolicy,
  FactoryPolicyMode,
  FactoryTaskClass,
} from '../types'

const ACTIONS: FactoryAction[] = ['fix', 'publish', 'open_pr', 'reply', 'close', 'approve', 'merge', 'deploy', 'notify', 'recover']
const MODES: { value: FactoryPolicyMode; label: string }[] = [
  { value: 'never', label: 'never — the agent may not do this' },
  { value: 'manual', label: 'manual — a person decides every time' },
  { value: 'criteria', label: 'criteria — a decision model checks the conditions (held for a person until one is connected)' },
  { value: 'after_fix', label: 'after_fix — only once a verified fix exists' },
  { value: 'after_merge', label: 'after_merge — only once the change is merged' },
]
const TASK_CLASSES: FactoryTaskClass[] = ['docs', 'tests', 'ui', 'backend', 'bugfix', 'refactor', 'migration', 'infra', 'security', 'unknown']

const SELECT_CLASS = 'mt-1 block w-full rounded-lg border border-border-primary bg-transparent px-3 py-2 text-sm text-text-primary focus:border-accent-blue focus:outline-none'

interface Draft {
  action: FactoryAction
  mode: FactoryPolicyMode
  project: string
  taskClass: FactoryTaskClass | ''
  allow: string
  stop: string
  /** Version the save will send: 1 for a new policy, current + 1 when editing. */
  version: number
}

const lines = (text: string) => text.split('\n').map(line => line.trim()).filter(Boolean)

function toDraft(policy?: FactoryPolicy): Draft {
  return policy
    ? {
        action: policy.action,
        mode: policy.mode,
        project: policy.scope.project ?? '',
        taskClass: policy.scope.task_class ?? '',
        allow: policy.allow.join('\n'),
        stop: policy.stop.join('\n'),
        version: policy.version + 1,
      }
    : { action: 'fix', mode: 'manual', project: '', taskClass: '', allow: '', stop: '', version: 1 }
}

function toPolicy(draft: Draft): FactoryActionPolicy {
  const scope: FactoryActionPolicy['scope'] = {}
  if (draft.project.trim()) scope.project = draft.project.trim()
  if (draft.taskClass) scope.task_class = draft.taskClass
  return {
    schema_version: 1,
    action: draft.action,
    mode: draft.mode,
    scope,
    allow: lines(draft.allow),
    stop: lines(draft.stop),
    version: draft.version,
  }
}

function saveErrorMessage(error: unknown): string {
  const failure = error as { status?: number; code?: string; message?: string }
  if (failure.status === 409) return 'Someone changed this policy since you opened it. Reload the page and apply your change again.'
  if (failure.code === 'invalid_policy') return failure.message ?? 'The policy is not valid.'
  return failure.message ?? 'The policy could not be saved.'
}

function scopeLabel(policy: FactoryPolicy) {
  const parts = [policy.scope.project ?? 'any project', policy.scope.task_class ?? 'any task class']
  return parts.join(' · ')
}

export default function FactoryPolicies() {
  const { session } = useAuth()
  const permissions = session?.user.permissions ?? []
  const canRead = permissions.includes('factory_policy:read')
  const canWrite = permissions.includes('factory_policy:write')
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()

  const [draft, setDraft] = useState<Draft | null>(null)
  const [editing, setEditing] = useState(false)
  const [saveError, setSaveError] = useState('')

  const policies = useQuery({
    queryKey: ['factory-policies'],
    queryFn: () => client.listFactoryPolicies(),
    enabled: canRead,
  })

  const save = useMutation({
    mutationFn: (policy: FactoryActionPolicy) => client.putFactoryPolicy(policy),
    onSuccess: () => {
      setDraft(null)
      queryClient.invalidateQueries({ queryKey: ['factory-policies'] })
    },
    onError: error => setSaveError(saveErrorMessage(error)),
  })

  const [deleteError, setDeleteError] = useState('')
  const remove = useMutation({
    mutationFn: (policy: FactoryPolicy) => client.deleteFactoryPolicy(policy.id),
    onMutate: () => setDeleteError(''),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['factory-policies'] }),
    // A silent failure would leave the admin believing the action now waits for a
    // person while the policy is in fact still active.
    onError: (error, policy) => {
      const message = (error as { message?: string }).message ?? 'unknown error'
      setDeleteError(`Could not delete the ${policy.action} policy: ${message}. It is still active.`)
    },
  })

  const open = (policy?: FactoryPolicy) => {
    setSaveError('')
    setEditing(Boolean(policy))
    setDraft(toDraft(policy))
  }

  if (!canRead) {
    return (
      <div className="p-6 md:p-8 max-w-7xl mx-auto">
        <EmptyState
          title="Factory policies"
          description="You need the factory_policy:read permission to see what agents are allowed to do. Ask an organization owner to grant it."
        />
      </div>
    )
  }

  const rows = policies.data ?? []
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setDraft(current => (current ? { ...current, [key]: value } : current))

  return (
    <div className="p-6 md:p-8 space-y-6 max-w-7xl mx-auto">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary flex items-center gap-2">
            <ShieldCheck className="w-6 h-6" />Factory policies
          </h1>
          <p className="mt-1 max-w-2xl text-sm text-text-tertiary">
            Each action an agent can take is decided on its own: allowing a fix never allows a merge. Safety floors
            (auth, payments, migrations, dependencies, CI, failing checks) always require a person, whatever a policy says.
          </p>
        </div>
        {canWrite && (
          <Button size="sm" leftIcon={<Plus className="h-4 w-4" />} onClick={() => open()}>New policy</Button>
        )}
      </div>

      {policies.isLoading && <p className="text-sm text-text-tertiary">Loading policies…</p>}
      {policies.isError && <p role="alert" className="text-sm text-text-secondary">Policies could not be loaded. Try again in a moment.</p>}

      <SandboxBotPanel client={client} canWrite={canWrite} />
      <ShadowRouterPanel client={client} canWrite={canWrite} />

      {deleteError && <p role="alert" className="text-sm text-text-primary">{deleteError}</p>}

      {policies.isSuccess && rows.length === 0 && (
        <EmptyState
          title="No policies — every action waits for a person"
          description="Without a policy, agents may propose work but every fix, merge, reply or deploy is held for a human decision."
        />
      )}

      {rows.length > 0 && (
        <div className="overflow-x-auto rounded-xl border border-border-primary">
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-text-tertiary">
              <tr className="border-b border-border-secondary">
                <th scope="col" className="px-4 py-3 font-medium">Action</th>
                <th scope="col" className="px-4 py-3 font-medium">Scope</th>
                <th scope="col" className="px-4 py-3 font-medium">Mode</th>
                <th scope="col" className="px-4 py-3 font-medium">Conditions</th>
                <th scope="col" className="px-4 py-3 font-medium">Version</th>
                <th scope="col" className="px-4 py-3"><span className="sr-only">Actions</span></th>
              </tr>
            </thead>
            <tbody>
              {rows.map(policy => (
                <tr key={policy.id} className="border-b border-border-secondary last:border-0 align-top">
                  <td className="px-4 py-3 font-mono text-text-primary">{policy.action}</td>
                  <td className="px-4 py-3 text-text-secondary">{scopeLabel(policy)}</td>
                  <td className="px-4 py-3 text-text-primary">{policy.mode}</td>
                  <td className="px-4 py-3 text-text-tertiary">
                    {policy.allow.length} allow · {policy.stop.length} stop
                  </td>
                  <td className="px-4 py-3 text-text-tertiary">v{policy.version}</td>
                  <td className="px-4 py-3">
                    {canWrite && (
                      <div className="flex justify-end gap-1">
                        <Button size="sm" variant="ghost" aria-label={`Edit ${policy.action} policy`} leftIcon={<Pencil className="h-4 w-4" />} onClick={() => open(policy)}>Edit</Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          aria-label={`Delete ${policy.action} policy`}
                          leftIcon={<Trash2 className="h-4 w-4" />}
                          loading={remove.isPending && remove.variables?.id === policy.id}
                          onClick={() => {
                            if (window.confirm(`Delete the ${policy.action} policy? That action will wait for a person again.`)) remove.mutate(policy)
                          }}
                        >
                          Delete
                        </Button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <Modal open={draft !== null} onOpenChange={value => { if (!value) setDraft(null) }} size="lg">
        {draft && (
          <>
            <ModalHeader>
              <ModalTitle>{editing ? `Edit ${draft.action} policy` : 'New policy'}</ModalTitle>
            </ModalHeader>
            <ModalContent>
              <div className="space-y-4">
                <div className="grid gap-4 sm:grid-cols-2">
                  <label className="block text-sm text-text-secondary" htmlFor="policy-action">
                    Action
                    <select id="policy-action" className={SELECT_CLASS} value={draft.action} disabled={editing} onChange={event => set('action', event.target.value as FactoryAction)}>
                      {ACTIONS.map(action => <option key={action} value={action}>{action}</option>)}
                    </select>
                  </label>
                  <label className="block text-sm text-text-secondary" htmlFor="policy-mode">
                    Mode
                    <select id="policy-mode" className={SELECT_CLASS} value={draft.mode} onChange={event => set('mode', event.target.value as FactoryPolicyMode)}>
                      {MODES.map(mode => <option key={mode.value} value={mode.value}>{mode.label}</option>)}
                    </select>
                  </label>
                  <label className="block text-sm text-text-secondary" htmlFor="policy-project">
                    Project
                    <Input id="policy-project" inputSize="sm" value={draft.project} disabled={editing} placeholder="Any project" onChange={event => set('project', event.target.value)} />
                  </label>
                  <label className="block text-sm text-text-secondary" htmlFor="policy-task-class">
                    Task class
                    <select id="policy-task-class" className={SELECT_CLASS} value={draft.taskClass} disabled={editing} onChange={event => set('taskClass', event.target.value as FactoryTaskClass | '')}>
                      <option value="">Any task class</option>
                      {TASK_CLASSES.map(taskClass => <option key={taskClass} value={taskClass}>{taskClass}</option>)}
                    </select>
                  </label>
                </div>
                <label className="block text-sm text-text-secondary" htmlFor="policy-allow">
                  Allow when (one condition per line{draft.mode === 'criteria' ? ', at least one required' : ''})
                  <Textarea id="policy-allow" className="text-sm" rows={3} value={draft.allow} onChange={event => set('allow', event.target.value)} placeholder="Only docs or tests paths change" />
                </label>
                <label className="block text-sm text-text-secondary" htmlFor="policy-stop">
                  Stop when (one condition per line)
                  <Textarea id="policy-stop" className="text-sm" rows={3} value={draft.stop} onChange={event => set('stop', event.target.value)} placeholder="The source is an external email" />
                </label>
                {draft.action === 'merge' && draft.project.trim() && (
                  <p className="text-xs text-text-secondary">
                    Autonomous reviews do not know which project a pull request belongs to yet. While any merge policy is
                    scoped to a project, the factory holds every autonomous merge in the organization for a person rather
                    than guess which policy applies.
                  </p>
                )}
                {editing && <p className="text-xs text-text-tertiary">Action and scope identify the policy and cannot change. Delete it and create a new one to move it.</p>}
                {saveError && <p role="alert" className="text-sm text-text-primary">{saveError}</p>}
              </div>
            </ModalContent>
            <ModalFooter>
              <Button size="sm" variant="ghost" onClick={() => setDraft(null)}>Cancel</Button>
              <Button size="sm" loading={save.isPending} onClick={() => { setSaveError(''); save.mutate(toPolicy(draft)) }}>Save policy</Button>
            </ModalFooter>
          </>
        )}
      </Modal>
    </div>
  )
}
