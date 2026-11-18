/**
 * Isolated Claude Code worker contract.
 *
 * This module is the only sanctioned way for the automation plane to describe a
 * Claude Code invocation. It is deliberately narrow: the shapes exported here
 * make the following unrepresentable (or explicitly rejected with a stable
 * reason code) rather than merely discouraged.
 *
 *  - arbitrary Claude CLI arguments — the argv vector is fixed; callers only
 *    supply the values of a closed set of flags (see {@link ClaudeInvocation}),
 *    and every interpolated value is validated against an allowlist;
 *  - interactive TTY input — {@link NONINTERACTIVE_STDIO} never inherits a tty
 *    and the prompt is delivered on stdin, never in argv;
 *  - unpinned managed extensions — {@link validateExtensions} accepts only an
 *    exact match against the control plane's pinned set;
 *  - non-canonical sandbox paths — {@link canonicalizeSandboxPath} resolves
 *    symlinks and rejects anything that escapes the sandbox root;
 *  - unredacted ephemeral secrets — {@link sanitizeOutput},
 *    {@link redactJsonSecrets} and {@link createEphemeralSecretBroker} drop a
 *    payload entirely rather than emit it partially redacted, and the broker
 *    fails closed once destroyed.
 *
 * Behaviour mirrors the Rust control plane (`apps/backend/src/automation/`):
 * `profiles.rs`, `policy.rs`, `runtime.rs` and `worker.rs`. Reason codes are
 * kept byte-identical to the Rust ones so operators see one vocabulary.
 *
 * Node built-ins only — this package has no runtime dependencies.
 */

import { lstatSync, realpathSync, type Stats } from 'node:fs';
import { isAbsolute, join, sep } from 'node:path';

/** The only provider this worker speaks to. */
export const CLAUDE_CODE_PROVIDER = 'claude-code';

/** Stable, operator-facing failure codes. Never reword: dashboards match them. */
export type ClaudeReasonCode =
  // invocation validation
  | 'claude_arg_empty'
  | 'claude_arg_flag_injection'
  | 'claude_arg_illegal_character'
  | 'claude_allowed_tools_empty'
  | 'claude_allowed_tool_contains_separator'
  | 'claude_max_turns_invalid'
  | 'claude_permission_mode_invalid'
  | 'claude_path_must_be_absolute'
  // runtime binary resolution (runtime.rs)
  | 'claude_binary_must_be_absolute'
  | 'claude_binary_missing'
  | 'claude_binary_symlink'
  // event stream parsing (worker.rs)
  | 'claude_event_stream_too_large'
  | 'claude_event_stream_limit_exceeded'
  | 'claude_result_event_missing'
  | 'claude_auth_required'
  | 'claude_event_reported_error'
  // sandbox path canonicalization
  | 'sandbox_root_unavailable'
  | 'sandbox_path_empty'
  | 'sandbox_path_absolute'
  | 'sandbox_path_traversal'
  | 'sandbox_path_missing'
  | 'sandbox_path_escape'
  // ephemeral secret custody
  | 'secret_broker_destroyed';

/** Every rejection carries a machine-readable {@link ClaudeReasonCode}. */
export class ClaudeWorkerError extends Error {
  readonly reasonCode: ClaudeReasonCode;

  constructor(reasonCode: ClaudeReasonCode, detail?: string) {
    super(detail === undefined ? reasonCode : `${reasonCode}: ${detail}`);
    this.name = 'ClaudeWorkerError';
    this.reasonCode = reasonCode;
  }
}

// ---------------------------------------------------------------------------
// Invocation contract
// ---------------------------------------------------------------------------

/**
 * Permission modes the isolated plane is allowed to request. `bypassPermissions`
 * is intentionally absent: an isolated run must stay inside its tool allowlist,
 * so the mode that disables the allowlist is unrepresentable here.
 */
export type PermissionMode = 'default' | 'plan' | 'acceptEdits';

const PERMISSION_MODES: readonly PermissionMode[] = ['default', 'plan', 'acceptEdits'];

/** Upper bound on `--max-turns`; a run that needs more is a planning failure. */
export const MAX_TURNS_LIMIT = 200;

