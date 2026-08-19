import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Check, ChevronLeft, ChevronRight, Bot, Target, SlidersHorizontal, CalendarClock, ClipboardCheck } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Modal, ModalHeader, ModalTitle, ModalContent, ModalFooter } from '../components/ui/Modal'
import { Button } from '../components/ui/Button'
import { Input, Textarea } from '../components/ui/Input'
import { Switch } from '../components/ui/Switch'
import { Badge } from '../components/ui/Badge'
import type { AutonomousAgentDetail, AutonomousAgentTemplate, AutonomousAgentTemplateKey } from '../types'

/**
 * Guided create/edit wizard for autonomous agents.
 *
 * Structured fields are the single source of truth; the raw configuration is
 * derived from them and shown read-only, with an optional "extra config" merge
 * for power users. Credentials/connectors are intentionally NOT collected here —
 * Slack and GitHub are wired through Claude Code (the `slack` MCP and the
 * server `gh` CLI), so the wizard only asks for the target (what to act on).
 */

type StepId = 'template' | 'target' | 'config' | 'schedule' | 'review'

interface StepMeta { id: StepId; title: string; icon: typeof Bot }

const ALL_STEPS: StepMeta[] = [
  { id: 'template', title: 'Template', icon: Bot },
  { id: 'target', title: 'Target', icon: Target },
  { id: 'config', title: 'Configuration', icon: SlidersHorizontal },
  { id: 'schedule', title: 'Schedule', icon: CalendarClock },
  { id: 'review', title: 'Review', icon: ClipboardCheck },
]

const DEFAULT_TEST_COMMAND = 'npx playwright test'

interface WizardProps {
  open: boolean
  onClose: () => void
  templates: AutonomousAgentTemplate[]
  editing?: AutonomousAgentDetail | null
}

interface FormState {
  name: string
  description: string
  template: AutonomousAgentTemplateKey
  // target
  targetKind: string
  targetName: string
  targetPrimary: string // owner/repo or URL
  // qa config
  outputSlack: boolean
  outputGithubIssue: boolean
  testAdapter: 'playwright' | 'allowlisted_command'
  testCommand: string
  // shared / repo templates
  repository: string
  baseBranch: string
  labels: string
  excludedPaths: string
  includeDrafts: boolean
  // advanced
  extraConfig: string
  budgets: string
  // schedule
  scheduleKind: string
  scheduleExpression: string
  timezone: string
}

function defaultState(template: AutonomousAgentTemplateKey): FormState {
  return {
    name: '',
    description: '',
    template,
    targetKind: template === 'qa' ? 'web_application' : 'repository',
    targetName: '',
    targetPrimary: '',
    outputSlack: false,
    outputGithubIssue: false,
    testAdapter: 'playwright',
    testCommand: DEFAULT_TEST_COMMAND,
    repository: '',
    baseBranch: 'main',
    labels: '',
    excludedPaths: '',
    includeDrafts: false,
    extraConfig: '',
    budgets: '',
    scheduleKind: 'manual',
    scheduleExpression: '06:00',
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
  }
}

function csv(value: string): string[] {
  return value.split(',').map(item => item.trim()).filter(Boolean)
}

/** Assemble the template configuration object from structured fields. */
function buildConfig(state: FormState): Record<string, unknown> {
  let config: Record<string, unknown> = {}
  if (state.template === 'qa') {
    const outputs = ['nexusmind']
    if (state.outputSlack) outputs.push('slack')
    if (state.outputGithubIssue) outputs.push('github_issue')
    config = {
      outputs,
      test_adapter: state.testAdapter,
      test_commands: [csvArgv(state.testCommand)],
    }
    if (state.repository.trim()) config.repository = state.repository.trim()
  } else if (state.template === 'github_issue_resolver') {
    config = { github_auth: 'server_gh_cli', base_branch: state.baseBranch.trim() || 'main' }
    if (state.repository.trim()) config.repository = state.repository.trim()
    if (csv(state.labels).length) config.labels = csv(state.labels)
    if (csv(state.excludedPaths).length) config.excluded_paths = csv(state.excludedPaths)
  } else {
    config = { github_auth: 'server_gh_cli', publish: 'comment_or_request_changes', include_drafts: state.includeDrafts }
    if (state.repository.trim()) config.repository = state.repository.trim()
  }
  const extra = parseJsonObject(state.extraConfig)
  return extra ? { ...config, ...extra } : config
}

