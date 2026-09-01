import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
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
  assignIssuesToSelf: boolean
  // pr reviewer auto-merge
  autoMerge: boolean
  // agent-to-agent chaining
  onSuccessTriggerAgentId: string
  onSuccessTriggerDelaySeconds: string
  testAdapter: 'playwright' | 'allowlisted_command'
  testCommand: string
  qaInstructions: string
  customInstructions: string
  // lead generation
  product: string
  icp: string
  leadCount: string
  // ai content manager (LinkedIn)
  topics: string // comma-separated
  audience: string
  contentLanguage: string
  tone: string
  cta: string
  hashtags: string // comma-separated
  postsPerRun: string
  destPersonal: boolean
  destOrganization: boolean
  // judge
  repositories: string // comma-separated owner/repo the judge may target
  publishComment: boolean
  // login credentials (qa/judge — used to authenticate into the live app)
  loginUser: string
  loginPassword: string
  loginUrl: string
  existingCredentialConnectorId: string
  // shared / repo templates
  repository: string
  baseBranch: string
  contextRepos: string
  labels: string
  excludedPaths: string
  includeDrafts: boolean
  // issue-resolver preview-review handoff
  reviewAfterDeploy: boolean
  judgeAgentId: string
  // advanced
  extraConfig: string
  budgets: string
  // schedule
  scheduleKind: string
  scheduleExpression: string
  scheduleUnit: 'minutes' | 'hours' | 'days'
  timezone: string
}