/**
 * The complete set of caller-supplied knobs. Everything else in the argv
 * vector is fixed by {@link buildClaudeArgs} and cannot be influenced.
 */
export interface ClaudeInvocation {
  /** Integer in `[1, MAX_TURNS_LIMIT]`. */
  readonly maxTurns: number;
  readonly permissionMode: PermissionMode;
  /** Non-empty; each entry is a single tool name or a `prefix*` wildcard. */
  readonly allowedTools: readonly string[];
  readonly model?: string;
  /** Extra readable directory; must be absolute. */
  readonly addDir?: string;
  /** MCP server configuration file; must be absolute. */
  readonly mcpConfigPath?: string;
}

/** A spawn description. The prompt is handed to stdin, never to argv. */
export interface ClaudeSpawnPlan {
  readonly binaryPath: string;
  readonly args: readonly string[];
  readonly env: Readonly<Record<string, string>>;
  readonly stdio: readonly ['pipe', 'pipe', 'pipe'];
  readonly prompt: string;
}

export interface ClaudePlanRequest {
  readonly binaryPath: string;
  readonly invocation: ClaudeInvocation;
  /** Delivered on stdin: a harness-sized prompt exceeds Linux's argv limit. */
  readonly prompt: string;
  /** Defaults to `process.env`; only the allowlist survives. */
  readonly sourceEnv?: Readonly<Record<string, string | undefined>>;
}

/** No inherited file descriptors, therefore no interactive tty. */
export const NONINTERACTIVE_STDIO: readonly ['pipe', 'pipe', 'pipe'] = Object.freeze([
  'pipe',
  'pipe',
  'pipe',
] as const);

/** Argument values: no shell metacharacters, no control characters, no comma. */
const SAFE_VALUE = /^[A-Za-z0-9_@:./+=-]+$/;
/** Tool names: an identifier, optionally ending in a single `*` wildcard. */
const SAFE_TOOL = /^[A-Za-z0-9_][A-Za-z0-9_.-]*\*?$/;

function assertSafeValue(value: string): string {
  if (value.length === 0) {
    throw new ClaudeWorkerError('claude_arg_empty');
  }
  if (value.startsWith('-')) {
    throw new ClaudeWorkerError('claude_arg_flag_injection', value);
  }
  if (!SAFE_VALUE.test(value)) {
    throw new ClaudeWorkerError('claude_arg_illegal_character', value);
  }
  return value;
}

function assertAbsolutePath(value: string): string {
  if (value.length === 0) {
    throw new ClaudeWorkerError('claude_arg_empty');
  }
  if (value.startsWith('-')) {
    throw new ClaudeWorkerError('claude_arg_flag_injection', value);
  }
  if (!isAbsolute(value)) {
    throw new ClaudeWorkerError('claude_path_must_be_absolute', value);
  }
  if (!SAFE_VALUE.test(value)) {
    throw new ClaudeWorkerError('claude_arg_illegal_character', value);
  }
  return value;
}

function assertMaxTurns(maxTurns: number): number {
  if (!Number.isInteger(maxTurns) || maxTurns < 1 || maxTurns > MAX_TURNS_LIMIT) {
    throw new ClaudeWorkerError('claude_max_turns_invalid', String(maxTurns));
  }
  return maxTurns;
}

function assertPermissionMode(mode: string): PermissionMode {
  const found = PERMISSION_MODES.find((candidate) => candidate === mode);
  if (found === undefined) {
    throw new ClaudeWorkerError('claude_permission_mode_invalid', mode);
  }
  return found;
}

function assertAllowedTools(tools: readonly string[]): string {
  if (tools.length === 0) {
    throw new ClaudeWorkerError('claude_allowed_tools_empty');
  }
  for (const tool of tools) {
    if (tool.length === 0) {
      throw new ClaudeWorkerError('claude_arg_empty');
    }
    if (tool.includes(',')) {
      // A comma would smuggle extra tools past a single-entry review.
      throw new ClaudeWorkerError('claude_allowed_tool_contains_separator', tool);
    }
    if (tool.startsWith('-')) {
      throw new ClaudeWorkerError('claude_arg_flag_injection', tool);
    }
    if (!SAFE_TOOL.test(tool)) {
      throw new ClaudeWorkerError('claude_arg_illegal_character', tool);
    }
  }
  return tools.join(',');
}

