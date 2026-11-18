// Isolated Claude Code worker contract.
//
// The autonomous engineering loop drives Claude Code over untrusted
// repositories, so the execution plane must be disposable and narrow: the
// caller may choose a role, a model and a sandbox, and nothing else. Every
// function here is pure (or filesystem-only) and returns plain data, so the
// contract is testable without ever spawning the CLI. The Rust worker
// (`apps/backend/src/automation/worker.rs`, `profiles.rs`, `policy.rs`) is the
// source of truth for the flags, limits, reason codes and redaction patterns
// mirrored below; keeping the two spellings identical is what lets operators
// read one audit trail across both planes.

import { spawn } from 'node:child_process'
import type { ChildProcess, SpawnOptions } from 'node:child_process'
import { mkdirSync, readdirSync, realpathSync, rmdirSync, statSync, unlinkSync, writeFileSync } from 'node:fs'
import { basename, dirname, isAbsolute, join, resolve, sep } from 'node:path'

/** Only provider this worker can execute; anything else is denied, never run. */
export const CLAUDE_CODE_PROVIDER = 'claude-code'

/** Literal every redactor substitutes, matched to the Rust `sanitize_output`. */
export const REDACTION_PLACEHOLDER = '[REDACTED]'

/** Marker used when redaction would collapse two distinct keys into one. */
export const REDACTION_COLLISION = '[content omitted: secret redaction collision]'

/**
 * Stream caps. Browser-driven QA produces large streams (an accessibility
 * snapshot per turn), so these are generous; they still bound worker memory.
 */
export const STREAM_LIMITS = {
  maxStreamBytes: 32 * 1_048_576,
  maxLineBytes: 4 * 1_048_576,
  maxNoiseLines: 100,
  maxLines: 100_000,
} as const

/** Hard ceiling on `--max-turns`: a runaway loop must end on its own. */
export const MAX_TURNS_CAP = 200

/**
 * The only variables copied into the child. The env starts EMPTY so a worker
 * host's own credentials (GITHUB_TOKEN, AWS_*, database URLs) can never reach
 * an agent session.
 */
export const CLAUDE_ENV_PASSTHROUGH = [
  'HOME',
  'PATH',
  'USER',
  'LOGNAME',
  'LANG',
  'LC_ALL',
  'TMPDIR',
  'HTTPS_PROXY',
  'HTTP_PROXY',
  'SSL_CERT_FILE',
  'NODE_EXTRA_CA_CERTS',
  // Propagated to the Playwright MCP (spawned by Claude Code) so it finds the
  // image's pre-installed browsers.
  'PLAYWRIGHT_BROWSERS_PATH',
  // Needed by the `nexusmind` MCP server to authenticate; without it the server
  // stays pending and the agent cannot pull project context.
  'NEXUSMIND_API_KEY',
] as const

export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue }

export interface PermissionPlan {
  readonly permissionMode: string
  readonly allowedTools: string
}

export interface ClaudeBudget {
  readonly wallTimeSeconds?: number
  readonly maxCostUsd?: number
  readonly maxTurns?: number
}

export interface ResolvedBudget {
  readonly wallTimeSeconds: number
  readonly maxCostUsd: number
  readonly maxTurns: number
}

export interface StdioPlan {
  readonly stdin: 'pipe'
  readonly stdout: 'pipe'
  readonly stderr: 'pipe'
  readonly promptOnStdin: true
}

export interface ClaudeInvocationRequest {
  readonly claudeBin: string
  readonly role: string
  readonly model: string
  readonly prompt: string
  readonly sandboxRoot: string
  readonly workspaceDir: string
  readonly addDirs?: readonly string[]
  readonly mcpConfigPath?: string
  readonly budget?: ClaudeBudget
  readonly env?: Readonly<Record<string, string | undefined>>
  /** Always rejected. Present so callers get a denial instead of silent drop. */
  readonly extraArgs?: readonly string[]
}

export interface ClaudeInvocation {
  readonly bin: string
  readonly argv: readonly string[]
  readonly cwd: string
  readonly env: Readonly<Record<string, string>>
  readonly stdio: StdioPlan
  readonly prompt: string
  readonly permission: PermissionPlan
  readonly budget: ResolvedBudget
}

export interface ExtensionRef {
  readonly name: string
  readonly version: string
  readonly hash: string
  readonly required: boolean
}

