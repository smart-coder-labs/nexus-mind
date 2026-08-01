import { afterAll, describe, expect, it } from 'vitest'
import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import {
  buildClaudeCodeInvocation,
  parseClaudeStreamEvent,
  validateManagedExtensions,
} from './claude_code'
import { prepareWorkerAttempt } from './worker'

const sandboxRoot = mkdtempSync(join(tmpdir(), 'nexus-mind-sandbox-'))
const attemptRoot = join(sandboxRoot, 'attempt-42')
const worktreePath = join(attemptRoot, 'worktree')

mkdirSync(worktreePath, { recursive: true })
mkdirSync(join(attemptRoot, '.automation'))

afterAll(() => {
  rmSync(sandboxRoot, { recursive: true, force: true })
})

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
  it('uses a fixed non-interactive argv with an argument boundary and rejects injected arguments and TTY', () => {
    const invocation = buildClaudeCodeInvocation({
      sandboxRoot,
      worktreePath,
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
        `${realpathSync(attemptRoot)}/.automation/settings.json`,
        '--mcp-config',
        `${realpathSync(attemptRoot)}/.automation/mcp.json`,
        '--',
        'Inspect the requested change.',
      ],
      cwd: realpathSync(worktreePath),
      shell: false,
      stdio: ['ignore', 'pipe', 'pipe'],
    })

    expect(() =>
      buildClaudeCodeInvocation({
        sandboxRoot,
        worktreePath,
        prompt: 'Inspect',
        profile,
        requestedArgs: ['--dangerously-skip-permissions'],
      }),
    ).toThrow('Worker-supplied Claude arguments are forbidden')

    expect(() =>
      buildClaudeCodeInvocation({
        sandboxRoot,
        worktreePath,
        prompt: 'Inspect',
        profile,
        tty: true,
      }),
    ).toThrow('Interactive TTY execution is forbidden')

    expect(
      buildClaudeCodeInvocation({
        sandboxRoot,
        worktreePath,
        prompt: '--dangerously-skip-permissions',
        profile,
      }).args.slice(-2),
    ).toEqual(['--', '--dangerously-skip-permissions'])
  })

  it('accepts only a resolved worktree within the configured sandbox root', () => {
    const outsideRoot = mkdtempSync(join(tmpdir(), 'nexus-mind-outside-'))
    const outsideWorktree = join(outsideRoot, 'worktree')
    mkdirSync(outsideWorktree)
    const escapedWorktree = join(attemptRoot, 'escaped-worktree')
    symlinkSync(outsideWorktree, escapedWorktree)

    expect(() =>
      buildClaudeCodeInvocation({
        sandboxRoot,
        worktreePath: outsideWorktree,
        prompt: 'Inspect',
        profile,
      }),
    ).toThrow('Worktree must be contained within the sandbox root')

    expect(() =>
      buildClaudeCodeInvocation({
        sandboxRoot,
        worktreePath: escapedWorktree,
        prompt: 'Inspect',
        profile,
      }),
    ).toThrow('Worktree must be contained within the sandbox root')

    rmSync(outsideRoot, { recursive: true, force: true })
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

  it('normalizes valid stream events using the prepared attempt identity and rejects malformed or unknown events', () => {
    expect(
      parseClaudeStreamEvent(
        JSON.stringify({
          type: 'assistant',
          session_id: 'attempt-42',
          message: { content: [{ type: 'text', text: 'Working.' }] },
        }),
        'expected-attempt-42',
      ),
    ).toEqual({
      kind: 'assistant',
      attemptId: 'expected-attempt-42',
      sessionId: 'attempt-42',
      text: 'Working.',
    })

    expect(() => parseClaudeStreamEvent('{invalid json', 'expected-attempt-42')).toThrow('Malformed Claude stream event')
    expect(() =>
      parseClaudeStreamEvent(JSON.stringify({ type: 'tool_use', session_id: 'attempt-42' }), 'expected-attempt-42'),
    ).toThrow('Unsupported Claude stream event type: tool_use')
  })

  it('creates strict managed settings, redacts ephemeral secrets, and tears them down', () => {
    const attempt = prepareWorkerAttempt({
      sandboxRoot,
      worktreePath,
      attemptId: 'expected-attempt-42',
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
    ).toEqual({
      kind: 'assistant',
      attemptId: 'expected-attempt-42',
      sessionId: 'attempt-42',
      text: 'token=[REDACTED]',
    })

    attempt.teardown()
    expect(attempt.consumeEvent(JSON.stringify({ type: 'result', session_id: 'attempt-42' }))).toEqual({
      kind: 'result',
      attemptId: 'expected-attempt-42',
      sessionId: 'attempt-42',
      text: undefined,
    })
  })
})
