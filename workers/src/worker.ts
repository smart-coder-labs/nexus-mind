import {
  buildClaudeCodeInvocation,
  buildStrictSettings,
  parseClaudeStreamEvent,
  validateManagedExtensions,
} from './claude_code'
import type { AttemptEvent } from './events'
import type { ManagedExtension, ManagedProfile } from './provider'
import { injectEphemeralSecrets, type SecretGrant } from './sandbox'

export interface WorkerAttempt {
  attemptId: string
  sandboxRoot: string
  worktreePath: string
  prompt: string
  profile: ManagedProfile
  requestedExtensions: ManagedExtension[]
  secrets: SecretGrant[]
}

export interface PreparedAttempt {
  invocation: ReturnType<typeof buildClaudeCodeInvocation>
  settings: string
  extensions: ManagedExtension[]
  consumeEvent(line: string): AttemptEvent
  teardown(): void
}

export function prepareWorkerAttempt(attempt: WorkerAttempt): PreparedAttempt {
  const secrets = injectEphemeralSecrets(attempt.secrets)
  const extensions = validateManagedExtensions(
    attempt.profile.approvedExtensions,
    attempt.requestedExtensions,
  )
  const invocation = buildClaudeCodeInvocation(attempt)

  return {
    invocation,
    settings: buildStrictSettings(attempt.profile),
    extensions,
    consumeEvent(line) {
      return redactEvent(parseClaudeStreamEvent(line, attempt.attemptId), secrets.values)
    },
    teardown() {
      secrets.destroy()
    },
  }
}

function redactEvent(event: AttemptEvent, secrets: Record<string, string>): AttemptEvent {
  if (!event.text) {
    return event
  }

  return {
    ...event,
    text: Object.values(secrets).reduce(
      (redacted, secret) => redacted.replaceAll(secret, '[REDACTED]'),
      event.text,
    ),
  }
}