function defaultState(template: AutonomousAgentTemplateKey): FormState {
  return {
    name: '',
    description: '',
    template,
    targetKind: template === 'qa' || template === 'judge' ? 'web_application' : template === 'lead_generation' || template === 'ai_content_manager' ? 'none' : 'repository',
    targetName: '',
    targetPrimary: '',
    outputSlack: false,
    outputGithubIssue: false,
    assignIssuesToSelf: false,
    autoMerge: false,
    onSuccessTriggerAgentId: '',
    onSuccessTriggerDelaySeconds: '',
    testAdapter: 'playwright',
    testCommand: '',
    qaInstructions: '',
    customInstructions: '',
    product: '',
    icp: '',
    leadCount: '10',
    topics: '',
    audience: '',
    contentLanguage: 'English',
    tone: '',
    cta: '',
    hashtags: '',
    postsPerRun: '3',
    destPersonal: true,
    destOrganization: false,
    repositories: '',
    publishComment: false,
    loginUser: '',
    loginPassword: '',
    loginUrl: '',
    existingCredentialConnectorId: '',
    repository: '',
    baseBranch: 'main',
    contextRepos: '',
    labels: '',
    excludedPaths: '',
    includeDrafts: false,
    reviewAfterDeploy: false,
    judgeAgentId: '',
    extraConfig: '',
    budgets: '',
    scheduleKind: 'manual',
    scheduleExpression: '06:00',
    scheduleUnit: 'hours',
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
    config = { outputs, test_adapter: state.testAdapter }
    if (state.testAdapter === 'allowlisted_command') {
      // Deterministic mode: the worker runs this argv as a subprocess.
      config.test_commands = [csvArgv(state.testCommand)]
    } else if (state.qaInstructions.trim()) {
      // Agent-driven mode: Claude drives the browser via the Playwright MCP;
      // these instructions tell it what to verify (no shell command needed).
      config.qa_instructions = state.qaInstructions.trim()
    }
    if (state.repository.trim()) config.repository = state.repository.trim()
    if (state.assignIssuesToSelf) config.assign_issues_to_self = true
  } else if (state.template === 'github_issue_resolver') {
    config = { github_auth: 'server_gh_cli', base_branch: state.baseBranch.trim() || 'main' }
    if (state.repository.trim()) config.repository = state.repository.trim()
    if (csv(state.contextRepos).length) config.context_repos = csv(state.contextRepos)
    if (csv(state.labels).length) config.labels = csv(state.labels)
    if (csv(state.excludedPaths).length) config.excluded_paths = csv(state.excludedPaths)
    if (state.customInstructions.trim()) config.custom_instructions = state.customInstructions.trim()
    if (state.reviewAfterDeploy) { config.review_after_deploy = true; config.judge_agent_id = state.judgeAgentId }
  } else if (state.template === 'lead_generation') {
    const outputs = ['nexusmind']
    if (state.outputSlack) outputs.push('slack')
    config = { outputs, product: state.product.trim(), icp: state.icp.trim(), count: Math.max(1, Math.min(25, Number(state.leadCount) || 10)) }
    if (state.customInstructions.trim()) config.custom_instructions = state.customInstructions.trim()
  } else if (state.template === 'judge') {
    const outputs = ['nexusmind']
    if (state.outputSlack) outputs.push('slack')
    config = {
      github_auth: 'server_gh_cli',
      outputs,
      repositories: csv(state.repositories),
      publish: state.publishComment ? 'comment' : 'none',
    }
    if (state.customInstructions.trim()) config.custom_instructions = state.customInstructions.trim()
  } else if (state.template === 'ai_content_manager') {
    const outputs = ['nexusmind']
    if (state.outputSlack) outputs.push('slack')
    const destinations: string[] = []
    if (state.destPersonal) destinations.push('personal')
    if (state.destOrganization) destinations.push('organization')
    config = {
      outputs,
      topics: csv(state.topics),
      audience: state.audience.trim(),
      language: state.contentLanguage.trim() || 'English',
      posts_per_run: Math.max(1, Math.min(10, Number(state.postsPerRun) || 3)),
      destinations,
    }
    if (state.tone.trim()) config.tone = state.tone.trim()
    if (state.cta.trim()) config.cta = state.cta.trim()
    if (csv(state.hashtags).length) config.hashtags = csv(state.hashtags)
    if (state.customInstructions.trim()) config.custom_instructions = state.customInstructions.trim()
  } else {
    config = { github_auth: 'server_gh_cli', publish: 'comment_or_request_changes', include_drafts: state.includeDrafts }
    if (state.repository.trim()) config.repository = state.repository.trim()
    if (state.customInstructions.trim()) config.custom_instructions = state.customInstructions.trim()
    if (state.autoMerge) config.auto_merge = true
  }
  // Agent-to-agent chaining: enqueue the next agent on the same PR on success.
  if (['github_issue_resolver', 'github_pr_reviewer', 'judge'].includes(state.template) && state.onSuccessTriggerAgentId.trim()) {
    config.on_success_trigger_agent_id = state.onSuccessTriggerAgentId.trim()
    if (state.onSuccessTriggerDelaySeconds.trim()) config.on_success_trigger_delay_seconds = Math.max(0, Number(state.onSuccessTriggerDelaySeconds) || 0)
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
    testCommand: firstCommand,
    qaInstructions: typeof config.qa_instructions === 'string' ? config.qa_instructions : '',
    customInstructions: typeof config.custom_instructions === 'string' ? config.custom_instructions : '',
    product: typeof config.product === 'string' ? config.product : '',
    icp: typeof config.icp === 'string' ? config.icp : '',
    leadCount: typeof config.count === 'number' ? String(config.count) : '10',
    repositories: Array.isArray(config.repositories) ? (config.repositories as string[]).join(', ') : '',
    publishComment: config.publish === 'comment',
    repository: typeof config.repository === 'string' ? config.repository : '',
    baseBranch: typeof config.base_branch === 'string' ? config.base_branch : 'main',
    contextRepos: Array.isArray(config.context_repos) ? (config.context_repos as string[]).join(', ') : '',
    labels: Array.isArray(config.labels) ? (config.labels as string[]).join(', ') : '',
    excludedPaths: Array.isArray(config.excluded_paths) ? (config.excluded_paths as string[]).join(', ') : '',
    includeDrafts: config.include_drafts === true,
    reviewAfterDeploy: config.review_after_deploy === true,
    judgeAgentId: typeof config.judge_agent_id === 'string' ? config.judge_agent_id : '',
    assignIssuesToSelf: config.assign_issues_to_self === true,
    autoMerge: config.auto_merge === true,
    onSuccessTriggerAgentId: typeof config.on_success_trigger_agent_id === 'string' ? config.on_success_trigger_agent_id : '',
    onSuccessTriggerDelaySeconds: typeof config.on_success_trigger_delay_seconds === 'number' ? String(config.on_success_trigger_delay_seconds) : '',
    topics: Array.isArray(config.topics) ? (config.topics as string[]).join(', ') : '',
    audience: typeof config.audience === 'string' ? config.audience : '',
    contentLanguage: typeof config.language === 'string' ? config.language : 'English',
    tone: typeof config.tone === 'string' ? config.tone : '',
    cta: typeof config.cta === 'string' ? config.cta : '',
    hashtags: Array.isArray(config.hashtags) ? (config.hashtags as string[]).join(', ') : '',
    postsPerRun: typeof config.posts_per_run === 'number' ? String(config.posts_per_run) : '3',
    destPersonal: Array.isArray(config.destinations) ? (config.destinations as string[]).includes('personal') : true,
    destOrganization: Array.isArray(config.destinations) ? (config.destinations as string[]).includes('organization') : false,
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
        const iv = schedule.kind === 'interval' ? minutesToInterval(Number(schedule.expression) || 0) : null
        setState(prev => ({ ...prev, scheduleKind: schedule.kind, scheduleExpression: iv ? iv.value : (schedule.expression ?? prev.scheduleExpression), scheduleUnit: iv ? iv.unit : prev.scheduleUnit, timezone: schedule.timezone || prev.timezone }))
      }).catch(() => undefined)
      void client.listAutonomousAgentTargets(editing.id).then(targets => {
        const first = targets.find(item => item.enabled) ?? targets[0]
        if (first) setState(prev => ({ ...prev, targetKind: first.kind, targetName: first.name, targetPrimary: primaryFromTargetConfig(first.config), existingCredentialConnectorId: first.credential_connector_id ?? '' }))
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
      // Login credentials are stored in an encrypted `target_secret` connector and
      // bound to the target, so the agent can authenticate into the live app. On
      // edit we preserve the existing binding unless a new password is entered.
      let credentialConnectorId: string | undefined = state.existingCredentialConnectorId || undefined
      if ((state.template === 'judge' || state.template === 'qa') && state.loginUser.trim() && state.loginPassword.trim()) {
        const secret = JSON.stringify({ USERNAME: state.loginUser.trim(), PASSWORD: state.loginPassword.trim() })
        const connector = await client.putAutonomousAgentConnector({ kind: 'target_secret', name: `login:${agentId}`, secret, metadata: {}, scopes: ['target:use'] })
        credentialConnectorId = connector.id
      }
      if (state.targetName.trim() || state.targetPrimary.trim()) {
        await client.putAutonomousAgentTarget(agentId, { kind: state.targetKind, name: state.targetName.trim() || 'Target', config: targetConfig, credential_connector_id: credentialConnectorId, enabled: true })
      }
      if (state.scheduleKind !== 'manual') {
        const expression = state.scheduleKind === 'interval'
          ? String(intervalToMinutes(state.scheduleExpression, state.scheduleUnit))
          : state.scheduleExpression
        await client.putAutonomousAgentSchedule(agentId, { kind: state.scheduleKind, expression, timezone: state.timezone, misfire_policy: 'run_once', enabled: true })
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
    case 'config':
      if (state.template === 'qa') return state.testAdapter === 'playwright' || csvArgv(state.testCommand).length > 0
      if (state.template === 'lead_generation') return state.product.trim().length > 0 && state.icp.trim().length > 0
      if (state.template === 'judge') return csv(state.repositories).length > 0
      if (state.template === 'ai_content_manager') return csv(state.topics).length > 0 && state.audience.trim().length > 0
      if (state.template === 'github_issue_resolver' && state.reviewAfterDeploy) return state.judgeAgentId.trim().length > 0
      return true
    case 'schedule':
      if (state.scheduleKind === 'manual') return true
      if (state.scheduleKind === 'interval') return intervalToMinutes(state.scheduleExpression, state.scheduleUnit) >= 15
      return state.scheduleExpression.trim().length > 0
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
  if (state.targetKind === 'web_application') {
    const config: Record<string, unknown> = { url: value }
    if (state.loginUrl.trim()) config.login_url = state.loginUrl.trim()
    return config
  }
  return { repository: value }
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
  if (state.template === 'lead_generation') {
    return <p className="text-[13px] text-text-secondary">This agent has no fixed target — it discovers companies from the web based on the product and ICP you set in the next step. Nothing to configure here.</p>
  }
  if (state.template === 'ai_content_manager') {
    return <p className="text-[13px] text-text-secondary">This agent has no fixed target — it writes LinkedIn posts from the topics and audience you set in the next step. Nothing to configure here.</p>
  }
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
  const client = useMemo(() => createClient(), [])
  const judgesQuery = useQuery({ queryKey: ['wizard-agents'], queryFn: () => client.listAutonomousAgents(), enabled: ['github_issue_resolver', 'github_pr_reviewer', 'judge'].includes(template) })
  const judges = (judgesQuery.data ?? []).filter(agent => agent.template_key === 'judge')
  const chainAgents = (judgesQuery.data ?? []).filter(agent => agent.status === 'enabled')
  const linkedinQuery = useQuery({ queryKey: ['wizard-linkedin'], queryFn: () => client.listLinkedinConnections(), enabled: template === 'ai_content_manager' })
  const linkedinConnected = new Set((linkedinQuery.data ?? []).map(connection => connection.destination))
  const connectLinkedin = async (destination: 'personal' | 'organization') => {
    try {
      const { url } = await client.linkedinAuthorize(destination)
      if (url) window.open(url, '_blank', 'width=600,height=760')
    } catch (err) {
      window.alert(`LinkedIn is not configured on the server yet: ${(err as { message?: string })?.message ?? 'error'}`)
    }
  }
  return (
    <div className="space-y-5">
      {template === 'qa' && (
        <>
          <Field label="Test adapter" hint="Playwright: the agent drives the browser via the Playwright MCP — no command needed. Allowlisted command: the worker runs a pinned argv.">
            <NativeSelect value={state.testAdapter} onChange={value => set('testAdapter', value as FormState['testAdapter'])}>
              <option value="playwright">Playwright (agent-driven)</option>
              <option value="allowlisted_command">Allowlisted command</option>
            </NativeSelect>
          </Field>
          {state.testAdapter === 'playwright' ? (
            <Field label="What should the agent test? (optional)" hint="Claude drives the browser against the target and reports findings. Leave notes on flows or checks to prioritise; blank means general exploratory QA.">
              <Textarea className="text-sm" rows={3} value={state.qaInstructions} onChange={event => set('qaInstructions', event.target.value)} placeholder="e.g. Sign in, add an item to the cart, and verify checkout totals." />
            </Field>
          ) : (
            <Field label="Test command" hint="Executed as argv (no shell), e.g. npx playwright test.">
              <Input inputSize="sm" value={state.testCommand} onChange={event => set('testCommand', event.target.value)} placeholder={DEFAULT_TEST_COMMAND} />
            </Field>
          )}
          <Field label="Repository to check out (owner/repo)" hint="Optional. Needed if the agent should inspect or run the codebase; the target URL is enough for pure browser QA.">
            <Input inputSize="sm" value={state.repository} onChange={event => set('repository', event.target.value)} placeholder="acme/web" />
          </Field>
          <div className="rounded-[12px] border border-border-primary p-3">
            <p className="text-xs font-medium text-text-secondary">Outputs</p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">NexusMind is always the canonical output. Slack/GitHub delivery uses the server-side integrations in Claude Code.</p>
            <div className="mt-3 space-y-2.5">
              <Switch checked disabled size="sm" label="NexusMind (canonical)" />
              <Switch checked={state.outputSlack} onCheckedChange={value => set('outputSlack', value)} size="sm" label="Slack summary" />
              <Switch checked={state.outputGithubIssue} onCheckedChange={value => set('outputGithubIssue', value)} size="sm" label="GitHub issue" />
              {state.outputGithubIssue && <Switch checked={state.assignIssuesToSelf} onCheckedChange={value => set('assignIssuesToSelf', value)} size="sm" label="Assign created issues to the logged-in gh account" />}
            </div>
          </div>
        </>
      )}

      {template === 'github_issue_resolver' && (
        <div className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Repository (owner/repo)"><Input inputSize="sm" value={state.repository} onChange={event => set('repository', event.target.value)} placeholder="acme/web" /></Field>
            <Field label="Base branch"><Input inputSize="sm" value={state.baseBranch} onChange={event => set('baseBranch', event.target.value)} placeholder="main" /></Field>
            <Field label="Context repositories" hint="Comma-separated owner/repo. Cloned read-only so the agent has cross-repo context; changes still land only in the primary repo."><Input inputSize="sm" value={state.contextRepos} onChange={event => set('contextRepos', event.target.value)} placeholder="acme/api, acme/shared-lib" /></Field>
            <Field label="Eligible labels" hint="Comma-separated."><Input inputSize="sm" value={state.labels} onChange={event => set('labels', event.target.value)} placeholder="autofix, good-first-issue" /></Field>
            <Field label="Excluded paths" hint="Comma-separated globs."><Input inputSize="sm" value={state.excludedPaths} onChange={event => set('excludedPaths', event.target.value)} placeholder="infra/**, .github/**" /></Field>
          </div>
          <Field label="Custom instructions (optional)" hint="Guidance for how the agent should approach the issue — priorities, conventions, gotchas. Cannot expand its scope or lift safety limits.">
            <Textarea className="text-sm" rows={3} value={state.customInstructions} onChange={event => set('customInstructions', event.target.value)} placeholder="e.g. Prefer the existing repository pattern in api/; keep the change minimal and add a test." />
          </Field>
          <div className="rounded-[12px] border border-border-primary p-3 space-y-3">
            <Switch checked={state.reviewAfterDeploy} onCheckedChange={value => set('reviewAfterDeploy', value)} size="sm" label="Review the PR with a Judge after deploy" />
            <p className="text-[11px] text-text-tertiary">When on, each opened PR is marked (label + body marker) so your deploy workflow can deploy that branch to a preview and then call the Judge to verify the running app and post visual evidence on the PR. The Judge produces the visual evidence; the resolver already includes its own verification output.</p>
            {state.reviewAfterDeploy && (
              <Field label="Judge agent" hint="Reviews the deployed preview. Create a Judge agent first if the list is empty.">
                <NativeSelect value={state.judgeAgentId} onChange={value => set('judgeAgentId', value)}>
                  <option value="">Select a Judge…</option>
                  {judges.map(judge => <option key={judge.id} value={judge.id}>{judge.name}</option>)}
                </NativeSelect>
              </Field>
            )}
          </div>
        </div>
      )}

      {template === 'github_pr_reviewer' && (
        <div className="space-y-4">
          <Field label="Repository (owner/repo)"><Input inputSize="sm" value={state.repository} onChange={event => set('repository', event.target.value)} placeholder="acme/web" /></Field>
          <Switch checked={state.includeDrafts} onCheckedChange={value => set('includeDrafts', value)} size="sm" label="Review draft pull requests" />
          <Field label="Custom instructions (optional)" hint="Guidance for what the review should focus on — areas, standards, risks. Cannot approve, merge, push, or publish.">
            <Textarea className="text-sm" rows={3} value={state.customInstructions} onChange={event => set('customInstructions', event.target.value)} placeholder="e.g. Focus on error handling and auth checks; flag any missing input validation." />
          </Field>
          <div className="rounded-[12px] border border-border-primary p-3 space-y-2">
            <Switch checked={state.autoMerge} onCheckedChange={value => set('autoMerge', value)} size="sm" label="Auto-merge if the review is clean" />
            <p className="text-[11px] text-text-tertiary">When on, squash-merges the PR (keeping the branch) ONLY if the review found no blocking issues AND every required GitHub check is green. If there are no checks to verify, it does not merge.</p>
          </div>
        </div>
      )}

      {template === 'lead_generation' && (
        <div className="space-y-4">
          <Field label="Product" hint="What you're selling — name plus a one-line value prop or a URL.">
            <Textarea className="text-sm" rows={2} value={state.product} onChange={event => set('product', event.target.value)} placeholder="NexusMind — persistent team memory for AI coding agents (nexusmind.smartcoderlabs.com)" />
          </Field>
          <Field label="Ideal customer profile (ICP)" hint="Who to target: industry, company size, role, geography.">
            <Textarea className="text-sm" rows={3} value={state.icp} onChange={event => set('icp', event.target.value)} placeholder="Software consultancies and dev-tool startups (10–200 people) using Claude Code / AI agents, in LATAM and the US." />
          </Field>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Leads per run" hint="1–25.">
              <Input inputSize="sm" type="number" min={1} max={25} value={state.leadCount} onChange={event => set('leadCount', event.target.value)} placeholder="10" />
            </Field>
          </div>
          <Field label="Custom instructions (optional)" hint="Tone, angle, what to emphasize or avoid in the drafts.">
            <Textarea className="text-sm" rows={2} value={state.customInstructions} onChange={event => set('customInstructions', event.target.value)} placeholder="e.g. Keep emails under 90 words, lead with a specific pain point, no buzzwords." />
          </Field>
          <div className="rounded-[12px] border border-border-primary p-3">
            <p className="text-xs font-medium text-text-secondary">Outputs</p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">Leads and drafted emails are stored in NexusMind for your review. This agent never sends email.</p>
            <div className="mt-3 space-y-2.5">
              <Switch checked disabled size="sm" label="NexusMind (canonical)" />
              <Switch checked={state.outputSlack} onCheckedChange={value => set('outputSlack', value)} size="sm" label="Slack summary" />
            </div>
          </div>
        </div>
      )}

      {template === 'judge' && (
        <div className="space-y-4">
          <Field label="Repositories (owner/repo, comma-separated)" hint="The repos this judge may target. The specific PRs/issues are chosen each time you run it. Set the live app URL in the Target step.">
            <Input inputSize="sm" value={state.repositories} onChange={event => set('repositories', event.target.value)} placeholder="acme/web, acme/api" />
          </Field>
          <Field label="Custom instructions (optional)" hint="What to prioritize while verifying. Cannot expand scope beyond what the PRs/issues touch.">
            <Textarea className="text-sm" rows={2} value={state.customInstructions} onChange={event => set('customInstructions', event.target.value)} placeholder="e.g. Pay special attention to the checkout totals and the empty-cart state." />
          </Field>
          <div className="rounded-[12px] border border-border-primary p-3">
            <p className="text-xs font-medium text-text-secondary">Verdict delivery</p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">NexusMind always records findings with evidence. Publishing a verdict comment on each PR/issue is opt-in.</p>
            <div className="mt-3 space-y-2.5">
              <Switch checked disabled size="sm" label="NexusMind (canonical)" />
              <Switch checked={state.outputSlack} onCheckedChange={value => set('outputSlack', value)} size="sm" label="Slack summary" />
              <Switch checked={state.publishComment} onCheckedChange={value => set('publishComment', value)} size="sm" label="Comment the verdict on GitHub" />
            </div>
          </div>
          <p className="text-[11px] text-text-tertiary">Verifies only what the PRs/issues touch against the live app — never approves, merges, or pushes.</p>
        </div>
      )}

      {template === 'ai_content_manager' && (
        <div className="space-y-4">
          <Field label="Topics (comma-separated)" hint="The themes/areas the agent writes about.">
            <Textarea className="text-sm" rows={2} value={state.topics} onChange={event => set('topics', event.target.value)} placeholder="AI coding agents, developer productivity, persistent memory for LLMs, team knowledge" />
          </Field>
          <Field label="Target audience / ICP" hint="Who the content should speak to and capture as leads.">
            <Textarea className="text-sm" rows={2} value={state.audience} onChange={event => set('audience', event.target.value)} placeholder="CTOs and lead engineers at software consultancies and dev-tool startups adopting AI agents." />
          </Field>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Language"><Input inputSize="sm" value={state.contentLanguage} onChange={event => set('contentLanguage', event.target.value)} placeholder="English" /></Field>
            <Field label="Posts per run" hint="1–10."><Input inputSize="sm" type="number" min={1} max={10} value={state.postsPerRun} onChange={event => set('postsPerRun', event.target.value)} placeholder="3" /></Field>
          </div>
          <Field label="Brand tone / voice (optional)"><Input inputSize="sm" value={state.tone} onChange={event => set('tone', event.target.value)} placeholder="practical, bold, friendly-expert" /></Field>
          <Field label="Call to action / lead magnet (optional)" hint="Woven in naturally at the end of posts.">
            <Input inputSize="sm" value={state.cta} onChange={event => set('cta', event.target.value)} placeholder="Try NexusMind free → nexusmind.smartcoderlabs.com" />
          </Field>
          <Field label="Preferred hashtags (optional, comma-separated)"><Input inputSize="sm" value={state.hashtags} onChange={event => set('hashtags', event.target.value)} placeholder="#AI, #DevTools, #Startups" /></Field>
          <Field label="Custom instructions (optional)" hint="Anything else to emphasize or avoid.">
            <Textarea className="text-sm" rows={2} value={state.customInstructions} onChange={event => set('customInstructions', event.target.value)} placeholder="e.g. Prefer short posts; open with a contrarian take; no emojis." />
          </Field>
          <div className="rounded-[12px] border border-border-primary p-3">
            <p className="text-xs font-medium text-text-secondary">Intended destinations</p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">Which LinkedIn destinations these posts are for. Nothing is published automatically — you approve each post before it goes out.</p>
            <div className="mt-3 space-y-2.5">
              <Switch checked={state.destPersonal} onCheckedChange={value => set('destPersonal', value)} size="sm" label="Personal profile" />
              <Switch checked={state.destOrganization} onCheckedChange={value => set('destOrganization', value)} size="sm" label="Company page" />
            </div>
          </div>
          <div className="rounded-[12px] border border-border-primary p-3">
            <div className="flex items-center justify-between gap-2">
              <p className="text-xs font-medium text-text-secondary">Connect LinkedIn</p>
              <button type="button" onClick={() => linkedinQuery.refetch()} className="text-[11px] text-accent-blue">Refresh</button>
            </div>
            <p className="mt-0.5 text-[11px] text-text-tertiary">Connect the account(s) once (shared by all content agents). A window opens to authorize on LinkedIn; approved posts publish from the Findings tab.</p>
            <div className="mt-3 flex flex-wrap gap-2">
              <Button size="sm" variant={linkedinConnected.has('personal') ? 'ghost' : 'secondary'} onClick={() => connectLinkedin('personal')}>{linkedinConnected.has('personal') ? '✓ Personal connected — reconnect' : 'Connect personal'}</Button>
              <Button size="sm" variant={linkedinConnected.has('organization') ? 'ghost' : 'secondary'} onClick={() => connectLinkedin('organization')}>{linkedinConnected.has('organization') ? '✓ Company connected — reconnect' : 'Connect company page'}</Button>
            </div>
            <p className="mt-2 text-[11px] text-text-tertiary">Requires the server's LinkedIn app to be configured (LINKEDIN_CLIENT_ID/SECRET + redirect URL).</p>
          </div>
          <div className="rounded-[12px] border border-border-primary p-3">
            <p className="text-xs font-medium text-text-secondary">Outputs</p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">Generated post drafts are stored in NexusMind for your review. This agent never posts to LinkedIn on its own.</p>
            <div className="mt-3 space-y-2.5">
              <Switch checked disabled size="sm" label="NexusMind (canonical)" />
              <Switch checked={state.outputSlack} onCheckedChange={value => set('outputSlack', value)} size="sm" label="Slack summary" />
            </div>
          </div>
        </div>
      )}

      {['github_issue_resolver', 'github_pr_reviewer', 'judge'].includes(template) && (
        <div className="rounded-[12px] border border-border-primary p-3 space-y-3">
          <p className="text-xs font-medium text-text-secondary">Chain — run another agent on success</p>
          <p className="text-[11px] text-text-tertiary">When this agent finishes successfully on a PR, enqueue the chosen agent on the SAME PR. E.g. Resolver → PR Reviewer → Judge, so the agents hand off to each other.</p>
          <Field label="Next agent (optional)">
            <NativeSelect value={state.onSuccessTriggerAgentId} onChange={value => set('onSuccessTriggerAgentId', value)}>
              <option value="">None</option>
              {chainAgents.map(agent => <option key={agent.id} value={agent.id}>{agent.name} ({agent.template_key})</option>)}
            </NativeSelect>
          </Field>
          {state.onSuccessTriggerAgentId && (
            <Field label="Delay before the next run (seconds, optional)" hint="Useful when the next agent (e.g. a Judge) must wait for your deploy to reach production after a merge.">
              <Input inputSize="sm" type="number" min={0} value={state.onSuccessTriggerDelaySeconds} onChange={event => set('onSuccessTriggerDelaySeconds', event.target.value)} placeholder="0" />
            </Field>
          )}
        </div>
      )}

      {(template === 'judge' || template === 'qa') && (
        <div className="rounded-[12px] border border-border-primary p-3 space-y-3">
          <div>
            <p className="text-xs font-medium text-text-secondary">App login</p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">If the app requires sign-in, provide credentials so the agent logs in before testing — without them it hits the login gate and can't verify the real UI. Stored encrypted and never shown again; leave blank to keep the current login when editing.</p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Username / email">
              <Input inputSize="sm" autoComplete="off" value={state.loginUser} onChange={event => set('loginUser', event.target.value)} placeholder="qa@example.com" />
            </Field>
            <Field label="Password">
              <Input inputSize="sm" type="password" autoComplete="new-password" value={state.loginPassword} onChange={event => set('loginPassword', event.target.value)} placeholder="••••••••" />
            </Field>
          </div>
          <Field label="Login URL (optional)" hint="Only if the login page isn't reachable from the app URL.">
            <Input inputSize="sm" value={state.loginUrl} onChange={event => set('loginUrl', event.target.value)} placeholder="https://app.example.com/login" />
          </Field>
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

const UNIT_MINUTES: Record<string, number> = { minutes: 1, hours: 60, days: 1440 }
/** Convert an interval (value + unit) to the minutes the backend stores. */
function intervalToMinutes(value: string, unit: string): number {
  const n = Number(value)
  return Number.isFinite(n) ? Math.round(n * (UNIT_MINUTES[unit] ?? 1)) : 0
}
/** Convert stored minutes back to the largest whole unit for display. */
function minutesToInterval(minutes: number): { value: string; unit: 'minutes' | 'hours' | 'days' } {
  if (minutes > 0 && minutes % 1440 === 0) return { value: String(minutes / 1440), unit: 'days' }
  if (minutes > 0 && minutes % 60 === 0) return { value: String(minutes / 60), unit: 'hours' }
  return { value: String(minutes), unit: 'minutes' }
}

function StepSchedule({ state, set }: { state: FormState; set: <K extends keyof FormState>(key: K, value: FormState[K]) => void }) {
  const manual = state.scheduleKind === 'manual'
  const interval = state.scheduleKind === 'interval'
  const intervalMinutes = interval ? intervalToMinutes(state.scheduleExpression, state.scheduleUnit) : 0
  const tooShort = interval && state.scheduleExpression.trim().length > 0 && intervalMinutes < 15
  return (
    <div className="space-y-4">
      <p className="text-[13px] text-text-secondary">When should this agent run? You can always trigger runs manually regardless of the schedule.</p>
      <div className="grid gap-4 sm:grid-cols-3">
        <Field label="Cadence">
          <NativeSelect value={state.scheduleKind} onChange={value => {
            set('scheduleKind', value)
            if (value === 'interval' && !/^\d+$/.test(state.scheduleExpression.trim())) set('scheduleExpression', '2')
            if (value === 'daily' && !/^\d{1,2}:\d{2}$/.test(state.scheduleExpression.trim())) set('scheduleExpression', '06:00')
          }}>
            <option value="manual">Manual only</option>
            <option value="daily">Daily</option>
            <option value="interval">Interval</option>
          </NativeSelect>
        </Field>
        {interval ? (
          <Field label="Every" hint={tooShort ? 'Minimum is 15 minutes.' : intervalMinutes >= 15 ? `Runs every ${intervalMinutes} min.` : undefined}>
            <div className="flex gap-2">
              <Input inputSize="sm" type="number" min={1} value={state.scheduleExpression} onChange={event => set('scheduleExpression', event.target.value)} placeholder="2" />
              <NativeSelect value={state.scheduleUnit} onChange={value => set('scheduleUnit', value as FormState['scheduleUnit'])}>
                <option value="minutes">minutes</option>
                <option value="hours">hours</option>
                <option value="days">days</option>
              </NativeSelect>
            </div>
          </Field>
        ) : (
          <Field label="Time (HH:MM)">
            <Input inputSize="sm" disabled={manual} value={state.scheduleExpression} onChange={event => set('scheduleExpression', event.target.value)} placeholder="06:00" />
          </Field>
        )}
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
