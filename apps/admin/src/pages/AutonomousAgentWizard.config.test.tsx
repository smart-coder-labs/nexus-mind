import { describe, it, expect } from 'vitest'
import { preservedConfig } from './AutonomousAgentWizard'
describe('agent configuration preservation', () => {
  it('keeps advanced verification and provider settings while rebuilding owned fields', () => {
    const config = {
      repository: 'acme/web',
      model: 'sonnet',
      executor: 'claude',
      model_source: 'agent',
      verification_commands: [{ kind: 'build', command: ['cargo', 'build'] }],
      verification_isolation: 'sandbox',
      model_tier: 'cheap',
      sandbox_allowed_hosts: ['registry.npmjs.org'],
      plugin_options: { mode: 'reviewed' },
    }
    expect(preservedConfig(config, 'github_issue_resolver')).toEqual({
      verification_commands: config.verification_commands,
      verification_isolation: 'sandbox',
      model_tier: 'cheap',
      sandbox_allowed_hosts: ['registry.npmjs.org'],
      plugin_options: { mode: 'reviewed' },
    })
  })
  it('does not retain stale values for fields the current form owns', () => {
    expect(
      preservedConfig(
        {
          outputs: ['slack'],
          qa_instructions: 'old',
          sandbox_allowed_hosts: ['example.com'],
          mcp_profile_ref: 'approved-ref',
        },
        'qa',
      ),
    ).toEqual({ mcp_profile_ref: 'approved-ref' })
  })
})