/** Split a shell-like command string into argv (whitespace, no shell). */
function csvArgv(command: string): string[] {
  return command.trim().split(/\s+/).filter(Boolean)
}

function parseJsonObject(raw: string): Record<string, unknown> | null {
  if (!raw.trim()) return null
  try {
    const value = JSON.parse(raw)
    return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null
  } catch {
    return null
  }
}

/** Derive structured fields from an existing agent's config (edit mode). */
function stateFromAgent(agent: AutonomousAgentDetail): FormState {
  const base = defaultState(agent.template_key)
  const config = agent.revision.config as Record<string, unknown>
  const outputs = Array.isArray(config.outputs) ? (config.outputs as string[]) : []
  const commands = Array.isArray(config.test_commands) ? (config.test_commands as unknown[]) : []
  const firstCommand = Array.isArray(commands[0]) ? (commands[0] as string[]).join(' ') : ''
  return {
    ...base,
    name: agent.name,
    description: agent.description ?? '',
    outputSlack: outputs.includes('slack'),
    outputGithubIssue: outputs.includes('github_issue'),
    testAdapter: config.test_adapter === 'allowlisted_command' ? 'allowlisted_command' : 'playwright',
    testCommand: firstCommand || DEFAULT_TEST_COMMAND,
    repository: typeof config.repository === 'string' ? config.repository : '',
    baseBranch: typeof config.base_branch === 'string' ? config.base_branch : 'main',
    labels: Array.isArray(config.labels) ? (config.labels as string[]).join(', ') : '',
    excludedPaths: Array.isArray(config.excluded_paths) ? (config.excluded_paths as string[]).join(', ') : '',
    includeDrafts: config.include_drafts === true,
    budgets: JSON.stringify(agent.revision.budgets ?? {}, null, 2),
  }
}