export interface ExecutionProfileVersion {
  readonly id: string
  readonly profile: string
  readonly version: number
  readonly provider: string
  readonly model: string
  readonly settingsHash: string
  readonly extensions: readonly ExtensionRef[]
}

export interface AuthorizationRequest {
  readonly provider: string
  readonly requestedProfile: string
  readonly organizationAllowedProfiles: readonly string[]
  readonly projectAllowedProfiles: readonly string[]
  readonly requestedCapabilities: readonly string[]
  readonly extensions: readonly ExtensionRef[]
}

export interface AuthorizationDecision {
  readonly status: 'allowed' | 'denied'
  readonly reason: string | null
  readonly provenance: ExecutionProfileVersion | null
}

export interface StreamMetrics {
  readonly format: 'stream-json'
  readonly events: Readonly<Record<string, number>>
  readonly lineCount: number
  readonly noiseLines: number
}

export interface ParsedEventStream {
  readonly result: Record<string, JsonValue>
  readonly metrics: StreamMetrics
}

export interface RedactOptions {
  /** Ephemeral literals for this run (see `EphemeralSecretStore.secrets`). */
  readonly secrets?: readonly string[]
  /** Byte ceiling applied before pattern matching. */
  readonly limit?: number
}

export type SpawnFn = (command: string, args: readonly string[], options: SpawnOptions) => ChildProcess

/**
 * Every rejection carries a stable snake_case `code` so the control plane can
 * record a machine-readable denial reason rather than a prose message.
 */
export class WorkerContractError extends Error {
  readonly code: string

  constructor(code: string) {
    super(code)
    this.name = 'WorkerContractError'
    this.code = code
  }
}

// --- (a) fixed noninteractive invocation -----------------------------------

// Keyed by template/role. A Map (not an object literal) so an attacker-supplied
// role like `constructor` cannot reach `Object.prototype` and widen the plan.
const PERMISSION_PLANS = new Map<string, PermissionPlan>([
  [
    'github_issue_resolver',
    { permissionMode: 'acceptEdits', allowedTools: 'Read,Edit,Write,Grep,Glob,Skill,Task,mcp__plugin_nexusmind_nexusmind__*' },
  ],
  ['judge', { permissionMode: 'default', allowedTools: 'Read,Grep,Glob,Skill,Task' }],
  // QA uses `default` (not `plan`) so the agent can drive the Playwright MCP;
  // repo mutation stays impossible because the allowlist omits Edit/Write/Bash.
  ['qa', { permissionMode: 'default', allowedTools: 'Read,Grep,Glob,Skill,Task,mcp__playwright__*' }],
  ['product_strategist', { permissionMode: 'plan', allowedTools: 'Read,Grep,Glob,Skill,Task' }],
])

/** Default-deny tail: an unknown role gets plan mode and read-only tools. */
export const DEFAULT_DENY_PLAN: PermissionPlan = { permissionMode: 'plan', allowedTools: 'Read,Grep,Glob' }

