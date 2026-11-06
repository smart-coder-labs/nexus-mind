import { StyledSelect } from '@/components/ui/Select/StyledSelect'
import { Plus, Trash2, X } from 'lucide-react'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import { Modal, ModalContent, ModalDescription, ModalFooter, ModalHeader, ModalTitle } from '../../../components/ui/Modal'
import type { FactoryAction, FactoryActionPolicy, FactoryPolicy, FactoryPolicyMode, FactoryTaskClass } from '../../../types'
import { Field, FieldGroup, InlineAlert, RadioCard, SELECT_CLASS } from './ui'
import { ACTION_DESCRIPTION, ACTIONS, MODES, TASK_CLASSES, taskClassLabel } from './words'

export interface PolicyDraft {
  action: FactoryAction
  mode: FactoryPolicyMode
  project: string
  taskClass: FactoryTaskClass | ''
  allow: string[]
  stop: string[]
  /** Version the save will send: 1 for a new policy, current + 1 when editing. */
  version: number
}

export function toDraft(policy?: FactoryPolicy, action: FactoryAction = 'fix'): PolicyDraft {
  return policy
    ? {
        action: policy.action,
        mode: policy.mode,
        project: policy.scope.project ?? '',
        taskClass: policy.scope.task_class ?? '',
        allow: policy.allow.length ? [...policy.allow] : [''],
        stop: [...policy.stop],
        version: policy.version + 1,
      }
    : { action, mode: 'manual', project: '', taskClass: '', allow: [''], stop: [], version: 1 }
}

const clean = (items: string[]) => items.map(item => item.trim()).filter(Boolean)

export function toPolicy(draft: PolicyDraft): FactoryActionPolicy {
  const scope: FactoryActionPolicy['scope'] = {}
  if (draft.project.trim()) scope.project = draft.project.trim()
  if (draft.taskClass) scope.task_class = draft.taskClass
  return {
    schema_version: 1,
    action: draft.action,
    mode: draft.mode,
    scope,
    allow: clean(draft.allow),
    stop: clean(draft.stop),
    version: draft.version,
  }
}

/**
 * Side panel that creates or edits one action policy. Action and scope identify
 * a policy, so they are fixed once it exists.
 */