/**
 * The fixed argv vector. `--setting-sources user --strict-mcp-config` ignores
 * repository-supplied settings and MCP servers; `-p` takes no inline prompt
 * because the prompt arrives on stdin.
 *
 * @throws {ClaudeWorkerError} with a stable reason code for any rejected value.
 */
export function buildClaudeArgs(invocation: ClaudeInvocation): readonly string[] {
  const maxTurns = assertMaxTurns(invocation.maxTurns);
  const permissionMode = assertPermissionMode(invocation.permissionMode);
  const allowedTools = assertAllowedTools(invocation.allowedTools);
  const args: string[] = [
    '--setting-sources',
    'user',
    '--strict-mcp-config',
    '-p',
    '--output-format',
    'stream-json',
    '--verbose',
    '--max-turns',
    String(maxTurns),
    '--permission-mode',
    permissionMode,
    '--allowedTools',
    allowedTools,
  ];
  if (invocation.model !== undefined) {
    args.push('--model', assertSafeValue(invocation.model));
  }
  if (invocation.addDir !== undefined) {
    args.push('--add-dir', assertAbsolutePath(invocation.addDir));
  }
  if (invocation.mcpConfigPath !== undefined) {
    args.push('--mcp-config', assertAbsolutePath(invocation.mcpConfigPath));
  }
  return Object.freeze(args);
}

/** Environment variables Claude Code may see. Nothing else is propagated. */
export const ALLOWED_ENV_KEYS: readonly string[] = Object.freeze([
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
]);

/**
 * Builds the child environment from scratch (the equivalent of `env_clear()`
 * followed by an explicit allowlist). Ephemeral secrets live in the broker and
 * must never be handed to this function: anything outside the allowlist is
 * dropped, so a leaked variable cannot reach the agent through the environment.
 */
export function restrictedClaudeEnv(
  sourceEnv: Readonly<Record<string, string | undefined>> = process.env,
): Readonly<Record<string, string>> {
  const env: Record<string, string> = {};
  for (const key of ALLOWED_ENV_KEYS) {
    const value = sourceEnv[key];
    if (typeof value === 'string') {
      env[key] = value;
    }
  }
  env.DISABLE_AUTOUPDATER = '1';
  env.NEXUSMIND_MCP_TOOL_PROFILE = 'essential';
  return Object.freeze(env);
}

/**
 * The operator-managed binary must be an absolute path to a real file and not
 * a symlink: a swapped symlink is how an attacker substitutes the agent.
 */
export function resolveClaudeBinary(binaryPath: string): string {
  if (!isAbsolute(binaryPath)) {
    throw new ClaudeWorkerError('claude_binary_must_be_absolute', binaryPath);
  }
  const stats: Stats = ((): Stats => {
    try {
      return lstatSync(binaryPath);
    } catch {
      throw new ClaudeWorkerError('claude_binary_missing', binaryPath);
    }
  })();
  if (stats.isSymbolicLink()) {
    throw new ClaudeWorkerError('claude_binary_symlink', binaryPath);
  }
  if (!stats.isFile()) {
    throw new ClaudeWorkerError('claude_binary_missing', binaryPath);
  }
  return binaryPath;
}

/** Full spawn plan: validated argv, restricted env, piped stdio, stdin prompt. */
export function planClaudeInvocation(request: ClaudePlanRequest): ClaudeSpawnPlan {
  return Object.freeze({
    binaryPath: resolveClaudeBinary(request.binaryPath),
    args: buildClaudeArgs(request.invocation),
    env: restrictedClaudeEnv(request.sourceEnv),
    stdio: NONINTERACTIVE_STDIO,
    prompt: request.prompt,
  });
}

// ---------------------------------------------------------------------------
// Pinned managed extensions (mirrors policy.rs)
// ---------------------------------------------------------------------------

/** A managed extension, pinned by version *and* content hash. */
export interface ExtensionRef {
  readonly name: string;
  readonly version: string;
  readonly hash: string;
  readonly required: boolean;
}

export type ExtensionDenyReason = 'unapproved_extension' | 'required_extension_unavailable';

