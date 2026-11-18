/**
 * Execution-plane adapter for one disposable Claude Code run.
 *
 * `openspec/changes/autonomous-loop-engineering/exploration.md` (architecture
 * point 2) puts the runner outside the control plane: it consumes a
 * NexusMind-owned job, reports structured events, and has no authority of its
 * own. This module is that boundary, and it is deliberately narrow — every
 * export is a pure function, so the whole contract is testable without a
 * sandbox, a network or a Claude CLI on the box.
 *
 * Five things the job spec must never be able to express, because the spec is
 * the only thing standing between an untrusted repository and the CLI:
 *
 *   1. Extra Claude CLI arguments. `argv` is assembled here from the managed
 *      profile alone. An unrecognised spec field is a hard failure, not an
 *      ignored one, so a widened control plane can never smuggle a flag.
 *   2. Interactive input. Fixed `-p` with the prompt on stdin (same shape as
 *      `apps/backend/src/automation/sandboxed.rs`), piped stdio and
 *      `TERM=dumb`: the child never gets a TTY to prompt on.
 *   3. Unpinned extensions. Managed extensions carry an exact version and a
 *      sha256 digest, and `--strict-mcp-config` stops the CLI from honouring a
 *      `.mcp.json` committed inside the sandboxed repository.
 *   4. Non-canonical sandbox paths. Roots must already be canonical and every
 *      derived path is proven to stay inside the root.
 *   5. Unredacted ephemeral secrets. Events are redacted against the live
 *      secret values and the redaction is asserted, not assumed; the material
 *      is then torn down. Per CLAUDE.md, a token never reaches a log, a prompt
 *      or a receipt.
 */

import { isAbsolute, join, normalize } from 'node:path/posix'

export const CLAUDE_CODE_PROVIDER = 'claude-code'

/** Same placeholder the control plane writes (`automation/worker.rs`). */
export const REDACTION_PLACEHOLDER = '[REDACTED]'

/** Mirrors `managed_profiles()` in `apps/backend/src/automation/profiles.rs`. */
export const MANAGED_PROFILES = ['read-only', 'implementation', 'qa-deploy'] as const

export type ManagedProfile = (typeof MANAGED_PROFILES)[number]

/**
 * Claude Code's own permission mode per managed profile. This only ever
 * narrows what the sandbox already permits — the sandbox, not the CLI, is the
 * authority on what a profile may write.
 */
const PERMISSION_MODES: Readonly<Record<ManagedProfile, string>> = {
  'read-only': 'plan',
  implementation: 'acceptEdits',
  'qa-deploy': 'acceptEdits',
}

/** Host variables the child may inherit. Everything else is dropped. */
const INHERITED_ENV = ['PATH', 'HOME', 'LANG', 'TZ'] as const

/** Noninteractive environment: no colour, no pager, no TTY affordances. */
const FIXED_ENV: Readonly<Record<string, string>> = {
  CI: '1',
  TERM: 'dumb',
  NO_COLOR: '1',
}

/** The only env names ephemeral secret material may bind. */
const EPHEMERAL_ENV_NAMES = ['ANTHROPIC_API_KEY', 'GH_TOKEN'] as const

/** Managed configuration lives beside the checkout, never inside it. */
const MANAGED_CONFIG_DIR = '.nexusmind'

export const STREAM_EVENT_TYPES = ['system', 'assistant', 'user', 'result'] as const

export type StreamEventType = (typeof STREAM_EVENT_TYPES)[number]

/** One `--output-format stream-json` line, after strict validation. */
export interface StreamEvent {
  readonly type: StreamEventType
  readonly [field: string]: unknown
}

/** A single stream line larger than this is treated as unparsable. */
export const MAX_STREAM_LINE_BYTES = 1_048_576

const JOB_SPEC_FIELDS = [
  'jobId',
  'profile',
  'provider',
  'model',
  'prompt',
  'sandboxRoot',
  'worktree',
  'extensions',
] as const

const EXTENSION_FIELDS = ['name', 'version', 'hash', 'required'] as const

const JOB_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/
/** `model` reaches argv, so it must not be able to look like a flag. */
const MODEL = /^[a-z0-9][a-z0-9.-]{0,63}$/
const EXTENSION_NAME = /^[a-z0-9][a-z0-9-]{0,63}$/
/** An exact pin: no range operator, no `latest`, no floating tag. */
const PINNED_VERSION = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/
const PINNED_DIGEST = /^sha256:[0-9a-f]{64}$/
/** Keys that must never survive a parse into an object we then walk. */
const UNSAFE_KEYS = new Set(['__proto__', 'constructor', 'prototype'])

