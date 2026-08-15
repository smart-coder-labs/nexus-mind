import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NexusMindClient } from './client'

const fetchMock = vi.fn()

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  vi.stubGlobal('window', { location: { pathname: '/memories', replace: vi.fn() } })
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

  it('does not redirect on a 401 raised while already on /login', async () => {
    vi.stubGlobal('window', { location: { pathname: '/login', replace: vi.fn() } })
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({ error: 'Unauthenticated', code: 'unauthorized' }), { status: 401 }))

    const client = new NexusMindClient('https://api.test')

    // The login page boots with getMe() to detect an existing cookie session.
    // Redirecting here reloads the document into the same 401 — an infinite loop.
    await expect(client.getMe()).rejects.toMatchObject({ status: 401 })
    expect(window.location.replace).not.toHaveBeenCalled()
  })

  it('does not redirect on a 401 raised while on /set-password', async () => {
    vi.stubGlobal('window', { location: { pathname: '/set-password', replace: vi.fn() } })
    fetchMock.mockResolvedValueOnce(new Response(JSON.stringify({ error: 'Unauthenticated', code: 'unauthorized' }), { status: 401 }))

    const client = new NexusMindClient('https://api.test')

    await expect(client.getMe()).rejects.toMatchObject({ status: 401 })
    expect(window.location.replace).not.toHaveBeenCalled()
  })
})