export type ExtensionDecision =
  | { readonly status: 'allowed' }
  | { readonly status: 'denied'; readonly reason: ExtensionDenyReason };

function sameExtension(left: ExtensionRef, right: ExtensionRef): boolean {
  return (
    left.name === right.name &&
    left.version === right.version &&
    left.hash === right.hash &&
    left.required === right.required
  );
}

/**
 * Exact structural equality against the control plane's pinned set — version or
 * hash drift is a different extension, not an upgrade. Every `required`
 * extension must also be present, so a run cannot silently skip one.
 *
 * `pinned` must come from a trusted control-plane snapshot; repository or
 * worker input must never fill it.
 */
export function validateExtensions(
  requested: readonly ExtensionRef[],
  pinned: readonly ExtensionRef[],
): ExtensionDecision {
  for (const extension of requested) {
    if (!pinned.some((approved) => sameExtension(approved, extension))) {
      return { status: 'denied', reason: 'unapproved_extension' };
    }
  }
  for (const extension of pinned.filter((candidate) => candidate.required)) {
    if (!requested.some((provided) => sameExtension(provided, extension))) {
      return { status: 'denied', reason: 'required_extension_unavailable' };
    }
  }
  return { status: 'allowed' };
}

// ---------------------------------------------------------------------------
// Output redaction
// ---------------------------------------------------------------------------

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

const REDACTED = '[REDACTED]';

/** Dropped-payload markers, byte-identical to the Rust transcript writer. */
export const SECRET_REDACTION_COLLISION_TEXT = '[content omitted: secret redaction collision]';
export const SECRET_REDACTION_COLLISION_EVENT =
  '{"type":"raw","text":"[transcript event omitted: secret redaction collision]"}';
export const MALFORMED_STREAM_EVENT = '{"type":"raw","text":"[malformed stream event omitted]"}';

const SECRET_PATTERNS: readonly RegExp[] = Object.freeze([
  /gh[pousr]_[A-Za-z0-9_]{20,}/g,
  /github_pat_[A-Za-z0-9_]{20,}/g,
  /https:\/\/hooks\.slack\.com\/services\/\S+/g,
  /xox[baprs]-[A-Za-z0-9-]{10,}/g,
  /AKIA[0-9A-Z]{16}/g,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/g,
  /(token|secret|password|api[_-]?key)\s*[=:]\s*\S+/gi,
]);

/**
 * Truncate first, then redact: redacting a buffer we are about to cut wastes
 * work and can leave a half-written secret at the boundary. `secrets` holds the
 * exact ephemeral values (repo tokens and the like), replaced on top of the
 * pattern matches.
 */
export function sanitizeOutput(
  value: Uint8Array | string,
  limit: number,
  secrets: readonly string[] = [],
): string {
  const bytes = typeof value === 'string' ? Buffer.from(value, 'utf8') : Buffer.from(value);
  const bounded = bytes.subarray(0, Math.max(0, Math.min(limit, bytes.length)));
  let text = bounded.toString('utf8');
  for (const pattern of SECRET_PATTERNS) {
    text = text.replace(pattern, REDACTED);
  }
  for (const secret of secrets) {
    if (secret.length > 0) {
      text = text.split(secret).join(REDACTED);
    }
  }
  return text;
}

const COLLISION = Symbol('secret_redaction_collision');

export type RedactionResult =
  | { readonly collision: false; readonly value: JsonValue }
  | { readonly collision: true; readonly value: string };

function redactValue(value: JsonValue, secrets: readonly string[]): JsonValue | typeof COLLISION {
  if (typeof value === 'string') {
    let cleaned = sanitizeOutput(value, Buffer.byteLength(value, 'utf8'), secrets);
    const nested = parseJsonContainer(cleaned);
    if (nested !== undefined) {
      const redacted = redactValue(nested, secrets);
      if (redacted === COLLISION) {
        return COLLISION;
      }
      cleaned = JSON.stringify(redacted);
    }
    return cleaned;
  }
  if (Array.isArray(value)) {
    const items: JsonValue[] = [];
    for (const item of value) {
      const redacted = redactValue(item, secrets);
      if (redacted === COLLISION) {
        return COLLISION;
      }
      items.push(redacted);
    }
    return items;
  }
  if (value !== null && typeof value === 'object') {
    const sanitized: { [key: string]: JsonValue } = {};
    for (const [key, item] of Object.entries(value)) {
      let cleanKey = key;
      for (const secret of secrets) {
        if (secret.length > 0) {
          cleanKey = cleanKey.split(secret).join(REDACTED);
        }
      }
      const redacted = redactValue(item, secrets);
      // Two distinct keys collapsing onto one redacted key would silently drop
      // a sibling value — treat it as a collision instead.
      if (redacted === COLLISION || Object.hasOwn(sanitized, cleanKey)) {
        return COLLISION;
      }
      sanitized[cleanKey] = redacted;
    }
    return sanitized;
  }
  return value;
}

