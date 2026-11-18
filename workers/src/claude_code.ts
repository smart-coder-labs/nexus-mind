/**
 * Isolated Claude Code worker contract for the autonomous engineering loop.
 *
 * The execution plane is disposable: it receives a run, spawns Claude Code once
 * inside a sandbox, streams the transcript back and then disappears. Everything
 * it is allowed to do is decided here, so the surface is deliberately narrow:
 *
 * - the Claude CLI invocation is fixed (noninteractive, no TTY, stdin closed
 *   after the prompt); callers choose values, never flags;
 * - managed extensions must match the control-plane pins (name, version, hash);
 * - every sandbox path is canonical and contained in the sandbox root;
 * - stream-json lines are parsed strictly, and anything that is not a known
 *   event is reported as noise instead of being trusted;
 * - ephemeral secret material is redacted out of every event and torn down with
 *   the session.
 *
 * This module has no dependencies and performs no I/O: the host supplies the
 * process environment and feeds stdout lines in. That keeps the contract
 * testable and keeps policy decisions out of the spawning code.
 */

export const CLAUDE_CODE_PROVIDER = 'claude-code';

/** Permission modes a managed run may ask for. `bypassPermissions` is absent on purpose. */
export const PERMISSION_MODES = ['plan', 'acceptEdits'] as const;
export type PermissionMode = (typeof PERMISSION_MODES)[number];

/** Upper bound for `--max-turns`; a run that needs more is a budget decision, not a worker one. */
export const MAX_TURNS_LIMIT = 200;

/** Longest stdout line kept. Claude Code never emits more; a longer line is noise or an attack. */
export const MAX_STREAM_LINE_BYTES = 1_048_576;

/** Event types the control plane knows how to store. Anything else is noise. */
export const STREAM_EVENT_TYPES = [
  'system',
  'assistant',
  'user',
  'result',
  'stream_event',
] as const;
export type StreamEventType = (typeof STREAM_EVENT_TYPES)[number];

/**
 * Environment variables Claude Code may inherit. Everything else is dropped so
 * the sandbox cannot pick up host credentials or a caller's settings override.
 */
export const ENV_ALLOWLIST = [
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
] as const;

export type WorkerContractCode =
  | 'unsupported_provider'
  | 'unsupported_permission_mode'
  | 'invalid_max_turns'
  | 'empty_prompt'
  | 'invalid_model'
  | 'argument_injection'
  | 'unapproved_extension'
  | 'unpinned_extension'
  | 'required_extension_unavailable'
  | 'non_canonical_sandbox_path'
  | 'path_escapes_sandbox'
  | 'secret_material_disposed'
  | 'worker_torn_down';

/** Every rejection carries a stable code so the control plane can record it as a denial reason. */
export class WorkerContractError extends Error {
  readonly code: WorkerContractCode;

  constructor(code: WorkerContractCode, detail?: string) {
    super(detail ? `${code}: ${detail}` : code);
    this.name = 'WorkerContractError';
    this.code = code;
  }
}

function deny(code: WorkerContractCode, detail?: string): never {
  throw new WorkerContractError(code, detail);
}

/** A managed extension pinned by the control plane. Mirrors the backend `ExtensionRef`. */
export interface ExtensionRef {
  readonly name: string;
  readonly version: string;
  readonly hash: string;
  readonly required: boolean;
}

export interface SandboxLayout {
  /** Absolute, canonical POSIX root of the disposable sandbox. */
  readonly root: string;
  /** Working copy, relative to `root`. */
  readonly workdir: string;
  /** Read-only sibling clones, relative to `root`. */
  readonly contextDirs?: readonly string[];
  /** MCP server config, relative to `root`. */
  readonly mcpConfig?: string;
}

export interface EphemeralSecret {
  /** Label used in the redaction marker; never the value. */
  readonly name: string;
  readonly value: string;
  /** Environment variable the sandbox receives this secret under, if any. */
  readonly env?: string;
}

export interface ClaudeCodeWorkerRequest {
  readonly provider: string;
  readonly prompt: string;
  readonly model: string;
  readonly permissionMode: PermissionMode;
  readonly maxTurns: number;
  readonly allowedTools: readonly string[];
  readonly sandbox: SandboxLayout;
  /** Pins from the control plane. Worker or repository input must never fill this. */
  readonly approvedExtensions: readonly ExtensionRef[];
  readonly requestedExtensions: readonly ExtensionRef[];
  /** Host process environment, filtered through `ENV_ALLOWLIST`. */
  readonly hostEnv: Readonly<Record<string, string | undefined>>;
}

