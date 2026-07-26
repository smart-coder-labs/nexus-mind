export type ManagedExtensionKind = 'mcp' | 'plugin' | 'skill' | 'hook'

export interface ManagedExtension {
  id: string
  kind: ManagedExtensionKind
  version: string
  sha256: string
}

export interface ManagedProfile {
  model: string
  approvedExtensions: ManagedExtension[]
  maxTurns: number
  maxCostUsd: number
  timeoutMs: number
}

export interface ClaudeInvocation {
  command: 'claude'
  args: string[]
  cwd: string
  shell: false
  stdio: ['ignore', 'pipe', 'pipe']
}

export interface ExecutionProvider {
  createInvocation(input: {
    sandboxRoot: string
    prompt: string
    profile: ManagedProfile
  }): ClaudeInvocation
}