// argv goes to `spawn` with `shell: false`, so a SPACE in a value is harmless
// and must stay legal: `/mnt/ci agents/sandbox-1/repo` is a real sandbox path
// and fail-closing on it would break the loop for no security gain. What is
// never legal is a byte that can forge an argument boundary or an audit record
// — NUL, and every other C0/DEL control byte, which covers LF, CR and TAB —
// plus the shell metacharacters: every path this worker passes it canonicalized
// itself, so a `;` or `$` in one means a caller is probing, not that we met an
// exotic filename. A leading `-` is refused separately (see
// `assertSafeArgument`) because the CLI would read the value as a flag.
const UNSAFE_ARGUMENT = /[;&|`$><(){}\[\]'"\\!*?~\x00-\x1f\x7f]/
const MODEL_NAME = /^[A-Za-z0-9][A-Za-z0-9._:-]*$/

/**
 * Resolves the pinned permission mode and tool allowlist for a role. Unknown
 * roles never get a permissive default — they get the read-only plan.
 */
export function resolvePermissionPlan(role: string): PermissionPlan {
  return PERMISSION_PLANS.get(role) ?? DEFAULT_DENY_PLAN
}

/**
 * Clamps a caller budget into the band the loop can actually afford. Wall time
 * is clamped (a too-small value is a config slip, not an attack) while a
 * nonsensical turn count is refused outright: it is the only budget that bounds
 * tool calls, so silently inventing one would hide the mistake.
 */
export function resolveBudget(budget: ClaudeBudget = {}): ResolvedBudget {
  const wallTimeSeconds = clamp(finiteOr(budget.wallTimeSeconds, 3600), 30, 3600)
  const maxCostUsd = finiteOr(budget.maxCostUsd, 25.0)
  if (!(maxCostUsd > 0)) throw new WorkerContractError('invalid_max_cost_usd')
  const maxTurns = budget.maxTurns ?? 40
  if (!Number.isInteger(maxTurns) || maxTurns < 1 || maxTurns > MAX_TURNS_CAP) {
    throw new WorkerContractError('invalid_max_turns')
  }
  return { wallTimeSeconds, maxCostUsd, maxTurns }
}

/**
 * Builds the complete, fixed invocation. The prompt is NOT in argv: a
 * harness-sized prompt exceeds Linux's 128 KiB per-argument limit (E2BIG) and
 * argv is world-readable, so it is fed to `-p` over stdin instead. Callers
 * cannot contribute flags; every path they do supply is canonicalized into the
 * sandbox first.
 */
export function buildClaudeInvocation(request: ClaudeInvocationRequest): ClaudeInvocation {
  if (!isAbsolute(request.claudeBin)) throw new WorkerContractError('claude_bin_not_absolute')
  assertSafeArgument(request.claudeBin)
  if (request.extraArgs !== undefined && request.extraArgs.length > 0) {
    throw new WorkerContractError('unsupported_claude_argument')
  }
  if (typeof request.prompt !== 'string' || request.prompt.trim() === '') {
    throw new WorkerContractError('prompt_required')
  }
  if (!MODEL_NAME.test(request.model)) throw new WorkerContractError('unsafe_argument')

  const permission = resolvePermissionPlan(request.role)
  const budget = resolveBudget(request.budget)
  const cwd = canonicalizeSandboxPath(request.sandboxRoot, request.workspaceDir)
  const addDirs = (request.addDirs ?? []).map(dir => canonicalizeSandboxPath(request.sandboxRoot, dir))
  const mcpConfigPath =
    request.mcpConfigPath === undefined
      ? undefined
      : canonicalizeSandboxPath(request.sandboxRoot, request.mcpConfigPath)

  const argv: string[] = [
    '-p',
    '--output-format', 'stream-json',
    '--verbose',
    '--max-turns', String(budget.maxTurns),
    '--permission-mode', permission.permissionMode,
    '--allowedTools', permission.allowedTools,
    '--model', request.model,
    // The repository under work is untrusted: Claude Code must not load its
    // `.claude/settings*.json` (SessionStart hooks there run shell commands
    // before the model acts) nor its `.mcp.json` servers.
    '--setting-sources', 'user',
    '--strict-mcp-config',
  ]
  for (const dir of addDirs) {
    assertSafeArgument(dir)
    argv.push('--add-dir', dir)
  }
  if (mcpConfigPath !== undefined) {
    assertSafeArgument(mcpConfigPath)
    argv.push('--mcp-config', mcpConfigPath)
  }
  // Defensive invariant: the prompt must never have leaked into argv.
  if (argv.includes(request.prompt)) throw new WorkerContractError('prompt_in_argv')

  return {
    bin: request.claudeBin,
    argv,
    cwd,
    env: restrictClaudeEnvironment(request.env),
    stdio: noninteractiveStdioPlan(),
    prompt: request.prompt,
    permission,
    budget,
  }
}

/**
 * Spawns the invocation. Injected `spawnFn` keeps the contract testable: tests
 * assert on `buildClaudeInvocation` output and never reach this function.
 */
export function launchClaudeCode(invocation: ClaudeInvocation, spawnFn: SpawnFn = spawn): ChildProcess {
  const child = spawnFn(invocation.bin, [...invocation.argv], {
    cwd: invocation.cwd,
    env: { ...invocation.env },
    stdio: [invocation.stdio.stdin, invocation.stdio.stdout, invocation.stdio.stderr],
    shell: false,
  })
  child.stdin?.end(invocation.prompt)
  return child
}

// --- (b) restricted environment and stdio ----------------------------------

/**
 * Builds the child env from an EMPTY base, copying through only the
 * passthrough list, then pinning the two settings the loop depends on:
 * autoupdates off (an agent must not swap its own binary mid-run) and the
 * ESSENTIAL `nexusmind` MCP tool profile.
 */
export function restrictClaudeEnvironment(
  source: Readonly<Record<string, string | undefined>> = process.env,
): Record<string, string> {
  const env: Record<string, string> = {}
  for (const key of CLAUDE_ENV_PASSTHROUGH) {
    const value = source[key]
    if (typeof value === 'string' && value !== '') env[key] = value
  }
  env.DISABLE_AUTOUPDATER = '1'
  env.NEXUSMIND_MCP_TOOL_PROFILE = 'essential'
  return env
}

/**
 * Noninteractive stdio: stdin is a pipe carrying the prompt and is never
 * inherited, so the child can never attach to a TTY and wait for a human.
 */
export function noninteractiveStdioPlan(): StdioPlan {
  return { stdin: 'pipe', stdout: 'pipe', stderr: 'pipe', promptOnStdin: true }
}

// --- (c) pinned managed extensions -----------------------------------------

/** One managed profile revision, pinned to a settings hash and extension set. */
export function managedProfile(name: string, version: number): ExecutionProfileVersion {
  return {
    id: `managed-${name}-v${version}`,
    profile: name,
    version,
    provider: CLAUDE_CODE_PROVIDER,
    model: 'claude-sonnet',
    settingsHash: 'settings-sha256',
    extensions: [{ name: 'approved-mcp', version: '1.0.0', hash: 'extension-sha256', required: true }],
  }
}

/** The trusted control-plane snapshot. Never built from worker/repo input. */
export function managedProfiles(): ExecutionProfileVersion[] {
  return [managedProfile('read-only', 1), managedProfile('implementation', 2), managedProfile('qa-deploy', 3)]
}

/**
 * Authorizes one run against the managed snapshot. Fails closed at every step:
 * a foreign provider, a profile missing from either allowlist, an unknown
 * profile, a capability the profile cannot hold, or an extension set that is
 * not an exact match in both directions all return a denial reason.
 */
export function authorizeExecution(
  request: AuthorizationRequest,
  profiles: readonly ExecutionProfileVersion[] = managedProfiles(),
): AuthorizationDecision {
  if (request.provider !== CLAUDE_CODE_PROVIDER) return denied('unsupported_provider')
  if (
    !request.organizationAllowedProfiles.includes(request.requestedProfile) ||
    !request.projectAllowedProfiles.includes(request.requestedProfile)
  ) {
    return denied('profile_not_allowed')
  }
  const profile = profiles.find(
    candidate => candidate.profile === request.requestedProfile && candidate.provider === CLAUDE_CODE_PROVIDER,
  )
  if (profile === undefined) return denied('profile_not_found')

  const capabilityDenial = validateCapabilities(profile.profile, request.requestedCapabilities)
  if (capabilityDenial !== null) return denied(capabilityDenial)
  const extensionDenial = validateExtensions(profile.extensions, request.extensions)
  if (extensionDenial !== null) return denied(extensionDenial)

  return { status: 'allowed', reason: null, provenance: profile }
}

function validateCapabilities(profile: string, capabilities: readonly string[]): string | null {
  const has = (capability: string): boolean => capabilities.includes(capability)
  if (profile === 'read-only') {
    const writes = ['repository_write', 'pr_publish', 'deployment_handoff', 'write_credentials']
    return writes.some(has) ? 'read_only_write_denied' : null
  }
  if (profile === 'implementation') {
    const escalations = ['merge', 'deployment_handoff', 'production_deploy']
    return escalations.some(has) ? 'profile_capability_denied' : null
  }
  if (profile === 'qa-deploy') {
    return has('merge') || has('production_deploy') ? 'profile_capability_denied' : null
  }
  return null
}

function validateExtensions(
  expected: readonly ExtensionRef[],
  received: readonly ExtensionRef[],
): string | null {
  // Exact match in both directions: nothing unapproved may load (version AND
  // hash must be the pinned ones) and nothing required may be missing.
  for (const extension of received) {
    if (!expected.some(approved => sameExtension(approved, extension))) return 'unapproved_extension'
  }
  for (const extension of expected.filter(candidate => candidate.required)) {
    if (!received.some(provided => sameExtension(provided, extension))) return 'required_extension_unavailable'
  }
  return null
}

function sameExtension(left: ExtensionRef, right: ExtensionRef): boolean {
  return (
    left.name === right.name &&
    left.version === right.version &&
    left.hash === right.hash &&
    left.required === right.required
  )
}

function denied(reason: string): AuthorizationDecision {
  return { status: 'denied', reason, provenance: null }
}

// --- (d) strict stream-event parsing ---------------------------------------

/**
 * Parses a `--output-format stream-json` stdout buffer. Strict by design: the
 * run only counts as finished when a machine-readable `result` event is
 * present, so a crashed or truncated session can never be read as success.
 * Returns the last `result` event — redacted, so nothing the caller persists
 * carries a credential — plus per-type metrics for the receipt.
 *
 * @param value raw stdout, as bytes or as an already-decoded string
 * @param secrets ephemeral literals for this run (`EphemeralSecretStore.secrets()`)
 */
export function parseClaudeEventStream(
  value: Uint8Array | string,
  secrets: readonly string[] = [],
): ParsedEventStream {
  const bytes = toBuffer(value)
  if (bytes.length > STREAM_LIMITS.maxStreamBytes) {
    throw new WorkerContractError('claude_event_stream_too_large')
  }
  // Parse FIRST and redact only what is retained. Redacting the raw stream
  // corrupts it: `{"text":"api_key: abc"}` is a legitimate event, but the
  // `api[_-]?key\s*[=:]\s*\S+` pattern eats the closing `"}` along with the
  // value, so the line stops being JSON, counts as noise, and enough of those
  // surface as `claude_event_stream_noisy` — a parser contract broken by its
  // own sanitizer. Nothing read here escapes except the redacted result.
  const text = bytes.toString('utf8')
  const counts = new Map<string, number>()
  let result: Record<string, JsonValue> | null = null
  let lineCount = 0
  let noiseLines = 0

  for (const line of text.split('\n')) {
    if (line.trim() === '') continue
    lineCount += 1
    if (lineCount > STREAM_LIMITS.maxLines || Buffer.byteLength(line, 'utf8') > STREAM_LIMITS.maxLineBytes) {
      throw new WorkerContractError('claude_event_stream_limit_exceeded')
    }
    const event = parseJsonObject(line)
    if (event === null) {
      noiseLines += 1
      continue
    }
    const kind = event['type']
    if (typeof kind !== 'string' || kind === '') {
      noiseLines += 1
      continue
    }
    counts.set(kind, (counts.get(kind) ?? 0) + 1)
    if (kind === 'result') result = event
  }

  if (result === null) {
    // A stream drowned in library noise is a different operational problem
    // (wrong binary, debug logging) than a run that simply never finished.
    if (noiseLines > STREAM_LIMITS.maxNoiseLines) throw new WorkerContractError('claude_event_stream_noisy')
    throw new WorkerContractError('claude_result_event_missing')
  }
  if (result['is_error'] === true) {
    const error = typeof result['error'] === 'string' ? result['error'].toLowerCase() : ''
    const message = typeof result['result'] === 'string' ? result['result'].slice(0, 1200).toLowerCase() : ''
    const status = result['api_error_status']
    // Revoked OAuth must block the loop (and page an operator) instead of
    // being retried as a transient model error.
    if (
      status === 401 ||
      error === 'authentication_failed' ||
      message.includes('failed to authenticate') ||
      message.includes('access token has been revoked')
    ) {
      throw new WorkerContractError('claude_auth_required')
    }
    throw new WorkerContractError('claude_event_reported_error')
  }

  const events: Record<string, number> = {}
  for (const kind of [...counts.keys()].sort()) events[kind] = counts.get(kind) ?? 0
  return {
    result: redactEvent(result, secrets),
    metrics: { format: 'stream-json', events, lineCount, noiseLines },
  }
}