export type WorkerContractCode =
  | 'unknown_job_field'
  | 'invalid_job_field'
  | 'unsupported_provider'
  | 'unknown_profile'
  | 'unpinned_extension'
  | 'unapproved_extension'
  | 'required_extension_unavailable'
  | 'sandbox_root_not_canonical'
  | 'sandbox_path_not_canonical'
  | 'sandbox_path_escapes_root'
  | 'unapproved_secret_name'
  | 'secret_material_disposed'
  | 'redaction_incomplete'
  | 'unparsable_stream_event'
  | 'unknown_stream_event'

/** Every rejection carries a stable code so receipts stay machine-readable. */
export class WorkerContractError extends Error {
  readonly code: WorkerContractCode

  constructor(code: WorkerContractCode, detail?: string) {
    super(detail ? `${code}: ${detail}` : code)
    this.name = 'WorkerContractError'
    this.code = code
  }
}

// A declaration, not an arrow: TypeScript only narrows control flow after a
// `never`-returning call when the callee is declared this way.
function fail(code: WorkerContractCode, detail?: string): never {
  throw new WorkerContractError(code, detail)
}

/** Mirrors `ExtensionRef` in `apps/backend/src/automation/profiles.rs`. */
export interface ManagedExtension {
  readonly name: string
  readonly version: string
  readonly hash: string
  readonly required: boolean
}

export interface WorkerJobSpec {
  readonly jobId: string
  readonly profile: ManagedProfile
  readonly provider: string
  readonly model: string
  readonly prompt: string
  readonly sandboxRoot: string
  readonly worktree: string
  readonly extensions: readonly ManagedExtension[]
}

export interface ClaudeCodeInvocation {
  readonly command: 'claude'
  readonly argv: readonly string[]
  readonly cwd: string
  /** Piped on every descriptor: the child cannot reach the operator's TTY. */
  readonly stdio: readonly ['pipe', 'pipe', 'pipe']
  /** The prompt. Never an argv entry, so it cannot be read as a flag. */
  readonly stdin: string
  readonly env: Readonly<Record<string, string>>
}

export interface PreparedRun {
  readonly jobId: string
  readonly profile: ManagedProfile
  readonly extensions: readonly ManagedExtension[]
  readonly invocation: ClaudeCodeInvocation
}

function asRecord(value: unknown, code: WorkerContractCode, field: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    fail(code, `${field} must be an object`)
  }
  const record = value as Record<string, unknown>
  for (const key of Object.keys(record)) {
    if (UNSAFE_KEYS.has(key)) fail(code, `${field}.${key} is not allowed`)
  }
  return record
}

function matched(record: Record<string, unknown>, field: string, pattern: RegExp): string {
  const value = record[field]
  if (typeof value !== 'string' || !pattern.test(value)) {
    fail('invalid_job_field', field)
  }
  return value as string
}

/**
 * Validates an untrusted job spec. Unknown fields are rejected rather than
 * ignored: that rejection is what keeps the CLI argument list closed.
 */
export function parseWorkerJobSpec(raw: unknown): WorkerJobSpec {
  const record = asRecord(raw, 'invalid_job_field', 'job')
  for (const key of Object.keys(record)) {
    if (!(JOB_SPEC_FIELDS as readonly string[]).includes(key)) fail('unknown_job_field', key)
  }

  const provider = record.provider
  if (provider !== CLAUDE_CODE_PROVIDER) fail('unsupported_provider', String(provider))

  const profile = record.profile
  if (typeof profile !== 'string' || !(MANAGED_PROFILES as readonly string[]).includes(profile)) {
    fail('unknown_profile', String(profile))
  }

  const prompt = record.prompt
  if (typeof prompt !== 'string' || prompt.length === 0 || prompt.includes('\0')) {
    fail('invalid_job_field', 'prompt')
  }

  const extensions = record.extensions
  if (!Array.isArray(extensions)) fail('invalid_job_field', 'extensions')

  return {
    jobId: matched(record, 'jobId', JOB_ID),
    profile: profile as ManagedProfile,
    provider: CLAUDE_CODE_PROVIDER,
    model: matched(record, 'model', MODEL),
    prompt: prompt as string,
    sandboxRoot: assertCanonicalSandboxRoot(record.sandboxRoot),
    worktree: matched(record, 'worktree', /^[^\0]+$/),
    extensions: (extensions as unknown[]).map(parseManagedExtension),
  }
}

export function parseManagedExtension(raw: unknown): ManagedExtension {
  const record = asRecord(raw, 'invalid_job_field', 'extension')
  for (const key of Object.keys(record)) {
    if (!(EXTENSION_FIELDS as readonly string[]).includes(key)) fail('unknown_job_field', key)
  }
  const required = record.required
  if (typeof required !== 'boolean') fail('invalid_job_field', 'extension.required')
  return {
    name: matched(record, 'name', EXTENSION_NAME),
    version: matched(record, 'version', /^[^\0]+$/),
    hash: matched(record, 'hash', /^[^\0]+$/),
    required,
  }
}

