import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NexusMindClient } from './client'

const fetchMock = vi.fn()

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  vi.stubGlobal('window', { location: { replace: vi.fn() } })
})

describe('NexusMindClient request() empty-body handling', () => {
  it('resolves without throwing when a relationship POST returns 201 with an empty body (linkTaskSpec)', async () => {
    fetchMock.mockResolvedValueOnce(new Response('', { status: 201 }))

    const client = new NexusMindClient('https://api.test')

    await expect(client.linkTaskSpec('task-1', 'change-1')).resolves.toBeUndefined()
  })

  it('resolves without throwing when addTaskLabel returns 201 with an empty body', async () => {
    fetchMock.mockResolvedValueOnce(new Response('', { status: 201 }))

    const client = new NexusMindClient('https://api.test')

    await expect(client.addTaskLabel('task-1', 'urgent')).resolves.toBeUndefined()
  })

  it('resolves without throwing when assignTask returns 201 with an empty body', async () => {
    fetchMock.mockResolvedValueOnce(new Response('', { status: 201 }))

    const client = new NexusMindClient('https://api.test')

    await expect(client.assignTask('task-1', ['user-1'])).resolves.toBeUndefined()
  })

  it('still parses a normal JSON 200 response', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ id: 'task-1', title: 'Do the thing' }), { status: 200 }),
    )

    const client = new NexusMindClient('https://api.test')
    const task = await client.getTask('task-1')

    expect(task).toEqual({ id: 'task-1', title: 'Do the thing' })
  })

  it('reads bounded Context Fabric diagnostics without exposing cache content', async () => {
    const diagnostics = {
      cache: { enabled: false, entries: 0, hits: 0, misses: 0, puts: 0, invalidations: 0, expirations: 0, invalidation_events: 0, reason_codes: [] },
      rollout: { shadow_enabled: false, canary_enabled: false, promotion_enabled: false, baseline_fallback: true, active_lane: 'baseline' },
      active_profile: null,
      active_generation: null,
      reason_codes: ['baseline_fallback_required'],
    }
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify(diagnostics), { status: 200 }))

    const client = new NexusMindClient('https://api.test')

    await expect(client.getContextFabricDiagnostics()).resolves.toEqual(diagnostics)
    expect(fetchMock).toHaveBeenCalledWith(
      'https://api.test/v1/context/diagnostics',
      expect.objectContaining({ credentials: 'include' }),
    )
  })

  it('does not redirect for a feature-level 403', async () => {
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({ error: 'Forbidden', code: 'forbidden' }), { status: 403 }))

    const client = new NexusMindClient('https://api.test')

    await expect(client.getAuditLog()).rejects.toMatchObject({ status: 403 })
    expect(window.location.replace).not.toHaveBeenCalled()
  })

  it('redirects to login when the session is unauthenticated', async () => {
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({ error: 'Unauthenticated', code: 'unauthorized' }), { status: 401 }))

    const client = new NexusMindClient('https://api.test')

    await expect(client.getStats()).rejects.toMatchObject({ status: 401 })
    expect(window.location.replace).toHaveBeenCalledWith('/login')
  })
})