/**
 * Redacts a retained event while preserving its object shape, so the receipt
 * can still be read as `Record<string, JsonValue>` even in the pathological
 * case where `redactJson` collapses the whole object on a key collision.
 */
function redactEvent(event: Record<string, JsonValue>, secrets: readonly string[]): Record<string, JsonValue> {
  const redacted = redactJson(event, secrets)
  if (typeof redacted === 'object' && redacted !== null && !Array.isArray(redacted)) return redacted
  return { type: event['type'] ?? 'result', result: REDACTION_COLLISION }
}

function parseJsonObject(line: string): Record<string, JsonValue> | null {
  try {
    const parsed: unknown = JSON.parse(line)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return null
    return parsed as Record<string, JsonValue>
  } catch {
    return null
  }
}

// --- (e) event redaction ---------------------------------------------------

// Mirrors the Rust `sanitize_output` patterns. JavaScript has no inline `(?i)`,
// so the case-insensitive pattern carries the `i` flag instead.
const SECRET_PATTERNS: readonly RegExp[] = [
  /gh[pousr]_[A-Za-z0-9_]{20,}/g,
  /github_pat_[A-Za-z0-9_]{20,}/g,
  /https:\/\/hooks\.slack\.com\/services\/\S+/g,
  /xox[baprs]-[A-Za-z0-9-]{10,}/g,
  /AKIA[0-9A-Z]{16}/g,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/g,
  /(token|secret|password|api[_-]?key)\s*[=:]\s*\S+/gi,
]

