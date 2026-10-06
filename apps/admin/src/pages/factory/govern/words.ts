import type { FactoryAction, FactoryPolicyMode, FactoryPrivacyClass, FactoryTaskClass } from '../../../types'

/**
 * Plain-language copy for the factory's machine codes (DESIGN_DIRECTION §11:
 * enum values never appear raw). Unknown codes fall back to a readable form of
 * the code itself, so a new backend reason still renders.
 */

/** `class_not_auto_startable` → `Class not auto startable`. */
export function readable(code: string): string {
  const text = code.replace(/[_-]+/g, ' ').trim()
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : code
}

export const ACTIONS: FactoryAction[] = ['fix', 'publish', 'open_pr', 'reply', 'close', 'approve', 'merge', 'deploy', 'notify', 'recover']

export const ACTION_DESCRIPTION: Record<FactoryAction, string> = {
  fix: 'Write a change in the sandbox to resolve the task.',
  publish: 'Push the change to the repository as a branch.',
  open_pr: 'Open a pull request for the pushed branch.',
  reply: 'Comment on the issue, pull request or thread it came from.',
  close: 'Close the issue or task it worked on.',
  approve: 'Approve a pull request as a reviewer.',
  merge: 'Merge a reviewed pull request into its base branch.',
  deploy: 'Release a merged change to an environment.',
  notify: 'Send a message to people, for example in Slack.',
  recover: 'Retry or repair after a failed run or check.',
}

export const TASK_CLASSES: FactoryTaskClass[] = ['docs', 'tests', 'ui', 'backend', 'bugfix', 'refactor', 'migration', 'infra', 'security', 'unknown']

const TASK_CLASS_LABEL: Record<string, string> = {
  docs: 'Docs',
  tests: 'Tests',
  ui: 'UI',
  backend: 'Backend',
  bugfix: 'Bug fix',
  refactor: 'Refactor',
  migration: 'Migration',
  infra: 'Infrastructure',
  security: 'Security',
  unknown: 'Unclassified',
}

export function taskClassLabel(value: string): string {
  return TASK_CLASS_LABEL[value] ?? readable(value)
}

/** The four rungs of the autonomy ladder; `after_fix`/`after_merge` share "Gated". */
export type Rung = 'never' | 'person' | 'gated' | 'model'

export const RUNGS: { rung: Rung; label: string }[] = [
  { rung: 'never', label: 'Never' },
  { rung: 'person', label: 'Person' },
  { rung: 'gated', label: 'Gated' },
  { rung: 'model', label: 'Model' },
]

export function rungOf(mode: FactoryPolicyMode): Rung {
  if (mode === 'never') return 'never'
  if (mode === 'manual') return 'person'
  if (mode === 'criteria') return 'model'
  return 'gated'
}

/** "Who decides" choices, in ladder order, with what each means. */
export const MODES: { value: FactoryPolicyMode; title: string; short: string; description: string }[] = [
  {
    value: 'never',
    title: 'Never',
    short: 'Never',
    description: 'The agent may not do this. The action is refused, not held.',
  },
  {
    value: 'manual',
    title: 'A person decides',
    short: 'A person decides',
    description: 'Held every time until someone approves it.',
  },
  {
    value: 'after_fix',
    title: 'After a verified fix',
    short: 'After a verified fix',
    description: 'Allowed once a fix exists and its checks passed. Held until then.',
  },
  {
    value: 'after_merge',
    title: 'After the merge',
    short: 'After the merge',
    description: 'Allowed once the change is merged. Held until then.',
  },
  {
    value: 'criteria',
    title: 'The decision model decides',
    short: 'Model decides',
    description: 'Allowed when the decision model finds the conditions met with enough confidence. Held for a person until a model is connected.',
  },
]

export function modeMeta(mode: FactoryPolicyMode) {
  return MODES.find(item => item.value === mode) ?? MODES[1]
}

export const PRIVACY_LABEL: Record<FactoryPrivacyClass, string> = {
  public: 'Public',
  internal: 'Internal',
  confidential: 'Confidential',
  restricted: 'Restricted',
}

