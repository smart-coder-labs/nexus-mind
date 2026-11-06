import { act, cleanup, fireEvent, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useGraphChrome } from './useGraphChrome'

const key = 'nexusmind-graph-auto-hide-test'
const options = { storageKey: 'test', hasSelection: false, clearSelection: vi.fn(), hoveredNode: false, graphReady: false }

beforeEach(() => {
  vi.useFakeTimers()
  localStorage.clear()
})
afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

describe('useGraphChrome operational controls', () => {
  it('keeps the controls visible by default even after inactivity', () => {
    const { result } = renderHook(() => useGraphChrome(options))
    act(() => { vi.advanceTimersByTime(10_000) })
    expect(result.current.autoHide).toBe(false)
    expect(result.current.focused).toBe(false)
  })

  it('respects opt-in idle hiding but keeps explicit Show UI visible', () => {
    localStorage.setItem(key, 'true')
    const { result } = renderHook(() => useGraphChrome(options))
    act(() => { vi.advanceTimersByTime(3500) })
    expect(result.current.focused).toBe(true)
    act(() => { result.current.toggleFocus() })
    act(() => {
      window.dispatchEvent(new Event('pointermove'))
      vi.advanceTimersByTime(10_000)
    })
    expect(result.current.focused).toBe(false)
    expect(result.current.autoHide).toBe(false)
    expect(localStorage.getItem(key)).toBe('false')
    // Users can explicitly re-enable idle hiding from settings.
    act(() => { result.current.setAutoHide(true) })
    act(() => { vi.advanceTimersByTime(3500) })
    expect(result.current.focused).toBe(true)
  })

  it('shows the UI in one click when the pointer approaches the focus toggle', () => {
    localStorage.setItem(key, 'true')
    const { result } = renderHook(() => useGraphChrome(options))
    const button = document.createElement('button')
    button.setAttribute('data-graph-focus-toggle', '')
    const label = document.createElement('span')
    label.textContent = 'Show UI'
    button.append(label)
    button.addEventListener('click', () => result.current.toggleFocus())
    document.body.append(button)
    try {
      act(() => { vi.advanceTimersByTime(3500) })
      expect(result.current.focused).toBe(true)
      fireEvent.pointerMove(label)
      expect(result.current.focused).toBe(true)
      fireEvent.click(label)
      expect(result.current.focused).toBe(false)
      expect(result.current.autoHide).toBe(false)
      act(() => { vi.advanceTimersByTime(10_000) })
      expect(result.current.focused).toBe(false)
    } finally {
      button.remove()
    }
  })

  it('does not hide settings while the user is configuring the graph', () => {
    localStorage.setItem(key, 'true')
    const { result } = renderHook(() => useGraphChrome(options))
    act(() => { result.current.setSettingsOpen(true) })
    act(() => { vi.advanceTimersByTime(10_000) })
    expect(result.current.focused).toBe(false)
    act(() => { result.current.setSettingsOpen(false) })
    act(() => { vi.advanceTimersByTime(3500) })
    expect(result.current.focused).toBe(true)
  })

  it('keeps manual focus stable on pointer movement and Escape restores the UI', () => {
    const { result } = renderHook(() => useGraphChrome(options))
    act(() => { result.current.toggleFocus() })
    act(() => { window.dispatchEvent(new Event('pointermove')) })
    expect(result.current.focused).toBe(true)
    act(() => { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })) })
    expect(result.current.focused).toBe(false)
  })
})