// A JSON key is only a name, so `{"token": "abc"}` never matches the `=`/`:`
// pattern above; these names redact their value instead.
const SENSITIVE_KEYS = new Set(['token', 'secret', 'password', 'api_key', 'api-key', 'apikey', 'authorization'])

/**
 * Truncates to `limit` bytes, then replaces every known credential shape and
 * every caller-supplied ephemeral literal with `[REDACTED]`. Call this on
 * anything that gets persisted, logged or returned to an operator.
 *
 * Order matters: pass `store.secrets()` BEFORE `store.teardown()`, which
 * deliberately drops the literals so later calls cannot re-read them.
 */
export function redactText(value: Uint8Array | string, options: RedactOptions = {}): string {
  const limit = options.limit ?? STREAM_LIMITS.maxStreamBytes
  const bytes = toBuffer(value)
  let text = bytes.subarray(0, Math.min(bytes.length, Math.max(limit, 0))).toString('utf8')
  for (const pattern of SECRET_PATTERNS) text = text.replace(pattern, REDACTION_PLACEHOLDER)
  for (const secret of options.secrets ?? []) {
    if (secret !== '') text = text.split(secret).join(REDACTION_PLACEHOLDER)
  }
  return text
}

/**
 * Redacts a parsed event recursively. Object KEYS are redacted too, because an
 * agent can put a credential in a key as easily as in a value; a sensitively
 * named key has its whole value dropped. If redaction would merge two distinct
 * keys, the whole object is replaced by a marker instead of silently losing a
 * field.
 */