export default function AutonomousAgentWizard({ open, onClose, templates, editing }: WizardProps) {
  const { session } = useAuth()
  const client = useMemo(() => createClient(), [session])
  const queryClient = useQueryClient()
  const isEdit = Boolean(editing)

  const [state, setState] = useState<FormState>(() => defaultState(templates[0]?.key ?? 'qa'))
  const [stepIndex, setStepIndex] = useState(0)
  const [error, setError] = useState('')

  const steps = useMemo(() => (isEdit ? ALL_STEPS.filter(step => step.id !== 'template') : ALL_STEPS), [isEdit])
  const step = steps[stepIndex]
  const selectedTemplate = templates.find(item => item.key === state.template)
  const set = <K extends keyof FormState>(key: K, value: FormState[K]) => setState(prev => ({ ...prev, [key]: value }))

  // (Re)initialise whenever the modal opens or the edit target changes.
  useEffect(() => {
    if (!open) return
    setStepIndex(0)
    setError('')
    if (editing) {
      setState(stateFromAgent(editing))
      // Prefill schedule + target from server (best-effort).
      void client.getAutonomousAgentSchedule(editing.id).then(schedule => {
        setState(prev => ({ ...prev, scheduleKind: schedule.kind, scheduleExpression: schedule.expression ?? prev.scheduleExpression, timezone: schedule.timezone || prev.timezone }))
      }).catch(() => undefined)
      void client.listAutonomousAgentTargets(editing.id).then(targets => {
        const first = targets.find(item => item.enabled) ?? targets[0]
        if (first) setState(prev => ({ ...prev, targetKind: first.kind, targetName: first.name, targetPrimary: primaryFromTargetConfig(first.config) }))
      }).catch(() => undefined)
    } else {
      setState(defaultState(templates[0]?.key ?? 'qa'))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, editing?.id])

  const budgetsError = state.budgets.trim() && !parseJsonObject(state.budgets)
  const extraError = state.extraConfig.trim() && !parseJsonObject(state.extraConfig)

  const submit = useMutation({
    mutationFn: async () => {
      const config = buildConfig(state)
      const budgets = parseJsonObject(state.budgets) ?? selectedTemplate?.default_budgets ?? {}
      const targetConfig = buildTargetConfig(state)
      let agentId: string
      if (editing) {
        const updated = await client.updateAutonomousAgent(editing.id, { name: state.name.trim(), description: state.description.trim() || undefined, config, budgets })
        agentId = updated.id
      } else {
        const created = await client.createAutonomousAgent({ name: state.name.trim(), description: state.description.trim() || undefined, template_key: state.template, config, budgets })
        agentId = created.id
      }
      if (state.targetName.trim()) {
        await client.putAutonomousAgentTarget(agentId, { kind: state.targetKind, name: state.targetName.trim(), config: targetConfig, enabled: true })
      }
      if (state.scheduleKind !== 'manual') {
        await client.putAutonomousAgentSchedule(agentId, { kind: state.scheduleKind, expression: state.scheduleExpression, timezone: state.timezone, misfire_policy: 'run_once', enabled: true })
      } else if (editing) {
        await client.putAutonomousAgentSchedule(agentId, { kind: 'manual', timezone: state.timezone, misfire_policy: 'run_once', enabled: false }).catch(() => undefined)
      }
      return agentId
    },
    onSuccess: () => {
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ['autonomous-agents'] }),
      ])
      onClose()
    },
    onError: value => setError(value instanceof Error ? value.message : 'Could not save the agent'),
  })

  if (!open) return null

  const canContinue = validateStep(step.id, state)
  const isLast = stepIndex === steps.length - 1

  return (
    <Modal open={open} onOpenChange={value => { if (!value) onClose() }} size="xl">
      <ModalHeader>
        <ModalTitle>{isEdit ? `Edit “${editing?.name}”` : 'Create autonomous agent'}</ModalTitle>
        <ol className="mt-4 flex items-center gap-1.5 overflow-x-auto" aria-label="Wizard steps">
          {steps.map((item, index) => {
            const Icon = item.icon
            const active = index === stepIndex
            const done = index < stepIndex
            return (
              <li key={item.id} className="flex items-center gap-1.5 shrink-0">
                <button
                  type="button"
                  onClick={() => index <= stepIndex && setStepIndex(index)}
                  disabled={index > stepIndex}
                  className={`flex items-center gap-2 rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${active ? 'bg-accent-blue/15 text-accent-blue' : done ? 'text-text-secondary hover:text-text-primary' : 'text-text-tertiary'} ${index > stepIndex ? 'cursor-not-allowed' : 'cursor-pointer'}`}
                >
                  <span className={`grid h-5 w-5 place-items-center rounded-full text-[10px] ${active ? 'bg-accent-blue text-white' : done ? 'bg-status-success/20 text-status-success' : 'bg-white/[0.06]'}`}>
                    {done ? <Check className="h-3 w-3" /> : <Icon className="h-3 w-3" />}
                  </span>
                  {item.title}
                </button>
                {index < steps.length - 1 && <ChevronRight className="h-3 w-3 text-text-tertiary" />}
              </li>
            )
          })}
        </ol>
      </ModalHeader>

      <ModalContent className="max-h-[60vh] overflow-y-auto">
        {step.id === 'template' && (
          <StepTemplate templates={templates} value={state.template} onChange={key => setState({ ...defaultState(key) })} />
        )}
        {step.id === 'target' && (
          <StepTarget state={state} set={set} />
        )}
        {step.id === 'config' && (
          <StepConfig state={state} set={set} template={state.template} extraError={Boolean(extraError)} config={buildConfig(state)} />
        )}
        {step.id === 'schedule' && (
          <StepSchedule state={state} set={set} />
        )}
        {step.id === 'review' && (
          <StepReview state={state} set={set} template={selectedTemplate} isEdit={isEdit} budgetsError={Boolean(budgetsError)} />
        )}
      </ModalContent>

      <ModalFooter className="justify-between">
        <div className="text-xs text-status-error min-h-[1rem]" role={error ? 'alert' : undefined}>{error}</div>
        <div className="flex items-center gap-2">
          {stepIndex > 0 && (
            <Button variant="ghost" size="sm" leftIcon={<ChevronLeft className="h-4 w-4" />} onClick={() => setStepIndex(index => index - 1)}>Back</Button>
          )}
          {!isLast && (
            <Button variant="primary" size="sm" rightIcon={<ChevronRight className="h-4 w-4" />} disabled={!canContinue} onClick={() => setStepIndex(index => index + 1)}>Continue</Button>
          )}
          {isLast && (
            <Button variant="primary" size="sm" loading={submit.isPending} disabled={!canContinue || Boolean(budgetsError)} onClick={() => submit.mutate()}>
              {isEdit ? 'Save revision' : 'Create (disabled)'}
            </Button>
          )}
        </div>
      </ModalFooter>
    </Modal>
  )
}

