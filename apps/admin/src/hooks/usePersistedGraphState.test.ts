import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { usePersistedGraphState } from './usePersistedGraphState'

describe('usePersistedGraphState', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.useRealTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('falls back to defaultValue when nothing is stored', () => {
    const { result } = renderHook(() => usePersistedGraphState<string[]>(`k-${Math.random()}`, ['a', 'b']))
    expect(result.current[0]).toEqual(['a', 'b'])
  })

  it('round-trips a value through localStorage', async () => {
    const key = `rt-${Math.random()}`
    localStorage.setItem(key, JSON.stringify(['x', 'y']))

    const { result } = renderHook(() => usePersistedGraphState<string[]>(key, []))

    expect(result.current[0]).toEqual(['x', 'y'])

    act(() => {
      result.current[1](['x', 'y', 'z'])
    })

    await waitFor(() => {
      expect(JSON.parse(localStorage.getItem(key) ?? 'null')).toEqual(['x', 'y', 'z'])
    })
  })

  it('survives a remount (reload simulation)', async () => {
    const key = `reload-${Math.random()}`

    const first = renderHook(() => usePersistedGraphState<string>(key, 'default'))
    act(() => {
      first.result.current[1]('saved-value')
    })
    await waitFor(() => {
      expect(localStorage.getItem(key)).toBe(JSON.stringify('saved-value'))
    })
    first.unmount()

    const second = renderHook(() => usePersistedGraphState<string>(key, 'default'))
    expect(second.result.current[0]).toBe('saved-value')
  })

  it('returns defaultValue when the stored JSON is corrupt', () => {
    const key = `corrupt-${Math.random()}`
    localStorage.setItem(key, '{not valid json')

    const { result } = renderHook(() => usePersistedGraphState<string[]>(key, ['fallback']))
    expect(result.current[0]).toEqual(['fallback'])
  })

  it('returns defaultValue when the stored JSON fails validate()', () => {
    const key = `validate-${Math.random()}`
    localStorage.setItem(key, JSON.stringify({ not: 'a string array' }))

    const { result } = renderHook(() =>
      usePersistedGraphState<string[]>(key, ['fallback'], {
        validate: v => Array.isArray(v) && v.every(x => typeof x === 'string'),
      }),
    )
    expect(result.current[0]).toEqual(['fallback'])
  })

  it('keeps the stored value when validate() accepts it', async () => {
    const key = `validate-ok-${Math.random()}`
    localStorage.setItem(key, JSON.stringify(['a']))

    const { result } = renderHook(() =>
      usePersistedGraphState<string[]>(key, ['fallback'], {
        validate: v => Array.isArray(v) && v.every(x => typeof x === 'string'),
      }),
    )
    expect(result.current[0]).toEqual(['a'])

    act(() => {
      result.current[1](['a', 'b'])
    })
    await waitFor(() => {
      expect(JSON.parse(localStorage.getItem(key) ?? 'null')).toEqual(['a', 'b'])
    })
  })

  it('busts the cache when the version changes', () => {
    const key = `version-${Math.random()}`
    localStorage.setItem(key, JSON.stringify({ __v: 1, value: 'stale' }))

    const v1 = renderHook(() =>
      usePersistedGraphState<string>(key, 'default', { version: 1 }),
    )
    expect(v1.result.current[0]).toBe('stale')
    v1.unmount()

    const v2 = renderHook(() =>
      usePersistedGraphState<string>(key, 'default', { version: 2 }),
    )
    expect(v2.result.current[0]).toBe('default')
  })

  it('wraps the stored value in { __v } when a version is provided', async () => {
    const key = `wrap-${Math.random()}`
    const { result } = renderHook(() =>
      usePersistedGraphState<string>(key, 'default', { version: 1 }),
    )

    act(() => {
      result.current[1]('hello')
    })
    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem(key) ?? '{}')
      expect(stored).toEqual({ __v: 1, value: 'hello' })
    })
  })

  it('debounces rapid writes into a single localStorage update', async () => {
    vi.useFakeTimers()
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem')
    const key = `debounce-${Math.random()}`

    const { result } = renderHook(() =>
      usePersistedGraphState<number>(key, 0, { debounceMs: 100 }),
    )

    act(() => {
      result.current[1](1)
      result.current[1](2)
      result.current[1](3)
    })

    // Advance past the debounce window
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150)
    })

    const calls = setItemSpy.mock.calls.filter(([k]) => k === key)
    expect(calls).toHaveLength(1)
    expect(JSON.parse(calls[0][1] as string)).toBe(3)
    setItemSpy.mockRestore()
  })

  it('writes synchronously-feel: latest value lands after debounce', async () => {
    const key = `latest-${Math.random()}`
    const { result } = renderHook(() =>
      usePersistedGraphState<string[]>(key, [], { debounceMs: 50 }),
    )

    act(() => {
      result.current[1](['one'])
    })
    await waitFor(() => {
      expect(JSON.parse(localStorage.getItem(key) ?? 'null')).toEqual(['one'])
    })

    act(() => {
      result.current[1](['one', 'two'])
    })
    await waitFor(() => {
      expect(JSON.parse(localStorage.getItem(key) ?? 'null')).toEqual(['one', 'two'])
    })
  })

  it('reset() restores defaultValue and removes the storage entry', async () => {
    const key = `reset-${Math.random()}`
    localStorage.setItem(key, JSON.stringify(['dirty']))

    const { result } = renderHook(() => usePersistedGraphState<string[]>(key, ['clean']))

    expect(result.current[0]).toEqual(['dirty'])

    act(() => {
      result.current[2]() // reset
    })

    expect(result.current[0]).toEqual(['clean'])
    await waitFor(() => {
      expect(localStorage.getItem(key)).toBeNull()
    })
  })

  it('accepts functional updaters like useState', async () => {
    const key = `fn-${Math.random()}`
    const { result } = renderHook(() => usePersistedGraphState<number>(key, 0))

    act(() => {
      result.current[1](prev => prev + 1)
      result.current[1](prev => prev + 1)
    })

    expect(result.current[0]).toBe(2)
    await waitFor(() => {
      expect(JSON.parse(localStorage.getItem(key) ?? 'null')).toBe(2)
    })
  })

  it('swallows localStorage write errors (e.g., quota exceeded)', () => {
    const key = `quota-${Math.random()}`
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceededError')
    })
    const { result } = renderHook(() => usePersistedGraphState<string>(key, 'default'))

    expect(() => {
      act(() => {
        result.current[1]('something')
      })
    }).not.toThrow()
    setItemSpy.mockRestore()
  })

  it('handles missing localStorage gracefully (SSR / private mode)', () => {
    const key = `ssr-${Math.random()}`
    const getItemSpy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('SecurityError')
    })
    const { result } = renderHook(() => usePersistedGraphState<string[]>(key, ['safe']))

    expect(result.current[0]).toEqual(['safe'])
    expect(() => {
      act(() => {
        result.current[1](['safe', 'updated'])
      })
    }).not.toThrow()
    getItemSpy.mockRestore()
  })
})
