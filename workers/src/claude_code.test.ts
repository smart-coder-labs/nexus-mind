import { describe, expect, it } from 'vitest'

import {
  CLAUDE_CODE_PROVIDER,
  MAX_STREAM_LINE_BYTES,
  REDACTION_PLACEHOLDER,
  WorkerContractError,
  canonicalSandboxPath,
  ephemeralSecretMaterial,
  parseStreamEvent,
  parseStreamEvents,
  prepareRun,
  redactEvent,
  validateManagedExtensions,
  type ManagedExtension,
} from './claude_code'

const APPROVED: ManagedExtension[] = [
  {
    name: 'approved-mcp',
    version: '1.0.0',
    hash: `sha256:${'a'.repeat(64)}`,
    required: true,
  },
]

const SANDBOX_ROOT = '/srv/nexusmind/sandbox/run-7'

const job = (overrides: Record<string, unknown> = {}) => ({
  jobId: 'run-7-wp-1',
  profile: 'implementation',
  provider: CLAUDE_CODE_PROVIDER,
  model: 'claude-sonnet-4-5',
  prompt: 'Implement the bounded change described in the manifest.',
  sandboxRoot: SANDBOX_ROOT,
  worktree: 'checkout',
  extensions: APPROVED,
  ...overrides,
})

const codeOf = (run: () => unknown): string => {
  try {
    run()
  } catch (error) {
    if (error instanceof WorkerContractError) return error.code
    throw error
  }
  throw new Error('expected a WorkerContractError')
}