export function redactJson(value: JsonValue, secrets: readonly string[] = []): JsonValue {
  if (typeof value === 'string') return redactText(value, { secrets })
  if (Array.isArray(value)) return value.map(item => redactJson(item, secrets))
  if (typeof value !== 'object' || value === null) return value

  const out: Record<string, JsonValue> = {}
  for (const [key, child] of Object.entries(value)) {
    const cleanKey = redactText(key, { secrets })
    if (Object.hasOwn(out, cleanKey)) return REDACTION_COLLISION
    out[cleanKey] = SENSITIVE_KEYS.has(key.toLowerCase()) ? REDACTION_PLACEHOLDER : redactJson(child, secrets)
  }
  return out
}

// --- canonical sandbox paths ----------------------------------------------

/**
 * Resolves `candidate` to its canonical location and proves it stays inside
 * the sandbox. Symlinks are followed (a link inside the sandbox pointing out of
 * it is an escape), `..` is collapsed, and a not-yet-created leaf is allowed
 * only when its deepest existing ancestor is itself inside the sandbox — that
 * is what makes `--add-dir`, `--mcp-config` and the workspace safe to pass to
 * a CLI running with write permissions.
 */
export function canonicalizeSandboxPath(sandboxRoot: string, candidate: string): string {
  if (!isAbsolute(sandboxRoot)) throw new WorkerContractError('sandbox_root_not_absolute')
  if (typeof candidate !== 'string' || candidate === '') throw new WorkerContractError('sandbox_path_escape')
  let root: string
  try {
    root = realpathSync(sandboxRoot)
  } catch {
    throw new WorkerContractError('sandbox_root_unavailable')
  }
  const target = isAbsolute(candidate) ? candidate : resolve(root, candidate)
  const real = canonicalizeExistingPrefix(target)
  if (real !== root && !real.startsWith(root + sep)) throw new WorkerContractError('sandbox_path_escape')
  return real
}

