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
})