export interface ClaudeCodeInvocation {
  readonly argv: readonly string[];
  readonly cwd: string;
  readonly env: Readonly<Record<string, string>>;
  /** Never `inherit`: an inherited descriptor would give the run a TTY. */
  readonly stdio: readonly ['pipe', 'pipe', 'pipe'];
  /** The prompt goes on stdin (a harness-sized prompt exceeds the per-argument limit). */
  readonly stdin: { readonly content: string; readonly closeAfterWrite: true };
  readonly tty: false;
}

export type StreamNoiseReason =
  | 'blank'
  | 'oversized_line'
  | 'not_json'
  | 'not_an_object'
  | 'unknown_event_type';

export interface StreamEvent {
  readonly type: StreamEventType;
  readonly [key: string]: unknown;
}

export type StreamLineResult =
  | { readonly kind: 'event'; readonly event: StreamEvent }
  | { readonly kind: 'noise'; readonly reason: StreamNoiseReason; readonly bytes: number };

/**
 * Holds the run's ephemeral credentials (repository token, short-lived keys) and
 * owns their lifetime.
 *
 * JavaScript strings cannot be zeroed, so `dispose()` drops every reference and
 * closes the store instead: after teardown nothing can read the values or emit
 * an event that was not redacted while they were still known.
 */
export interface EphemeralSecretStore {
  readonly disposed: boolean;
  /** Environment entries the sandbox receives. Returns a fresh object the caller owns. */
  env(): Record<string, string>;
  /** Replaces every known secret in `value` (deeply, for JSON data) with a marker. */
  redact<T>(value: T): T;
  dispose(): void;
}

export interface WorkerSession {
  readonly invocation: ClaudeCodeInvocation;
  /** Parses one stdout line and redacts it. Throws once the session is torn down. */
  consume(line: string): StreamLineResult;
  teardown(): void;
}

const MODEL_PATTERN = /^[a-z][a-z0-9.-]*$/;
/** `Bash(git status:*)`-style tool specs. A leading `-` or a separator is refused. */
const ALLOWED_TOOL_PATTERN = /^[A-Za-z][A-Za-z0-9_-]*(\([^(),\n]*\))?$/;
/** Token shapes that must never reach a transcript even when the value is not registered. */
const SECRET_PATTERNS: readonly RegExp[] = [
  /\bsk-ant-[A-Za-z0-9_-]{8,}/g,
  /\bgh[pousr]_[A-Za-z0-9]{8,}/g,
  /\bnm_[A-Za-z0-9_]{8,}/g,
];

/**
 * Validates and joins one sandbox-relative path.
 *
 * The path must already be canonical: this rejects traversal instead of
 * normalising it away, so a caller cannot smuggle `a/../../etc` past a later
 * prefix check. Symlinks are not resolved here — the sandbox is mounted without
 * them, and resolving would require I/O this contract deliberately avoids.
 */
export function canonicalSandboxPath(sandboxRoot: string, relativePath: string): string {
  if (!sandboxRoot.startsWith('/')) {
    deny('non_canonical_sandbox_path', 'sandbox root must be an absolute POSIX path');
  }
  if (sandboxRoot !== '/' && sandboxRoot.endsWith('/')) {
    deny('non_canonical_sandbox_path', 'sandbox root must not end with a separator');
  }
  assertCanonicalSegments(sandboxRoot.slice(1), 'sandbox root');

  if (relativePath === '.') {
    return sandboxRoot;
  }
  if (relativePath.startsWith('/')) {
    deny('path_escapes_sandbox', 'absolute paths are chosen by the control plane, not the run');
  }
  assertCanonicalSegments(relativePath, 'sandbox path');

  return sandboxRoot === '/' ? `/${relativePath}` : `${sandboxRoot}/${relativePath}`;
}