function validateStep(id: StepId, state: FormState): boolean {
  switch (id) {
    case 'template': return Boolean(state.template)
    case 'target': return true // target is optional
    case 'config': return state.template !== 'qa' || csvArgv(state.testCommand).length > 0
    case 'schedule': return state.scheduleKind === 'manual' || state.scheduleExpression.trim().length > 0
    case 'review': return state.name.trim().length > 0
    default: return true
  }
}

function primaryFromTargetConfig(config: Record<string, unknown>): string {
  if (typeof config.url === 'string') return config.url
  if (typeof config.repository === 'string') return config.repository
  return ''
}

function buildTargetConfig(state: FormState): Record<string, unknown> {
  const value = state.targetPrimary.trim()
  if (!value) return {}
  return state.targetKind === 'web_application' ? { url: value } : { repository: value }
}

/* ---------- Steps ---------- */

function StepTemplate({ templates, value, onChange }: { templates: AutonomousAgentTemplate[]; value: AutonomousAgentTemplateKey; onChange: (key: AutonomousAgentTemplateKey) => void }) {
  return (
    <div className="space-y-3">
      <p className="text-[13px] text-text-secondary">Pick a managed template. Each pins its own workflow, capability envelope, and budgets — you configure it, you don't rewrite it.</p>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {templates.map(item => {
          const active = item.key === value
          return (
            <button key={item.key} type="button" onClick={() => onChange(item.key)} className={`text-left rounded-[14px] border p-4 transition-colors ${active ? 'border-accent-blue bg-accent-blue/[0.06]' : 'border-border-primary hover:border-white/20'}`}>
              <div className="flex items-center justify-between">
                <h3 className="font-semibold text-text-primary">{item.name}</h3>
                {active && <Check className="h-4 w-4 text-accent-blue" />}
              </div>
              <p className="mt-1.5 text-xs text-text-tertiary">{item.description}</p>
              <div className="mt-3 flex flex-wrap gap-1">{item.capabilities.slice(0, 4).map(cap => <Badge key={cap} size="sm" variant="default">{cap}</Badge>)}</div>
            </button>
          )
        })}
      </div>
    </div>
  )
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="text-xs font-medium text-text-secondary">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-[11px] text-text-tertiary">{hint}</span>}
    </label>
  )
}

function NativeSelect({ value, onChange, children }: { value: string; onChange: (value: string) => void; children: React.ReactNode }) {
  return (
    <select value={value} onChange={event => onChange(event.target.value)} className="mt-1 block w-full rounded-lg border border-border-primary bg-transparent px-3 py-2 text-sm text-text-primary focus:border-accent-blue focus:outline-none">
      {children}
    </select>
  )
}

