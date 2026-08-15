import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Migrations from './Migrations'
import type { MigrationCandidate, MigrationRun } from '../types'

const mockListRuns = vi.fn()
const mockListCandidates = vi.fn()
const mockReview = vi.fn()
const mockCommit = vi.fn()

vi.mock('../api/client', () => ({
  createClient: () => ({
    listMigrationRuns: mockListRuns,
    listMigrationCandidates: mockListCandidates,
    reviewMigrationCandidates: mockReview,
    commitMigrationRun: mockCommit,
  }),
}))

const run: MigrationRun = {
  id: 'run1',
  org_id: 'org1',
  client_id: 'cl_a',
  project_id: null,
  source_kind: 'repo-docs',
  status: 'staging',
  source_ref: './',
  runner_version: '2.1.233',
  attestation: {},
  created_by: 'u1',
  created_at: '2026-08-15T00:00:00Z',
  updated_at: '2026-08-15T00:00:00Z',
}

function candidate(over: Partial<MigrationCandidate>): MigrationCandidate {
  return {
    id: 'c1',
    run_id: 'run1',
    source_identity: 'repo-docs:docs/a.md:abc',
    destination_kind: 'memory',
    destination_hint: {},
    content: 'proposed content',
    source_excerpt: 'the verbatim line from the source',
    confidence: 0.5,
    normalized_metadata: {},
    attestation: {},
    provenance_kind: 'verified_manifest',
    status: 'staged',
    version: 1,
    indexed_at: null,
    created_at: '2026-08-15T00:00:00Z',
    updated_at: '2026-08-15T00:00:00Z',
    ...over,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  mockListRuns.mockResolvedValue([run])
  mockReview.mockResolvedValue({ applied: 1, conflicts: 0, results: [] })
  mockCommit.mockResolvedValue({
    committed: 1,
    skipped: 0,
    failed: 0,
    indexed: 0,
    pending_index: 1,
    results: [],
  })
})

async function openRun() {
  renderWithProviders(<Migrations />)
  const runButton = await screen.findByRole('button', { name: /repo-docs/ })
  fireEvent.click(runButton)
}

describe('Migrations review queue', () => {
  it('orders the queue by confidence, highest first', async () => {
    mockListCandidates.mockResolvedValue([
      candidate({ id: 'low', source_identity: 'src:low', confidence: 0.2 }),
      candidate({ id: 'high', source_identity: 'src:high', confidence: 0.95 }),
      candidate({ id: 'none', source_identity: 'src:none', confidence: null }),
    ])
    await openRun()

    const boxes = await screen.findAllByRole('checkbox')
    const labels = boxes.map((b) => b.getAttribute('aria-label'))
    expect(labels).toEqual([
      'Select candidate src:high',
      'Select candidate src:low',
      'Select candidate src:none',
    ])
  })

  it('blocks batch approval when a client-attested candidate is selected', async () => {
    mockListCandidates.mockResolvedValue([
      candidate({ id: 'a', source_identity: 'src:a', provenance_kind: 'verified_manifest' }),
      candidate({ id: 'b', source_identity: 'src:b', provenance_kind: 'client_attested' }),
    ])
    await openRun()

    fireEvent.click(await screen.findByLabelText('Select candidate src:a'))
    fireEvent.click(await screen.findByLabelText('Select candidate src:b'))

    expect(await screen.findByText(/must be approved one at a time/)).toBeInTheDocument()
    const approve = screen.getByRole('button', { name: /^Approve/ })
    expect(approve).toBeDisabled()
    expect(mockReview).not.toHaveBeenCalled()
  })

  it('allows approving a single client-attested candidate', async () => {
    mockListCandidates.mockResolvedValue([
      candidate({ id: 'b', source_identity: 'src:b', provenance_kind: 'client_attested' }),
    ])
    await openRun()
    fireEvent.click(await screen.findByLabelText('Select candidate src:b'))

    const approve = screen.getByRole('button', { name: /^Approve/ })
    expect(approve).not.toBeDisabled()
    fireEvent.click(approve)
    await waitFor(() => expect(mockReview).toHaveBeenCalledTimes(1))
  })

  it('shows the verbatim source excerpt so the reviewer need not open the file', async () => {
    mockListCandidates.mockResolvedValue([candidate({})])
    await openRun()
    fireEvent.click(await screen.findByRole('button', { name: 'Inspect' }))

    expect(await screen.findByText('the verbatim line from the source')).toBeInTheDocument()
    expect(screen.getByText('proposed content')).toBeInTheDocument()
  })

  it('sends the version the reviewer actually read', async () => {
    mockListCandidates.mockResolvedValue([candidate({ id: 'c1', version: 7 })])
    await openRun()
    fireEvent.click(await screen.findByLabelText('Select candidate repo-docs:docs/a.md:abc'))
    fireEvent.click(screen.getByRole('button', { name: /^Approve/ }))

    await waitFor(() =>
      expect(mockReview).toHaveBeenCalledWith('run1', [
        { candidate_id: 'c1', action: 'approved', expected_version: 7 },
      ]),
    )
  })

  it('tells the reviewer to look again when a version conflict comes back', async () => {
    mockListCandidates.mockResolvedValue([candidate({})])
    mockReview.mockResolvedValue({
      applied: 0,
      conflicts: 1,
      results: [{ candidate_id: 'c1', outcome: 'stale_version', actual_version: 2 }],
    })
    await openRun()
    fireEvent.click(await screen.findByLabelText('Select candidate repo-docs:docs/a.md:abc'))
    fireEvent.click(screen.getByRole('button', { name: /^Approve/ }))

    expect(await screen.findByRole('alert')).toHaveTextContent(/look again before deciding/)
  })

  /// The two gates are different questions with different owners. Without this
  /// copy a reviewer reasonably assumes they are authorizing execution.
  it('distinguishes the migration gate from the install gate for a harness', async () => {
    mockListCandidates.mockResolvedValue([
      candidate({ id: 'h', source_identity: 'src:h', destination_kind: 'harness' }),
    ])
    await openRun()
    fireEvent.click(await screen.findByLabelText('Select candidate src:h'))

    const note = await screen.findByRole('note')
    expect(note).toHaveTextContent(/becomes a tool of the team/)
    expect(note).toHaveTextContent(/does not install it anywhere/i)
    expect(note).toHaveTextContent(/approves the install separately/)
  })

  it('does not show the install-gate notice for a plain memory', async () => {
    mockListCandidates.mockResolvedValue([candidate({})])
    await openRun()
    fireEvent.click(await screen.findByLabelText('Select candidate repo-docs:docs/a.md:abc'))
    expect(screen.queryByRole('note')).not.toBeInTheDocument()
  })

  it('reports the indexing backlog after a commit instead of claiming everything is searchable', async () => {
    mockListCandidates.mockResolvedValue([candidate({ status: 'approved' })])
    await openRun()
    fireEvent.click(await screen.findByRole('button', { name: /^Commit/ }))

    expect(
      await screen.findByText(/stored but not yet searchable by similarity/),
    ).toBeInTheDocument()
  })

  it('says plainly that nothing has entered the brain yet', async () => {
    mockListCandidates.mockResolvedValue([])
    renderWithProviders(<Migrations />)
    expect(
      await screen.findByText(/Nothing here has entered the company brain yet/),
    ).toBeInTheDocument()
  })
})