function assertCanonicalSegments(path: string, label: string): void {
  if (path.length === 0) {
    deny('non_canonical_sandbox_path', `${label} must not be empty`);
  }
  if (/[\0\\]/.test(path) || /[ -]/.test(path)) {
    deny('non_canonical_sandbox_path', `${label} contains a control character or a backslash`);
  }
  for (const segment of path.split('/')) {
    if (segment === '..') {
      deny('path_escapes_sandbox', `${label} traverses out of the sandbox`);
    }
    if (segment === '' || segment === '.') {
      deny('non_canonical_sandbox_path', `${label} is not normalised`);
    }
  }
}

/**
 * Checks the run's extensions against the control-plane pins.
 *
 * Mirrors `apps/backend/src/automation/policy.rs`: an extension the pins do not
 * carry is refused, and a required pin that the sandbox cannot provide fails the
 * run instead of silently degrading it.
 */
export function assertPinnedExtensions(
  approved: readonly ExtensionRef[],
  requested: readonly ExtensionRef[],
): void {
  for (const extension of requested) {
    if (!extension.version || !extension.hash) {
      deny('unpinned_extension', extension.name);
    }
    if (!approved.some((pin) => samePin(pin, extension))) {
      deny('unapproved_extension', extension.name);
    }
  }
  for (const pin of approved.filter((pin) => pin.required)) {
    if (!requested.some((extension) => samePin(pin, extension))) {
      deny('required_extension_unavailable', pin.name);
    }
  }
}

function samePin(left: ExtensionRef, right: ExtensionRef): boolean {
  return (
    left.name === right.name &&
    left.version === right.version &&
    left.hash === right.hash &&
    left.required === right.required
  );
}

/**
 * Builds the one invocation this worker is allowed to make.
 *
 * Flags are literals here: the request only contributes values, and each of them
 * is checked for argument injection first. `--setting-sources user` plus
 * `--strict-mcp-config` keep the repository under work from loading its own
 * hooks and MCP servers, which run before the model acts.
 */
export function buildInvocation(
  request: ClaudeCodeWorkerRequest,
  secrets: EphemeralSecretStore,
): ClaudeCodeInvocation {
  if (request.provider !== CLAUDE_CODE_PROVIDER) {
    deny('unsupported_provider', request.provider);
  }
  if (!PERMISSION_MODES.includes(request.permissionMode)) {
    deny('unsupported_permission_mode', String(request.permissionMode));
  }
  if (!Number.isInteger(request.maxTurns) || request.maxTurns < 1 || request.maxTurns > MAX_TURNS_LIMIT) {
    deny('invalid_max_turns', String(request.maxTurns));
  }
  if (request.prompt.trim().length === 0) {
    deny('empty_prompt');
  }
  if (!MODEL_PATTERN.test(request.model)) {
    deny('invalid_model', request.model);
  }
  if (request.allowedTools.length === 0) {
    deny('argument_injection', 'a run must declare the tools it may use');
  }
  for (const tool of request.allowedTools) {
    if (!ALLOWED_TOOL_PATTERN.test(tool)) {
      deny('argument_injection', tool);
    }
  }
  assertPinnedExtensions(request.approvedExtensions, request.requestedExtensions);

  const cwd = canonicalSandboxPath(request.sandbox.root, request.sandbox.workdir);
  const contextDirs = (request.sandbox.contextDirs ?? []).map((dir) =>
    canonicalSandboxPath(request.sandbox.root, dir),
  );

  const argv = [
    'claude',
    '-p',
    '--output-format',
    'stream-json',
    '--verbose',
    '--setting-sources',
    'user',
    '--strict-mcp-config',
    '--permission-mode',
    request.permissionMode,
    '--max-turns',
    String(request.maxTurns),
    '--allowedTools',
    request.allowedTools.join(','),
    '--model',
    request.model,
  ];
  for (const dir of contextDirs) {
    argv.push('--add-dir', dir);
  }
  // `--mcp-config` is variadic, so it stays last: anything appended after it
  // would be read as another config path.
  if (request.sandbox.mcpConfig !== undefined) {
    argv.push('--mcp-config', canonicalSandboxPath(request.sandbox.root, request.sandbox.mcpConfig));
  }

  return {
    argv,
    cwd,
    env: buildEnv(request.hostEnv, secrets),
    stdio: ['pipe', 'pipe', 'pipe'],
    stdin: { content: request.prompt, closeAfterWrite: true },
    tty: false,
  };
}