function StepTarget({ state, set }: { state: FormState; set: <K extends keyof FormState>(key: K, value: FormState[K]) => void }) {
  const isWeb = state.targetKind === 'web_application'
  return (
    <div className="space-y-4">
      <p className="text-[13px] text-text-secondary">What should this agent act on? The target is optional now and can be added later. No credentials are stored here — Slack and GitHub are configured in Claude Code.</p>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Target type">
          <NativeSelect value={state.targetKind} onChange={value => set('targetKind', value)}>
            <option value="web_application">Web application</option>
            <option value="repository">Repository</option>
            <option value="project">Project</option>
          </NativeSelect>
        </Field>
        <Field label="Target name" hint="A label to recognise this target.">
          <Input inputSize="sm" value={state.targetName} onChange={event => set('targetName', event.target.value)} placeholder={isWeb ? 'Staging web app' : 'app repo'} />
        </Field>
      </div>
      <Field label={isWeb ? 'URL' : 'Repository (owner/repo)'} hint={isWeb ? 'The base URL the QA agent will exercise.' : 'GitHub repository the agent operates on.'}>
        <Input inputSize="sm" value={state.targetPrimary} onChange={event => set('targetPrimary', event.target.value)} placeholder={isWeb ? 'https://staging.example.com' : 'acme/web'} />
      </Field>
    </div>
  )
}

function StepConfig({ state, set, template, extraError, config }: { state: FormState; set: <K extends keyof FormState>(key: K, value: FormState[K]) => void; template: AutonomousAgentTemplateKey; extraError: boolean; config: Record<string, unknown> }) {
  return (
    <div className="space-y-5">
      {template === 'qa' && (
        <>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Test adapter" hint="Playwright runs an E2E suite; allowlisted command runs a pinned argv.">
              <NativeSelect value={state.testAdapter} onChange={value => set('testAdapter', value as FormState['testAdapter'])}>
                <option value="playwright">Playwright</option>
                <option value="allowlisted_command">Allowlisted command</option>
              </NativeSelect>
            </Field>
            <Field label="Test command" hint="Executed as argv (no shell). Default targets Playwright.">
              <Input inputSize="sm" value={state.testCommand} onChange={event => set('testCommand', event.target.value)} placeholder={DEFAULT_TEST_COMMAND} />
            </Field>
          </div>
          <Field label="Repository to check out (owner/repo)" hint="Where the test suite lives. Optional if the target already defines it.">
            <Input inputSize="sm" value={state.repository} onChange={event => set('repository', event.target.value)} placeholder="acme/web" />
          </Field>
          <div className="rounded-[12px] border border-border-primary p-3">
            <p className="text-xs font-medium text-text-secondary">Outputs</p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">NexusMind is always the canonical output. Slack/GitHub delivery uses the server-side integrations in Claude Code.</p>
            <div className="mt-3 space-y-2.5">
              <Switch checked disabled size="sm" label="NexusMind (canonical)" />
              <Switch checked={state.outputSlack} onCheckedChange={value => set('outputSlack', value)} size="sm" label="Slack summary" />
              <Switch checked={state.outputGithubIssue} onCheckedChange={value => set('outputGithubIssue', value)} size="sm" label="GitHub issue" />
            </div>
          </div>
        </>
      )}

      {template === 'github_issue_resolver' && (
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Repository (owner/repo)"><Input inputSize="sm" value={state.repository} onChange={event => set('repository', event.target.value)} placeholder="acme/web" /></Field>
          <Field label="Base branch"><Input inputSize="sm" value={state.baseBranch} onChange={event => set('baseBranch', event.target.value)} placeholder="main" /></Field>
          <Field label="Eligible labels" hint="Comma-separated."><Input inputSize="sm" value={state.labels} onChange={event => set('labels', event.target.value)} placeholder="autofix, good-first-issue" /></Field>
          <Field label="Excluded paths" hint="Comma-separated globs."><Input inputSize="sm" value={state.excludedPaths} onChange={event => set('excludedPaths', event.target.value)} placeholder="infra/**, .github/**" /></Field>
        </div>
      )}

      {template === 'github_pr_reviewer' && (
        <div className="space-y-4">
          <Field label="Repository (owner/repo)"><Input inputSize="sm" value={state.repository} onChange={event => set('repository', event.target.value)} placeholder="acme/web" /></Field>
          <Switch checked={state.includeDrafts} onCheckedChange={value => set('includeDrafts', value)} size="sm" label="Review draft pull requests" />
          <p className="text-[11px] text-text-tertiary">Publishes COMMENT or REQUEST_CHANGES only — never approves, merges, or pushes.</p>
        </div>
      )}

      <details className="rounded-[12px] border border-border-primary p-3">
        <summary className="cursor-pointer text-xs font-medium text-text-secondary">Advanced — extra configuration (merged JSON)</summary>
        <Textarea
          className="mt-2 font-mono text-xs"
          rows={4}
          value={state.extraConfig}
          onChange={event => set('extraConfig', event.target.value)}
          placeholder='{"test_timeout_seconds": 900}'
          error={extraError ? 'Must be a JSON object' : undefined}
        />
        <p className="mt-2 text-[11px] text-text-tertiary">Resulting configuration:</p>
        <pre className="mt-1 max-h-40 overflow-auto rounded-lg bg-black/30 p-2 font-mono text-[11px] text-text-tertiary">{JSON.stringify(config, null, 2)}</pre>
      </details>
    </div>
  )
}