/** An extension is usable only when both its version and its digest are pinned. */
export function isPinnedExtension(extension: ManagedExtension): boolean {
  return PINNED_VERSION.test(extension.version) && PINNED_DIGEST.test(extension.hash)
}

const sameExtension = (left: ManagedExtension, right: ManagedExtension): boolean =>
  left.name === right.name &&
  left.version === right.version &&
  left.hash === right.hash &&
  left.required === right.required

/**
 * Execution-plane counterpart of `validate_extensions` in
 * `apps/backend/src/automation/policy.rs`, plus the pinning check: the control
 * plane decides *which* extensions are approved, the worker refuses to load
 * one that is not pinned to an exact version and digest.
 */
export function validateManagedExtensions(
  approved: readonly ManagedExtension[],
  requested: readonly ManagedExtension[],
): readonly ManagedExtension[] {
  for (const extension of approved) {
    if (!isPinnedExtension(extension)) fail('unpinned_extension', `approved ${extension.name}`)
  }
  for (const extension of requested) {
    if (!isPinnedExtension(extension)) fail('unpinned_extension', extension.name)
    if (!approved.some((entry) => sameExtension(entry, extension))) {
      fail('unapproved_extension', extension.name)
    }
  }
  for (const extension of approved.filter((entry) => entry.required)) {
    if (!requested.some((entry) => sameExtension(entry, extension))) {
      fail('required_extension_unavailable', extension.name)
    }
  }
  return requested
}

/**
 * A sandbox root must already be canonical. Canonicalising it here would hide
 * a control-plane bug that handed the worker an ambiguous path.
 */
export function assertCanonicalSandboxRoot(raw: unknown): string {
  if (typeof raw !== 'string' || raw.length === 0 || raw.includes('\0')) {
    fail('sandbox_root_not_canonical', 'sandboxRoot must be a path')
  }
  const root = raw as string
  if (!isAbsolute(root)) fail('sandbox_root_not_canonical', root)
  if (root !== '/' && root.endsWith('/')) fail('sandbox_root_not_canonical', root)
  if (normalize(root) !== root) fail('sandbox_root_not_canonical', root)
  return root
}

/** Resolves a sandbox-relative path and proves it stays inside the root. */
export function canonicalSandboxPath(sandboxRoot: string, relativePath: string): string {
  const root = assertCanonicalSandboxRoot(sandboxRoot)
  if (relativePath.length === 0 || relativePath.includes('\0')) {
    fail('sandbox_path_not_canonical', relativePath)
  }
  if (isAbsolute(relativePath)) fail('sandbox_path_escapes_root', relativePath)
  if (normalize(relativePath) !== relativePath) fail('sandbox_path_not_canonical', relativePath)

  const resolved = normalize(join(root, relativePath))
  const prefix = root === '/' ? '/' : `${root}/`
  if (resolved !== root && !resolved.startsWith(prefix)) {
    fail('sandbox_path_escapes_root', relativePath)
  }
  return resolved
}

/**
 * Short-lived credentials for one run. Values live only here, are handed to
 * the child as environment, and are overwritten on teardown. Reading a
 * disposed handle throws rather than returning an empty set, so a caller
 * cannot redact against nothing and believe it redacted.
 */
export interface EphemeralSecretMaterial {
  readonly disposed: boolean
  env(): Readonly<Record<string, string>>
  values(): readonly string[]
  teardown(): void
}

export function ephemeralSecretMaterial(
  entries: Readonly<Record<string, string>>,
): EphemeralSecretMaterial {
  const material = new Map<string, string>()
  for (const [name, value] of Object.entries(entries)) {
    if (!(EPHEMERAL_ENV_NAMES as readonly string[]).includes(name)) {
      fail('unapproved_secret_name', name)
    }
    if (typeof value !== 'string' || value.length === 0) fail('invalid_job_field', name)
    material.set(name, value)
  }

  let disposed = false
  const live = () => {
    if (disposed) fail('secret_material_disposed')
  }

  return {
    get disposed() {
      return disposed
    },
    env() {
      live()
      return Object.freeze(Object.fromEntries(material))
    },
    values() {
      live()
      return Object.freeze([...material.values()])
    },
    teardown() {
      for (const name of material.keys()) material.set(name, '')
      material.clear()
      disposed = true
    },
  }
}

/**
 * Builds the one invocation this worker is allowed to make. Nothing in the job
 * spec can add, remove or reorder a flag.
 */