function canonicalizeExistingPrefix(target: string): string {
  const missing: string[] = []
  let current = resolve(target)
  for (;;) {
    try {
      // The missing tail cannot contain symlinks (it does not exist yet), so
      // resolving the existing ancestor is enough to pin the whole path.
      return resolve(realpathSync(current), ...[...missing].reverse())
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') {
        throw new WorkerContractError('sandbox_path_unavailable')
      }
      const parent = dirname(current)
      if (parent === current) throw new WorkerContractError('sandbox_path_unavailable')
      missing.push(basename(current))
      current = parent
    }
  }
}

// --- (f) ephemeral secret teardown ----------------------------------------

/**
 * Tracks the secret material minted for a single run: files written into an
 * ephemeral directory and the literals that must be redacted out of output.
 *
 * `teardown()` overwrites each file with zeros before unlinking it (so the
 * bytes do not survive in a reused block), removes the ephemeral directory
 * when empty, and clears the in-memory literals — after which `secrets()`
 * returns nothing, so redaction must run BEFORE teardown. It is idempotent:
 * calling it twice (normal exit plus a `finally`) must never throw.
 */
export class EphemeralSecretStore {
  readonly #dir: string | undefined
  readonly #files = new Set<string>()
  #values: string[] = []
  #tornDown = false

  constructor(dir?: string) {
    this.#dir = dir
  }

  /** Writes `contents` to `name` inside the ephemeral dir, owner-only. */
  writeSecretFile(name: string, contents: string): string {
    if (this.#dir === undefined) throw new WorkerContractError('ephemeral_dir_required')
    if (this.#tornDown) throw new WorkerContractError('ephemeral_store_torn_down')
    if (name === '' || name.includes(sep) || name === '.' || name === '..') {
      throw new WorkerContractError('ephemeral_secret_name_invalid')
    }
    mkdirSync(this.#dir, { recursive: true, mode: 0o700 })
    const target = join(this.#dir, name)
    writeFileSync(target, contents, { mode: 0o600 })
    this.#files.add(target)
    this.registerValue(contents)
    return target
  }

  /** Tracks a file this run must shred (e.g. one written by a helper). */
  registerFile(path: string): void {
    if (!this.#tornDown) this.#files.add(path)
  }

  /** Tracks a literal that must never appear in persisted output. */
  registerValue(value: string): void {
    if (!this.#tornDown && value !== '' && !this.#values.includes(value)) this.#values.push(value)
  }

  /** The literals to pass to `redactText`/`redactJson`. Empty after teardown. */
  secrets(): readonly string[] {
    return [...this.#values]
  }

  /** Files still tracked for shredding. Empty after teardown. */
  files(): readonly string[] {
    return [...this.#files]
  }

  get tornDown(): boolean {
    return this.#tornDown
  }

  /** Zeroes and removes every tracked file, then forgets the literals. */
  teardown(): void {
    for (const path of this.#files) {
      try {
        const size = statSync(path).size
        writeFileSync(path, Buffer.alloc(size, 0))
      } catch {
        // Already gone or unreadable: unlink below is still attempted.
      }
      try {
        unlinkSync(path)
      } catch {
        // Idempotent teardown: a missing file is the desired end state.
      }
    }
    this.#files.clear()
    this.#values = []
    if (this.#dir !== undefined) {
      try {
        if (readdirSync(this.#dir).length === 0) rmdirSync(this.#dir)
      } catch {
        // A non-empty or already removed dir is not a teardown failure.
      }
    }
    this.#tornDown = true
  }
}

// --- helpers ---------------------------------------------------------------

function assertSafeArgument(value: string): void {
  if (typeof value !== 'string' || value === '') throw new WorkerContractError('unsafe_argument')
  // A value starting with `-` would be parsed as a flag, not as the operand of
  // the flag it follows.
  if (value.startsWith('-')) throw new WorkerContractError('unsafe_argument')
  if (UNSAFE_ARGUMENT.test(value)) throw new WorkerContractError('unsafe_argument')
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high)
}

function finiteOr(value: number | undefined, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function toBuffer(value: Uint8Array | string): Buffer {
  return typeof value === 'string' ? Buffer.from(value, 'utf8') : Buffer.from(value)
}
