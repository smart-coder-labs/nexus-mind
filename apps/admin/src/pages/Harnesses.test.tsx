import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, screen, waitFor, within } from '@testing-library/react'
import { renderWithProviders } from '../test/render'
import Harnesses from './Harnesses'

const listHarnessesMock = vi.fn()
const createHarnessMock = vi.fn()
const publishHarnessVersionMock = vi.fn()
const approveHarnessInstallMock = vi.fn()
const downloadHarnessVersionMock = vi.fn()
const createHarnessConfigReviewMock = vi.fn()

vi.mock('../api/client', () => ({
  createClient: vi.fn(() => ({
    listHarnesses: listHarnessesMock,
    createHarness: createHarnessMock,
    publishHarnessVersion: publishHarnessVersionMock,
    approveHarnessInstall: approveHarnessInstallMock,
    downloadHarnessVersion: downloadHarnessVersionMock,
    createHarnessConfigReview: createHarnessConfigReviewMock,
  })),
}))

const baseHarnesses = [
  {
    id: 'h-1',
    org_id: 'org-test-1',
    slug: 'claude-base',
    name: 'Claude Base',
    description: 'Shared Claude setup',
    visibility: 'org',
    status: 'published',
    created_by: 'user-admin-1',
    created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z',
    latest_version: {
      id: 'hv-1',
      version: '1.0.0',
      manifest_hash: 'sha256:abc',
      targets: ['claude'],
      status: 'published',
      published_at: '2026-07-01T00:00:00Z',
    },
  },
]

beforeEach(() => {
  vi.clearAllMocks()
  listHarnessesMock.mockResolvedValue(baseHarnesses)
  createHarnessMock.mockResolvedValue({ ...baseHarnesses[0], id: 'h-new', slug: 'team-open-code', name: 'Team OpenCode' })
  publishHarnessVersionMock.mockResolvedValue({
    id: 'hv-new',
    harness_id: 'h-1',
    version: '1.1.0',
    manifest: { schema_version: '1.0', targets: ['claude'] },
    manifest_hash: 'sha256:def',
    targets: ['claude'],
    provenance: { source: 'test' },
    status: 'published',
    published_by: 'user-admin-1',
    published_at: '2026-07-02T00:00:00Z',
    revoked_at: null,
  })
  approveHarnessInstallMock.mockResolvedValue({
    id: 'approval-1',
    org_id: 'org-test-1',
    user_id: 'user-admin-1',
    harness_version_id: 'hv-1',
    target_tool: 'claude',
    target_scope: 'project',
    manifest_hash: 'sha256:abc',
    status: 'approved',
    metadata: {},
    approved_at: '2026-07-02T00:00:00Z',
  })
  downloadHarnessVersionMock.mockResolvedValue({
    harness_id: 'h-1',
    version: '1.0.0',
    manifest: { schema_version: '1.0', targets: ['claude'] },
    manifest_hash: 'sha256:abc',
    approval_required: true,
  })
  createHarnessConfigReviewMock.mockResolvedValue({
    id: 'review-1',
    org_id: 'org-test-1',
    user_id: 'user-admin-1',
    source_tool: 'claude',
    redacted_config: { env: { NEXUSMIND_API_KEY: '[REDACTED]' } },
    redaction_report: { secret_count: 1, categories: ['env'] },
    content_hash: 'sha256:redacted',
    status: 'shared',
    created_at: '2026-07-02T00:00:00Z',
    shared_at: '2026-07-02T00:00:00Z',
  })
})

describe('Harnesses page', () => {
  it('lists harnesses and filters by target', async () => {
    renderWithProviders(<Harnesses />)

    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())
    expect(screen.getByText('sha256:abc')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText(/target filter/i), { target: { value: 'claude' } })

    await waitFor(() => expect(listHarnessesMock).toHaveBeenLastCalledWith({ target: 'claude' }))
  })

  it('creates a draft harness and publishes a validated manifest version', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /new harness/i }))
    const createDialog = await screen.findByRole('dialog', { name: /create harness/i })
    fireEvent.change(within(createDialog).getByLabelText(/name/i), { target: { value: 'Team OpenCode' } })
    fireEvent.change(within(createDialog).getByLabelText(/slug/i), { target: { value: 'team-open-code' } })
    fireEvent.click(within(createDialog).getByRole('button', { name: /^create$/i }))

    await waitFor(() => expect(createHarnessMock).toHaveBeenCalledWith(expect.objectContaining({ slug: 'team-open-code', name: 'Team OpenCode' })))

    fireEvent.click(screen.getByRole('button', { name: /publish version for claude base/i }))
    const publishDialog = await screen.findByRole('dialog', { name: /publish harness version/i })
    fireEvent.change(within(publishDialog).getByLabelText(/version/i), { target: { value: '1.1.0' } })
    fireEvent.change(within(publishDialog).getByLabelText(/manifest json/i), {
      target: { value: JSON.stringify({ schema_version: '1.0', targets: ['claude'], provenance: { source: 'test' }, compatibility: {}, components: [], security: { requires_approval: true } }) },
    })
    fireEvent.click(within(publishDialog).getByRole('button', { name: /^publish$/i }))

    await waitFor(() => expect(publishHarnessVersionMock).toHaveBeenCalledWith('h-1', expect.objectContaining({ version: '1.1.0' })))
  })

  it('requires explicit approval before downloading a manifest', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /download claude base/i }))
    const dialog = await screen.findByRole('dialog', { name: /approve harness download/i })
    expect(within(dialog).getByText(/nexusmind will not mutate local files/i)).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: /approve and download/i }))

    await waitFor(() => expect(approveHarnessInstallMock).toHaveBeenCalledWith('h-1', '1.0.0', expect.objectContaining({ manifest_hash: 'sha256:abc' })))
    await waitFor(() => expect(downloadHarnessVersionMock).toHaveBeenCalledWith('h-1', '1.0.0'))
  })

  it('previews a redaction report before submitting a config review snapshot', async () => {
    renderWithProviders(<Harnesses />)
    await waitFor(() => expect(screen.getByText('Claude Base')).toBeInTheDocument())

    fireEvent.change(screen.getByLabelText(/redacted config json/i), {
      target: { value: JSON.stringify({ env: { NEXUSMIND_API_KEY: '[REDACTED]' } }) },
    })
    fireEvent.change(screen.getByLabelText(/redaction report json/i), {
      target: { value: JSON.stringify({ secret_count: 1, categories: ['env'] }) },
    })
    fireEvent.change(screen.getByLabelText(/content hash/i), { target: { value: 'sha256:redacted' } })

    expect(screen.getAllByText(/secret_count/i).length).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('button', { name: /submit config review/i }))

    await waitFor(() => expect(createHarnessConfigReviewMock).toHaveBeenCalledWith(expect.objectContaining({ source_tool: 'claude', content_hash: 'sha256:redacted' })))
  })
})
