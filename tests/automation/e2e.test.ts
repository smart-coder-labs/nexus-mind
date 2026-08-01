import { describe, it, expect } from 'vitest'
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { prepareWorkerAttempt } from '../../workers/src/worker'
import type { ManagedProfile } from '../../workers/src/provider'

describe('Automation Worker E2E Lifecycle', () => {
  it('enforces non-interactive structured execution and secret redaction', () => {
    const sandboxRoot = mkdtempSync(join(tmpdir(), 'sbx-e2e-'))
    const worktreePath = join(sandboxRoot, 'worktree')
    mkdirSync(worktreePath, { recursive: true })

    try {
      const profile: ManagedProfile = {
        id: 'impl-v1',
        profile: 'implementation',
        provider: 'claude-code',
        model: 'claude-sonnet',
        maxTurns: 10,
        maxCostUsd: 5.0,
        approvedExtensions: [],
      }

      const prepared = prepareWorkerAttempt({
        attemptId: 'attempt-e2e-1',
        sandboxRoot,
        worktreePath,
        prompt: 'Implement feature X',
        profile,
        requestedExtensions: [],
        secrets: [{ name: 'API_KEY', value: 'secret_key_12345' }],
      })

      expect(prepared.invocation.command).toBe('claude')
      expect(prepared.invocation.args).toContain('--print')
      expect(prepared.invocation.args).toContain('stream-json')

      const rawEvent = JSON.stringify({
        type: 'assistant',
        session_id: 'sess-e2e-1',
        message: {
          content: [
            { type: 'text', text: 'Using secret_key_12345 to process request' }
          ]
        }
      })

      const redacted = prepared.consumeEvent(rawEvent)
      expect(redacted.text).not.toContain('secret_key_12345')
      expect(redacted.text).toContain('[REDACTED]')

      prepared.teardown()
    } finally {
      rmSync(sandboxRoot, { recursive: true, force: true })
    }
  })
})