/** Why the policy engine or merge gate held something (policy_engine.rs, merge_gate.rs). */
export function holdReason(reason: string): string {
  const [code, ...rest] = reason.split(':')
  const detail = rest.join(':')
  switch (code) {
    case 'policy_manual':
      return 'The policy for this action says a person decides.'
    case 'no_policy_defaults_to_manual':
      return 'No policy covers this action, so it waits for a person.'
    case 'project_unresolved':
      return 'A project-scoped policy exists for this action, but the factory could not tell which project this belongs to.'
    case 'decision_model_not_configured':
      return 'The policy lets the decision model decide, but no model is connected yet.'
    case 'decision_model_failed':
      return 'The decision model did not answer, so a person decides.'
    case 'decision_model_low_confidence':
      return `The decision model was not confident enough${detail ? ` (${Math.round(Number(detail) * 100)}%)` : ''}.`
    case 'decision_model_hold':
      return 'The decision model judged it too risky to do on its own.'
    case 'awaiting_verified_fix':
      return 'Waiting for a verified fix.'
    case 'awaiting_merge':
      return 'Waiting for the change to be merged.'
    case 'policy_never':
      return 'The policy forbids this action.'
    case 'floor':
      return `A safety floor applies${detail ? `: ${floorLabel(detail)}` : ''}.`
    case 'risky_path':
    case 'risky_title':
      return `It touches a risky area${detail ? `: ${riskArea(detail)}` : ''}.`
    case 'path_not_eligible':
      return 'It changes build, CI, dependency or agent files.'
    case 'checks_not_green':
      return 'A required check is not passing.'
    case 'required_check_missing':
      return `A required check has not reported${detail ? ` (${detail})` : ''}.`
    case 'no_checks_to_verify':
      return 'No checks ran on this commit, so nothing proves it works.'
    default:
      return readable(reason)
  }
}

function riskArea(area: string): string {
  const areas: Record<string, string> = {
    infra: 'infrastructure',
    database: 'database or migrations',
    payments: 'payments',
    external_provider: 'an external provider',
    security: 'security',
  }
  return areas[area.split(':')[0]] ?? readable(area).toLowerCase()
}

/** `sensitive_path:.github/workflows/ci.yml` → `sensitive path .github/workflows/ci.yml`. */
function floorLabel(floor: string): string {
  const [kind, ...rest] = floor.split(':')
  return `${readable(kind).toLowerCase()}${rest.length ? ` ${rest.join(':')}` : ''}`
}

export function holdSource(source: string): string {
  if (source === 'floor') return 'Held by a safety floor'
  if (source === 'decision_model') return 'Held by the decision model'
  return 'Held by policy'
}

/** Run status → words (mirrors the Runs page). */
export function runStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    blocked_policy: 'Blocked by policy',
    blocked_runtime: 'Blocked by the runtime',
    budget_exhausted: 'Out of budget',
    partial: 'Partly done',
    failed: 'Failed',
    cancelled: 'Cancelled',
  }
  return labels[status] ?? readable(status)
}

/** Why an intake item did not start on its own (factory_intake.rs). */
export function startReason(reason: string): string {
  const [code, detail] = reason.split(':')
  switch (code) {
    case 'class_not_auto_startable':
      return `${detail ? taskClassLabel(detail) : 'This kind of'} work never starts on its own.`
    case 'restricted_data_needs_a_person':
      return 'The source is marked restricted, so a person starts it.'
    case 'decision_model_unavailable':
      return 'No decision model is connected.'
    case 'decision_model_error':
      return 'The decision model did not answer.'
    case 'daily_start_cap_reached':
      return 'The daily limit of automatic starts was reached.'
    case 'resolver_not_ready':
      return 'The issue resolver is not ready to run.'
    case 'repository_not_private':
      return 'The repository is public; only private repositories start on their own.'
    case 'repository_unreadable':
      return 'The repository could not be read.'
    case 'github_unavailable':
      return 'GitHub was not reachable.'
    case 'issue_create_failed':
      return 'The GitHub issue could not be created.'
    case 'deciding':
      return 'Still deciding.'
    default:
      return readable(reason)
  }
}
