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

describe('FindingDetail evidence', () => {
  const base = { deliveries: [], connectedDestinations: new Set<string>(), can: () => true, onResolveWithAgent: () => {},
    onCreateIssue: () => {}, creatingIssue: false, onMarkResolved: () => {}, onArchive: () => {}, onRestore: () => {},
    onPublish: () => {}, publishing: false, onRetryDelivery: () => {} }

  it('shows a Product Strategist proposal in full with its screenshot', () => {
    const strategist: AutonomousAgentFinding = { ...finding, evidence: {
      problem: 'Owners cannot see **why** a run failed.', proposal: 'Add a failure reason to the run list.',
      acceptance_criteria: ['The run list shows the reason', 'The reason links to the transcript'],
      type: 'improvement', module: 'factory/runs', repository: 'acme/admin', screenshot_url: 'https://cdn.example/shot.png',
    } }
    render(<FindingDetail {...base} finding={strategist} templateKey="product_strategist" />)
    expect(screen.getByText('Proposal · Improvement')).toBeInTheDocument()
    expect(screen.queryByText(/Bug/)).toBeNull()
    expect(screen.getByText('Problem')).toBeInTheDocument()
    expect(screen.getByText('why').tagName).toBe('STRONG')
    expect(screen.getByText('Add a failure reason to the run list.')).toBeInTheDocument()
    expect(screen.getByText('The run list shows the reason')).toBeInTheDocument()
    expect(screen.getByText('The reason links to the transcript')).toBeInTheDocument()
    expect(screen.getByText('acme/admin')).toBeInTheDocument()
    expect(screen.getByText('factory/runs')).toBeInTheDocument()
    expect(screen.getByRole('img', { name: `Screenshot for ${finding.title}` })).toHaveAttribute('src', 'https://cdn.example/shot.png')
  })

  it('still shows a QA finding as a bug with steps and no proposal', () => {
    const qa: AutonomousAgentFinding = { ...finding, evidence: { kind: 'functional', steps: ['Open the page', 'Click save'] } }
    render(<FindingDetail {...base} finding={qa} templateKey="qa" />)
    expect(screen.getByText(/^Bug · /)).toBeInTheDocument()
    expect(screen.getByText('Steps to reproduce')).toBeInTheDocument()
    expect(screen.getByText('Click save')).toBeInTheDocument()
    expect(screen.queryByText('Acceptance criteria')).toBeNull()
  })
})
