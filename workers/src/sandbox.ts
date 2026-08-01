import { realpathSync } from 'node:fs'
import { dirname, relative } from 'node:path'

export interface SandboxPaths {
  root: string
  worktree: string
  automation: string
}

export interface SecretGrant {
  name: string
  value: string
}

export interface SecretEnvironment {
  values: Record<string, string>
  destroy(): void
}

export function createSandboxPaths(sandboxRoot: string, worktreePath: string): SandboxPaths {
  const root = realpathSync(sandboxRoot)
  const worktree = realpathSync(worktreePath)

  assertWorktreeInSandbox(root, worktree)

  return {
    root,
    worktree,
    automation: `${dirname(worktree)}/.automation`,
  }
}

export function injectEphemeralSecrets(grants: SecretGrant[]): SecretEnvironment {
  const values = Object.fromEntries(grants.map(({ name, value }) => [name, value]))

  return {
    values,
    destroy() {
      for (const name of Object.keys(values)) {
        delete values[name]
      }
    },
  }
}

function assertWorktreeInSandbox(sandboxRoot: string, worktree: string): void {
  const pathFromSandbox = relative(sandboxRoot, worktree)

  if (!pathFromSandbox || pathFromSandbox === '..' || pathFromSandbox.startsWith('../')) {
    throw new Error('Worktree must be contained within the sandbox root')
  }
}
