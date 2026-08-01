import type { ClaudeInvocation, ManagedExtension, ManagedProfile } from './provider'
import { parseClaudeStreamEvent } from './events'
import { createSandboxPaths } from './sandbox'

export { parseClaudeStreamEvent } from './events'

interface BuildInvocationInput {
  sandboxRoot: string
  worktreePath: string
  prompt: string
  profile: ManagedProfile
  requestedArgs?: string[]
  tty?: boolean
}

export function buildClaudeCodeInvocation(input: BuildInvocationInput): ClaudeInvocation {
  if (input.requestedArgs?.length) {
    throw new Error('Worker-supplied Claude arguments are forbidden')
  }

  if (input.tty) {
    throw new Error('Interactive TTY execution is forbidden')
  }

  const sandbox = createSandboxPaths(input.sandboxRoot, input.worktreePath)

  return {
    command: 'claude',
    args: [
      '--print',
      '--output-format',
      'stream-json',
      '--model',
      input.profile.model,
      '--max-turns',
      String(input.profile.maxTurns),
      '--strict-mcp-config',
      '--settings',
      `${sandbox.automation}/settings.json`,
      '--mcp-config',
      `${sandbox.automation}/mcp.json`,
      '--',
      input.prompt,
    ],
    cwd: sandbox.worktree,
    shell: false,
    stdio: ['ignore', 'pipe', 'pipe'],
  }
}

export function validateManagedExtensions(
  approved: ManagedExtension[],
  requested: ManagedExtension[],
): ManagedExtension[] {
  for (const extension of requested) {
    const matchingExtension = approved.find(
      (candidate) =>
        candidate.id === extension.id &&
        candidate.kind === extension.kind &&
        candidate.version === extension.version &&
        candidate.sha256 === extension.sha256,
    )

    if (!matchingExtension || !isPinned(extension)) {
      throw new Error(`Extension ${extension.id} is not pinned and approved`)
    }
  }

  return requested.map((extension) => ({ ...extension }))
}

export function buildStrictSettings(profile: ManagedProfile): string {
  return JSON.stringify({
    permissions: { defaultMode: 'deny' },
    model: profile.model,
    maxTurns: profile.maxTurns,
    maxCostUsd: profile.maxCostUsd,
    extensions: profile.approvedExtensions.map(({ id, version, sha256 }) => ({ id, version, sha256 })),
  })
}

function isPinned(extension: ManagedExtension): boolean {
  return /^\d+\.\d+\.\d+([-.][a-zA-Z0-9.]+)?$/.test(extension.version) && /^[a-f0-9]{64}$/.test(extension.sha256)
}
