/**
 * Execution isolation of an autonomous agent (software factory F1).
 *
 * `sandbox` runs the agent in an ephemeral, credential-free task pod; `local` runs
 * it inside the worker with the worker's secrets (unsafe, kept as an explicit
 * escape hatch); `default` leaves the choice to the server.
 */

export type IsolationChoice = 'default' | 'sandbox' | 'local'

export interface IsolationState {
  isolation: IsolationChoice
  /** Free text, comma or line separated. */
  allowedHosts: string
}

/** Templates the server can sandbox (`SANDBOX_TEMPLATES`), Claude executor only. */
const SANDBOX_TEMPLATES = [
  'github_pr_reviewer',
  'qa',
  'judge',
  'github_issue_resolver',
  'security_scan',
  'security_dast',
]

/** Templates that drive a browser and may need extra hosts (CDN, SSO, payments). */
const BROWSER_TEMPLATES = ['qa', 'judge']

/** Templates the Codex executor runs (`CODEX_TEMPLATES`): sandbox only. */
export const CODEX_TEMPLATES = ['github_issue_resolver', 'github_pr_reviewer']

export function codexSupported(template: string): boolean {
  return CODEX_TEMPLATES.includes(template)
}

export function sandboxSupported(template: string, executor: string): boolean {
  if (executor === 'codex') return codexSupported(template)
  return executor === 'claude' && SANDBOX_TEMPLATES.includes(template)
}

/** Whether the worker may run this executor locally (Codex never does: its key lives in the proxy). */
export function localSupported(executor: string): boolean {
  return executor !== 'codex'
}

/** Mirrors the server's rule: lowercase DNS names, at least one dot, no IPs. */
function validHost(host: string): boolean {
  const labels = host.split('.')
  return (
    host.length <= 253 &&
    labels.length >= 2 &&
    labels.every(label => /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/.test(label)) &&
    !labels.every(label => /^[0-9]+$/.test(label))
  )
}

/** The server's limit on hosts one run may reach. */
export const MAX_SANDBOX_HOSTS = 16

export function parseAllowedHosts(text: string): { hosts: string[] } | { error: string } {
  const hosts: string[] = []
  for (const raw of text.split(/[\s,]+/)) {
    const entry = raw.trim()
    if (!entry) continue
    const host = entry.toLowerCase()
    if (!validHost(host)) return { error: `Not a host name: ${entry}` }
    if (!hosts.includes(host)) hosts.push(host)
  }
  if (hosts.length > MAX_SANDBOX_HOSTS) return { error: `At most ${MAX_SANDBOX_HOSTS} hosts` }
  return { hosts }
}

export function isolationConfig(state: IsolationState, template: string): Record<string, unknown> {
  const config: Record<string, unknown> = {}
  if (state.isolation !== 'default') config.isolation = state.isolation
  if (BROWSER_TEMPLATES.includes(template)) {
    const parsed = parseAllowedHosts(state.allowedHosts)
    if ('hosts' in parsed && parsed.hosts.length > 0) config.sandbox_allowed_hosts = parsed.hosts
  }
  return config
}

export function isolationFromConfig(config: Record<string, unknown>): IsolationState {
  const isolation = config.isolation === 'sandbox' || config.isolation === 'local' ? config.isolation : 'default'
  const hosts = Array.isArray(config.sandbox_allowed_hosts)
    ? config.sandbox_allowed_hosts.filter((host): host is string => typeof host === 'string')
    : []
  return { isolation, allowedHosts: hosts.join(', ') }
}
