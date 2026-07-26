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

export function createSandboxPaths(root: string): SandboxPaths {
  assertCanonicalSandboxRoot(root)

  return {
    root,
    worktree: `${root}/worktree`,
    automation: `${root}/.automation`,
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

export function assertCanonicalSandboxRoot(root: string): void {
  if (!root.startsWith('/') || root.includes('..') || root.endsWith('/')) {
    throw new Error('Sandbox root must be canonical')
  }
}
