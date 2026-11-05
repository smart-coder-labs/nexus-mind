import { describe, it, expect } from 'vitest'
import { preservedConfig, defaultState, buildConfig } from './AutonomousAgentWizard'
describe('agent configuration preservation', () => {
  it('shares the Workflow publisher by reference without changing reviewer identity', () => {
    const state = defaultState('github_issue_resolver')
    state.repository = 'acme/web'
    state.githubPublicationConnectorId = 'workflow-publisher'
    expect(buildConfig(state)).toMatchObject({ github_publication_connector_id: 'workflow-publisher', github_auth: 'server_gh_cli' })
    expect(preservedConfig(buildConfig(state), 'github_issue_resolver')).not.toHaveProperty('github_publication_connector_id')
    state.template = 'github_pr_reviewer'
    state.extraConfig = '{"github_publication_connector_id":"injected"}'
    expect(buildConfig(state)).not.toHaveProperty('github_publication_connector_id')
  })
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