function asJsonObject(value: JsonValue | undefined): { [key: string]: JsonValue } | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value : undefined;
}

/** A JSON string that itself holds an object/array (tool results do this a lot). */
function parseJsonContainer(text: string): JsonValue | undefined {
  const trimmed = text.trim();
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) {
    return undefined;
  }
  try {
    const parsed: unknown = JSON.parse(trimmed);
    return typeof parsed === 'object' && parsed !== null ? (parsed as JsonValue) : undefined;
  } catch {
    return undefined;
  }
}

/**
 * Redacts a JSON payload in place of emitting it raw, recursing through nested
 * JSON-in-string values and object keys. A *collision* — any case where
 * redaction would make two values indistinguishable — drops the whole payload:
 * a partially redacted transcript event is worse than no event.
 */
export function redactJsonSecrets(value: JsonValue, secrets: readonly string[]): RedactionResult {
  const redacted = redactValue(value, secrets);
  if (redacted === COLLISION) {
    return { collision: true, value: SECRET_REDACTION_COLLISION_TEXT };
  }
  return { collision: false, value: redacted };
}

/**
 * Custody for the values that must never reach Claude's environment or argv
 * (ephemeral repository tokens and similar). JavaScript strings are immutable,
 * so `destroy()` cannot scrub the bytes in place; it drops every reference and
 * latches the broker closed. `values()` and `redact()` then throw rather than
 * returning text that would silently travel unredacted.
 */
export interface EphemeralSecretBroker {
  values(): readonly string[];
  redact(text: string): string;
  destroy(): void;
}