describe('isolated Claude Code worker', () => {
  it('invokes the CLI noninteractively and refuses any caller-supplied argument', () => {
    const secrets = ephemeralSecretMaterial({ ANTHROPIC_API_KEY: 'sk-ant-live', GH_TOKEN: 'ghs-live' })
    const { invocation, profile } = prepareRun(job(), {
      approvedExtensions: APPROVED,
      secrets,
      baseEnv: { PATH: '/usr/bin', HOME: '/home/runner', AWS_SECRET_ACCESS_KEY: 'leaked' },
    })

    expect(profile).toBe('implementation')
    expect(invocation.command).toBe('claude')
    expect(invocation.argv).toEqual([
      '-p',
      '--output-format',
      'stream-json',
      '--verbose',
      '--model',
      'claude-sonnet-4-5',
      '--permission-mode',
      'acceptEdits',
      '--settings',
      `${SANDBOX_ROOT}/.nexusmind/settings.json`,
      '--strict-mcp-config',
      '--mcp-config',
      `${SANDBOX_ROOT}/.nexusmind/mcp.json`,
    ])

    // The prompt travels on stdin, and every descriptor is a pipe: there is no
    // TTY for the CLI to ask a question on.
    expect(invocation.argv).not.toContain(invocation.stdin)
    expect(invocation.stdin).toBe(job().prompt)
    expect(invocation.stdio).toEqual(['pipe', 'pipe', 'pipe'])
    expect(invocation.env.TERM).toBe('dumb')
    expect(invocation.env.CI).toBe('1')

    // Only the allowlisted host variables and the ephemeral secrets survive.
    expect(Object.keys(invocation.env).sort()).toEqual([
      'ANTHROPIC_API_KEY',
      'CI',
      'GH_TOKEN',
      'HOME',
      'NO_COLOR',
      'PATH',
      'TERM',
    ])

    // A spec field the worker does not know is a rejection, not a silent drop,
    // so nothing can grow the argument list later.
    expect(codeOf(() => prepareRun(job({ claudeArgs: ['--dangerously-skip-permissions'] }), { approvedExtensions: APPROVED, secrets }))).toBe('unknown_job_field')
    expect(codeOf(() => prepareRun(job({ extraArgs: '--permission-mode=bypassPermissions' }), { approvedExtensions: APPROVED, secrets }))).toBe('unknown_job_field')
    // A model that could be read as a flag never reaches argv.
    expect(codeOf(() => prepareRun(job({ model: '--dangerously-skip-permissions' }), { approvedExtensions: APPROVED, secrets }))).toBe('invalid_job_field')
    expect(codeOf(() => prepareRun(job({ profile: 'root' }), { approvedExtensions: APPROVED, secrets }))).toBe('unknown_profile')
    expect(codeOf(() => prepareRun(job({ provider: 'codex' }), { approvedExtensions: APPROVED, secrets }))).toBe('unsupported_provider')
  })

  it('loads managed extensions only when pinned, approved and complete', () => {
    expect(validateManagedExtensions(APPROVED, APPROVED)).toEqual(APPROVED)

    const floating = { ...APPROVED[0], version: '^1.0.0' }
    expect(codeOf(() => validateManagedExtensions(APPROVED, [floating]))).toBe('unpinned_extension')

    const unpinnedDigest = { ...APPROVED[0], hash: 'extension-sha256' }
    expect(codeOf(() => validateManagedExtensions(APPROVED, [unpinnedDigest]))).toBe('unpinned_extension')
    // A placeholder digest on the approved side fails closed too.
    expect(codeOf(() => validateManagedExtensions([unpinnedDigest], [unpinnedDigest]))).toBe('unpinned_extension')

    const other = { ...APPROVED[0], name: 'side-channel-mcp' }
    expect(codeOf(() => validateManagedExtensions(APPROVED, [other]))).toBe('unapproved_extension')

    const bumped = { ...APPROVED[0], version: '1.0.1' }
    expect(codeOf(() => validateManagedExtensions(APPROVED, [bumped]))).toBe('unapproved_extension')

    expect(codeOf(() => validateManagedExtensions(APPROVED, []))).toBe('required_extension_unavailable')
  })

  it('keeps every sandbox path canonical and inside the root', () => {
    expect(canonicalSandboxPath(SANDBOX_ROOT, 'checkout/apps/backend')).toBe(
      `${SANDBOX_ROOT}/checkout/apps/backend`,
    )
    expect(canonicalSandboxPath(SANDBOX_ROOT, '.')).toBe(SANDBOX_ROOT)

    expect(codeOf(() => canonicalSandboxPath(SANDBOX_ROOT, '../run-8/checkout'))).toBe('sandbox_path_escapes_root')
    expect(codeOf(() => canonicalSandboxPath(SANDBOX_ROOT, 'checkout/../../run-8'))).toBe('sandbox_path_not_canonical')
    expect(codeOf(() => canonicalSandboxPath(SANDBOX_ROOT, 'checkout//apps'))).toBe('sandbox_path_not_canonical')
    expect(codeOf(() => canonicalSandboxPath(SANDBOX_ROOT, '/etc/passwd'))).toBe('sandbox_path_escapes_root')
    expect(codeOf(() => canonicalSandboxPath(SANDBOX_ROOT, 'checkout\0/x'))).toBe('sandbox_path_not_canonical')

    // A root that is not already canonical is a control-plane bug, so the
    // worker reports it instead of quietly canonicalising it.
    expect(codeOf(() => canonicalSandboxPath('srv/sandbox/run-7', 'checkout'))).toBe('sandbox_root_not_canonical')
    expect(codeOf(() => canonicalSandboxPath(`${SANDBOX_ROOT}/`, 'checkout'))).toBe('sandbox_root_not_canonical')
    expect(codeOf(() => canonicalSandboxPath('/srv/nexusmind/../nexusmind/sandbox', 'checkout'))).toBe('sandbox_root_not_canonical')
    // A sibling root sharing a textual prefix must not pass the containment check.
    expect(codeOf(() => canonicalSandboxPath('/srv/nexusmind/sandbox/run-7', '../run-70'))).toBe('sandbox_path_escapes_root')
  })

  it('parses stream events strictly, redacts them, then tears the secrets down', () => {
    const secrets = ephemeralSecretMaterial({ GH_TOKEN: 'ghs-live-token' })

    const events = parseStreamEvents(
      '{"type":"system","subtype":"init","session_id":"s-1"}\n\n' +
        '{"type":"assistant","message":{"content":[{"type":"text","text":"pushing with ghs-live-token"}]}}\n' +
        '{"type":"result","subtype":"success","is_error":false,"result":"done"}\n',
    )
    expect(events.map((event) => event.type)).toEqual(['system', 'assistant', 'result'])

    expect(codeOf(() => parseStreamEvent('not json'))).toBe('unparsable_stream_event')
    expect(codeOf(() => parseStreamEvent('[{"type":"system"}]'))).toBe('unparsable_stream_event')
    expect(codeOf(() => parseStreamEvent('{"type":"tool_use_confirm"}'))).toBe('unknown_stream_event')
    expect(codeOf(() => parseStreamEvent('{"type":"result","subtype":"success"}'))).toBe('unparsable_stream_event')
    expect(codeOf(() => parseStreamEvent('{"type":"system","__proto__":{"isAdmin":true}}'))).toBe('unparsable_stream_event')
    expect(codeOf(() => parseStreamEvent(`{"type":"system","pad":"${'x'.repeat(MAX_STREAM_LINE_BYTES)}"}`))).toBe('unparsable_stream_event')

    const redacted = redactEvent({ events, 'ghs-live-token': 'as-a-key' }, secrets.values())
    expect(JSON.stringify(redacted)).not.toContain('ghs-live-token')
    expect(JSON.stringify(redacted)).toContain(REDACTION_PLACEHOLDER)

    secrets.teardown()
    expect(secrets.disposed).toBe(true)
    // Reading disposed material throws rather than handing back an empty set,
    // so a later redaction cannot silently become a no-op.
    expect(codeOf(() => secrets.values())).toBe('secret_material_disposed')
    expect(codeOf(() => secrets.env())).toBe('secret_material_disposed')
    expect(codeOf(() => ephemeralSecretMaterial({ AWS_SECRET_ACCESS_KEY: 'x' }))).toBe('unapproved_secret_name')
  })
})
