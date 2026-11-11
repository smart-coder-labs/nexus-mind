import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { FindingDetail } from './FindingDetail'
import type { AutonomousAgentDelivery, AutonomousAgentFinding } from '../../../types'

const finding: AutonomousAgentFinding = {
  id: 'finding-1', definition_id: 'agent-1', run_id: 'run-1', fingerprint: 'api-response-mismatch',
  title: '[API] Response omits total', severity: 'low', status: 'open', summary: 'The response is incomplete.',
  evidence: {}, occurrence_count: 1, created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z',
}

describe('FindingDetail repository selector', () => {
  it('offers a related issue in another authorized repository after the first delivery', () => {
    const onCreateIssue=vi.fn()
    const first:AutonomousAgentDelivery={id:'delivery-1',run_id:'run-1',finding_id:'finding-1',channel:'github_issue',status:'delivered',external_id:'7',external_url:'https://github.com/acme/api/issues/7',attempts:1,last_error_code:null,created_at:'2026-10-01T00:00:00Z',updated_at:'2026-10-01T00:00:00Z'}
    render(<FindingDetail finding={finding} deliveries={[first]} templateKey="qa" connectedDestinations={new Set()} can={()=>true} onResolveWithAgent={()=>{}} onCreateIssue={onCreateIssue} creatingIssue={false} issueRepositories={['acme/api','acme/web']} issueRepository="acme/web" onIssueRepositoryChange={()=>{}} onMarkResolved={()=>{}} onArchive={()=>{}} onRestore={()=>{}} onPublish={()=>{}} publishing={false} onRetryDelivery={()=>{}} />)
    fireEvent.click(screen.getByRole('button',{name:'Create related issue'}))
    expect(onCreateIssue).toHaveBeenCalledWith('finding-1','acme/web')
  })

  it('publishes to the selected authorized repository', () => {
    const onCreateIssue = vi.fn()
    const onIssueRepositoryChange = vi.fn()
    const props = { finding, deliveries: [], templateKey: 'qa', connectedDestinations: new Set<string>(),
      can: () => true, onResolveWithAgent: () => {}, onCreateIssue, creatingIssue: false,
      issueRepositories: ['acme/api', 'acme/web'], onIssueRepositoryChange,
      onMarkResolved: () => {}, onArchive: () => {}, onRestore: () => {}, onPublish: () => {},
      publishing: false, onRetryDelivery: () => {} }
    const view=render(<FindingDetail {...props} issueRepository="" />)
    const button = screen.getByRole('button', { name: 'Create issue' })
    expect(button).toBeDisabled()
    fireEvent.change(screen.getByLabelText('Authorized issue repository'), { target: { value: 'acme/api' } })
    expect(onIssueRepositoryChange).toHaveBeenCalledWith('acme/api')
    view.rerender(<FindingDetail {...props} issueRepository="acme/api" />)
    fireEvent.click(screen.getByRole('button', { name: 'Create issue' }))
    expect(onCreateIssue).toHaveBeenCalledWith('finding-1', 'acme/api')
  })
})