function StepSchedule({ state, set }: { state: FormState; set: <K extends keyof FormState>(key: K, value: FormState[K]) => void }) {
  const manual = state.scheduleKind === 'manual'
  return (
    <div className="space-y-4">
      <p className="text-[13px] text-text-secondary">When should this agent run? You can always trigger runs manually regardless of the schedule.</p>
      <div className="grid gap-4 sm:grid-cols-3">
        <Field label="Cadence">
          <NativeSelect value={state.scheduleKind} onChange={value => set('scheduleKind', value)}>
            <option value="manual">Manual only</option>
            <option value="daily">Daily</option>
            <option value="interval">Interval</option>
          </NativeSelect>
        </Field>
        <Field label={state.scheduleKind === 'interval' ? 'Every (minutes)' : 'Time (HH:MM)'}>
          <Input inputSize="sm" disabled={manual} value={state.scheduleExpression} onChange={event => set('scheduleExpression', event.target.value)} placeholder={state.scheduleKind === 'interval' ? '60' : '06:00'} />
        </Field>
        <Field label="Timezone">
          <Input inputSize="sm" disabled={manual} value={state.timezone} onChange={event => set('timezone', event.target.value)} />
        </Field>
      </div>
    </div>
  )
}

function StepReview({ state, set, template, isEdit, budgetsError }: { state: FormState; set: <K extends keyof FormState>(key: K, value: FormState[K]) => void; template?: AutonomousAgentTemplate; isEdit: boolean; budgetsError: boolean }) {
  return (
    <div className="space-y-4">
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Agent name" hint="Required.">
          <Input inputSize="sm" autoFocus value={state.name} onChange={event => set('name', event.target.value)} placeholder="Nightly QA" />
        </Field>
        <Field label="Description">
          <Input inputSize="sm" value={state.description} onChange={event => set('description', event.target.value)} placeholder="Optional" />
        </Field>
      </div>
      <Field label="Budgets (JSON)" hint="Defaults come from the template; adjust wall-time, attempts, cost, concurrency.">
        <Textarea className="font-mono text-xs" rows={5} value={state.budgets || JSON.stringify(template?.default_budgets ?? {}, null, 2)} onChange={event => set('budgets', event.target.value)} error={budgetsError ? 'Must be a JSON object' : undefined} />
      </Field>
      {template && (
        <div className="rounded-[12px] border border-border-primary bg-white/[0.02] p-3 text-[12px] text-text-secondary">
          <p className="font-medium text-text-primary">Authority envelope</p>
          <div className="mt-2 flex flex-wrap gap-1">{template.capabilities.map(cap => <Badge key={cap} size="sm" variant="info">{cap}</Badge>)}</div>
        </div>
      )}
      <div className="flex items-start gap-2 rounded-[12px] border border-status-warning/30 bg-status-warning/[0.06] p-3 text-[12px] text-status-warning">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
        <p>{isEdit ? 'Saving creates a new revision and disables the agent. Validate and enable it again to resume runs.' : 'Creation always saves the agent disabled. Validate and enable it explicitly after reviewing this configuration.'}</p>
      </div>
    </div>
  )
}
