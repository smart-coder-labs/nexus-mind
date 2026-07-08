import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NexusMindClient } from './client'

const fetchMock = vi.fn()

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  vi.stubGlobal('window', { location: { replace: vi.fn() } })
})

describe('NexusMindClient harness contracts', () => {
  it('lists harnesses with target filtering and maps the array response', async () => {
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify([
      {
        id: 'h-1',
        org_id: 'org-1',
        slug: 'claude-base',
        name: 'Claude Base',
        visibility: 'org',
        status: 'published',
        created_by: 'user-1',
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
    ]), { status: 200 }))

    const client = new NexusMindClient('https://api.test')
    const rows = await client.listHarnesses({ target: 'claude' })

    expect(fetchMock).toHaveBeenCalledWith(
      'https://api.test/v1/harnesses?target=claude',
      expect.objectContaining({ credentials: 'include' }),
    )
    expect(rows[0].latest_version?.manifest_hash).toBe('sha256:abc')
  })

  it('publishes, approves, records install result, and downloads exact harness versions without local mutation fields', async () => {
    fetchMock
      .mockResolvedValueOnce(new Response(JSON.stringify({
        id: 'hv-1',
        harness_id: 'h-1',
        version: '1.0.0',
        manifest: { schema_version: '1.0', targets: ['claude'] },
        manifest_hash: 'sha256:abc',
        targets: ['claude'],
        provenance: { source: 'test' },
        status: 'published',
        published_by: 'user-1',
        published_at: '2026-07-01T00:00:00Z',
        revoked_at: null,
      }), { status: 201 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        id: 'approval-1',
        org_id: 'org-1',
        user_id: 'user-1',
        harness_version_id: 'hv-1',
        target_tool: 'claude',
        target_scope: 'project',
        manifest_hash: 'sha256:abc',
        status: 'approved',
        metadata: { reason: 'admin-test' },
        approved_at: '2026-07-01T00:00:00Z',
      }), { status: 201 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        id: 'approval-1',
        org_id: 'org-1',
        user_id: 'user-1',
        harness_version_id: 'hv-1',
        target_tool: 'claude',
        target_scope: 'project',
        manifest_hash: 'sha256:abc',
        status: 'approved',
        metadata: { install_result: { status: 'installed', changed_files_count: 2 } },
        approved_at: '2026-07-01T00:00:00Z',
      }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        harness_id: 'h-1',
        version: '1.0.0',
        manifest: { schema_version: '1.0', targets: ['claude'] },
        manifest_hash: 'sha256:abc',
        approval_required: true,
      }), { status: 200 }))

    const client = new NexusMindClient('')
    const published = await client.publishHarnessVersion('h-1', {
      version: '1.0.0',
      manifest: { schema_version: '1.0', targets: ['claude'] },
    })
    const approval = await client.approveHarnessInstall('h-1', '1.0.0', {
      target_tool: 'claude',
      target_scope: 'project',
      manifest_hash: published.manifest_hash,
      metadata: { reason: 'admin-test' },
    })
    const result = await client.recordHarnessInstallResult('h-1', '1.0.0', {
      approval_id: approval.id,
      manifest_hash: published.manifest_hash,
      status: 'installed',
      metadata: { changed_files_count: 2 },
    })
    const download = await client.downloadHarnessVersion('h-1', '1.0.0')

    expect(approval.manifest_hash).toBe('sha256:abc')
    expect(result.metadata.install_result).toEqual({ status: 'installed', changed_files_count: 2 })
    expect(download.approval_required).toBe(true)
    expect(download).not.toHaveProperty('apply_local_changes')
    expect(download).not.toHaveProperty('install')
  })

  it('stores config review snapshots with redacted content and redaction reports', async () => {
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({
      id: 'review-1',
      org_id: 'org-1',
      user_id: 'user-1',
      source_tool: 'claude',
      redacted_config: { mcpServers: { nexusmind: { env: { NEXUSMIND_API_KEY: '[REDACTED]' } } } },
      redaction_report: { secret_count: 1, categories: ['env'] },
      content_hash: 'sha256:redacted',
      status: 'shared',
      created_at: '2026-07-01T00:00:00Z',
      shared_at: '2026-07-01T00:00:00Z',
    }), { status: 201 }))

    const client = new NexusMindClient('')
    const review = await client.createHarnessConfigReview({
      source_tool: 'claude',
      redacted_config: { mcpServers: { nexusmind: { env: { NEXUSMIND_API_KEY: '[REDACTED]' } } } },
      redaction_report: { secret_count: 1, categories: ['env'] },
      content_hash: 'sha256:redacted',
      status: 'shared',
    })

    expect(fetchMock).toHaveBeenCalledWith('/v1/harness-config-reviews', expect.objectContaining({ method: 'POST' }))
    expect(JSON.stringify(review.redacted_config)).toContain('[REDACTED]')
    expect(JSON.stringify(review.redacted_config)).not.toContain('nm_secret')
  })
})
