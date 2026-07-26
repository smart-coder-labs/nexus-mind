import { describe, expect, it } from 'vitest'

import {
  buildClaudeCodeInvocation,
  parseClaudeStreamEvent,
  validateManagedExtensions,
} from './claude_code'
import { prepareWorkerAttempt } from './worker'

const sandboxRoot = '/sandbox/attempt-42'

const profile = {
  model: 'claude-sonnet-4-20250514',
  approvedExtensions: [
    {
      id: 'approved-mcp',
      kind: 'mcp' as const,
      version: '1.2.3',
      sha256: 'a'.repeat(64),
    },
  ],
  maxTurns: 12,
  maxCostUsd: 5,
  timeoutMs: 60_000,
}

describe('Claude Code managed invocation', () => {
  it('uses a fixed non-interactive argv and rejects injected arguments, TTY, and sandbox escapes', () => {
    const invocation = buildClaudeCodeInvocation({
      sandboxRoot,
      prompt: 'Inspect the requested change.',
      profile,
    })

    expect(invocation).toEqual({
      command: 'claude',
      args: [
        '--print',
        '--output-format',
        'stream-json',
        '--model',
        'claude-sonnet-4-20250514',
        '--max-turns',
        '12',
        '--strict-mcp-config',
        '--settings',
        '/sandbox/attempt-42/.automation/settings.json',
        '--mcp-config',
        '/sandbox/attempt-42/.automation/mcp.json',
        'Inspect the requested change.',
      ],
      cwd: '/sandbox/attempt-42/worktree',
      shell: false,
      stdio: ['ignore', 'pipe', 'pipe'],
    })

    expect(() =>
      buildClaudeCodeInvocation({
        sandboxRoot,
        prompt: 'Inspect',
        profile,
        requestedArgs: ['--dangerously-skip-permissions'],
      }),
    ).toThrow('Worker-supplied Claude arguments are forbidden')

    expect(() =>
      buildClaudeCodeInvocation({
        sandboxRoot,
        prompt: 'Inspect',
        profile,
        tty: true,
      }),
    ).toThrow('Interactive TTY execution is forbidden')

    expect(() =>
      buildClaudeCodeInvocation({
        sandboxRoot: '/sandbox/attempt-42/../host',
        prompt: 'Inspect',
        profile,
      }),
    ).toThrow('Sandbox root must be canonical')
  })

  it('accepts only fully pinned managed extensions', () => {
    expect(
      validateManagedExtensions(profile.approvedExtensions, profile.approvedExtensions),
    ).toEqual(profile.approvedExtensions)

    expect(() =>
      validateManagedExtensions(profile.approvedExtensions, [
        { id: 'repo-plugin', kind: 'plugin', version: 'latest', sha256: '' },
      ]),
    ).toThrow('Extension repo-plugin is not pinned and approved')
  })

  it('normalizes valid stream events and rejects malformed or unknown events', () => {
    expect(
      parseClaudeStreamEvent(
        JSON.stringify({
          type: 'assistant',
          session_id: 'attempt-42',
          message: { content: [{ type: 'text', text: 'Working.' }] },
        }),
      ),
    ).toEqual({
      kind: 'assistant',
      attemptId: 'attempt-42',
      text: 'Working.',
    })

    expect(() => parseClaudeStreamEvent('{invalid json')).toThrow('Malformed Claude stream event')
    expect(() =>
      parseClaudeStreamEvent(JSON.stringify({ type: 'tool_use', session_id: 'attempt-42' })),
    ).toThrow('Unsupported Claude stream event type: tool_use')
  })

  it('creates strict managed settings, redacts ephemeral secrets, and tears them down', () => {
    const attempt = prepareWorkerAttempt({
      sandboxRoot,
      prompt: 'Inspect the requested change.',
      profile,
      requestedExtensions: profile.approvedExtensions,
      secrets: [{ name: 'GITHUB_TOKEN', value: 'ephemeral-token' }],
    })

    expect(JSON.parse(attempt.settings)).toEqual({
      permissions: { defaultMode: 'deny' },
      model: 'claude-sonnet-4-20250514',
      maxTurns: 12,
      maxCostUsd: 5,
      extensions: [
        {
          id: 'approved-mcp',
          version: '1.2.3',
          sha256: 'a'.repeat(64),
        },
      ],
    })
    expect(
      attempt.consumeEvent(
        JSON.stringify({
          type: 'assistant',
          session_id: 'attempt-42',
          message: { content: [{ type: 'text', text: 'token=ephemeral-token' }] },
        }),
      ),
    ).toEqual({ kind: 'assistant', attemptId: 'attempt-42', text: 'token=[REDACTED]' })

    attempt.teardown()
    expect(attempt.consumeEvent(JSON.stringify({ type: 'result', session_id: 'attempt-42' }))).toEqual({
      kind: 'result',
      attemptId: 'attempt-42',
      text: undefined,
    })
  })
})
