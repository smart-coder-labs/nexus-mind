import { describe, expect, it } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach } from 'vitest'
import { buildConfig, buildTargetConfig, defaultState, StepTarget, stateFromAgent } from './AutonomousAgentWizard'
import type { AutonomousAgentDetail, Project } from '../types'

afterEach(cleanup)
const project: Project = { id: 'project-id', org_id: 'org', name: 'Product', description: null, parent_id: null, created_at: '' }

describe('project agent targets', () => {
  it('lists real projects and stores the chosen id instead of asking for a repository', async () => {
    HTMLElement.prototype.scrollIntoView = () => {}
    const user = userEvent.setup()
    const state = { ...defaultState('github_pr_reviewer'), targetKind: 'project' }
    const changes: unknown[] = []
    render(<StepTarget state={state} set={(key, value) => changes.push([key, value])} projects={[project]} projectsLoading={false} projectsError={false} />)
    expect(screen.queryByPlaceholderText('acme/web')).toBeNull()
    await user.click(screen.getByRole('combobox', { name: /^Project/ }))
    await user.click(await screen.findByRole('option', { name: 'Product' }))
    expect(changes).toContainEqual(['targetProjectId', 'project-id'])
    state.targetProjectId = 'project-id'
    state.targetPrimary = 'acme/stale'
    state.repository = 'acme/stale'
    state.extraConfig = '{"repository":"acme/override"}'
    expect(buildTargetConfig(state)).toEqual({ project_id: 'project-id' })
    expect(buildConfig(state)).toMatchObject({ project_id: 'project-id' })
    expect(buildConfig(state)).not.toHaveProperty('repository')
    state.targetKind = 'repository'
    state.extraConfig = '{"project_id":"stale"}'
    expect(buildConfig(state)).not.toHaveProperty('project_id')
  })

  it('keeps the project on edit, including when its target list is not loaded yet', () => {
    const agent = { template_key: 'github_pr_reviewer', name: 'Reviewer', revision: { config: { project_id: 'project-id' }, budgets: {} } } as unknown as AutonomousAgentDetail
    const state = stateFromAgent(agent)
    expect(state.targetKind).toBe('project')
    expect(state.targetProjectId).toBe('project-id')
    expect(buildConfig(state)).toHaveProperty('project_id', 'project-id')
  })

  it('shows load failures and does not offer archived projects', () => {
    render(<StepTarget state={{ ...defaultState('github_pr_reviewer'), targetKind: 'project' }} set={() => {}} projects={[{ ...project, archived_at: '2026-01-01' }]} projectsLoading={false} projectsError={true} />)
    expect(screen.getByRole('alert')).toHaveTextContent('Could not load projects')
    expect(screen.queryByRole('option', { name: 'Product' })).toBeNull()
  })
})