export function createEphemeralSecretBroker(
  secrets: readonly string[],
): EphemeralSecretBroker {
  let held: string[] | null = secrets.filter((secret) => secret.length > 0);
  const live = (): string[] => {
    if (held === null) {
      throw new ClaudeWorkerError('secret_broker_destroyed');
    }
    return held;
  };
  return {
    values: () => [...live()],
    redact: (text: string) => sanitizeOutput(text, Buffer.byteLength(text, 'utf8'), live()),
    destroy: () => {
      if (held !== null) {
        held.fill('');
        held.length = 0;
        held = null;
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Event stream parsing (mirrors worker.rs::parse_claude_event_stream)
// ---------------------------------------------------------------------------

/** Browser-driven QA produces large streams, so the caps are generous. */
export const MAX_STREAM_BYTES = 32 * 1_048_576;
export const MAX_LINE_BYTES = 4 * 1_048_576;
export const MAX_NOISE_LINES = 100;
export const MAX_STREAM_LINES = 100_000;

export interface ClaudeStreamSummary {
  readonly format: 'stream-json';
  readonly events: Readonly<Record<string, number>>;
  readonly line_count: number;
  readonly noise_lines: number;
}

export interface ClaudeStreamParse {
  readonly result: { readonly [key: string]: JsonValue };
  readonly summary: ClaudeStreamSummary;
}

function isAuthFailure(result: { [key: string]: JsonValue }): boolean {
  const error = typeof result.error === 'string' ? result.error.toLowerCase() : '';
  const message =
    typeof result.result === 'string' ? result.result.slice(0, 1200).toLowerCase() : '';
  return (
    result.api_error_status === 401 ||
    error === 'authentication_failed' ||
    message.includes('failed to authenticate') ||
    message.includes('access token has been revoked')
  );
}

/**
 * Parses `--output-format stream-json` output into the terminal `result` event
 * plus a counts summary. Bounded on every axis so a runaway run cannot exhaust
 * worker memory. Lines that are not JSON, or that lack a non-empty string
 * `type`, are *noise*: tolerated up to {@link MAX_NOISE_LINES} rather than
 * failing the run, because the CLI also writes plain diagnostics to stdout.
 *
 * @throws {ClaudeWorkerError} `claude_event_stream_too_large`,
 * `claude_event_stream_limit_exceeded`, `claude_result_event_missing`,
 * `claude_auth_required` or `claude_event_reported_error`.
 */
export function parseClaudeEventStream(
  bytes: Uint8Array | string,
  secrets: readonly string[] = [],
): ClaudeStreamParse {
  const byteLength =
    typeof bytes === 'string' ? Buffer.byteLength(bytes, 'utf8') : bytes.byteLength;
  if (byteLength > MAX_STREAM_BYTES) {
    throw new ClaudeWorkerError('claude_event_stream_too_large');
  }
  const sanitized = sanitizeOutput(bytes, MAX_STREAM_BYTES, secrets);
  const events: Record<string, number> = {};
  let result: { [key: string]: JsonValue } | undefined;
  let lineCount = 0;
  let noiseLines = 0;
  for (const line of sanitized.split(/\r?\n/)) {
    if (line.trim().length === 0) {
      continue;
    }
    lineCount += 1;
    if (lineCount > MAX_STREAM_LINES || line.length > MAX_LINE_BYTES) {
      throw new ClaudeWorkerError('claude_event_stream_limit_exceeded');
    }
    const event = asJsonObject(parseJsonContainer(line));
    const kind = event?.type;
    if (event === undefined || typeof kind !== 'string' || kind.length === 0) {
      noiseLines += 1;
      if (noiseLines > MAX_NOISE_LINES) {
        throw new ClaudeWorkerError('claude_event_stream_limit_exceeded');
      }
      continue;
    }
    events[kind] = (events[kind] ?? 0) + 1;
    if (kind === 'result') {
      result = event;
    }
  }
  if (result === undefined) {
    throw new ClaudeWorkerError('claude_result_event_missing');
  }
  if (result.is_error === true) {
    throw new ClaudeWorkerError(
      isAuthFailure(result) ? 'claude_auth_required' : 'claude_event_reported_error',
    );
  }
  return {
    result,
    summary: {
      format: 'stream-json',
      events: Object.freeze(events),
      line_count: lineCount,
      noise_lines: noiseLines,
    },
  };
}

// ---------------------------------------------------------------------------
// Sandbox paths
// ---------------------------------------------------------------------------

/**
 * Resolves `candidate` inside the sandbox `root` and proves it stayed there.
 *
 * Absolute inputs and `..` segments are rejected outright; what is left is
 * joined onto the canonical root and canonicalized again, so a symlink planted
 * in the workspace cannot make the worker read (or ship) a file outside it.
 *
 * @returns the canonical absolute path.
 * @throws {ClaudeWorkerError} `sandbox_root_unavailable`, `sandbox_path_empty`,
 * `sandbox_path_absolute`, `sandbox_path_traversal`, `sandbox_path_missing` or
 * `sandbox_path_escape`.
 */
export function canonicalizeSandboxPath(root: string, candidate: string): string {
  const canonicalRoot = ((): string => {
    try {
      return realpathSync(root);
    } catch {
      throw new ClaudeWorkerError('sandbox_root_unavailable', root);
    }
  })();
  if (candidate.length === 0) {
    throw new ClaudeWorkerError('sandbox_path_empty');
  }
  if (isAbsolute(candidate)) {
    throw new ClaudeWorkerError('sandbox_path_absolute', candidate);
  }
  if (candidate.split(/[/\\]+/).some((segment) => segment === '..')) {
    throw new ClaudeWorkerError('sandbox_path_traversal', candidate);
  }
  const resolved = ((): string => {
    try {
      return realpathSync(join(canonicalRoot, candidate));
    } catch {
      throw new ClaudeWorkerError('sandbox_path_missing', candidate);
    }
  })();
  if (resolved !== canonicalRoot && !resolved.startsWith(canonicalRoot + sep)) {
    throw new ClaudeWorkerError('sandbox_path_escape', candidate);
  }
  return resolved;
}