function buildEnv(
  hostEnv: Readonly<Record<string, string | undefined>>,
  secrets: EphemeralSecretStore,
): Record<string, string> {
  const env: Record<string, string> = {};
  for (const key of ENV_ALLOWLIST) {
    const value = hostEnv[key];
    if (value !== undefined) {
      env[key] = value;
    }
  }
  // No TTY and no interactive prompt: the run answers to the policy it was
  // started with, never to a terminal.
  env.CI = '1';
  env.TERM = 'dumb';
  env.NO_COLOR = '1';
  return { ...env, ...secrets.env() };
}

export function createEphemeralSecrets(
  secrets: readonly EphemeralSecret[],
): EphemeralSecretStore {
  let live: EphemeralSecret[] | null = secrets.map((secret) => ({ ...secret }));

  const alive = (): EphemeralSecret[] => {
    if (live === null) {
      deny('secret_material_disposed');
    }
    return live;
  };

  return {
    get disposed(): boolean {
      return live === null;
    },
    env(): Record<string, string> {
      const env: Record<string, string> = {};
      for (const secret of alive()) {
        if (secret.env !== undefined) {
          env[secret.env] = secret.value;
        }
      }
      return env;
    },
    redact<T>(value: T): T {
      const known = alive();
      return redactValue(value, known) as T;
    },
    dispose(): void {
      live = null;
    },
  };
}

function redactValue(value: unknown, secrets: readonly EphemeralSecret[]): unknown {
  if (typeof value === 'string') {
    return redactString(value, secrets);
  }
  if (Array.isArray(value)) {
    return value.map((item) => redactValue(item, secrets));
  }
  if (value !== null && typeof value === 'object') {
    const redacted: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
      redacted[key] = redactValue(item, secrets);
    }
    return redacted;
  }
  return value;
}

function redactString(value: string, secrets: readonly EphemeralSecret[]): string {
  let redacted = value;
  for (const secret of secrets) {
    if (secret.value.length > 0) {
      redacted = redacted.split(secret.value).join(`[redacted:${secret.name}]`);
    }
  }
  for (const pattern of SECRET_PATTERNS) {
    redacted = redacted.replace(pattern, '[redacted:token]');
  }
  return redacted;
}

/**
 * Parses one stdout line of a Claude Code stream-json run.
 *
 * Nothing is trusted: a line that is not a known event is returned as noise so
 * the caller can count it without storing or acting on it.
 */
export function parseStreamLine(line: string): StreamLineResult {
  const bytes = byteLength(line);
  if (line.trim().length === 0) {
    return { kind: 'noise', reason: 'blank', bytes };
  }
  if (bytes > MAX_STREAM_LINE_BYTES) {
    return { kind: 'noise', reason: 'oversized_line', bytes };
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch {
    return { kind: 'noise', reason: 'not_json', bytes };
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return { kind: 'noise', reason: 'not_an_object', bytes };
  }

  const type = (parsed as Record<string, unknown>).type;
  if (typeof type !== 'string' || !STREAM_EVENT_TYPES.includes(type as StreamEventType)) {
    return { kind: 'noise', reason: 'unknown_event_type', bytes };
  }
  return { kind: 'event', event: parsed as StreamEvent };
}

function byteLength(value: string): number {
  let bytes = 0;
  for (const character of value) {
    const code = character.codePointAt(0) ?? 0;
    bytes += code <= 0x7f ? 1 : code <= 0x7ff ? 2 : code <= 0xffff ? 3 : 4;
  }
  return bytes;
}

/**
 * Creates the session for one disposable run: a fixed invocation, a redacted
 * event stream, and a teardown that ends both.
 */
export function createClaudeCodeWorker(
  request: ClaudeCodeWorkerRequest,
  secrets: EphemeralSecretStore,
): WorkerSession {
  const invocation = buildInvocation(request, secrets);
  let open = true;

  return {
    invocation,
    consume(line: string): StreamLineResult {
      if (!open) {
        deny('worker_torn_down');
      }
      const result = parseStreamLine(line);
      if (result.kind !== 'event') {
        return result;
      }
      return { kind: 'event', event: secrets.redact(result.event) };
    },
    teardown(): void {
      open = false;
      secrets.dispose();
    },
  };
}