export function PolicyEditor({
  draft,
  editing,
  onChange,
  onClose,
  onSave,
  saving,
  saveError,
  onDelete,
  deleting,
  deleteError,
}: {
  draft: PolicyDraft | null
  editing: boolean
  onChange: (draft: PolicyDraft) => void
  onClose: () => void
  onSave: () => void
  saving: boolean
  saveError: string
  onDelete?: () => void
  deleting: boolean
  deleteError: string
}) {
  const set = <K extends keyof PolicyDraft>(key: K, value: PolicyDraft[K]) => draft && onChange({ ...draft, [key]: value })

  return (
    <Modal open={draft !== null} onOpenChange={value => { if (!value) onClose() }} size="xl" position="right">
      {draft && (
        <div className="flex min-h-full flex-col">
          <ModalHeader className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <ModalTitle>{editing ? `Edit the ${draft.action} policy` : 'New policy'}</ModalTitle>
              <ModalDescription>
                {editing
                  ? 'Saving creates a new version. Agents use it from their next decision.'
                  : 'Choose the action, where the policy applies, and who decides.'}
              </ModalDescription>
            </div>
            <Button size="sm" variant="ghost" aria-label="Close" leftIcon={<X className="h-4 w-4" />} onClick={onClose} />
          </ModalHeader>
          <ModalContent className="flex-1 space-y-6">
            <FieldGroup
              title="Action and scope"
              description={editing ? 'Action and scope identify the policy and cannot change. Delete it and create a new one to move it.' : undefined}
            >
              <Field id="policy-action" label="Action" hint={ACTION_DESCRIPTION[draft.action]}>
                <StyledSelect id="policy-action" className={`${SELECT_CLASS} font-mono`} value={draft.action} disabled={editing} onChange={event => set('action', event.target.value as FactoryAction)}>
                  {ACTIONS.map(action => <option key={action} value={action}>{action}</option>)}
                </StyledSelect>
              </Field>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field id="policy-project" label="Project" hint="Leave empty for every project.">
                  <Input id="policy-project" value={draft.project} disabled={editing} placeholder="Any project" onChange={event => set('project', event.target.value)} />
                </Field>
                <Field id="policy-task-class" label="Task class">
                  <StyledSelect id="policy-task-class" className={SELECT_CLASS} value={draft.taskClass} disabled={editing} onChange={event => set('taskClass', event.target.value as FactoryTaskClass | '')}>
                    <option value="">Any task class</option>
                    {TASK_CLASSES.map(taskClass => <option key={taskClass} value={taskClass}>{taskClassLabel(taskClass)}</option>)}
                  </StyledSelect>
                </Field>
              </div>
              {draft.action === 'merge' && draft.project.trim() && (
                <p className="rounded-md border border-status-warning/20 bg-status-warning/[0.08] px-3.5 py-2.5 text-sm text-text-primary">
                  Autonomous reviews do not know which project a pull request belongs to yet. While any merge policy is
                  scoped to a project, the factory holds every autonomous merge in the organization for a person rather
                  than guess which policy applies.
                </p>
              )}
            </FieldGroup>

            <FieldGroup title="Who decides">
              <div role="radiogroup" aria-label="Who decides" className="grid gap-2">
                {MODES.map(mode => (
                  <RadioCard
                    key={mode.value}
                    name="policy-mode"
                    value={mode.value}
                    checked={draft.mode === mode.value}
                    onChange={value => set('mode', value as FactoryPolicyMode)}
                    title={mode.title}
                    description={mode.description}
                  />
                ))}
              </div>
            </FieldGroup>

            <FieldGroup title="Conditions" description="One condition per row. The decision model reads them; safety floors still apply.">
              <ConditionList
                kind="allow"
                title={`Allow when${draft.mode === 'criteria' ? ' (at least one required)' : ''}`}
                items={draft.allow}
                placeholder="Only docs or tests paths change"
                onChange={items => set('allow', items)}
              />
              <ConditionList
                kind="stop"
                title="Stop when"
                items={draft.stop}
                placeholder="The source is an external email"
                onChange={items => set('stop', items)}
              />
            </FieldGroup>

            {saveError && <InlineAlert>{saveError}</InlineAlert>}
            {deleteError && <InlineAlert>{deleteError}</InlineAlert>}
          </ModalContent>
          <ModalFooter className="flex-wrap border-t border-border-primary pt-4">
            {editing && onDelete && (
              <Button
                size="sm"
                variant="destructive"
                className="mr-auto"
                leftIcon={<Trash2 className="h-4 w-4" />}
                aria-label={`Delete ${draft.action} policy`}
                loading={deleting}
                onClick={onDelete}
              >
                Delete
              </Button>
            )}
            <Button size="sm" variant="secondary" onClick={onClose}>Cancel</Button>
            <Button size="sm" loading={saving} onClick={onSave}>Save as v{draft.version}</Button>
          </ModalFooter>
        </div>
      )}
    </Modal>
  )
}

function ConditionList({
  kind,
  title,
  items,
  placeholder,
  onChange,
}: {
  kind: 'allow' | 'stop'
  title: string
  items: string[]
  placeholder: string
  onChange: (items: string[]) => void
}) {
  const noun = kind === 'allow' ? 'Allow' : 'Stop'
  return (
    <div role="group" aria-labelledby={`policy-${kind}-title`} className="space-y-2">
      <p id={`policy-${kind}-title`} className="text-[12px] font-medium text-text-secondary">{title}</p>
      {items.length === 0 && <p className="text-[12px] text-text-tertiary">None.</p>}
      <ul className="m-0 list-none space-y-2 p-0">
        {items.map((item, index) => (
          <li key={index} className="flex items-center gap-2">
            <Input
              aria-label={`${noun} condition ${index + 1}`}
              value={item}
              placeholder={index === 0 ? placeholder : undefined}
              onChange={event => onChange(items.map((current, at) => (at === index ? event.target.value : current)))}
            />
            <Button
              size="sm"
              variant="ghost"
              aria-label={`Remove ${noun.toLowerCase()} condition ${index + 1}`}
              leftIcon={<X className="h-4 w-4" />}
              onClick={() => onChange(items.filter((_, at) => at !== index))}
            />
          </li>
        ))}
      </ul>
      <Button size="sm" variant="secondary" leftIcon={<Plus className="h-4 w-4" />} onClick={() => onChange([...items, ''])}>
        Add {kind === 'allow' ? 'an allow' : 'a stop'} condition
      </Button>
    </div>
  )
}