export function buildInvocation(
  spec: WorkerJobSpec,
  secrets: EphemeralSecretMaterial,
  baseEnv: Readonly<Record<string, string | undefined>> = {},
): ClaudeCodeInvocation {
  const cwd = canonicalSandboxPath(spec.sandboxRoot, spec.worktree)
  const settings = canonicalSandboxPath(spec.sandboxRoot, `${MANAGED_CONFIG_DIR}/settings.json`)
  const mcpConfig = canonicalSandboxPath(spec.sandboxRoot, `${MANAGED_CONFIG_DIR}/mcp.json`)

  const env: Record<string, string> = { ...FIXED_ENV }
  for (const name of INHERITED_ENV) {
    const value = baseEnv[name]
    if (typeof value === 'string' && value.length > 0) env[name] = value
  }
  Object.assign(env, secrets.env())

  return Object.freeze({
    command: 'claude' as const,
    argv: Object.freeze([
      '-p',
      '--output-format',
      'stream-json',
      '--verbose',
      '--model',
      spec.model,
      '--permission-mode',
      PERMISSION_MODES[spec.profile],
      '--settings',
      settings,
      // Managed servers only. Without this the CLI would also load a
      // `.mcp.json` committed inside the sandboxed repository.
      '--strict-mcp-config',
      '--mcp-config',
      mcpConfig,
    ]),
    cwd,
    stdio: Object.freeze(['pipe', 'pipe', 'pipe'] as const),
    stdin: spec.prompt,
    env: Object.freeze(env),
  })
}

/** Validates an untrusted job spec against the approved profile and returns the one run it allows. */
export function prepareRun(
  raw: unknown,
  options: {
    approvedExtensions: readonly ManagedExtension[]
    secrets: EphemeralSecretMaterial
    baseEnv?: Readonly<Record<string, string | undefined>>
  },
): PreparedRun {
  const spec = parseWorkerJobSpec(raw)
  const extensions = validateManagedExtensions(options.approvedExtensions, spec.extensions)
  return {
    jobId: spec.jobId,
    profile: spec.profile,
    extensions,
    invocation: buildInvocation(spec, options.secrets, options.baseEnv),
  }
}

/**
 * Strictly parses one `stream-json` line. Anything the control plane cannot
 * interpret is rejected instead of forwarded as an opaque blob.
 */
export function parseStreamEvent(line: string): StreamEvent {
  if (Buffer.byteLength(line, 'utf8') > MAX_STREAM_LINE_BYTES) {
    fail('unparsable_stream_event', 'line_too_large')
  }

  let parsed: unknown
  try {
    parsed = JSON.parse(line) as unknown
  } catch {
    fail('unparsable_stream_event', 'invalid_json')
  }

  const event = asRecord(parsed, 'unparsable_stream_event', 'event')
  const type = event.type
  if (typeof type !== 'string' || !(STREAM_EVENT_TYPES as readonly string[]).includes(type)) {
    fail('unknown_stream_event', String(type))
  }
  if (type === 'result') {
    if (typeof event.subtype !== 'string') fail('unparsable_stream_event', 'result.subtype')
    if (typeof event.is_error !== 'boolean') fail('unparsable_stream_event', 'result.is_error')
  }
  return event as StreamEvent
}

/** Parses a stdout chunk, ignoring the blank lines the CLI pads between events. */
export function parseStreamEvents(chunk: string): readonly StreamEvent[] {
  return chunk
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .map(parseStreamEvent)
}

/**
 * Removes secret values from an event, keys included, and then asserts the
 * result is clean. Like the control plane's `redact_json_secrets`, a collision
 * created by redacting keys drops the payload rather than merging two fields.
 */
export function redactEvent<T>(event: T, secrets: readonly string[]): T {
  // Longest first, so a secret that contains another is replaced whole.
  const values = [...secrets].filter((secret) => secret.length > 0).sort((a, b) => b.length - a.length)
  const scrub = (text: string): string =>
    values.reduce((carry, secret) => carry.split(secret).join(REDACTION_PLACEHOLDER), text)

  const walk = (value: unknown): unknown => {
    if (typeof value === 'string') return scrub(value)
    if (Array.isArray(value)) return value.map(walk)
    if (typeof value === 'object' && value !== null) {
      const redacted: Record<string, unknown> = {}
      for (const [key, nested] of Object.entries(value)) {
        const cleanKey = scrub(key)
        if (Object.prototype.hasOwnProperty.call(redacted, cleanKey)) {
          fail('redaction_incomplete', 'key_collision')
        }
        redacted[cleanKey] = walk(nested)
      }
      return redacted
    }
    return value
  }

  const result = walk(event)
  const serialized = JSON.stringify(result) ?? ''
  for (const secret of values) {
    if (serialized.includes(secret)) fail('redaction_incomplete', 'secret_survived')
  }
  return result as T
}
